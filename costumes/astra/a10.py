# 10 멧돼지 광전사 — an oversized grey boar-head mask with a dark bristling
# mane, a pink snout and tusks; a bare tattooed torso crossed by a red strap,
# a gold belt over a ragged brown fur skirt to the knees, black baggy
# trousers, bandaged ankles, bare feet.
from astra import kit

NAME = '멧돼지 광전사'
PREVIEW_CLASS = 1

PAL = {
    'K': '#1c1a20',   # outline
    'd': '#4a4650',   # fur dark
    'g': '#6e6a74',   # fur mid
    'G': '#92909a',   # fur light
    'H': '#b4b2bc',   # fur highlight
    'p': '#d88898',   # pink
    'P': '#f0b0bc',   # pink light
    'n': '#8a4858',   # nostril
    'e': '#1a1018',   # eye
    'W': '#f4f0e0',   # tusk
    'w': '#c8c0a8',   # tusk shade
    'b': '#3a1c1c',   # mane dark
    'B': '#6a3428',   # mane mid
    # body
    'k': '#2a1812',   # outline
    'S': '#f2c09e',   # skin light
    's': '#c88a6e',   # skin mid
    'x': '#8a5240',   # skin shade
    't': '#3c4868',   # tattoo
    'R': '#c42a32',   # strap
    'r': '#7a1620',   # strap shade
    'Y': '#f0c050',   # gold
    'y': '#a07828',   # gold shade
    'F': '#9a6a3a',   # fur skirt mid
    'f': '#5e3c20',   # fur skirt dark
    'L': '#c89a62',   # fur skirt light
    'c': '#2a1812',   # skirt outline (moves with it)
}

HAIR = {}   # the mask covers everything
EYE = None

# 3/4 (face side on the left): ears up, snout out to the left, one tusk.
MASK_34 = [
    '  KK            KK   ',
    ' KGgK          KgGK  ',
    ' KGpgK        KgpGK  ',
    ' KGppgKKKKKKKKgppGK  ',
    '  KGggggGGGgggggggK  ',
    '  KHGgggGGGGggggggdK ',
    ' KHGGggggGGGgggggggdK',
    ' KHGggggggGGggggggddK',
    ' KHGgggeegggggggggddK',
    'KHGgggggeeggggggggddK',
    'KHGgggggggggggggggddK',
    'KHGgggggggggggggggddK',
    'KHGgggggggggggggggddK',
    'KPppppKggggggggggdddK',
    'KPpPppKgggggggggggdK ',
    'KPPPPpKgggggggggggdK ',
    'KpnpnpKggggggggggddK ',
    ' KKKKKWKgggggggggdK  ',
    '     KwKKggggggddK   ',
    '      W  KKgggdKK    ',
    '           KKKK      ',
]
MASK_FRONT = [
    '  KK            KK  ',
    ' KGgK          KgGK ',
    ' KGpgK        KgpGK ',
    ' KGppgKKKKKKKKgppGK ',
    '  KGgggGGGGGGggggK  ',
    '  KHGggGGGGGGgggggK ',
    ' KHGGgggGGGGggggggdK',
    ' KHGggggggggggggggdK',
    ' KHGgeegggggggeeggdK',
    ' KHGgeegggggggeeggdK',
    ' KHGggggggggggggggdK',
    ' KHGggggggggggggggdK',
    ' KHGgggKPPPPPPKggddK',
    ' KHGgggKPpPPpPKggddK',
    '  KGgggKPnPPnPKgddK ',
    '  KGggWKPPPPPPKWddK ',
    '   KgwKKKKKKKKKwdK  ',
    '    KKgggggggggKK   ',
    '      KKKKKKKKK     ',
]
MASK_BACK = [
    '  KK            KK  ',
    ' KGgK          KgGK ',
    ' KGggK        KggGK ',
    ' KGgggKKKKKKKKgggGK ',
    '  KGgggGGGGGGggggK  ',
    '  KHGggGGGGGGgggggK ',
    ' KHGGgggGGGGggggggdK',
    ' KHGggggGGGgggggggdK',
    ' KHGggggggggggggggdK',
    ' KHGggggggggggggggdK',
    ' KHGggggggggggggggdK',
    ' KHGggggggggggggggdK',
    ' KHGggggggggggggggdK',
    ' KHGggggggggggggggdK',
    '  KGggggggggggggggdK',
    '  KGggggggggggggddK ',
    '   KggggggggggggdK  ',
    '    KKgggggggggKK   ',
    '      KKKKKKKKK     ',
]
# A dark mane bristling over the brow, between the ears.
MANE = [
    '      b  B  b     ',
    '     bBb bBbBb b  ',
    '    bBBbbBBBBbbBb ',
]
HEAD = {
    1: [{'at': (-2, -5), 'over_face': True, 'rows': MASK_34}, {'at': (0, -6), 'rows': MANE}],
    0: [{'at': (-2, -5), 'over_face': True, 'rows': MASK_FRONT}, {'at': (-1, -6), 'rows': MANE}],
    3: [{'at': (-2, -4), 'over_face': True, 'rows': MASK_BACK}, {'at': (-1, -5), 'rows': MANE}],
    4: [{'at': (-2, -5), 'over_face': True, 'rows': MASK_34}, {'at': (0, -6), 'rows': MANE}],
    6: [{'at': (-2, -4), 'over_face': True, 'rows': MASK_34}, {'at': (0, -5), 'rows': MANE}],
}

