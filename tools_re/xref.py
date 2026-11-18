#!/usr/bin/env python3
"""Find every place a 32-bit ARM binary reaches a given symbol.

    python xref.py libgame.so DM_DrawScreen565 memcpy
    python xref.py libgame.so --addr 0x47c2c
    python xref.py libgame.so SomeFunc --words     # also scan for its address

Two kinds of reference, and you need both:

  direct    a `bl` / `b` / `blx` whose immediate target is the symbol (or its
            PLT stub, for an imported one). Reported with the *owning* function
            name, which is the thing you actually want to read next.

  word      the symbol's address stored as a 32-bit word somewhere in the
            image — a literal pool, a GOT slot, a jump table, a vtable. Turn
            this on with `--words`.

The second one is not an optional extra. A function reached only through a
pointer selected by pixel format, by state, or by a vtable has **zero** direct
call sites, and a search that only looks for `bl` will report it as dead code
while it is burning 99% of your frame. If `--words` finds the address in `.got`
and direct calls are zero, the entry points are `blx <reg>` / `ldr pc, [..]`
sites you must find by reading the callers instead.

Needs `pip install pyelftools capstone`.
"""

from __future__ import annotations

import argparse
import bisect
import sys
from pathlib import Path

try:
    from elftools.elf.elffile import ELFFile
except ImportError:  # pragma: no cover
    sys.exit("pip install pyelftools")
try:
    from capstone import Cs, CS_ARCH_ARM, CS_MODE_ARM, CS_MODE_THUMB
except ImportError:  # pragma: no cover
    sys.exit("pip install capstone")

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")


class Image:
    def __init__(self, path: Path):
        self.raw = path.read_bytes()
        self.fh = path.open("rb")
        self.elf = ELFFile(self.fh)
        self.segs = [(s["p_vaddr"], s["p_offset"], s["p_filesz"], s["p_memsz"])
                     for s in self.elf.iter_segments() if s["p_type"] == "PT_LOAD"]

        # symbol table: address -> (name, size); Thumb functions have bit 0 set
        # in st_value, which is how you tell the two instruction sets apart
        # without guessing.
        self.syms: dict[int, tuple[str, int, bool]] = {}
        self.by_name: dict[str, int] = {}
        for secname in (".symtab", ".dynsym"):
            sec = self.elf.get_section_by_name(secname)
            if sec is None:
                continue
            for s in sec.iter_symbols():
                if s["st_shndx"] == "SHN_UNDEF" or not s.name:
                    continue
                if s["st_info"]["type"] not in ("STT_FUNC", "STT_OBJECT"):
                    continue
                v = s["st_value"]
                self.syms.setdefault(v & ~1, (s.name, s["st_size"], bool(v & 1)))
                self.by_name.setdefault(s.name, v)
        self.addrs = sorted(self.syms)

        # PLT stub -> imported name. ARM PLT: 20-byte header then 12 bytes per
        # entry, in .rel.plt order.
        self.plt: dict[int, str] = {}
        rel = self.elf.get_section_by_name(".rel.plt")
        dynsym = self.elf.get_section_by_name(".dynsym")
        plt = self.elf.get_section_by_name(".plt")
        if rel is not None and dynsym is not None and plt is not None:
            base = plt["sh_addr"]
            for i, r in enumerate(rel.iter_relocations()):
                nm = dynsym.get_symbol(r["r_info_sym"]).name
                self.plt[base + 20 + 12 * i] = nm

    def owner(self, addr: int) -> str:
        i = bisect.bisect_right(self.addrs, addr) - 1
        if i < 0:
            return f"{addr:#x}"
        a = self.addrs[i]
        name, size, _ = self.syms[a]
        if size and addr - a >= size and addr - a > 0x4000:
            return f"{addr:#x}"
        return name if addr == a else f"{name}+{addr - a:#x}"

    def resolve(self, spec: str) -> list[tuple[str, int]]:
        """A name, a PLT-backed import name, or 0x... — all to addresses."""
        if spec.lower().startswith("0x"):
            return [(spec, int(spec, 16))]
        out = []
        if spec in self.by_name:
            out.append((spec, self.by_name[spec] & ~1))
        for a, nm in self.plt.items():
            if nm == spec:
                out.append((f"{spec}@plt", a))
        return out


