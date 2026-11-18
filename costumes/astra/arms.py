"""Weapons for the astra avatars, drawn as vectors along each sheet piece's
own axis (astra_studio.WEAPON_AXES) and shaded the way the game's own
weapons are: a dark outline (added after the downsample, so it is always
one pixel and never lost), a lit upper edge, a darker lower edge, metal
fittings and a stone.

A design names a style and a palette per class:

    WEAPONS = {
        'sword':  {'style': 'katana', 'pal': {...}},   # warrior
        'dagger': {'style': 'kris',   'pal': {...}},   # rogue
        'gun':    {'style': 'flintlock', 'pal': {...}},  # gunner
        'spear':  {'style': 'yari',   'pal': {...}},   # knight
        'shield': {'style': 'kite',   'pal': {...}},   # knight's off hand
    }

Palette keys (any missing falls back to a neighbour): out (outline), blade,
shine, edge, fuller, guard, guard_hi, grip, grip_hi, gem, shaft, shaft_hi,
tassel, body, body_hi, metal, metal_hi, accent.

Coordinates: t runs from the hand end (0) to the far end (c.length), s
across it; for guns s > 0 is the side the grip hangs on (`c.down`).
"""
import math


def _p(P, *keys):
    for k in keys:
        if k in P:
            return P[k]
    raise KeyError(keys)


# ---------------------------------------------------------------- blades
def _blade(c, t0, t1, hw, off, P, fuller=False, tip=0.16, back_edge=False):
    """A blade from t0 to t1: half-width hw(u), centre offset off(u), u in
    0..1 along it; the last `tip` of it tapers to the point."""
    n = 24
    top, bot = [], []
    for k in range(n + 1):
        u = k / n
        t = t0 + (t1 - t0) * u
        w = hw(u)
        if u > 1 - tip:
            w *= max(0.0, (1 - u) / tip)
        o = off(u)
        top.append((t, o - w))
        bot.append((t, o + w))
    c.poly(top + bot[::-1], _p(P, 'blade'))
    # the lit upper edge
    shine = []
    for k in range(n + 1):
        u = k / n
        t = t0 + (t1 - t0) * u
        w = hw(u) * (max(0.0, (1 - u) / tip) if u > 1 - tip else 1)
        o = off(u)
        shine.append((t, o - w))
    inner = [(t, s + min(0.9, abs(s - off((t - t0) / (t1 - t0))) * 0.9)) for t, s in shine]
    c.poly(shine + inner[::-1], _p(P, 'shine'))
    # the cutting edge below
    edge = []
    for k in range(n + 1):
        u = k / n
        t = t0 + (t1 - t0) * u
        w = hw(u) * (max(0.0, (1 - u) / tip) if u > 1 - tip else 1)
        o = off(u)
        edge.append((t, o + w))
    e_in = [(t, s - min(0.75, abs(s - off((t - t0) / (t1 - t0))) * 0.8)) for t, s in edge]
    c.poly(edge + e_in[::-1], _p(P, 'edge', 'blade'))
    if fuller:
        pts = [(t0 + (t1 - t0) * u, off(u) - 0.35) for u in (0.04, 0.7)]
        pts += [(t0 + (t1 - t0) * u, off(u) + 0.35) for u in (0.7, 0.04)]
        c.poly(pts, _p(P, 'fuller', 'edge'))


def _grip(c, t0, t1, w, P, wraps=True):
    c.band(t0, t1, -w, w, _p(P, 'grip'))
    if wraps:
        k = t0 + 0.4
        while k < t1 - 0.6:
            c.band(k, k + 0.9, -w, w * 0.1, _p(P, 'grip_hi', 'grip'))
            k += 2.2


def _pommel(c, t, r, P):
    c.circle(t, 0, r, _p(P, 'guard'))
    c.circle(t - r * 0.2, -r * 0.3, r * 0.5, _p(P, 'guard_hi', 'guard'))


