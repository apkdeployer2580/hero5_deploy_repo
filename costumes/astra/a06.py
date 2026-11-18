# 06 설야 지휘관 — spiky silver hair, a high studded collar over the lower
# face, a fitted navy coat under a huge white fur mantle whose fur runs down
# the open front to the hem, gold belt, black boots. No weapon (bare hands).
from astra import kit

NAME = '설야 지휘관'

PAL = {
    # hair (kit letters)
    'X': '#262838',
    'Q': '#7a7c94',
    'q': '#a4a6bc',
    'o': '#cdd0e0',
    'O': '#e8eaf4',
    'Y': '#ffffff',
    # navy coat and collar
    'k': '#0a0c18',
    'D': '#121834',
    'N': '#1c2650',
    'L': '#30427a',
    'G': '#c8ccdc',   # studs
    'g': '#7c84a4',   # stud shade
    # fur
    'F': '#ffffff',
    'f': '#dfe1ec',
    'x': '#a4a7bc',
    'z': '#62657e',
    # belt
    'B': '#e0b040',
    'b': '#8a6418',
}

EYE = {'iris': '#5c7ab4', 'glint': '#dce8ff', 'pupil': '#1c2848', 'lash': '#12121c'}
HAIR = {}

# The collar rises over the mouth and nose: drawn over the face, under the
# eyes, and down past the chin into the neck.
COLLAR = {
    1: {'at': (1, 14), 'over_face': True, 'rows': [
        'kNNNNNNNNNNk',
        'kLGNNNGNNNDk',
        'kLgNNNgNNNDk',
        ' kLGNNNGNDk ',
        ' kkkkkkkkkk ',
    ]},
    0: {'at': (2, 14), 'over_face': True, 'rows': [
        'kNNNNNNNNNNNk',
        'kLNGNNNNNGNDk',
        'kLNgNNNNNgNDk',
        ' kLGNNNNNGDk ',
        '  kkkkkkkkk  ',
    ]},
    4: {'at': (1, 14), 'over_face': True, 'rows': [
        'kNNNNNNNNNk',
        'kLGNNNGNNDk',
        'kLgNNNgNDk ',
        ' kLGNNGNDk ',
        ' kkkkkkkkk ',
    ]},
    6: {'at': (1, 16), 'over_face': True, 'rows': [
        'kNNNNNNNNNNk',
        'kLGNNNGNNNDk',
        ' kLgNNgNNDk ',
        ' kkkkkkkkkk ',
    ]},
    3: {'at': (3, 16), 'over_face': True, 'rows': [
        'kDNNNNNNNDk',
        'kDNNNNNNNDk',
        ' kDNNNNNDk ',
        ' kkkkkkkkk ',
    ]},
}

# v4: the hair is the game's own wild silver cut (img0/016), whitened
# (costumes/donor.py); the collar is drawn over it.
HEAD = {v: [c] for v, c in COLLAR.items()}
DONOR = {'sheet': 16,
         'hair': ['#212829', '#313c42', '#424952', '#636973', '#84869c', '#a5aec6', '#d6dbef', '#fffff7'],
         'ramp': ['#22243a', '#4a4e68', '#767a96', '#a2a6c0', '#c8cce0', '#e6e8f4', '#ffffff'],
         'skin': ['#9c5d5a', '#e79a84', '#ffdbce', '#633031', '#733839', '#e79e8c', '#bd796b', '#7b3431',
                  '#ceaa9c', '#efa273', '#e77d5a', '#b5615a']}

