#!/usr/bin/env python3
"""The avatar rig: extra head and torso drawings, one per pose, and the
animation table that picks a pose for every frame of the four motions a hero
is seen in most.

Why this exists. The hero's animation (`/c/sp/cif/00N.cif`) draws the head
and the torso as ONE image each for a whole motion — the idle bob and the
walk move them by a pixel, nothing more. A costume that only replaces those
images (v2) therefore cannot move: a long coat is a board over the legs and
hair is a helmet. The game advances exactly one cif frame per tick
(`<CLASS>::EndProc` -> `CHAR::IncreaseSpriteFrame`, end test `IsEndFrame`),
so adding frames would change motion lengths, hit timing and stride. The rig
keeps every count and every offset and changes only WHICH image a frame
names: the frame where the head rises names a head whose hair lags, the walk
frames name coats whose hem swings with the stride.

Layout — shared by every rigged costume, so one cif variant per class serves
all of them:

    head sheet (img0)   0..6 the game's indices (0 front, 1 3/4, 3 back,
                        4 side, 6 3/4 down; 2 = the hair-back slot the side
                        view draws behind the head; 5 unused)
                        7 + view*9 + (hx+1)*3 + (hy+1)   hair poses
                        34 + (hx+1)                      hair-back poses
    body sheet (img1)   0..11 the game's indices (0 front, 1 3/4, 2 back;
                        3..11 the skirt slots the game's armour uses)
                        12 + view*5 + (s+2)              hem poses

`view` is the cif direction: 0 back (UP), 1 front (DOWN), 2 side (the stored
frames face left; LEFT/RIGHT is the guest's mirror). `hx` is how far the hair
trails sideways (+1 = toward +x of the stored frame), `hy` is the hair's
vertical lag (+1 = tips pushed down: the head just rose; -1 = tips stretched
up: the head just dropped). `s` is the hem's displacement at its lowest row,
in pixels, toward +x.

The table is derived, not typed: a part's movement between consecutive
frames gives the lag (the hair and hem stay where they were for one frame),
and the walk adds the stride's swing. `hl5_core::costume` applies the same
remaps to the archive's cif at boot (`build.py` transcribes REMAPS).
"""
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools_re'))
import mgr  # noqa: E402

# The four motions: 0 stand, 1 walk (no weapon — towns), 2 stand, 3 walk
# (weapon drawn — fields). `HERO::SetStandMotion` sets 2, `SetWalkMotion` 3,
# and `HERO::CheckWeaponMotion` drops them to 0/1 for an empty hand.
RIG_ANIMS = (0, 1, 2, 3)
WALKS = (1, 3)

HEAD_FOR_VIEW = {0: 3, 1: 0, 2: 1}
TORSO_FOR_VIEW = {0: 2, 1: 0, 2: 1}
HAIR_BACK = 2

HEAD_BASE_COUNT = 7
BODY_BASE_COUNT = 12
HEAD_RIG = HEAD_BASE_COUNT
BACK_RIG = HEAD_RIG + 3 * 9
HEAD_COUNT = BACK_RIG + 3
BODY_RIG = BODY_BASE_COUNT
BODY_COUNT = BODY_RIG + 3 * 5
SWAYS = (-2, -1, 0, 1, 2)


def head_slot(view, hx, hy):
    return HEAD_RIG + view * 9 + (hx + 1) * 3 + (hy + 1)


def back_slot(hx):
    return BACK_RIG + hx + 1


def body_slot(view, s):
    return BODY_RIG + view * 5 + (s + 2)


def head_poses():
    """(view, hx, hy) for every head rig slot, in index order."""
    return [(v, hx, hy) for v in range(3) for hx in (-1, 0, 1) for hy in (-1, 0, 1)]


def body_poses():
    return [(v, s) for v in range(3) for s in SWAYS]


# ---------------------------------------------------------------- the table
def clamp(v, lo, hi):
    return max(lo, min(hi, v))


def _part(parts, layer, indices):
    for p in parts:
        if p.layer == layer and p.index in indices:
            return p
    return None


# The stride swing, by walk frame. Front and back: the hem swings toward the
# side whose leg is passing (the cif's legs are 1,2,3 then 1,2,3 mirrored), a
# tick behind the legs. Side: the coat streams behind (+x of the stored,
# left-facing frame) and flaps on each contact frame.
STRIDE = {0: (-1, 1, 1, 1, -1, -1), 1: (1, -1, -1, -1, 1, 1), 2: (1, 2, 2, 1, 2, 2)}
# The hair's own swing: side view trails behind while walking.
WALK_TRAIL = {0: 0, 1: 0, 2: 1}


def frame_poses(cif):
    """{(anim, dir, frame): (s, hx, hy)} from one class's cif (the warrior's
    is the reference: every class shares the motions, see `table`)."""
    out = {}
    for a in RIG_ANIMS:
        for d, frames in enumerate(cif.anims[a][:3]):
            n = len(frames)
            for f in range(n):
                cur, prev = frames[f], frames[(f - 1) % n]
                h0 = _part(cur, 0, (HEAD_FOR_VIEW[d],))
                h1 = _part(prev, 0, (HEAD_FOR_VIEW[d],))
                t0 = _part(cur, 1, (TORSO_FOR_VIEW[d],))
                t1 = _part(prev, 1, (TORSO_FOR_VIEW[d],))
                hdx = (h0.x - h1.x) if h0 and h1 else 0
                hdy = (h0.y - h1.y) if h0 and h1 else 0
                tdx = (t0.x - t1.x) if t0 and t1 else 0
                # Lag: what moved this frame leaves its loose ends behind.
                hx = clamp(-hdx, -1, 1)
                hy = clamp(-hdy, -1, 1) if hdy else 0
                # hy: the head rose (dy < 0) -> tips pushed down (+1); the
                # head dropped (dy > 0) -> tips stretched up (-1).
                hy = 1 if hdy < 0 else -1 if hdy > 0 else 0
                s = clamp(-tdx, -2, 2)
                if a in WALKS:
                    s = clamp(STRIDE[d][f % 6], -2, 2)
                    if WALK_TRAIL[d]:
                        hx = WALK_TRAIL[d]
                out[(a, d, f)] = (s, hx, hy)
    return out


