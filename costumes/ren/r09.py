# 09 고딕 레이디 — a big burgundy bow, floor-length pale rose-platinum hair
# fanning out behind her with an iridescent sheen at the shoulders, a black
# off-shoulder dress with puffed sleeves and gold embroidery, a long skirt
# slit over a crimson lining, a gold-patterned hem, black tights.
from ren import kit

NAME = '고딕 드레스'
REF = '09_gothic_lady.png'

PAL = {
    'K': '#0c0810',  # dress outline
    'k': '#1c1624',  # dress
    'h': '#2e2638',  # dress light
    'H': '#463c52',  # dress edge light
    'G': '#d4a84c',  # gold
    'g': '#8a6428',  # gold shade
    'Q': '#3a0a1c',  # bow outline
    'q': '#6c1830',  # bow
    'R': '#a02c4c',  # bow light
    'E': '#d86484',  # bow shine
    'O': '#6c5464',  # hair outline
    'b': '#a88ca2',  # hair shade
    'y': '#cfb2c4',  # hair
    'Y': '#e8d4dc',  # hair light
    'L': '#fcf2f2',  # hair shine
    'c': '#7cc4e8',  # iridescent blue
    'C': '#a8f0d0',  # iridescent green
    'S': '#ffe7de',  # skin light
    's': '#efb6a5',  # skin
    'r': '#c67d63',  # skin shade
}

HAIR = ['#6c5464', '#8c7488', '#ae94aa', '#cfb2c4', '#e2ccd6', '#f2e4e8', '#fcf2f2']
ORNAMENT = 'hair'

BOW_FRONT = {'at': (6, -4), 'rows': [
    ' QQ    QQ ',
    'QqRQ  QRqQ',
    'QqERQQREqQ',
    'QqRRqqRRqQ',
    'QqRQqqQRqQ',
    ' QQ QQ QQ ',
]}
BOW_SIDE = {'at': (7, -4), 'rows': [
    '  QQ   QQ',
    ' QqRQ QRqQ',
    'QqERQQqRqQ',
    'QqRRqqRqQ ',
    ' QQQqqQQQ ',
    '    QQ    ',
]}
BOW_BACK = {'at': (4, -4), 'rows': [
    ' QQ    QQ ',
    'QqqQ  QqqQ',
    'QqRqQQqRqQ',
    'QqqqqqqqqQ',
    'QqqQqqQqqQ',
    ' QQ QQ QQ ',
]}

# In front: a long lock over each shoulder, iridescent where it leaves the
# head, and the fan of hair behind her showing past her sides.
LOCK_L = kit.lock([(13, 1, 3), (14, 1, 3), (15, 0, 4), (16, 0, 4), (17, -1, 4), (18, -1, 4), (19, -1, 4),
                   (20, -1, 4), (21, -1, 3), (22, -1, 3), (23, -1, 3), (24, -1, 3), (25, -2, 3), (26, -2, 3),
                   (27, -2, 3), (28, -2, 3), (29, -2, 3), (30, -3, 3), (31, -3, 3), (32, -3, 3)], shine={17, 26})
LOCK_R = kit.lock([(13, 14, 3), (14, 14, 3), (15, 14, 4), (16, 15, 4), (17, 16, 4), (18, 16, 4), (19, 16, 4),
                   (20, 16, 4), (21, 17, 3), (22, 17, 3), (23, 17, 3), (24, 17, 3), (25, 18, 3), (26, 18, 3),
                   (27, 18, 3), (28, 18, 3), (29, 18, 3), (30, 19, 3), (31, 19, 3), (32, 19, 3)], shine={17, 26})
FAN_L = kit.lock([(16, -2, 3), (17, -3, 3), (18, -3, 3), (19, -4, 3), (20, -4, 3), (21, -4, 4), (22, -5, 4),
                  (23, -5, 4), (24, -5, 4), (25, -6, 4), (26, -6, 4), (27, -6, 4), (28, -6, 4), (29, -7, 4),
                  (30, -7, 4), (31, -7, 4), (32, -7, 3), (33, -7, 3), (34, -6, 3)], shine={22, 30})