def _guard(c, t, style, P, size=1.0):
    g, gh = _p(P, 'guard'), _p(P, 'guard_hi', 'guard')
    if style == 'cross':
        c.band(t, t + 1.6 * size, -3.4 * size, 3.4 * size, g)
        c.band(t, t + 0.8 * size, -3.4 * size, 3.4 * size, gh)
    elif style == 'tsuba':
        c.band(t - 0.2, t + 1.4, -2.6 * size, 2.6 * size, g)
        c.band(t - 0.2, t + 0.6, -2.6 * size, 2.6 * size, gh)
    elif style == 'wings':
        c.poly([(t, -1), (t - 2.6 * size, -4.2 * size), (t + 0.6, -3.4 * size), (t + 1.8, -1),
                (t + 1.8, 1), (t + 0.6, 3.4 * size), (t - 2.6 * size, 4.2 * size), (t, 1)], g)
        c.poly([(t, -1), (t - 2.2 * size, -3.6 * size), (t + 0.4, -3.0 * size), (t + 1.0, -1)], gh)
    elif style == 'flame':
        c.poly([(t, -1.2), (t - 1.5, -4.5 * size), (t + 0.6, -2.6 * size), (t + 0.9, -4.0 * size),
                (t + 2.0, -1.2), (t + 2.0, 1.2), (t + 0.9, 4.0 * size), (t + 0.6, 2.6 * size),
                (t - 1.5, 4.5 * size), (t, 1.2)], g)
        c.poly([(t, -1.2), (t - 1.2, -3.8 * size), (t + 0.5, -2.2 * size), (t + 0.9, -1.2)], gh)
    elif style == 'disc':
        c.circle(t + 0.8, 0, 2.6 * size, g)
        c.circle(t + 0.4, -0.8, 1.2 * size, gh)
    elif style == 'knuckle':
        # a sabre's D-guard sweeping back over the grip on the lower side
        c.band(t, t + 1.4, -2.2, 3.0, g)
        c.poly([(t + 0.2, 2.2), (-0.6, 3.4), (-0.6, 2.4), (t - 0.4, 1.4)], g)
        c.band(t, t + 0.7, -2.2, 3.0, gh)
    elif style == 'bone':
        c.poly([(t, -1.6), (t - 0.8, -3.2), (t + 2.2, -2.2), (t + 2.2, 2.2), (t - 0.8, 3.2), (t, 1.6)], g)
        c.band(t + 0.2, t + 1.0, -2.0, 2.0, gh)


