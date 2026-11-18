# 08 마녀 — a wide-brimmed witch hat with a crooked tip and a patterned red
# band, very long wavy cream hair, a black robe open over a red dress, white
# patterned trim, a teal brooch, black boots.
from ren import kit

NAME = '마녀'
REF = '08_witch.png'

PAL = {
    'K': '#0e0c14',  # hat / robe outline
    'k': '#1e1c2a',  # hat
    'h': '#3a3a52',  # hat light
    'H': '#5a5a78',  # hat edge light
    'p': '#4a3660',  # brim underside
    'P': '#6a4e86',  # brim underside light
    'R': '#a01830',  # band red
    'r': '#5e0a1c',  # band red dark
    'g': '#e8b84a',  # band gold dot
    'c': '#3aa8d0',  # band blue dot
    'O': '#6a5a48',  # hair outline
    'b': '#a89478',  # hair shade
    'y': '#d4c2a4',  # hair
    'Y': '#ece0c8',  # hair light
    'L': '#fcf6e8',  # hair shine
    't': '#24b0b0',  # brooch
    'T': '#9cf0e8',  # brooch shine
    'W': '#ece4d6',  # trim
}

HAIR = ['#6a5a48', '#8c7a64', '#b4a286', '#d4c2a4', '#e6d8bc', '#f4ead6', '#fcf6e8']
ORNAMENT = 'hair'

HAT_FRONT = {'at': (-3, -7), 'rows': [
    '       KKK              ',
    '      KhkkK             ',
    '       KKhkK            ',
    '         KhkK           ',
    '        KHhkkK          ',
    '       KHhkkkkK         ',
    '      KHhkkkkkkK        ',
    '      KrRRgRRcRK        ',
    '      KRrRRrRRrK        ',
    '  KKKKkkkkkkkkkkKKKKKK  ',
    ' KHhhkkkkkkkkkkkkkkkhkK ',
    'KHhhkkkkkkkkkkkkkkkkkkkK',
    ' KKPppppppppppppppppPKK ',
    '   KKpppppppppppppppKK  ',
    '      KKKKKKKKKKKKKK    ',
]}
HAT_SIDE = {'at': (-4, -7), 'rows': [
    '                 KKK    ',
    '                KkkhK   ',
    '              KKkhKK    ',
    '             KkhK       ',
    '            KkkhHK      ',
    '           KkkkkhHK     ',
    '          KkkkkkkhHK    ',
    '          KRcRRgRRrK    ',
    '          KrRRrRRrRK    ',
    '   KKKKKKKkkkkkkkkkkKKK ',
    ' KKHhkkkkkkkkkkkkkkkkhK ',
    'KHhhkkkkkkkkkkkkkkkkkkK ',
    ' KKPpppppppppppppppPKK  ',
    '   KKKpppppppppppppKK   ',
    '      KKKKKKKKKKKKK     ',
]}
HAT_BACK = {'at': (-3, -7), 'rows': [
    '              KKK       ',
    '             KkkhK      ',
    '            KhkKK       ',
    '           KhkK         ',
    '          KHhkkK        ',
    '         KHhkkkkK       ',
    '        KHhkkkkkkK      ',
    '        KrRRrRRrRK      ',
    '        KRRrRRrRRK      ',
    '  KKKKKKkkkkkkkkkkKKKK  ',
    ' KHhhkkkkkkkkkkkkkkkhkK ',
    'KHhhkkkkkkkkkkkkkkkkkkkK',
    ' KKkkkkkkkkkkkkkkkkkkKK ',
    '   KKkkkkkkkkkkkkkkKK   ',
    '      KKKKKKKKKKKKK     ',
]}

# Hair: two long wavy locks in front of her shoulders, a curtain behind.
LOCK_L = kit.lock([(13, 1, 3), (14, 1, 3), (15, 0, 4), (16, 0, 4), (17, -1, 4), (18, -1, 4), (19, -2, 4),
                   (20, -2, 4), (21, -2, 4), (22, -1, 4), (23, -1, 4), (24, -2, 4), (25, -2, 4), (26, -2, 4),
                   (27, -1, 4), (28, -1, 3), (29, -2, 3), (30, -2, 3)], shine={16, 23})
LOCK_R = kit.lock([(13, 14, 3), (14, 14, 3), (15, 14, 4), (16, 15, 4), (17, 16, 4), (18, 16, 4), (19, 17, 4),
                   (20, 17, 4), (21, 17, 4), (22, 16, 4), (23, 16, 4), (24, 17, 4), (25, 17, 4), (26, 17, 4),
                   (27, 16, 4), (28, 17, 3), (29, 17, 3), (30, 17, 3)], shine={16, 23})
