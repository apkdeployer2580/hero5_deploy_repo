# 02 흑립 방랑검객 — a wide dark purple bamboo hat with strings of red and
# white beads swinging from the brim, a white mask with red marks, long pale
# hair down the back, a black-purple robe over a white collar to the ankles,
# a burgundy sash whose ends stream behind, katana.
from astra import kit

NAME = '흑립 방랑검객'

PAL = {
    'K': '#1c1626',   # hat outline
    'd': '#2c2240',   # hat dark
    'P': '#46385e',   # hat mid
    'p': '#6a5886',   # hat light
    'B': '#f0e8dc',   # white bead
    'b': '#a89880',   # bead shade
    'E': '#c42a3c',   # red bead
    'H': '#e2dee8',   # pale hair light
    'h': '#aea8bc',   # pale hair mid
    'g': '#76708a',   # pale hair shade
    'W': '#f4f0ea',   # mask / white collar
    'w': '#c8c0bc',   # mask shade
    'e': '#2a2030',   # mask eye slit
    'R': '#a8243a',   # mask mark, sash
    'r': '#5e1020',   # sash shade
    # robe (kit.ROBE letters)
    'k': '#120e18',
    'L': '#4a3e5c',
    'M': '#2e2638',
    'D': '#1c1624',
    'G': '#5a5266',   # hakama pleat light
}
# The front panel is the hakama between the robe's edges (the robe's P/p are
# the hat's letters here, so the panel is drawn as Q/q).
PAL.update({'Q': '#2a2632', 'q': '#1c1822'})

# Long pale hair: the game's own hair, recoloured, under the hat.
HAIR = {'teal': ['#1c1626', '#76708a', '#aea8bc', '#e2dee8'],
        'white': ['#76708a', '#aea8bc', '#e2dee8', '#e2dee8', '#f6f4f8'],
        'outline': '#1c1626'}
EYE = {'lash': '#2a2030', 'pupil': '#2a2030', 'iris': '#5a1020', 'glint': '#c42a3c'}

HAT = [
    '          KKK          ',
    '         KpPPK         ',
    '        KpPPPdK        ',
    '       KpPPPPddK       ',
    '      KppPPPPPddK      ',
    '     KppPPPPPPdddK     ',
    '    KppPPPPPPPPdddK    ',
    '   KppPPPPPPPPPddddK   ',
    '  KppPPPPPPPPPPPddddK  ',
    ' KppPPPPPPPPPPPPdddddK ',
    'KpppPPPPPPPPPPPPPdddddK',
    'KKKKKKKKKKKKKKKKKKKKKKK',
]
# Strings of beads off the brim, red and white, swinging with the head.
BEADS = [
    'B  E               E  B',
    'E  B               B  E',
    'B  E               E  B',
    'b  B               B  b',
    '   b               b   ',
]
# v4: the hat is painted (kit.cone) in seven tones of night purple with a
# dark rim; the bead strings hang from it as before. HAT above is v3's.
PAL.update({'1': '#0e0a16', '2': '#181224', '3': '#241c34', '4': '#322846', '5': '#42365c',
            '6': '#564874', '7': '#706090',
            'v': '#120e1a', 'x': '#1e1828', 'y': '#2c2438', 'z': '#3e3450'})
HEAD = {v: {'at': (-3, -4 if v != 6 else -3), 'rows': [r.ljust(23) for r in kit.cone(v)] + BEADS}
        for v in (1, 0, 3, 4, 6)}
NOFORM = '1234567vxyz'   # painted already: the finishing pass leaves its tones

# Mask over the 3/4 face (frame 1: face rows 11..16, x 3..12).
MASK_34 = [
    '   WWWWWWWWw ',
    '   WeeWWWeeWw',
    '   WWWWWWWWWw',
    '   WWWRWWWWw ',
    '    wWWWWWw  ',
    '     wwwww   ',
]
# Hair falling past the chin on both sides.
STRANDS_34 = [
    'KHhg        KHhg ',
    'KHhg        KHhgK',
    'KHhgK      KHhhgK',
    'KHhgK      KHhhgK',
    ' KhgK       KhhgK',
    ' KhgK       KhggK',
    '  KK         KKK ',
]
HEAD_EXTRA = {
    1: [{'at': (0, 11), 'rows': MASK_34}, {'at': (0, 13), 'rows': STRANDS_34}],
    0: [{'at': (0, 12), 'rows': [
        '  WWWWWWWWWw  ',
        '  WeeWWWWeeWw ',
        '  WWWWWWWWWWw ',
        '   WWRWWWWWw  ',
        '    wWWWWw    ',
        '     wwww     ',
    ]}, {'at': (0, 13), 'rows': [
        'KHhg        KHhg',
        'KHhg        KHhg',
        'KHhgK      KHhhg',
        'KHhgK      KHhhg',
        ' KhgK       KhhgK',
        ' KhgK       KhggK',
        '  KK         KKK ',
    ]}],
    4: [{'at': (0, 11), 'rows': [
        '   WWWWWWWw  ',
        '   WeeWWWWw  ',
        '   WWWWWWw   ',
        '   WRWWWw    ',
        '    wWWw     ',
        '     ww      ',
    ]}, {'at': (0, 13), 'rows': [
        'KHhg       KHhhg ',
        'KHhg       KHhhgK',
        'KHhgK     KHhhhgK',
        ' KhgK     KHhhhgK',
        ' KhgK      KhhhgK',
        '  KK       KhhggK',
        '            KKKK ',
    ]}],
    6: [{'at': (0, 12), 'rows': [
        '  WWWWWWWWWw ',
        '  WeeWWWWeeWw',
        '  WWWWWWWWWWw',
        '   WWRWWWWWw ',
        '    wWWWWw   ',
    ]}, {'at': (0, 13), 'rows': [
        'KHhg        KHhg ',
        'KHhg        KHhgK',
        'KHhgK      KHhhgK',
        ' KhgK      KHhhgK',
        ' KhgK       KhhgK',
        '  KK        KhggK',
        '             KKK ',
    ]}],
    # From behind the hair is one mass falling past the shoulders.
    3: [{'at': (0, 14), 'rows': [
        ' KHhHhhHhhhhhggK ',
        ' KHhHhhHhhhhhggK ',
        ' KHhHhhHhhhhhggK ',
        ' KHhHhhHhhhhhggK ',
        '  KhhhhhhhhhhgK  ',
        '  KhhhhhhhhhhgK  ',
        '   KhhhhhhhhgK   ',
        '    KKKKKKKKK    ',
    ]}],
}


