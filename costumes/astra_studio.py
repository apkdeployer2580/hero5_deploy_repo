#!/usr/bin/env python3
"""Astra avatars, second pass: hand-pixelled heads and bodies, limbs recoloured
on the game's own shading, weapons rendered as vectors along the base axis.

    python costumes/astra_studio.py preview 1 out/astra01.png   # one avatar, zoomed
    python costumes/astra_studio.py build                        # all -> sets/, docs/astra_avatar/native
    python costumes/astra_studio.py build 3                      # one avatar

Designs live in costumes/astra/aNN.py. Nothing of the game's pixels is stored
here: a design is an overlay grid plus ramp remaps, composed on the archive at
build time (and, on the phone, by hl5_core::costume from the same spec).
"""
import importlib
import json
import math
import pprint
import sys
from collections import deque
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools_re'))
sys.path.insert(0, str(ROOT / 'costumes'))
import mgr  # noqa: E402
import rig  # noqa: E402

OUT = ROOT / 'docs/astra_avatar/native'
CAT = json.loads((ROOT / 'docs/astra_avatar/catalog.json').read_text(encoding='utf-8'))


# ---------------------------------------------------------------- base art
def frames(src):
    _, d = mgr.load_source(src)
    return [mgr.gbm_to_png_image(g, g.palette).convert('RGBA') for g in mgr.mgr_decode(d)]


def q565(h):
    """A design colour as the phone will show it (RGB565 round trip)."""
    h = h.lstrip('#')
    r, g, b = (int(h[i:i + 2], 16) for i in (0, 2, 4))
    return mgr.c565_to_rgb(mgr.rgb_to_c565(r, g, b))


def hx(p):
    return '#%02x%02x%02x' % tuple(p[:3])


def lum(h):
    r, g, b = q565(h)
    return 0.299 * r + 0.587 * g + 0.114 * b


# The bare head, img0/000: what each colour is. Ramps are dark -> light.
HEAD_OUTLINE = ['#211c31']
HEAD_TEAL = ['#39414a', '#39555a', '#526d6b', '#84b2b5']
HEAD_WHITE = ['#7b695a', '#9c8e84', '#bdb2ad', '#dedbd6', '#ffffff']
HEAD_FACE = ['#ffdbce', '#e79a84', '#9c5d5a', '#a56163', '#6b3031', '#6b3431', '#633031', '#733839']

# The bare body, img1/000 (the red vest): outline, red, purple, gold.
BODY_OUTLINE = ['#210c10', '#392029']
BODY_RED = ['#5a3039', '#733839', '#bd454a', '#f75d63']
BODY_PURPLE = ['#4a3852', '#6b6184']
BODY_GOLD = ['#c68663', '#efc373']

# The bare arm, img2/000: one skin ramp (dark -> light).
ARM_SKIN = ['#311818', '#422021', '#5a2c29', '#631c00', '#733839', '#7b494a', '#8c5952', '#9c5d5a', '#9c6152',
            '#a54521', '#bd796b', '#c68663', '#e79a84', '#ff927b', '#efc373', '#ffdbce', '#ffeba5', '#4a3c52']

# The bare legs, img3/000: regions, each a ramp dark -> light.
LEG_REGIONS = {
    'outline': ['#210c10', '#211010'],
    'boot': ['#211c18', '#212021', '#312c31', '#393c39', '#423029', '#524d52', '#5a454a', '#635d5a'],
    'trouser': ['#392029', '#4a3852', '#6b6184', '#846573', '#a59284'],
    'band': ['#6b3831', '#733839', '#9c5552', '#bd454a'],
    'leather': ['#631c00', '#633c31', '#8c5952', '#c68663'],
    'light': ['#cebec6', '#cec3bd'],
}


def ramp_map(src_colours, ramp):
    """Map a base ramp (dark -> light, by luminance) onto a design ramp of any
    length, keeping the shading order."""
    src = sorted(src_colours, key=lum)
    n = len(ramp)
    out = {}
    for i, c in enumerate(src):
        j = round(i * (n - 1) / max(1, len(src) - 1))
        out[c] = ramp[j]
    return out


# ---------------------------------------------------------------- canvas
class Canvas:
    """Sparse pixels in the base frame's own coordinates. Values are hex
    colours, '.' (keep the base pixel) or None (clear)."""

    def __init__(self):
        self.p = {}

    def put(self, x, y, v):
        self.p[(int(x), int(y))] = v

    def blit_grid(self, at, rows, pal, protect=()):
        """`protect`: positions the grid may not touch (the face and eyes,
        for a hair grid — a mask or a helm says `over_face`)."""
        ox, oy = at
        # Trailing spaces are no-ops, so ragged rows are padded, not refused.
        width = max(len(r) for r in rows)
        rows = [r.ljust(width) for r in rows]
        for gy, row in enumerate(rows):
            for gx, ch in enumerate(row):
                if ch == ' ' or (ox + gx, oy + gy) in protect:
                    continue
                if ch == '.':
                    self.put(ox + gx, oy + gy, '.')
                elif ch == '-':
                    self.put(ox + gx, oy + gy, None)
                else:
                    if ch not in pal:
                        raise KeyError('grid colour %r is not in PAL' % ch)
                    self.put(ox + gx, oy + gy, pal[ch])

    def spec(self, w, h, letters):
        xs = [0, w - 1] + [x for x, _ in self.p]
        ys = [0, h - 1] + [y for _, y in self.p]
        x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
        grid = []
        for y in range(y0, y1 + 1):
            row = ''
            for x in range(x0, x1 + 1):
                v = self.p.get((x, y), None)
                row += '-' if v is None else '.' if v == '.' else letters[v]
            grid.append(row)
        return {'at': (x0, y0), 'clear_above': -999, 'grid': grid}


LETTER_POOL = 'ABCDEFGHIJLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%&*+=<>?/^~'


def letters_for(colours):
    out = {}
    for c in colours:
        if c not in out:
            out[c] = LETTER_POOL[len(out)]
    return out