def sword(c, style, P):
    L = c.length
    gem = P.get('gem')
    if style == 'katana':
        gl = 0.22 * L
        _pommel(c, 0.4, 1.1, P)
        _grip(c, 0.6, gl, 1.0, P)
        _guard(c, gl, 'tsuba', P, 0.9)
        _blade(c, gl + 1.2, L, lambda u: 1.45, lambda u: -1.4 * u * u, P, tip=0.12)
    elif style == 'long':
        gl = 0.18 * L
        _pommel(c, 0.6, 1.5, P)
        _grip(c, 1.4, gl, 1.0, P)
        _guard(c, gl, 'cross', P)
        _blade(c, gl + 1.4, L, lambda u: 2.0 - 0.4 * u, lambda u: 0, P, fuller=True, tip=0.14)
    elif style == 'broad':
        gl = 0.17 * L
        _pommel(c, 0.6, 1.7, P)
        _grip(c, 1.4, gl, 1.1, P)
        _guard(c, gl, 'wings', P, 1.1)
        _blade(c, gl + 1.6, L, lambda u: 2.9 - 0.3 * u, lambda u: 0, P, fuller=True, tip=0.12)
    elif style == 'curved':
        gl = 0.17 * L
        _grip(c, 0, gl, 1.2, P)
        _guard(c, gl, 'bone', P)
        _blade(c, gl + 1.6, L, lambda u: 1.8 + 1.4 * math.sin(math.pi * min(1, u * 1.1)),
               lambda u: 2.4 * u * u, P, tip=0.2)
    elif style == 'saber':
        gl = 0.18 * L
        _pommel(c, 0.4, 1.2, P)
        _grip(c, 0.6, gl, 1.0, P)
        _guard(c, gl, 'knuckle', P)
        _blade(c, gl + 1.4, L, lambda u: 1.6, lambda u: -1.8 * u * u, P, tip=0.18)
    elif style == 'flame':
        gl = 0.2 * L
        _pommel(c, 0.4, 1.3, P)
        _grip(c, 0.6, gl, 1.0, P)
        _guard(c, gl, 'flame', P)
        _blade(c, gl + 1.6, L, lambda u: 1.9 + 0.55 * math.sin(u * 14), lambda u: -0.6 * u * u, P, tip=0.14)
    elif style == 'dao':
        gl = 0.2 * L
        _pommel(c, 0.4, 1.2, P)
        _grip(c, 0.6, gl, 1.0, P)
        _guard(c, gl, 'disc', P, 0.9)
        _blade(c, gl + 1.6, L, lambda u: 1.5 + 1.2 * u, lambda u: 0.9 * u * u, P, tip=0.2)
    elif style == 'cleaver':
        gl = 0.22 * L
        _grip(c, 0, gl, 1.2, P)
        _guard(c, gl, 'bone', P, 0.9)
        _blade(c, gl + 1.4, L, lambda u: 2.4 + 0.9 * u, lambda u: 0.6 * u, P, tip=0.07)
    elif style == 'ice':
        gl = 0.18 * L
        _pommel(c, 0.4, 1.3, P)
        _grip(c, 0.6, gl, 1.0, P)
        _guard(c, gl, 'wings', P, 0.9)
        _blade(c, gl + 1.4, L, lambda u: 2.2 - 0.6 * u + 0.4 * (int(u * 7) % 2), lambda u: 0, P, tip=0.2)
    else:
        raise ValueError(style)
    if gem:
        c.circle(gl + 0.9, 0, 0.9, gem)


def dagger(c, style, P):
    L = c.length
    gem = P.get('gem')
    gl = 0.34 * L
    if style == 'kris':
        _grip(c, 0, gl, 1.0, P)
        _guard(c, gl, 'flame', P, 0.6)
        _blade(c, gl + 1.0, L, lambda u: 1.5 + 0.45 * math.sin(u * 11), lambda u: 0, P, tip=0.25)
    elif style == 'tanto':
        _grip(c, 0, gl, 1.0, P)
        _guard(c, gl, 'tsuba', P, 0.6)
        _blade(c, gl + 0.8, L, lambda u: 1.3, lambda u: -0.4 * u * u, P, tip=0.2)
    elif style == 'stiletto':
        _pommel(c, 0.3, 1.0, P)
        _grip(c, 0.4, gl, 0.9, P)
        _guard(c, gl, 'cross', P, 0.6)
        _blade(c, gl + 1.0, L, lambda u: 1.1, lambda u: 0, P, tip=0.4)
    elif style == 'claw':
        _grip(c, 0, gl, 1.1, P)
        _guard(c, gl, 'bone', P, 0.7)
        _blade(c, gl + 1.0, L, lambda u: 1.6 + 0.6 * u, lambda u: 1.8 * u * u, P, tip=0.3)
    elif style == 'leaf':
        _pommel(c, 0.3, 1.0, P)
        _grip(c, 0.4, gl, 0.9, P)
        _guard(c, gl, 'wings', P, 0.6)
        _blade(c, gl + 1.0, L, lambda u: 1.2 + 0.9 * math.sin(math.pi * u), lambda u: 0, P, tip=0.3)
    else:
        raise ValueError(style)
    if gem:
        c.circle(gl + 0.6, 0, 0.75, gem)