def table():
    """The poses, from the warrior's cif."""
    return frame_poses(warrior())


def warrior():
    return mgr.cif_decode(mgr.load_source('c/sp/cif/001.cif')[1], True)


def remaps():
    """[(anim, dir, frame, layer, from_index, to_index)] — what the cif
    variant changes. Every class's cif names the same images in these frames
    (checked by `check_classes`), so one list serves the four. The hair-back
    slot is remapped only where the motion draws it (the side view of 0, 1
    and 3 — the weapon stance has no hair-back part)."""
    cif = warrior()
    poses = frame_poses(cif)
    out = []
    for (a, d, f), (s, hx, hy) in sorted(poses.items()):
        parts = cif.anims[a][d][f]
        out.append((a, d, f, 0, HEAD_FOR_VIEW[d], head_slot(d, hx, hy)))
        if any(p.layer == 0 and p.index == HAIR_BACK for p in parts):
            out.append((a, d, f, 0, HAIR_BACK, back_slot(hx)))
        out.append((a, d, f, 1, TORSO_FOR_VIEW[d], body_slot(d, s)))
    return out


def apply(raw, rig_head=True, rig_body=True):
    """The decoded variant, for previews."""
    return mgr.cif_decode(patch_cif(raw, rig_head, rig_body), True)


def check_classes():
    """Every class's cif has the motions the rig remaps, with the same image
    in the same frame — otherwise a remap would name nothing."""
    rm = remaps()
    for c in range(4):
        cif = mgr.cif_decode(mgr.load_source('c/sp/cif/%03d.cif' % (c + 1))[1], True)
        for a, d, f, layer, src, dst in rm:
            parts = cif.anims[a][d][f]
            if src == HAIR_BACK and layer == 0:
                continue  # drawn by some classes' frames only; absent = skipped
            assert any(p.layer == layer and p.index == src for p in parts), (c, a, d, f, layer, src)
    return True


def part_offsets(raw):
    """{(anim, dir, frame): [offset of each part's first byte]} for a hero
    cif (12-bit coordinates), walking it exactly as
    `SPRITE_DATA::LoadSprData` (0xDBC64) does: a part is 5 bytes, 10 when
    `b0 & 0x80` carries the effect tail; an empty slot (`b0 == 0x7F`) is 5
    bytes and counts toward the frame's part number."""
    def s8(v):
        return v - 256 if v > 127 else v
    nl = s8(raw[0])
    r = 1 + max(nl, 0)
    na = raw[r]
    r += 1
    out = {}
    for a in range(na):
        nd = s8(raw[r])
        r += 1
        for d in range(max(nd, 0)):
            nf = raw[r]
            r += 1
            for f in range(nf):
                np_ = s8(raw[r])
                r += 1
                offs = []
                for _ in range(max(np_, 0)):
                    offs.append(r)
                    r += 10 if raw[r] & 0x80 else 5
                out[(a, d, f)] = offs
    if r != len(raw):
        raise ValueError('cif walk ended at %d of %d' % (r, len(raw)))
    return out


def patch_cif(raw, rig_head=True, rig_body=True, rm=None):
    """The cif variant: the archive's bytes with the remapped image indices
    rewritten in place. Nothing else moves — counts, offsets, effects and
    every other layer are the original bytes."""
    rm = rm if rm is not None else remaps()
    out = bytearray(raw)
    offs = part_offsets(raw)
    for a, d, f, layer, src, dst in rm:
        if (layer == 0 and not rig_head) or (layer == 1 and not rig_body):
            continue
        for o in offs.get((a, d, f), ()):
            b0 = out[o]
            if b0 != 0x7F and b0 & 0x1F == layer and out[o + 1] == src:
                out[o + 1] = dst
                break
    return bytes(out)


def roundtrip():
    """The walk covers the four hero files exactly, and a variant decodes to
    the same structure with only the remapped indices changed."""
    for c in range(4):
        raw = mgr.load_source('c/sp/cif/%03d.cif' % (c + 1))[1]
        part_offsets(raw)
        a = mgr.cif_decode(raw, True)
        b = mgr.cif_decode(patch_cif(raw), True)
        assert len(a.anims) == len(b.anims)
        for x, y in zip(a.anims, b.anims):
            assert [len(f) for f in x] == [len(f) for f in y]
    return True


TABLE = ROOT / 'costumes' / 'rig_table.txt'


def write_table(path=TABLE):
    """What build.py transcribes for hl5_core: the sheet sizes and the remaps."""
    lines = ["# @generated by costumes/rig.py from the archive's warrior cif - do not edit.",
             '# (anim dir frame layer from to) per line; see the module docstring.',
             'head %d' % HEAD_COUNT, 'body %d' % BODY_COUNT]
    lines += ['%d %d %d %d %d %d' % r for r in remaps()]
    Path(path).write_text('\n'.join(lines) + '\n', encoding='utf-8')


if __name__ == '__main__':
    print('roundtrip', roundtrip(), 'classes', check_classes())
    t = table()
    for k in sorted(t):
        print(k, t[k])
    print(len(remaps()), 'remaps')
