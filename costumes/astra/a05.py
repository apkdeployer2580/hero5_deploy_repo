# 05 백각 도깨비 — thick curved violet horns, white hair falling to the knees,
# a pale face with violet eyes, an indigo kimono with pale blue figures and
# baggy trousers, a great white fur stole over the shoulders, an ivory
# curved blade.
from astra import kit

NAME = '백각 도깨비'

PAL = {
    'K': '#1a1620',   # outline (hair, horns)
    'n': '#3a2a5e',   # horn dark
    'N': '#6a54a8',   # horn mid
    'J': '#a490e0',   # horn light
    'H': '#f4f2f8',   # hair light
    'h': '#c8c6d4',   # hair mid
    'g': '#8e8ca2',   # hair shade
    # robe (kit.ROBE letters)
    'k': '#120e1e',
    'L': '#5a52a0',
    'M': '#3a3478',
    'D': '#221e4c',
    'W': '#d8e4ff',   # pale collar
    'w': '#8ea4dc',
    'R': '#16122c',   # dark sash
    'r': '#0c0a1a',
    'Q': '#2c2662',   # panel = the robe's own shade
    'q': '#1c1844',
    'G': '#9ec0ff',   # pale blue figures
    # fur
    'F': '#ffffff',
    'f': '#e0dfe8',
    'x': '#a8a7ba',
    'z': '#74738a',
}

HAIR = {'teal': ['#1a1620', '#8e8ca2', '#c8c6d4', '#f4f2f8'],
        'white': ['#8e8ca2', '#c8c6d4', '#f4f2f8', '#f4f2f8', '#ffffff'],
        'outline': '#1a1620'}
EYE = {'lash': '#1a1424', 'pupil': '#2a1a4a', 'iris': '#7a5ad0', 'glint': '#e4d8ff'}

# Thick horns curving up and out from the temples. v4: five tones — lit on
# the side toward the light (upper left), ringed every other row like horn,
# a glint near each tip.
PAL.update({'u': '#221838', 'j': '#dcd0ff'})
HORNS = [
    '  K              K  ',
    ' KjK            KjK ',
    ' KJNK          KjNK ',
    'KjJnK          KJnuK',
    'KJNnuK        KJNnuK',
    ' KjNnnK      KjNnnK ',
    ' KJJNnuK    KjJNnuK ',
    '  KKKKKK    KKKKKK  ',
]
# Long hair hanging past the jaw on both sides, the length of the torso.
STRANDS = [
    'KHhg         KHhgK',
    'KHhg         KHhgK',
    'KHhgK       KHhhgK',
    'KHhgK       KHhhgK',
    'KHhhgK      KHhhgK',
    ' KHhgK      KHhhgK',
    ' KHhgK       KHhgK',
    ' KHhgK       KHhgK',
    ' KhhgK       KhggK',
    '  KhgK       KhgK ',
    '  KhgK       KhgK ',
    '   KK         KK  ',
]
# v4: the hair is the game's own long cut (img0/027), whitened, its grey clip
# kept as silver (costumes/donor.py); below it the hair runs on to the waist —
# a lock over each shoulder and a curtain behind, painted in clumps
# (ren/kit.py, its letters O b y Y L as 1..5 here). STRANDS above are v3's.
from ren import kit as hairkit  # noqa: E402

HAIR = {}
_LONG = ['#3a3650', '#6e6a88', '#9c98b4', '#c4c2d6', '#e2e0ec', '#f4f2f8', '#ffffff']
DONOR = {'sheet': 27,
         'hair': ['#631021', '#7b1839', '#ad304a', '#ce495a', '#e7596b', '#ff7d8c', '#ffbac6'],
         'ramp': _LONG,
         'skin': ['#9c5d5a', '#e79a84', '#ffdbce', '#733839', '#6b3431', '#211c18'],
         'keep': {'#9c8e84': '#8a7eb0', '#bdb2ad': '#c8bce8', '#847973': '#5c5480', '#6b5952': '#3e3860',
                  '#fff7f7': '#ffffff'}}
PAL.update({'1': '#3a3650', '2': '#7e7a9a', '3': '#aeaac4', '4': '#d4d2e2', '5': '#f6f4fa'})
_KIT = str.maketrans('ObyYL', '12345')


def _hair(g):
    return {'at': g['at'], 'rows': [r.translate(_KIT) for r in g['rows']]}


_LOCK_L = _hair(hairkit.lock(hairkit.wave(14, 24, -2, 5, amp=1, period=8, grow=-0.15), shine={17}))
_LOCK_R = _hair(hairkit.lock(hairkit.wave(14, 24, 13, 5, amp=1, period=8, phase=2, grow=-0.15), shine={17},
                             light_side='right'))
_LOCK_SIDE = _hair(hairkit.lock(hairkit.wave(14, 24, 9, 5, amp=1, period=8, grow=-0.15), shine={17},
                                light_side='right'))
_CURTAIN = _hair(hairkit.curtain([(y, 1, 15) for y in range(15, 30)], shine=(), part=4,
                                 tips=(2, 1, 3, 0, 2), shade_from=26))
