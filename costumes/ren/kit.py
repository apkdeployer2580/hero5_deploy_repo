"""Shared drawing for 렌's outfits: long hair as she would be hand-pixelled.

Every function returns a grid ({'at', 'rows'}) in the letters the designs
share for hair:

    O  outline        b  shade        y  hair        Y  light        L  shine

A design maps those letters to its own ramp in PAL. Nothing here is a fill:
a lock is a strand two to four pixels wide with an outline on both sides, a
shaded side away from the light (upper left) and a shine where the light
catches it; a curtain is strands side by side, separated by shade lines that
part every three pixels, with jagged tips.
"""


def _grid(at, cells):
    if not cells:
        return {'at': at, 'rows': []}
    xs = [x for x, _ in cells]
    ys = [y for _, y in cells]
    x0, y0 = min(xs), min(ys)
    w, h = max(xs) - x0 + 1, max(ys) - y0 + 1
    rows = [[' '] * w for _ in range(h)]
    for (x, y), ch in cells.items():
        rows[y - y0][x - x0] = ch
    return {'at': (at[0] + x0, at[1] + y0), 'rows': [''.join(r).rstrip() for r in rows]}


def lock(path, shine=(), light_side='left', tip=True):
    """One lock of hair. `path` is [(y, x_left, width)] in head-frame pixels,
    top to bottom; each row is outlined both ends. `shine` lists rows where
    the light catches it."""
    cells = {}
    for y, x, w in path:
        inner = max(0, w - 2)
        # lit first, then mid, the far edge in shade: Y y b / L Y y b ...
        if inner == 1:
            body = ['L' if y in shine else 'y']
        elif inner == 2:
            body = ['L' if y in shine else 'Y', 'y']
        else:
            body = ['L' if y in shine else 'Y'] + ['Y' if y in shine else 'y'] * (inner - 2) + ['b']
            if inner >= 4:
                body[1] = 'Y'
        if light_side == 'right':
            body = body[::-1]
        row = ['O'] + body + (['O'] if w >= 2 else [])
        for i, ch in enumerate(row[:w]):
            cells[(x + i, y)] = ch
    if tip and path:
        y, x, w = path[-1]
        cells[(x + w // 2, y + 1)] = 'O'
    return _grid((0, 0), cells)


def curtain(rows, shine=(), part=5, tips=(2, 0, 3, 1), shade_from=None, taper=True):
    """A mass of hair seen from behind: `rows` is [(y, x_left, x_right)].

    Painted the way the game paints a head of hair (img0/027's back): the
    mass falls in clumps about `part` pixels wide whose partings sway as they
    fall; each clump is lit on its left and shaded on its right, a dark
    parting between neighbours; a sheen crosses the crown (the `shine` rows,
    its lower edge waving clump by clump); the mass darkens toward its ends
    (from `shade_from`, and through its lowest quarter) and narrows below its
    middle (`taper`), so it hangs rather than standing like a board. Each
    clump runs on past the last row by its own length from `tips` and ends
    in a point. Letters O b y Y L as `lock`."""
    import math
    rows = sorted(rows)
    y0, y1 = rows[0][0], rows[-1][0]
    H = max(1, y1 - y0)
    span = {}
    for (y, l, r) in rows:
        if taper:
            t = (y - y0) / H
            if t > 0.5:
                k = int((t - 0.5) / 0.5 * (r - l) * 0.16 + 0.5)
                l, r = l + k, r - k
        span[y] = (l, r)
    width = max(r - l for l, r in span.values())
    n = max(2, round(width / part))
    shine = set(shine)

    def bounds(y):
        return [0.0] + [k / n + 0.045 * math.sin((y - y0) / 3.5 + k * 2.1) for k in range(1, n)] + [1.0]

    cells = {}
    for y, (l, r) in span.items():
        b = bounds(y)
        for x in range(l, r + 1):
            sx = (x + 0.5 - l) / max(1, r + 1 - l)
            j = max(0, min(n - 1, sum(1 for bb in b[1:-1] if bb <= sx)))
            c = (sx - b[j]) / max(1e-6, b[j + 1] - b[j])
            near = min(abs(x + 0.5 - (l + bb * (r + 1 - l))) for bb in b[1:-1]) if n > 1 else 9
            v = (y - y0) / H
            T = 2.3 - 0.55 * (2 * sx - 1)
            T += 0.65 if c < 0.4 else (-0.6 if c > 0.72 else 0.0)
            band = y in shine or 0.10 < v < 0.22 + 0.05 * math.sin(j * 1.7 + 0.6)
            if band:
                T += 1.25
            if v > 0.75:
                T -= 0.7
            if shade_from is not None and y >= shade_from:
                T -= 0.45
            if near < 0.55 and not band:
                T = min(T, 1.0)
            cells[(x, y)] = 'bbyYL'[max(1, min(4, int(round(T))))]
    # ragged ends: every clump runs on by its own length and comes to a point
    l, r = span[y1]
    b = bounds(y1)
    for j in range(n):
        a, z = l + b[j] * (r + 1 - l), l + b[j + 1] * (r + 1 - l)
        extra = tips[j % len(tips)]
        for d in range(1, extra + 1):
            shrink = (z - a) * d / (extra + 1) / 2
            for x in range(int(math.floor(a + shrink + 0.5)), int(math.ceil(z - shrink - 0.5))):
                cells[(x, y1 + d)] = 'y' if d < extra else 'b'
    # outline the sides and the ends (the top joins the head's own hair)
    out = dict(cells)
    for (x, y) in cells:
        if y == y0:
            if (x - 1, y) not in cells or (x + 1, y) not in cells:
                out[(x, y)] = 'O'
            continue
        if any((x + dx, y + dy) not in cells for dx, dy in ((1, 0), (-1, 0), (0, 1))):
            out[(x, y)] = 'O'
    return _grid((0, 0), out)


def wave(y0, y1, x, w, amp=1, period=6, phase=0, grow=0.0):
    """A path for `lock`: rows y0..y1 at x, swaying `amp` px every `period`
    rows; `grow` widens it (px per row, may be negative)."""
    out = []
    for k, y in enumerate(range(y0, y1 + 1)):
        s = ((k + phase) // (period // 2)) % 2
        dx = amp if s else 0
        ww = max(2, round(w + grow * k))
        out.append((y, x + dx, ww))
    return out


def wing(root, wrist, primaries, width=1.6, covert=2.2, mirror=False):
    """A feathered wing as a pixel artist builds one: long flight feathers
    hanging from the wrist, each its own outlined blade lit along its upper
    edge, laid outermost first so every inner feather overlaps the one
    beyond it; then the coverts — the white arm from shoulder (`root`) to
    `wrist` — over their roots. `primaries` is [(degrees out from straight
    down, length)]. Letters: F outline, f shade, m feather, l light,
    W white. `mirror` draws the right wing (out = +x)."""
    from math import cos, hypot, radians, sin
    cells = {}
    s = 1 if mirror else -1

    def blade(p0, p1, half, fill_light, fill, tip_taper=True):
        (x0, y0), (x1, y1) = p0, p1
        L = hypot(x1 - x0, y1 - y0) or 1.0
        ux, uy = (x1 - x0) / L, (y1 - y0) / L
        nx, ny = -uy, ux
        xs = range(int(min(x0, x1) - half - 2), int(max(x0, x1) + half + 3))
        ys = range(int(min(y0, y1) - half - 2), int(max(y0, y1) + half + 3))
        inside = {}
        for y in ys:
            for x in xs:
                px, py = x + 0.5 - x0, y + 0.5 - y0
                t = px * ux + py * uy
                d = px * nx + py * ny
                if t < -0.5 or t > L + 0.5:
                    continue
                w = half * (1.0 - 0.75 * max(0.0, (t - 0.55 * L) / (0.45 * L))) if tip_taper else half
                if abs(d) <= w + 0.25:
                    inside[(x, y)] = d
        for (x, y), d in inside.items():
            edge = any((x + dx, y + dy) not in inside for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
            # the light comes from the upper left: the blade's side facing it is lit
            lit = -d * (nx + ny) > 0
            cells[(x, y)] = 'F' if edge else (fill_light if lit else fill)

    for ang, length in sorted(primaries, key=lambda a: -a[0]):
        a = radians(ang)
        tip = (wrist[0] + s * sin(a) * length, wrist[1] + cos(a) * length)
        blade(wrist, tip, width, 'l', 'm')
    blade(root, wrist, covert, 'W', 'l', tip_taper=False)
    return _grid((0, 0), cells)


def merge(*grids):
    """Several grids as one list, drawn in order."""
    return [g for g in grids if g['rows']]