FAN_R = kit.lock([(16, 17, 3), (17, 18, 3), (18, 18, 3), (19, 19, 3), (20, 19, 3), (21, 19, 4), (22, 20, 4),
                  (23, 20, 4), (24, 20, 4), (25, 21, 4), (26, 21, 4), (27, 21, 4), (28, 21, 4), (29, 22, 4),
                  (30, 22, 4), (31, 22, 4), (32, 22, 3), (33, 22, 3), (34, 21, 3)], shine={22, 30})
SHEEN_FRONT = {'at': (0, 14), 'rows': [' c' + ' ' * 14 + 'c', ' C' + ' ' * 14 + 'C']}
BACK_HAIR = kit.curtain([(y, 1 - min(3, (y - 13) // 3) + max(0, y - 29), 16 + min(3, (y - 13) // 3) - max(0, y - 29))
                         for y in range(13, 34)], shine={15, 16}, shade_from=28, tips=(3, 1, 4, 2, 0))
SIDE_HAIR = kit.lock([(12, 11, 4), (13, 11, 5), (14, 12, 5), (15, 12, 6), (16, 12, 6), (17, 13, 6), (18, 13, 6),
                      (19, 13, 7), (20, 13, 7), (21, 13, 7), (22, 13, 7), (23, 14, 7), (24, 14, 7), (25, 14, 7),
                      (26, 14, 7), (27, 15, 6), (28, 15, 6), (29, 15, 6), (30, 16, 5), (31, 16, 5), (32, 16, 4),
                      (33, 17, 3)], shine={15, 24}, light_side='right')
HEAD = {
    0: [FAN_L, FAN_R, LOCK_L, LOCK_R, SHEEN_FRONT, BOW_FRONT],
    1: [SIDE_HAIR, BOW_SIDE],
    2: [dict(SIDE_HAIR, at=(SIDE_HAIR['at'][0] + 1, SIDE_HAIR['at'][1])), dict(BOW_SIDE, at=(8, -4))],
    3: [BACK_HAIR, BOW_BACK],
}

TORSO = {
    33: {'at': (0, 0), 'rows': [
        '-ssss-',
        'sSSSSs',
        'sSSSSs',
        'rsSSsr',
        'KkkkkK',
        'KhGGkK',
        'KhgGkK',
        'KhGgkK',
        'KkGGkK',
        '-KgkK-',
    ]},
    34: {'at': (0, 0), 'rows': [
        '--sssss',
        '-sSSSsr',
        '-sSSSsr',
        'rsSSsrr',
        'KkkkkkK',
        'KGGhkK-',
        'KgGhkK-',
        'KGghkK-',
        'KGGhkkK',
        'KKkkkK-',
        '--KK---',
    ]},
    35: {'at': (0, 0), 'rows': [
        '--ssss--',
        '-sSSSSs-',
        '-sSSSSs-',
        '-rsSSsr-',
        '-KkkkkK-',
        '-KhkkkK-',
        '-KhkkkK-',
        '-KhkkkK-',
        'KKhkkkKK',
        'KkhkkkkK',
        '-KKKKKK-',
    ]},
}

ARM = {
    'sleeve': ['#0c0810', '#1c1624', '#2e2638', '#463c52', '#5e5270'],
    'trim': ['#8a6428', '#d4a84c'],
    'jewel': ['#d4a84c'],
    'cuff': ['#1c1624', '#2e2638'],
    'glove': ['#0c0810', '#1c1624', '#2e2638', '#463c52'],
    'hand': 'skin',
}

LEGS = {
    'leg': ['#08060c', '#141018', '#201a26', '#2e2638', '#40364e'],
}

DRESS = {
    'ramp': ['#08060c', '#141018', '#1e1828', '#2a2236', '#383048', '#4a405e'],
    'outline': '#050308',
    'light': 0.9,
    'slope': 0.45,
    'hem': ['#d4a84c', '#8a6428', '#141018'],
    'slit': 'left',
    'slit_at': 0.45,
    'slit_open': 0.35,
    'lining': ['#3a0612', '#7a1026', '#b42238'],
    'show': 2,
    'flare': 1,
    'max_half': 8,
}

WINGS = ['#4a5a90', '#78a0d0', '#a8d4e4', '#dcf0f0', '#fff6ff']
