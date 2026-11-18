# 05 눈사람 — a seagull asleep on her head, a little red bow, a red striped
# scarf, an egg-shaped snowman suit with one black button, bear-paw mittens,
# stubby white legs.
NAME = '눈사람'
REF = '05_snowman.png'

PAL = {
    'g': '#8c8ca0',  # gull wing
    'G': '#5c5c74',  # gull wing dark
    'W': '#ffffff',  # gull
    'w': '#d8d8e8',  # gull shade
    'B': '#f0b020',  # beak
    'e': '#141018',  # eye / button
    'R': '#e8241c',  # bow / scarf
    'r': '#a01818',  # bow / scarf shade
    'Q': '#6c1418',  # bow outline
    'U': '#fff4f0',  # scarf stripe
    'n': '#a8a0c4',  # button rim
}

HAIR = ['#7a5a32', '#a88050', '#d0aa70', '#ecc890', '#f6dca8', '#fcecc4', '#fffadc']
ORNAMENT = 'hair'

GULL = {'at': (6, -6), 'rows': [
    '     WW  ',
    '    WWeBB',
    '  wWWWW  ',
    'GggwWWWW ',
    ' GggwWWw ',
    '  GGwww  ',
]}
GULL_SIDE = {'at': (4, -6), 'rows': [
    '  WW     ',
    'BBeWW    ',
    '  WWWWw  ',
    ' WWWWwggG',
    ' wWWwggG ',
    '  wwwGG  ',
]}
GULL_BACK = {'at': (5, -6), 'rows': [
    '     WW  ',
    '    WWWBB',
    '  ggwWW  ',
    'GgggwWWW ',
    ' GggwWWw ',
    '  GGwww  ',
]}
BOW = {'at': (1, 3), 'rows': [' Q Q ', 'QRQRQ', 'QrRrQ', ' Q Q ']}
BOW_SIDE = {'at': (11, 3), 'rows': [' Q Q ', 'QRQRQ', 'QrRrQ', ' Q Q ']}
HEAD = {
    0: [BOW, GULL],
    1: [BOW_SIDE, GULL_SIDE],
    2: [dict(BOW_SIDE, at=(12, 3)), dict(GULL_SIDE, at=(5, -6))],
    3: [dict(BOW, at=(13, 3)), GULL_BACK],
}


def egg(cx, cy, rx, ry_top, ry_bot, face=None, button=None, scarf=None):
    """The suit: an egg lit from the upper left, as one would shade a ball —
    outline, a shadow crescent away from the light, a mid band, a light band
    and one highlight — with the scarf over its top and a button."""
    from math import hypot
    pal = {'o': '#6c6488', 'd': '#9a94bc', 'm': '#c6c0dc', 'l': '#e6e0f4', 'h': '#ffffff'}
    cells = {}
    for y in range(cy - ry_top - 1, cy + ry_bot + 2):
        ry = ry_top if y < cy else ry_bot
        for x in range(cx - rx - 1, cx + rx + 2):
            d = hypot((x - cx) / (rx + 0.5), (y - cy) / (ry + 0.5))
            if d > 1.0:
                continue
            # light from the upper left
            lx, ly = (x - cx) / (rx + 0.5), (y - cy) / (ry + 0.5)
            k = 0.62 - 0.34 * lx - 0.30 * ly
            if d > 0.86:
                ch = 'o'
            elif k < 0.28 or d > 0.78 and lx > 0.1:
                ch = 'd'
            elif k < 0.55:
                ch = 'm'
            elif k < 0.85:
                ch = 'l'
            else:
                ch = 'h'
            cells[(x, y)] = ch
    for (x, y), ch in (scarf or {}).items():
        if (x, y) in cells or ch != ' ':
            cells[(x, y)] = ch
    if button:
        bx, by = button
        cells[(bx, by)] = 'e'
        cells[(bx - 1, by)] = 'n'
    x0 = min(x for x, _ in cells)
    y0 = min(y for _, y in cells)
    w = max(x for x, _ in cells) - x0 + 1
    h = max(y for _, y in cells) - y0 + 1
    rows = [[' '] * w for _ in range(h)]
    for (x, y), ch in cells.items():
        rows[y - y0][x - x0] = ch
    return {'at': (x0, y0), 'rows': [''.join(r) for r in rows]}, pal


def scarf_cells(x0, x1, y, tail=None):
    """The scarf: two rows of red with white stripes across the neck, and its
    end hanging from `tail` = (x, length)."""
    cells = {}
    for x in range(x0, x1 + 1):
        cells[(x, y)] = 'U' if (x - x0) % 3 == 1 else 'R'
        cells[(x, y + 1)] = 'r' if (x - x0) % 3 != 1 else 'U'
    cells[(x0 - 1, y)] = cells[(x1 + 1, y)] = 'Q'
    cells[(x0 - 1, y + 1)] = cells[(x1 + 1, y + 1)] = 'Q'
    if tail:
        tx, n = tail
        for k in range(n):
            cells[(tx, y + 2 + k)] = 'U' if k % 3 == 1 else 'R'
            cells[(tx + 1, y + 2 + k)] = 'r'
            cells[(tx - 1, y + 2 + k)] = 'Q'
            cells[(tx + 2, y + 2 + k)] = 'Q'
    return cells


# Torso images: (0,0) is the image's top-left; front 33 is 6x10 centred at
# (0,-16), so her neck is about (3, 3) and the suit hangs to (3, 20).
_front, EGG_PAL = egg(3, 12, 8, 9, 8, button=(5, 11), scarf=scarf_cells(-1, 7, 2, tail=(5, 4)))
_side, _ = egg(3, 12, 7, 9, 8, button=(0, 12), scarf=scarf_cells(-1, 6, 2, tail=(5, 5)))
_back, _ = egg(4, 12, 8, 9, 8, scarf=scarf_cells(0, 8, 2, tail=(2, 4)))
PAL.update({'o': EGG_PAL['o'], 'd': EGG_PAL['d'], 'm': EGG_PAL['m'], 'l': EGG_PAL['l'], 'h': EGG_PAL['h']})
TORSO = {33: _front, 34: _side, 35: _back}
NOFORM = 'odmlh'   # the suit is shaded as a ball already (egg)

ARM = {
    'sleeve': ['#6c1418', '#a01818', '#e8241c', '#ff7060'],
    'trim': ['#fff4f0'],
    'jewel': ['#fff4f0'],
    'cuff': ['#c8a070', '#f0d8a8'],
    'glove': ['#6c1418', '#a01818', '#e8241c', '#ff7060'],
    'hand': ['#4a2c1c', '#7a4c30', '#b07848', '#e0b070', '#f8e0a8'],
}

LEGS = {
    'skirt': ['#6c6488', '#9a94bc', '#c6c0dc', '#e6e0f4', '#ffffff'],
    'panel': ['#9a94bc', '#c6c0dc'],
    'trim': ['#c6c0dc', '#e6e0f4'],
    'inner': ['#c6c0dc', '#e6e0f4', '#ffffff'],
    'shade': ['#6c6488', '#9a94bc'],
    'leg': ['#5c5478', '#8a84ac', '#b4aed0', '#d6d0ea', '#efeaf8', '#ffffff'],
}

WINGS = 'hide'
