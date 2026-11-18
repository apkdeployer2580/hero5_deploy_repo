# 03 장교복 — the officer: peaked cap with a gold badge, navy coat trimmed in
# gold, white shirt and black tie, white shorts, black thigh boots.
NAME = '장교복'
REF = '03_officer.png'

PAL = {
    'N': '#10132a',  # outline navy-black
    'n': '#232a52',  # navy
    'm': '#323d73',  # navy mid
    'h': '#4a5aa0',  # navy highlight
    'B': '#161a36',  # cap band
    'G': '#f0c860',  # gold
    'g': '#b07e2c',  # gold shade
    't': '#4ad8cc',  # teal gem
    'K': '#0c0c12',  # visor / tie
    'k': '#2e2e3e',  # visor shine
    'W': '#f4f6fa',  # shirt
    'w': '#b8bcca',  # shirt shade
}

# Ash-platinum, warm at the light end.
HAIR = ['#3a3644', '#5e5868', '#8a8292', '#b2aab4', '#d6d0d4', '#ece6e4', '#f8f4ee']
ORNAMENT = 'hair'

CAP_FRONT = {'at': (1, -1), 'rows': [
    '   NNNNNNNNNN   ',
    ' NNmhhhhmmmmmNN ',
    'NmhhmmmmmmmmmmnN',
    ' NnmmmnGtGnnnnN ',
    '  NnmmngGgnnnN  ',
    '  NBBBBBBBBBBN  ',
    '  NgGGGGGGGGgN  ',
    '   KKKKKKKKKK   ',
    '    KkkkkkkK    ',
]}
CAP_SIDE = {'at': (0, -1), 'rows': [
    '    NNNNNNNNN    ',
    '  NNmhhhmmmmmNN  ',
    ' NmhhmmmmmmmmmnN ',
    '  NnmGtGnnnnnnN  ',
    '  NnmgGgnnnnnN   ',
    '  NBBBBBBBBBBN   ',
    '  NgGGGGGGGgBN   ',
    'KKKKKKKKKN       ',
    ' KkkkkkK         ',
]}
CAP_BACK = {'at': (1, -1), 'rows': [
    '   NNNNNNNNNN   ',
    ' NNmhhhhmmmmmNN ',
    'NmhhmmmmmmmmmmnN',
    ' NnmmmnnnnnnnnN ',
    '  NnmmnnnnnnnN  ',
    '  NBBBBBBBBBBN  ',
    '  NBggggggggBN  ',
]}
# v4: the cap is the game's own officer's cap (img0/113), navy with a gold
# band and badge, seated on her brow (costumes/donor.py fit_hat). CAP_* above
# are v3's.
HEAD = {}
DONOR_HAT = {'sheet': 113, 'views': {0: 0, 1: 1, 2: 1, 3: 3}, 'groups': [
    {'cols': ['#422431', '#523442', '#6b515a', '#947584', '#bda6ad'],
     'ramp': ['#141830', '#212a50', '#2f3c6c', '#44568e', '#6478b0']},
    {'map': {'#211418': '#0c0e1c', '#ad5952': '#161a36', '#e7a629': '#f0c860',
             '#731421': '#b07e2c', '#ce0418': '#ffe08a'}},
]}

TORSO = {
    33: {'at': (0, 0), 'rows': [
        '-NWWN-',
        'NnWWnN',
        'NGWKGN',
        'nGWKGn',
        'nnKKnn',
        'nGnnGn',
        'mnnnnm',
        'nGnnGn',
        'Nnnnnn',
        '-NnnN-',
    ]},
    34: {'at': (0, 0), 'rows': [
        '--NnnnN',
        '-NnmWWN',
        '-NmGWKN',
        'NnmGWKN',
        'NnmmKKN',
        'NnmnGnN',
        'Nnmnnn-',
        'NnmnGn-',
        'Nnmnnnn',
        'NNnnnN-',
        '--NN---',
    ]},
    35: {'at': (0, 0), 'rows': [
        '--NnnN--',
        '-NnmmnN-',
        '-NmhmnN-',
        '-NmhmnN-',
        '-Nmmnnn-',
        '-Nmmnnn-',
        '-NmnnnN-',
        '-NmGGnN-',
        'NnmnnnnN',
        'NnmnnnnN',
        '-NnnnnN-',
    ]},
}

ARM = {
    'sleeve': ['#10132a', '#1c2246', '#2a3364', '#3a4788', '#5262ac'],
    'trim': ['#b07e2c', '#f0c860'],
    'jewel': ['#f0c860'],
    'cuff': ['#b8bcca', '#f4f6fa'],
    'glove': ['#0c0c12', '#1a1a24', '#2c2c3c', '#45455c'],
}

LEGS = {
    'skirt': ['#10132a', '#1c2246', '#2a3364', '#3a4788', '#5262ac'],
    'panel': ['#7a5418', '#b07e2c', '#f0c860'],
    'trim': ['#b07e2c', '#f0c860'],
    'inner': ['#8a8ea0', '#c4c8d4', '#eef0f6'],
    'shade': ['#0c0e1e', '#161a36'],
    'leg': ['#08080c', '#121218', '#1e1e28', '#2c2c3c', '#424258', '#5c5c78'],
}

# Her light-tails stay light, in the coat's gold.
WINGS = ['#6a5a3a', '#a89060', '#d8c48a', '#f2e6bc', '#fffaf0']

# Worn (§64): ren_studio.hat_fit brings the brim down to her lash line and
# fits the crown to her skull; the row under the brim is shaded.
