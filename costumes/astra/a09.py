# 09 청운 무사 — a blue conical woven hat with a red rim, a long deep-blue
# robe to the ankles with a red collar, a red sash knotted with cords that
# swing, a black panel with gold down the front, red bracers, white shoes.
# No weapon (bare hands).
from astra import kit

NAME = '청운 무사'

PAL = {
    'K': '#141a30',   # hat outline
    'd': '#1c3268',   # hat dark
    'P': '#2a4a94',   # hat mid
    'p': '#4a72c4',   # hat light
    'b': '#c42a3a',   # hat rim (red)
    # robe (kit.ROBE letters)
    'k': '#0a1028',
    'L': '#4876d0',
    'M': '#2a54ac',
    'D': '#18347a',
    'W': '#d8303e',   # red collar
    'w': '#8a1a28',
    'R': '#d02a3a',   # sash
    'r': '#7a1626',
    'G': '#e8b848',   # gold on the panel
}

# Dark hair showing under the brim.
HAIR = {'teal': ['#141a30', '#1c2238', '#2a3048', '#3c4460'],
        'white': ['#1c2238', '#2a3048', '#3c4460', '#3c4460', '#505870'],
        'outline': '#141a30'}
EYE = {'lash': '#0c1020', 'pupil': '#141c3a', 'iris': '#3a5ca8', 'glint': '#c8dcff'}

# A shallow woven cone: the slope is one pixel out per row.
HAT = [
    '          KKK          ',
    '         KppPK         ',
    '        KppPPdK        ',
    '       KppPPPddK       ',
    '      KppPPPPPddK      ',
    '     KppPPPPPPdddK     ',
    '    KppPPPPPPPPdddK    ',
    '   KppPPPPPPPPPddddK   ',
    '  KppPPPPPPPPPPPddddK  ',
    ' KppPPPPPPPPPPPPdddddK ',
    'KbbbbbbbbbbbbbbbbbbbbbK',
    'KKKKKKKKKKKKKKKKKKKKKKK',
]
# v4: the cone is painted (kit.cone) — seven tones from the lit left flank to
# the shaded right, radial ribs that turn with the view, a weave ring every
# third row, a red rim. HAT above is v3's.
PAL.update({'1': '#0c1430', '2': '#14224e', '3': '#1c3270', '4': '#26448e', '5': '#3458ac',
            '6': '#4870c8', '7': '#6a92e0',
            'v': '#5a0e18', 'x': '#9c1c2a', 'y': '#cc2e3c', 'z': '#ee5a5a'})
HEAD = {v: {'at': (-3, -3 if v != 6 else -2), 'rows': kit.cone(v)} for v in (1, 0, 3, 4, 6)}
NOFORM = '1234567vxyz'   # painted already: the finishing pass leaves its tones

_ROBE = kit.robe()
# The panel is black with gold; letters P/p are the hat's, so the robe's
# panel is drawn in its own pair.
def _panel(rows):
    return [r.replace('P', 'Q').replace('p', 'q') for r in rows]


PAL.update({'Q': '#1c1a24', 'q': '#0e0c14'})
CORDS = {
    0: {'at': (-6, 0), 'rows': [''] * 11 + [
        '          RR',
        '         kRrk',
        '         kRk',
        '          R',
        '         kRk',
        '          r',
    ]},
    1: {'at': (-6, 0), 'rows': [''] * 11 + [
        '     kRk',
        '     kRrk',
        '      kRk',
        '      kR',
        '       r',
    ]},
}
BODY = {v: [{'at': _ROBE[v]['at'], 'rows': _panel(_ROBE[v]['rows'])}] + ([CORDS[v]] if v in CORDS else [])
        for v in _ROBE}

# Blue sleeves, red bracers, bare fists.
ARM = {'sleeve': ['#0a1028', '#18347a', '#2a54ac', '#4876d0', '#6a94e8'],
       'cuff': ['#7a1626', '#d02a3a'],
       'hand': ['#4a2a28', '#9c6a58', '#e8b89a', '#ffe0cc'],
       'hand_len': 1}

LEG = {'trouser': ['#0a1028', '#101c44', '#18347a', '#2a54ac', '#3a64bc', '#4876d0'],
       'foot': ['#141a30', '#2a3a6a', '#8a94b8', '#d8dce8', '#f4f6fa', '#ffffff'],
       'band': ['#0a1028', '#18347a', '#2a54ac']}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'MLD'

SWAY = {'head': {'rigid': True},
        'body': {'from': {0: 12, 1: 12, 2: 12}}}

# Weapons: a wanderer's blue-and-red arms with red tassels; the effects
# turn azure.
_AZ = {'out': '#0a1028', 'blade': '#b8c4d8', 'shine': '#f4f8ff', 'edge': '#7c8aa8', 'fuller': '#5c6a88',
       'guard': '#e8b848', 'guard_hi': '#f8e080', 'grip': '#d02a3a', 'grip_hi': '#ff5a5a', 'gem': '#d02a3a'}
WEAPONS = {
    'sword': {'style': 'dao', 'pal': _AZ},
    'dagger': {'style': 'leaf', 'pal': _AZ},
    'gun': {'style': 'pistol', 'pal': dict(_AZ, body='#2a54ac', body_hi='#4876d0', grip='#1c1a24',
                                            grip_hi='#3a3644', guard='#d02a3a', guard_hi='#ff5a5a')},
    'spear': {'style': 'yari', 'pal': dict(_AZ, shaft='#18347a', shaft_hi='#2a54ac', accent='#d02a3a',
                                            tassel='#d02a3a')},
    'shield': {'style': 'round', 'pal': {'k': '#0a1028', 'R': '#d02a3a', 'r': '#7a1626', 'F': '#2a54ac',
                                         'f': '#18347a', 'E': '#e8b848', 'e': '#a07828', 'emblem': 'sun'}},
}
THEME = {'hue': 214, 'sat': 1.0}
