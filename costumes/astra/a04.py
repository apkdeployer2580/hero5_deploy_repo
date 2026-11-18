# 04 태양 성기사 — a closed silver helm with a gold crest and a glowing
# visor, silver plate, great gold pauldrons breaking into flame, a white-gold
# tabard hanging to the knees front and back, silver greaves, broadsword.
from astra import kit

NAME = '태양 성기사'

PAL = {
    'K': '#1a1824',   # outline
    'd': '#464a5c',   # steel dark
    'm': '#767c92',   # steel mid
    'l': '#a6acc0',   # steel light
    'L': '#dadeea',   # steel highlight
    'G': '#e2ae44',   # gold
    'g': '#9a6c26',   # gold shade
    'Y': '#fae486',   # gold light
    'o': '#ff8c2a',   # flame
    'r': '#d8461c',   # flame deep
    'v': '#0e0e16',   # visor slit
    'V': '#ff6a3a',   # visor glow
    'W': '#f2eee6',   # tabard
    'w': '#c6c0b4',   # tabard shade
    'k': '#1a1824',   # tabard outline (moves with it)
}
# The helm's glow (v2 used 'o' for it; the flames own 'o' now).
HELM_34 = [
    '       KYK       ',
    '      KGYGK      ',
    '     KgGGGgK     ',
    '    KLllmmmmK    ',
    '  KLllllmmmmmdK  ',
    ' KLllllmmmmmmddK ',
    ' KLlllmmmmmmmddK ',
    ' KllllmmmmmmmddK ',
    ' KllGGGGGGGmmddK ',
    ' KlmmmmmmmmmmddK ',
    ' KlmmmmmmmmmmddK ',
    ' KvvvvvvvvKmmddK ',
    ' KvovvvvovKmmddK ',
    ' KlmmmmmmmmmmddK ',
    ' KlmmmmmmmmmdddK ',
    '  KlmmmmmmmmdddK ',
    '  KlmmmGGGmmddK  ',
    '   KlmmmmmmddK   ',
    '    KKKKKKKKK    ',
]
HELM_FRONT = [
    '      KYK       ',
    '     KGYGK      ',
    '    KgGGGgK     ',
    '   KLllmmmmK    ',
    ' KLllllmmmmmdK  ',
    ' KLllllmmmmmddK ',
    ' KLlllmmmmmmddK ',
    ' KllllmmmmmmddK ',
    ' KllGGGGGGGGddK ',
    ' KlmmmmmmmmmddK ',
    ' KlmmmmmmmmmddK ',
    ' KvvvvvKKvvvvdK ',
    ' KvVvvvKKvvVvdK ',
    ' KlmmmmmmmmmddK ',
    ' KlmmmmmmmmdddK ',
    '  KlmmmmmmmdddK ',
    '  KlmmmGGGmmdK  ',
    '   KlmmmmmmdK   ',
    '    KKKKKKKK    ',
]
HELM_BACK = [
    '      KYK       ',
    '     KGYGK      ',
    '    KgGGGgK     ',
    '   KLllgmmmK    ',
    ' KLllllgmmmmdK  ',
    ' KLlllllgmmmmdK ',
    ' KLlllllgmmmmdK ',
    ' KlllllllgmmmdK ',
    ' KllGGGGGGGGddK ',
    ' KlllllmmmmmddK ',
    ' KlllllmmmmmddK ',
    ' KlllllmmmmmddK ',
    ' KlllllmmmmmddK ',
    ' KllllmmmmmmddK ',
    ' KllllmmmmmdddK ',
    '  KlllmmmmmdddK ',
    '  KllmmmmmmmdK  ',
    '   KlmmmmmmdK   ',
    '    KKKKKKKK    ',
]
HELM_SIDE = [
    '       KYK       ',
    '      KGYGK      ',
    '     KgGGGgK     ',
    '    KLllmmmmK    ',
    '  KLllllmmmmmdK  ',
    ' KLllllmmmmmmddK ',
    ' KLlllmmmmmmmddK ',
    ' KllllmmmmmmmddK ',
    ' KllGGGGGGGmmddK ',
    ' KlmmmmmmmmmmddK ',
    ' KlmmmmmmmmmmddK ',
    ' KvvvvvvKmmmmddK ',
    ' KvVvvvvKmmmmddK ',
    ' KlmmmmmmmmmmddK ',
    ' KlmmmmmmmmmdddK ',
    '  KlmmmmmmmmdddK ',
    '  KlmmmGGGmmddK  ',
    '   KlmmmmmmddK   ',
    '    KKKKKKKKK    ',
]
HAIR = {}   # the helm covers everything
EYE = None   # and the eyes

# v4: the helm is the game's own (img0/082, a nasal helm with gold bands),
# its steel brightened and its gold warmed — drawn in the game's hand, not
# ours (costumes/donor.py). The HELM_* grids above are v3's, kept for the
# record.
HEAD = {}
DONOR = {'sheet': 82, 'groups': [
    {'cols': ['#393842', '#5a556b', '#84829c', '#adb2c6', '#f7eff7'],
     'ramp': ['#2a2c3e', '#565c76', '#8a92ac', '#c0c8dc', '#f6f8ff']},
    {'cols': ['#8c5952', '#c68663', '#efc373', '#ffeba5'],
     'ramp': ['#8a561c', '#cc8a2a', '#f0be4a', '#fff2a8']},
    {'cols': ['#6b2c18', '#9c3c29', '#d64939'],
     'ramp': ['#4a1a0a', '#b8461a', '#ff8a32']},
]}

