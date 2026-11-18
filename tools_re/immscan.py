#!/usr/bin/env python3
"""Every instruction in a set of guest functions that uses one of a set of
immediates — as an offset, an operand or a compare — with its function name.

    python immscan.py libgame.so --funcs 'StateNetMenu|NETWORK' --imm 0x148 0x198 0x50
    python immscan.py libgame.so --funcs '^_ZN7BagItem' --imm 0x188 --context 3

`--funcs` is a regular expression over mangled names; `--imm` values are
matched against every immediate capstone reports for the instruction (memory
displacements included), so `ldrb r0, [r1, #0x148]`, `add r0, r0, #0x148`,
`cmp r4, #0x50` and `mov r2, #0x140` all show.

It is a census, not a classifier: `#0x148` is also an `ItemInfo` field and
`#0x50` is every other loop bound in the game. Read each hit's base register.
Needs `pip install pyelftools capstone`.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

from capstone import CS_ARCH_ARM, CS_MODE_ARM, Cs
from capstone.arm import ARM_OP_IMM, ARM_OP_MEM
from elftools.elf.elffile import ELFFile

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("so", type=Path)
    ap.add_argument("--funcs", required=True)
    ap.add_argument("--imm", nargs="+", required=True)
    ap.add_argument("--context", type=int, default=0)
    a = ap.parse_args()
    want = {int(v, 0) for v in a.imm}
    rx = re.compile(a.funcs)
    raw = a.so.read_bytes()
    elf = ELFFile(open(a.so, "rb"))
    text = elf.get_section_by_name(".text")
    tlo, thi, toff = text["sh_addr"], text["sh_addr"] + text["sh_size"], text["sh_offset"]
    funcs = []
    for sec in (elf.get_section_by_name(".symtab"), elf.get_section_by_name(".dynsym")):
        if sec is None:
            continue
        for s in sec.iter_symbols():
            if s["st_info"]["type"] == "STT_FUNC" and s["st_size"] and tlo <= s["st_value"] < thi:
                funcs.append((s["st_value"] & ~1, s["st_size"], s.name))
    funcs = sorted(set(funcs))
    md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
    md.detail = True
    # A literal pool inside a function is data, and capstone stops at the
    # first word it cannot decode -- without this the rest of a big function
    # is silently missing from the census.
    md.skipdata = True
    total = 0
    for addr, size, name in funcs:
        if not rx.search(name):
            continue
        code = raw[toff + addr - tlo : toff + addr - tlo + size]
        insns = list(md.disasm(code, addr))
        for i, ins in enumerate(insns):
            if ins.id == 0:  # a skipped data word
                continue
            hit = False
            for op in ins.operands:
                if op.type == ARM_OP_IMM and (op.imm & 0xFFFFFFFF) in want:
                    hit = True
                if op.type == ARM_OP_MEM and op.mem.disp in want:
                    hit = True
            if not hit:
                continue
            total += 1
            lo = max(0, i - a.context)
            for j in range(lo, min(len(insns), i + a.context + 1)):
                c = insns[j]
                mark = ">" if j == i else " "
                print(f"{mark} {c.address:#08x} {name}+{c.address - addr:#x}  {c.mnemonic} {c.op_str}")
            if a.context:
                print()
    print(f"# {total} hit(s)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
