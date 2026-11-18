#!/usr/bin/env python3
"""The five hats drawn for the hero's bare head, fitted so they are WORN.

    python costumes/hats.py            # write sets/{witch_hat,crown,cat_ears,santa_hat,straw_hat}.py
    python costumes/hats.py preview out/hats.png

The first pass (§57.7) placed each hat on top of the hero's hair: the brims
sat at row 5-7 of the head while the bushy hair below them was as wide as
the brim, so every hat looked perched. A hat that is worn

  * comes down to the brow: the brim's lowest row is the row above the
    forehead (8 on the level views, 10 on the look-down view 6), so only the
    bangs show under it;
  * is centred on the head as the head is drawn in THAT view (the hair's
    span on the brim row — the 3/4 views are not the front's centre);
  * hides the hair above the brim (`clear_above`), and
  * shades what is right under it: the row under the brim is the hair one
    step darker along its own ramp — the brim's shadow.

A crown and a pair of ears are not brimmed: they sit down INTO the hair by
their own anchor row, with only the tall tuft above them cleared.

The grids are the designs (ours); the fit and the shadow are computed from
the archive's head sheet (img0/000) at write time, like every other costume
here. Nothing of the game's pixels is written: the shadow row names our own
colours for the hair under the brim.
"""
import pprint
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'tools_re'))
sys.path.insert(0, str(HERE))
import astra_studio as A  # noqa: E402

VIEWS = {0: 'FRONT', 1: 'THREE_Q', 3: 'BACK', 4: 'THREE_Q', 6: 'THREE_Q'}
# The brow: the hair's last row above the forehead, per view.
BROW = {0: 8, 1: 8, 3: 8, 4: 8, 6: 10}

# One step darker along the hero's own hair ramps (white bangs, teal, outline).
DARKER = {
    '#ffffff': '#dedbd6', '#dedbd6': '#bdb2ad', '#bdb2ad': '#9c8e84', '#9c8e84': '#7b695a',
    '#7b695a': '#4e4048', '#84b2b5': '#526d6b', '#526d6b': '#39555a', '#39555a': '#39414a',
    '#39414a': '#211c31',
}

HATS = {}

HATS['witch_hat'] = {
    'NAME': '마법사 모자',
    'kind': 'brim',
    'COLOURS': {'K': '#1c1430', 'P': '#3c2a6a', 'M': '#5e46a0', 'L': '#8a6cc8', 'H': '#b49ae6',
                'Y': '#e8b84a', 'y': '#a8782a'},
    'FRONT': [
        '............KK......',
        '...........KMK......',
        '..........KLMK......',
        '..........KLMPK.....',
        '.........KHLMPK.....',
        '........KHLMMPK.....',
        '........KLLMMPK.....',
        '.......KHLMMMMPK....',
        '.......KLLMMMMPK....',
        '......KHLMMMMMPK....',
        '.....KLLMMMMMMPPK...',
        '....KLLMMMMMMMMPK...',
        '...KYYYYYYYYYYyyK...',
        '...KyyyyyyyyyyyyK...',
        '.KHLLLMMMMMMMMMMPPK.',
        'KLMMMMMMMMMMMMMMMPPK',
        '.KKPPPPPPPPPPPPPPKK.',
    ],
    'THREE_Q': [
        '.................KK..',
        '................KMK..',
        '...............KLMK..',
        '..............KLMPK..',
        '.............KHLMPK..',
        '.............KLLMPK..',
        '............KHLMMPK..',
        '...........KLLMMMPK..',
        '..........KHLMMMMPK..',
        '.........KLLMMMMMPK..',
        '........KHLMMMMMMPK..',
        '.......KLLMMMMMMMPPK.',
        '.....KYYYYYYYYYYYyyK.',
        '.....KyyyyyyyyyyyyyK.',
        '..KHLLLMMMMMMMMMMMPPK',
        'KLMMMMMMMMMMMMMMMMMPPK',
        '.KKPPPPPPPPPPPPPPPPKK.',
    ],
}
HATS['witch_hat']['BACK'] = [r[::-1].replace('H', 'L') for r in HATS['witch_hat']['FRONT']]

