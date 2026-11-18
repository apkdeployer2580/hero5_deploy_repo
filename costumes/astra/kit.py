"""Shared head shapes for the astra designs.

Each shape is drawn once, in six letters — X outline, Q deepest shade,
q shade, o mid, O light, Y highlight — and a design maps those letters to its
own colours in PAL. `place(...)` gives the HEAD entries for the five views.
Templates: `python costumes/astra_template.py <frame> <at x> <at y> <w> <h>`.
"""

# Spiky, swept back (01 ember, 06 frost). 3/4 at (-2, -5), front and back at
# (-3, -6); the side and look-down views reuse the 3/4 with a patch.
SPIKY = {}

SPIKY[1] = [
    '         X           ',
    '     X  XOX    X     ',
    '    XOX XYX   XoX    ',
    '    XOoXXOYoXXooX  X ',
    '   XOYoqOOYoqoooqXXoX',
    '   XOYYoqYYoqooooqqoX',
    'X  XOYOOqYOoqooooqqX ',
    'XOXXOYOOqOOoqoooqqqX ',
    ' XOOOOYYqOooqooqqqQX ',
    '  XOOOYOOqooqoooqqQQX',
    '  XOOOOooqooqooqqqQQX',
    ' XOOOOooqoooqoqqqQQX ',
    ' XOoOoooqooqooqqqQQX ',
    '  XooXooqoqooqqqqQX  ',
    '  XoXooXooqoXoqqqQX  ',
    '  XoXoqXqXoqXqqXqQX  ',
    '   XoX qX  XqqXQQX   ',
    '   XX   X  XqqQQQX   ',
    '    X      XqQQQX    ',
    '    X        XQQQX   ',
    '    X        XQQX    ',
    '             XX      ',
]
SPIKY[0] = [
    '                      ',
    '       X     X        ',
    '      XOX   XOX   X   ',
    '  X   XOoX XOoX  XoX  ',
    '  XOX XOYoXXOYoX XooX ',
    '   XOXXOYooOYooXXooX  ',
    '   XOOOYYoOYYooooooX  ',
    ' XXOOOYYOOYYOooooqqXX ',
    'XOOOOYOOqYOOooqooqqqoX',
    ' XOOOOOqOOooqoooqqqqX ',
    '  XOOOOqOOooqoooqqqQX ',
    ' XOOOOqOOooqooooqqqQQX',
    'XOOOOoqOooqooqoooqqQQX',
    ' XOOooqooqoooqooqqqQX ',
    '  XOoqooqooqoooqqqqQX ',
    '  XOoXooXoqXoqXoqqQQX ',
    '  XoXooX oqX qXqXqQQX ',
    '  XoXoqX  qX  qqXQQX  ',
    '  XoXXqX   g   qXQX   ',
    '    XqqX       q QX   ',
    '     XX        XQQX   ',
    '      X        X X    ',
    '      X        X      ',
    '                      ',
]
SPIKY[3] = [
    '                      ',
    '                      ',
    '         X    X       ',
    '   X    XOX  XoX   X  ',
    '   XoX XOOoXXooqX XoX ',
    '    XoXXOOooXoooqXXooX',
    '   XXOoOOooqooooqqooX ',
    '  XOOOOOoooqoooooqqqX ',
    ' XOOOOOoooqooooooqqqXX',
    'XOOOOOooqoooqooooqqqoX',
    ' XOOOOooqooqooooqqqqX ',
    '  XOOOoooqooqoooqqqQX ',
    ' XOOOooooqoooqooqqqQQX',
    'XOOOoooqooooqoooqqqQQX',
    ' XOOooooqoooqoooqqQQX ',
    '  XOooqooqoooqoqqqQQX ',
    '  XOooqooqooqooqqqQQX ',
    '  XOoqooqoooqooqqQQX  ',
    '   XoqoqooqoqoqqqQQX  ',
    '   XoXoqXoqXoqXqQQX   ',
    '    XXoXXoqXqXqQQX    ',
    '      XoXoqXqXqQX     ',
    '      X XoqXqX X      ',
    '          XX          ',
    '                      ',
]

