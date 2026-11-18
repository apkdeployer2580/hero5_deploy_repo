# 10 여름 나들이 — red hair in a low side ponytail under a wide white sun hat
# with a blue ribbon, a pink bikini top, a full black knee skirt with a red
# stripe, white frilled anklets, bare feet, a basket of oranges.
from ren import kit

NAME = '여름 나들이'
REF = '10_summer.png'

PAL = {
    'Q': '#8c8ca4',  # hat outline
    'W': '#ffffff',  # hat
    'w': '#d4d4e4',  # hat shade
    'U': '#4c94e8',  # ribbon
    'u': '#285aa8',  # ribbon shade
    'O': '#5a1a10',  # hair outline
    'b': '#a83420',  # hair shade
    'y': '#d84a2c',  # hair
    'Y': '#f47048',  # hair light
    'L': '#ffb08c',  # hair shine
    'P': '#f0609c',  # bikini
    'p': '#b03470',  # bikini shade
    'K': '#120e14',  # skirt waist
    'o': '#f08018',  # orange
    'A': '#ffc040',  # orange light
    't': '#8a5a2c',  # basket
    'T': '#c89458',  # basket light
    'S': '#ffe7de',
    's': '#efb6a5',
    'r': '#c67d63',
}

HAIR = ['#5a1a10', '#88281a', '#b43824', '#d84a2c', '#ee6640', '#f88c68', '#ffb08c']
ORNAMENT = 'hair'

HAT_FRONT = {'at': (-3, -1), 'rows': [
    '        QWWWWWWQ        ',
    '      QWWwwwwwwWWQ      ',
    '     QWwwwwwwwwwwwQ     ',
    '  UU QUUUUUUUUUUUUQ     ',
    ' UuUUQuuuuuuuuuuuuQQQQ  ',
    ' QUuUWWWWWWWWWWWWWWWWWQ ',
    'QwWuWWWWWWWWWWWWWWWWWWwQ',
    ' QwwwwwwwwwwwwwwwwwwwwQ ',
    '   QQQQQQQQQQQQQQQQQQ   ',
]}
HAT_SIDE = {'at': (-4, -1), 'rows': [
    '         QWWWWWWQ       ',
    '       QWWwwwwwwWWQ     ',
    '      QWwwwwwwwwwwwQ    ',
    '      QUUUUUUUUUUUUQUU  ',
    '   QQQQuuuuuuuuuuuuUUuU ',
    ' QWWWWWWWWWWWWWWWWWWUuQ ',
    'QwWWWWWWWWWWWWWWWWWWWwQ ',
    ' QwwwwwwwwwwwwwwwwwwwQ  ',
    '   QQQQQQQQQQQQQQQQQ    ',
]}
HAT_BACK = {'at': (-3, -1), 'rows': [
    '        QWWWWWWQ        ',
    '      QWWwwwwwwWWQ      ',
    '     QWwwwwwwwwwwwQ  UU ',
    '     QUUUUUUUUUUUUQ UUuU',
    '  QQQQuuuuuuuuuuuuQUuUQ ',
    ' QWWWWWWWWWWWWWWWWWuUWQ ',
    'QwWWWWWWWWWWWWWWWWWWWwwQ',
    ' QwwwwwwwwwwwwwwwwwwwwQ ',
    '   QQQQQQQQQQQQQQQQQQ   ',
]}
TAIL_FRONT = kit.lock([(12, 1, 3), (13, 0, 4), (14, -1, 4), (15, -1, 4), (16, -1, 4), (17, -1, 4), (18, -1, 3),
                       (19, -1, 3), (20, -1, 3), (21, 0, 3)], shine={15})
TAIL_SIDE = kit.lock([(11, 12, 4), (12, 13, 4), (13, 13, 4), (14, 14, 4), (15, 14, 4), (16, 14, 4), (17, 15, 3),
                      (18, 15, 3), (19, 15, 3), (20, 15, 3)], shine={14}, light_side='right')
