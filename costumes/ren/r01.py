# 01 이브닝 드레스 — a wavy blonde high ponytail, teal earrings, a white
# bustier with black panels and a teal gem, long white gloves, a floor-length
# white gown in tiers of black ruffles with a high slit, black boots.
NAME = '이브닝 드레스'
REF = '01_evening_gown.png'

PAL = {
    'O': '#5a3c22',  # blonde outline
    'b': '#a87440',  # blonde shade
    'y': '#d8a85e',  # blonde
    'Y': '#f0cc88',  # blonde light
    'L': '#fbe6b4',  # blonde shine
    'K': '#0e0c12',  # black
    'k': '#2c2834',  # black sheen
    'W': '#ffffff',  # white
    'w': '#cfcfdc',  # white shade
    'v': '#8e8aa2',  # white deep shade
    't': '#2ec4c8',  # teal gem
    'T': '#a8f4f4',  # teal shine
    'S': '#ffe7de',  # skin light
    's': '#efb6a5',  # skin
    'r': '#c67d63',  # skin shade
}

HAIR = ['#5a3c22', '#8a5c30', '#b88448', '#d8a85e', '#ecc480', '#f6dca4', '#fdeec8']
ORNAMENT = 'hair'

# A tiny teal drop under each ear tip, and the ponytail's waves behind.
EARRINGS_FRONT = {'at': (0, 11), 'rows': ['t' + ' ' * 16 + 't', 'T' + ' ' * 16 + 'T']}
TAIL_FRONT = {'at': (-5, 1), 'rows': [
    '      OO   ',
    '     OyYO  ',
    '    OyYLO  ',
    '   OyYLyO  ',
    '  OyYLybO  ',
    ' OyYLyyO   ',
    ' OyLYybO   ',
    'OyYYybO    ',
    'OyLYbO     ',
    ' OyYbO     ',
    ' ObyYO     ',
    'OyYybO     ',
    'OyLybO     ',
    ' OyYbO     ',
    ' ObyYO     ',
    'ObyYbO     ',
    'OyYbO      ',
    ' OyO       ',
    ' ObO       ',
    '  O        ',
]}
TAIL_SIDE = {'at': (9, -2), 'rows': [
    '  OOO    ',
    ' OKkKO   ',
    ' OyYLyO  ',
    '  OyLYyO ',
    '  OyYLyO ',
    '   OyYLO ',
    '   ObyYyO',
    '   OyYLyO',
    '    OyYbO',
    '   OyLYbO',
    '   OyYybO',
    '    ObyYO',
    '    OyYbO',
    '   OyLYbO',
    '   OyYbO ',
    '    ObyO ',
    '    OyYO ',
    '     ObO ',
    '     OyO ',
    '      O  ',
]}
TAIL_BACK = {'at': (5, -3), 'rows': [
    '   OOOO   ',
    '  OKkkKO  ',
    '  OyYLyO  ',
    ' OyYLLYyO ',
    ' OyYLYYbO ',
    ' ObyYLybO ',
    '  OyYLYbO ',
    '  ObyLYbO ',
    '  OyYYybO ',
    ' OyLYybO  ',
    ' OyYyybO  ',
    '  ObyYbO  ',
    '  OyYLbO  ',
    '  OyYYbO  ',
    '   OyYbO  ',
    '   ObyO   ',
    '  OyYbO   ',
    '  OybO    ',
    '   OO     ',
]}
HEAD = {
    0: [TAIL_FRONT, EARRINGS_FRONT],
    1: [TAIL_SIDE],
    2: [dict(TAIL_SIDE, at=(10, -2))],
    3: [TAIL_BACK],
}

TORSO = {
    33: {'at': (0, 0), 'rows': [
        '-ssss-',
        'sSSSSs',
        'sSSSSs',
        'rsSSsr',
        'KWtTWK',
        'KWwtwK',
        'kWWWwK',
        'KwWWvK',
        'KkwwkK',
        '-KKKK-',
    ]},
    34: {'at': (0, 0), 'rows': [
        '--sssss',
        '-sSSSsr',
        '-sSSSsr',
        'rsSSsrr',
        'KtTWwKK',
        'KWtWwK-',
        'kWWwvK-',
        'KwWWvK-',
        'KkwwvkK',
        'KKkkkK-',
        '--KK---',
    ]},
    35: {'at': (0, 0), 'rows': [
        '--ssss--',
        '-sSSSSs-',
        '-sSSSSs-',
        '-rsSSsr-',
        '-KWWWWK-',
        '-KwKKwK-',
        '-KWKKwK-',
        '-KwKKvK-',
        'KKWKKwKK',
        'KkwkkvkK',
        '-KKKKKK-',
    ]},
}

ARM = {
    'sleeve': 'skin',
    'trim': 'skin',
    'jewel': 'skin',
    'cuff': ['#0e0c12', '#2c2834'],
    'glove': ['#4c4860', '#8e8aa2', '#cfcfdc', '#eeeef4', '#ffffff'],
}

LEGS = {
    'leg': ['#7a3c3a', '#c8806a', '#e09c86', '#eeb6a2', '#f8cebe', '#ffe2d8', '#fff2ec'],
    'foot': ['#08080c', '#16161c', '#24242e', '#2a5e68', '#4ccdd4'],
}

DRESS = {
    'ramp': ['#1c1824', '#4c4860', '#7a7690', '#a6a4b8', '#cbcbd8', '#e8e8f0', '#ffffff'],
    'outline': '#1c1824',
    'tiers': [(0.22, ['#0c0a10', '#26222e', '#46405a'])],
    'hem': ['#26222e', '#0c0a10'],
    'hem_scallop': '#0c0a10',
    'slit': 'left',
    'slit_at': 0.55,
    'slit_open': 0.32,
    'lining': ['#1c1824', '#3a3648', '#5c5870'],
    'show': 2,
    'flare': 1,
    'max_half': 8,
}

WINGS = ['#5a566a', '#9894a8', '#cfcfdc', '#eeeef6', '#ffffff']