BACK_HAIR = kit.curtain([(y, 1 - min(1, (y - 13) // 3) + max(0, (y - 24) // 2), 16 + min(1, (y - 13) // 3) - max(0, (y - 24) // 2))
                         for y in range(13, 30)], shine={15, 16}, shade_from=25, tips=(3, 1, 2, 0))
SIDE_HAIR = kit.lock([(12, 11, 4), (13, 11, 5), (14, 12, 5), (15, 12, 5), (16, 12, 5), (17, 13, 5), (18, 13, 5),
                      (19, 13, 5), (20, 13, 5), (21, 12, 5), (22, 12, 5), (23, 13, 5), (24, 13, 5), (25, 13, 4),
                      (26, 13, 4), (27, 14, 4), (28, 14, 3), (29, 14, 3)], shine={15, 22}, light_side='right')
# v4: the hat is the game's own wizard's hat (img0/109), black-violet with its
# gold band turned red (costumes/donor.py fit_hat). HAT_* above are v3's.
HEAD = {
    0: [LOCK_L, LOCK_R],
    1: [SIDE_HAIR],
    2: [dict(SIDE_HAIR, at=(SIDE_HAIR['at'][0] + 1, SIDE_HAIR['at'][1]))],
    3: [BACK_HAIR],
}
DONOR_HAT = {'sheet': 109, 'views': {0: 0, 1: 1, 2: 1, 3: 3},
             # a witch's hat is tall: the skull ratio shortens its crown less
             'squash': {0: -2, 1: -2, 2: -2, 3: -2},
             'face_cols': ['#efbe73', '#ffe7a5', '#bd866b', '#dea673', '#ffebc6', '#ffe7c6', '#ffe3e7'],
             'groups': [
    {'cols': ['#181829', '#181c29', '#21284a', '#394163', '#39415a', '#4a518c', '#63618c', '#6b79bd',
              '#8479ad', '#adb6e7', '#b5bade', '#b5bad6'],
     'ramp': ['#0c0a12', '#16121e', '#221c2e', '#302842', '#42385a', '#5a4e78', '#7a6c98']},
    {'cols': ['#bd866b', '#dea673', '#efbe73', '#ffe7a5', '#ffebc6', '#ffe7c6'],
     'ramp': ['#2a0810', '#4a0c18', '#6a1224', '#8a1a30', '#a82a3c', '#c84a54']},
]}

TORSO = {
    33: {'at': (0, 0), 'rows': [
        '-KkkK-',
        'KkRRkK',
        'KhRRkK',
        'KhtTkK',
        'KhkRkK',
        'KhkRkK',
        'KkkRkK',
        'KWkRkW',
        'KkkRkK',
        '-KkRK-',
    ]},
    34: {'at': (0, 0), 'rows': [
        '--KkkkK',
        '-KhkRkK',
        '-KhkRkK',
        'KhtTRkK',
        'KhkkRkK',
        'KhkkRK-',
        'KhkkRK-',
        'KWkkRK-',
        'KhkkRkK',
        'KKkkkK-',
        '--KK---',
    ]},
    35: {'at': (0, 0), 'rows': [
        '--KkkK--',
        '-KkhkkK-',
        '-KhhkkK-',
        '-KhkkkK-',
        '-KhkkkK-',
        '-KhkkkK-',
        '-KhkkkK-',
        '-KhkkkK-',
        'KKhkkkKK',
        'KkhkkkkK',
        '-KKKKKK-',
    ]},
}

ARM = {
    'sleeve': ['#0e0c14', '#1e1c2a', '#2e2c40', '#44425c'],
    'trim': ['#5e0a1c', '#a01830'],
    'jewel': ['#24b0b0'],
    'cuff': ['#8c8478', '#ece4d6'],
    'glove': ['#0e0c14', '#1e1c2a', '#2e2c40', '#44425c'],
    'hand': 'skin',
}

LEGS = {
    'skirt': ['#3a0612', '#5e0a1c', '#8a1428', '#b42238', '#d84a58'],
    'panel': ['#5e0a1c', '#8a1428'],
    'trim': ['#8a1428', '#b42238'],
    'inner': ['#5e0a1c', '#8a1428', '#b42238'],
    'shade': ['#2a040c', '#3a0612'],
    'leg': ['#06060a', '#101018', '#1a1a26', '#26263a', '#363650'],
}

DRESS = {
    'ramp': ['#0a0810', '#16141e', '#22202e', '#302c40', '#423c56', '#5a5274'],
    'outline': '#06050a',
    'light': 0.9,
    'slope': 0.45,
    'hem': ['#ece4d6', '#8c8478', '#16141e'],
    'open': 6,
    'open_trim': '#ece4d6',
    'show': 2,
    'flare': 1,
    'max_half': 8,
}

WINGS = ['#3e2c5a', '#6a5490', '#9c88c4', '#c8b8e8', '#ece4fc']

# Worn (§64): ren_studio.hat_fit brings the brim down to her lash line and
# fits the crown to her skull; the row under the brim is shaded.