# ---------------------------------------------------------------- head
def face_mask(im):
    """Which pixels of a bare-head frame are the face (kept under hair)."""
    w, h = im.size
    keep = set()
    for y in range(h):
        for x in range(w):
            p = im.getpixel((x, y))
            if p[3] >= 128 and hx(p) in HEAD_FACE:
                keep.add((x, y))
    # The chin/jaw outline: dark pixels touching the face from row 9 down.
    for y in range(9, h):
        for x in range(w):
            p = im.getpixel((x, y))
            if p[3] >= 128 and hx(p) in HEAD_OUTLINE:
                if any((x + dx, y + dy) in keep for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                    keep.add((x, y))
    return keep


# The bare head's eyes, by frame (measured off img0/000). The game paints
# them with the hair's teal ramp, so a hair recolour used to recolour the eyes
# too — orange eyes in an orange fringe. They are their own region now,
# painted from EYE:
#   lash  the upper lid (dark), pupil  the dark of the eye,
#   iris  the coloured lower half, glint  the light catch.
EYES_BASE = {
    0: {(5, 11): 'lash', (6, 11): 'lash', (5, 12): 'glint', (6, 12): 'pupil', (6, 13): 'iris',
        (10, 11): 'lash', (9, 11): 'lash', (10, 12): 'glint', (9, 12): 'pupil', (9, 13): 'iris'},
    1: {(4, 11): 'pupil', (4, 12): 'iris',
        (8, 11): 'lash', (9, 11): 'lash', (8, 12): 'pupil', (9, 12): 'glint', (8, 13): 'iris'},
    4: {(5, 11): 'lash', (6, 11): 'lash', (5, 12): 'pupil', (6, 12): 'glint', (5, 13): 'iris'},
    6: {(4, 13): 'pupil', (4, 14): 'iris',
        (9, 13): 'lash', (10, 13): 'lash', (9, 14): 'pupil', (10, 14): 'glint', (9, 15): 'iris'},
}
EYE_DEFAULT = {'lash': '#1c1424', 'pupil': '#2a1c30', 'iris': '#4a8a9a', 'glint': '#d8f4f4'}


def eye_colours(design):
    eye = dict(EYE_DEFAULT)
    eye.update(design.get('EYE') or {})
    return eye


def head_canvas(design, i, im):
    c = Canvas()
    w, h = im.size
    keep = face_mask(im)
    eyes = EYES_BASE.get(i, {}) if design.get('EYE', True) is not None else {}
    eye = eye_colours(design)
    hair = design.get('HAIR', {})
    teal = ramp_map(HEAD_TEAL, hair['teal']) if hair.get('teal') else None
    white = ramp_map(HEAD_WHITE, hair['white']) if hair.get('white') else None
    outline = hair.get('outline')
    for y in range(h):
        for x in range(w):
            p = im.getpixel((x, y))
            if p[3] < 128:
                continue
            col = hx(p)
            if (x, y) in eyes:
                c.put(x, y, eye[eyes[(x, y)]])
            elif (x, y) in keep:
                c.put(x, y, '.')
            elif col in HEAD_TEAL and teal:
                c.put(x, y, teal[col])
            elif col in HEAD_WHITE and white:
                c.put(x, y, white[col])
            elif col in HEAD_OUTLINE and outline and (teal or white):
                c.put(x, y, outline)
            else:
                c.put(x, y, None)
    # Hair (or a hat) lifted off one of the game's own heads (costumes/donor.py).
    if design.get('DONOR'):
        import donor
        donor.head(design['DONOR'], i, im, c, protect=set(eyes))
    # A hair grid may cover the game's jaw outline, never its skin or the eyes.
    face = {pt for pt in keep if hx(im.getpixel(pt)) in HEAD_FACE} | set(eyes)
    specs = design['HEAD'].get(i)
    specs = [] if not specs else specs if isinstance(specs, list) else [specs]
    for spec in specs + design.get('HEAD_EXTRA', {}).get(i, []):
        c.blit_grid(spec['at'], spec['rows'], design['PAL'],
                    () if spec.get('over_face') or design.get('LEGACY_HEAD') else face)
    return c


def holes(canvas):
    """Clear pixels walled in by drawn ones on all four sides — a gap in a
    face or a hair mass that the eye reads as a hole."""
    on = {pt for pt, v in canvas.p.items() if v}
    out = []
    for (x, y), v in canvas.p.items():
        if v:
            continue
        if all(any((x + dx * k, y + dy * k) in on for k in range(1, 3))
               for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
            out.append((x, y))
    return sorted(out, key=lambda t: (t[1], t[0]))


def hair_back_canvas(design):
    """Image 2 of the head sheet: the side view draws it behind the head
    (motions 0, 1, 3), centred 6..8 px back and 8 px down from the head's
    centre — long hair, a ponytail, a hood's fall. The base is 1x1 clear."""
    c = Canvas()
    c.put(0, 0, None)
    spec = design.get('HAIR_BACK')
    if spec:
        c.blit_grid(spec['at'], spec['rows'], design['PAL'])
    return c


# ---------------------------------------------------------------- rig poses
def loose_set(design, part):
    """The colours that move with the pose. None = every drawn colour."""
    sway = (design.get('SWAY') or {}).get(part) or {}
    letters = sway.get('loose')
    if letters is None:
        return None
    if letters == 'donor':
        import donor
        return donor.colours_of(design['DONOR']) | set(sway.get('loose_hex', ()))
    return {design['PAL'][ch] for ch in letters} | set(sway.get('loose_hex', ()))


def moved(canvas, moves):
    """`moves`: {(x, y): (dx, dy, keep)} for pixels of `canvas`. Clears the
    sources that do not keep, then lands every moved pixel (lower rows land
    last, so a fold reads bottom over top)."""
    out = Canvas()
    out.p = dict(canvas.p)
    for (x, y), (dx, dy, keep) in moves.items():
        if (dx or dy) and not keep:
            out.p[(x, y)] = None
    for (x, y), (dx, dy, keep) in sorted(moves.items(), key=lambda kv: kv[0][1]):
        if dx or dy:
            out.p[(x + dx, y + dy)] = canvas.p[(x, y)]
    return out


def loose_points(design, part, canvas):
    """The positions of a canvas that move with the pose, read off its colours
    — before the finishing pass changes them."""
    loose = loose_set(design, part)
    return {(x, y) for (x, y), v in canvas.p.items() if v and v != '.' and (loose is None or v in loose)}


def head_pose(design, base, hx_, hy_, loose_pts=None):
    """A hair pose of a head canvas. Tips (the top `band` rows of the loose
    colours) trail by hx and squash (+1) or stretch (-1) by hy; hanging
    strands (rows from `hang` down) swing by hx."""
    sway = (design.get('SWAY') or {}).get('head') or {}
    if sway.get('rigid'):
        return base
    if loose_pts is None:
        loose_pts = loose_points(design, 'head', base)
    pts = sorted(pt for pt in loose_pts if base.p.get(pt))
    if not pts:
        return base
    top = min(y for _, y in pts)
    band = sway.get('band', 3)
    hang = sway.get('hang')
    moves = {}
    for (x, y) in pts:
        dx = dy = 0
        keep = False
        if y < top + band:
            dx = hx_ if sway.get('trail', True) else 0
            if hy_ > 0:
                dy = 1
            elif hy_ < 0:
                dy, keep = -1, True
        elif hang is not None and y >= hang:
            dx = hx_ if (y - hang + 1) >= sway.get('hang_step', 2) else 0
        if dx or dy:
            moves[(x, y)] = (dx, dy, keep)
    return moved(base, moves)


def back_pose(design, base, hx_):
    """The hair-back piece swings from its top rows."""
    if not design.get('HAIR_BACK'):
        return base
    pts = [(x, y) for (x, y), v in base.p.items() if v and v != '.']
    if not pts:
        return base
    top = min(y for _, y in pts)
    step = (design.get('SWAY') or {}).get('back_step', 3)
    moves = {(x, y): (hx_ if (y - top) >= step else 0, 0, False) for (x, y) in pts}
    return moved(base, moves)


def body_pose(design, base, view, s, loose_pts=None):
    """The hem's pose: every loose pixel at or below `from` (per view) moves
    sideways — nothing at the anchor, `s` px at the lowest row."""
    sway = (design.get('SWAY') or {}).get('body') or {}
    anchor = (sway.get('from') or {}).get(view)
    if anchor is None or not s:
        return base
    if loose_pts is None:
        loose_pts = loose_points(design, 'body', base)
    pts = sorted((x, y) for (x, y) in loose_pts if y >= anchor and base.p.get((x, y)))
    if not pts:
        return base
    bottom = max(y for _, y in pts)
    span = max(1, bottom - anchor + 1)
    moves = {}
    for (x, y) in pts:
        f = s * (y - anchor + 1) / span
        dx = int(math.floor(abs(f) + 0.5)) * (1 if f > 0 else -1)
        if dx:
            moves[(x, y)] = (dx, 0, False)
    return moved(base, moves)


# ---------------------------------------------------------------- body
def body_canvas(design, i, im):
    c = Canvas()
    w, h = im.size
    remap = {}
    base = design.get('BODY_BASE')
    if base:
        for cls, ramp in (('red', BODY_RED), ('purple', BODY_PURPLE), ('gold', BODY_GOLD), ('outline', BODY_OUTLINE)):
            if base.get(cls):
                remap.update(ramp_map(ramp, base[cls]))
    for y in range(h):
        for x in range(w):
            p = im.getpixel((x, y))
            if p[3] < 128:
                continue
            col = hx(p)
            c.put(x, y, remap.get(col) if base else None)
    specs = design['BODY'].get(i)
    for spec in ([] if not specs else specs if isinstance(specs, list) else [specs]):
        c.blit_grid(spec['at'], spec['rows'], design['PAL'])
    return c


# ---------------------------------------------------------------- limbs
def arm_regions(im, hand_len=3):
    """hand / cuff / sleeve by walking the piece from the shoulder (its top
    row) to the far end: the last `hand_len` steps are the hand, the next the
    cuff."""
    pts = {(x, y) for y in range(im.height) for x in range(im.width) if im.getpixel((x, y))[3] >= 128}
    top = min(y for _, y in pts)
    starts = [p for p in pts if p[1] == top]
    dist = {p: 0 for p in starts}
    dq = deque(starts)
    while dq:
        x, y = dq.popleft()
        for n in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1), (x + 1, y + 1), (x - 1, y - 1), (x + 1, y - 1), (x - 1, y + 1)):
            if n in pts and n not in dist:
                dist[n] = dist[(x, y)] + 1
                dq.append(n)
    far = max(dist.values())
    out = {}
    for p in pts:
        d = far - dist.get(p, 0)
        out[p] = 'hand' if d <= hand_len else 'cuff' if d == hand_len + 1 else 'sleeve'
    return out


