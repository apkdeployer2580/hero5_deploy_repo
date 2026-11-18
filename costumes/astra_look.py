#!/usr/bin/env python3
"""A working view of one astra avatar while drawing it.

    python costumes/astra_look.py 1 out/astra/look01.png

Top: the head views (1 3/4, 0 front, 3 back, 4 side, 6 3/4 down, 2 hair-back)
and the torso views (1, 0, 2) at 12x, ruled in the BASE image's coordinates
(the ones a grid's `at` is written in). Bottom: the hero assembled the way
the guest draws it — stand, weapon stance and a walk frame in four
directions — beside the draft. Holes (clear pixels walled in on four sides)
are printed per frame.
"""
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'tools_re'))
sys.path.insert(0, str(HERE))
import mgr  # noqa: E402
import rig  # noqa: E402
import astra_studio as studio  # noqa: E402
import astra_motion as motion  # noqa: E402

BG = (44, 52, 66, 255)
CAT = json.loads((HERE.parent / 'docs/astra_avatar/catalog.json').read_text(encoding='utf-8'))


def ruled(im, base_size, sc, label):
    padx = (im.width - base_size[0]) // 2
    pady = (im.height - base_size[1]) // 2
    z = im.resize((im.width * sc, im.height * sc), Image.NEAREST)
    t = Image.new('RGBA', (z.width + 24, z.height + 30), BG)
    t.alpha_composite(z, (22, 28))
    d = ImageDraw.Draw(t)
    for x in range(im.width + 1):
        d.line([(22 + x * sc, 28), (22 + x * sc, 28 + z.height)], fill=(0, 0, 0, 70))
    for y in range(im.height + 1):
        d.line([(22, 28 + y * sc), (22 + z.width, 28 + y * sc)], fill=(0, 0, 0, 70))
    for x in range(im.width):
        bx = x - padx
        d.text((22 + x * sc + 2, 16), str(bx % 10), fill=(255, 220, 0, 255) if bx == 0 else (190, 190, 120, 255))
    for y in range(im.height):
        by = y - pady
        d.text((2, 28 + y * sc + 1), str(by), fill=(255, 220, 0, 255) if by == 0 else (190, 190, 120, 255))
    # the base image's own rectangle
    d.rectangle([22 + padx * sc, 28 + pady * sc, 22 + (padx + base_size[0]) * sc, 28 + (pady + base_size[1]) * sc],
                outline=(255, 255, 255, 60))
    d.text((24, 2), label, fill=(255, 255, 0, 255))
    return t


def row(tiles, gap=10):
    w = sum(t.width + gap for t in tiles)
    h = max(t.height for t in tiles)
    out = Image.new('RGBA', (w, h), BG)
    x = 0
    for t in tiles:
        out.alpha_composite(t, (x, 0))
        x += t.width + gap
    return out


def figures(n, out, sc=10):
    """The hero assembled, big: stand front/side/back, weapon stance side and
    front, two walk frames — the view to judge a body by."""
    design = studio.load_design(n)
    cls = design.get('PREVIEW_CLASS', 0)
    design, raw, sheets = motion.sheets_for(n, cls)
    cif = mgr.cif_decode(rig.patch_cif(raw), True)
    picks = [(0, 1, 0), (0, 2, 0), (0, 0, 0), (2, 1, 0), (2, 2, 0), (1, 2, 1), (1, 2, 4), (1, 1, 1)]
    cells = []
    for m, d, f in picks:
        im = motion.render(cif.anims[m][d][f], sheets, False)
        im = im.crop((8, 4, 64, 66))
        bg = Image.new('RGBA', im.size, BG)
        bg.alpha_composite(im)
        cells.append(bg.resize((im.width * sc, im.height * sc), Image.NEAREST))
    page = row(cells, 6)
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    page.save(out)
    print('saved', out, page.size)


def frames_of(n, out, anim, d, sc=8, static=False):
    """Every frame of one motion and direction, side by side."""
    design = studio.load_design(n)
    cls = design.get('PREVIEW_CLASS', 0)
    design, raw, sheets = motion.sheets_for(n, cls)
    cif = mgr.cif_decode(raw if static else rig.patch_cif(raw), True)
    cells = []
    for parts in cif.anims[anim][d]:
        im = motion.render(parts, sheets, False).crop((8, 4, 64, 66))
        bg = Image.new('RGBA', im.size, BG)
        bg.alpha_composite(im)
        cells.append(bg.resize((im.width * sc, im.height * sc), Image.NEAREST))
    page = row(cells, 4)
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    page.save(out)
    print('saved', out, page.size)