SPIKY_SIDE_PATCH = [''] * 16 + [
    '         qq',
    '         qq',
    '         qq',
    '         qQ',
    '         QQ',
]
SPIKY_DOWN_PATCH = [''] * 18 + ['                Q']



def spiky():
    return {
        1: {'at': (-2, -5), 'rows': SPIKY[1]},
        0: {'at': (-3, -6), 'rows': SPIKY[0]},
        3: {'at': (-3, -6), 'rows': SPIKY[3]},
        4: [{'at': (-2, -4), 'rows': SPIKY[1]}, {'at': (-2, -4), 'rows': SPIKY_SIDE_PATCH}],
        6: [{'at': (-2, -3), 'rows': SPIKY[1]}, {'at': (-2, -3), 'rows': SPIKY_DOWN_PATCH}],
    }


def fur(rows, spans, light='F', mid='f', shade='x', deep='z', outline='k', seed=0, keep=''):
    """Lay a fluffy fur band over `rows` (a list of strings, edited in place
    and returned). `spans`: {row: (left, right)} inclusive columns. The edge
    pixels are outline, the lowest row of each column is shade with a
    jagged fringe, the inside is light with mid tufts in a fixed pattern. A
    row may take a list of spans (fur draped down both arms).
    Characters in `keep` (a jacket underneath) are left alone."""
    grid = [list(r) for r in rows]
    w = max(len(r) for r in rows)
    for g in grid:
        g.extend(' ' * (w - len(g)))
    cells = set()
    for y, span in spans.items():
        for l, r in (span if isinstance(span, list) else [span]):
            cells |= {(x, y) for x in range(l, r + 1)}
    for (x, y) in sorted(cells):
        if grid[y][x] in keep and grid[y][x] != ' ':
            continue
        below = (x, y + 1) in cells
        edge = (x - 1, y) not in cells or (x + 1, y) not in cells or (x, y - 1) not in cells
        if edge and not below or (x - 1, y) not in cells or (x + 1, y) not in cells:
            ch = outline
        elif (x, y - 1) not in cells:
            ch = outline
        elif not below:
            ch = deep if (x + seed) % 3 == 0 else shade
        elif (x, y + 2) not in cells:
            ch = shade if (x + seed) % 2 else mid
        else:
            # v4: tufts, laid like bricks — each two rows tall and three
            # wide, lit on top, a crevice of shade between neighbours; the
            # side away from the light (right) a step darker.
            ty, sub = divmod(y + seed, 2)
            ph = (x + 2 * (ty % 2) + seed) % 4
            if ph == 3:
                ch = shade if sub else mid
            elif sub == 0:
                ch = light
            else:
                ch = light if ph == 1 else mid
            span_r = max(xx for (xx, yy) in cells if yy == y)
            span_l = min(xx for (xx, yy) in cells if yy == y)
            if x > span_l + (span_r - span_l) * 0.66:
                ch = {light: mid, mid: shade, shade: deep}.get(ch, ch)
        grid[y][x] = ch
    # jagged fringe: drop alternate bottom pixels one row
    for (x, y) in sorted(cells):
        if (x, y + 1) not in cells and (x + seed) % 3 == 1 and y + 1 < len(grid):
            if grid[y + 1][x] == ' ':
                grid[y + 1][x] = outline
    return [''.join(g) for g in grid]


