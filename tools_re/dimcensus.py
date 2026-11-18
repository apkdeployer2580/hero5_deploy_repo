#!/usr/bin/env python3
"""Does the guest hardcode a screen *dimension*, or does it ask for one?

    python tools_re/dimcensus.py payload/libHeroesLore5.so
    python tools_re/dimcensus.py payload/libHeroesLore5.so --imm 240 320 480

This is the audit behind ANALYSIS.md §27.8 (the width) and §33.1 (the height),
and it is the thing to re-run before moving either extent again. It answers two
questions over the whole of `.text`:

  A  what does the guest compare a *reported* dimension against?
     `Graphic::GetWidth()` is tested against #240 / #320 / #480 -- a resolution
     ladder the canvas must stay on the right rung of. `GetHeight()` is tested
     against **nothing constant**, which is why the height was free to move.

  B  where does a bare immediate like 240 appear, and as what?
     Most of them are `[sp, #0xf0]` and friends. The ones that matter are `cmp`
     and `mov`, and on this binary every one of those is a width test, an
     `_ENEMY_G_DATA` stride (240-byte records, always next to a `mul`), a
     `memset` length, or an HSB hue.

Two traps, both of which silently produce the *reassuring* answer:

  * **capstone stops at the first word it cannot decode.** `Cs.disasm` is a
    generator; on this image it gives up 1,298 instructions in -- 0.4% of
    `.text` -- and every count below comes out zero, which reads exactly like
    "nothing is hardcoded". Restart at the next word. The banner prints the
    coverage so a truncated run is visible; it should say 99.7%.

  * **the register a getter returned gets reused.** Walking forward from
    `bl GetHeight` for a `cmp #imm` without noticing that `r0` was reloaded two
    instructions later reports `Graphic::FillRoundRect` as testing the height
    against 255. It is testing an *alpha* against 255. Any instruction that
    writes the tracked register ends the trail.

Needs `pip install pyelftools capstone`.
"""

from __future__ import annotations

import argparse
import bisect
import collections
import re
import sys

from capstone import CS_ARCH_ARM, CS_MODE_ARM, Cs
from capstone.arm import ARM_OP_IMM, ARM_OP_REG
from elftools.elf.elffile import ELFFile

# The six accessors that report a screen extent. Addresses are stable in this
# binary; resolved by name so a rebuilt guest still works.
GETTERS = (
    "_ZN7Graphic8GetWidthEv",
    "_ZN7Graphic9GetHeightEv",
    "_ZN7Graphic12GetHalfWidthEv",
    "_ZN7Graphic13GetHalfHeightEv",
    "_ZN15FrameBufferInfo8GetWidthEv",
    "_ZN15FrameBufferInfo9GetHeightEv",
)

PRETTY = {
    "_ZN7Graphic8GetWidthEv": "Graphic::GetWidth",
    "_ZN7Graphic9GetHeightEv": "Graphic::GetHeight",
    "_ZN7Graphic12GetHalfWidthEv": "Graphic::GetHalfWidth",
    "_ZN7Graphic13GetHalfHeightEv": "Graphic::GetHalfHeight",
    "_ZN15FrameBufferInfo8GetWidthEv": "FrameBufferInfo::GetWidth",
    "_ZN15FrameBufferInfo9GetHeightEv": "FrameBufferInfo::GetHeight",
}

IMM = re.compile(r"#(0x[0-9a-fA-F]+|\d+)")
ADDRESSING = re.compile(r"\[[^\]]*#(0x[0-9a-fA-F]+|\d+)[^\]]*\]")


def load(path):
    elf = ELFFile(open(path, "rb"))
    text = elf.get_section_by_name(".text")
    if text is None:
        sys.exit(f"{path}: no .text")

    funcs = sorted(
        (s["st_value"] & ~1, s["st_size"], s.name)
        for sec in elf.iter_sections()
        if sec.name in (".symtab", ".dynsym")
        for s in sec.iter_symbols()
        if s["st_info"]["type"] == "STT_FUNC" and s["st_value"]
    )
    return text["sh_addr"], text.data(), funcs


