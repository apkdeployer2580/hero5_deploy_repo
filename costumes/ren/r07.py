# 07 천사 — great lavender-white wings, small feather wings at her temples, a
# pale-gold high ponytail, a black halter with gold trim and a teal gem, a
# long black skirt starred in gold, open over a cream panel, gold sandals.
from ren import kit

NAME = '천사'
REF = '07_angel.png'

PAL = {
    'O': '#6e5236',  # hair outline
    'b': '#b08a5a',  # hair shade
    'y': '#dcbc8c',  # hair
    'Y': '#f0dcb4',  # hair light
    'L': '#fff6e2',  # hair shine
    'F': '#5e5a9c',  # feather outline (the cif draws wings at 59%: contrast must survive it)
    'f': '#9e9ad8',  # feather shade
    'm': '#d2d0f2',  # feather
    'l': '#f2f0ff',  # feather light
    'W': '#ffffff',  # feather white
    'K': '#0e0c14',  # black
    'k': '#26222e',  # black light
    'G': '#f0c850',  # gold
    'g': '#a88430',  # gold shade
    't': '#2cc8c8',  # teal gem
    'S': '#ffe7de',  # skin light
    's': '#efb6a5',  # skin
    'r': '#c67d63',  # skin shade
}

HAIR = ['#6e5236', '#94724c', '#b8966a', '#dcbc8c', '#ecd6ac', '#f8eacc', '#fff6e2']
ORNAMENT = 'hair'

# Small wings at her temples, where her flowers were.
TEMPLE_FRONT = [
    {'at': (0, 1), 'rows': ['F   ', 'lF  ', 'WlF ', 'WmlF', ' WmF', '  FF']},
    {'at': (14, 1), 'rows': ['   F', '  Fl', ' FlW', 'FlmW', 'FmW ', 'FF  ']},
]
TEMPLE_SIDE = [{'at': (11, 0), 'rows': ['   F', '  Fl', ' FlW', 'FlmW', 'FmW ', 'FF  ']}]
TEMPLE_BACK = [
    {'at': (0, 1), 'rows': ['F   ', 'mF  ', 'lmF ', 'lmmF', ' lmF', '  FF']},
    {'at': (14, 1), 'rows': ['   F', '  Fm', ' Fml', 'Fmml', 'Fml ', 'FF  ']},
]
# The ponytail, high at the back of her head.
TAIL_SIDE = {'at': (11, 1), 'rows': [
    ' OOO  ',
    'OyYLO ',
    'OyYLyO',
    ' OyYyO',
    ' ObyYO',
    '  OyYO',
    '  OyYO',
    '  ObyO',
    '  OyYO',
    '  OYyO',
    '   ObO',
    '   OyO',
    '    O ',
]}
TAIL_BACK = {'at': (6, -2), 'rows': [
    '  OOOO  ',
    ' OtGGtO ',
    ' OyYLyO ',
    'OyYLLYyO',
    'OyYLYYbO',
    ' OyYLybO',
    ' OyYYybO',
    ' ObyYyO ',
    ' OyYLbO ',
    '  OyYbO ',
    '  OyYbO ',
    '  ObyO  ',
    '   OyO  ',
    '   OO   ',
]}
HEAD = {
    0: TEMPLE_FRONT,
    1: [TAIL_SIDE] + TEMPLE_SIDE,
    2: [dict(TAIL_SIDE, at=(12, 1)), dict(TEMPLE_SIDE[0], at=(12, 0))],
    3: TEMPLE_BACK + [TAIL_BACK],
}

TORSO = {
    33: {'at': (0, 0), 'rows': [
        '-ssss-',
        'sSSSSs',
        'sSttSs',
        'rGGGGr',
        'sKkkKs',
        'rKkkKr',
        'sSSSSs',
        'GgGGgG',
        'KkkkkK',
        '-KkkK-',
    ]},
    34: {'at': (0, 0), 'rows': [
        '--sssss',
        '-sSSSsr',
        '-stSSsr',
        'rGGGGsr',
        'KkkKsr-',
        'KkkKrr-',
        'SSSssr-',
        'GgGGgG-',
        'KkkkkkK',
        'KKkkkK-',
        '--KK---',
    ]},
    35: {'at': (0, 0), 'rows': [
        '--ssss--',
        '-sSSSSs-',
        '-sSSSSs-',
        '-rSSSsr-',
        '-sKKKKs-',
        '-sSSSSs-',
        '-sSSSSr-',
        '-sSSSsr-',
        'GgGGGGgG',
        'KkkkkkkK',
        '-KKKKKK-',
    ]},
}

ARM = {
    'sleeve': 'skin',
    'trim': ['#a88430', '#f0c850'],
    'jewel': ['#2cc8c8'],
    'cuff': ['#a88430', '#f0c850'],
    'glove': 'skin',
}

LEGS = {
    'skirt': ['#d0c0a0', '#f0e4c8', '#fffaf0'],
    'panel': ['#a88430', '#f0c850'],
    'trim': ['#a88430', '#f0c850'],
    'inner': ['#d0c0a0', '#f0e4c8', '#fffaf0'],
    'shade': ['#b0a080', '#d0c0a0'],
    'leg': 'skin',
    'foot': ['#7a5a1c', '#a88430', '#d8a840', '#f0c850', '#fff0a0'],
}

DRESS = {
    'ramp': ['#0a0810', '#16121e', '#221c2c', '#30283c', '#40364e'],
    'outline': '#050408',
    'light': 0.9,
    'slope': 0.4,
    'open': 7,
    'open_trim': '#f0c850',
    'open_fill': ['#d0c0a0', '#f0e4c8', '#fffaf0'],
    'stars': '#f0c850',
    'hem': ['#a88430'],
    'show': 2,
    'flare': 2,
    'max_half': 9,
}

# The great wings: her two light-tails redrawn as feathers rising behind her
# shoulders. The cif still draws them at 59% opacity — an angel's are light.
# Wing images are drawn in the light-tail's own pixels: the left one's (0, 0)
# sits at (-10, -19) from her feet in the front view, the right one's at
# (3, -18). Shoulder to wrist rises above her head; the flight feathers hang
# from the wrist past her hips.
FEATHERS = [(0, 16), (7, 17), (14, 17), (21, 15), (28, 12)]
WING_L = kit.wing(root=(6, 2), wrist=(0, -10), primaries=FEATHERS, width=2.3, covert=2.6)
WING_R = kit.wing(root=(2, 1), wrist=(8, -11), primaries=FEATHERS, width=2.3, covert=2.6, mirror=True)
WINGS = {29: WING_L, 30: WING_R}
NOFORM = 'Ffmlw'   # feathers carry their own light