# The officer's coat (07 nocturne, 08 crimson): gold epaulettes with fringe,
# a double-breasted jacket closed to the belt, a long coat open below it to
# the ankles and the cape behind. Letters:
#   k outline | J j i jacket light/mid/dark | C c e coat light/mid/dark
#   T t trim light/shade | p fringe
# Officer coat, semantic letters:
#  k outline | J j i jacket light/mid/dark | C c e coat light/mid/dark
#  T t trim light/shade (gold) | U u lining light/dark | p fringe
OFFICER = {}
OFFICER[0] = ((-7, 0), [
    '                        ',
    '                        ',
    '          kkkkk         ',
    '        kkiJjJikk       ',
    '      kkCkiJjJikCkk     ',
    '    kkTTTkiJTJikTTTkk   ',
    '   kTTTTTkiJjJikTTTTTk  ',
    '   kptptpkiTJTikptptpk  ',
    '  keCcCckJiJjJiJkCcCcek ',
    '  keCcCckJiTJTiJkCcCcek ',
    '  keCcCckJiJjJiJkCccCek ',
    '  keCcCkTTTTtTTTTkcCcek ',
    '  keCcCkJjiJkJijJkCccek ',
    ' keCcCTkiJk   kJikTCccek',
    ' keCcCTk         kTCccek',
    ' keCcCTk         kTCcCek',
    'keCcCcTk         kTCccCek',
    'keCcCcTk         kTCccCek',
    'keCcCTk           kTCcCek',
    'keCcCTk           kTCccek',
    'kTTTTk             kTTTTk',
    ' kkkk               kkkk ',
])
OFFICER[1] = ((-6, 0), [
    '                          ',
    '                          ',
    '         kkkkkkk          ',
    '       kkTTTTTTTkk        ',
    '      kTTTTTTTTTTTk       ',
    '      kptptptptptpkk      ',
    '     kJjiJjCcccccCcek     ',
    '     kJTiJjCcccccCccek    ',
    '     kJjiJjCcccccCcccek   ',
    '     kJTiJjCcccccCccccek  ',
    '     kTTTTTTcccccCccccek  ',
    '     kJjiJTCcccccCcccccek ',
    '     kkJikTCccccccCcccccek',
    '      kkkTCcccccccCccccek ',
    '        kTCcccccccCcccccek',
    '        kTCcccccccCccccek ',
    '       kTCcccccccccCccccek',
    '       kTCccccccccCcccccek',
    '      kTCcccccccccCcccccek',
    '      kTCccccccccccCccccek',
    '      kTTTTTTTTTTTTTTTTTTk',
    '       kkkkkkkkkkkkkkkkkk ',
])
OFFICER[2] = ((-6, 0), [
    '                         ',
    '                         ',
    '         kkkkkkk         ',
    '      kkkcccccccckk      ',
    '    kkTTTcccccccTTTkk    ',
    '   kTTTTTCcccccccTTTTTk  ',
    '   kptptpCcccccccptptpk  ',
    '   keCcCcCcccccccCcCcek  ',
    '  keCCcCcCcccccccCcCcCek ',
    '  keCCcCcCcccccccCcCcCek ',
    '  keCCcCTTTTTTTTTTTcCCek ',
    '  keCCcCcCcccccccCcCcCek ',
    ' keCCccCcCcccccccCcCcCCek',
    ' keCCccCcCcccccccCcCcCCek',
    ' keCCcccCcCcccccCcCccCCek',
    'keCCccccCcCcccccCcCcccCCek',
    'keCCccccCcCcccccCcCccccCek',
    'keCCcccccCcCcccCcCcccccCek',
    'keCCcccccCcCcccCcCccccCCek',
    'keCCccccccCcCcCcCcccccCCek',
    'kTTTTTTTTTTTTTTTTTTTTTTTTk',
    ' kkkkkkkkkkkkkkkkkkkkkkkk ',
])


def officer():
    return {v: {'at': at, 'rows': rows} for v, (at, rows) in OFFICER.items()}