# Front and back are symmetric: drawn as left halves (kit.sym), the right
# side shaded.
SHADE = {'l': 'm', 'L': 'l', 'Y': 'G', 'G': 'g', 'W': 'w'}
FRONT = kit.sym([
    '            ',
    '            ',
    '         KKK',
    ' o      KlLl',
    'Kor   KKGlLl',
    'KoYoKKGYGKLl',
    'KYYGGYYGGKll',
    ' KGYYYGGgKll',
    '  KgGGgggKll',
    '   KKKKK KlL',
    '        KllL',
    '        KGGG',
    '       KlKwW',
    '      KlmKwW',
    '      KmmKwW',
    '       KKKwW',
    '         KwW',
    '         KwW',
    '         KwW',
    '         KGG',
    '         KKK',
], SHADE)
BACK = kit.sym([
    '             ',
    '             ',
    '          KKK',
    ' o       KmlL',
    'Kor    KKGmlL',
    'KoYoK KGYGKml',
    'KYYGGKYYGGKml',
    ' KGYYYGGgKmml',
    '  KgGGgggKmml',
    '   KKKKK KmmL',
    '         KmmL',
    '         KGGG',
    '        KmKwW',
    '       KmmKwW',
    '       KmmKwW',
    '        KKKwW',
    '          KwW',
    '          KwW',
    '          KwW',
    '          KGG',
    '          KKK',
], SHADE)
SIDE = [
    '                        ',
    '                        ',
    '          KKKKK         ',
    '   o    KKlLllmKK    o  ',
    '  Kor KKGGlLlmmmK  roK  ',
    '  KoYKGYYGKlLmmmKKYoK   ',
    '  KYYGYYGGKlLmmmdKGYK   ',
    '   KGYYGGgKlLmmmdKgK    ',
    '    KggggKKlLmmmmdK     ',
    '     KKKK KlLmmmmdK     ',
    '          KlLmmmddK     ',
    '          KGGGGGGGK     ',
    '         KWKlmmmdKK     ',
    '        KWWKlmmdK       ',
    '        KWwKmmmdK       ',
    '       KWWwK KmdK       ',
    '       KWwwK  KK        ',
    '       KGWwK            ',
    '        KGWK            ',
    '         KK             ',
]
def _tabard(rows, start=12):
    """The tabard's own outline is `k`, so the rig swings it whole."""
    out = []
    for y, r in enumerate(rows):
        if y >= start:
            r = ''.join('k' if c == 'K' and ((x > 0 and r[x - 1] in 'WwG') or (x + 1 < len(r) and r[x + 1] in 'WwG')
                                              or (y + 1 < len(rows) and x < len(rows[y + 1]) and rows[y + 1][x] in 'WwG'))
                        else c for x, c in enumerate(r))
        out.append(r)
    return out


BODY = {
    0: {'at': (-7, 0), 'rows': _tabard(FRONT)},
    1: {'at': (-6, 0), 'rows': _tabard(SIDE)},
    2: {'at': (-7, 0), 'rows': _tabard(BACK)},
}

# Plate arms, gold-edged gauntlets.
ARM = {'sleeve': ['#1a1824', '#464a5c', '#767c92', '#a6acc0', '#dadeea'],
       'cuff': ['#9a6c26', '#e2ae44', '#fae486'],
       'hand': ['#1a1824', '#464a5c', '#767c92', '#a6acc0', '#dadeea'],
       'hand_len': 2}

# Silver greaves and sabatons with a gold knee.
LEG = {'trouser': ['#1a1824', '#3a3e50', '#5a6076', '#80869c', '#a6acc0', '#dadeea'],
       'foot': ['#1a1824', '#464a5c', '#767c92', '#a6acc0', '#dadeea', '#ffffff'],
       'band': ['#9a6c26', '#e2ae44', '#fae486']}

# Cloth that hangs in folds (costumes/finish.py, step 2b).
FOLDS = 'Ww'

SWAY = {'head': {'rigid': True},
        'body': {'from': {0: 13, 1: 12, 2: 13}, 'loose': 'WwGk'}}

# Weapons: holy silver with gold wings and a ruby; the effects blaze gold.
_STEEL = {'out': '#1a1824', 'blade': '#a6acc0', 'shine': '#f2f4fa', 'edge': '#767c92', 'fuller': '#5a6076',
          'guard': '#e2ae44', 'guard_hi': '#fae486', 'grip': '#464a5c', 'grip_hi': '#767c92', 'gem': '#e0283a'}
WEAPONS = {
    'sword': {'style': 'broad', 'pal': _STEEL},
    'dagger': {'style': 'stiletto', 'pal': _STEEL},
    'gun': {'style': 'cannon', 'pal': dict(_STEEL, body='#e2ae44', body_hi='#fae486', grip='#f2eee6',
                                            grip_hi='#ffffff', barrel='#a6acc0', barrel_hi='#f2f4fa')},
    'spear': {'style': 'lance', 'pal': dict(_STEEL, shaft='#f2eee6', shaft_hi='#ffffff', accent='#e2ae44',
                                             tassel='#ff8c2a')},
    'shield': {'style': 'kite', 'pal': {'k': '#1a1824', 'R': '#e2ae44', 'r': '#9a6c26', 'F': '#dadeea',
                                        'f': '#a6acc0', 'E': '#ff8c2a', 'e': '#d8461c', 'emblem': 'sun'}},
}
THEME = {'hue': 40, 'sat': 1.0}