def scan_text(img: Image, targets: dict[int, str]):
    """Disassemble every executable section and collect branches into targets."""
    hits: dict[str, list[tuple[int, str, str]]] = {n: [] for n in targets.values()}
    for sec in img.elf.iter_sections():
        if not (sec["sh_flags"] & 0x4):  # SHF_EXECINSTR
            continue
        data = sec.data()
        base = sec["sh_addr"]
        # Disassemble as ARM. Thumb-only images are handled by the fallback
        # below; a mixed image is disassembled twice and duplicate hits are
        # deduplicated by address.
        for mode in (CS_MODE_ARM, CS_MODE_THUMB):
            md = Cs(CS_ARCH_ARM, mode)
            step = 4 if mode == CS_MODE_ARM else 2
            off = 0
            while off < len(data):
                n = 0
                for ins in md.disasm(data[off:], base + off):
                    n += 1
                    off = ins.address - base + ins.size
                    if not ins.mnemonic.startswith("b"):
                        continue
                    if not ins.op_str.startswith("#"):
                        continue
                    try:
                        tgt = int(ins.op_str[1:], 0)
                    except ValueError:
                        continue
                    name = targets.get(tgt) or targets.get(tgt & ~1)
                    if name:
                        hits[name].append((ins.address, img.owner(ins.address),
                                           f"{ins.mnemonic} {ins.op_str}"))
                if n == 0:
                    off += step
            if mode == CS_MODE_ARM and any(hits.values()):
                break  # ARM decoded cleanly; do not double-count as Thumb
    for k in hits:
        seen, uniq = set(), []
        for a, o, t in sorted(hits[k]):
            if a not in seen:
                seen.add(a)
                uniq.append((a, o, t))
        hits[k] = uniq
    return hits


def scan_words(img: Image, targets: dict[int, str]):
    """Every 4-byte-aligned word in the loaded image equal to a target address.

    A literal pool entry, a GOT slot, a vtable entry and a jump table all look
    the same here, and that is the point: it tells you the function is reachable
    at all when no branch names it."""
    want = {}
    for a, n in targets.items():
        want[a] = n
        want[a | 1] = n  # Thumb entry as stored in a pointer
    out: dict[str, list[tuple[int, str]]] = {n: [] for n in targets.values()}
    for vaddr, off, filesz, _ in img.segs:
        blob = img.raw[off:off + filesz]
        for i in range(0, len(blob) - 3, 4):
            w = int.from_bytes(blob[i:i + 4], "little")
            n = want.get(w)
            if n:
                out[n].append((vaddr + i, img.owner(vaddr + i)))
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("so")
    ap.add_argument("names", nargs="*", help="symbol names, or 0x addresses")
    ap.add_argument("--addr", action="append", default=[],
                    help="an address to look for, as 0x...")
    ap.add_argument("--words", action="store_true",
                    help="also scan the image for the address as a 32-bit word")
    ap.add_argument("--limit", type=int, default=40, help="sites to print each")
    a = ap.parse_args()

    img = Image(Path(a.so))
    targets: dict[int, str] = {}
    for spec in list(a.names) + list(a.addr):
        found = img.resolve(spec)
        if not found:
            print(f"!! {spec}: no such symbol and not an address")
            continue
        for label, addr in found:
            targets[addr] = label
    if not targets:
        return 1

    direct = scan_text(img, targets)
    words = scan_words(img, targets) if a.words else {}

    for addr, label in sorted(targets.items(), key=lambda kv: kv[1]):
        sites = direct.get(label, [])
        print(f"\n=== {label}  ({addr:#x}) — {len(sites)} direct branch sites")
        for at, owner, txt in sites[:a.limit]:
            print(f"    {at:#010x}  {owner:<44} {txt}")
        if len(sites) > a.limit:
            print(f"    ... and {len(sites) - a.limit} more")
        if a.words:
            w = words.get(label, [])
            print(f"    -- {len(w)} words in the image hold this address")
            for at, owner in w[:a.limit]:
                print(f"    {at:#010x}  {owner}")
            if not sites and w:
                print("    !! zero direct calls but the address is stored: this is")
                print("       reached through a function pointer. Find the callers")
                print("       by reading whoever loads these words — the entry is a")
                print("       `blx <reg>` or `ldr pc, [..]`, not a `bl`.")
    print("")
    return 0


if __name__ == "__main__":
    sys.exit(main())