# ---------------------------------------------------------------- spears
def spear(c, style, P):
    L = c.length
    if c.short:
        # The foreshortened piece: the shaft only, with its ferrule.
        c.band(0, L, -0.75, 0.75, _p(P, 'shaft'))
        c.band(0, L, -0.75, -0.15, _p(P, 'shaft_hi', 'shaft'))
        c.band(L - 2.5, L, -1.1, 1.1, _p(P, 'metal', 'guard'))
        return
    head = {'yari': 0.2, 'lance': 0.26, 'leaf': 0.2, 'glaive': 0.26, 'halberd': 0.24,
            'trident': 0.22, 'flame': 0.22, 'crescent': 0.24}[style] * L
    h0 = L - head
    # shaft and butt cap
    c.band(1.5, h0, -0.85, 0.85, _p(P, 'shaft'))
    c.band(1.5, h0, -0.85, -0.2, _p(P, 'shaft_hi', 'shaft'))
    k = 6.0
    while k < h0 - 2:
        c.band(k, k + 1.0, -0.85, 0.85, _p(P, 'accent', 'guard'))
        k += 7.0
    c.band(0, 2.2, -1.2, 1.2, _p(P, 'metal', 'guard'))
    # collar at the head
    c.band(h0 - 1.6, h0 + 0.6, -1.5, 1.5, _p(P, 'metal', 'guard'))
    c.band(h0 - 1.6, h0 + 0.6, -1.5, -0.6, _p(P, 'metal_hi', 'guard_hi', 'guard'))
    if P.get('tassel'):
        c.poly([(h0 - 0.6, 0.8), (h0 - 4.0, 4.6), (h0 - 1.6, 4.2), (h0 - 0.2, 1.2)], P['tassel'])
        c.poly([(h0 - 1.6, 1.0), (h0 - 6.0, 3.6), (h0 - 4.4, 2.4)], P['tassel'])
    m, mh = _p(P, 'blade'), _p(P, 'shine', 'blade')
    t0 = h0 + 0.6
    if style in ('yari', 'lance', 'leaf', 'flame'):
        if style == 'yari':
            hw = lambda u: 1.6 * (1 - u) + 0.4
        elif style == 'lance':
            hw = lambda u: 2.6 * (1 - u) ** 1.2 + 0.2
        elif style == 'leaf':
            hw = lambda u: 0.8 + 2.0 * math.sin(math.pi * min(1.0, u * 1.15))
        else:
            hw = lambda u: 1.4 + 0.9 * math.sin(u * 13) * (1 - u)
        _blade(c, t0, L, hw, lambda u: 0, P, tip=0.3 if style != 'lance' else 0.5)
    elif style == 'glaive':
        _blade(c, t0, L, lambda u: 1.4 + 1.5 * math.sin(math.pi * u), lambda u: 1.6 * math.sin(math.pi * u * 0.9),
               P, tip=0.25)
    elif style == 'halberd':
        _blade(c, t0, L, lambda u: 0.9, lambda u: 0, P, tip=0.35)
        a = t0 + head * 0.15
        c.poly([(a, 0.6), (a + head * 0.15, 0.6), (a + head * 0.45, 4.6), (a - head * 0.1, 4.2)], m)
        c.poly([(a, 0.6), (a + head * 0.15, 0.6), (a + head * 0.3, 2.4), (a + head * 0.02, 2.4)], mh)
        c.poly([(a + head * 0.05, -0.6), (a + head * 0.2, -0.6), (a + head * 0.1, -3.0)], m)
    elif style == 'trident':
        for s in (-2.2, 0, 2.2):
            _blade(c, t0 + (0 if s == 0 else head * 0.15), L - (0 if s == 0 else head * 0.12),
                   lambda u: 0.75, lambda u, s=s: s, P, tip=0.35)
        c.band(t0, t0 + head * 0.2, -2.8, 2.8, _p(P, 'metal', 'guard'))
    elif style == 'crescent':
        _blade(c, t0, L, lambda u: 0.9, lambda u: 0, P, tip=0.35)
        a = t0 + head * 0.1
        c.poly([(a, -0.6), (a + head * 0.6, -1.2), (a + head * 0.75, -4.4), (a + head * 0.3, -2.6), (a, -2.0)], m)
        c.poly([(a, 0.6), (a + head * 0.6, 1.2), (a + head * 0.75, 4.4), (a + head * 0.3, 2.6), (a, 2.0)], m)
    if P.get('gem'):
        c.circle(h0 - 0.5, 0, 0.9, P['gem'])


