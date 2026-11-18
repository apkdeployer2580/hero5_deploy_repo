# 02 바니걸 — black bunny ears on a headband (one folded), a high blonde side
# ponytail in a blue scrunchie, white collar and black bow tie, a black
# leotard, big cream fur cuffs, bare legs.
NAME = '바니걸'
REF = '02_bunny.png'

PAL = {
    'K': '#120e16',  # ear / leotard outline
    'k': '#2a2432',  # ear / leotard
    'h': '#4c4458',  # sheen
    'i': '#3a2c3a',  # inner ear
    'O': '#5a3c22',  # blonde outline
    'b': '#a87440',  # blonde shade
    'y': '#d8a85e',  # blonde
    'Y': '#f0cc88',  # blonde light
    'L': '#fbe6b4',  # blonde shine
    'u': '#3c6cb4',  # scrunchie blue
    'U': '#8cc4f0',  # scrunchie light
    'W': '#fbf8f4',  # collar
    'w': '#c8c0c8',  # collar shade
    'S': '#ffe7de',  # skin light
    's': '#efb6a5',  # skin
    'r': '#c67d63',  # skin shade
}

HAIR = ['#5a3c22', '#8a5c30', '#b88448', '#d8a85e', '#ecc480', '#f6dca4', '#fdeec8']
ORNAMENT = 'hair'

# Ears: the left stands straight, the right folds over to the right.
EARS_FRONT = {'at': (3, -8), 'rows': [
    '  KK         ',
    ' KhkK        ',
    ' KhikK   KKKK',
    ' KhikK  KhkikK',
    ' KhikK  KhikKK',
    ' KhikK  KhikK ',
    '  KhkK KhikK  ',
    '  KhkK KhkK   ',
    '  KhkK KhkK   ',
    '  KKkK KkK    ',
    '   KKKKKKK    ',
]}
# The ponytail, tied high on her right (the viewer's left), falling to her
# waist behind the shoulder.
TAIL_FRONT = {'at': (-5, 0), 'rows': [
    '       OOO ',
    '      OuUuO',
    '      OUuuO',
    '     OyYYO ',
    '   OOyYLO  ',
    '  OyYLyO   ',
    ' OyYLyO    ',
    ' OyYYbO    ',
    'OyYybO     ',
    'OyLYbO     ',
    'OyYybO     ',
    'ObyYbO     ',
    'ObyYyO     ',
    'ObYybO     ',
    'ObyYbO     ',
    ' ObyO      ',
    ' ObYO      ',
    ' ObyO      ',
    ' ObyO      ',
    '  ObO      ',
    '  ObO      ',
    '  OyO      ',
    '   O       ',
]}
EARS_SIDE = {'at': (4, -8), 'rows': [
    '   KK       ',
    '  KhkK      ',
    '  KhikK     ',
    '  KhikK KKKK',
    '  KhikKKhkikK',
    '  KhikKKhikKK',
    '   KhkKhikK ',
    '   KhkKhkK  ',
    '   KhkKhkK  ',
    '   KKkKKkK  ',
    '    KKKKK   ',
]}
TAIL_SIDE = {'at': (10, 0), 'rows': [
    ' OOO    ',
    'OuUuO   ',
    'OUuuyO  ',
    ' OyYLO  ',
    '  OyLYO ',
    '  OyYLyO',
    '   OyYbO',
    '   OyYbO',
    '   ObYyO',
    '   ObyYO',
    '   ObyYO',
    '   ObYyO',
    '   ObyYO',
    '    ObyO',
    '    ObYO',
    '    ObyO',
    '    ObyO',
    '    ObyO',
    '     ObO',
    '     ObO',
    '     OyO',
    '      O ',
]}
EARS_BACK = {'at': (3, -8), 'rows': [
    '         KK  ',
    '        KkkK ',
    'KKKK   KkkkK ',
    'KkkkK  KkkkK ',
    'KKkkkK KkkkK ',
    ' KKkkK KkkkK ',
    '  KkkK KkkK  ',
    '   KkkKKkkK  ',
    '   KkkKKkkK  ',
    '   KkKK KkK  ',
    '   KKKKKKKK  ',
]}
TAIL_BACK = {'at': (12, 0), 'rows': [
    ' OOO    ',
    'OuUuO   ',
    'OuUUyO  ',
    ' OyLYO  ',
    '  OyLYO ',
    '  OyYLyO',
    '   OyYbO',
    '   OyYbO',
    '   OyLbO',
    '   OyYbO',
    '   OyYbO',
    '   OYybO',
    '   OyYbO',
    '   OyYO ',
    '   OYbO ',
    '   OybO ',
    '   OybO ',
    '   OybO ',
    '   ObO  ',
    '   ObO  ',
    '   OyO  ',
    '    O   ',
]}
HEAD = {
    0: [TAIL_FRONT, EARS_FRONT],
    1: [TAIL_SIDE, EARS_SIDE],
    2: [dict(TAIL_SIDE, at=(10, 0)), dict(EARS_SIDE, at=(5, -8))],
    3: [TAIL_BACK, EARS_BACK],
}

TORSO = {
    33: {'at': (0, 0), 'rows': [
        '-ssss-',
        'sSSSSs',
        'sSSSSs',
        'sWKKWs',
        'rsSSsr',
        'KkssKk',
        'KhkkkK',
        'KhkkkK',
        'KkkkkK',
        '-KkkK-',
    ]},
    34: {'at': (0, 0), 'rows': [
        '--sssss',
        '-sSSSsr',
        '-sSSSsr',
        'sWKWssr',
        'sSSsssr',
        'kssKkKr',
        'KhkkkK-',
        'KhkkkK-',
        'KhkkkkK',
        'KkkkkK-',
        '--KK---',
    ]},
    35: {'at': (0, 0), 'rows': [
        '--ssss--',
        '-sSSSSs-',
        '-sSSSSs-',
        '-WwwwwW-',
        '-sSSSSs-',
        '-sSSSSr-',
        '-KkkkkK-',
        '-KhkkkK-',
        'KKhkkkKK',
        'KkhkkkkK',
        '-KkkkkK-',
    ]},
}

ARM = {
    'sleeve': 'skin',
    'trim': 'skin',
    'jewel': 'skin',
    'cuff': ['#4a6c9c', '#8cb0d8'],
    'glove': ['#8c7056', '#c8ae8c', '#e6d6bc', '#f8f0e2', '#fffcf6'],
}

LEGS = {
    'skirt': ['#0c0a10', '#1a1620', '#2a2432', '#3c3448', '#585070'],
    'panel': ['#1a1620', '#2a2432', '#3c3448'],
    'trim': ['#2a2432', '#3c3448'],
    'inner': ['#1a1620', '#2a2432', '#3c3448'],
    'shade': ['#0c0a10', '#1a1620'],
    'leg': ['#7a3c3a', '#c8806a', '#e09c86', '#eeb6a2', '#f8cebe', '#ffe2d8', '#fff2ec'],
}

# A high-cut leotard: black only at the hips, bare thighs at the sides.
CUT = {'base': 2, 'depth': 3}

WINGS = ['#7a5a68', '#b8909e', '#e8c8d2', '#fbe8ee', '#fffafc']
