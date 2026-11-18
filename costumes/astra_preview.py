#!/usr/bin/env python3
"""Previews and checks for the astra avatars (v3), from the exported sheets.

    python costumes/astra_preview.py

For every avatar folder in docs/astra_avatar/native:
  motion_<class>.gif   the four rigged motions in four directions at the
                       game's tick, through the rig's cif variant, for the
                       class the avatar is shown with (워리어; 10 로그)
  motion_strip.png     the same motions frame by frame
  weapons.png          the class weapons, the shield and a strip of each
                       class's recoloured effects
and, for all of them:
  overview_2x.png      stand, front and 3/4, 2x
  validation.json      every part reference of every animation of every
                       class, through the rig variant with every avatar's
                       sheets, lands inside its sheet

These are assembly previews (effect transforms approximated), not
framebuffer captures; the replays (astra_replay.py) give those.
"""
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools_re'))
sys.path.insert(0, str(ROOT / 'costumes'))
import mgr  # noqa: E402
import rig  # noqa: E402
import astra_motion as motion  # noqa: E402

OUT = ROOT / 'docs/astra_avatar/native'
BG = (24, 33, 46, 255)
KIND = {0: 'sword', 1: 'dagger', 2: 'gun', 3: 'spear'}
CLASS_NAME = {0: 'warrior', 1: 'rogue', 2: 'gunner', 3: 'knight'}


def decode(path):
    return [mgr.gbm_to_png_image(g, g.palette).convert('RGBA') for g in mgr.mgr_decode(path.read_bytes())]


def defaults(cls, cif):
    out = {}
    for layer in range(len(cif.layer_ids)):
        src = 'c/sp/img%d/%03d.mgr' % (layer, mgr.CLASS_WEAPON[cls] if layer == 4 else cls if layer == 5 else 0)
        out[layer] = [mgr.gbm_to_png_image(g, g.palette).convert('RGBA') for g in mgr.mgr_decode(mgr.load_source(src)[1])]
    return out


def dressed(entries, n, cls, base):
    """The sheets a `cls` hero wearing avatar n in full draws with."""
    sheets = dict(base)
    for e in entries:
        if e['avatar'] != n or not e['classes'] & (1 << cls):
            continue
        sheets[e['layer']] = decode(OUT / e['mgr'])
    return sheets


def main():
    manifest = json.loads((OUT / 'manifest.json').read_text(encoding='utf-8'))
    catalog = json.loads((OUT.parent / 'catalog.json').read_text(encoding='utf-8'))
    raws = [mgr.load_source('c/sp/cif/%03d.cif' % (c + 1))[1] for c in range(4)]
    cifs = [mgr.cif_decode(rig.patch_cif(r), True) for r in raws]
    bases = [defaults(c, mgr.cif_decode(raws[c], True)) for c in range(4)]
    checks = []
    board = Image.new('RGBA', (11 * 112, 236), BG)
    d = ImageDraw.Draw(board)
    for n, a in enumerate(catalog, 1):
        folder = OUT / ('%02d_%s' % (n, a['slug']))
        shown = 1 if n == 10 else 0
        for c in range(4):
            sheets = dressed(manifest, n, c, bases[c])
            count = 0
            for anim in cifs[c].anims:
                for direction in anim:
                    for parts in direction:
                        for p in parts:
                            assert p.layer in sheets and p.index < len(sheets[p.layer]), (n, c, p)
                            count += 1
            checks.append({'avatar': n, 'class': c, 'part_references_checked': count})
            if c != shown:
                continue
            # motion GIF: rows = motions, columns = down / right / up / left
            longest = max(len(cifs[c].anims[m][0]) for m in rig.RIG_ANIMS)
            gif = []
            for t in range(longest * 2):
                page = Image.new('RGBA', (4 * motion.CELL, 4 * motion.CELL), BG)
                for row, m in enumerate(rig.RIG_ANIMS):
                    for col, want in enumerate((1, 2, 0, 3)):
                        dd = 2 if want == 3 else want
                        frames = cifs[c].anims[m][dd]
                        page.alpha_composite(motion.render(frames[t % len(frames)], sheets, want == 3),
                                             (col * motion.CELL, row * motion.CELL))
                gif.append(page.resize((page.width * 3, page.height * 3), Image.NEAREST).convert('RGB'))
            gif[0].save(folder / ('motion_%s.gif' % CLASS_NAME[c]), save_all=True, append_images=gif[1:],
                        duration=motion.TICK_MS, loop=0)
            rows = []
            for m in rig.RIG_ANIMS:
                for want in (1, 2, 0):
                    frames = cifs[c].anims[m][want]
                    strip = Image.new('RGBA', (len(frames) * motion.CELL, motion.CELL), BG)
                    for f, parts in enumerate(frames):
                        strip.alpha_composite(motion.render(parts, sheets, False), (f * motion.CELL, 0))
                    rows.append(strip)
            w = max(r.width for r in rows)
            page = Image.new('RGBA', (w, motion.CELL * len(rows)), BG)
            for k, r in enumerate(rows):
                page.alpha_composite(r, (0, k * motion.CELL))
            page.resize((page.width * 2, page.height * 2), Image.NEAREST).save(folder / 'motion_strip.png')
            for k, want in enumerate((1, 2)):
                im = motion.render(cifs[c].anims[0][want][0], sheets, False)
                crop = im.crop((8, 4, 64, 59)).resize((112, 110), Image.NEAREST)
                board.alpha_composite(crop, ((n - 1) * 112, 16 + k * 110))
        d.text(((n - 1) * 112 + 5, 1), '%02d' % n, fill='#f2c36c')
        # weapons: every class's pieces, the shield, the effects
        tiles = []
        for e in sorted((e for e in manifest if e['avatar'] == n and e['layer'] in (4, 5, 6)),
                        key=lambda e: (e['layer'] == 5, e['layer'], e['sheet'])):
            ims = decode(OUT / e['mgr'])
            k = 4 if e['layer'] != 5 else 2
            if e['layer'] == 5:
                ims = [im for im in ims if im.width > 12][:10]
            row_tiles = []
            for im in ims:
                bg = Image.new('RGBA', im.size, BG)
                bg.alpha_composite(im)
                row_tiles.append(bg.resize((im.width * k, im.height * k), Image.NEAREST))
            rw = sum(t.width + 6 for t in row_tiles)
            rh = max(t.height for t in row_tiles)
            r = Image.new('RGBA', (rw, rh), BG)
            x = 0
            for t in row_tiles:
                r.alpha_composite(t, (x, 0))
                x += t.width + 6
            tiles.append(r)
        W = max(t.width for t in tiles)
        page = Image.new('RGBA', (W, sum(t.height + 8 for t in tiles)), BG)
        y = 0
        for t in tiles:
            page.alpha_composite(t, (0, y))
            y += t.height + 8
        page.save(folder / 'weapons.png')
        for old in folder.glob('anim_*'):
            old.unlink()
    board.save(OUT / 'overview_2x.png')
    (OUT / 'validation.json').write_text(json.dumps(checks, indent=2), encoding='utf-8')
    print('All %d avatar/class combinations passed CIF index checks through the rig variant; %d part references'
          % (len(checks), sum(x['part_references_checked'] for x in checks)))


if __name__ == '__main__':
    main()
