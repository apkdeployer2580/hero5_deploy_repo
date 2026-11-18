# 01 적염 검사 — orange spiky hair, open black sleeveless robe over a bare
# chest, red sash, dark hakama, dark katana.
from astra import kit

NAME = '적염 검사'

PAL = {
    'K': '#2a1410',   # hair outline
    'q': '#b8431a',   # hair shade
    'o': '#f0741e',   # hair mid
    'O': '#ffb03a',   # hair light
    'k': '#16121a',   # robe outline
    'd': '#2a2630',   # robe shadow
    'm': '#46404c',   # robe mid
    'h': '#6a6272',   # robe light
    'R': '#c8283c',   # sash
    'r': '#7a1826',   # sash shade
    'S': '#f3c09b',   # skin light
    's': '#b97c67',   # skin shade
    'W': '#d8d2c8',   # pale trim
}

# v3: the hair is drawn whole (the game's hair shape was a helmet under the
# recolour); the game's face stays, the eyes are their own red.
HAIR = {}
PAL.update({
    'X': '#3a0e08',   # hair outline
    'Q': '#9e2a12',   # hair deep shade
    'q': '#d4501c',   # hair shade
    'o': '#f47c22',   # hair mid
    'O': '#ffac3c',   # hair light
    'Y': '#ffdc70',   # hair highlight
    'f': '#ffdbce',   # skin light (fills the gaps the game's fringe left)
    'g': '#e79a84',   # skin mid
})

# Hair: the game's own spiky head (img0/001), recoloured ember orange —
# clumps drawn in the game's hand, the face underneath ours (costumes/donor.py).
HEAD = {}
DONOR = {'sheet': 1,
         'hair': ['#ffeba5', '#efc373', '#c68663', '#8c5952', '#733839', '#6b4539'],
         'ramp': ['#4a1208', '#94280e', '#d0441a', '#f0661e', '#ff9430', '#ffc45c'],
         'skin': ['#e79a84', '#bd796b', '#fff3e7', '#9c5d5a', '#de867b'],
         'keep': {'#ad1c31': '#c8283c', '#8c3031': '#7a1826'}}

# Body (v3): the robe is long — to mid-shin, open down the front over the
# hakama — and sleeveless over a bare chest; the red sash knots at the waist
# and its ends hang. Torso rows 0..6 are under the head; the hem (rows 13 on)
# is what the rig swings.
PAL.update({
    'k': '#120e16',   # robe outline
    'd': '#221e28',   # robe shadow
    'm': '#3a3542',   # robe mid
    'h': '#5c5666',   # robe light
    'W': '#e6e0d6',   # white trim
    'w': '#9e98a0',   # trim shade
    'S': '#f6c8a4',   # chest light
    's': '#c88a6e',   # chest shade
    'R': '#d42a3a',   # sash
    'r': '#7e1626',   # sash shade
})
BODY = {
    0: {'at': (-4, 0), 'rows': [
        '                  ',
        '                  ',
        '      kkkkkk      ',
        '     kmmmmmmk     ',
        '    kkmhSSmdkk    ',
        '   kmmhWSSWddmk   ',
        '  kmmhWSSSSWddmk  ',
        '  kmhmWSSsSWmdmk  ',
        '  kmhWSSssSSWmdk  ',
        '  kmhWSsSSsSWmdk  ',
        '  kmhRRRRRRRRmdk  ',
        '  kmhRrRRRRrRmdk  ',
        '  kmhWkRrrRkWmdk  ',
        ' kmhhWk RR kWmddk ',
        ' kmhmWk Rr kWmddk ',
        ' kmhmWk rR  kWmdk ',
        'kmhhmWk R   kWmddk',
        'kmhmmWk r   kWmmdk',
        'khmmWk      kWmmdk',
        'kmmWkk       kWmdk',
        'kkWk k       k kWk',
        ' k  k         k k ',
    ]},
    1: {'at': (-5, 0), 'rows': [
        '                    ',
        '                    ',
        '      kkkkkkk       ',
        '     kmmhmmmmk      ',
        '    kSWmhmmmdmk     ',
        '   kSSWmhmmmddmk    ',
        '   kSSsWmhmmmddmk   ',
        '   kSsSWmhmmmddmk   ',
        '   kSSsWmhmmmmddk   ',
        '   kRRRRRRRmmmddmk  ',
        '   kRrRRRrRmmmmddk  ',
        '   krRkWhhmmmmmddmk ',
        '   kRrkWhmmmmmmdddk ',
        '   krRkWhmmmmmmmddmk',
        '   kkRkWhmmmmmmmdddk',
        '    krkWhhmmmmmmmddk',
        '     kkWhmmmmmmmmddk',
        '      kWhmmmmmmmdddk',
        '      kWhmmmmmmmmddk',
        '      kkWkmmkmmkdddk',
        '       k kWk kmk kdk',
        '          k   k   k ',
    ]},
    2: {'at': (-4, 0), 'rows': [
        '                     ',
        '                     ',
        '       kkkkkkk       ',
        '     kkmmmmmmmkk     ',
        '    kmhmmmmmmmmdk    ',
        '   kmhhmmmmmmmmddk   ',
        '   kmhmmmmmmmmmddk   ',
        '   kmhmmmmmmmmmmdk   ',
        '   kmhmmmmmmmmmmdk   ',
        '   kmhmmmmmmmmmmdk   ',
        '   kmRRRRRRRRRRRdk   ',
        '   kmRrRRRRRRRrRdk   ',
        '  kmhmmmmmmmmmmmddk  ',
        '  kmhmmmmmmmmmmmddk  ',
        '  kWhmmmmmmmmmmmddk  ',
        ' kmWhmmmmmmmmmmmddWk ',
        ' kmWhmmmmmmmmmmmmdWk ',
        ' kmWhhmmmmmmmmmmmdWk ',
        'kmmWhmmmmmmmmmmmmdWdk',
        'kmWkhmmkmmmmkmmmdkWdk',
        'kWk kmk kmmk kmdk kWk',
        ' k   k   kk   k    k ',
    ]},
}