for _v, _l in HEAD_EXTRA.items():
    for _g in _l:
        _g['over_face'] = True
# From behind the hair runs down to the waist.
HEAD_EXTRA[3] = [{'at': (0, 14), 'over_face': True, 'rows': [
    ' KHhHhgHhHhgHhgK ',
    ' KHhHhgHhHhgHhgK ',
    ' KHhhgKHhhgKHhgK ',
    ' KhHhgKhHhgKhHgK ',
    ' KhHhgKhHhgKhHgK ',
    '  KhHgKhHhgKhgK  ',
    '  KhhgKhhggKhgK  ',
    '  KhhgKhhggKhgK  ',
    '   KgK KhgK KgK  ',
    '   KgK KhgK KK   ',
    '    K   KK       ',
]}]
# The side view's hair-back piece: the hair falls behind the shoulder.
HAIR_BACK = {'at': (-3, -4), 'rows': [
    ' KHhK ',
    'KHhhgK',
    'KHhhgK',
    'KHhggK',
    'KHhhgK',
    ' KHhgK',
    ' KHhgK',
    ' KhggK',
    '  KhgK',
    '  KhgK',
    '   KK ',
]}

# The robe; the sash ends stream from the back and trail behind in the side
# view.
_ROBE = kit.robe()
RIBBONS = {
    0: {'at': (-6, 0), 'rows': [''] * 11 + [
        '   r                 r',
        '  kRk               kRk',
        '  kRk               kRk',
        '   kRk             kRk ',
    ]},
    1: {'at': (-6, 0), 'rows': [''] * 11 + [
        '                    kRRk ',
        '                    kRrRk',
        '                     kRRk',
        '                      kRk',
        '                     kRk ',
        '                      kRk',
        '                       kk',
    ]},
    2: {'at': (-6, 0), 'rows': [''] * 11 + [
        '         kRRkRRk         ',
        '         kRrkrRk         ',
        '         kRk kRk         ',
        '        kRk   kRk        ',
        '        kRk   kRk        ',
        '       kRk     kRk       ',
        '       kk       kk       ',
    ]},
}
BODY = {v: [{'at': _ROBE[v]['at'], 'rows': [r.replace('P', 'Q').replace('p', 'q') for r in _ROBE[v]['rows']]},
            RIBBONS[v]] for v in _ROBE}

# Wide dark sleeves, pale hands.
ARM = {'sleeve': ['#120e18', '#1c1624', '#2e2638', '#4a3e5c', '#62567a'],
       'cuff': ['#c8c0bc', '#f4f0ea'],
       'hand': ['#4a2a28', '#9c6a58', '#e8b89a', '#ffe0cc'],
       'hand_len': 2}

# Dark hakama; white tabi on wooden geta.
LEG = {'trouser': ['#0e0a12', '#1c1822', '#2a2632', '#3a3444', '#4c4658', '#5e5870'],
       'foot': ['#3a2416', '#6a4428', '#a8a0a0', '#e0dcdc', '#f4f2f2', '#ffffff'],
       'band': ['#0e0a12', '#1c1822', '#2a2632']}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'MLD'

SWAY = {'head': {'loose': 'BbEHhg', 'band': 0, 'hang': 8, 'hang_step': 1},
        'body': {'from': {0: 12, 1: 12, 2: 12}},
        'back_step': 2}

# Weapons: a long black-violet blade, bead-red fittings; the effects turn
# to a bruised magenta.
_STEEL = {'out': '#0c0812', 'blade': '#2c2638', 'shine': '#9c90b8', 'edge': '#5a4e72', 'fuller': '#1c1626',
          'guard': '#6a1420', 'guard_hi': '#b02838', 'grip': '#1c1626', 'grip_hi': '#4a3a62', 'gem': '#e8dcc8'}
WEAPONS = {
    'sword': {'style': 'katana', 'pal': _STEEL},
    'dagger': {'style': 'tanto', 'pal': _STEEL},
    'gun': {'style': 'flintlock', 'pal': dict(_STEEL, body='#2c2238', body_hi='#4a3a62', grip='#3a2a20',
                                               grip_hi='#5e4632', guard='#b02838', guard_hi='#e05060')},
    'spear': {'style': 'crescent', 'pal': dict(_STEEL, shaft='#1c1626', shaft_hi='#3a3048', tassel='#b02838',
                                                accent='#6a1420')},
    'shield': {'style': 'round', 'pal': {'k': '#0c0812', 'R': '#4a3a62', 'r': '#2c2238', 'F': '#1c1626',
                                         'f': '#120e18', 'E': '#e8dcc8', 'e': '#b02838', 'emblem': 'diamond'}},
}
THEME = {'hue': 318, 'sat': 0.95}