# The fur mantle sits on the shoulders, wider than the hair; the coat opens
# under the belt and the fur runs down both front edges to the hem.
BODY = {
    0: {'at': (-6, 0), 'rows': [
        '                      ',
        '                      ',
        '        kkkkkk        ',
        '     kkkDNNNNDkkk     ',
        '   kxFFFkNLNNDkFFFxk  ',
        '  kxFFFFFkLGNGkFFFFFxk',
        ' kxFFfFFFkLNNDkFFfFFxk',
        ' kxfFFFfFkLGNGkFfFFFxk',
        '  kxfFxffkLNNDkffxFfxk',
        '   kxxzfxkLGNGkxfzxxk ',
        '    kkNxkkLNNDkkxNkk  ',
        '     kNkBBBBBBBBkNk   ',
        '     kNkLbBbbBbNkNk   ',
        '    kDNFkLNk kNDFNDk  ',
        '    kDNFfkLk kDfFNDk  ',
        '    kDNFfk     kfFNDk ',
        '   kDNNFfk     kfFNNDk',
        '   kDNFfk       kfFNDk',
        '   kDNFfk       kfFNDk',
        '  kDNNFfk       kfFNNDk',
        '  kxFFfFk       kFfFFxk',
        '   kxxkk         kkxxk',
        '    kk             kk ',
    ]},
    1: {'at': (-6, 0), 'rows': [
        '                        ',
        '                        ',
        '         kkkkkkk        ',
        '      kkkFFFFFFFkk      ',
        '    kxFFFFfFFFFFFFxk    ',
        '   kxFFFfFFFFFfFFFFxk   ',
        '  kxFFfFFFFfFFFFfFFFxk  ',
        '   kLGNkxfFFFffFFFFxk   ',
        '   kLNNNkxxfFFxfxzxk    ',
        '   kLGNNNkkzxxzzkDNk    ',
        '   kLNNNNNNNDDNNNDNNk   ',
        '   kBBBBBBBBNNNNNDNNk   ',
        '   kLbbNLNNNNNNNNDNNk   ',
        '  kFfkLNNNNNNNNNNDDNNk  ',
        '  kFfkLNNNNNNNNNNNDDNk  ',
        '  kFfkLNNNNNNNNNNNNDNNk ',
        '  kFfkLNNNNNNNNNNNNDDNk ',
        '  kFfkLNNNNNNNNNNNNNDNk ',
        '  kFfkLNNNNNNNNNNNNNDDk ',
        '  kFFfkLNNNNNNNNNNNNNDk ',
        '  kxFFfFFfFFFfFFfFFFFxk ',
        '   kkxxkxxkxxkxxkxxxkk  ',
        '     kk  kk  kk  kk     ',
    ]},
    2: {'at': (-6, 0), 'rows': [
        '                         ',
        '                         ',
        '         kkkkkkk         ',
        '      kkkFFFFFFFkkk      ',
        '   kxxFFFFFfFFFFFFFxxk   ',
        '  kxFFFfFFFFFFFFfFFFFxk  ',
        ' kxFFfFFFFfFFFFFFFfFFFxk ',
        ' kxfFFFFfFFFFfFFFFFFfFxk ',
        '  kxfFFxFFfFFFxFFxFfxk   ',
        '   kxzxfxxzxxxzxfxzxk    ',
        '    kLNNNNNNNNNNNDDk     ',
        '    kLBBBBBBBBBBBBDk     ',
        '    kLNNNNNNNNNNNNDk     ',
        '   kLNNNNNNNNNNNNNDDk    ',
        '   kLNNNNNNNNNNNNNNDk    ',
        '  kLLNNNNNNNNNNNNNNDDk   ',
        '  kLNNNNNNNNNNNNNNNNDk   ',
        '  kLNNNNNNNNNNNNNNNNDDk  ',
        ' kLLNNNNNNNNNNNNNNNNNDk  ',
        ' kLNNNNNNNNNNNNNNNNNNDDk ',
        ' kxFFfFFFfFFfFFFfFFfFFxk ',
        '  kxxkxxkxxxkxxkxxkxxk   ',
        '   kk  kk   kk  kk  kk   ',
    ]},
}

# Navy sleeves, white fur cuffs, dark navy gloves.
ARM = {'sleeve': ['#0a0c18', '#121834', '#1c2650', '#2a3a6c', '#3c5088'],
       'cuff': ['#a4a7bc', '#ffffff'],
       'hand': ['#06070e', '#0e1224', '#1a2240', '#2a3460']}