def weapons(n, out, sc=6):
    """Every weapon piece of an avatar, by class, the shield, and a strip of
    each class's recoloured effects."""
    design = studio.load_design(n)
    rows = []
    for part in studio.parts(n):
        kind = part[4]
        if kind is None:
            continue
        source, ims = studio.compose_part(design, n, part)
        imgs = studio.compose_images(source, ims)
        if kind.startswith('fx'):
            keep = source['RECOLOUR']['keep']
            imgs = [im for i, im in enumerate(imgs) if i not in keep][:14]
            k = 2
        else:
            k = sc
        tiles = []
        for im in imgs:
            bg = Image.new('RGBA', im.size, (24, 28, 38, 255))
            bg.alpha_composite(im)
            tiles.append(bg.resize((im.width * k, im.height * k), Image.NEAREST))
        rows.append(row(tiles, 8))
    W = max(r.width for r in rows)
    page = Image.new('RGBA', (W, sum(r.height + 10 for r in rows)), BG)
    y = 0
    for r in rows:
        page.alpha_composite(r, (0, y))
        y += r.height + 10
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    page.save(out)
    print('saved', out, page.size)


def main():
    if sys.argv[1] == 'weapons':
        weapons(int(sys.argv[2]), sys.argv[3], int(sys.argv[4]) if len(sys.argv) > 4 else 6)
        return
    if sys.argv[1] == 'frames':
        frames_of(int(sys.argv[2]), sys.argv[3], int(sys.argv[4]), int(sys.argv[5]),
                  int(sys.argv[6]) if len(sys.argv) > 6 else 8, 'static' in sys.argv)
        return
    if sys.argv[1] == 'fig':
        figures(int(sys.argv[2]), sys.argv[3], int(sys.argv[4]) if len(sys.argv) > 4 else 6)
        return
    n = int(sys.argv[1])
    out = Path(sys.argv[2])
    sc = int(sys.argv[3]) if len(sys.argv) > 3 else 12
    design = studio.load_design(n)
    cls = design.get('PREVIEW_CLASS', 0)
    design, raw, sheets = motion.sheets_for(n, cls)
    base_head = studio.frames('c/sp/img0/000.mgr')
    base_body = studio.frames('c/sp/img1/000.mgr')
    # holes, from the canvases
    for i, im in enumerate(base_head):
        if im.size == (1, 1):
            continue
        h = studio.holes(studio.head_canvas(design, i, im))
        if h:
            print('head %d holes: %s' % (i, h))
    tiles = []
    for i in (1, 0, 3, 4, 6, 2):
        tiles.append(ruled(sheets[0][i], base_head[i].size, sc, 'head %d' % i))
    top = row(tiles)
    tiles = [ruled(sheets[1][i], base_body[i].size, sc, 'body %d' % i) for i in (1, 0, 2)]
    mid = row(tiles)
    cif = mgr.cif_decode(rig.patch_cif(raw), True)
    cells = []
    for m, f in ((0, 0), (2, 0), (1, 1), (3, 4)):
        for want in (1, 2, 0, 3):
            d = 2 if want == 3 else want
            frames = cif.anims[m][d]
            im = motion.render(frames[f % len(frames)], sheets, want == 3)
            cells.append(im.resize((im.width * 4, im.height * 4), Image.NEAREST))
    strip = Image.new('RGBA', (8 * cells[0].width, 2 * cells[0].height), BG)
    for k, c in enumerate(cells):
        strip.alpha_composite(c, ((k % 8) * c.width, (k // 8) * c.height))
    draft = Image.open(HERE.parent / 'docs/astra_avatar' / CAT[n - 1]['draft']).convert('RGBA')
    draft = draft.crop(draft.getbbox())
    draft = draft.resize((int(draft.width * strip.height / draft.height), strip.height), Image.LANCZOS)
    bottom = row([strip, draft])
    W = max(top.width, mid.width, bottom.width)
    page = Image.new('RGBA', (W, top.height + mid.height + bottom.height + 20), BG)
    page.alpha_composite(top, (0, 0))
    page.alpha_composite(mid, (0, top.height + 10))
    page.alpha_composite(bottom, (0, top.height + mid.height + 20))
    out.parent.mkdir(parents=True, exist_ok=True)
    page.save(out)
    print('saved', out, page.size)


if __name__ == '__main__':
    main()