def arm_canvas(design, i, im):
    c = Canvas()
    spec = design.get('ARM', {})
    regions = arm_regions(im, spec.get('hand_len', 3))
    maps = {}
    for r in ('sleeve', 'cuff', 'hand'):
        ramp = spec.get(r)
        if ramp and ramp != 'skin':
            maps[r] = ramp_map(ARM_SKIN, ramp)
    for (x, y), r in regions.items():
        col = hx(im.getpixel((x, y)))
        if r in maps:
            c.put(x, y, maps[r].get(col, col))
        else:
            c.put(x, y, '.')
    grow = spec.get('hand_grow', 0)
    if grow and isinstance(spec.get('hand'), list):
        # A bigger fist (boxing gloves): the hand's outline steps out a pixel,
        # the old edge becomes glove.
        ramp = spec['hand']
        hand = {p for p, r in regions.items() if r == 'hand'}
        for _ in range(grow):
            ring = {(x + dx, y + dy) for (x, y) in hand for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))}
            ring -= set(regions)
            ring -= hand
            for (x, y) in hand:
                if c.p.get((x, y)) == ramp[0]:
                    c.put(x, y, ramp[1])
            for pt in ring:
                c.put(pt[0], pt[1], ramp[0])
            hand |= ring
    return c


LEG_ALL = sorted({c for cs in LEG_REGIONS.values() for c in cs}, key=lum)