TAIL_BACK = kit.lock([(12, 15, 3), (13, 15, 4), (14, 16, 4), (15, 16, 4), (16, 16, 4), (17, 16, 4), (18, 16, 3),
                      (19, 16, 3), (20, 16, 3), (21, 15, 3)], shine={15}, light_side='right')
# v4: the hat is the game's own wide-brimmed hat (img0/117), whitened, its
# folded band a blue ribbon (costumes/donor.py fit_hat). HAT_* above are v3's.
HEAD = {
    0: [TAIL_FRONT],
    1: [TAIL_SIDE],
    2: [dict(TAIL_SIDE, at=(TAIL_SIDE['at'][0] + 1, TAIL_SIDE['at'][1]))],
    3: [TAIL_BACK],
}
DONOR_HAT = {'sheet': 117, 'views': {0: 0, 1: 1, 2: 1, 3: 3},
             'face_cols': ['#ffd7bd', '#f7d3bd', '#e7967b', '#e7927b', '#a5555a', '#ad5d63', '#a5515a', '#a55d63'],
             'groups': [
    {'cols': ['#292431', '#42384a', '#423c4a', '#4a3c4a', '#4a4152', '#52495a', '#635d6b', '#63596b',
              '#7b7184', '#94869c', '#948e9c', '#bdb2c6', '#c6bace'],
     'ramp': ['#6a6a84', '#9898ae', '#bcbccc', '#d6d6e2', '#e8e8f0', '#f6f6fa', '#ffffff']},
    {'cols': ['#a5515a', '#a5555a', '#a55d63', '#ad5d63', '#e7927b', '#e7967b', '#f7d3bd', '#ffd7bd'],
     'ramp': ['#163a7a', '#2050a0', '#2c68c4', '#4c8ae0', '#78acf0', '#a8ccff']},
]}

TORSO = {
    33: {'at': (0, 0), 'rows': [
        '-ssss-',
        'sSSSSs',
        'sSSSSs',
        'sPSSPs',
        'PPpPPp',
        'pPprPp',
        'sSSSSs',
        'sSSSSr',
        'rSSSsr',
        '-KKKK-',
    ]},
    34: {'at': (0, 0), 'rows': [
        '--sssss',
        '-sSSSsr',
        '-sSSSsr',
        'sPSSsrr',
        'PPpPsr-',
        'pPprsr-',
        'sSSSsr-',
        'sSSSsr-',
        'rSSSsrr',
        'KKKKKK-',
        '--KK---',
    ]},
    35: {'at': (0, 0), 'rows': [
        '--ssss--',
        '-sSSSSs-',
        '-sSSSSs-',
        '-sSSSSs-',
        '-pPPPPp-',
        '-sSSSSs-',
        '-sSSSSr-',
        '-sSSSsr-',
        'rsSSSsrr',
        'KKKKKKKK',
        '-KKKKKK-',
    ]},
}

ARM = {
    'sleeve': 'skin',
    'trim': 'skin',
    'jewel': 'skin',
    'cuff': 'skin',
    'glove': 'skin',
}

# The basket of oranges, on her left hand.
BODY = {0: [{'at': (-2, 7), 'rows': [' oAo ', 'oAooA', 'tTtTt', 'tTtTt', ' ttt ']}]}

LEGS = {
    'leg': 'skin',
}

DRESS = {
    'ramp': ['#060408', '#16101c', '#241c2c', '#342a40', '#463a56', '#5a4c6c'],
    'outline': '#040306',
    'light': 0.95,
    'slope': 0.4,
    'length': 0.62,
    'hem': ['#ff4848', '#d02028', '#120e16'],
    'show': 2,
    'flare': 3,
    'max_half': 9,
    'bell': 0.75,
}

ANKLET = ['#ffffff', '#d4d4e4']

WINGS = ['#0a6870', '#20a0a8', '#60d8e0', '#a8f4f8', '#e8fdff']

# Worn (§64): ren_studio.hat_fit brings the brim down to her lash line and
# fits the crown to her skull; the row under the brim is shaded.
