# 11 적흑 권투가 — a black hood lined red over long dark hair and red eyes,
# an open, tattered black-red jacket over a bare tattooed chest, red-white
# trunks, big red boxing gloves, bandaged ankles, sandals. No weapon (fists).
from astra import kit

NAME = '적흑 권투가'

PAL = {
    'K': '#0e0c12',   # outline
    'd': '#2a2834',   # hood dark (v4: a step up, so its folds read)
    'm': '#3e3c4a',   # hood mid
    'l': '#5a5868',   # hood light
    'R': '#c82838',   # red trim / trunks
    'r': '#7a1826',   # red shade
    'k': '#14101a',   # jacket outline
    'D': '#1e1a24',   # jacket shadow
    'M': '#302a36',   # jacket mid
    'N': '#463e4e',   # jacket light
    'S': '#f2c09e',   # skin light
    's': '#c88a6e',   # skin mid
    'x': '#8a5240',   # skin shade
    't': '#3c4868',   # tattoo
    'W': '#f0ece4',   # white trim
    'Y': '#e8b848',   # gold emblem
    'c': '#14101a',   # tail outline (moves with it)
}

# Long dark hair under the hood.
HAIR = {'teal': ['#0e0c12', '#141218', '#1c1a22', '#26242e'],
        'white': ['#141218', '#1c1a22', '#26242e', '#26242e', '#34323e'],
        'outline': '#0e0c12'}
EYE = {'lash': '#0c0608', 'pupil': '#4a0a12', 'iris': '#e02838', 'glint': '#ffd0c8'}

HOOD_34 = [
    '       KKKKK      ',
    '     KKdddddKK    ',
    '    KlddddddddK   ',
    '   KlmdddddddddK  ',
    '   KlmddddddddddK ',
    '  KlmmdddddddddddK',
    '  KlmdddddddddddddK',
    '  KlmdddddddddddddK',
    '  KlmdRRRRRRRRdddddK',
    '  KlRR        RRdddK',
    '  KlR         RddddK',
    '  KlR         RddddK',
    '  KlR         RddddK',
    '  KlR         RdddK ',
    '  KlR         RdddK ',
    '  KlRR       RRddK  ',
    '   KlRR     RRddK   ',
    '   KlmRRRRRRRdddK   ',
    '    KlmmdddddddK    ',
    '     KKKKKKKKKK     ',
]
HOOD_FRONT = [
    '      KKKKKK     ',
    '    KKddddddKK   ',
    '   KlddddddddddK ',
    '  KlmddddddddddK ',
    '  KlmdddddddddddK',
    ' KlmmdddddddddddK',
    ' KlmdddddddddddddK',
    ' KlmdddddddddddddK',
    ' KlmdRRRRRRRRRddddK',
    ' KlRRR       RRRddK',
    ' KlR           RddK',
    ' KlR           RddK',
    ' KlR           RddK',
    ' KlR           RddK',
    ' KlR           RddK',
    ' KlRR         RRdK ',
    '  KlRR       RRdK  ',
    '  KlmRRRRRRRRRddK  ',
    '   KlmmddddddddK   ',
    '    KKKKKKKKKKK    ',
]
HOOD_BACK = [
    '      KKKKKK     ',
    '    KKddddddKK   ',
    '   KlddddddddddK ',
    '  KlmddddddddddK ',
    '  KlmdddddddddddK',
    ' KlmmdddddddddddK',
    ' KlmdddddddddddddK',
    ' KlmdddddddddddddK',
    ' KlmdddddddddddddK',
    ' KlmdddddddddddddK',
    ' KlmddddddddddddddK',
    ' KlmddddddddddddddK',
    ' KlmddddddddddddddK',
    ' KlmddddddddddddddK',
    ' KlmddddddddddddddK',
    ' KlmddddddddddddddK',
    '  KlmdddddddddddddK',
    '  KlmmddddddddddddK',
    '   KlmmddddddddddK ',
    '    KKKKKKKKKKKKK  ',
]
HEAD = {
    1: {'at': (-1, -3), 'rows': HOOD_34},
    0: {'at': (-1, -3), 'rows': HOOD_FRONT},
    3: {'at': (-1, -3), 'rows': HOOD_BACK},
    4: {'at': (-1, -3), 'rows': HOOD_34},
    6: {'at': (-1, -2), 'rows': HOOD_34},
}