# A long wrapped robe to the ankles (02 wanderer, 09 azure, 05 oni): a
# crossed collar, the sash, and a front panel down the middle. Letters:
#   k outline | L M D robe light/mid/dark | W w collar | R r sash
#   P p front panel | G pattern on the panel
ROBE = {}
ROBE[0] = ((-6, 0), [
    '                      ',
    '                      ',
    '        kkkkkk        ',
    '      kkMDWWDMkk      ',
    '     kLMDWwwWDMDk     ',
    '    kLMMDWwWDMMDDk    ',
    '   kLLMMMDWWDMMMDDk   ',
    '   kLMMMMDWwDMMMMDk   ',
    '   kLMMMMMDWDMMMMDk   ',
    '   kLMMMMMDWDMMMMDk   ',
    '   kLMRRRRRRRRRRRMDk  ',
    '   kLMRrRRRRRRRrRMDk  ',
    '   kLMMMMMDPPDMMMMDk  ',
    '  kLLMMMMDPppPDMMMDDk ',
    '  kLMMMMMDPppPDMMMMDk ',
    '  kLMMMMMDPGpPDMMMMDk ',
    ' kLLMMMMMDPppPDMMMMDDk',
    ' kLMMMMMMDPpGPDMMMMMDk',
    ' kLMMMMMMDPppPDMMMMMDk',
    'kLLMMMMMMDPGpPDMMMMMDDk',
    'kkkkkkkkkkPppPkkkkkkkkk',
    '          kkkk         ',
])
ROBE[1] = ((-6, 0), [
    '                        ',
    '                        ',
    '         kkkkkk         ',
    '       kkDWWMMMkk       ',
    '      kLDWwWMMMMDk      ',
    '     kLLDWwWMMMMDDk     ',
    '     kLLMDWWMMMMMDDk    ',
    '     kLLMMDWMMMMMMDk    ',
    '     kLLMMMDMMMMMMDk    ',
    '     kLLMMMDMMMMMMDDk   ',
    '     kRRRRRRRRRRRRRDk   ',
    '     kRrRRRRRRRRRrRDk   ',
    '     kPPDMMMMMMMMMMDDk  ',
    '    kPppDMMMMMMMMMMMDk  ',
    '    kPpGDMMMMMMMMMMMDDk ',
    '    kPppDMMMMMMMMMMMMDk ',
    '   kPpGPDMMMMMMMMMMMMDDk',
    '   kPppPDMMMMMMMMMMMMMDk',
    '   kPpGPDMMMMMMMMMMMMMDk',
    '   kPppPDMMMMMMMMMMMMMDDk',
    '   kkkkkkkkkkkkkkkkkkkkkk',
    '                        ',
])
ROBE[2] = ((-6, 0), [
    '                         ',
    '                         ',
    '         kkkkkkk         ',
    '       kkMMMMMMMkk       ',
    '      kLMMMMMMMMMDk      ',
    '     kLLMMMMMMMMMDDk     ',
    '    kLLMMMMMMMMMMMDDk    ',
    '    kLMMMMMMMMMMMMMDk    ',
    '    kLMMMMMMMMMMMMMDk    ',
    '    kLMMMMMMMMMMMMMDk    ',
    '    kLRRRRRRRRRRRRRDk    ',
    '    kLRrRRRRRRRRRrRDk    ',
    '    kLMMMMMMMMMMMMMDDk   ',
    '   kLLMMMMMMMMMMMMMDDk   ',
    '   kLMMMMMMMMMMMMMMMDk   ',
    '   kLMMMMMMMMMMMMMMMDDk  ',
    '  kLLMMMMMMMMMMMMMMMMDk  ',
    '  kLMMMMMMMMMMMMMMMMMDDk ',
    '  kLMMMMMMMMMMMMMMMMMMDk ',
    ' kLLMMMMMMMMMMMMMMMMMMDDk',
    ' kkkkkkkkkkkkkkkkkkkkkkkk',
    '                         ',
])


def robe():
    return {v: {'at': at, 'rows': rows} for v, (at, rows) in ROBE.items()}


