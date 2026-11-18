#!/usr/bin/env python3
"""렌 (the spirit) avatars: her own two sheets, re-dressed.

    python costumes/ren_studio.py labels out/ren_labels.png    # what each base pixel is
    python costumes/ren_studio.py zoom 3 out/ren03.png          # one outfit, large, with the reference
    python costumes/ren_studio.py sheet out/ren_all.png         # every outfit, idle in three directions
    python costumes/ren_studio.py build                         # all -> sets/, catalog, docs/ren_avatar/native
    python costumes/ren_studio.py build 3                       # one outfit

렌 is drawn from `img0/042` (head, 6 images) and `img1/025` (46 images: arms,
torso, skirt-and-legs, two light-tails, one summon effect) by `cif/065`. An
outfit is a pair of costume sheets, head and body, under one number; the game
loads them in place of hers (hl5_core::spirit_avatar).

Designs live in costumes/ren/rNN.py. As with the Astra studio, none of the
game's pixels is stored here: a design is ramps over a per-pixel *label* of
her art (computed here from the archive), hand grids for what she did not
have (a hat, a ponytail, a cape), and a few rules for long skirts. The
composite is written as overlays (sets/ren_NN_L.py) that hl5_core::costume
replays on the player's own archive.

What a design names (all optional except NAME):

  NAME        the outfit's name in the wardrobe (EUC-KR-encodable, short)
  REF         the reference image under docs/ren_avatar/references
  PAL         letter -> '#rrggbb' for every grid
  HAIR        ramp dark -> light for her hair (8 base levels map onto it)
  ORNAMENT    'hair' (the side flowers become hair), 'keep', or a ramp
  EYES        ramp for her eyes (2 levels), or None to keep them
  HEAD        {frame: [grid, ...]} drawn over the recoloured head
  TORSO       {33|34|35: grid} her torso, replaced whole
  ARM         {'sleeve', 'trim', 'jewel', 'glove', 'cuff'}: ramp or 'skin'
  LEGS        {'skirt', 'panel', 'trim', 'inner', 'leg', 'foot'}: ramp or 'skin'
  DRESS       a long skirt drawn over every lower-body frame (see dress())
  WINGS       ramp for her two light-tails, 'hide', or {29|30: grid}
  BODY        {frame: [grid, ...]} drawn last on any body frame

A grid is {'at': (x, y), 'rows': [...]} in the base frame's own pixels:
' ' leaves the pixel, '.' keeps the base pixel, '-' clears, a letter is PAL.
"""
import importlib
import json
import pprint
import sys
from collections import deque
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools_re'))
sys.path.insert(0, str(ROOT / 'costumes'))
import mgr  # noqa: E402
from astra_studio import Canvas, hx, letters_for, lum, q565, ramp_map  # noqa: E402
import finish  # noqa: E402

HEAD_SRC = 'c/sp/img0/042.mgr'
BODY_SRC = 'c/sp/img1/025.mgr'
CIF_SRC = 'c/sp/cif/065.cif'
OUT = ROOT / 'docs/ren_avatar/native'
REFS = ROOT / 'docs/ren_avatar/references'
COUNT = 10
SHEET0 = 320          # outfit n is served at img0/(320+n) and img1/(320+n)
ID0 = 130000          # outfit n is ids 130000 + n*10 + layer


def frames(src):
    _, d = mgr.load_source(src)
    return [mgr.gbm_to_png_image(g, g.palette).convert('RGBA') for g in mgr.mgr_decode(d)]


def solid(im):
    return {(x, y): hx(im.getpixel((x, y))) for y in range(im.height) for x in range(im.width)
            if im.getpixel((x, y))[3] >= 128}


N4 = ((1, 0), (-1, 0), (0, 1), (0, -1))
N8 = N4 + ((1, 1), (-1, -1), (1, -1), (-1, 1))


def components(pts, n=N4):
    left = set(pts)
    out = []
    while left:
        s = left.pop()
        comp = {s}
        dq = deque([s])
        while dq:
            x, y = dq.popleft()
            for dx, dy in n:
                q = (x + dx, y + dy)
                if q in left:
                    left.remove(q)
                    comp.add(q)
                    dq.append(q)
        out.append(comp)
    return out


# ------------------------------------------------------------------ the head
# img0/042: what each colour of her head is.
HEAD_HAIR = ['#424952', '#4a596b', '#6b7d9c', '#8ca2ad', '#b5cfd6', '#deeff7', '#ffffde', '#ffffe7']
HEAD_SKIN = ['#422829', '#5a3031', '#633031', '#733839', '#9c5d5a', '#a56563', '#c67d63', '#c6825a',
             '#e79a84', '#e79a8c', '#efb6a5', '#ffe7de']
HEAD_GOLD = ['#efc373']
HEAD_RED = ['#e72021', '#f72421']
# Her eyes are the two darkest skin tones inside the face.
EYE = ['#422829', '#5a3031', '#633031']


def head_labels(im):
    """hair / face / ear / eye / orn per pixel. The side flowers are skin-
    coloured petals around a gold and a red pixel, above the face; the face
    and the pointed ears are the skin components that reach the lower half."""
    px = solid(im)
    lab = {}
    skin = [p for p, c in px.items() if c in HEAD_SKIN or c in HEAD_GOLD or c in HEAD_RED]
    h = im.height
    for comp in components(skin, N8):
        cols = {px[p] for p in comp}
        top = min(y for _, y in comp)
        bottom = max(y for _, y in comp)
        has_orn = bool(cols & set(HEAD_GOLD + HEAD_RED))
        if has_orn and bottom < h * 0.62:
            kind = 'orn'
        elif len(comp) <= 6 and top >= h * 0.45 and any(x <= 2 or x >= im.width - 3 for x, _ in comp):
            kind = 'ear'
        else:
            kind = 'face'
        for p in comp:
            lab[p] = kind
    for p, c in px.items():
        if p in lab:
            if lab[p] == 'face' and c in EYE:
                lab[p] = 'eye' if c != '#422829' else 'face'
            continue
        lab[p] = 'hair'
    return lab