# Sleeveless: bare upper arms; dark armguards and wraps on the forearms with
# a red band at the elbow.
SKIN = ['#4a2220', '#7a3e34', '#a8604c', '#d08a6e', '#f0b896', '#ffd8bc']
ARM = {'sleeve': SKIN, 'cuff': ['#7e1626', '#d42a3a'], 'hand': ['#120e16', '#2a2632', '#4a4654', '#6e6a78']}

# Black hakama (a shade lighter than the robe so the two read apart), red
# sandals.
LEG = {'trouser': ['#0e0a12', '#1c1822', '#2a2632', '#3a3644', '#4c4858', '#5e5a6a'],
       'foot': ['#2a0a10', '#5a141e', '#8a1c2a', '#b8283a', '#d84a52', '#e8c8a0'],
       'band': ['#0e0a12', '#221e28', '#3a3644']}

EYE = {'iris': '#d8283a', 'glint': '#ffe2d6', 'pupil': '#6a1020', 'lash': '#1a0808'}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'md'

SWAY = {'head': {'loose': 'donor', 'band': 3},
        'body': {'from': {0: 13, 1: 12, 2: 13}}}


# Weapons for every class, in the robe's black and the sash's red; the
# effects burn ember orange.
_STEEL = {'out': '#0e0a10', 'blade': '#3a3644', 'shine': '#a8a4b8', 'edge': '#6a6678', 'fuller': '#24202c',
          'guard': '#c89050', 'guard_hi': '#f0c878', 'grip': '#7e1626', 'grip_hi': '#d42a3a'}
WEAPONS = {
    'sword': {'style': 'katana', 'pal': _STEEL},
    'dagger': {'style': 'tanto', 'pal': _STEEL},
    'gun': {'style': 'pistol', 'pal': dict(_STEEL, body='#26222c', body_hi='#4a4656', guard='#d42a3a',
                                            guard_hi='#ff6a4a', grip='#3a2a26', grip_hi='#6a4a3a')},
    'spear': {'style': 'yari', 'pal': dict(_STEEL, shaft='#1c1820', shaft_hi='#3a3644', guard='#c89050',
                                            tassel='#d42a3a', accent='#7e1626')},
    'shield': {'style': 'kite', 'pal': {'k': '#0e0a10', 'R': '#c89050', 'r': '#8a5a2e', 'F': '#2a2632',
                                        'f': '#1a161e', 'E': '#ff7a2a', 'e': '#c8441a', 'emblem': 'sun'}},
}
THEME = {'hue': 18, 'sat': 1.05}
