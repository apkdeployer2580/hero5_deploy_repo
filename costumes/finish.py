"""The finishing pass: what we drew, rendered the way the game's own art is.

Side by side with the archive's sprites, the costumes drawn here read as
icons — near-black strokes round every shape, two or three flat tones per
material, blacks that are just black. The game's art has none of that: its
darkest tones sit at 25-40 luminance and are tinted (#211c31, a violet; the
spirit's boots run #211c18 -> #bdb2ad), an outline is the darkest tone of
the material it bounds and a step lighter where the light falls, and a plane
turns through several tones from its lit upper left to its shaded lower
right.

`finish_canvas(canvas, base, drawn, noform)` applies that to a studio canvas
(costumes/ren_studio.py, costumes/astra_studio.py), and only to the pixels
whose colour is one of OURS (`drawn`: a design's grid colours, its painted
skirts). Recolours of the game's own pixels (hair, arms, legs) keep the
game's shading untouched; base pixels kept as they are ('.') are read as
neighbours, never written. Steps:

1. Lift. A near-black of ours rises to the game's floor, its hue kept (a
   grey is tinted toward the game's violet-black).
2. Form. Every flat area of ours (one colour, connected, eight pixels or
   more, not hair) is shaded across its whole extent under light from the
   upper left — a diagonal gradient of up to two ramp steps either way,
   plus a lit rim on its upper-left edge and a darker one lower right.
3. Outline. A stroke of ours (a dark pixel on the silhouette or between
   materials) becomes the darkest tone of the darkest material of ours it
   bounds; a step lighter on the lit side. A stroke that bounds only the
   game's pixels (the face under a hat brim) is just lifted.

Ramps come from the design's own colours grouped by hue; each family is
extended two tones past either end, shifted toward blue-violet as it darkens
and toward yellow as it lightens. Nothing is random and nothing depends on
frame order, so neighbouring frames of one costume shade alike.
"""
import colorsys
from collections import Counter, deque

N4 = ((1, 0), (-1, 0), (0, 1), (0, -1))
DARK_FLOOR = 24     # the game's darkest tones sit at 25-40 (#211c31 is 31)
OUTLINE_AT = 44     # a drawn pixel this dark is a stroke
LIGHT = (-0.62, -0.78)  # toward the light: up and to the left


def rgb(h):
    h = h.lstrip('#')
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def hexs(c):
    return '#%02x%02x%02x' % tuple(max(0, min(255, round(v))) for v in c[:3])


def lum(c):
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


def hls(c):
    return colorsys.rgb_to_hls(*(v / 255 for v in c[:3]))


def from_hls(h, l, s):
    return tuple(round(v * 255) for v in colorsys.hls_to_rgb(h % 1.0, max(0, min(1, l)), max(0, min(1, s))))


def shift(c, dl, toward_cool):
    """`c` lighter (dl > 0) or darker, the hue turning toward yellow as it
    lightens and toward blue-violet as it darkens, the saturation easing at
    the light end — the hue-shifted ramp of hand-painted pixel art."""
    h, l, s = hls(c)
    target = 0.70 if toward_cool else 0.14
    k = min(1.0, abs(dl) * 1.6)
    dh = ((target - h + 0.5) % 1.0) - 0.5
    h2 = h + dh * (0.18 if s > 0.12 else 0.0) * k
    s2 = s * (1.0 - 0.25 * k) if dl > 0 else min(1.0, s * (1.0 + 0.12 * k))
    return from_hls(h2, l + dl, s2)


def family_key(c):
    """A material: hue bucket and chroma band. Chroma, not saturation, so a
    warm white (high HLS saturation, almost no colour) is not the gold beside
    it."""
    h, l, s = hls(c)
    chroma = s * (1 - abs(2 * l - 1))
    if chroma < 0.10:
        return ('grey', round(h * 6) % 6 if chroma > 0.04 else -1)
    return ('hue', round(h * 18) % 18, 0 if chroma < 0.32 else 1)


RAMP_GAP = 64       # two tones of a family this far apart are two materials


def ramps_for(colours):
    """Hue families over `colours`, sorted dark -> light, each extended by two
    tones past either end so every member has room to move. A family is cut
    where two neighbouring tones are RAMP_GAP apart — a white suit and its
    black button share no hue, and a step down from the white must not land
    on the button. {family key: [ramp, ...]}."""
    fam = {}
    for c in colours:
        fam.setdefault(family_key(c), set()).add(c)
    out = {}
    for k, cs in fam.items():
        members = sorted(cs, key=lum)
        runs = [[members[0]]]
        for c in members[1:]:
            if lum(c) - lum(runs[-1][-1]) > RAMP_GAP:
                runs.append([c])
            else:
                runs[-1].append(c)
        ramps = []
        for run in runs:
            lo, hi = run[0], run[-1]
            dark = [shift(lo, -0.075 * (j + 1), True) for j in range(2)]
            light = [shift(hi, +0.065 * (j + 1), False) for j in range(2)]
            ramps.append([lifted(d) if lum(d) < DARK_FLOOR else d for d in dark[::-1]] + run + light)
        out[k] = ramps
    return out