# ---------------------------------------------------------------- guns
def gun(c, style, P):
    """Grip at t = 0 (in the hand), muzzle at t = L. The grip hangs to the
    `c.down` side; for the piece seen end-on (down == 0) it sits behind."""
    L = c.length
    d = c.down or 0
    metal, metal_hi = _p(P, 'barrel', 'blade'), _p(P, 'barrel_hi', 'shine')
    body, body_hi = _p(P, 'body'), _p(P, 'body_hi', 'body')
    grip, grip_hi = _p(P, 'grip'), _p(P, 'grip_hi', 'grip')
    acc = _p(P, 'accent', 'guard')
    if style == 'flintlock':
        bw, rw = 0.95, 1.6
        r0, r1, b0 = 0.18, 0.5, 0.42
    elif style == 'revolver':
        bw, rw = 1.1, 2.0
        r0, r1, b0 = 0.2, 0.55, 0.5
    elif style == 'cannon':
        bw, rw = 1.7, 2.1
        r0, r1, b0 = 0.18, 0.45, 0.4
    elif style == 'double':
        bw, rw = 1.6, 1.9
        r0, r1, b0 = 0.18, 0.45, 0.4
    else:   # 'pistol'
        bw, rw = 1.15, 1.8
        r0, r1, b0 = 0.18, 0.5, 0.45
    # grip
    if d:
        g0, g1 = 0.0, r0 * L + 1.2
        c.poly([(g0 - 0.6, -0.6 * d), (g1, -0.6 * d), (g1 - 0.4, 2.2 * d), (g0 + 0.8, 4.4 * d),
                (g0 - 1.6, 4.0 * d)], grip)
        c.poly([(g0 - 0.6, -0.6 * d), (g1, -0.6 * d), (g1 - 0.4, 0.8 * d), (g0 - 0.6, 1.4 * d)], grip_hi)
        # trigger guard
        c.poly([(g1 - 0.2, 0.9 * d), (g1 + 2.0, 0.9 * d), (g1 + 1.4, 2.6 * d), (g1 - 0.2, 2.2 * d)], acc)
    else:
        c.band(-0.5, r0 * L + 1.0, -2.0, 2.0, grip)
        c.band(-0.5, r0 * L + 1.0, -2.0, -0.6, grip_hi)
    # receiver
    c.band(r0 * L, r1 * L, -rw, rw, body)
    c.band(r0 * L, r1 * L, -rw, -rw + 0.9, body_hi)
    if style == 'revolver':
        c.circle((r0 + r1) * 0.5 * L, 0, rw + 0.4, body)
        c.circle((r0 + r1) * 0.5 * L - 0.4, -0.8, 0.9, body_hi)
    # barrel
    if style == 'double':
        c.band(b0 * L, L - 0.4, -bw, -0.1, metal)
        c.band(b0 * L, L - 0.4, 0.1, bw, metal)
        c.band(b0 * L, L - 0.4, -bw, -bw + 0.6, metal_hi)
    else:
        c.band(b0 * L, L - 0.4, -bw, bw, metal)
        c.band(b0 * L, L - 0.4, -bw, -bw + 0.7, metal_hi)
    # muzzle and fittings
    c.band(L - 1.4, L, -bw - 0.35, bw + 0.35, acc)
    c.band(b0 * L - 0.2, b0 * L + 1.0, -bw - 0.3, bw + 0.3, acc)
    if style == 'flintlock':
        c.band(r0 * L + 0.6, r0 * L + 2.0, -rw - 1.2, -rw, acc)   # the hammer
    if P.get('gem'):
        c.circle((r0 + r1) * 0.5 * L, 0, 0.8, P['gem'])