# ------------------------------------------------------------------ the body
ARMS = [0, 1, 6, 8, 31, 32]
TORSOS = [33, 34, 35]
LOWERS = [13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 25, 26, 27, 28, 37, 38, 39, 40, 41, 42, 43, 44, 45]
WINGS = [29, 30]
# The lower-body frames seen from behind (a slit or an opening is in front).
BACK_LOWERS = {20, 21, 22, 26, 39, 42, 45}

WARM = ['#6b3c39', '#733839', '#7b494a', '#945d5a', '#9c5d5a', '#a56563', '#a5695a', '#a57173', '#ad7173',
        '#bd796b', '#c6825a', '#c68663', '#ce968c', '#e79a84', '#e79a8c', '#e7aaa5', '#efb6a5', '#ffdbce',
        '#ffe3d6', '#ffe7de']
PURPLE = ['#52557b', '#5a5984', '#5a5d84', '#5a5d8c', '#6b6d9c', '#8482ce', '#bdaaff', '#decfff', '#4a7db5',
          '#6b7d9c', '#4a596b']
GOLD = ['#de8a29', '#efc373']
RED = ['#de2021', '#e72021']
DARK = ['#211c18', '#292821', '#393431', '#524d42', '#524d4a']
LIGHT = ['#73656b', '#736963', '#847d7b', '#9c8e84', '#9c928c', '#9c968c', '#9c9694', '#b5b2a5', '#b5b2ad',
         '#bdb2ad', '#dedbd6', '#dedbde', '#e7dfce', '#ffffef']


def arm_labels(im):
    """sleeve (the puffed shoulder), trim (its gold), jewel (red), glove (the
    long white glove), cuff (the glove's purple band)."""
    px = solid(im)
    lab = {}
    for p, c in px.items():
        if c in RED:
            lab[p] = 'jewel'
        elif c in GOLD:
            lab[p] = 'trim'
        elif c in WARM:
            lab[p] = 'sleeve'
        elif c in PURPLE:
            lab[p] = 'cuff'
        else:
            lab[p] = 'glove'
    # The hand: the glove's last two rows (the piece hangs from the shoulder;
    # a raised arm is the same image turned by the cif).
    gl = [p for p, l in lab.items() if l == 'glove']
    if gl:
        bottom = max(y for _, y in gl)
        for p in gl:
            if p[1] >= bottom - 1:
                lab[p] = 'hand'
    return lab


def lower_labels(im):
    """skirt / panel / trim / inner (the pale front panel) above the hem;
    leg / foot below. The hem is per column: the lowest skirt-coloured pixel,
    carried down through the pale panel."""
    px = solid(im)
    w, h = im.size
    cloth = {p for p, c in px.items() if c in WARM or c in PURPLE or c in GOLD or c in RED}
    pale = {p for p, c in px.items() if c in LIGHT and lum(c) >= 150}
    # Her boots carry the skirt's maroon as a highlight and their knees its
    # pale grey, so the skirt ends at the last row with purple or gold —
    # colours the legs never use.
    marks = [p[1] for p in cloth if px[p] in PURPLE or px[p] in GOLD]
    limit = max(marks) if marks else h // 2
    # The pale panel is skirt where it touches cloth, above that row.
    skirt = {p for p in cloth if p[1] <= limit}
    grow = deque(p for p in pale if any((p[0] + dx, p[1] + dy) in skirt for dx, dy in N4))
    while grow:
        p = grow.popleft()
        if p in skirt or p[1] > limit:
            continue
        skirt.add(p)
        for dx, dy in N4:
            q = (p[0] + dx, p[1] + dy)
            if q in pale and q not in skirt:
                grow.append(q)
    hem = {}
    for x in range(w):
        rows = [y for (xx, y) in skirt if xx == x]
        hem[x] = max(rows) if rows else -1
    lab = {}
    for p, c in px.items():
        x, y = p
        if y <= hem[x]:
            if c in GOLD:
                lab[p] = 'trim'
            elif c in RED:
                lab[p] = 'trim'
            elif c in PURPLE:
                lab[p] = 'panel'
            elif c in WARM:
                lab[p] = 'skirt'
            elif p in skirt:
                lab[p] = 'inner'
            else:
                lab[p] = 'shade'
        else:
            lab[p] = 'leg'
    # Feet: the bottom three rows of every leg component.
    legs = [p for p, l in lab.items() if l == 'leg']
    for comp in components(legs, N8):
        bottom = max(y for _, y in comp)
        for p in comp:
            if p[1] >= bottom - 2:
                lab[p] = 'foot'
    return lab


def wing_labels(im):
    px = solid(im)
    return {p: ('tip' if c in PURPLE + ['#84a6b5', '#add3e7', '#d6f3ff'] else 'wing') for p, c in px.items()}


def body_labels(i, im):
    if i in ARMS:
        return arm_labels(im)
    if i in LOWERS:
        return lower_labels(im)
    if i in WINGS:
        return wing_labels(im)
    return None


