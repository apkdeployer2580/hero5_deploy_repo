"""Generate UI-only replays that dress all eleven avatars in the guest, one
replay per class, each avatar in full: head, body, arms, legs, the class's
own weapon (and the knight's shield).

    python costumes/astra_replay.py [out_dir]     # default docs/astra_avatar/native

Every replay starts from tools_re/scripts/avatar.txt (a fresh save, so no
stat point is pending and the *아바타 slot is free) and differs only in the
class picked on the class screen. Wardrobe rows are [기본 외형, our costumes
in catalogue order, the game's own looks]: the five hand-drawn hats come
before the astra heads, every other tab starts with astra 01.
"""
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[1]
# The class screen opens on 워리어's detail popup: any other class closes it
# first, then picks its tile (the next `key select` reopens the popup on it).
CLASSES = {
    'warrior': ('tap 140 68           # class: 워리어', False),
    'rogue': ('key back\nframes 6\ntap 255 78           # class: 로그', False),
    'gunner': ('key back\nframes 6\ntap 148 135          # class: 건슬링어', False),
    'knight': ('key back\nframes 6\ntap 255 135          # class: 나이트', True),
}
TABS = (129, 157, 185, 213, 241, 269)
start = (root / 'tools_re/scripts/avatar.txt').read_text(encoding='utf-8').split('dump out/shot/avatar-char.png')[0]


def replay(cls, shots):
    pick, shield = CLASSES[cls]
    head = start.replace('tap 140 68           # class: 워리어', pick)
    assert pick.split('\n')[-1] in head, cls
    lines = [head, 'tap 140 190', 'frames 10']
    tabs = TABS if shield else TABS[:5]
    for n in range(1, 12):
        for k, x in enumerate(tabs):
            lines.append(f'tap {x} 94')
            lines.extend(['key down'] * (6 if n == 1 and k == 0 else 1))
            lines.extend(['key select', 'frames 3'])
        lines.extend([f'dump {shots}/{cls}_{n:02d}_wardrobe.png', 'tap 140 190', 'frames 4',
                      f'dump {shots}/{cls}_{n:02d}_character.png'])
        lines.extend(['key back', 'key back', 'frames 12', f'dump {shots}/{cls}_{n:02d}_field.png',
                      'key back', 'frames 4', 'key select', 'frames 8'])
        if n < 11:
            lines.extend(['tap 140 190', 'frames 4'])
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else root / 'docs/astra_avatar/native'
    shots = (out / 'screenshots').as_posix() if out.is_absolute() is False else (out / 'screenshots').relative_to(root).as_posix()
    (out / 'screenshots').mkdir(parents=True, exist_ok=True)
    for cls in CLASSES:
        p = out / f'replay_{cls}.txt'
        p.write_text(replay(cls, shots), encoding='utf-8')
        print(p)