SHADE = {'S': 's', 's': 'x', 'L': 'F', 'F': 'f'}
FRONT = [
    '                    ',
    '                    ',
    '       kkkkkk       ',
    '     kkSSSSSskk     ',
    '    kSSSSSSSssxk    ',
    '   kSSRSSSSSsssxk   ',
    '   kSStRRSSSstsxk   ',
    '   kSStSSRRSssxxk   ',
    '   kSSttSSSRRsxxk   ',
    '   ksSSSSSSSsRRxk   ',
    '   kxsSSSSSssxRrk   ',
    '   kyYYYYYYYYYYyk   ',
    '   kcLFLFYYFLFLck   ',
    '  cLFLFfFyyFfFLFLc  ',
    '  cFfLFfFFFFfFLfFc  ',
    '  cLFfFLfFFfLFfLFc  ',
    '  cFfFfFfLfFfFfFfc  ',
    '   cfcFfcFfcFfcfc   ',
    '    c  c  c  c  c   ',
]
SIDE = [
    '                    ',
    '                    ',
    '       kkkkkk       ',
    '     kkSSSSSkk      ',
    '    kSSSSSSSsxk     ',
    '    kSRSSSSSsxk     ',
    '    kStRSSSssxk     ',
    '    kSttRSSsxxk     ',
    '    kSSSSRSsxxk     ',
    '    ksSSSSRsxxk     ',
    '    kxsSSSSRrxk     ',
    '    kyYYYYYYYyk     ',
    '   cLFLFYYFLFLc     ',
    '   cFLFfFyyFfFLc    ',
    '  cLFfFLfFFfLFfFc   ',
    '  cFfLFfFLfFfFLfc   ',
    '  cFfFfLfFfFfFfFFc  ',
    '   cfcFfcFfcFfcfc   ',
    '    c  c  c  c  c   ',
]
BACK = [
    '                     ',
    '                     ',
    '       kkkkkkk       ',
    '     kkSSSSSSskk     ',
    '    kSSSSSSSSssxk    ',
    '   kSSSSttSSSssxxk   ',
    '   kSSStSStSSsssxk   ',
    '   kSSSttttSSsssxk   ',
    '   kSSSStSStSssxxk   ',
    '   ksSSSSttSSsxRxk   ',
    '   kxsSSSSSSssRrxk   ',
    '   kyYYYYYYYYYYYyk   ',
    '   cLFLFLFLFLFLFLc   ',
    '  cLFLFfFLFfFLFfLFc  ',
    '  cFfLFfFFFFfFLfFFc  ',
    '  cLFfFLfFFfLFfLFFc  ',
    '  cFfFfFfLfFfFfFfFc  ',
    '   cfcFfcFfcFfcFfc   ',
    '    c  c  c  c  c    ',
]
def _strands(rows, start=12):
    """The fur skirt in hanging strands rather than a check."""
    out = []
    for y, r in enumerate(rows):
        if y > start:
            r = ''.join(('fFLF'[(x + (y // 3)) % 4] if c in 'FLf' else c) for x, c in enumerate(r))
        out.append(r)
    return out


BODY = {
    0: {'at': (-5, 0), 'rows': _strands(FRONT)},
    1: {'at': (-4, 0), 'rows': _strands(SIDE)},
    2: {'at': (-4, 0), 'rows': _strands(BACK)},
}

SKIN = ['#4a2220', '#7a3e34', '#a8604c', '#d08a6e', '#f0b896', '#ffd8bc']
# Bare arms with a dark leather bracer.
ARM = {'sleeve': SKIN, 'cuff': ['#2a1812', '#5e3c20'], 'hand': SKIN, 'hand_len': 2}

# Black baggy trousers; bandages above bare feet.
LEG = {'trouser': ['#0c0a0e', '#16141a', '#221e28', '#2e2a36', '#3c3846', '#4c4858'],
       'foot': ['#4a2220', '#a8604c', '#d08a6e', '#f0b896', '#e8e4dc', '#ffffff'],
       'band': ['#a8a4a0', '#e8e4dc', '#ffffff']}

# The boar's hide is a pelt: short bristle strokes (costumes/finish.py).
PELT = 'gG'

SWAY = {'head': {'loose': 'bB', 'band': 2},
        'body': {'from': {0: 13, 1: 13, 2: 13}, 'loose': 'cLFfYy'}}

# Weapons: crude iron and rawhide, gold-hilted twin knives (as drawn); the
# effects run amber.
_IRON = {'out': '#1c1a20', 'blade': '#8a90a4', 'shine': '#e0e4ee', 'edge': '#5c6274', 'fuller': '#4a4e5e',
         'guard': '#d8a848', 'guard_hi': '#f8e080', 'grip': '#5e3c20', 'grip_hi': '#9a6a3a', 'gem': '#c42a32'}
WEAPONS = {
    'sword': {'style': 'cleaver', 'pal': _IRON},
    'dagger': {'style': 'kris', 'pal': _IRON},
    'gun': {'style': 'double', 'pal': dict(_IRON, body='#5e3c20', body_hi='#9a6a3a', grip='#8a5a30',
                                            grip_hi='#c89a62', barrel='#5c6274', barrel_hi='#a8aec0')},
    'spear': {'style': 'leaf', 'pal': dict(_IRON, shaft='#5e3c20', shaft_hi='#9a6a3a', accent='#d8a848',
                                            tassel='#8a5a30')},
    'shield': {'style': 'round', 'pal': {'k': '#1c1a20', 'R': '#8a90a4', 'r': '#5c6274', 'F': '#8a5a30',
                                         'f': '#5e3c20', 'E': '#f4f0e0', 'e': '#c8c0a8', 'emblem': 'fang'}},
}
THEME = {'hue': 32, 'sat': 1.0}