HEAD = {
    1: [{'at': (-2, -6), 'rows': HORNS}, _LOCK_SIDE],
    0: [{'at': (-2, -6), 'rows': HORNS}, _LOCK_L, _LOCK_R],
    3: [{'at': (-2, -6), 'rows': HORNS}, _CURTAIN],
    4: [{'at': (-2, -6), 'rows': HORNS}, _LOCK_SIDE],
    6: [{'at': (-2, -5), 'rows': HORNS}, dict(_LOCK_SIDE, at=(_LOCK_SIDE['at'][0], _LOCK_SIDE['at'][1] + 2))],
}
HAIR_BACK = {'at': (-3, -5), 'rows': [
    ' KHhK ',
    'KHhhgK',
    'KHhhgK',
    'KHhggK',
    'KHhhgK',
    'KHhhgK',
    ' KHhgK',
    ' KHhgK',
    ' KhhgK',
    ' KhggK',
    '  KhgK',
    '  KhgK',
    '   KK ',
]}

# The kimono, under a fur stole wider than the hair.
def _stole(view, spec):
    pad = 4
    rows = [' ' * pad + r + ' ' * pad for r in spec['rows']]
    rows = [r.replace('P', 'Q').replace('p', 'q') for r in rows]
    w = max(len(r) for r in rows)
    c = w // 2
    if view == 0:
        spans = {4: (c - 9, c + 8), 5: (c - 11, c + 10), 6: (c - 12, c + 11), 7: (c - 12, c + 11),
                 8: (c - 11, c + 10), 9: (c - 9, c + 8)}
        keep = 'WwMDL'
    elif view == 1:
        spans = {3: (c - 8, c + 6), 4: (c - 10, c + 9), 5: (c - 11, c + 11), 6: (c - 10, c + 11),
                 7: (c - 8, c + 10), 8: (c - 5, c + 9)}
        keep = 'WwMDL'
    else:
        spans = {3: (c - 8, c + 8), 4: (c - 11, c + 11), 5: (c - 12, c + 12), 6: (c - 13, c + 13),
                 7: (c - 12, c + 12), 8: (c - 10, c + 10)}
        keep = ''
    if view != 2:
        # the fur rests on the shoulders, not over the collar
        for y in list(spans):
            l, r = spans[y]
            spans[y] = (l, r)
    rows = kit.fur(rows, spans, keep=keep if view == 2 else 'Ww', seed=view + 5)
    # pale blue figures on the kimono
    out = []
    for y, r in enumerate(rows):
        r = list(r)
        for x in range(1, len(r) - 1):
            if r[x] == 'M' and y > 12 and (x * 3 + y * 7) % 23 == 0:
                r[x] = 'G'
        out.append(''.join(r))
    ax, ay = spec['at']
    return {'at': (ax - pad, ay), 'rows': out}


BODY = {v: _stole(v, s) for v, s in kit.robe().items()}

ARM = {'sleeve': ['#120e1e', '#221e4c', '#3a3478', '#5a52a0', '#7a72c0'],
       'cuff': ['#a8a7ba', '#ffffff'],
       'hand': ['#6a5a6a', '#b8a8b8', '#ece0ea', '#fff6fc'],
       'hand_len': 2}

LEG = {'trouser': ['#0e0a1a', '#1a163a', '#2a2560', '#3a3478', '#4a4490', '#5a52a0'],
       'foot': ['#0e0a1a', '#1a163a', '#2a2560', '#3a3478', '#4a4490', '#5a52a0'],
       'band': ['#0e0a1a', '#2a2560', '#4a4490']}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'MLDQ'

SWAY = {'head': {'loose': 'donor', 'loose_hex': ['#3a3650', '#7e7a9a', '#aeaac4', '#d4d2e2', '#f6f4fa'],
                 'band': 0, 'hang': 13, 'hang_step': 2},
        'body': {'from': {0: 12, 1: 12, 2: 12}},
        'back_step': 2}

# Weapons: ivory and bone with violet wraps; the effects glow violet.
_BONE = {'out': '#3a3630', 'blade': '#e4dcc4', 'shine': '#fffaea', 'edge': '#b8ae94', 'fuller': '#a49a80',
         'guard': '#c8c0a8', 'guard_hi': '#f0ecd8', 'grip': '#3a3478', 'grip_hi': '#6a62a0', 'gem': '#a490e0'}
WEAPONS = {
    'sword': {'style': 'curved', 'pal': _BONE},
    'dagger': {'style': 'claw', 'pal': _BONE},
    'gun': {'style': 'cannon', 'pal': dict(_BONE, body='#3a3478', body_hi='#6a62a0', grip='#c8c0a8',
                                            grip_hi='#f0ecd8', barrel='#e4dcc4', barrel_hi='#fffaea')},
    'spear': {'style': 'glaive', 'pal': dict(_BONE, shaft='#221e4c', shaft_hi='#3a3478', accent='#c8c0a8',
                                              tassel='#f4f2f8')},
    'shield': {'style': 'round', 'pal': {'k': '#120e1e', 'R': '#c8c0a8', 'r': '#8a8070', 'F': '#3a3478',
                                         'f': '#221e4c', 'E': '#f0ecd8', 'e': '#c8c0a8', 'emblem': 'fang'}},
}
THEME = {'hue': 268, 'sat': 1.0}