HATS['santa_hat'] = {
    'NAME': '산타 모자',
    'kind': 'brim',
    'COLOURS': {'K': '#2a0c10', 'R': '#c81e2c', 'r': '#8a1020', 'L': '#ee5060', 'W': '#ffffff',
                'w': '#c8ccd8', 'v': '#9aa0b4'},
    'FRONT': [
        '..............KK....',
        '.............KWWK...',
        '..........KKKKWwK...',
        '.........KLRRKKK....',
        '........KLRRRRK.....',
        '......KKLRRRRRrK....',
        '.....KLLRRRRRRrK....',
        '....KLRRRRRRRRRrK...',
        '...KLRRRRRRRRRRRrK..',
        '..KWWWWWWWWWWWWWWWK.',
        '.KWWWwWWWWwWWWWwWWwK',
        '.KwwwvwwwwvwwwwvwwK.',
        '..KKKKKKKKKKKKKKKK..',
    ],
    'THREE_Q': [
        '.................KK..',
        '................KWWK.',
        '.............KKKKWwK.',
        '............KLRRKKK..',
        '..........KKLRRRRK...',
        '........KKLRRRRRrK...',
        '......KKLRRRRRRRrK...',
        '....KKLRRRRRRRRRrK...',
        '...KLRRRRRRRRRRRRrK..',
        '..KWWWWWWWWWWWWWWWWK.',
        '.KWWWwWWWWwWWWWwWWWwK',
        '.KwwwvwwwwvwwwwvwwwK.',
        '..KKKKKKKKKKKKKKKKK..',
    ],
}
HATS['santa_hat']['BACK'] = [r[::-1].replace('L', 'R') for r in HATS['santa_hat']['FRONT']]

HATS['straw_hat'] = {
    'NAME': '밀짚모자',
    'kind': 'brim',
    'COLOURS': {'K': '#3a2410', 'S': '#e8c070', 's': '#b88a40', 'L': '#fbe0a0', 'R': '#c02838',
                'r': '#801828', 'd': '#8a6430'},
    'FRONT': [
        '.......KKKKKK.......',
        '.....KKLLSSSSKK.....',
        '....KLLSSSSSSssK....',
        '....KSSSSSSSSssK....',
        '....KRRRRRRRRrrK....',
        '..KKKSSSSSSSSssKKK..',
        'KKLLSSSSSSSSSSSSssKK',
        'KsSSSSSSSSSSSSSSSssK',
        '.KdssssssssssssssdK.',
        '..KKKKKKKKKKKKKKKK..',
    ],
    'THREE_Q': [
        '........KKKKKK.......',
        '......KKLLSSSSKK.....',
        '.....KLLSSSSSSssK....',
        '.....KSSSSSSSSssK....',
        '.....KRRRRRRRRrrK....',
        '...KKKSSSSSSSSssKKK..',
        '.KKLLSSSSSSSSSSSSssKK',
        'KsSSSSSSSSSSSSSSSSssK',
        '.KdsssssssssssssssdK.',
        '..KKKKKKKKKKKKKKKKK..',
    ],
}
HATS['straw_hat']['BACK'] = [r[::-1].replace('L', 'S') for r in HATS['straw_hat']['FRONT']]

HATS['crown'] = {
    'NAME': '황금 왕관',
    'kind': 'crown',
    # Its bottom row sits this far below the brow line's top of head: the
    # band rests in the hair, the points stand out of it.
    'anchor': {0: 6, 1: 6, 3: 6, 4: 6, 6: 8},
    'clear': {0: 2, 1: 2, 3: 2, 4: 2, 6: 3},
    'COLOURS': {'K': '#2a1a10', 'y': '#a06a18', 'Y': '#e0a830', 'W': '#fff2a0', 'R': '#d02838',
                'r': '#ff7080', 'B': '#2860d8'},
    'FRONT': [
        '...K...K...K...',
        '..KWK.KWK.KWK..',
        '..KYK.KYK.KYK..',
        '..KYYKYRYKYYK..',
        '..KWYYYYYYYyK..',
        '..KYBYYRYYByK..',
        '..KyyyyyyyyyK..',
        '...KKKKKKKKK...',
    ],
    'THREE_Q': [
        '....K...K...K..',
        '...KWK.KWK.KWK.',
        '...KYK.KYK.KYK.',
        '...KYYKYRYKYyK.',
        '...KWYYYYYYyyK.',
        '...KYRYYBYYyyK.',
        '...KyyyyyyyyyK.',
        '....KKKKKKKKK..',
    ],
}
HATS['crown']['BACK'] = [r[::-1].replace('W', 'Y').replace('R', 'Y').replace('B', 'Y')
                         for r in HATS['crown']['FRONT']]

