#!/usr/bin/env python3
"""Symbol-aware disassembler for a 32-bit ARM guest, with PIC globals resolved.

    python disasm.py libgame.so DM_DrawScreen565
    python disasm.py libgame.so 0x47c2c:0x48000
    python disasm.py libgame.so SomeFunc --plain      # no annotations

What it adds over `objdump -d`, and why each one matters when you are porting a
guest function to the host:

  branch targets      resolved to `name+offset`, so a call chain reads as names
  literal pools       `ldr rX, [pc, #n]` shows the word that will be loaded
  PIC globals         GCC PIC reaches a global as
                          ldr rG, [pc, #lit]      ; GOT *offset*
                          add r4, pc, rL          ; GOT base
                          ldr rP, [r4, rG]        ; the global's address
                          ldr/str [rP, #field]    ; the access
                      A linear trace follows that chain and annotates the last
                      two lines with the global's **name**. Without this, a
                      PIC-compiled guest reads as an unbroken wall of `ldr`s and
                      you cannot tell a loop counter from a framebuffer pointer.
  stack arguments     with `--frame`, prints the `[sp, #N]` slot that each
                      incoming argument occupies, computed from the prologue.
                      Argument 5 and beyond live on the stack, and their offset
                      moves with every `push` and `sub sp` — getting this wrong
                      is the single most common way a host port reads the wrong
                      arguments and produces almost-right output.

The trace is a linear approximation: it ignores control flow, so a register
written on one path and read on another can be annotated wrongly. It does not
guess — a slot it is unsure of is simply left unannotated.

Needs `pip install pyelftools capstone`.
"""

from __future__ import annotations

import argparse
import bisect
import re
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
        self.syms: dict[int, tuple[str, int, bool]] = {}
        self.by_name: dict[str, tuple[int, int, bool]] = {}
        for secname in (".symtab", ".dynsym"):
            sec = self.elf.get_section_by_name(secname)
            if sec is None:
                continue
            for s in sec.iter_symbols():
                if s["st_shndx"] == "SHN_UNDEF" or not s.name:
                    continue
                if s["st_info"]["type"] not in ("STT_FUNC", "STT_OBJECT"):
                    continue
                v, sz = s["st_value"], s["st_size"]
                self.syms.setdefault(v & ~1, (s.name, sz, bool(v & 1)))
                self.by_name.setdefault(s.name, (v & ~1, sz, bool(v & 1)))
        self.addrs = sorted(self.syms)

        self.plt: dict[int, str] = {}
        rel = self.elf.get_section_by_name(".rel.plt")
        dynsym = self.elf.get_section_by_name(".dynsym")
        plt = self.elf.get_section_by_name(".plt")
        if rel is not None and dynsym is not None and plt is not None:
            base = plt["sh_addr"]
            for i, r in enumerate(rel.iter_relocations()):
                self.plt[base + 20 + 12 * i] = dynsym.get_symbol(r["r_info_sym"]).name

    def read(self, va: int, n: int) -> bytes | None:
        for v, off, filesz, memsz in self.segs:
            if v <= va < v + memsz:
                d = va - v
                if d + n > filesz:
                    return bytes(n)  # .bss
                return self.raw[off + d:off + d + n]
        return None

    def word(self, va: int) -> int | None:
        b = self.read(va, 4)
        return int.from_bytes(b, "little") if b else None

    def label(self, va: int) -> str:
        if va in self.plt:
            return f"{self.plt[va]}@plt"
        i = bisect.bisect_right(self.addrs, va & ~1) - 1
        if i < 0:
            return f"{va:#x}"
        a = self.addrs[i]
        name, size, _ = self.syms[a]
        if (va & ~1) == a:
            return name
        if size and (va & ~1) < a + size:
            return f"{name}+{(va & ~1) - a:#x}"
        if (va & ~1) - a < 0x2000:
            return f"{name}+{(va & ~1) - a:#x}"
        return f"{va:#x}"


ARG_REGS = 4  # r0..r3; argument 5 onwards is on the stack


