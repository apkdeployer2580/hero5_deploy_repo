#!/usr/bin/env python3
"""Every small-field access a menu state makes through its own `this` — the
census behind widening the list cursors (ANALYSIS.md §65.11).

    python cursorscan.py libgame.so --funcs 'StateInNpcMenu|StateInGameMenu'
    python cursorscan.py libgame.so --funcs '...' --offs 0x11,0x12 --uses

The bag's filtered lists (`BagItem::SetItemFilter`) are paged through by
cursors the screens keep in their own object as **signed bytes**; with 172
bag slots a list can pass 127. This reuses `bagscan.py`'s data flow with one
more source — `r0` at the entry of a member function is `THIS` — and prints
every byte/halfword access to `THIS+off`.

`--uses` follows each value loaded from one of `--offs` through the
registers it is copied/derived into until the next call or store, and prints
the instructions that read it — the ones whose meaning depends on the sign:
`lsl #24`/`asr #24`, `tst #0x80`, signed condition codes, call arguments.
It is a reading aid, not a proof; the proof is the replay (`--memwatch`).
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import bagscan  # noqa: E402
from bagscan import Image, Scan, Fn  # noqa: E402


class ThisScan(Scan):
    def entry_state(self, name):
        st = super().entry_state(name)
        st["r0"] = frozenset([("p", "THIS", 0, frozenset())])
        return st

    def note(self, f, i, a, w, store, hits):
        if a is None or a[0] not in ("p", "x") or a[1] != "THIS":
            return
        off = a[2]
        if not (0 <= off < 0x400) or w > 2:
            return
        scale = a[3] if a[0] == "x" else 0
        hits.append((i.address, f.name, "THIS", off, scale, w, store,
                     bagscan.prov(a), f"{i.mnemonic} {i.op_str}"))


SIGNED_CC = {"lt", "ge", "gt", "le", "mi", "pl"}


def uses(img, md, fn_name, start, size, at, dst):
    """Straight-line forward walk from `at` following `dst` and its copies."""
    f = Fn(img, md, fn_name, start, size)
    live = {dst}
    out = []
    a = at + 4
    steps = 0
    while a in f.ins and steps < 40 and live:
        i = f.ins[a]
        steps += 1
        txt = f"{i.mnemonic} {i.op_str}"
        toks = set(re.findall(r"\b(r\d+|sb|sl|fp|ip|lr)\b", i.op_str))
        dst_reg = i.op_str.split(",")[0].strip() if i.op_str else ""
        reads = toks - ({dst_reg} if not i.mnemonic.startswith(("str", "cmp", "cmn", "tst", "teq")) else set())
        if i.mnemonic.startswith(("str", "cmp", "cmn", "tst", "teq")):
            reads = toks
        hit = bool(reads & live)
        if hit:
            out.append((a, txt))
            if not i.mnemonic.startswith(("str", "cmp", "cmn", "tst", "teq", "b")):
                live.add(dst_reg)
        elif dst_reg in live and not i.mnemonic.startswith(("str", "cmp", "cmn", "tst", "teq", "b")):
            live.discard(dst_reg)
        if i.mnemonic in ("bl", "blx") or i.mnemonic.startswith(("pop", "ldm")) or i.mnemonic == "b":
            out.append((a, txt + "   <stop>"))
            break
        if i.mnemonic.startswith("b") and i.mnemonic[1:] in SIGNED_CC | {"eq", "ne", "hi", "ls", "cc", "cs", "lo", "hs"}:
            out.append((a, txt))
        a += 4
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("so", type=Path)
    ap.add_argument("--funcs", default="StateInNpcMenu|StateInGameMenu")
    ap.add_argument("--offs", default=None, help="comma list; default: every offset")
    ap.add_argument("--uses", action="store_true")
    a = ap.parse_args()
    img = Image(a.so)
    rx = re.compile(a.funcs)
    sc = ThisScan(img)
    text = img.elf.get_section_by_name(".text")
    lo, hi = text["sh_addr"], text["sh_addr"] + text["sh_size"]
    for addr in img.addrs:
        name, size, thumb = img.syms[addr]
        if thumb or not size or not (lo <= addr < hi) or not rx.search(name):
            continue
        try:
            sc.run(name, addr, size)
        except Exception as e:
            print(f"!! {name}: {e}", file=sys.stderr)
    offs = {int(x, 0) for x in a.offs.split(",")} if a.offs else None
    rows = sorted(sc.hits)
    for (adr, fn, base, off, scale, w, store, pv, txt) in rows:
        if offs is not None and off not in offs:
            continue
        func_off = adr - img.by_name[fn][0]
        print(f"{adr:#08x} {fn}+{func_off:#x}  {'W' if store else 'R'}{w} "
              f"THIS+{off:#x}{f' x{scale}' if scale else ''}  [{txt}]")
        if a.uses and not store:
            dst = txt.split()[1].rstrip(",")
            start = img.by_name[fn][0]
            size = img.syms[start][1]
            for (u, t) in uses(img, sc.md, fn, start, size, adr, dst):
                print(f"      {u:#08x}  {t}")
    print(f"# {len(rows)} access(es)", file=sys.stderr)


if __name__ == "__main__":
    main()
