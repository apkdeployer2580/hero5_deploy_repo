# 08 홍련 제독 — black-gold officer cap, an open face with red eyes, a black
# double-breasted uniform under a long crimson coat with gold trim falling to
# the ankles, gold epaulettes, white gloves with red cuffs, black boots.
# No weapon (bare hands).
from astra import kit
from astra.a07 import CAP_34, CAP_FRONT, CAP_BACK, CAP_SIDE  # noqa: E402

NAME = '홍련 제독'

PAL = {
    'K': '#14121c',
    'd': '#1e1c28',
    'm': '#2e2c3a',
    'l': '#46445a',
    'G': '#d8a848',
    'g': '#9a7030',
    'Y': '#f8e080',
    # coat (kit.OFFICER letters): black jacket, crimson coat
    'k': '#0e080c',
    'J': '#3c3a46',
    'j': '#24222c',
    'i': '#141218',
    'C': '#e0303e',
    'c': '#b01c2e',
    'e': '#6a0e1c',
    'T': '#f0c050',
    't': '#a07828',
    'p': '#d8a848',
}

HAIR = {'teal': ['#14121c', '#1e1c28', '#2e2c3a', '#46445a'],
        'white': ['#1e1c28', '#2e2c3a', '#46445a', '#46445a', '#5a586e'],
        'outline': '#14121c'}
EYE = {'lash': '#120808', 'pupil': '#4a0a14', 'iris': '#d02838', 'glint': '#ffd8d0'}

# v4: the game's own officer's cap (img0/113) in black with a gold band and a
# red badge (costumes/donor.py).
HEAD = {}
DONOR = {'sheet': 113, 'groups': [
    {'cols': ['#422431', '#523442', '#6b515a', '#947584', '#bda6ad'],
     'ramp': ['#0e0c12', '#1a1820', '#2a2832', '#3e3c4a', '#5c5a6a']},
    {'cols': ['#211418'], 'map': {'#211418': '#060508'}},
    {'map': {'#ad5952': '#3a1620', '#e7a629': '#f0c050', '#731421': '#6a0e1c', '#ce0418': '#e0303e'}},
    {'cols': ['#39414a', '#39555a', '#526d6b', '#84b2b5', '#7b695a', '#9c8e84', '#bdb2ad', '#dedbd6', '#ffffff'],
     'ramp': ['#0c0a10', '#16141c', '#24222c', '#36343e', '#4a4854']},
]}

BODY = kit.officer_open()

ARM = {'sleeve': ['#0e080c', '#141218', '#24222c', '#3c3a46', '#545260'],
       'cuff': ['#6a0e1c', '#b01c2e', '#e0303e'],
       'hand': ['#8a8ca0', '#c8cad8', '#eceef6', '#ffffff'],
       'hand_len': 2}

LEG = {'trouser': ['#06050a', '#0e0c12', '#18161e', '#24222c', '#34323e', '#46444f'],
       'foot': ['#06050a', '#0e0c12', '#1c1a22', '#2e2c36', '#a07828', '#f0c050'],
       'foot_rows': 6,
       'band': ['#a07828', '#f0c050']}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'Cce'

SWAY = {'head': {'rigid': True},
        'body': {'from': {0: 13, 1: 12, 2: 11}}}

# Weapons: polished steel with crimson and gold; the effects flare crimson.
_RED = {'out': '#0e080c', 'blade': '#c0c4d0', 'shine': '#ffffff', 'edge': '#80869a', 'fuller': '#62687c',
        'guard': '#f0c050', 'guard_hi': '#fff0a0', 'grip': '#b01c2e', 'grip_hi': '#e0303e', 'gem': '#e0303e'}
WEAPONS = {
    'sword': {'style': 'saber', 'pal': _RED},
    'dagger': {'style': 'leaf', 'pal': _RED},
    'gun': {'style': 'flintlock', 'pal': dict(_RED, body='#b01c2e', body_hi='#e0303e', grip='#24222c',
                                               grip_hi='#3c3a46', barrel='#24222c', barrel_hi='#545260')},
    'spear': {'style': 'lance', 'pal': dict(_RED, shaft='#b01c2e', shaft_hi='#e0303e', accent='#f0c050',
                                             tassel='#f0c050')},
    'shield': {'style': 'heater', 'pal': {'k': '#0e080c', 'R': '#f0c050', 'r': '#a07828', 'F': '#e0303e',
                                          'f': '#b01c2e', 'E': '#f0c050', 'e': '#a07828', 'emblem': 'cross'}},
}
THEME = {'hue': 354, 'sat': 1.0}