# ---------------------------------------------------------------- shields
def shield(style, P, w=14, h=20):
    """A shield face as grid rows (the knight's img6, 14x20): rim, field,
    a darker lower half, and an emblem. Letters: k rim outline, R rim,
    r rim shade, F field, f field shade, E emblem, e emblem shade."""
    cx = (w - 1) / 2
    rows = []
    for y in range(h):
        row = ''
        for x in range(w):
            u = (x - cx) / (w / 2)
            v = y / (h - 1)
            if style == 'kite':
                half = 1.0 if v < 0.45 else max(0.0, 1.0 - (v - 0.45) / 0.58)
                inside = abs(u) <= half * 0.98 and v >= 0.02
            elif style == 'round':
                inside = u * u + ((v - 0.5) * 2) ** 2 * (w / h) ** -2 * 0.55 <= 1.0
            else:   # 'heater'
                half = 1.0 if v < 0.55 else max(0.0, 1.0 - ((v - 0.55) / 0.48) ** 1.6)
                inside = abs(u) <= half * 0.98
            row += 'F' if inside else ' '
        rows.append(row)
    grid = [list(r) for r in rows]

    def on(x, y):
        return 0 <= y < h and 0 <= x < w and rows[y][x] == 'F'
    for y in range(h):
        for x in range(w):
            if not on(x, y):
                continue
            if not (on(x - 1, y) and on(x + 1, y) and on(x, y - 1) and on(x, y + 1)):
                grid[y][x] = 'k'
            elif not (on(x - 2, y) and on(x + 2, y) and on(x, y - 2) and on(x, y + 2)):
                grid[y][x] = 'R' if x <= cx else 'r'
            elif y > h * 0.55 or x > cx + 1:
                grid[y][x] = 'f'
    # emblem
    emb = P.get('emblem', 'cross')
    ey = int(h * 0.42)
    pts = []
    if emb == 'cross':
        pts = [(int(cx), y) for y in range(ey - 4, ey + 5)] + [(int(cx) + 1, y) for y in range(ey - 4, ey + 5)]
        pts += [(x, ey - 1) for x in range(int(cx) - 3, int(cx) + 5)] + [(x, ey) for x in range(int(cx) - 3, int(cx) + 5)]
    elif emb == 'sun':
        for y in range(ey - 3, ey + 4):
            for x in range(int(cx) - 3, int(cx) + 5):
                if (x - cx) ** 2 + (y - ey) ** 2 <= 6.5 or (x - cx == 0.5 or x - cx == -0.5) or y == ey:
                    pts.append((x, y))
    elif emb == 'diamond':
        for y in range(ey - 4, ey + 5):
            for x in range(int(cx) - 4, int(cx) + 6):
                if abs(x - cx) + abs(y - ey) <= 3.6:
                    pts.append((x, y))
    elif emb == 'fang':
        pts = [(int(cx) - 2, ey - 3), (int(cx) - 2, ey - 2), (int(cx) - 1, ey - 1), (int(cx) - 1, ey),
               (int(cx) + 2, ey - 3), (int(cx) + 2, ey - 2), (int(cx) + 1, ey - 1), (int(cx) + 1, ey),
               (int(cx), ey + 2), (int(cx) + 1, ey + 2), (int(cx), ey + 3)]
    for (x, y) in pts:
        if 0 <= y < h and 0 <= x < w and grid[y][x] in 'Ff':
            grid[y][x] = 'E' if x <= cx else 'e'
    return [''.join(r) for r in grid]
