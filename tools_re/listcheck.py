#!/usr/bin/env python3
"""Every PC the probe saw read a list position, against `widen.py`'s seeds
(ANALYSIS.md §65.11).

    python tools_re/widen.py payload/libHeroesLore5.so --seeds | grep ^SEED > seeds.txt
    cargo run -q -p hl5_probe -- --boot --ten-pages --memwatch lists ... > run.log
    python tools_re/listcheck.py payload/libHeroesLore5.so seeds.txt run.log [more.log ...]

`--memwatch lists` records each PC that touches `StateInNpcMenu+0x11/+0x14`,
`StateInGameMenu+0xf/+0x10/+0x12` and `gv+0x1B3E`. A read on a list screen
of that screen's own list field that is not a seed is a hole in the static
census — an instruction `tenlists::WIDEN` never looked at. The other reads
are printed for the record: another screen (the shop keeps its page cell in
`+0x11`), or a list screen using that byte for something else (the bag
menu's grid cell in `+0xf`).
"""
import re
import sys
from pathlib import Path

from capstone import Cs, CS_ARCH_ARM, CS_MODE_ARM

sys.path.insert(0, str(Path(__file__).parent))
from bagscan import Image  # noqa: E402

KIND_LAB = {"npc+11": "THIS+0x11", "npc+14": "THIS+0x14", "game+f": "THIS+0xf", "game+10": "THIS+0x10",
            "game+12": "THIS+0x12", "gv+1b3e": "gv+0x1b3e"}


def main():
    img = Image(Path(sys.argv[1]))
    md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
    seeds = {}
    for line in open(sys.argv[2]):
        _, a, lab, name = line.split()
        seeds[int(a, 16)] = (lab, name)
    fields = {}
    for (lab, name) in seeds.values():
        fields.setdefault(name, set()).add(lab)
    hits = {}
    for rep in sys.argv[3:]:
        for line in open(rep, encoding="utf-8", errors="replace"):
            m = re.match(r"\s*(npc\+11|npc\+14|game\+f|game\+10|game\+12|gv\+1b3e)\s+(0x[0-9a-f]+) (\S+)"
                         r"\s+\[.*\] r(\d+) w(\d+)", line)
            if m and int(m.group(4)):
                key = (m.group(1), int(m.group(2), 16))
                where, n = hits.get(key, (m.group(3), 0))
                hits[key] = (where, n + int(m.group(4)))
    bad = 0
    for (kind, pc), (where, r) in sorted(hits.items(), key=lambda x: x[0][1]):
        ins = next(md.disasm(img.read(pc, 4), pc))
        fn = where.split("+")[0]
        if pc in seeds:
            continue
        if fn not in fields:
            status = "OTHER-SCREEN"
        elif KIND_LAB[kind] not in fields[fn]:
            status = "OTHER-FIELD"
        else:
            status = "NOT A SEED"
            bad += 1
        print(f"{status:12} {kind:8} {pc:#x} {where:60} {ins.mnemonic} {ins.op_str}  reads {r}")
    print(f"{len(hits)} reading PC(s), {sum(1 for k in hits if k[1] in seeds)} seeds, "
          f"{bad} list-screen read(s) the census missed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