HATS['cat_ears'] = {
    'NAME': '고양이 귀',
    'kind': 'crown',
    'anchor': {0: 5, 1: 5, 3: 5, 4: 5, 6: 7},
    'clear': {},
    'COLOURS': {'K': '#1a1424', 'D': '#3a3046', 'F': '#5c4c6c', 'P': '#f09cb0', 'p': '#c06878'},
    'FRONT': [
        '.KK..........KK.',
        '.KFK........KFK.',
        '.KPFK......KFPK.',
        '.KPPFK....KFPPK.',
        '.KpPPFK..KFPPpK.',
        '.KDpPPK..KPPpDK.',
        '..KKKK....KKKK..',
    ],
    'THREE_Q': [
        '..KK.........KK..',
        '..KFK.......KFK..',
        '..KPFK.....KFDK..',
        '..KPPFK...KFFDK..',
        '..KpPPFK.KFFDDK..',
        '..KDpPPK.KDDDK...',
        '...KKKK...KKK....',
    ],
    'BACK': [
        '.KK..........KK.',
        '.KFK........KFK.',
        '.KDFK......KFDK.',
        '.KDDFK....KFDDK.',
        '.KDDDFK..KFDDDK.',
        '.KDDDDK..KDDDDK.',
        '..KKKK....KKKK..',
    ],
}


def solid_span(grid, row):
    xs = [x for x, ch in enumerate(grid[row]) if ch != '.']
    return min(xs), max(xs)


def head_span(im, y):
    xs = [x for x in range(im.width) if im.getpixel((x, y))[3] >= 128]
    return min(xs), max(xs)


def fit(slug, hat, frame, im):
    grid = hat[VIEWS[frame]]
    last = len(grid) - 1
    if hat['kind'] == 'brim':
        bottom = BROW[frame]
        gl, gr = solid_span(grid, last - 1)  # the brim's widest row
    else:
        bottom = hat['anchor'][frame]
        gl, gr = solid_span(grid, last)
    hl, hr = head_span(im, bottom)
    ax = round((hl + hr) / 2 - (gl + gr) / 2)
    ay = bottom - last
    spec = {'at': (ax, ay), 'grid': list(grid)}
    if hat['kind'] == 'brim':
        # Everything above the brim's top is the hat or nothing.
        brim_top = next(k for k, r in enumerate(grid) if r.count('K') + r.count('.') < len(r) and
                        solid_span(grid, k)[1] - solid_span(grid, k)[0] >= gr - gl - 2)
        spec['clear_above'] = ay + brim_top
        # The brim's shadow on the hair right under it.
        keep = A.face_mask(im) | set(A.EYES_BASE.get(frame, {}))
        y = bottom + 1
        row = ''
        for gx in range(len(grid[0])):
            x = ax + gx
            if 0 <= x < im.width and y < im.height:
                p = im.getpixel((x, y))
                col = A.hx(p) if p[3] >= 128 else None
                if col in DARKER and (x, y) not in keep and gl <= gx <= gr:
                    row += shadow_letter(hat, DARKER[col])
                    continue
            row += '.'
        spec['grid'].append(row)
    elif hat['clear'].get(frame) is not None:
        spec['clear_above'] = hat['clear'][frame]
    return spec


def shadow_letter(hat, colour):
    cols = hat['COLOURS']
    for k, v in cols.items():
        if v == colour:
            return k
    for k in 'abcdefghijlmnoqtuxz0123456789':
        if k not in cols:
            cols[k] = colour
            return k
    raise ValueError('out of letters')


# The finishing pass (costumes/finish.py) over each fitted frame — the game's
# tinted outlines, form shading — and a material where a hat has one: felt
# hangs in folds, straw and fur bristle. `--raw` turns it off.
FINISH = True
MATERIAL = {
    'witch_hat': {'cloth': 'MP'},
    'santa_hat': {'cloth': 'R', 'fur': 'Ww'},
    'straw_hat': {'fur': 'Ss'},
}


