#!/usr/bin/env python3
"""Where does each filter-list index come from? (ANALYSIS.md §65.11)

    python idxsource.py libgame.so bagscan-output.txt

For every `filter`-region access `bagscan.py` printed, walk backwards from
the load through straight-line code and follow the index register through
`mov`/`add`/`sub`/`lsl`/`asr` until it is defined by a memory load — the
field or global the list position lives in — or the trail goes cold (a
branch target, a call, or more than 30 instructions). Prints the chain, so
the result is checkable by eye.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from bagscan import Image, Fn  # noqa: E402
from capstone import CS_ARCH_ARM, CS_MODE_ARM, Cs  # noqa: E402

for _s in (sys.stdout,):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")

REG = r"(r\d+|sb|sl|fp|ip|lr)"


def branch_targets(f):
    t = set()
    for i in f.ins.values():
        m = re.match(r"b(?:eq|ne|cs|hs|cc|lo|mi|pl|vs|vc|hi|ls|ge|lt|gt|le)?$", i.mnemonic)
        if m and i.op_str.startswith("#"):
            t.add(int(i.op_str[1:], 0))
    return t


def chain(f, targets, at, r):
    out = []
    a = at - 4
    want = {r}
    n = 0
    while a in f.ins and n < 30 and want:
        i = f.ins[a]
        n += 1
        ops = [x.strip() for x in i.op_str.split(",")]
        d = ops[0] if ops else ""
        if i.mnemonic in ("bl", "blx") and want & {"r0", "r1", "r2", "r3", "ip"}:
            out.append((a, f"{i.mnemonic} {i.op_str}", "CALL-RESULT"))
            break
        if d in want and not i.mnemonic.startswith(("str", "cmp", "cmn", "tst", "teq", "b", "push", "stm")):
            out.append((a, f"{i.mnemonic} {i.op_str}", ""))
            want.discard(d)
            if i.mnemonic.startswith("ldr"):
                continue  # a source
            for s in re.findall(REG, ",".join(ops[1:])):
                want.add(s)
        if a in targets:
            out.append((a, "<- branch target", ""))
            break
        a -= 4
    return out


def main():
    img = Image(Path(sys.argv[1]))
    md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
    md.skipdata = True
    seen = set()
    fns = {}
    for line in open(sys.argv[2], encoding="utf-8"):
        if not line.startswith("filter"):
            continue
        f_ = line.split()
        adr = int(f_[1], 16)
        fn = f_[2].split("+")[0]
        if adr in seen or "BagItem" in fn:
            continue
        seen.add(adr)
        start = img.by_name[fn][0]
        size = img.syms[start][1]
        if fn not in fns:
            f = Fn(img, md, fn, start, size)
            fns[fn] = (f, branch_targets(f))
        f, tg = fns[fn]
        i = f.ins[adr]
        m = re.search(r"\[" + REG + r", " + REG, i.op_str)
        if not m:
            continue
        idx = m.group(2)
        print(f"{adr:#08x} {fn}+{adr - start:#x}  {i.mnemonic} {i.op_str}")
        for (a, t, note) in chain(f, tg, adr, idx):
            print(f"    {a:#08x} {t} {note}")


if __name__ == "__main__":
    main()
