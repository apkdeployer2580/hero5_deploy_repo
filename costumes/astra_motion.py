#!/usr/bin/env python3
"""Motion previews of an astra avatar through the rig's cif variant — the
four rigged motions (stand / walk, without and with a weapon) in the four
directions, at the game's own tick.

    python costumes/astra_motion.py 1 out/astra/m01        # GIF + strip PNG
    python costumes/astra_motion.py 1 out/astra/m01 --static  # v2 behaviour:
                                                           # the archive's cif

The game advances one cif frame per tick and its designed tick is 36 ms
(CLAUDE.md, `timerLoop`); the GIF uses 40 ms, the nearest a GIF can say.
Drawn the way SPRITE::DrawSprite does: parts in list order, each centred at
its (x, y). The stored side frames face left and the guest mirrors them
for RIGHT (x -> -x, flip toggled).
"""
import argparse
import sys
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'tools_re'))
sys.path.insert(0, str(HERE))
import mgr  # noqa: E402
import rig  # noqa: E402
import astra_studio as studio  # noqa: E402

CELL = 72
TICK_MS = 40
NAMES = {0: 'stand', 1: 'walk', 2: 'stand_armed', 3: 'walk_armed'}


def render(parts, sheets, mirror):
    out = Image.new('RGBA', (CELL, CELL), (0, 0, 0, 0))
    for p in parts:
        if p.layer == 5 or p.layer not in sheets:
            continue
        im = sheets[p.layer][p.index]
        flip, x = p.flip, p.x
        if mirror:
            x = -x
            flip = {0: 1, 1: 0}.get(flip, 3)
        if p.effect:
            sx, sy, rot, alpha, _ = p.effect
            im = im.resize((max(1, im.width * sx // 25), max(1, im.height * sy // 25)), Image.NEAREST)
            im = im.rotate((-rot * 2 if mirror else rot * 2), expand=True)
        if flip:
            im = im.transpose({1: Image.Transpose.FLIP_LEFT_RIGHT, 2: Image.Transpose.FLIP_TOP_BOTTOM,
                               3: Image.Transpose.ROTATE_180}[flip])
        out.alpha_composite(im, (CELL // 2 + x - im.width // 2, CELL - 14 + p.y - im.height // 2))
    return out


def sheets_for(n, cls):
    design = studio.load_design(n)
    cif_raw = mgr.load_source('c/sp/cif/%03d.cif' % (cls + 1))[1]
    sheets = {}
    base = mgr.cif_decode(cif_raw, True)
    for layer in range(len(base.layer_ids)):
        src = 'c/sp/img%d/%03d.mgr' % (layer, mgr.CLASS_WEAPON[cls] if layer == 4 else cls if layer == 5 else 0)
        sheets[layer] = studio.frames(src)
    for layer in range(4):
        source, ims = studio.compose_layer(design, n, layer)
        sheets[layer] = studio.compose_images(source, ims)
    if design.get('WEAPONS'):
        kind = ('sword', 'dagger', 'gun', 'spear')[cls]
        for part in studio.parts(n):
            if part[4] in (kind, 'fx%d' % cls) or (part[4] == 'shield' and cls == 3):
                source, ims = studio.compose_part(design, n, part)
                sheets[part[1]] = studio.compose_images(source, ims)
    else:
        source, ims = studio.compose_layer(design, n, 4)
        if source['CLASSES'] & (1 << cls):
            sheets[4] = studio.compose_images(source, ims)
    return design, cif_raw, sheets


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('n', type=int)
    ap.add_argument('out')
    ap.add_argument('--cls', type=int)
    ap.add_argument('--static', action='store_true', help='the archive cif (no rig)')
    ap.add_argument('--scale', type=int, default=4)
    a = ap.parse_args()
    design = studio.load_design(a.n)
    cls = a.cls if a.cls is not None else design.get('PREVIEW_CLASS', 0)
    design, raw, sheets = sheets_for(a.n, cls)
    cif = mgr.cif_decode(raw if a.static else rig.patch_cif(raw), True)
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    bg = (44, 52, 66, 255)
    # One GIF: the four motions stacked, four directions across.
    longest = max(len(cif.anims[m][0]) for m in rig.RIG_ANIMS)
    gif = []
    for t in range(longest * 2):
        page = Image.new('RGBA', (4 * CELL, 4 * CELL), bg)
        for row, m in enumerate(rig.RIG_ANIMS):
            dirs = cif.anims[m]
            for col, want in enumerate((1, 2, 0, 3)):
                d = 2 if want == 3 else want
                frames = dirs[d]
                im = render(frames[t % len(frames)], sheets, want == 3)
                page.alpha_composite(im, (col * CELL, row * CELL))
        gif.append(page.resize((page.width * a.scale, page.height * a.scale), Image.NEAREST).convert('RGB'))
    gif[0].save(str(out) + '.gif', save_all=True, append_images=gif[1:], duration=TICK_MS, loop=0)
    # A strip PNG: every frame of each motion, for reading frame by frame.
    rows = []
    for m in rig.RIG_ANIMS:
        for want in (1, 2, 0):
            frames = cif.anims[m][want]
            strip = Image.new('RGBA', (len(frames) * CELL, CELL), bg)
            for f, parts in enumerate(frames):
                strip.alpha_composite(render(parts, sheets, False), (f * CELL, 0))
            rows.append(strip)
    w = max(r.width for r in rows)
    page = Image.new('RGBA', (w, CELL * len(rows)), bg)
    for k, r in enumerate(rows):
        page.alpha_composite(r, (0, k * CELL))
    page.resize((page.width * a.scale, page.height * a.scale), Image.NEAREST).save(str(out) + '_strip.png')
    print('saved', str(out) + '.gif', str(out) + '_strip.png')


if __name__ == '__main__':
    main()
