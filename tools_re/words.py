#!/usr/bin/env python3
"""Print the instruction word at `symbol+offset`, decoded, for building a
verify-then-write table (hl5_core::tenpages).

    python words.py libgame.so _ZN14NetStorageItemC1Ev+0x20 _ZN14NetStorageItemC1Ev+0x30
"""
import sys
from pathlib import Path
from capstone import Cs, CS_ARCH_ARM, CS_MODE_ARM
from elftools.elf.elffile import ELFFile

so = Path(sys.argv[1])
elf = ELFFile(open(so, "rb"))
syms = {}
for sec in (elf.get_section_by_name(".symtab"), elf.get_section_by_name(".dynsym")):
    if sec:
        for s in sec.iter_symbols():
            if s["st_info"]["type"] == "STT_FUNC" and s["st_value"]:
                syms.setdefault(s.name, s["st_value"] & ~1)
text = elf.get_section_by_name(".text")
raw = so.read_bytes()
md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
for arg in sys.argv[2:]:
    name, off = arg.rsplit("+", 1)
    a = syms[name] + int(off, 0)
    o = text["sh_offset"] + a - text["sh_addr"]
    w = int.from_bytes(raw[o:o + 4], "little")
    ins = next(md.disasm(raw[o:o + 4], a))
    print(f"{arg:60} {a:#08x} {w:#010x}  {ins.mnemonic} {ins.op_str}")