FRONT = [
    '                      ',
    '                      ',
    '        kkkkkk        ',
    '      kkRSSSSRkk      ',
    '     kMRSSSSSSRMk     ',
    '    kNMRSSSSSsRMDk    ',
    '   kNMMRStSSStRMDDk   ',
    '   kNMRSStSStSsRMDk   ',
    '   kNMRSSttttSsRMDk   ',
    '   kNMRSStSStSsRMDk   ',
    '   kNMRsSSttSSxRMDk   ',
    '   kNMRsSSSSSsxRMDk   ',
    '   kNMRRWRRRRWRRMDk   ',
    '  cNMDkRRRRYRRRkDMDc  ',
    '  cNMDkRRrRRrRRkDMDc  ',
    '  cNDc kWRk kRWk cDDc ',
    '  cDc   kk   kk   cDc ',
    '   c               c  ',
]
SIDE = [
    '                      ',
    '                      ',
    '        kkkkkk        ',
    '      kkSSSRMMkk      ',
    '     kSSSSSRMMDk      ',
    '     kStSSSRMMDDk     ',
    '     kSttSSRMMDDk     ',
    '     kSStSSRMMDDk     ',
    '     kSSSSsRMMDDk     ',
    '     ksSSsxRMMDDk     ',
    '     kxsSxxRMMDDk     ',
    '     kWRRRRRRMDDDc    ',
    '     kRRYRRRrMDDDc    ',
    '     kRRRRrrkMDDDDc   ',
    '     kWRkkWRkcDDcDc   ',
    '      kk  kk  cc cDc  ',
    '                  cc  ',
]
BACK = [
    '                       ',
    '                       ',
    '        kkkkkkk        ',
    '      kkMMMMMMMkk      ',
    '     kNMMMMMMMMMDk     ',
    '    kNMMMMMMMMMMDDk    ',
    '   kNMMMMRRRRMMMMDDk   ',
    '   kNMMMRRrrRRMMMMDk   ',
    '   kNMMMRrRRrRMMMMDk   ',
    '   kNMMMMRRRRMMMMMDk   ',
    '   kNMMMMMMMMMMMMMDk   ',
    '   kNMMMMMMMMMMMMMDk   ',
    '   kRRRRRRRRRRRRRRRk   ',
    '  cNMDDMDMMMDMDMMDDMc  ',
    '  cNMDcMDcMMDcMDcDDMc  ',
    '  cNDc cDc cDc cDc cDc ',
    '  cDc   c   c   c   c  ',
    '   c                   ',
]
BODY = {
    0: {'at': (-6, 0), 'rows': FRONT},
    1: {'at': (-5, 0), 'rows': SIDE},
    2: {'at': (-5, 0), 'rows': BACK},
}

SKIN = ['#4a2220', '#7a3e34', '#a8604c', '#d08a6e', '#f0b896', '#ffd8bc']
# Torn black sleeves to the elbow, bare forearms, big red gloves.
ARM = {'sleeve': ['#0e0c12', '#1e1a24', '#302a36', '#463e4e', '#5a5262'],
       'cuff': SKIN,
       'hand': ['#4a0e16', '#9a1a28', '#d02838', '#f05060'],
       'hand_len': 2,
       'hand_grow': 1}

# Bare legs, white wraps at the ankles, sandals.
LEG = {'trouser': SKIN,
       'foot': ['#2a1812', '#7a3e34', '#d08a6e', '#f0b896', '#e8e4dc', '#ffffff'],
       'band': ['#a8a4a0', '#e8e4dc', '#ffffff']}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'MDNdm'

SWAY = {'head': {'rigid': True},
        'body': {'from': {0: 13, 1: 12, 2: 13}, 'loose': 'cDMN'}}

# Weapons: black steel and the gloves' red; the effects burn red.
_RB = {'out': '#0e0c12', 'blade': '#2a2632', 'shine': '#8a8698', 'edge': '#c82838', 'fuller': '#1a1620',
       'guard': '#c82838', 'guard_hi': '#f05060', 'grip': '#1e1a24', 'grip_hi': '#463e4e', 'gem': '#f0ece4'}
WEAPONS = {
    'sword': {'style': 'flame', 'pal': _RB},
    'dagger': {'style': 'claw', 'pal': _RB},
    'gun': {'style': 'pistol', 'pal': dict(_RB, body='#c82838', body_hi='#f05060', grip='#1e1a24',
                                            grip_hi='#463e4e', barrel='#2a2632', barrel_hi='#8a8698')},
    'spear': {'style': 'flame', 'pal': dict(_RB, shaft='#1e1a24', shaft_hi='#463e4e', accent='#c82838',
                                             tassel='#c82838')},
    'shield': {'style': 'round', 'pal': {'k': '#0e0c12', 'R': '#c82838', 'r': '#7a1826', 'F': '#1e1a24',
                                         'f': '#0e0c12', 'E': '#f0ece4', 'e': '#c82838', 'emblem': 'cross'}},
}
THEME = {'hue': 356, 'sat': 1.0}
