# 06 별빛 망토 — a glass bubble helmet over lilac-white hair, a pink collar
# with an orange bow, a great navy cape sown with gold stars and trimmed in
# gold, a white ruffled dress, white tights, cream heels.
from math import hypot

NAME = '별빛 망토'
REF = '06_starry_cape.png'

PAL = {
    'B': '#a8d8f0',  # bubble rim
    'H': '#ffffff',  # bubble shine
    'b': '#6ca4cc',  # bubble rim shade
    'P': '#f0a0c0',  # collar pink
    'p': '#c06890',  # collar shade
    'Z': '#f8a830',  # bow orange
    'z': '#c06c10',  # bow shade
    'N': '#0a1032',  # cape outline
    'n': '#16246a',  # cape
    'm': '#24389a',  # cape mid
    'M': '#3a56c8',  # cape light
    'G': '#f4cc58',  # gold
    'g': '#a8802c',  # gold shade
    'W': '#ffffff',  # dress
    'w': '#d8d4ec',  # dress shade
    'v': '#a8a2c8',  # dress deep
    'S': '#ffe7de',
    's': '#efb6a5',
    'r': '#c67d63',
}

HAIR = ['#5a5470', '#7c7496', '#a49cc0', '#c8c0e0', '#e2dcf4', '#f4f0fc', '#ffffff']
ORNAMENT = 'hair'


def bubble(cx, cy, r, shine=()):
    """The helmet as glass is painted (v4): a rim that turns from white on
    the upper left through two blues to a deep one lower right, the window's
    reflection — a bright arc just inside the rim toward the light — and a
    faint second one opposite. Her head shows through; we only draw glass."""
    from math import atan2, cos
    light = atan2(-0.78, -0.62)
    cells = {}
    for y in range(int(cy - r - 1), int(cy + r + 2)):
        for x in range(int(cx - r - 1), int(cx + r + 2)):
            d = hypot(x - cx, y - cy)
            k = cos(atan2(y - cy, x - cx) - light)
            if abs(d - r) <= 0.5:
                cells[(x, y)] = 'H' if k > 0.72 else 'B' if k > 0.0 else 'b' if k > -0.6 else 'c'
            elif abs(d - (r - 2)) <= 0.5:
                if k > 0.94:
                    cells[(x, y)] = 'H'
                elif k > 0.86:
                    cells[(x, y)] = 'B'
                elif k < -0.97:
                    cells[(x, y)] = 'B'
    for fx, fy in shine:
        cells[(round(cx + fx * (r - 2)), round(cy + fy * (r - 2)))] = 'H'
    x0 = min(x for x, _ in cells)
    y0 = min(y for _, y in cells)
    w = max(x for x, _ in cells) - x0 + 1
    h = max(y for _, y in cells) - y0 + 1
    rows = [[' '] * w for _ in range(h)]
    for (x, y), ch in cells.items():
        rows[y - y0][x - x0] = ch
    return {'at': (x0, y0), 'rows': [''.join(row) for row in rows]}


PAL['c'] = '#3e6c98'  # bubble rim, deep
NOFORM = 'HBbc'       # glass: painted already


HEAD = {
    0: [bubble(8.5, 8.0, 10.5)],
    1: [bubble(7.5, 8.5, 10.5)],
    2: [bubble(8.0, 8.0, 10.5)],
    3: [bubble(8.5, 9.0, 10.5)],
}

TORSO = {
    33: {'at': (0, 0), 'rows': [
        '-pPPp-',
        'pPZZPp',
        'NpzZpN',
        'nWzZWm',
        'nWWWwm',
        'nWGgwm',
        'nwWWvm',
        'NWwwvN',
        'NwWWvN',
        '-NnnN-',
    ]},
    34: {'at': (0, 0), 'rows': [
        '--pPPPp',
        '-pPZZPp',
        '-NzZpNN',
        'NWzWwmN',
        'NWWWwmN',
        'NWGgwN-',
        'NwWWvN-',
        'NWwwvN-',
        'NwWWvnN',
        'NNnnnN-',
        '--NN---',
    ]},
    35: {'at': (0, 0), 'rows': [
        '--pPPp--',
        '-pPPPPp-',
        '-NnmmnN-',
        '-NmMmnN-',
        '-NmMGnN-',
        '-NmmmnN-',
        '-NnmGnN-',
        '-NnmmnN-',
        'NnmmnnnN',
        'NnmGmnnN',
        '-NNNNNN-',
    ]},
}

ARM = {
    'sleeve': ['#0a1032', '#16246a', '#24389a', '#3a56c8'],
    'trim': ['#a8802c', '#f4cc58'],
    'jewel': ['#9c5cf0'],
    'cuff': ['#a8a2c8', '#ffffff'],
    'glove': ['#a8a2c8', '#d8d4ec', '#ffffff'],
    'hand': 'skin',
}

LEGS = {
    'skirt': ['#a8a2c8', '#d8d4ec', '#ffffff'],
    'panel': ['#d8d4ec', '#ffffff'],
    'trim': ['#d8d4ec', '#ffffff'],
    'inner': ['#d8d4ec', '#ffffff'],
    'shade': ['#a8a2c8', '#d8d4ec'],
    'leg': ['#8a84a8', '#b4aed0', '#d6d0ea', '#eeeaf8', '#ffffff'],
    'foot': ['#8a6a40', '#c8a878', '#ecd8b0', '#fff4dc'],
}

DRESS = {
    'ramp': ['#0a1032', '#121c56', '#1a2a7a', '#24389a', '#3048b8', '#4664d4'],
    'outline': '#060a24',
    'light': 0.9,
    'slope': 0.45,
    'open': 7,
    'open_trim': '#f4cc58',
    'open_fill': ['#a8a2c8', '#d8d4ec', '#ffffff'],
    'stars': '#f4cc58',
    'hem': ['#f4cc58', '#a8802c'],
    'show': 2,
    'flare': 3,
    'max_half': 11,
    'bell': 1.2,
}

WINGS = ['#16246a', '#2c4cac', '#5c7cd8', '#a8c0f0', '#e8f0ff']
