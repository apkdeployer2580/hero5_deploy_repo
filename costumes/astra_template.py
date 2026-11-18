#!/usr/bin/env python3
"""Print the drawing template for a head frame: what the composite keeps of
the game's head (`s` skin, the eyes as L/P/G/I) in the coordinates of a grid
placed at `at`, so a hair grid can be drawn around the face by hand.

    python costumes/astra_template.py 1 -2 -4 21 21      # frame, at x, at y, w, h

`.` is empty: whatever the grid puts there is the only pixel. The grid's own
' ' leaves the template untouched (face stays, empty stays empty).
"""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'tools_re'))
sys.path.insert(0, str(HERE))
import astra_studio as A  # noqa: E402

ROLE = {'lash': 'L', 'pupil': 'P', 'glint': 'G', 'iris': 'I'}


def template(frame, ax, ay, w, h):
    im = A.frames('c/sp/img0/000.mgr')[frame]
    keep = A.face_mask(im)
    eyes = A.EYES_BASE.get(frame, {})
    rows = []
    for gy in range(h):
        row = ''
        for gx in range(w):
            x, y = gx + ax, gy + ay
            if (x, y) in eyes:
                row += ROLE[eyes[(x, y)]]
            elif (x, y) in keep:
                row += 's' if A.hx(im.getpixel((x, y))) in A.HEAD_FACE else 'k'
            elif 0 <= x < im.width and 0 <= y < im.height and im.getpixel((x, y))[3] >= 128:
                row += ','   # a base hair pixel: cleared unless drawn
            else:
                row += '.'
        rows.append(row)
    return rows


def check(frame, at, rows):
    """`rows` laid on the template: the grid's letters, `s`/eyes where skin
    shows, `!` where a base hair pixel is left uncovered (a hole), `k` where
    the jaw outline stays."""
    w = max(len(r) for r in rows)
    t = template(frame, at[0], at[1], w, len(rows))
    out = []
    for r, tr in zip(rows, t):
        line = ''
        for ch, tc in zip(r.ljust(w), tr):
            if tc in 'sLPGI':
                line += tc
            elif ch != ' ':
                line += ch
            elif tc == ',':
                line += '!'
            elif tc == 'k':
                line += 'k'
            else:
                line += ' '
        out.append(line)
    return out


def check_design(n, frame):
    import importlib
    d = importlib.import_module('astra.a%02d' % n)
    specs = d.HEAD[frame] if isinstance(d.HEAD[frame], list) else [d.HEAD[frame]]
    for spec in specs:
        print('frame %d at %s' % (frame, spec['at']))
        rows = check(frame, spec['at'], spec['rows'])
        print('    ' + ''.join(str(x % 10) for x in range(max(len(r) for r in rows))))
        for y, r in enumerate(rows):
            print('%3d %s' % (y, r))


if __name__ == '__main__' and sys.argv[1] == 'check':
    check_design(int(sys.argv[2]), int(sys.argv[3]))
elif __name__ == '__main__':
    f, ax, ay, w, h = (int(v) for v in sys.argv[1:6])
    print('frame %d at (%d, %d), %dx%d   (s skin, k jaw outline, "," base hair; a hair grid covers k and ",")' % (f, ax, ay, w, h))
    print('    ' + ''.join(str(x % 10) for x in range(w)))
    for y, r in enumerate(template(f, ax, ay, w, h)):
        print('%3d %s' % (y, r))