def finished(slug, hat, frames, ims):
    """The fitted frames, finished: {frame: spec} and the new COLOURS."""
    import finish
    cols = dict(hat['COLOURS'])
    shadow = set(DARKER.values())
    drawn = {v for v in cols.values() if v not in shadow}
    mat = MATERIAL.get(slug, {})
    canvases = {}
    for f, spec in frames.items():
        c = A.Canvas()
        ax, ay = spec['at']
        top = spec.get('clear_above', -999)
        for gy, row in enumerate(spec['grid']):
            for gx, ch in enumerate(row):
                x, y = ax + gx, ay + gy
                if ch == '.':
                    c.put(x, y, None if y < top else '.')
                elif ch == '-':
                    c.put(x, y, None)
                else:
                    c.put(x, y, cols[ch])
        finish.finish_canvas(c, ims[f], drawn, shadow,
                             cloth={cols[k] for k in mat.get('cloth', '') if k in cols},
                             fur={cols[k] for k in mat.get('fur', '') if k in cols})
        canvases[f] = c
    letters = finish.assign_letters(list(canvases.values()))
    out = {}
    for f, spec in frames.items():
        ax, ay = spec['at']
        grid = []
        for gy, row in enumerate(spec['grid']):
            line = ''
            for gx in range(len(row)):
                v = canvases[f].p.get((ax + gx, ay + gy), '.')
                line += '.' if v == '.' else '-' if v is None else letters[v]
            grid.append(line)
        out[f] = dict(spec, grid=grid)
    return out, {k: v for v, k in letters.items()}


def build(write=True):
    ims = A.frames('c/sp/img0/000.mgr')
    out = {}
    for slug, hat in HATS.items():
        frames = {f: fit(slug, hat, f, ims[f]) for f in VIEWS}
        colours = hat['COLOURS']
        if FINISH:
            frames, colours = finished(slug, hat, frames, ims)
        out[slug] = (frames, colours)
        if write:
            text = ('# Generated by costumes/hats.py — edit the hat there.\n'
                    'NAME = %r\n' % hat['NAME']
                    + 'COLOURS = ' + pprint.pformat(colours, width=110, sort_dicts=False) + '\n'
                    + 'FRAMES = ' + pprint.pformat(frames, width=110, sort_dicts=False) + '\n')
            (HERE / 'sets' / (slug + '.py')).write_text(text, encoding='utf-8')
    return out


def preview(path, sc=6):
    """The hero standing, front / 3-4 / back, bare and in each hat."""
    import mgr
    import atelier
    from PIL import Image, ImageDraw
    fits = build(write=False)
    base = {layer: A.frames('c/sp/img%d/000.mgr' % layer) for layer in range(4)}
    base[4] = A.frames('c/sp/img4/000.mgr')
    cif = mgr.cif_decode(mgr.load_source('c/sp/cif/001.cif')[1], True)
    looks = [('bare', base[0])]
    for slug, hat in HATS.items():
        frames, colours = fits[slug]
        cols = {k: atelier.hexrgb(v) for k, v in colours.items()}
        looks.append((slug, [atelier.compose(b, frames[i], cols) if i in frames else b
                             for i, b in enumerate(base[0])]))
    box = (12, 0, 52, 50)
    cw, ch = (box[2] - box[0]) * sc, (box[3] - box[1]) * sc
    page = Image.new('RGBA', (len(looks) * (3 * cw + 10), ch + 16), (52, 58, 72, 255))
    d = ImageDraw.Draw(page)
    for k, (name, heads) in enumerate(looks):
        sheets = dict(base)
        sheets[0] = heads
        for j, want in enumerate((1, 2, 0)):
            cell = Image.new('RGBA', (64, 64), (0, 0, 0, 0))
            dirs = cif.anims[0]
            di = min(want, len(dirs) - 1)
            for p in dirs[di][0]:
                if p.layer not in sheets or p.effect:
                    continue
                im = sheets[p.layer][p.index]
                if p.flip:
                    im = im.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
                cell.alpha_composite(im, (32 + p.x - im.width // 2, 46 + p.y - im.height // 2))
            page.alpha_composite(cell.crop(box).resize((cw, ch), Image.NEAREST), (k * (3 * cw + 10) + j * cw, 14))
        d.text((k * (3 * cw + 10) + 3, 1), name, fill=(255, 255, 0))
    page.save(path)
    print('saved', path, page.size)


if __name__ == '__main__':
    if '--raw' in sys.argv:
        sys.argv.remove('--raw')
        FINISH = False
    if len(sys.argv) > 1 and sys.argv[1] == 'preview':
        preview(sys.argv[2])
    else:
        build()
        print('wrote', ', '.join('sets/%s.py' % s for s in HATS))
