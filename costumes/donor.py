"""Hair and headgear from the game's own heads.

The archive carries heads drawn in the game's hand: story characters and
headgear (`/c/sp/img0/001` spiky blond, `016` silver, `109` a wizard's hat,
`113` an officer's cap ...), most of them on the hero's own seven-image
layout (0 front, 1 3/4, 3 back, 4 side, 6 3/4 down). Hair drawn from scratch
at 17 px reads flat beside them — the game's hair is clumps, each turning
from a lit edge to a shaded one, every pixel a step on a ramp. So a design
may name a DONOR: its hair (or hat) is lifted off that head, recoloured into
the design's ramp, and laid on the base head; the base face stays.

Placement is the game's own: the cif draws every head image by its centre,
so a donor image centred on the base image sits where the game would draw
that character's head. `shift` nudges it.

Recolouring keeps the donor's tonal structure. Each donor colour's place in
the donor's luminance range is read off the design's ramp (interpolated), so
the game's steps survive — two donor tones never merge, the darkest stays a
tinted dark, never a black.
"""
import colorsys
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools_re'))
import mgr  # noqa: E402

N4 = ((1, 0), (-1, 0), (0, 1), (0, -1))
_FRAMES = {}


def frames(src):
    if isinstance(src, int):
        src = 'c/sp/img0/%03d.mgr' % src
    if src not in _FRAMES:
        _FRAMES[src] = [mgr.gbm_to_png_image(g, g.palette).convert('RGBA')
                        for g in mgr.mgr_decode(mgr.load_source(src)[1])]
    return _FRAMES[src]


def rgb(h):
    h = h.lstrip('#')
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def hexs(c):
    return '#%02x%02x%02x' % tuple(max(0, min(255, round(v))) for v in c[:3])


def lum(c):
    if isinstance(c, str):
        c = rgb(c)
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


def pixels(im):
    """{(x, y): '#rrggbb'} of an image's opaque pixels."""
    px = im.load()
    return {(x, y): hexs(px[x, y]) for y in range(im.height) for x in range(im.width) if px[x, y][3] >= 128}


def palette(src, views=(0, 1, 3, 4, 6)):
    """Every colour a donor's views use, with counts — for choosing `hair`."""
    from collections import Counter
    out = Counter()
    ims = frames(src)
    for i in views:
        if i < len(ims):
            out.update(pixels(ims[i]).values())
    return out


def ramp_at(ramp, t):
    """The design ramp (dark -> light, hex) read at t in [0, 1], linearly."""
    t = max(0.0, min(1.0, t))
    pos = t * (len(ramp) - 1)
    k = min(len(ramp) - 2, int(pos))
    f = pos - k
    a, b = rgb(ramp[k]), rgb(ramp[k + 1])
    return hexs(tuple(a[j] + (b[j] - a[j]) * f for j in range(3)))


def tone_map(colours, ramp, snap=False):
    """Donor colours -> design colours by place in the donor's luminance
    range. `snap` takes the nearest ramp entry instead (a design that wants
    exactly its own letters)."""
    cs = sorted({c.lower() for c in colours}, key=lum)
    lo, hi = lum(cs[0]), lum(cs[-1])
    out = {}
    for c in cs:
        t = 0.0 if hi == lo else (lum(c) - lo) / (hi - lo)
        if snap:
            out[c] = ramp[round(t * (len(ramp) - 1))]
        else:
            out[c] = ramp_at(ramp, t)
    return out


def hue_map(colours, hue, sat=None, lift=0.0, gain=1.0):
    """Donor colours -> the same luminances in another hue (the game's ramp
    kept tone for tone). `lift`/`gain` move the luminance (0..1 scale)."""
    out = {}
    for c in colours:
        r, g, b = (v / 255 for v in rgb(c))
        h, l, s = colorsys.rgb_to_hls(r, g, b)
        l2 = max(0.0, min(1.0, lift + gain * l))
        s2 = s if sat is None else sat
        out[c.lower()] = hexs(tuple(v * 255 for v in colorsys.hls_to_rgb(hue % 1.0, l2, s2)))
    return out