def leg_regions(im, foot_rows=3, near=7):
    """Where the leg piece is foot, knee band or trouser. By position, not by
    colour: the game's trousers are its dark greys and its shoes its purple
    greys, so the colour names above say little. A foot is the bottom
    `foot_rows` of each column (with its neighbours), near the piece's
    bottom; the band is the game's red."""
    pts = {(x, y) for y in range(im.height) for x in range(im.width) if im.getpixel((x, y))[3] >= 128}
    bottom = {}
    for x, y in pts:
        bottom[x] = max(bottom.get(x, -1), y)
    band = set(LEG_REGIONS['band'])
    out = {}
    for x, y in pts:
        b = max(bottom.get(xx, -1) for xx in (x - 1, x, x + 1))
        if y >= b - (foot_rows - 1) and y >= im.height - near:
            out[(x, y)] = 'foot'
        elif hx(im.getpixel((x, y))) in band:
            out[(x, y)] = 'band'
        else:
            out[(x, y)] = 'trouser'
    return out


def leg_canvas(design, i, im):
    c = Canvas()
    spec = design.get('LEG', {})
    if 'foot' in spec:
        regions = leg_regions(im, spec.get('foot_rows', 3), spec.get('foot_near', 7))
        maps = {}
        for r in ('trouser', 'foot', 'band'):
            ramp = spec.get(r) or spec['trouser']
            maps[r] = ramp_map(LEG_ALL, ramp)
        for (x, y), r in regions.items():
            c.put(x, y, maps[r][hx(im.getpixel((x, y)))])
        return c
    maps = {}
    for region, cols in LEG_REGIONS.items():
        ramp = spec.get(region)
        if ramp:
            maps.update(ramp_map(cols, ramp))
    for y in range(im.height):
        for x in range(im.width):
            p = im.getpixel((x, y))
            if p[3] < 128:
                continue
            col = hx(p)
            c.put(x, y, maps.get(col, '.') if spec else '.')
    return c


# ---------------------------------------------------------------- weapon
# Tip and hilt of the warrior/knight sword sheet (img4/000), per frame.
SWORD_AXIS = [((2, 41), (15, 4)), ((1, 24), (28, 4)), ((1, 9), (35, 4)), ((1, 10), (18, 4))]
SS = 8


class WeaponCtx:
    """Vector drawing in blade coordinates: t along the axis in px from the
    hilt end, s across it (positive toward the blade's upper/right side)."""

    def __init__(self, hilt, tip, w, h, down=0, short=False):
        self.hilt, self.tip = hilt, tip
        self.down, self.short = down, short
        dx, dy = tip[0] - hilt[0], tip[1] - hilt[1]
        self.length = math.hypot(dx, dy)
        self.ux, self.uy = dx / self.length, dy / self.length
        self.nx, self.ny = -self.uy, self.ux
        self.im = Image.new('RGBA', (w * SS, h * SS), (0, 0, 0, 0))
        self.d = ImageDraw.Draw(self.im)

    def xy(self, t, s):
        x = self.hilt[0] + 0.5 + self.ux * t + self.nx * s
        y = self.hilt[1] + 0.5 + self.uy * t + self.ny * s
        return (x * SS, y * SS)

    def poly(self, pts, colour):
        self.d.polygon([self.xy(t, s) for t, s in pts], fill=q565(colour) + (255,))

    def band(self, t0, t1, s0, s1, colour):
        self.poly([(t0, s0), (t1, s0), (t1, s1), (t0, s1)], colour)

    def circle(self, t, s, r, colour):
        x, y = self.xy(t, s)
        self.d.ellipse([x - r * SS, y - r * SS, x + r * SS, y + r * SS], fill=q565(colour) + (255,))


def downsample(im, w, h):
    out = Image.new('RGBA', (w, h), (0, 0, 0, 0))
    px = im.load()
    for y in range(h):
        for x in range(w):
            counts = {}
            for sy in range(SS):
                for sx in range(SS):
                    p = px[x * SS + sx, y * SS + sy]
                    if p[3]:
                        counts[p[:3]] = counts.get(p[:3], 0) + 1
            total = sum(counts.values())
            if total * 2 >= SS * SS:
                out.putpixel((x, y), max(counts, key=counts.get) + (255,))
    return out


def weapon_canvas(design, i, im, base_src):
    """v2's single-sheet weapon (kept for designs that still name WEAPON)."""
    c = Canvas()
    spec = design.get('WEAPON')
    if spec is None or spec == 'hidden':
        return c
    if spec.get('recolour'):
        # Rank the base piece's own colours and map them onto the design ramp.
        cols = sorted({hx(im.getpixel((x, y))) for y in range(im.height) for x in range(im.width) if im.getpixel((x, y))[3] >= 128}, key=lum)
        m = ramp_map(cols, spec['recolour'])
        for y in range(im.height):
            for x in range(im.width):
                p = im.getpixel((x, y))
                if p[3] >= 128:
                    c.put(x, y, m[hx(p)])
        return c
    w, h = im.size
    tip, hilt = SWORD_AXIS[i]
    ctx = WeaponCtx(hilt, tip, w, h)
    spec['draw'](ctx)
    small = downsample(ctx.im, w, h)
    for y in range(h):
        for x in range(w):
            p = small.getpixel((x, y))
            if p[3]:
                c.put(x, y, hx(p))
    return c


# Each class's weapon sheet and, per piece, the axis the art is drawn along:
# (hand end, far end[, side the gun's grip hangs to | 'short']). None = a
# piece too foreshortened to draw (a dagger seen point-on): recoloured.
# Measured off the archive's pieces (the sword's from SWORD_AXIS; the gun's
# off img4/044, whose pieces show the grip; astra_look.py weapons N shows them).
WEAPON_KINDS = {
    'sword': {'base': 'c/sp/img4/000.mgr', 'classes': 0b0001,
              'axes': [((15, 4), (2, 41)), ((28, 4), (1, 24)), ((35, 4), (1, 9)), ((18, 4), (1, 10))]},
    'dagger': {'base': 'c/sp/img4/016.mgr', 'classes': 0b0010,
               'axes': [((2, 20), (13, 8)), ((4, 26), (5, 9)), ((2, 26), (7, 9)), ((1, 12), (8, 0)),
                        None, None, None]},
    'gun': {'base': 'c/sp/img4/044.mgr', 'classes': 0b0100,
            'axes': [((4, 3), (4, 20), 0), ((8, 4), (2, 17), -1), ((16, 4), (1, 4), -1),
                     ((10, 3), (2, 14), -1), ((11, 8), (1, 2), -1)]},
    'spear': {'base': 'c/sp/img4/050.mgr', 'classes': 0b1000,
              'axes': [((54, 0), (0, 29)), ((59, 6), (0, 6)), ((35, 1), (0, 1), 'short')]},
}


