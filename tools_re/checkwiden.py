#!/usr/bin/env python3
"""Decode every word of `tenlists::WIDEN` back and check it is the rewrite its
comment says (ANALYSIS.md §65.11).

    python tools_re/checkwiden.py hl5_core/src/tenlists.rs

The unit test `every_rewrite_is_one_of_the_documented_shapes` checks the bit
patterns; this checks the same thing through capstone, as text, so a person
can read what changed: `ldrsb rT, [...]` must become `ldrb rT, [...]` and
nothing else, `lsl rD, rM, #0x18` must become `mov rD, rM`, and so on.
"""
import re
import sys
from collections import Counter

from capstone import Cs, CS_ARCH_ARM, CS_MODE_ARM


def main():
    md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
    rows = []
    for line in open(sys.argv[1], encoding="utf-8"):
        m = re.match(r"\s*wd\((\w+), (0x[0-9a-f]+), 0x([0-9a-f]{4})_([0-9a-f]{4}), "
                     r"0x([0-9a-f]{4})_([0-9a-f]{4})\), // (.*)", line)
        if m:
            was = int(m.group(3) + m.group(4), 16)
            new = int(m.group(5) + m.group(6), 16)
            rows.append((m.group(1), m.group(2), was, new, m.group(7)))

    def dis(w):
        for i in md.disasm(w.to_bytes(4, "little"), 0x1000):
            return f"{i.mnemonic} {i.op_str}"
        return "??"

    def num(t):
        # `#7` and `#0x7` are the same number
        return re.sub(r"#(0x[0-9a-f]+|\d+)\b", lambda m: f"#{int(m.group(1), 0):#x}", t)

    pat, bad = Counter(), []
    for (sym, off, was, new, why) in rows:
        a, b = num(dis(was)), num(dis(new))
        kind = why.split(":")[0].split(" ")[0]
        if kind.startswith("ldrsb"):
            ok = a.replace("ldrsb", "ldrb") == b
        elif kind == "sx":
            m = re.match(r"(\w+) (\w+), (\w+), #0x18", a)
            ok = m is not None and b == f"{'movs' if m.group(1).endswith('s') else 'mov'} {m.group(2)}, {m.group(3)}"
        elif kind == "opsx" or (kind == "cmpS" and "lsl" in why):
            ok = re.sub(r", (asr|lsr|lsl) #0x18", "", a) == b
        elif kind == "cmpS":
            m = re.match(r"cmp (\w+), #(0x[0-9a-f]+)", a)
            ok = m is not None and b == f"cmp {m.group(1)}, #{int(m.group(2), 16) >> 24:#x}"
        elif kind == "and255":
            m = re.match(r"and(\w*) (\w+), (\w+), #0xff", a)
            ok = m is not None and b == f"add{m.group(1)} {m.group(2)}, {m.group(3)}, #0x0"
        elif kind == "tst80":
            ok = a.replace("#0x80", "#0x80000000") == b
        elif kind == "sentinel":
            if "cmp #0" in why:
                ok = a.replace("#0x0", "#0xff") == b
            else:
                ok = (a.replace("lt", "eq", 1) == b) or (a.replace("ge", "ne", 1) == b)
        else:
            ok = False
        pat[kind] += 1
        if not ok:
            bad.append((sym, off, a, b, why))
    print(dict(pat))
    print(f"{len(rows)} word(s), {len(bad)} not the rewrite their comment names")
    for r in bad:
        print("  ", r)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
