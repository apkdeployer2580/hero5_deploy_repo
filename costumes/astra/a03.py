# 03 백야 검사 — tousled silver hair and blue eyes, white fur on the
# shoulders, a long white coat with gold trim and jade studs open over a red
# inner robe and a gold obi, black gloves, gold-trimmed shoes, a gold-hilted
# long sword.
from astra import kit

NAME = '백야 검사'

PAL = {
    'K': '#3a3a4c',   # hair outline
    'q': '#b8b8c8',   # hair shade
    'o': '#dcdce6',   # hair mid
    'O': '#f4f4fa',   # hair light
    # coat (kit.OFFICER letters): red inner robe, white coat, gold trim
    'k': '#1e1a22',
    'J': '#d8303e',
    'j': '#a81e2e',
    'i': '#5a0e1a',
    'C': '#f4f1ea',
    'c': '#d6d0c6',
    'e': '#9a9488',
    'T': '#e6b84a',
    't': '#9a7030',
    'p': '#e6b84a',
    'Z': '#3cbcae',   # jade
    # fur (kit.fur letters F f x z, outline k)
    'F': '#ffffff',
    'f': '#e2e0ea',
    'x': '#aeacbc',
    'z': '#7c7a8e',
}
EYE = {'lash': '#14141e', 'pupil': '#1a3a6a', 'iris': '#3a8ad8', 'glint': '#c8ecff'}

# The game's own tousled hair, silvered, with a few tufts on top (the base
# hair is already this cut).
HAIR = {'teal': ['#5a5a70', '#aeaec0', '#d4d4e0', '#f2f2f8'],
        'white': ['#9c9cae', '#c4c4d2', '#e0e0ea', '#f2f2f8', '#ffffff'],
        'outline': '#3a3a4c'}

# The game's own tousled cut, silvered — it is already the draft's hair.
HEAD = {}

# The officer's coat in white over a red robe; the epaulettes become fur, and
# a jade stud sits on each coat panel.
def _coat(view, spec):
    pad = 3
    rows = [' ' * pad + r + ' ' * pad for r in spec['rows']]
    w = max(len(r) for r in rows)
    c = w // 2
    if view == 0:
        spans = {4: (c - 8, c + 7), 5: (c - 10, c + 9), 6: (c - 11, c + 10), 7: (c - 11, c + 10),
                 8: (c - 9, c + 8)}
        keep = 'JjiT'
    elif view == 1:
        spans = {3: (c - 7, c + 5), 4: (c - 9, c + 8), 5: (c - 10, c + 10), 6: (c - 9, c + 10),
                 7: (c - 6, c + 9)}
        keep = 'JjiT'
    else:
        spans = {3: (c - 7, c + 7), 4: (c - 10, c + 10), 5: (c - 11, c + 11), 6: (c - 11, c + 11),
                 7: (c - 10, c + 10), 8: (c - 8, c + 8)}
        keep = ''
    rows = [r.replace('p', 'c') for r in rows]
    rows = kit.fur(rows, spans, keep=keep, seed=view + 3)
    out = []
    for y, r in enumerate(rows):
        r = list(r)
        if y in (14, 17):
            for x in range(len(r)):
                if r[x] == 'c' and r[x - 1] == 'C' and (x + y) % 3 == 0:
                    r[x] = 'Z'
        out.append(''.join(r))
    ax, ay = spec['at']
    return {'at': (ax - pad, ay), 'rows': out}


BODY = {v: _coat(v, s) for v, s in kit.officer_open().items()}

# White sleeves, gold cuffs, black gloves.
ARM = {'sleeve': ['#1e1a22', '#9a9488', '#d6d0c6', '#f4f1ea', '#ffffff'],
       'cuff': ['#9a7030', '#e6b84a'],
       'hand': ['#0c0a0e', '#1c1a20', '#2e2c34', '#46444e'],
       'hand_len': 2}

# Black hakama, gold-trimmed shoes over white socks.
LEG = {'trouser': ['#0c0a0e', '#16141a', '#221e26', '#2e2a34', '#3c3844', '#4a4654'],
       'foot': ['#3a2808', '#9a7030', '#e6b84a', '#f4f1ea', '#ffffff', '#ffffff'],
       'band': ['#5a0e1a', '#a81e2e', '#d8303e']}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'CcJj'

SWAY = {'head': {'loose': 'Koqo', 'band': 2},
        'body': {'from': {0: 13, 1: 12, 2: 11}}}

# Weapons: silver blades with gold fittings and jade stones; the effects
# shine jade.
_STEEL = {'out': '#1e1a22', 'blade': '#b8c0d0', 'shine': '#f4f8ff', 'edge': '#8890a4', 'fuller': '#6a7288',
          'guard': '#e6b84a', 'guard_hi': '#fff0a0', 'grip': '#a81e2e', 'grip_hi': '#d8303e', 'gem': '#3cbcae'}
WEAPONS = {
    'sword': {'style': 'long', 'pal': _STEEL},
    'dagger': {'style': 'leaf', 'pal': _STEEL},
    'gun': {'style': 'revolver', 'pal': dict(_STEEL, body='#f4f1ea', body_hi='#ffffff', grip='#d6d0c6',
                                              grip_hi='#f4f1ea', barrel='#b8c0d0', barrel_hi='#f4f8ff')},
    'spear': {'style': 'lance', 'pal': dict(_STEEL, shaft='#f4f1ea', shaft_hi='#ffffff', accent='#e6b84a',
                                             tassel='#3cbcae')},
    'shield': {'style': 'heater', 'pal': {'k': '#1e1a22', 'R': '#e6b84a', 'r': '#9a7030', 'F': '#f4f1ea',
                                          'f': '#d6d0c6', 'E': '#3cbcae', 'e': '#1e8a7e', 'emblem': 'diamond'}},
}
THEME = {'hue': 172, 'sat': 0.9}