# ------------------------------------------------------------------ labels view
LABEL_COLOURS = {
    'hair': (120, 170, 255), 'face': (255, 210, 190), 'ear': (255, 120, 120), 'eye': (200, 0, 60),
    'orn': (255, 220, 0), 'sleeve': (230, 110, 90), 'trim': (255, 200, 0), 'jewel': (255, 0, 0),
    'glove': (240, 240, 240), 'cuff': (150, 120, 255), 'hand': (255, 170, 140), 'skirt': (190, 60, 70), 'panel': (110, 90, 230),
    'inner': (250, 250, 210), 'shade': (90, 40, 50), 'leg': (60, 60, 60), 'foot': (20, 120, 60),
    'wing': (220, 240, 255), 'tip': (80, 160, 255),
}


def labels_view(out, sc=8):
    tiles = []
    for name, src, idx, fn in (('H', HEAD_SRC, range(6), lambda i, im: head_labels(im)),
                               ('B', BODY_SRC, ARMS + LOWERS + WINGS, body_labels)):
        ims = frames(src)
        for i in idx:
            im = ims[i]
            lab = fn(i, im)
            t = Image.new('RGBA', (im.width * sc * 2 + 6, im.height * sc + 14), (40, 44, 56, 255))
            t.alpha_composite(im.resize((im.width * sc, im.height * sc), Image.NEAREST), (0, 14))
            d = ImageDraw.Draw(t)
            for (x, y), l in lab.items():
                ox = im.width * sc + 6
                d.rectangle([ox + x * sc, 14 + y * sc, ox + x * sc + sc - 1, 14 + y * sc + sc - 1], fill=LABEL_COLOURS[l])
            d.text((2, 1), '%s%d' % (name, i), fill=(255, 255, 0))
            tiles.append(t)
    page_rows, row, x = [], [], 0
    for t in tiles:
        if x + t.width > 2400 and row:
            page_rows.append(row)
            row, x = [], 0
        row.append(t)
        x += t.width + 8
    page_rows.append(row)
    H = sum(max(t.height for t in r) + 8 for r in page_rows)
    page = Image.new('RGBA', (2400, H), (24, 26, 34, 255))
    y = 0
    for r in page_rows:
        x = 0
        for t in r:
            page.alpha_composite(t, (x, y))
            x += t.width + 8
        y += max(t.height for t in r) + 8
    key = ImageDraw.Draw(page)
    page.save(out)
    print('saved', out, page.size)


# ------------------------------------------------------------------ composition
# Her own skin, for bare arms and legs (dark -> light): the face's tones.
SKIN = ['#7a3c3a', '#c8806a', '#e09c86', '#eeb6a2', '#f8cebe', '#ffe2d8', '#fff2ec']

_REGION_COLOURS = None


def region_colours():
    """Every base colour each body label takes, over all her frames, so one
    region is ramped one way everywhere (a colour shared by the skirt of one
    frame and of another maps the same)."""
    global _REGION_COLOURS
    if _REGION_COLOURS is None:
        out = {}
        for i, im in enumerate(frames(BODY_SRC)):
            lab = body_labels(i, im)
            if not lab:
                continue
            kind = 'arm' if i in ARMS else 'lower' if i in LOWERS else 'wing'
            for p, l in lab.items():
                out.setdefault((kind, l), set()).add(hx(im.getpixel(p)))
                if kind == 'wing':
                    out.setdefault((kind, 'all'), set()).add(hx(im.getpixel(p)))
        _REGION_COLOURS = {k: sorted(v, key=lum) for k, v in out.items()}
    return _REGION_COLOURS


def ramp_for(spec):
    return SKIN if spec == 'skin' else spec


def load_design(n):
    mod = importlib.import_module('ren.r%02d' % n)
    importlib.reload(mod)
    return {k: getattr(mod, k) for k in dir(mod) if k.isupper()}


def blit(c, grids, pal):
    for g in grids or []:
        c.blit_grid(g['at'], g['rows'], pal)


def head_canvas(design, i, im):
    c = Canvas()
    lab = head_labels(im)
    hair = design.get('HAIR')
    hair_map = ramp_map(HEAD_HAIR, hair) if hair else {}
    orn = design.get('ORNAMENT', 'keep')
    px = solid(im)
    for (x, y), l in lab.items():
        col = px[(x, y)]
        if l == 'hair':
            c.put(x, y, hair_map.get(col, '.'))
        elif l == 'orn':
            if orn == 'keep':
                c.put(x, y, '.')
            elif orn == 'hair':
                # The nearest hair pixel's colour, so the lock runs on.
                best = min((p for p, ll in lab.items() if ll == 'hair'),
                           key=lambda p: (p[0] - x) ** 2 + (p[1] - y) ** 2)
                c.put(x, y, hair_map.get(px[best], px[best]))
            else:
                cols = sorted({px[p] for p, ll in lab.items() if ll == 'orn'}, key=lum)
                c.put(x, y, ramp_map(cols, orn)[col])
        else:
            c.put(x, y, '.')
    # A hat of the game's own (costumes/donor.py), WORN: see hat_fit.
    sh = design.get('HAT_SHADOW', {}).get(i)
    if design.get('DONOR_HAT'):
        import donor
        brow, cx, top = hat_fit(lab, i)
        clearable = [p for p, l in lab.items() if l in ('hair', 'orn')]
        laid = donor.fit_hat(c, im, design['DONOR_HAT'], i, brow, cx, clearable, head_top=top)
        under = [x for x, y in laid if y == brow]
        sh = (brow + 1, min(under), max(under)) if under else None
    # A worn hat's shadow: the hair in the row right under its brim, one step
    # darker along her hair ramp.
    if sh and hair:
        row, x0, x1 = sh
        for x in range(x0, x1 + 1):
            if lab.get((x, row)) in ('hair', 'orn'):
                v = c.p.get((x, row))
                if v in hair:
                    c.put(x, row, hair[max(0, hair.index(v) - 1)])
    blit(c, design.get('HEAD', {}).get(i), design.get('PAL', {}))
    return c


