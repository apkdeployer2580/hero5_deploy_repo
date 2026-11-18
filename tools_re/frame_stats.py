#!/usr/bin/env python3
"""Per-frame cost as a distribution, not a mean.

    python frame_stats.py cost.txt --inst-per-sec 46e6 --budget-ms 33
    python frame_stats.py after.txt --before before.txt
    your_probe --frames 2600 --emit-cost | python frame_stats.py -

Input is one number per frame — guest instructions, microseconds, whatever your
runner counts — as lines, or comma-separated, on a file or stdin.

Why this exists at all:

  "It stutters after a boss fight" and "it lags when there are lots of
  monsters" are the two most common bug reports on a rehosted game, and
  **neither of them shows up in an average**. A run can average a comfortable
  10 ms and still miss the budget on one frame in twelve, because one guest
  function does a whole second of work in a single frame — a full-screen blur
  when a menu opens, a DES pass over a save file, an archive rescan.

  So the numbers that matter are p99, the max, the count of frames over budget,
  and — most useful of all — **which frame numbers those were**. A frame number
  is directly actionable: the next command is your probe's `--dump <that
  frame>`, and the one after that is a profile of that frame alone.

`--before` prints the same distribution for a previous run beside the current
one, which is the shape of evidence a performance claim needs: not "it feels
smoother" but "p99 went from 84.6 ms to 2.2 ms and frames over budget went from
205 to 1, on the identical replay".
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")


def read_costs(src: str) -> list[float]:
    text = sys.stdin.read() if src == "-" else Path(src).read_text(encoding="utf-8",
                                                                  errors="replace")
    out: list[float] = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        toks = [t for t in re.split(r"[\s,;]+", line) if t]
        try:
            # A pure list of numbers: one per line, or all on one line.
            out.extend(float(t) for t in toks)
        except ValueError:
            # A runner's log line — "frame 812: 1234567 inst (+98765)". Take the
            # *first* number after any leading frame index, i.e. the last token
            # that is a bare number, so a trailing delta does not win.
            nums = [t for t in toks if re.fullmatch(r"\d+(?:\.\d+)?", t)]
            if nums:
                out.append(float(nums[-1]))
    if not out:
        sys.exit(f"no numbers found in {src}")
    return out


def summarise(cost: list[float], per_sec: float | None, budget_ms: float):
    s = sorted(cost)
    n = len(s)

    def q(p):
        return s[min(n - 1, int((n - 1) * p))]

    ms = (lambda v: v / per_sec * 1e3) if per_sec else (lambda v: v)
    stats = {
        "n": n,
        "mean": sum(cost) / n,
        "p50": q(0.50),
        "p90": q(0.90),
        "p99": q(0.99),
        "max": s[-1],
    }
    stats["over"] = sum(1 for c in cost if ms(c) > budget_ms)
    stats["ms"] = {k: ms(v) for k, v in stats.items() if k in
                   ("mean", "p50", "p90", "p99", "max")}
    return stats


def line(label: str, st: dict, unit: str) -> str:
    m = st["ms"]
    return (f"  {label:<10} mean {st['mean']:>12,.0f} ({m['mean']:>7.1f} {unit})   "
            f"p50 {m['p50']:>7.1f}   p90 {m['p90']:>7.1f}   "
            f"p99 {m['p99']:>7.1f}   max {m['max']:>8.1f}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("costs", help="file with one cost per frame, or - for stdin")
    ap.add_argument("--before", help="a previous run's costs, for comparison")
    ap.add_argument("--inst-per-sec", type=float, default=None,
                    help="convert counts to milliseconds at this rate "
                         "(measure it on the slowest device you support)")
    ap.add_argument("--budget-ms", type=float, default=33.0,
                    help="frame budget; 33 for 30 fps, 16.7 for 60")
    ap.add_argument("--worst", type=int, default=8,
                    help="how many worst frame numbers to name")
    a = ap.parse_args()

    unit = "ms" if a.inst_per_sec else "raw"
    cost = read_costs(a.costs)
    st = summarise(cost, a.inst_per_sec, a.budget_ms)

    print()
    if a.inst_per_sec:
        print(f"frame cost, converted at {a.inst_per_sec / 1e6:.0f} M/s, "
              f"budget {a.budget_ms:g} ms")
    else:
        print("frame cost (raw units — pass --inst-per-sec to see milliseconds)")
    print()
    if a.before:
        prev = read_costs(a.before)
        pst = summarise(prev, a.inst_per_sec, a.budget_ms)
        print(line("before", pst, unit))
        print(line("after", st, unit))
        tot_b, tot_a = sum(prev), sum(cost)
        if tot_a:
            print(f"\n  total       {tot_b:,.0f} -> {tot_a:,.0f}"
                  f"   = {tot_b / tot_a:.1f}x")
        print(f"  over budget {pst['over']}/{pst['n']} -> {st['over']}/{st['n']}")
        if pst["ms"]["p99"] and st["ms"]["p99"]:
            print(f"  p99         {pst['ms']['p99']:.1f} -> {st['ms']['p99']:.1f} "
                  f"{unit}   = {pst['ms']['p99'] / st['ms']['p99']:.1f}x")
        print()
        print("  The mean is the least interesting row here. If the mean improved")
        print("  and p99 did not, you sped up the frames that were already fine.")
    else:
        print(line("frames", st, unit))
        print()
        print(f"  {st['over']}/{st['n']} frames over the {a.budget_ms:g} "
              f"{unit} budget ({100.0 * st['over'] / st['n']:.1f}%)")

    ranked = sorted(enumerate(cost, 1), key=lambda kv: -kv[1])[:a.worst]
    conv = (lambda v: v / a.inst_per_sec * 1e3) if a.inst_per_sec else (lambda v: v)
    print("\n  worst frames: " + "  ".join(
        f"#{i} ({conv(c):.0f}{unit})" for i, c in ranked))
    print()
    print("  Next command: dump the worst frame, then profile that frame alone.")
    print("  A single frame's profile names the guest function to port; a")
    print("  whole-run profile averages it away.")
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