def ramp_of(c, ramps):
    """The ramp `c` sits on (or the nearest by luminance) in its family."""
    rs = ramps.get(family_key(c))
    if not rs:
        return None
    for r in rs:
        if c in r:
            return r
    return min(rs, key=lambda r: min(abs(lum(m) - lum(c)) for m in r))


def step(c, n, ramps):
    """`c` moved `n` steps along its ramp (clamped)."""
    if n == 0:
        return c
    ramp = ramp_of(c, ramps)
    if not ramp:
        return c
    i = ramp.index(c) if c in ramp else min(range(len(ramp)), key=lambda k: abs(lum(ramp[k]) - lum(c)))
    return ramp[max(0, min(len(ramp) - 1, i + n))]


def lifted(col):
    """A near-black of ours raised to the game's floor, its hue kept; a grey
    tinted toward the game's violet-black."""
    L = lum(col)
    if L >= 52:
        return col
    target = max(L, 0.6 * L + DARK_FLOOR)
    h, l, s = hls(col)
    if s < 0.12:
        h, s = 0.72, 0.22
    for _ in range(14):
        c2 = from_hls(h, l, s)
        if lum(c2) >= target:
            return c2
        l += 0.012
    return from_hls(h, l, s)


def finish_canvas(c, base, drawn, noform=(), min_area=8, amp=1.6, cloth=(), fur=()):
    """Finish a studio Canvas in place; see the module header. `base` is the
    PIL image the canvas is drawn on (the canvas's coordinates are its
    pixels), or None for an image with no base. `cloth`: colours whose
    planes hang in folds (step 2b)."""
    drawn = {d.lower() for d in drawn}
    noform = {d.lower() for d in noform}
    cloth = {rgb(d.lower()) for d in cloth}
    fur = {rgb(d.lower()) for d in fur}
    B = base.load() if base is not None else None
    comp = {}
    for (x, y), v in c.p.items():
        if v is None:
            continue
        if v == '.':
            if B is not None and 0 <= x < base.width and 0 <= y < base.height and B[x, y][3] >= 128:
                comp[(x, y)] = B[x, y][:3]
            continue
        comp[(x, y)] = rgb(v)
    ours = {pt for pt, v in c.p.items() if isinstance(v, str) and v != '.' and v.lower() in drawn}
    if not ours:
        return c
    keep_shading = {pt for pt in ours if c.p[pt].lower() in noform}
    for pt, col in render(comp, ours, keep_shading, min_area, amp, cloth, fur).items():
        c.p[pt] = hexs(col)
    return c


