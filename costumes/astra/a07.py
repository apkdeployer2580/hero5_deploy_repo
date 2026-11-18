# 07 흑금 제독 — black-gold officer cap, a dark mask over the whole face with
# only the eyes' glint showing, a navy long coat open from the belt to the
# ankles over a navy cape, gold epaulettes, gold flames licking the hem,
# white gloves, tall navy boots. No weapon (bare hands).
from astra import kit

NAME = '흑금 제독'

PAL = {
    # cap
    'K': '#14121c',
    'd': '#1e1c28',
    'm': '#2e2c3a',
    'l': '#46445a',
    'G': '#d8a848',
    'g': '#9a7030',
    'Y': '#f8e080',
    # mask
    'v': '#14141e',
    'V': '#2a2a3a',
    # coat (kit.OFFICER letters)
    'k': '#06081a',
    'J': '#2c3c6c',
    'j': '#1c284c',
    'i': '#0e1532',
    'C': '#24325e',
    'c': '#151e40',
    'e': '#0a0f26',
    'T': '#ecbc4c',
    't': '#9a7030',
    'p': '#c8983a',
}

# Dark hair under the cap.
HAIR = {'teal': ['#14121c', '#1e1c28', '#2e2c3a', '#46445a'],
        'white': ['#1e1c28', '#2e2c3a', '#46445a', '#46445a', '#5a586e'],
        'outline': '#14121c'}
# The eyes are dark hollows in the mask; only the glint shows.
EYE = {'lash': '#05060c', 'pupil': '#05060c', 'iris': '#1a1c2c', 'glint': '#f4f4ff'}

# Peaked cap: a raised black crown, a thin gold band with the emblem over
# the face side, and a short peak.
CAP_34 = [
    '     KKKKKKKK    ',
    '   KKlmmmmmmmKK  ',
    '  KlmmmmmmmmmmmK ',
    ' KlmmmmmmmmmmmmdK',
    ' KlmmmmmmmmmmmmdK',
    ' KdYGdddddddddddK',
    ' KGGGGGGGGGGGGGgK',
    ' KKKKKKKKKKKKKKKK',
    'KddddddddK       ',
    ' KKKKKKKK        ',
]
CAP_FRONT = [
    '    KKKKKKKK    ',
    '  KKlmmmmmmmKK  ',
    ' KlmmmmmmmmmmmK ',
    ' KlmmmmmmmmmmdK ',
    ' KlmmmmmmmmmmdK ',
    ' KddddddYGdddddK',
    ' KGGGGGGGGGGGGgK',
    ' KKKKKKKKKKKKKK ',
    'KddddddddddddddK',
    ' KKKKKKKKKKKKKK ',
]
CAP_BACK = [
    '    KKKKKKKK    ',
    '  KKlmmmmmmmKK  ',
    ' KlmmmmmmmmmmmK ',
    ' KlmmmmmmmmmmdK ',
    ' KlmmmmmmmmmmdK ',
    ' KddddddddddddK ',
    ' KGGGGGGGGGGGgK ',
    ' KKKKKKKKKKKKKK ',
]
CAP_SIDE = [
    '     KKKKKKKK    ',
    '   KKlmmmmmmmKK  ',
    '  KlmmmmmmmmmmmK ',
    ' KlmmmmmmmmmmmmdK',
    ' KlmmmmmmmmmmmmdK',
    ' KdYGdddddddddddK',
    ' KGGGGGGGGGGGGGgK',
    ' KKKKKKKKKKKKKKKK',
    'KdddddddK        ',
    ' KKKKKKK         ',
]
# The mask covers the face from the brows down; the eye cells are left so
# their glint shows through.
MASK = {
    1: {'at': (2, 11), 'over_face': True, 'rows': [
        'vv vvv  vvv',
        'vv vvv  vvvv',
        'VvvvvvvvvvvV',
        'kVvvvvvvvvvk',
        ' kVvvvvvvvk ',
        '   kkkkkkk  ',
    ]},
    0: {'at': (1, 11), 'over_face': True, 'rows': [
        'vvvv  vv  vvvv',
        'vvvv  vv  vvvv',
        'VvvvvvvvvvvvvV',
        'kVvvvvvvvvvvvk',
        ' kVvvvvvvvvvk ',
        '   kkkkkkkk   ',
    ]},
    4: {'at': (2, 11), 'over_face': True, 'rows': [
        'vvv  vvvv',
        'vvv  vvvv',
        'Vvvvvvvvk',
        'kVvvvvvk ',
        ' kvvvvk  ',
        '  kkkk   ',
    ]},
    6: {'at': (2, 13), 'over_face': True, 'rows': [
        'vv vvvv  vvv',
        'vv vvvv  vvv',
        'Vvvvvvvvvvvk',
        ' kVvvvvvvvk ',
        '   kkkkkk   ',
    ]},
}