def disassemble(base, data):
    """Every ARM word, restarting past the ones capstone will not decode."""
    md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
    md.detail = True
    out, off = [], 0
    while off < len(data):
        n = 0
        for insn in md.disasm(data[off:], base + off):
            out.append(insn)
            n += insn.size
        off += n if n else 4
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("so", nargs="?", default="payload/libHeroesLore5.so")
    ap.add_argument("--imm", type=int, nargs="*", default=[240, 400],
                    help="immediates to classify (default: the two authored extents)")
    ap.add_argument("--list", action="store_true",
                    help="print every cmp/mov site, not just the counts")
    args = ap.parse_args()

    base, data, funcs = load(args.so)
    starts = [a for a, _, _ in funcs]
    by_name = {name: addr for addr, _, name in funcs}

    def owner(addr):
        n = bisect.bisect_right(starts, addr) - 1
        if n < 0:
            return hex(addr)
        start, size, name = funcs[n]
        return f"{name}+0x{addr - start:x}" if (size == 0 or addr < start + size) else hex(addr)

    insns = disassemble(base, data)
    words = len(data) // 4
    coverage = 100.0 * len(insns) / words
    print(f"{args.so}: decoded {len(insns)}/{words} words ({coverage:.1f}% of .text)")
    if coverage < 99.0:
        print("  !! low coverage -- every count below is an undercount", file=sys.stderr)
    print()

    targets = {by_name[n]: PRETTY[n] for n in GETTERS if n in by_name}
    missing = [n for n in GETTERS if n not in by_name]
    if missing:
        print(f"  (not in this binary: {', '.join(missing)})\n")

    # --- A. every call, and what the returned value is compared against ------
    sites = collections.defaultdict(list)
    for insn in insns:
        if insn.mnemonic in ("bl", "b") and insn.op_str.startswith("#"):
            target = int(insn.op_str[1:], 0)
            if target in targets:
                sites[targets[target]].append(insn.address)

    print("=== A. dimension getters: call sites ===")
    for name in targets.values():
        fns = {owner(a).split("+")[0] for a in sites[name]}
        print(f"  {name:28s} {len(sites[name]):4d} calls in {len(fns):3d} distinct functions")

    index = {insn.address: n for n, insn in enumerate(insns)}
    ladder = collections.defaultdict(list)
    for name, addrs in sites.items():
        for addr in addrs:
            n = index.get(addr)
            if n is None:
                continue
            reg = "r0"
            for insn in insns[n + 1: n + 12]:
                ops = insn.operands
                written = {insn.reg_name(r) for r in insn.regs_access()[1]}
                if (insn.mnemonic.startswith("cmp") and len(ops) == 2
                        and ops[0].type == ARM_OP_REG and ops[1].type == ARM_OP_IMM
                        and insn.reg_name(ops[0].reg) == reg):
                    ladder[(name, ops[1].imm)].append(addr)
                    break
                if (insn.mnemonic.startswith("cmp") and len(ops) == 2
                        and ops[0].type == ARM_OP_REG and ops[1].type == ARM_OP_REG
                        and reg in (insn.reg_name(ops[0].reg), insn.reg_name(ops[1].reg))):
                    ladder[(name, None)].append(addr)
                    break
                # A plain move forwards the value; anything else that writes the
                # register ends the trail (see the FillRoundRect trap above).
                if (insn.mnemonic == "mov" and len(ops) == 2 and ops[1].type == ARM_OP_REG
                        and insn.reg_name(ops[1].reg) == reg):
                    reg = insn.reg_name(ops[0].reg)
                    continue
                if reg in written or insn.mnemonic in ("bl", "blx"):
                    break

    print("\n=== A. what the returned value is compared against ===")
    print("    (a constant is a resolution ladder; a register is a runtime clip)")
    for (name, imm), addrs in sorted(ladder.items(),
                                     key=lambda kv: (kv[0][0], kv[0][1] is None, -len(kv[1]))):
        what = "another register" if imm is None else f"#{imm}"
        print(f"  {name:28s} vs {what:20s} {len(addrs):4d} sites")
        if imm is not None:
            for a in addrs[:12]:
                print(f"        {owner(a)}")

    # --- B. bare immediates, classified -------------------------------------
    for want in args.imm:
        hits = [i for i in insns if any(int(m, 0) == want for m in IMM.findall(i.op_str))]
        cat, examples = collections.Counter(), collections.defaultdict(list)
        for insn in hits:
            if any(int(m, 0) == want for m in ADDRESSING.findall(insn.op_str)):
                kind = "addressing  [reg/pc/sp, #imm]"
            elif insn.mnemonic.startswith(("and", "bic", "orr", "eor", "tst")):
                kind = "bitmask"
            elif insn.mnemonic.startswith("cmp"):
                kind = "cmp         <- a value test"
            elif insn.mnemonic.startswith("mov"):
                kind = "mov         <- a constant load"
            elif insn.mnemonic.startswith(("add", "sub", "rsb")):
                kind = "add/sub     offset arithmetic"
            else:
                kind = "other: " + insn.mnemonic
            cat[kind] += 1
            if len(examples[kind]) < 3:
                examples[kind].append(insn)

        print(f"\n=== B. immediate {want} in .text: {len(hits)} sites ===")
        for kind, n in cat.most_common():
            print(f"  {n:4d}  {kind}")
            for insn in examples[kind]:
                print(f"          {owner(insn.address)}  {insn.mnemonic} {insn.op_str}")

        # `cmp` and `mov` are the only shapes a coordinate could hide in. Print
        # them all, flagging the ones sitting next to a `mul` -- that is a
        # record stride, not a screen extent.
        show = [i for i in hits if i.mnemonic.startswith(("cmp", "mov"))]
        if show and (args.list or len(show) <= 40):
            print(f"\n  every cmp/mov site for {want}:")
            for insn in show:
                n = index[insn.address]
                near = [j.mnemonic for j in insns[max(0, n - 3): n + 4]]
                tag = "  <- next to a mul (record stride)" if any(
                    m.startswith(("mul", "mla")) for m in near) else ""
                print(f"    {owner(insn.address):50s} {insn.mnemonic} {insn.op_str}{tag}")


if __name__ == "__main__":
    main()