def frame_note(img: Image, start: int, code: bytes, thumb: bool) -> list[str]:
    """Work out where incoming stack arguments sit, from the prologue.

    `push {r4..r11, lr}` moves sp down by 4 per register; `sub sp, sp, #N` moves
    it by N. After both, incoming argument index i (0-based, counting from the
    first stack argument, i.e. the function's fifth parameter) is at
    `[sp, #pushed + N + 4*i]`. Every host port that reads arguments has to get
    this exactly right, and two functions in the same binary routinely differ
    because one of them has a bigger frame."""
    md = Cs(CS_ARCH_ARM, CS_MODE_THUMB if thumb else CS_MODE_ARM)
    pushed = 0
    subbed = 0
    out = []
    for ins in md.disasm(code[:64], start):
        if ins.mnemonic.startswith("push") or (
            ins.mnemonic.startswith("stmdb") and "sp!" in ins.op_str
        ):
            pushed += 4 * (ins.op_str.count(",") + 1 - ins.op_str.count("sp!"))
            # count registers in the {..} list properly, including ranges
            m = re.search(r"\{(.*)\}", ins.op_str)
            if m:
                n = 0
                for part in m.group(1).split(","):
                    part = part.strip()
                    if "-" in part:
                        a, b = part.split("-")
                        n += _rnum(b) - _rnum(a) + 1
                    elif part:
                        n += 1
                pushed = 4 * n
            out.append(f"    prologue: push {m.group(1) if m else ins.op_str}"
                       f"  -> sp -= {pushed:#x}")
        elif ins.mnemonic == "sub" and ins.op_str.startswith("sp, sp, #"):
            subbed = int(ins.op_str.split("#")[1], 0)
            out.append(f"    prologue: sub sp, #{subbed:#x}")
            break
        elif ins.mnemonic in ("bl", "b", "bx", "blx"):
            break
    if pushed or subbed:
        base = pushed + subbed
        out.append(f"    incoming stack args (parameter 5 onward):")
        for i in range(8):
            out.append(f"      arg[{i}] (parameter {ARG_REGS + 1 + i}) = "
                       f"[sp, #{base + 4 * i:#x}]")
    return out


def _rnum(name: str) -> int:
    name = name.strip()
    table = {"sl": 10, "fp": 11, "ip": 12, "sp": 13, "lr": 14, "pc": 15}
    if name in table:
        return table[name]
    return int(name.lstrip("r"))