# Where a hat sits on her (2026-10-15, §64). The game's hats come down over
# the forehead to the lash line — on the hero the brim's lowest row is the
# first row under the skin's top (skin 10, brim 11) — so hers is her face's
# first row + 1; and her skull is shorter than his, so fit_hat shortens the
# crown by the ratio. The back view has no face: it takes the front's row.
def hat_fit(lab, view):
    """(brim row, brim centre, skull top) for a worn hat in her head view."""
    face = [y for (x, y), l in lab.items() if l in ('face', 'eye')]
    if face:
        brow = min(face) + 1
    else:
        brow = REN_BACK_BROW
    rows = {}
    for (x, y), l in lab.items():
        if l in ('hair', 'orn', 'face', 'eye'):
            rows.setdefault(y, []).append(x)
    # the skull's top: the first row that is head, not the ahoge's wisp
    top = min(y for y, xs in rows.items() if len(xs) >= 5)
    xs = rows.get(brow) or rows[max(rows)]
    return brow, (min(xs) + max(xs)) / 2.0, top


REN_BACK_BROW = 9


def recolour_regions(c, im, lab, kind, spec):
    rc = region_colours()
    px = solid(im)
    maps = {}
    for region in set(lab.values()):
        want = spec.get(region)
        if want is None and region == 'foot':
            want = spec.get('leg')
        if want is None and region == 'hand':
            want = spec.get('glove')
        if want is None and region == 'shade' and spec.get('skirt'):
            want = ramp_for(spec['skirt'])[:2]
        if want is None or want == 'keep':
            continue
        if want == 'clear':
            maps[region] = None
            continue
        maps[region] = ramp_map(rc[(kind, region)], ramp_for(want))
    for p, l in lab.items():
        if l in maps:
            m = maps[l]
            c.put(p[0], p[1], None if m is None else m[px[p]])
        else:
            c.put(p[0], p[1], '.')


def body_canvas(design, i, im):
    c = Canvas()
    lab = body_labels(i, im)
    pal = design.get('PAL', {})
    if i in TORSOS and i in design.get('TORSO', {}):
        blit(c, [design['TORSO'][i]], pal)
    elif i in ARMS:
        recolour_regions(c, im, lab, 'arm', design.get('ARM', {}))
    elif i in LOWERS:
        recolour_regions(c, im, lab, 'lower', design.get('LEGS', {}))
        if design.get('CUT'):
            cut_skirt(c, im, lab, design['CUT'], design.get('LEGS', {}))
        if design.get('DRESS'):
            dress(c, im, lab, design['DRESS'], i)
        if design.get('ANKLET'):
            anklet(c, lab, design['ANKLET'])
    elif i in WINGS:
        w = design.get('WINGS')
        if w == 'hide':
            for p in lab:
                c.put(p[0], p[1], None)
        elif isinstance(w, dict):
            blit(c, [w[i]] if i in w else [], pal)
        elif w:
            recolour_regions(c, im, {p: 'all' for p in lab}, 'wing', {'all': w})
            # (both labels share one ramp: the tail is one piece of light)
        else:
            for p in lab:
                c.put(p[0], p[1], '.')
    else:
        for (x, y) in solid(im):
            c.put(x, y, '.')
    blit(c, design.get('BODY', {}).get(i), pal)
    return c


# ------------------------------------------------------------------ short cuts
def by_lum(col, ramp, lo, hi):
    """A ramp colour for `col` by where its luminance sits in [lo, hi]."""
    t = 0.0 if hi <= lo else (lum(col) - lo) / (hi - lo)
    return ramp[max(0, min(len(ramp) - 1, round(t * (len(ramp) - 1))))]


def cut_skirt(c, im, lab, spec, legs):
    """Shorten her skirt into something that hugs the hips (a leotard, hot
    pants): skirt pixels below a cut line become bare leg, shaded by their own
    luminance on the leg ramp. The line is `base` rows below the skirt's top
    at the sides and `base + depth` at the centre (a high cut is a large
    depth)."""
    px = solid(im)
    skirt = [p for p, l in lab.items() if l in ('skirt', 'panel', 'trim', 'inner', 'shade')]
    if not skirt:
        return
    top = min(y for _, y in skirt)
    xs = [x for x, _ in skirt]
    cx = (min(xs) + max(xs)) / 2
    half = max(1.0, (max(xs) - min(xs)) / 2)
    ramp = ramp_for(legs.get('leg', 'skin'))
    lums = [lum(px[p]) for p in skirt]
    lo, hi = min(lums), max(lums)
    for (x, y) in skirt:
        line = top + spec.get('base', 2) + spec.get('depth', 3) * max(0.0, 1 - abs(x - cx) / half)
        if y > line:
            c.put(x, y, by_lum(px[(x, y)], ramp[1:], lo, hi))


def anklet(c, lab, cols):
    """A band round each ankle: the top row of every foot, in `cols`
    (lit, shaded, alternating so it reads as a frill)."""
    feet = [p for p, l in lab.items() if l == 'foot']
    for comp in components(feet, N8):
        top = min(y for _, y in comp)
        for (x, y) in comp:
            if y == top:
                c.put(x, y, cols[x % len(cols)])


