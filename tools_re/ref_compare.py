#!/usr/bin/env python3
"""Compare a probe frame against a photo of the original running on the S9.

    python ref_compare.py docs/reference/original-s9-02-class-select.png \
                          out/shot/24.png --out out/ab-class-select.png

`ab_compare.py` answers a different question. It demands byte equality between
two runs of *our own* replay, which is the right bar for "did porting this
guest function change any pixel". It cannot be pointed at the answer key,
because the answer key is a device screenshot: the original ran at 400x240 and
the S9 scaled it up by an integer factor into a 2220x1080 frame with black
letterbox bars, its own navigation glyphs down the right edge, and whatever
the panel did to the colours.

So this tool does the inverse mapping and then measures:

  1. find the game rectangle -- the largest non-black region, snapped to the
     5:3 WIPI aspect, or forced with `--rect x,y,w,h`
  2. box-downscale it to 400x240, which is exactly undoing an integer upscale
  3. report mean and 95th-percentile per-channel error, and the fraction of
     pixels off by more than `--tol`
  4. write a three-panel image: reference, ours, and the difference

Read the numbers as "is this the same screen", not as "is this the same
bytes". A photo of a phone cannot answer the second question and this tool
does not pretend to. What it does catch, and catch reliably, is the class of
bug that actually happens here: a panel drawn at the wrong offset, a font a
row too high, a palette entry off, a layer missing entirely.

Requires `pip install pillow`.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

try:
    from PIL import Image, ImageChops
except ImportError:  # pragma: no cover
    sys.exit("pip install pillow")

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")


def longest_run(flags: list[bool]) -> tuple[int, int]:
    """(start, length) of the longest unbroken run of True."""
    best = (0, 0)
    start = None
    for i, f in enumerate(flags + [False]):
        if f and start is None:
            start = i
        elif not f and start is not None:
            if i - start > best[1]:
                best = (start, i - start)
            start = None
    return best


def game_rect(im: Image.Image, aspect: float, black: int) -> tuple[int, int, int, int]:
    """The game area inside a device capture.

    The letterbox is black, so "brighter than `black`" separates content from
    bars. Two things break the naive first-to-last scan: the system navigation
    glyphs down one edge are content by that test, and a game frame that is
    itself mostly black (this game's fire background is) has no content near
    its own edges. So the scan is per line, longest unbroken run, and the
    widest such run across all lines wins — the nav strip loses because it is
    narrow, and a dark frame loses only if it is dark *everywhere*.

    That last case is real, which is why `--calibrate` exists: point it at a
    capture of a bright screen from the same device and session, and the rect
    comes from that instead.
    """
    g = im.convert("L")
    w, h = g.size
    px = g.load()
    x0 = bw = y0 = bh = 0
    for y in range(0, h, 2):
        s, n = longest_run([px[x, y] > black for x in range(w)])
        if n > bw:
            x0, bw = s, n
    for x in range(0, w, 2):
        s, n = longest_run([px[x, y] > black for y in range(h)])
        if n > bh:
            y0, bh = s, n
    if bw == 0 or bh == 0:
        return 0, 0, w, h
    # Snap to the WIPI aspect: keep the shorter axis, shrink the longer one
    # around the box centre.
    if bw / bh > aspect:
        nw = int(round(bh * aspect))
        x0 += (bw - nw) // 2
        bw = nw
    else:
        nh = int(round(bw / aspect))
        y0 += (bh - nh) // 2
        bh = nh
    return x0, y0, bw, bh


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("reference", help="device capture of the original")
    ap.add_argument("ours", help="probe frame dump (WIPI resolution)")
    ap.add_argument("--rect", help="force the game rect in the reference: x,y,w,h")
    ap.add_argument("--calibrate", metavar="IMAGE",
                    help="take the game rect from this capture instead (use a "
                         "bright screen from the same device and session)")
    ap.add_argument("--black", type=int, default=24,
                    help="luma at or below which a pixel counts as letterbox")
    ap.add_argument("--tol", type=int, default=32,
                    help="per-channel difference that counts as a mismatch")
    ap.add_argument("--out", help="write the three-panel comparison here")
    a = ap.parse_args()

    ours = Image.open(a.ours).convert("RGB")
    w, h = ours.size
    ref_full = Image.open(a.reference).convert("RGB")

    if a.rect:
        rx, ry, rw, rh = (int(v) for v in a.rect.split(","))
    else:
        src = Image.open(a.calibrate).convert("RGB") if a.calibrate else ref_full
        if src.size != ref_full.size:
            sys.exit("--calibrate image must be the same size as the reference")
        rx, ry, rw, rh = game_rect(src, w / h, a.black)
    print(f"reference {ref_full.size[0]}x{ref_full.size[1]}, "
          f"game rect {rw}x{rh} at ({rx},{ry})  "
          f"scale {rw / w:.2f}x")

    ref = ref_full.crop((rx, ry, rx + rw, ry + rh)).resize((w, h), Image.BOX)

    diff = ImageChops.difference(ref, ours)
    vals = list(diff.getdata())
    flat = [c for px in vals for c in px]
    flat.sort()
    n = len(flat)
    mean = sum(flat) / n
    p95 = flat[int(n * 0.95)]
    bad = sum(1 for px in vals if max(px) > a.tol)
    print(f"mean |error| {mean:6.2f}   p95 {p95:3d}   "
          f"pixels over tol {bad}/{w * h} ({100 * bad / (w * h):.1f}%)")

    if a.out:
        gap = 8
        panel = Image.new("RGB", (w * 3 + gap * 2, h), (0, 0, 0))
        panel.paste(ref, (0, 0))
        panel.paste(ours, (w + gap, 0))
        panel.paste(diff, (w * 2 + gap * 2, 0))
        Path(a.out).parent.mkdir(parents=True, exist_ok=True)
        panel.save(a.out)
        print(f"wrote {a.out}  (reference | ours | difference)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