# Navy trousers; tall black boots with a silver buckle tone.
LEG = {'trouser': ['#06070e', '#0c1022', '#141c3a', '#1e2a52', '#2c3c6e', '#3c5088'],
       'foot': ['#08070a', '#141216', '#221e24', '#36303a', '#504852', '#8a8494'],
       'foot_rows': 5,
       'band': ['#06070e', '#141c3a', '#2c3c6e']}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'NDL'

SWAY = {'head': {'loose': 'donor', 'band': 3},
        'body': {'from': {0: 13, 1: 12, 2: 12}}}



# The mantle, wider than the hair so it reads around the head.
def _widen(rows, pad):
    return [' ' * pad + r + ' ' * pad for r in rows]


def _mantle(view):
    spec = BODY[view]
    pad = 5
    rows = _widen(spec['rows'], pad)
    w = len(rows[0])
    c = w // 2
    # The mantle puffs up beside the collar, spreads over the shoulders and
    # drapes down the upper arms.
    if view == 0:
        spans = {2: [(c - 10, c - 7), (c + 6, c + 9)], 3: [(c - 12, c - 6), (c + 5, c + 11)],
                 4: (c - 13, c + 12), 5: (c - 14, c + 13), 6: (c - 14, c + 13), 7: (c - 14, c + 13),
                 8: (c - 13, c + 12), 9: [(c - 13, c - 5), (c + 4, c + 12)],
                 10: [(c - 13, c - 7), (c + 6, c + 12)], 11: [(c - 12, c - 8), (c + 7, c + 11)],
                 12: [(c - 12, c - 9), (c + 8, c + 11)]}
        keep = 'NLDGg'
    elif view == 1:
        spans = {2: (c - 7, c + 3), 3: (c - 10, c + 8), 4: (c - 12, c + 10), 5: (c - 13, c + 12),
                 6: (c - 13, c + 13), 7: (c - 11, c + 13), 8: [(c - 11, c - 6), (c - 1, c + 12)],
                 9: [(c - 10, c - 7), (c + 1, c + 11)], 10: (c + 3, c + 10), 11: (c + 4, c + 9)}
        keep = 'NLDGgB'
    else:
        spans = {2: (c - 9, c + 9), 3: (c - 12, c + 12), 4: (c - 14, c + 14), 5: (c - 15, c + 15),
                 6: (c - 15, c + 15), 7: (c - 15, c + 15), 8: (c - 14, c + 14), 9: (c - 13, c + 13),
                 10: [(c - 13, c - 8), (c + 8, c + 13)], 11: [(c - 12, c - 9), (c + 9, c + 12)]}
        keep = ''
    rows = kit.fur(rows, spans, keep=keep, seed=view)
    ax, ay = spec['at']
    return {'at': (ax - pad, ay), 'rows': rows}


BODY = {v: _mantle(v) for v in BODY}

# Weapons: ice and navy steel with fur-white fittings; the effects freeze
# pale blue.
_ICE = {'out': '#0a0c18', 'blade': '#a8d8f4', 'shine': '#f4fcff', 'edge': '#5c9cd0', 'fuller': '#7cbce4',
        'guard': '#c8ccdc', 'guard_hi': '#ffffff', 'grip': '#1c2650', 'grip_hi': '#30427a', 'gem': '#5ce0ff'}
WEAPONS = {
    'sword': {'style': 'ice', 'pal': _ICE},
    'dagger': {'style': 'stiletto', 'pal': _ICE},
    'gun': {'style': 'revolver', 'pal': dict(_ICE, body='#1c2650', body_hi='#30427a', grip='#121834',
                                              grip_hi='#1c2650', barrel='#c8ccdc', barrel_hi='#ffffff')},
    'spear': {'style': 'trident', 'pal': dict(_ICE, shaft='#1c2650', shaft_hi='#30427a', accent='#c8ccdc',
                                               tassel='#ffffff')},
    'shield': {'style': 'heater', 'pal': {'k': '#0a0c18', 'R': '#ffffff', 'r': '#a4a7bc', 'F': '#1c2650',
                                          'f': '#121834', 'E': '#a8d8f4', 'e': '#5c9cd0', 'emblem': 'diamond'}},
}
THEME = {'hue': 200, 'sat': 0.85}