# ------------------------------------------------------------------ long skirts
def drape(rows, top, hem, waist, hemspan, ramp, outline, spec):
    """The cloth of a long skirt, as a pixel artist drapes it.

    Folds are pairs — a lit ridge and, on its far side from the light, a
    shaded groove that deepens toward the hem — and they fan out: close
    together under the waist, spread evenly across the hem. The plane between
    them turns from the lit left to the shaded right and darkens a step under
    the waist (the bodice's shadow) and just above the hem. The left edge
    catches a rim of light, the right edge sinks. The hem is not ruled: under
    each groove it rises a pixel, so the bottom edge waves with the folds.
    Returns {(x, y): colour}."""
    n = len(ramp)
    wl, wr = waist
    hl, hr = hemspan
    width_h = max(1, hr - hl)
    count = spec.get('fold_count') or max(2, min(5, round(width_h / 4)))
    if spec.get('folds') is False:
        count = 0
    cxw = (wl + wr) / 2
    fold_w = [cxw + (k + 0.5 - count / 2) * max(1.0, (wr - wl) * 0.5) / max(1, count) for k in range(count)]
    fold_h = [hl + 1 + (k + 0.5) * (width_h - 2) / max(1, count) for k in range(count)]
    light = spec.get('light', 0.95)
    slope = spec.get('slope', 0.4)
    body = {}
    grooves = set()
    for y, (l, r) in rows.items():
        t = (y - top) / max(1, hem - top)
        e = t ** 0.8
        axes = [fw + (fh - fw) * e for fw, fh in zip(fold_w, fold_h)]
        width = max(1, r - l)
        for x in range(l, r + 1):
            u = (x - l) / width
            tone = light - slope * u - 0.10 * t
            if t < 0.10:
                tone -= 0.14
            if t > 0.18:
                for a in axes:
                    d = x - a
                    if -0.5 <= d < 0.5:
                        tone += 0.10 + 0.06 * t          # the ridge
                    elif 0.5 <= d < 1.5:
                        tone -= 0.16 + 0.16 * t          # its groove
                        if y >= hem - 1:
                            grooves.add(x)
            if x == l + 1 and t > 0.08:
                tone += 0.12
            if x == r - 1:
                tone -= 0.10
            if y >= hem - 1:
                tone -= 0.10
            idx = max(1, min(n - 1, round(tone * (n - 1))))
            body[(x, y)] = ramp[idx]
            if x in (l, r):
                body[(x, y)] = outline
    # The waving hem: under a groove the cloth ends a row higher.
    l, r = rows[hem]
    for x in range(l + 1, r):
        if x in grooves and (x, hem - 1) in body:
            del body[(x, hem)]
            body[(x, hem - 1)] = outline
        else:
            body[(x, hem)] = outline
    return body