def hero_face(sheet, v):
    """The hero's face in a donor drawn on the hero's own head (a helm, a
    hat — img0/108.. and the like): the positions, in the donor's
    coordinates, that are face on the bare head (img0/000) of that view."""
    import astra_studio as A
    base = frames(0)[v]
    im = frames(sheet)[v]
    ox, oy = base.width // 2 - im.width // 2, base.height // 2 - im.height // 2
    keep = A.face_mask(base) | set(A.EYES_BASE.get(v, {}))
    return {(x - ox, y - oy) for (x, y) in keep}


def lay(canvas, base, donor, cmap, shift=(0, 0), protect=(), skin=(), keep=None, where=None):
    """Lay the donor's mapped pixels on a studio canvas (base image coords).

    `cmap`     donor colour -> design colour; only these are laid.
    `skin`     donor colours of its face: a dark of `cmap` touching one is the
               face's outline, not hair, and is left out.
    `protect`  canvas positions never covered (the base eyes).
    `keep`     donor colour -> colour laid as is (a ribbon, a gem).
    `where`    optional predicate(x, y) in donor coords: lay only there.
    Returns the canvas positions laid."""
    ox = base.width // 2 - donor.width // 2 + shift[0]
    oy = base.height // 2 - donor.height // 2 + shift[1]
    dp = pixels(donor)
    skin = {s.lower() for s in skin}
    cmap = {k.lower(): v for k, v in cmap.items()}
    keep = {k.lower(): v for k, v in (keep or {}).items()}
    dark = lum(min(cmap, key=lum)) if cmap else 0
    laid = set()
    for (x, y), col in dp.items():
        if where and not where(x, y):
            continue
        at = (x + ox, y + oy)
        if at in protect:
            continue
        if col in cmap:
            if skin and lum(col) <= dark + 40 and any(dp.get((x + dx, y + dy)) in skin for dx, dy in N4):
                continue
            canvas.put(*at, cmap[col])
            laid.add(at)
        elif col in keep:
            canvas.put(*at, keep[col])
            laid.add(at)
    return laid


def group_map(g):
    """One colour group of a donor: {'cols': [...], 'ramp': [...]} (tone
    map), {'cols', 'hue', 'sat', 'lift', 'gain'} (hue map), or {'map': {...}}."""
    if 'map' in g:
        return {k.lower(): v for k, v in g['map'].items()}
    if 'hue' in g:
        return hue_map(g['cols'], g['hue'], g.get('sat'), g.get('lift', 0.0), g.get('gain', 1.0))
    return tone_map(g['cols'], g['ramp'], g.get('snap', False))


def spec_map(d):
    """A DONOR spec's whole colour map: its groups, the `hair`+`ramp`
    shortcut for a single group, and `cmap` as is."""
    cmap = {}
    if 'hair' in d:
        cmap.update(group_map(dict(d, cols=d['hair'])))
    for g in d.get('groups', ()):
        cmap.update(group_map(g))
    cmap.update({k.lower(): v for k, v in (d.get('cmap') or {}).items()})
    return cmap


def head(design_donor, view, base_im, canvas, protect=()):
    """Apply a design's DONOR spec to one head view (astra_studio/ren_studio).

    DONOR = {'sheet': 1, 'hair': [...donor colours], 'ramp': [...hex],
             'groups': [{'cols': [...], 'ramp': [...]}, ...],
             'skin': [...], 'keep': {...}, 'views': {base view: donor view},
             'shift': {view: (dx, dy)} or (dx, dy), 'snap': False}
    or a list of such (a hat over hair)."""
    specs = design_donor if isinstance(design_donor, list) else [design_donor]
    laid = set()
    for d in specs:
        views = d.get('views')
        v = views.get(view, view) if views else view
        if v is None:
            continue
        ims = frames(d['sheet'])
        if v >= len(ims) or ims[v].size == (1, 1):
            continue
        sh = d.get('shift', (0, 0))
        if isinstance(sh, dict):
            sh = sh.get(view, (0, 0))
        laid |= lay(canvas, base_im, ims[v], spec_map(d), sh, protect, d.get('skin', ()), d.get('keep'),
                    d.get('where', {}).get(view) if isinstance(d.get('where'), dict) else None)
    return laid


def colours_of(design_donor):
    """Every design colour a DONOR spec can lay (for the rig's loose set and
    the finishing pass, which leaves the game's shading alone)."""
    specs = design_donor if isinstance(design_donor, list) else [design_donor]
    out = set()
    for d in specs:
        out |= set(spec_map(d).values())
        out |= set((d.get('keep') or {}).values())
    return out


