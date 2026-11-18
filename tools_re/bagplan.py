#!/usr/bin/env python3
"""Turn `bagscan.py`'s accesses into a rewrite plan for the ten-page bag.

    python bagscan.py libgame.so > scan.txt
    python bagplan.py libgame.so scan.txt

For every access to the stack-count array (`BAG+0x188..`) or the filter array
(`BAG+0x1e8..`), pick the instruction among its provenance whose immediate
carries the array's base, and say what it becomes:

    stack   BAG+0x188 -> BAG+0x2b8      (+0x130 bytes)
    filter  BAG+0x1e8 -> BAG+0x860      (+0x678 bytes, +0x19e words)

and check that every other access depending on the same instruction is in the
same array (otherwise changing it would move something that must stay). The
output is one line per instruction word: `sym+off was old new`, ready to
paste into `hl5_core::tenpages`, followed by the problems, if any.
"""
import re
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from disasm import Image  # noqa: E402

from capstone import CS_ARCH_ARM, CS_MODE_ARM, Cs  # noqa: E402
from capstone.arm import ARM_OP_IMM, ARM_OP_MEM  # noqa: E402

DELTA = {"stack": 0x2B8 - 0x188, "filter": 0x860 - 0x1E8}
BASE = {"stack": 0x188, "filter": 0x1E8}
HERO_BAG = 0x17C0


def imm_of(w):
    t = (w >> 25) & 7
    if t == 1:
        rot = (w >> 8) & 0xF
        v = w & 0xFF
        return ((v >> (2 * rot)) | (v << (32 - 2 * rot))) & 0xFFFFFFFF if rot else v
    if t == 2:
        return w & 0xFFF
    if t == 0 and w & (1 << 22) and w & 0x90 == 0x90:
        return ((w >> 4) & 0xF0) | (w & 0xF)
    return None


def encodable(w, v):
    t = (w >> 25) & 7
    if t == 1:
        for rot in range(16):
            x = ((v << (2 * rot)) | (v >> (32 - 2 * rot))) & 0xFFFFFFFF if rot else v
            if x <= 0xFF:
                return True
        return False
    if t == 2:
        return 0 <= v <= 0xFFF
    if t == 0:
        return 0 <= v <= 0xFF
    return False


def main():
    so, scan = Path(sys.argv[1]), Path(sys.argv[2])
    img = Image(so)
    md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
    md.detail = True
    rows = []
    for line in scan.read_text(encoding="utf-8").splitlines():
        m = re.match(r"^(\w+)\s+(0x[0-9a-f]+) (\S+)\+(0x[0-9a-f]+)\s+([RW])(\d) (BAG|HERO)\+(-?0x[0-9a-f]+)(?: x(\d+))?\s+\[(.*)\]\s+imm@(.*)$", line)
        if not m:
            continue
        region, adr, fn, off, rw, w, base, boff, scale, text, prov = m.groups()
        prov = [int(p, 16) for p in prov.split(",") if p]
        rows.append(dict(region=region, adr=int(adr, 16), fn=fn, boff=int(boff, 16),
                         scale=int(scale or 0), text=text, prov=prov, base=base))
    users = defaultdict(set)  # instruction -> regions of accesses depending on it
    for r in rows:
        for p in r["prov"]:
            users[p].add(r["region"])
    plan = {}
    problems = []
    for r in rows:
        if r["region"] not in DELTA:
            continue
        reg = r["region"]
        cands = []
        for p in r["prov"]:
            w = img.word(p)
            ins = next(md.disasm(w.to_bytes(4, "little"), p))
            v = imm_of(w)
            if v is None:
                continue
            cands.append((p, w, v, ins))
        choice = None
        # 1. the access's own displacement carrying the array base
        for p, w, v, ins in cands:
            if p == r["adr"] and v >= BASE[reg] - 0x10 and v < BASE[reg] + 0x180:
                choice = (p, w, v, v + DELTA[reg], ins)
        # 2. an index bias in words (filter: 0x7a + k)
        if choice is None and reg == "filter":
            for p, w, v, ins in cands:
                if 0x7a <= v < 0x7a + 0x60 and ins.mnemonic.startswith("add"):
                    choice = (p, w, v, v + DELTA[reg] // 4, ins)
        # 3. a byte base pointer: add rX, rY, #0x188 / #0x1e8 (+k)
        if choice is None:
            for p, w, v, ins in cands:
                if BASE[reg] <= v < BASE[reg] + 0x10 and ins.mnemonic.startswith(("add", "mov")):
                    choice = (p, w, v, v + DELTA[reg], ins)
        # 4. HERO-relative: the small half of a split immediate
        if choice is None:
            hero_off = HERO_BAG + BASE[reg]
            for p, w, v, ins in sorted(cands, key=lambda c: c[2]):
                total = sum(c[2] for c in cands if c[3].mnemonic.startswith("add") or c[3].mnemonic.startswith("mov"))
                if v < 0x400 and ins.mnemonic.startswith("add") and encodable(w, v + DELTA[reg]):
                    choice = (p, w, v, v + DELTA[reg], ins)
                    break
        if choice is None:
            problems.append(f"NO CHOICE  {r['fn']}+{r['adr'] - img.by_name[r['fn']][0]:#x} [{r['text']}] prov={[hex(p) for p in r['prov']]}")
            continue
        p, w, v, nv, ins = choice
        if not encodable(w, nv):
            problems.append(f"NOT ENCODABLE {p:#x} {ins.mnemonic} {ins.op_str}: {nv:#x}")
        if users[p] - {reg}:
            problems.append(f"SHARED {p:#x} {ins.mnemonic} {ins.op_str}: used by {sorted(users[p])}")
        prev = plan.get(p)
        if prev and prev[3] != nv:
            problems.append(f"CONFLICT {p:#x}: {prev[3]:#x} vs {nv:#x}")
        plan[p] = (w, v, reg, nv, ins, r["fn"])
    for p in sorted(plan):
        w, v, reg, nv, ins, fn = plan[p]
        sym = img.label(p)
        print(f"{reg:6} {p:#08x} {sym:58} {w:#010x} {v:#x} -> {nv:#x}   {ins.mnemonic} {ins.op_str}")
    print(f"# {len(plan)} word(s)")
    for x in problems:
        print("!!", x)


if __name__ == "__main__":
    main()