def dress(c, im, lab, spec, i):
    """A floor-length skirt over one lower-body frame, shaped by that frame's
    own legs so it moves with them: it hangs from her waist (the frame's top
    rows) to just above her lowest foot, widening from the waist's span
    toward the span of everything below, and is shaded the way her own skirt
    is — light from the upper left, a dark outline, folds that fan out toward
    the hem — with optional ruffle tiers, a hem band and a slit.

    spec keys: 'ramp' (dark -> light), 'outline', 'show' (rows of the lowest
    foot left visible), 'flare' (px each side past what is below), 'bell'
    (curve of the widening), 'max_half' (cap on the hem's half width),
    'tiers' [(fraction down, ramp dark -> light)], 'hem' (ramp of the hem
    band, top row first), 'slit' ('left'/'right', which leg shows), 'slit_at'
    (fraction down where it opens), 'lining' (ramp inside the slit),
    'folds' (False for a smooth skirt), 'open' (a front opening this many px
    wide at the hem: a coat or cape that shows what is under it).
    """
    pts = solid(im)
    if not pts:
        return
    # What the leg ramps made of each pixel, for the slit and the opening.
    under = dict(c.p)
    px0 = pts
    feet = [p for p, l in lab.items() if l == 'foot']
    bottom = max(y for _, y in pts)
    show = spec.get('show', 2)
    hem = (max(y for _, y in feet) if feet else bottom) - show
    top = min(y for _, y in pts)
    # A shorter skirt ends part of the way down (`length` 1.0 = the floor).
    hem = top + round((hem - top) * spec.get('length', 1.0))
    if hem - top < 4:
        return
    waist = [x for (x, y) in pts if y <= top + 1]
    wl, wr = min(waist), max(waist)
    low = [x for (x, y) in pts if y >= hem - 3]
    flare = spec.get('flare', 1)
    hl, hr = min(low) - flare, max(low) + flare
    mh = spec.get('max_half', 9)
    if (hr - hl) / 2 > mh:
        mid = (hl + hr) / 2
        hl, hr = round(mid - mh), round(mid + mh)
    ramp = spec['ramp']
    n = len(ramp)
    outline = spec.get('outline', ramp[0])
    rows = {}
    for y in range(top, hem + 1):
        t = (y - top) / (hem - top)
        e = t ** spec.get('bell', 1.5)
        rows[y] = (round(wl + (hl - wl) * e), round(wr + (hr - wr) * e))
    cxw = (wl + wr) / 2
    cxh = (hl + hr) / 2
    body = drape(rows, top, hem, (wl, wr), (hl, hr), ramp, outline, spec)
    # Ruffle tiers: a lit top row, a body row, a scalloped dark edge, each a
    # pixel wider than the skirt so they stand off it.
    for frac, tr in spec.get('tiers', []):
        ty = top + round(frac * (hem - top))
        for k, dy in enumerate((0, 1, 2)):
            y = ty + dy
            if y not in rows:
                continue
            l, r = rows[y]
            for x in range(l - 1, r + 2):
                u = (x - l) / max(1, r - l)
                if dy == 0:
                    col = tr[-1] if u < 0.6 else tr[len(tr) // 2]
                elif dy == 1:
                    col = tr[len(tr) // 2] if u < 0.6 else tr[1]
                else:
                    if (x + y) % 2:
                        continue
                    col = tr[0]
                if x in (l - 1, r + 1) and dy < 2:
                    col = tr[0]
                body[(x, y)] = col
    if spec.get('stars'):
        # A scatter of small stars: fixed by position, so a frame and the
        # next agree where the skirt has them; never on the outline.
        for (x, y), col in list(body.items()):
            l, r = rows.get(y, (x, x))
            t = (y - top) / (hem - top)
            if l < x < r and t > 0.25 and (x * 7 + y * 11) % 19 == 0:
                body[(x, y)] = spec['stars']
    # The hem band and the scallop follow the waving hem, column by column.
    bottoms = {}
    for (x, y) in body:
        if y <= hem:
            bottoms[x] = max(bottoms.get(x, y), y)
    hl_, hr_ = rows[hem]
    band = spec.get('hem', [])
    for x, by in bottoms.items():
        if x in (hl_, hr_):
            continue
        for k, col in enumerate(band):
            y = by - len(band) + 1 + k
            if y >= top and (x, y) in body:
                body[(x, y)] = col
    if spec.get('hem_scallop'):
        for x, by in bottoms.items():
            if (x + by) % 2 == 1:
                body[(x, by + 1)] = spec['hem_scallop']
    for (x, y), col in body.items():
        c.put(x, y, col)
    # What stood out past the skirt above the hem goes; legs and feet below
    # it stay as the leg ramps made them.
    for (x, y) in pts:
        if top <= y <= hem and (x, y) not in body:
            c.put(x, y, None)
    opening = spec.get('open')
    if opening and i not in BACK_LOWERS:
        # A coat or cape left open down the front: from the waist the gap
        # widens to `opening` px at the hem and shows her legs and the inner
        # skirt; its edges are the trim colour.
        trim = spec.get('open_trim', outline)
        for y, (l, r) in rows.items():
            t = (y - top) / (hem - top)
            half = opening / 2 * t ** 0.8
            cx = cxw + (cxh - cxw) * t
            x0, x1 = round(cx - half), round(cx + half)
            for x in range(x0, x1 + 1):
                if (x, y) in pts:
                    if lab.get((x, y)) in ('skirt', 'panel', 'shade', 'inner', 'trim') and spec.get('open_fill'):
                        # the cloth seen through the opening, shaded by its own light
                        fill = spec['open_fill']
                        c.put(x, y, by_lum(px0[(x, y)], fill, 40, 220) if isinstance(fill, list) else fill)
                    else:
                        c.put(x, y, under.get((x, y), '.'))
                else:
                    c.put(x, y, None)
            if x1 - x0 >= 1:
                c.put(x0 - 1, y, trim)
                c.put(x1 + 1, y, trim)
    slit = spec.get('slit')
    if slit and i not in BACK_LOWERS:
        lining = spec.get('lining', ramp)
        legs = [p for p, l in lab.items() if l in ('leg', 'foot')]
        if legs:
            lx = sorted(x for x, _ in legs)
            side = lx[:len(lx) // 2] if slit == 'left' else lx[len(lx) // 2:]
            x0 = sum(side) / len(side)
            start = top + round(spec.get('slit_at', 0.45) * (hem - top))
            for y in range(start, hem + 1):
                half = (y - start) * spec.get('slit_open', 0.45)
                a, b = round(x0 - half), round(x0 + half)
                for x in range(a, b + 1):
                    if lab.get((x, y)) in ('leg', 'foot'):
                        c.put(x, y, under.get((x, y), '.'))
                    elif (x, y) in body:
                        c.put(x, y, lining[len(lining) // 2])
                # the cut edge toward the light catches it
                edge = b + 1 if slit == 'left' else a - 1
                if (edge, y) in body and body[(edge, y)] != outline:
                    c.put(edge, y, lining[-1])


# ------------------------------------------------------------------ sheets
# The finishing pass (costumes/finish.py) over what we drew: the game's
# coloured outlines, lifted blacks, form shading. `--raw` turns it off for a
# before/after look.
FINISH = True


def drawn_colours(design):
    """The colours that are ours: every grid colour and every colour a
    painted skirt uses. The ramps that recolour her own pixels (HAIR, ARM,
    LEGS, a WINGS ramp) are not — those keep the game's shading."""
    out = set(design.get('PAL', {}).values())
    d = design.get('DRESS') or {}
    for k in ('ramp', 'hem', 'lining', 'open_fill'):
        v = d.get(k)
        if isinstance(v, list):
            out.update(v)
    for k in ('outline', 'stars', 'hem_scallop', 'open_trim'):
        if isinstance(d.get(k), str):
            out.add(d[k])
    return out


def hair_colours(design):
    """Hair drawn with `ren/kit.py` keeps its strand shading, and so does
    anything a design lists in NOFORM (a suit already shaded as a ball,
    feathers)."""
    pal = design.get('PAL', {})
    return {pal[k] for k in 'ObyYL' + design.get('NOFORM', '') if k in pal}


def compose_layer(design, layer):
    src = HEAD_SRC if layer == 0 else BODY_SRC
    ims = frames(src)
    canvases = {}
    colours = {}
    for i, im in enumerate(ims):
        if im.size == (1, 1):
            continue
        if layer == 0:
            c = head_canvas(design, i, im)
        else:
            if i == 36:
                continue  # her summon's dragon, not her
            c = body_canvas(design, i, im)
        if all(v == '.' for v in c.p.values()):
            continue
        if FINISH and design.get('FINISH', True):
            pal = design.get('PAL', {})
            finish.finish_canvas(c, im, drawn_colours(design), hair_colours(design),
                                 cloth={pal[k] for k in design.get('FOLDS', '') if k in pal},
                                 fur={pal[k] for k in design.get('PELT', '') if k in pal})
        canvases[i] = c
        for v in c.p.values():
            if v and v != '.':
                colours[v] = True
    letters = finish.assign_letters(list(canvases.values()))
    specs = {i: c.spec(ims[i].width, ims[i].height, letters) for i, c in canvases.items()}
    source = {'NAME': design['NAME'], 'BASE': src, 'CLASSES': 16,
              'COLOURS': {k: v for v, k in letters.items()}, 'FRAMES': specs}
    return source, ims


def composed_sheets(design):
    import atelier
    out = {}
    for layer in (0, 1):
        source, ims = compose_layer(design, layer)
        colours = {k: atelier.hexrgb(v) for k, v in source['COLOURS'].items()}
        out[layer] = [atelier.compose(b, source['FRAMES'][i], colours) if i in source['FRAMES'] else b
                      for i, b in enumerate(ims)]
    return out


# ------------------------------------------------------------------ preview
_CIF = None


def cif():
    global _CIF
    if _CIF is None:
        _CIF = mgr.cif_decode(mgr.load_source(CIF_SRC)[1], True)
    return _CIF


def pose(sheets, ai, want, f, C=64, oy=46, effects=True):
    """One frame of her, as SPRITE::DrawSprite assembles it (layer 5 left out)."""
    dirs = cif().anims[ai]
    di = 2 if want == 3 and len(dirs) == 3 else min(want, len(dirs) - 1)
    mirror = want == 3 and len(dirs) == 3
    parts = dirs[di][f % len(dirs[di])]
    cell = Image.new('RGBA', (C, C), (0, 0, 0, 0))
    for p in parts:
        if p.layer not in sheets:
            continue
        im = sheets[p.layer][p.index]
        flip, x = p.flip, p.x
        if mirror:
            x = -x
            flip = {0: 1, 1: 0}.get(flip, 3)
        if p.effect:
            if not effects:
                continue
            sx, sy, rot, alpha, _ = p.effect
            im = im.resize((max(1, im.width * sx // 25), max(1, im.height * sy // 25)), Image.NEAREST)
            im = im.rotate((-rot * 2 if mirror else rot * 2), expand=True)
            if alpha < 255:
                im = im.copy()
                im.putalpha(im.getchannel('A').point(lambda v, k=alpha: v * k // 255))
        if flip:
            im = im.transpose({1: Image.Transpose.FLIP_LEFT_RIGHT, 2: Image.Transpose.FLIP_TOP_BOTTOM,
                               3: Image.Transpose.ROTATE_180}[flip])
        cell.alpha_composite(im, (C // 2 + x - im.width // 2, oy + p.y - im.height // 2))
    return cell


BG = (52, 58, 72, 255)


def strip(sheets, cells, sc):
    C = 64
    page = Image.new('RGBA', (len(cells) * C * sc, C * sc), BG)
    for k, (ai, want, f) in enumerate(cells):
        page.alpha_composite(pose(sheets, ai, want, f).resize((C * sc, C * sc), Image.NEAREST), (k * C * sc, 0))
    return page


IDLE = [(0, 1, 0), (0, 2, 0), (0, 0, 0), (0, 3, 0)]
WALK = [(1, 1, f) for f in range(6)] + [(1, 2, f) for f in range(0, 6, 2)] + [(1, 0, f) for f in (0, 1, 2)]
MORE = [(2, 1, 0), (2, 2, 0), (39, 1, 3), (39, 2, 3), (36, 1, 4), (35, 1, 4), (4, 1, 9), (4, 2, 10)]


def zoom(n, out):
    design = load_design(n)
    sheets = composed_sheets(design)
    base = {0: frames(HEAD_SRC), 1: frames(BODY_SRC)}
    rows = [strip(base, IDLE, 7), strip(sheets, IDLE, 7), strip(sheets, WALK, 3), strip(sheets, MORE, 3)]
    ref = Image.open(REFS / design['REF']).convert('RGBA')
    ref = ref.resize((ref.width * 2, ref.height * 2), Image.NEAREST)
    # In context: 1x and 2x on a field-coloured ground.
    ctx = Image.new('RGBA', (360, 140), (58, 66, 58, 255))
    for k, cell in enumerate(IDLE[:3]):
        im = pose(sheets, *cell)
        ctx.alpha_composite(im, (k * 40, 30))
        ctx.alpha_composite(im.resize((128, 128), Image.NEAREST), (120 + k * 80 - 30, 4))
    W = max(r.width for r in rows) + ref.width + 10
    H = max(sum(r.height for r in rows) + ctx.height, ref.height)
    page = Image.new('RGBA', (W, H), BG)
    y = 0
    for r in rows:
        page.alpha_composite(r, (0, y))
        y += r.height
    page.alpha_composite(ctx, (0, y))
    page.alpha_composite(ref, (W - ref.width, 0))
    d = ImageDraw.Draw(page)
    d.text((4, 4), 'base', fill=(255, 255, 0))
    d.text((4, rows[0].height + 4), '%02d %s' % (n, design['NAME']), fill=(255, 255, 0))
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    page.save(out)
    print('saved', out, page.size)


def parts_view(n, out, sc=12):
    """Every frame she uses, composed, large, on a pixel grid."""
    design = load_design(n)
    sheets = composed_sheets(design)
    tiles = []
    for layer, idx in ((0, [0, 1, 2, 3]), (1, TORSOS + ARMS[:2] + [37, 38, 39, 13, 16, 20] + WINGS)):
        for i in idx:
            im = sheets[layer][i]
            z = im.resize((im.width * sc, im.height * sc), Image.NEAREST)
            t = Image.new('RGBA', (z.width + 2, z.height + 14), BG)
            t.alpha_composite(z, (1, 13))
            d = ImageDraw.Draw(t)
            for x in range(im.width + 1):
                d.line([(1 + x * sc, 13), (1 + x * sc, 13 + z.height)], fill=(0, 0, 0, 60))
            for y in range(im.height + 1):
                d.line([(1, 13 + y * sc), (1 + z.width, 13 + y * sc)], fill=(0, 0, 0, 60))
            d.text((2, 1), 'L%d#%d %dx%d' % (layer, i, im.width, im.height), fill=(255, 255, 0))
            tiles.append(t)
    W = 2000
    rows, row, x = [], [], 0
    for t in tiles:
        if x + t.width > W and row:
            rows.append(row)
            row, x = [], 0
        row.append(t)
        x += t.width + 6
    rows.append(row)
    page = Image.new('RGBA', (W, sum(max(t.height for t in r) + 6 for r in rows)), (30, 32, 40, 255))
    y = 0
    for r in rows:
        x = 0
        for t in r:
            page.alpha_composite(t, (x, y))
            x += t.width + 6
        y += max(t.height for t in r) + 6
    page.save(out)
    print('saved', out, page.size)


def sheet(out, only=None, sc=3):
    """Every outfit, idle front / side / back, after her own look; two rows."""
    base = {0: frames(HEAD_SRC), 1: frames(BODY_SRC)}
    looks = [('렌', base)]
    for n in (only or range(1, COUNT + 1)):
        if (ROOT / 'costumes/ren' / ('r%02d.py' % n)).exists():
            d = load_design(n)
            looks.append(('%02d' % n, composed_sheets(d)))
    box = (8, 0, 56, 56)
    cw, ch = (box[2] - box[0]) * sc, (box[3] - box[1]) * sc
    per_row = 4
    page = Image.new("RGBA", (per_row * (3 * cw + 16), ((len(looks) + per_row - 1) // per_row) * (ch + 16)), BG)
    d = ImageDraw.Draw(page)
    for k, (name, sh) in enumerate(looks):
        ox = (k % per_row) * (3 * cw + 16)
        oy = (k // per_row) * (ch + 16)
        for j, cell in enumerate(IDLE[:3]):
            im = pose(sh, *cell).crop(box).resize((cw, ch), Image.NEAREST)
            page.alpha_composite(im, (ox + j * cw, oy + 14))
        d.text((ox + 4, oy + 2), name, fill=(255, 255, 0))
    page.save(out)
    print('saved', out, page.size)


# ------------------------------------------------------------------ build
def write_set(slug, source):
    text = '# Generated by costumes/ren_studio.py from costumes/ren/; edit the design there.\n'
    text += ''.join(k + ' = ' + pprint.pformat(v, width=110, sort_dicts=False) + '\n\n' for k, v in source.items())
    (ROOT / 'costumes/sets' / (slug + '.py')).write_text(text, encoding='utf-8')


def build(only=None):
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    manifest = []
    for n in range(1, COUNT + 1):
        if not (ROOT / 'costumes/ren' / ('r%02d.py' % n)).exists():
            continue
        design = load_design(n)
        for layer in (0, 1):
            slug = 'ren_%02d_%d' % (n, layer)
            cid = ID0 + n * 10 + layer
            sheet_no = SHEET0 + n
            rows.append('%d    %d      %d    %s' % (cid, layer, sheet_no, slug))
            if only and n != only:
                continue
            source, ims = compose_layer(design, layer)
            write_set(slug, source)
            import atelier
            colours = {k: atelier.hexrgb(v) for k, v in source['COLOURS'].items()}
            images = [atelier.compose(b, source['FRAMES'][i], colours) if i in source['FRAMES'] else b
                      for i, b in enumerate(ims)]
            binary = mgr.mgr_encode([mgr.gbm_from_png(im, None, '%s/%d' % (slug, i)) for i, im in enumerate(images)])
            dest = OUT / ('%02d' % n)
            dest.mkdir(exist_ok=True)
            (dest / ('img%d_%d.mgr' % (layer, sheet_no))).write_bytes(binary)
            manifest.append({'outfit': n, 'layer': layer, 'id': cid, 'sheet': sheet_no, 'name': design['NAME'],
                             'frames': len(images), 'authored_frames': len(source['FRAMES'])})
    catalog = ROOT / 'costumes/catalog.txt'
    original = [line for line in catalog.read_text(encoding='utf-8').splitlines() if 'ren_' not in line]
    catalog.write_text('\n'.join(original + rows) + '\n', encoding='utf-8')
    if not only:
        (OUT / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf-8')
    print('built %s; %d catalogue row(s)' % ('outfit %d' % only if only else 'all outfits', len(rows)))


if __name__ == '__main__':
    if '--raw' in sys.argv:
        sys.argv.remove('--raw')
        FINISH = False
    cmd = sys.argv[1]
    if cmd == 'labels':
        labels_view(sys.argv[2])
    elif cmd == 'zoom':
        zoom(int(sys.argv[2]), sys.argv[3])
    elif cmd == 'parts':
        parts_view(int(sys.argv[2]), sys.argv[3])
    elif cmd == 'sheet':
        sheet(sys.argv[2], [int(a) for a in sys.argv[3:]] or None)
    elif cmd == 'build':
        build(int(sys.argv[2]) if len(sys.argv) > 2 else None)
    else:
        sys.exit('unknown command ' + cmd)
