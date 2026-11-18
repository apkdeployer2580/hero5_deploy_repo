#!/usr/bin/env python3
"""Pixel-exact A/B comparison of two frame-dump directories.

    python ab_compare.py out_ref out_patched
    python ab_compare.py out_ref out_patched --diff-dir out_diff

This is the acceptance test for every host reimplementation of a guest
function. The claim you are making is not "it looks the same" — it is **"the
host path produces the same bytes the shipped code produced"**, and the only
evidence for that is two runs of the same deterministic replay, one with the
patch and one without, whose dumped frames are byte-identical.

Anything less is a guess. A blend rounded the other way, a clip off by one, a
palette index taken from the wrong nibble — all of those look fine and are all
wrong, and they surface later as one corrupt sprite in one scene you did not
replay.

Exit code 0 when every pair matches, 1 otherwise, so it drops straight into a
script or a pre-commit check.

Requires `pip install pillow`. PNG-only by default; with `--raw WxH` it also
compares headerless framebuffer dumps.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")


def load(path: Path, raw: tuple[int, int] | None):
    if raw:
        data = path.read_bytes()
        w, h = raw
        return (w, h, data)
    try:
        from PIL import Image
    except ImportError:  # pragma: no cover
        sys.exit("pip install pillow (or use --raw WxH for headerless dumps)")
    im = Image.open(path).convert("RGBA")
    return (im.width, im.height, im.tobytes())


def compare_pair(a: Path, b: Path, raw, diff_dir: Path | None) -> tuple[bool, str]:
    wa, ha, da = load(a, raw)
    wb, hb, db = load(b, raw)
    if (wa, ha) != (wb, hb):
        return False, f"size differs: {wa}x{ha} vs {wb}x{hb}"
    if da == db:
        return True, "identical"

    stride = len(da) // ha if ha else len(da)
    px = stride // wa if wa else 4
    first = None
    ndiff = 0
    for i in range(0, len(da), px):
        if da[i:i + px] != db[i:i + px]:
            ndiff += 1
            if first is None:
                y, x = divmod(i // px, wa)
                first = (x, y, da[i:i + px].hex(), db[i:i + px].hex())
    total = wa * ha
    x, y, av, bv = first
    msg = (f"{ndiff}/{total} pixels differ ({100.0 * ndiff / total:.4f}%); "
           f"first at ({x}, {y}): {av} vs {bv}")

    if diff_dir is not None:
        try:
            from PIL import Image
            diff_dir.mkdir(parents=True, exist_ok=True)
            out = bytearray(len(da))
            for i in range(0, len(da), px):
                if da[i:i + px] != db[i:i + px]:
                    out[i:i + px] = b"\xff\x00\xff\xff"[:px]
                else:
                    # Keep the reference at quarter brightness so the magenta
                    # differences read against the scene they are in.
                    out[i:i + px] = bytes(v // 4 for v in da[i:i + px])
            Image.frombytes("RGBA", (wa, ha), bytes(out)).save(diff_dir / a.name)
            msg += f"  -> {diff_dir / a.name}"
        except ImportError:
            pass
    return False, msg


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("ref", help="reference dump directory (or a single file)")
    ap.add_argument("test", help="dump directory to check against it")
    ap.add_argument("--glob", default="*.png", help="which files to pair up")
    ap.add_argument("--raw", metavar="WxH",
                    help="compare headerless framebuffer dumps of this size")
    ap.add_argument("--diff-dir", help="write a magenta difference image per "
                                       "mismatching frame")
    a = ap.parse_args()

    raw = None
    if a.raw:
        w, h = a.raw.lower().split("x")
        raw = (int(w), int(h))

    ref, test = Path(a.ref), Path(a.test)
    diff_dir = Path(a.diff_dir) if a.diff_dir else None

    if ref.is_file():
        pairs = [(ref, test)]
    else:
        names = sorted(p.name for p in ref.glob(a.glob))
        if not names:
            sys.exit(f"no files matching {a.glob} in {ref}")
        pairs = [(ref / n, test / n) for n in names]

    ok = bad = missing = 0
    for pa, pb in pairs:
        if not pb.exists():
            print(f"  MISSING  {pb}")
            missing += 1
            continue
        same, msg = compare_pair(pa, pb, raw, diff_dir)
        if same:
            ok += 1
            print(f"  same     {pa.name}")
        else:
            bad += 1
            print(f"  DIFFERS  {pa.name}: {msg}")

    print()
    print(f"{ok} identical, {bad} differing, {missing} missing "
          f"({len(pairs)} compared)")
    if bad or missing:
        print()
        print("A difference is not automatically a bug in the port — but it is")
        print("automatically something you have to explain. Before assuming the")
        print("host path is wrong, check the three things that make a *replay*")
        print("diverge rather than the code:")
        print("  1. the save/storage directory carried over between runs")
        print("  2. the run used the wall clock instead of a virtual one")
        print("  3. a switch that deliberately changes what is drawn was left on")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