def sym(halves, swap=None):
    """Rows from their left halves, mirrored into the right — for the front
    and back of a symmetric piece. `swap` maps a letter to its mirror twin
    (a lit edge on the left is a shaded edge on the right)."""
    swap = swap or {}
    out = []
    for h in halves:
        out.append(h + ''.join(swap.get(c, c) for c in reversed(h)))
    return out


def recede(rows, start, step, edge='k', trim=None):
    """Cut a coat's front (left) edge back as it falls: from row `start`
    each row loses round((y - start + 1) * step) more pixels on the left,
    re-edged with `edge` (and `trim` inside it) — the open front of a long
    coat in profile, so the leading leg steps out of it."""
    out = []
    for y, r in enumerate(rows):
        if y >= start:
            cut = int((y - start + 1) * step + 0.5)
            l = len(r) - len(r.lstrip())
            body = r[l:]
            if len(body) > cut + 3:
                body = body[cut:]
                body = edge + (trim if trim else body[1]) + body[2:]
                r = ' ' * (l + cut) + body
        out.append(r)
    return out


def officer_open():
    """The officer's coat with its front cut away in profile."""
    out = officer()
    side = out[1]
    out[1] = {'at': side['at'], 'rows': recede(side['rows'], 12, 0.55, 'k', 'T')}
    return out


# ---------------------------------------------------------------- cone hats
# A woven cone (02's jingasa, 09's kasa), painted rather than filled: the
# game's hats turn from a lit left flank to a shaded right one through six or
# seven tones, and a woven cone carries the weave — radial ribs from the apex
# and a ring every third row. The ribs' phase turns with the view, so the hat
# reads as turning with the head. Letters: `ramp` dark -> light (seven),
# `rim` dark -> light (three or four), `outline`.
CONE_PHASE = {0: 0.0, 1: 0.14, 3: 0.5, 4: 0.27, 6: 0.14}


def cone(view, width=23, height=12, ramp='1234567', rim='vxyz', outline='K', ribs=5, flare=0.85):
    import math
    w2 = width / 2.0
    cx = (width - 1) / 2.0
    phase = CONE_PHASE.get(view, 0.0)
    rows = []
    body_h = height - 2          # the last two rows: the rim band and its outline
    for y in range(height):
        row = [' '] * width
        if y < body_h:
            t = (y + 1) / body_h
            half = max(1.0, w2 * t ** flare)
            if y == 0:
                half = 1.2
            lo, hi = int(round(cx - half + 0.5)), int(round(cx + half - 0.5))
            for x in range(lo, hi + 1):
                u = (x - cx) / max(half, 1.0)            # -1 left .. +1 right
                if x in (lo, hi) or y == 0:
                    row[x] = outline
                    continue
                # light from the upper left; a soft highlight a third of the
                # way in on the lit flank
                light = 0.50 - 0.36 * u + 0.16 * math.exp(-((u + 0.45) / 0.28) ** 2)
                light -= 0.10 * t                         # the cone darkens toward the brim
                # radial ribs: a rib is darker, the strip after it lighter
                a = (math.asin(max(-1.0, min(1.0, u))) / math.pi + 0.5 + phase) * ribs
                f = a - math.floor(a)
                if f < 0.14:
                    light -= 0.13
                elif f < 0.30:
                    light += 0.05
                # a weave ring every third row
                if y % 3 == 2 and (x + y) % 2 == 0:
                    light -= 0.05
                k = max(0, min(len(ramp) - 1, int(light * len(ramp))))
                row[x] = ramp[k]
        elif y == body_h:
            for x in range(width):
                u = (x - cx) / w2
                if x in (0, width - 1):
                    row[x] = outline
                    continue
                light = 0.75 - 0.5 * u
                k = max(0, min(len(rim) - 1, int(light * len(rim))))
                row[x] = rim[k]
        else:
            row = [outline] * width
        rows.append(''.join(row).rstrip())
    return rows
