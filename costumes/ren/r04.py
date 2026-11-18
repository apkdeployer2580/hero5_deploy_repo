# 04 여신 — long straight black hair with a white lily and a lavender ribbon,
# a green gem at the throat, a white bustier and a silver girdle, a white
# front panel and long pale-gold veils at her sides, bare legs and feet.
from ren import kit

NAME = '여신'
REF = '04_goddess.png'

PAL = {
    'O': '#08060a',  # hair outline
    'b': '#1e1622',  # hair shade
    'y': '#2e2434',  # hair
    'Y': '#4a3e52',  # hair light
    'L': '#7a6c84',  # hair shine
    'W': '#ffffff',  # lily / bustier
    'w': '#d8d4e0',  # white shade
    'v': '#9a94a8',  # white deep
    'G': '#f0c850',  # gold
    'p': '#a88cd8',  # ribbon
    'e': '#2cc070',  # gem
    'E': '#a8f8c8',  # gem shine
    'A': '#c8ccd8',  # silver
    'a': '#7c8094',  # silver shade
    'S': '#ffe7de',  # skin light
    's': '#efb6a5',  # skin
    'r': '#c67d63',  # skin shade
}

HAIR = ['#08060a', '#140e16', '#1e1622', '#2a202e', '#3a3040', '#524658', '#7a6c84']
ORNAMENT = 'hair'

LILY = {'at': (0, 1), 'rows': [
    ' W  W ',
    'WwGGwW',
    ' WGwW ',
    'wWWWWw',
    ' ppvp ',
    '  p p ',
]}
LILY_SIDE = {'at': (9, 1), 'rows': [
    ' W  W ',
    'WwGGwW',
    ' WGwW ',
    'wWWWWw',
    ' pvpp ',
    ' p  p ',
]}
LILY_BACK = {'at': (12, 1), 'rows': [
    ' W  W ',
    'WwwwwW',
    ' WwwW ',
    'wWWWWw',
    ' pvpp ',
    ' p  p ',
]}
# Straight to the backs of her knees: a lock down each side in front, a
# curtain behind.
LOCK_L = kit.lock([(y, 0 if y < 16 else -1, 3) for y in range(13, 32)], shine={15, 24})
LOCK_R = kit.lock([(y, 15 if y < 16 else 16, 3) for y in range(13, 32)], shine={15, 24}, light_side='right')
BACK_HAIR = kit.curtain([(y, 1 if y < 15 else 0, 16 if y < 15 else 17) for y in range(13, 32)],
                        shine={14, 15}, part=6, tips=(1, 0, 1, 0), shade_from=27)
SIDE_HAIR = kit.lock([(y, 11 if y < 15 else 12, 5 if y < 15 else 6) for y in range(12, 32)], shine={15, 24},
                     light_side='right')
HEAD = {
    0: [LOCK_L, LOCK_R, LILY],
    1: [SIDE_HAIR, LILY_SIDE],
    2: [dict(SIDE_HAIR, at=(SIDE_HAIR['at'][0] + 1, SIDE_HAIR['at'][1])), dict(LILY_SIDE, at=(10, 1))],
    3: [BACK_HAIR, LILY_BACK],
}

TORSO = {
    33: {'at': (0, 0), 'rows': [
        '-ssss-',
        'sSSSSs',
        'sSSSSs',
        'rsSeEs',
        'wWWWWw',
        'vwWWwv',
        'rsSSsr',
        'aAAAAa',
        'aAeAAa',
        '-aaaa-',
    ]},
    34: {'at': (0, 0), 'rows': [
        '--sssss',
        '-sSSSsr',
        '-sSSSsr',
        'reEsssr',
        'WWWwvs-',
        'wWWwvr-',
        'sSSsrr-',
        'AAAAaa-',
        'AeAAaaa',
        'aaaaaa-',
        '--ss---',
    ]},
    35: {'at': (0, 0), 'rows': [
        '--ssss--',
        '-sSSSSs-',
        '-sSSSSs-',
        '-rsSSsr-',
        '-wWWWWw-',
        '-sSSSSs-',
        '-sSSSSr-',
        '-sSSSsr-',
        'aAAAAAaa',
        'aAAAAaaa',
        '-aaaaaa-',
    ]},
}

ARM = {
    'sleeve': 'skin',
    'trim': ['#a88430', '#f0c850'],
    'jewel': ['#f0c850'],
    'cuff': ['#a88430', '#f0c850'],
    'glove': 'skin',
}

LEGS = {
    'skirt': ['#8c889c', '#c8c4d4', '#ecebf2', '#ffffff'],
    'panel': ['#7c8094', '#c8ccd8'],
    'trim': ['#a88430', '#f0c850'],
    'inner': ['#c8c4d4', '#ffffff'],
    'shade': ['#5c586c', '#8c889c'],
    'leg': 'skin',
}

# The veils: long and pointed, open over her legs in front.
DRESS = {
    'ramp': ['#8a7a4c', '#c4b47c', '#e2d8a8', '#f2ead0', '#faf6e6', '#ffffff'],
    'outline': '#a8985e',
    'light': 1.0,
    'slope': 0.3,
    'open': 5,
    'open_trim': '#fffdf2',
    'open_fill': ['#c8c4d4', '#ecebf2', '#ffffff'],
    'hem_scallop': '#e2d8a8',
    'show': 1,
    'flare': 2,
    'bell': 1.2,
    'max_half': 9,
}

WINGS = ['#8a7a4c', '#c4b47c', '#e8deae', '#f8f2d8', '#fffef4']