def folds(region):
    """{pt: -1|0|+1}: cloth hanging in folds over a region — columns every
    three or four pixels, each a lit ridge with its groove on the shaded
    (right) side, opening from a third of the way down to the hem. The
    spacing comes from the region's own top-left corner, so the same plane
    folds alike in every frame that draws it."""
    xs = [x for x, _ in region]
    ys = [y for _, y in region]
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    H = y1 - y0 + 1
    out = {}
    if H < 6 or x1 - x0 < 3:
        return out
    cols = {}
    x = x0 + 1 + (x0 + y0) % 2
    k = 0
    while x <= x1 - 1:
        start = y0 + max(1, H // 3) + (k * 5 + x0) % 3 - 1
        cols[x] = (+1, start)
        cols[x + 1] = (-1, start + 1)
        x += 3 + ((k * 7 + x0 + y0) % 3 == 0)
        k += 1
    for (x, y) in region:
        if x in cols:
            v, start = cols[x]
            if y >= start:
                out[(x, y)] = v
    return out


def bristles(region):
    """{pt: -1|0|+1}: an animal's coat over a region — short strokes two or
    three pixels long falling down the pelt, lit and shaded strokes laid
    between plain ones, offset column by column so they never line up into
    stripes."""
    out = {}
    for (x, y) in region:
        p = (x * 7) % 3
        stroke = (y + p) // 3
        k = (x * 5 + stroke * 3) % 6
        out[(x, y)] = {0: 1, 3: -1, 4: -1}.get(k, 0)
    return out


def render(comp, ours, noform, min_area=8, amp=1.6, cloth=(), fur=()):
    """Lift, form, outline over a composite {pt: rgb}; the new colours of
    `ours`."""
    out = {pt: lifted(comp[pt]) for pt in ours}
    ramps = ramps_for(set(out.values()))
    cur = dict(comp)
    cur.update(out)

    # 2. Form: flat areas shaded across their extent. A dark area is a plane
    # too (a black hood, a navy coat) once it is broad — four pixels or more
    # with the area on all four sides; a dark line is a stroke (step 3).
    seen = set()
    planes = set()
    for start in sorted(ours):
        if start in seen or start in noform:
            continue
        c0 = cur[start]
        region = {start}
        dq = deque([start])
        seen.add(start)
        while dq:
            x, y = dq.popleft()
            for dx, dy in N4:
                q = (x + dx, y + dy)
                if q in ours and q not in seen and q not in noform and cur[q] == c0:
                    seen.add(q)
                    region.add(q)
                    dq.append(q)
        if len(region) < min_area:
            continue
        # A line one pixel wide (a trim, a seam) is not a plane: it keeps its
        # colour. A plane has pixels with region on all four sides.
        core = sum(1 for (x, y) in region if all((x + dx, y + dy) in region for dx, dy in N4))
        if core < (2 if lum(comp[start]) >= OUTLINE_AT else 4):
            continue
        planes |= region
        fold = folds(region) if comp[start] in cloth else bristles(region) if comp[start] in fur else {}
        xs = [x for x, _ in region]
        ys = [y for _, y in region]
        cx, cy = sum(xs) / len(xs), sum(ys) / len(ys)
        rx = max(1.5, (max(xs) - min(xs)) / 2)
        ry = max(1.5, (max(ys) - min(ys)) / 2)
        for (x, y) in region:
            # the diagonal: +1 toward the light, -1 away from it
            t = -(LIGHT[0] * (x - cx) / rx + LIGHT[1] * (y - cy) / ry)
            n = -t * amp
            up = (x, y - 1) not in region
            left = (x - 1, y) not in region
            down = (x, y + 1) not in region
            right = (x + 1, y) not in region
            if (up or left) and not (down or right):
                n += 0.8
            elif (down or right) and not (up or left):
                n -= 0.8
            n += fold.get((x, y), 0)
            out[(x, y)] = step(c0, max(-2, min(2, round(n))), ramps)
    cur.update(out)

    # 3. Outline: strokes take the darkest tone of the material they bound.
    for pt in sorted(ours):
        if lum(comp[pt]) >= OUTLINE_AT or pt in planes:
            continue
        x, y = pt
        mats = [cur[q] for q in ((x + dx, y + dy) for dx, dy in N4)
                if q in ours and (lum(comp[q]) >= OUTLINE_AT or q in planes)]
        if not mats:
            out[pt] = lifted(comp[pt])
            continue
        # The darkest material it bounds owns it (a black robe, not the white
        # trim beside it).
        fams = Counter(family_key(m) for m in mats)
        key = min(fams, key=lambda k: min(lum(m) for m in mats if family_key(m) == k))
        m = min((m for m in mats if family_key(m) == key), key=lum)
        ramp = ramp_of(m, ramps) or ramps_for({m})[family_key(m)][0]
        o = ramp[0]
        if lum(o) > 0.62 * lum(m):
            o = shift(m, -hls(m)[1] * 0.45, True)
        lit_edge = (x - 1, y) not in comp or (x, y - 1) not in comp
        shade_edge = (x + 1, y) not in comp or (x, y + 1) not in comp
        if lit_edge and not shade_edge:
            o = shift(o, +0.06, False)
        out[pt] = lifted(o) if lum(o) < DARK_FLOOR else o
    return out


# Grid letters: every printable ASCII the grid format and the Rust table can
# carry ('.', '-', space are grid syntax; quotes and backslash would break the
# generated literals).
LETTERS = ''.join(ch for ch in map(chr, range(33, 127)) if ch not in ".-'\"\\")


def assign_letters(canvases):
    """Letters for every colour the canvases use. Finishing multiplies a
    sheet's colours; past the pool, the least-used colours fold into their
    nearest kept neighbour (the canvases are rewritten to match)."""
    use = Counter(v for c in canvases for v in c.p.values() if v and v != '.')
    keep = [col for col, _ in use.most_common(len(LETTERS))]
    if len(use) > len(keep):
        kept_rgb = [(k, rgb(k)) for k in keep]
        remap = {}
        for col in use:
            if col in keep:
                continue
            r = rgb(col)
            remap[col] = min(kept_rgb, key=lambda kr: sum((a - b) ** 2 for a, b in zip(kr[1], r)))[0]
        for c in canvases:
            for pt, v in c.p.items():
                if v in remap:
                    c.p[pt] = remap[v]
    return {col: LETTERS[i] for i, col in enumerate(sorted(keep))}