def hat_extent(d, v):
    """A hat donor's own geometry in view `v` (donor image coords): the
    centre of its widest row (the brim), that row, and its lowest row."""
    im = frames(d['sheet'])[v]
    cmap = spec_map(d)
    face = hero_face(d['sheet'], v) if d.get('face_cols') else set()
    fc = {c.lower() for c in d.get('face_cols', ())}
    pts = [p for p, c in pixels(im).items() if c in cmap and not (p in face and c in fc)]
    rows = {}
    for x, y in pts:
        rows.setdefault(y, []).append(x)
    wide = max(rows, key=lambda y: (max(rows[y]) - min(rows[y]), y))
    span = max(rows[wide]) - min(rows[wide])
    cx = (min(rows[wide]) + max(rows[wide])) / 2.0
    # the hat's lowest row: going down from the brim, the last row still most
    # of the brim's width. Below it the donor is the head under the hat — its
    # hair in the hat's shadow shares the hat's colours, and is not hat.
    bottom = wide
    while bottom + 1 in rows and max(rows[bottom + 1]) - min(rows[bottom + 1]) >= 0.6 * span:
        bottom += 1
    # and never past the brow of the face it was drawn on (a 3/4 view's
    # shaded back hair is as wide as the brim)
    if d['sheet'] != 0 and len(frames(d['sheet'])) == 7:
        f = hero_face(d['sheet'], v)
        if f:
            bottom = max(wide, min(bottom, min(y for _, y in f) + 1))
    return cx, wide, bottom


HERO_TOP = 1   # the bare hero head's first row (img0/000, every view)


def fit_hat(canvas, base_im, d, view, brow, cx, clearable, protect=(), head_top=None):
    """Seat a hat donor on a head that is not the hero's (렌): its lowest row
    on `brow`, its brim centred on `cx` (base coords). The game draws its hats
    on the hero, whose skull rises HERO_TOP..brim; given the target head's
    `head_top`, the crown is shortened by the ratio of the two skulls —
    rows dropped evenly from the crown's middle, never its rounded top or the
    brim — so the hat encloses the head instead of standing on it. Then the
    head's own pixels in `clearable` (hair, ornaments) above the brim and not
    under the hat are cleared. Returns the positions laid."""
    views = d.get('views')
    v = views.get(view) if views else view   # a view the design leaves out gets no hat
    if v is None:
        return set()
    im = frames(d['sheet'])[v]
    dcx, dwide, dbottom = hat_extent(d, v)
    cmap = spec_map(d)
    keep = {k.lower(): c for k, c in (d.get('keep') or {}).items()}
    face = hero_face(d['sheet'], v) if d.get('face_cols') else set()
    fc = {c.lower() for c in d.get('face_cols', ())}
    # nothing below the hat; and colours the hat shares with the face it was
    # drawn on are face there
    pts = {p: c for p, c in pixels(im).items()
           if p[1] <= dbottom and (c in cmap or c in keep) and not (p in face and c in fc)}
    dtop = min(y for _, y in pts)
    drop = set()
    if head_top is not None:
        hero_brow = dbottom + (frames(0)[v].height // 2 - im.height // 2)
        ratio = (brow - head_top) / max(1, hero_brow - HERO_TOP)
        crown = list(range(dtop + 2, dwide - 1))
        n = min(len(crown) // 2, max(0, round((dbottom - dtop + 1) * (1 - ratio))))
        n += d.get('squash', {}).get(view, 0) if isinstance(d.get('squash'), dict) else 0
        n = max(0, min(len(crown) // 2, n))
        if n:
            step = len(crown) / n
            drop = {crown[int(step * (k + 0.5))] for k in range(n)}

    def ny(y):
        return y - sum(1 for r in drop if r < y)

    nudge = d.get('nudge', {}).get(view, (0, 0)) if isinstance(d.get('nudge'), dict) else (0, 0)
    ox = round(cx - dcx) + nudge[0]
    oy = brow - ny(dbottom) + nudge[1]
    laid = set()
    for (x, y), c in pts.items():
        if y in drop:
            continue
        at = (x + ox, ny(y) + oy)
        if at in protect:
            continue
        canvas.put(*at, cmap.get(c, keep.get(c)))
        laid.add(at)
    top = ny(dwide) + oy
    for (x, y) in clearable:
        if y < top and (x, y) not in laid:
            canvas.put(x, y, None)
    return laid