def disassemble(img: Image, start: int, end: int, thumb: bool, annotate: bool):
    md = Cs(CS_ARCH_ARM, CS_MODE_THUMB if thumb else CS_MODE_ARM)
    # A literal pool in the middle of a function is data, and capstone stops at
    # the first word it cannot decode: without this, everything after such a
    # word was silently missing from the listing (ANALYSIS.md §65.2).
    md.skipdata = True
    code = img.read(start, end - start) or b""

    # Linear trace state. reg/stack slot -> ('lit', v) | ('gotbase', a) |
    # ('got', addr-of-global)
    val: dict = {}
    got_base = None

    for ins in md.disasm(code, start):
        extra = ""
        op = ins.op_str
        if annotate:
            m = re.match(r"^(\w+), \[pc, #(-?(?:0x)?[0-9a-fA-F]+)\]$", op)
            if ins.mnemonic == "ldr" and m:
                t = ins.address + (4 if thumb else 8) + int(m.group(2), 0)
                v = img.word(t)
                if v is not None:
                    val[m.group(1)] = ("lit", v)
                    extra = f"   ; lit = {v:#x}"
            elif ins.mnemonic == "add":
                m2 = re.match(r"^(\w+), pc, (\w+)$", op)
                if m2 and val.get(m2.group(2), (None,))[0] == "lit":
                    got_base = ins.address + (4 if thumb else 8) + val[m2.group(2)][1]
                    val[m2.group(1)] = ("gotbase", got_base)
                    extra = f"   ; GOT base = {got_base:#x}"
                else:
                    _kill(val, op)
            elif ins.mnemonic == "ldr":
                m2 = re.match(r"^(\w+), \[(\w+), (\w+)\]$", op)
                m3 = re.match(r"^(\w+), \[sp, #((?:0x)?[0-9a-fA-F]+)\]$", op)
                m4 = re.match(r"^(\w+), \[(\w+)(?:, #((?:0x)?[0-9a-fA-F]+))?\]$", op)
                if (m2 and val.get(m2.group(2), (None,))[0] == "gotbase"
                        and val.get(m2.group(3), (None,))[0] == "lit"):
                    slot = got_base + val[m2.group(3)][1]
                    addr = img.word(slot)
                    if addr is not None:
                        val[m2.group(1)] = ("got", addr)
                        extra = f"   ; GOT -> {img.label(addr)}"
                elif m3 and ("sp", int(m3.group(2), 0)) in val:
                    val[m3.group(1)] = val[("sp", int(m3.group(2), 0))]
                    kind, v = val[m3.group(1)]
                    extra = (f"   ; lit = {v:#x} (reloaded)" if kind == "lit"
                             else f"   ; GOT -> {img.label(v)} (reloaded)")
                elif m4 and val.get(m4.group(2), (None,))[0] == "got":
                    off = int(m4.group(3), 0) if m4.group(3) else 0
                    extra = f"   ; = *{img.label(val[m4.group(2)][1] + off)}"
                    val.pop(m4.group(1), None)
                else:
                    _kill(val, op)
            elif ins.mnemonic == "str":
                m3 = re.match(r"^(\w+), \[sp, #((?:0x)?[0-9a-fA-F]+)\]$", op)
                m4 = re.match(r"^(\w+), \[(\w+)(?:, #((?:0x)?[0-9a-fA-F]+))?\]$", op)
                if m3 and m3.group(1) in val:
                    val[("sp", int(m3.group(2), 0))] = val[m3.group(1)]
                elif m4 and val.get(m4.group(2), (None,))[0] == "got":
                    off = int(m4.group(3), 0) if m4.group(3) else 0
                    extra = f"   ; -> *{img.label(val[m4.group(2)][1] + off)}"
            elif ins.mnemonic.startswith("mov"):
                m2 = re.match(r"^(\w+), (\w+)$", op)
                if m2 and m2.group(2) in val:
                    val[m2.group(1)] = val[m2.group(2)]
                else:
                    _kill(val, op)
            elif op.startswith("#"):
                try:
                    extra = "   ; " + img.label(int(op[1:], 0))
                except ValueError:
                    pass
            else:
                _kill(val, op)
            if ins.mnemonic.startswith(("bl", "b.", "b\t")) or ins.mnemonic == "b":
                if op.startswith("#"):
                    try:
                        extra = "   ; " + img.label(int(op[1:], 0))
                    except ValueError:
                        pass
        print(f"  {ins.address:#010x}  {ins.bytes.hex():<8} {ins.mnemonic:<8} {op}{extra}")


def _kill(val: dict, op: str):
    m = re.match(r"^(\w+), ", op)
    if m and not op.startswith("["):
        val.pop(m.group(1), None)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("so")
    ap.add_argument("targets", nargs="+",
                    help="symbol names, or START:END as hex addresses")
    ap.add_argument("--plain", action="store_true", help="no annotations")
    ap.add_argument("--frame", action="store_true",
                    help="print where incoming stack arguments live")
    ap.add_argument("--thumb", action="store_true",
                    help="force Thumb (default: from the symbol's low bit)")
    a = ap.parse_args()

    img = Image(Path(a.so))
    for t in a.targets:
        if ":" in t:
            s, e = t.split(":")
            start, end, thumb = int(s, 0), int(e, 0), a.thumb
            name = f"{start:#x}..{end:#x}"
        else:
            if t not in img.by_name:
                print(f"!! no symbol named {t}")
                continue
            start, size, thumb = img.by_name[t]
            thumb = thumb or a.thumb
            if not size:
                nxt = bisect.bisect_right(img.addrs, start)
                size = (img.addrs[nxt] - start) if nxt < len(img.addrs) else 0x400
            end = start + size
            name = t
        print(f"\n===== {name} @ {start:#x}, {end - start:#x} bytes, "
              f"{'Thumb' if thumb else 'ARM'} =====")
        if a.frame:
            code = img.read(start, min(end - start, 64)) or b""
            for line in frame_note(img, start, code, thumb):
                print(line)
            print("")
        disassemble(img, start, end, thumb, not a.plain)
    print("")
    return 0


if __name__ == "__main__":
    sys.exit(main())