# v4: the cap is the game's own officer's cap (img0/113) in navy with a gold
# band and badge (costumes/donor.py); the mask is ours. CAP_* (v3) are kept
# for 08's record.
HEAD = {v: [m] for v, m in MASK.items()}
DONOR = {'sheet': 113, 'groups': [
    {'cols': ['#422431', '#523442', '#6b515a', '#947584', '#bda6ad'],
     'ramp': ['#121426', '#1e2440', '#2e3a60', '#465a8a', '#6c82b4']},
    {'cols': ['#211418'], 'map': {'#211418': '#0a0a16'}},
    {'map': {'#ad5952': '#1a1e34', '#e7a629': '#e8b84a', '#731421': '#a07828', '#ce0418': '#ffe27a'}},
    {'cols': ['#39414a', '#39555a', '#526d6b', '#84b2b5', '#7b695a', '#9c8e84', '#bdb2ad', '#dedbd6', '#ffffff'],
     'ramp': ['#0c0c16', '#161626', '#24243a', '#363652', '#4a4a68']},
]}

BODY = kit.officer_open()

# Gold flames licking up the hem.
FLAMES = {
    0: {17: 'T', 18: 'Tt', 19: 'TtT'},
}


def _flames(view, rows):
    out = []
    for y, r in enumerate(rows):
        r = list(r)
        if y >= len(rows) - 5 and y < len(rows) - 2:
            depth = y - (len(rows) - 5)          # 0, 1, 2
            for x, ch in enumerate(r):
                if ch in 'ce' and (x * 3 + depth * 2) % 5 < depth + 1:
                    r[x] = 'T' if (x + depth) % 2 else 't'
        out.append(''.join(r))
    return out


for _v in BODY:
    BODY[_v] = {'at': BODY[_v]['at'], 'rows': _flames(_v, BODY[_v]['rows'])}

ARM = {'sleeve': ['#06081a', '#0e1532', '#1c284c', '#2c3c6c', '#3e5288'],
       'cuff': ['#9a7030', '#ecbc4c', '#f8e080'],
       'hand': ['#8a8ca0', '#c8cad8', '#eceef6', '#ffffff'],
       'hand_len': 2}

LEG = {'trouser': ['#06081a', '#0a0f26', '#141c3c', '#1c284c', '#2c3c6c', '#3e5288'],
       'foot': ['#06081a', '#0e1532', '#1c284c', '#2c3c6c', '#9a7030', '#ecbc4c'],
       'foot_rows': 6,
       'band': ['#9a7030', '#ecbc4c']}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'Cce'

SWAY = {'head': {'rigid': True},
        'body': {'from': {0: 13, 1: 12, 2: 11}}}

# Weapons: navy and gold, a naval officer's arms; the effects burn gold.
_NAVY = {'out': '#06081a', 'blade': '#b0b6c8', 'shine': '#f2f4fa', 'edge': '#7a8098', 'fuller': '#5a6078',
         'guard': '#ecbc4c', 'guard_hi': '#f8e080', 'grip': '#151e40', 'grip_hi': '#2c3c6c', 'gem': '#f8e080'}
WEAPONS = {
    'sword': {'style': 'saber', 'pal': _NAVY},
    'dagger': {'style': 'stiletto', 'pal': _NAVY},
    'gun': {'style': 'flintlock', 'pal': dict(_NAVY, body='#151e40', body_hi='#2c3c6c', grip='#2a1c14',
                                               grip_hi='#4a3424', barrel='#24325e', barrel_hi='#4a5c94')},
    'spear': {'style': 'halberd', 'pal': dict(_NAVY, shaft='#151e40', shaft_hi='#2c3c6c', accent='#ecbc4c')},
    'shield': {'style': 'heater', 'pal': {'k': '#06081a', 'R': '#ecbc4c', 'r': '#9a7030', 'F': '#24325e',
                                          'f': '#151e40', 'E': '#f8e080', 'e': '#ecbc4c', 'emblem': 'sun'}},
}
THEME = {'hue': 44, 'sat': 0.95}