def outline(small, colour):
    """One pixel of outline around the drawn weapon, inside the canvas."""
    w, h = small.size
    px = small.load()
    out = small.copy()
    o = out.load()
    rgb = q565(colour) + (255,)
    for y in range(h):
        for x in range(w):
            if px[x, y][3]:
                continue
            if any(0 <= x + dx < w and 0 <= y + dy < h and px[x + dx, y + dy][3]
                   for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                o[x, y] = rgb
    return out


def kind_canvas(design, kind, i, im):
    import astra.arms as arms
    c = Canvas()
    spec = design['WEAPONS'][kind]
    pal = spec['pal']
    axis = WEAPON_KINDS[kind]['axes'][i]
    w, h = im.size
    for y in range(h):
        for x in range(w):
            c.put(x, y, None)
    if axis is None:
        cols = sorted({hx(im.getpixel((x, y))) for y in range(h) for x in range(w)
                       if im.getpixel((x, y))[3] >= 128}, key=lum)
        m = ramp_map(cols, [pal['out'], pal['grip'], pal['blade'], pal['shine']])
        for y in range(h):
            for x in range(w):
                p = im.getpixel((x, y))
                if p[3] >= 128:
                    c.put(x, y, m[hx(p)])
        return c
    hand, far = axis[0], axis[1]
    down = axis[2] if len(axis) > 2 and axis[2] != 'short' else 0
    short = len(axis) > 2 and axis[2] == 'short'
    ctx = WeaponCtx(hand, far, w, h, down=down, short=short)
    getattr(arms, kind)(ctx, spec['style'], pal)
    small = outline(downsample(ctx.im, w, h), pal['out'])
    for y in range(h):
        for x in range(w):
            p = small.getpixel((x, y))
            if p[3]:
                c.put(x, y, hx(p))
    return c


def shield_canvas(design, im):
    import astra.arms as arms
    spec = design['WEAPONS']['shield']
    rows = arms.shield(spec['style'], spec['pal'], im.width, im.height)
    c = Canvas()
    for y in range(im.height):
        for x in range(im.width):
            c.put(x, y, None)
    c.blit_grid((0, 0), rows, spec['pal'])
    return c


# ---------------------------------------------------------------- effects
# Effect images that keep the game's colours: buff labels, rank arrows,
# dust and smoke, cartridges — their colour is information, not style.
FX_KEEP = {
    0: [7, 29, 30, 31, 32, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46],
    1: [3, 15, 36, 37, 38, 39, 40, 41, 42, 43],
    2: [7, 8, 9, 11, 12, 19, 21] + list(range(28, 46)) + [47, 48, 49, 50],
    3: [6, 9, 24, 25, 26] + list(range(33, 45)),
}


def fx_recolour(design, cls):
    """The class's effect sheet turned to the avatar's colour: every
    saturated colour of the effect images takes the theme's hue (lightness
    kept, white cores stay white). Returns (map 565 -> 565, keep)."""
    import colorsys
    theme = design.get('THEME') or {}
    hue = (theme.get('hue', 0) % 360) / 360.0
    sat = theme.get('sat', 1.0)
    spread = theme.get('spread', 0.06)
    _, data = mgr.load_source('c/sp/img5/%03d.mgr' % cls)
    gbms = mgr.mgr_decode(data)
    keep = FX_KEEP[cls]
    m = {}
    for i, g in enumerate(gbms):
        if i in keep:
            continue
        for c565 in g.palette:
            if c565 == mgr.TRANSPARENT or c565 in m:
                continue
            r, gg, b = mgr.c565_to_rgb(c565)
            h, sv, v = colorsys.rgb_to_hsv(r / 255, gg / 255, b / 255)
            if sv < 0.2 or v < 0.06:
                continue
            # keep a little of the original hue spread so cores and rims part
            dh = ((h - 0.5) % 1.0 - 0.5) * spread
            nh = (hue + dh) % 1.0
            ns = min(1.0, sv * sat)
            nr, ng, nb = colorsys.hsv_to_rgb(nh, ns, v)
            to = mgr.rgb_to_c565(int(nr * 255 + 0.5), int(ng * 255 + 0.5), int(nb * 255 + 0.5))
            if to == mgr.TRANSPARENT:
                to ^= 1
            if to != c565:
                m[c565] = to
    return m, keep


# The finishing pass (costumes/finish.py) over the drawn head and body;
# `--raw` turns it off for a before/after look.
FINISH = True

LAYER_NAMES = ['머리', '의상', '장갑', '신발', '무기']
KIND_NAMES = {'sword': '검', 'dagger': '쌍검', 'gun': '총', 'spear': '창', 'shield': '방패'}
FX_CLASSES = (0, 1, 2, 3)


def load_design(n):
    mod = importlib.import_module('astra.a%02d' % n)
    return {k: getattr(mod, k) for k in dir(mod) if k.isupper()}


def parts(n):
    """Every sheet an avatar ships: (slug suffix, layer, sheet, id, kind).
    ids are what hl5_avatar.txt records — never renumber them."""
    out = [('%d' % layer, layer, 300 + n, 110000 + n * 10 + layer, None) for layer in range(4)]
    out += [('sword', 4, 300 + n, 110000 + n * 10 + 4, 'sword'),
            ('dagger', 4, 320 + n, 110000 + n * 10 + 5, 'dagger'),
            ('gun', 4, 340 + n, 110000 + n * 10 + 6, 'gun'),
            ('spear', 4, 360 + n, 110000 + n * 10 + 7, 'spear'),
            ('shield', 6, 300 + n, 110000 + n * 10 + 8, 'shield')]
    out += [('fx%d' % c, 5, 300 + n * 10 + c, 120000 + n * 10 + c, 'fx%d' % c) for c in FX_CLASSES]
    return out


def compose_part(design, n, part):
    suffix, layer, sheet, cid, kind = part
    if kind is None:
        return compose_layer(design, n, layer)
    if kind.startswith('fx'):
        cls = int(kind[2:])
        m, keep = fx_recolour(design, cls)
        base = 'c/sp/img5/%03d.mgr' % cls
        source = {'NAME': design['NAME'] + ' 이펙트', 'BASE': base, 'CLASSES': 1 << cls,
                  'HIDDEN_WEAPON': False, 'RIG': False, 'COLOURS': {}, 'FRAMES': {},
                  'RECOLOUR': {'map': m, 'keep': keep}}
        return source, frames(base)
    if kind == 'shield':
        base = 'c/sp/img6/000.mgr'
        ims = frames(base)
        canvases = {0: shield_canvas(design, ims[0])}
        classes = 0b1000
    else:
        base = WEAPON_KINDS[kind]['base']
        ims = frames(base)
        canvases = {i: kind_canvas(design, kind, i, im) for i, im in enumerate(ims)}
        classes = WEAPON_KINDS[kind]['classes']
    colours = {}
    for c in canvases.values():
        for v in c.p.values():
            if v and v != '.':
                colours[v] = True
    letters = letters_for(list(colours))
    specs = {i: c.spec(ims[i].width, ims[i].height, letters) for i, c in canvases.items()}
    fx = 0
    if kind in ('sword', 'dagger', 'gun', 'spear'):
        cls = {'sword': 0, 'dagger': 1, 'gun': 2, 'spear': 3}[kind]
        fx = 300 + n * 10 + cls
    source = {'NAME': design['NAME'] + ' ' + KIND_NAMES[kind], 'BASE': base, 'CLASSES': classes,
              'HIDDEN_WEAPON': False, 'RIG': False, 'FX': fx,
              'COLOURS': {k: v for v, k in letters.items()}, 'FRAMES': specs}
    return source, ims


def compose_layer(design, n, layer):
    hidden = layer == 4 and design.get('WEAPON') == 'hidden'
    rogue = layer == 4 and isinstance(design.get('WEAPON'), dict) and design['WEAPON'].get('rogue')
    base = 'c/sp/img%d/%03d.mgr' % (layer, 16 if layer == 4 and (hidden or rogue) else 0)
    ims = frames(base)
    colours = {}
    canvases = {}
    srcs = {}
    for i, im in enumerate(ims):
        if im.size == (1, 1):
            if layer == 0 and i == rig.HAIR_BACK and design.get('HAIR_BACK'):
                canvases[i] = hair_back_canvas(design)
            continue
        if hidden:
            c = Canvas()
            for y in range(im.height):
                for x in range(im.width):
                    c.put(x, y, None)
        elif layer == 0:
            c = head_canvas(design, i, im)
        elif layer == 1:
            c = body_canvas(design, i, im)
        elif layer == 2:
            c = arm_canvas(design, i, im)
        elif layer == 3:
            c = leg_canvas(design, i, im)
        else:
            c = weapon_canvas(design, i, im, base)
        canvases[i] = c
    # The finishing pass over what we drew (costumes/finish.py), on the views
    # before the rig poses them — so a fold moves with its hem. What moves is
    # read off the colours first, since finishing changes them.
    rigged = layer in (0, 1)
    part = 'head' if layer == 0 else 'body'
    loose = {i: loose_points(design, part, c) for i, c in canvases.items()} if rigged else {}
    if FINISH and not hidden and rigged and design.get('FINISH', True):
        import finish
        drawn = set(design.get('PAL', {}).values())
        hair = {design['PAL'][k] for k in 'XQqoOY' + design.get('NOFORM', '') if k in design.get('PAL', {})}
        cloth = {design['PAL'][k] for k in design.get('FOLDS', '') if k in design.get('PAL', {})}
        fur = {design['PAL'][k] for k in design.get('PELT', '') if k in design.get('PAL', {})}
        for i, c in canvases.items():
            if c.p:
                finish.finish_canvas(c, ims[i] if ims[i].size != (1, 1) else None, drawn, hair, cloth=cloth,
                                     fur=fur)
    # The rig: one extra image per pose, drawn from the view's own image.
    if layer == 0:
        for k, (view, hx_, hy_) in enumerate(rig.head_poses()):
            src = rig.HEAD_FOR_VIEW[view]
            canvases[rig.HEAD_RIG + k] = head_pose(design, canvases[src], hx_, hy_, loose[src])
            srcs[rig.HEAD_RIG + k] = src
        back = canvases.get(rig.HAIR_BACK) or hair_back_canvas(design)
        for k, hx_ in enumerate((-1, 0, 1)):
            canvases[rig.BACK_RIG + k] = back_pose(design, back, hx_)
            srcs[rig.BACK_RIG + k] = rig.HAIR_BACK
    elif layer == 1:
        for k, (view, s_) in enumerate(rig.body_poses()):
            src = rig.TORSO_FOR_VIEW[view]
            canvases[rig.BODY_RIG + k] = body_pose(design, canvases[src], view, s_, loose[src])
            srcs[rig.BODY_RIG + k] = src
    import finish
    letters = finish.assign_letters(list(canvases.values()))
    specs = {}
    for i, c in canvases.items():
        b = ims[srcs.get(i, i)]
        spec = c.spec(b.width, b.height, letters)
        if i in srcs:
            spec['src'] = srcs[i]
        specs[i] = spec
    classes = 15 if hidden else (2 if rogue else 9) if layer == 4 else 15
    name = design['NAME'] + ' ' + ('맨손' if hidden else LAYER_NAMES[layer])
    source = {'NAME': name, 'BASE': base, 'CLASSES': classes, 'HIDDEN_WEAPON': hidden, 'RIG': rigged,
              'COLOURS': {k: v for v, k in letters.items()}, 'FRAMES': specs}
    return source, ims


def compose_images(source, ims):
    """The finished sheet: the base images with their overlays, then the rig
    images appended, each drawn on its `src` image; or the base recoloured."""
    rec = source.get('RECOLOUR')
    if rec:
        out = []
        for i, b in enumerate(ims):
            if i in rec['keep']:
                out.append(b)
                continue
            im = b.copy()
            px = im.load()
            for y in range(im.height):
                for x in range(im.width):
                    p = px[x, y]
                    if p[3] < 128:
                        continue
                    c = mgr.rgb_to_c565(*p[:3])
                    if c in rec['map']:
                        px[x, y] = mgr.c565_to_rgb(rec['map'][c]) + (255,)
            out.append(im)
        return out
    import atelier
    colours = {k: atelier.hexrgb(v) for k, v in source['COLOURS'].items()}
    n = max([len(ims)] + [i + 1 for i in source['FRAMES']])
    out = []
    for i in range(n):
        spec = source['FRAMES'].get(i)
        b = ims[spec.get('src', i)] if spec else ims[i]
        out.append(atelier.compose(b, spec, colours) if spec else b)
    return out


def write_set(slug, source):
    text = '# Generated by costumes/astra_studio.py from costumes/astra/; edit the design there.\n'
    text += ''.join(k + ' = ' + pprint.pformat(v, width=110, sort_dicts=False) + '\n\n' for k, v in source.items())
    (ROOT / 'costumes/sets' / (slug + '.py')).write_text(text, encoding='utf-8')


def build(only=None):
    OUT.mkdir(parents=True, exist_ok=True)
    rig.check_classes()
    rig.write_table()
    manifest_path = OUT / 'manifest.json'
    manifest = json.loads(manifest_path.read_text(encoding='utf-8')) if manifest_path.exists() else []
    rows = []
    for n, a in enumerate(CAT, 1):
        design = load_design(n)
        dest = OUT / ('%02d_%s' % (n, a['slug']))
        dest.mkdir(exist_ok=True)
        for part in parts(n):
            suffix, layer, sheet, cid, kind = part
            slug = 'astra_%02d_%s' % (n, suffix)
            rows.append('%d    %d      %d    %s' % (cid, layer, sheet, slug))
            if only and n != only:
                continue
            source, ims = compose_part(design, n, part)
            write_set(slug, source)
            composed = compose_images(source, ims)
            path = dest / ('img%d_%d.mgr' % (layer, sheet))
            binary = mgr.mgr_encode([mgr.gbm_from_png(im, None, '%s/%d' % (slug, i)) for i, im in enumerate(composed)])
            path.write_bytes(binary)
            quant = [mgr.gbm_to_png_image(g, g.palette).convert('RGBA') for g in mgr.mgr_decode(binary)]
            pngdir = dest / ('img%d_%d' % (layer, sheet))
            pngdir.mkdir(exist_ok=True)
            for i, im in enumerate(quant):
                im.save(pngdir / ('%03d.png' % i))
            entry = {'avatar': n, 'part': suffix, 'layer': layer, 'id': cid, 'sheet': sheet,
                     'classes': source['CLASSES'], 'hidden_weapon': source['HIDDEN_WEAPON'],
                     'base': source['BASE'], 'frames': len(quant), 'rig': source.get('RIG', False),
                     'authored_frames': len(source['FRAMES']),
                     'mgr': str(path.relative_to(OUT)).replace('\\', '/')}
            manifest = [e for e in manifest if not (e['avatar'] == n and e.get('part', str(e['layer'])) == suffix)
                        and not (e['avatar'] == n and 'part' not in e)] + [entry]
    manifest.sort(key=lambda e: (e['avatar'], e['layer'], e['sheet']))
    catalog = ROOT / 'costumes/catalog.txt'
    original = [line for line in catalog.read_text(encoding='utf-8').splitlines() if 'astra_' not in line]
    catalog.write_text('\n'.join(original + rows) + '\n', encoding='utf-8')
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf-8')
    # sets of parts that no longer exist (v2's single weapon sheet)
    names = {'astra_%02d_%s' % (n, part[0]) for n in range(1, len(CAT) + 1) for part in parts(n)}
    for f in (ROOT / 'costumes/sets').glob('astra_*.py'):
        if f.stem not in names:
            f.unlink()
    print('built %s; manifest %d sheets' % ('avatar %d' % only if only else 'all avatars', len(manifest)))


# ---------------------------------------------------------------- preview
def assembled(sheets_by_layer, cls, anims=(0, 1, 2), scale=4, cif=None):
    """Idle/walk/attack in four directions, drawn like SPRITE::DrawSprite."""
    cif = cif or mgr.cif_decode(mgr.load_source('c/sp/cif/%03d.cif' % (cls + 1))[1], True)
    CELL = 64
    cells = []
    for ai in anims:
        dirs = cif.anims[ai]
        for want in range(4):
            di = 2 if want == 3 and len(dirs) == 3 else min(want, len(dirs) - 1)
            mirror = want == 3 and len(dirs) == 3
            for f in range(0, len(dirs[di]), max(1, len(dirs[di]) // 3)):
                parts = dirs[di][f]
                cell = Image.new('RGBA', (CELL, CELL), (0, 0, 0, 0))
                for p in parts:
                    if p.layer == 5 or p.layer not in sheets_by_layer:
                        continue
                    im = sheets_by_layer[p.layer][p.index]
                    flip, x = p.flip, p.x
                    if mirror:
                        x = -x
                        flip = {0: 1, 1: 0}.get(flip, 3)
                    if p.effect:
                        sx, sy, rot, alpha, _ = p.effect
                        im = im.resize((max(1, im.width * sx // 25), max(1, im.height * sy // 25)), Image.NEAREST)
                        im = im.rotate((-rot * 2 if mirror else rot * 2), expand=True)
                    if flip:
                        im = im.transpose({1: Image.Transpose.FLIP_LEFT_RIGHT, 2: Image.Transpose.FLIP_TOP_BOTTOM, 3: Image.Transpose.ROTATE_180}[flip])
                    cell.alpha_composite(im, (32 + x - im.width // 2, 46 + p.y - im.height // 2))
                cells.append(cell)
    ncol = 12
    nrow = (len(cells) + ncol - 1) // ncol
    page = Image.new('RGBA', (CELL * scale * ncol, CELL * scale * nrow), (52, 58, 72, 255))
    for k, c in enumerate(cells):
        page.alpha_composite(c.resize((CELL * scale, CELL * scale), Image.NEAREST), ((k % ncol) * CELL * scale, (k // ncol) * CELL * scale))
    return page


def preview(n, out):
    design = load_design(n)
    a = CAT[n - 1]
    cls = design.get('PREVIEW_CLASS', 0)
    cif = mgr.cif_decode(mgr.load_source('c/sp/cif/%03d.cif' % (cls + 1))[1], True)
    sheets = {}
    for layer in range(len(cif.layer_ids)):
        src = 'c/sp/img%d/%03d.mgr' % (layer, mgr.CLASS_WEAPON[cls] if layer == 4 else cls if layer == 5 else 0)
        sheets[layer] = frames(src)
    big = {}
    for layer in range(5):
        source, ims = compose_layer(design, n, layer)
        if source['CLASSES'] & (1 << cls):
            sheets[layer] = compose_images(source, ims)
        big[layer] = sheets[layer]
    page1 = assembled(sheets, cls, cif=cif)
    sc = 10
    parts = [(0, i) for i in (0, 1, 3, 4, 6)] + [(1, i) for i in (0, 1, 2)] + [(2, i) for i in (0, 1, 9)] + [(3, i) for i in (0, 4, 9, 13)]
    parts += [(4, i) for i in range(len(big[4])) if big[4][i].width > 1][:4]
    tiles = []
    for layer, i in parts:
        im = big[layer][i]
        z = im.resize((im.width * sc, im.height * sc), Image.NEAREST)
        t = Image.new('RGBA', (z.width + 2, z.height + 14), (52, 58, 72, 255))
        t.alpha_composite(z, (1, 13))
        d = ImageDraw.Draw(t)
        for x in range(0, im.width + 1):
            d.line([(1 + x * sc, 13), (1 + x * sc, 13 + z.height)], fill=(0, 0, 0, 60))
        for y in range(0, im.height + 1):
            d.line([(1, 13 + y * sc), (1 + z.width, 13 + y * sc)], fill=(0, 0, 0, 60))
        d.text((2, 1), 'L%d#%d %dx%d' % (layer, i, im.width, im.height), fill=(255, 255, 0, 255))
        tiles.append(t)
    draft = Image.open(ROOT / 'docs/astra_avatar' / a['draft']).convert('RGBA')
    draft = draft.crop(draft.getbbox())
    draft = draft.resize((int(draft.width * 420 / draft.height), 420), Image.LANCZOS)
    w2 = sum(t.width for t in tiles) + 8 * len(tiles)
    h2 = max(t.height for t in tiles)
    page2 = Image.new('RGBA', (max(w2, page1.width), max(h2, draft.height)), (52, 58, 72, 255))
    x = 0
    for t in tiles:
        page2.alpha_composite(t, (x, 0))
        x += t.width + 8
    page = Image.new('RGBA', (max(page1.width, page2.width + draft.width + 10), page1.height + page2.height + 10), (52, 58, 72, 255))
    page.alpha_composite(page1, (0, 0))
    page.alpha_composite(page2, (0, page1.height + 10))
    page.alpha_composite(draft, (page2.width + 10, page1.height + 10))
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    page.save(out)
    print('saved', out, page.size)


def zoom(n, out, sc=9):
    """Head and body frames, and the idle hero in three directions, large."""
    design = load_design(n)
    cls = design.get('PREVIEW_CLASS', 0)
    cif = mgr.cif_decode(mgr.load_source('c/sp/cif/%03d.cif' % (cls + 1))[1], True)
    sheets = {}
    for layer in range(len(cif.layer_ids)):
        src = 'c/sp/img%d/%03d.mgr' % (layer, mgr.CLASS_WEAPON[cls] if layer == 4 else cls if layer == 5 else 0)
        sheets[layer] = frames(src)
    for layer in range(5):
        source, ims = compose_layer(design, n, layer)
        if source['CLASSES'] & (1 << cls):
            sheets[layer] = compose_images(source, ims)
    tiles = []
    for layer, i in [(0, 1), (0, 0), (0, 3), (0, 4), (0, 6), (1, 1), (1, 0), (1, 2)]:
        im = sheets[layer][i]
        z = im.resize((im.width * sc, im.height * sc), Image.NEAREST)
        t = Image.new('RGBA', (z.width + 2, z.height + 14), (52, 58, 72, 255))
        t.alpha_composite(z, (1, 13))
        d = ImageDraw.Draw(t)
        for x in range(0, im.width + 1):
            d.line([(1 + x * sc, 13), (1 + x * sc, 13 + z.height)], fill=(0, 0, 0, 70))
        for y in range(0, im.height + 1):
            d.line([(1, 13 + y * sc), (1 + z.width, 13 + y * sc)], fill=(0, 0, 0, 70))
        d.text((2, 1), 'L%d#%d %dx%d' % (layer, i, im.width, im.height), fill=(255, 255, 0, 255))
        tiles.append(t)
    page1 = assembled(sheets, cls, anims=(0,), scale=6, cif=cif)
    # keep one frame per direction: columns 0, 3, 6, 9 of the idle row
    cells = [page1.crop((k * 64 * 6, 0, (k + 1) * 64 * 6, 64 * 6)) for k in (3, 6, 0, 9)]
    w = sum(t.width + 6 for t in tiles)
    h = max(t.height for t in tiles)
    page = Image.new('RGBA', (max(w, 4 * 64 * 6), h + 64 * 6 + 8), (52, 58, 72, 255))
    x = 0
    for t in tiles:
        page.alpha_composite(t, (x, 0))
        x += t.width + 6
    for k, c in enumerate(cells):
        page.alpha_composite(c, (k * 64 * 6, h + 8))
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    page.save(out)
    print('saved', out, page.size)


if __name__ == '__main__':
    if '--raw' in sys.argv:
        sys.argv.remove('--raw')
        FINISH = False
    cmd = sys.argv[1]
    if cmd == 'preview':
        preview(int(sys.argv[2]), sys.argv[3])
    elif cmd == 'zoom':
        zoom(int(sys.argv[2]), sys.argv[3])
    elif cmd == 'build':
        build(int(sys.argv[2]) if len(sys.argv) > 2 else None)
    else:
        sys.exit('unknown command ' + cmd)
