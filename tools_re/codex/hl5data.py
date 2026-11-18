"""Shared decoder for the HL5 csv tables, straight out of the VFS."""
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VFS = ROOT / "payload" / "data.vfs.mp3"
SRC = ROOT / "hl5_core" / "src"

def rust_ints(file, name):
    """The integers in one `const NAME: T = <value>;` of the shipping Rust.

    The published odds are decided by constants that live in `hl5_core`, not
    here. Reading them back instead of copying them is the whole point: a row
    list or a roll count that moves in the Rust would otherwise leave this page
    quietly printing the probabilities of a build nobody runs any more. Dying
    with the constant's name is the wanted failure — a stale number is not."""
    src = (SRC / file).read_text(encoding="utf-8")
    pat = rf"^[ \t]*(?:pub(?:\([^)]*\))?\s+)?const\s+{name}\s*:[^=]*=\s*([^;]+);"
    hits = re.findall(pat, src, re.M)
    if len(hits) != 1:
        raise SystemExit(f"{file}: expected one `const {name}`, found {len(hits)}")
    ints = [int(x) for x in re.findall(r"-?\d+", hits[0])]
    if not ints:
        raise SystemExit(f"{file}: `const {name}` = {hits[0]!r} holds no integer")
    return ints

def rust_range(file, name):
    """A Rust `lo..=hi` constant as a Python range."""
    lo, hi = rust_ints(file, name)
    return range(lo, hi + 1)

def djb2(name):
    h = 5381
    for b in name.lstrip("/\\").encode("ascii", "replace"):
        h = (h * 33 + b) & 0xFFFFFFFF
    return h

_blob = VFS.read_bytes()
def entry(path):
    want = djb2(path)
    pos, n = 0, len(_blob)
    while pos + 8 <= n:
        h, size = struct.unpack_from("<II", _blob, pos)
        pos += 8
        if h == want:
            return _blob[pos:pos+size]
        pos += size
    return None

def dec(b):
    return b.decode("euc-kr", "replace")

EQUIP = 21
def items(kind):
    d = entry(f"c/csv/item_{kind:02d}.dat")
    if d is None:
        return []
    count = struct.unpack_from("<H", d, 0)[0]
    pos, rows = 2, []
    for i in range(count):
        rl = struct.unpack_from("<H", d, pos)[0]; pos += 2
        body = d[pos:pos+rl]; pos += rl
        nlen = body[2]
        name = body[3:3+nlen]
        rec = {"index": i, "name": dec(name), "raw": body}
        if len(body) >= EQUIP:
            eq = body[len(body)-EQUIP:]
            rec["equip"] = eq
            rec["sprite"] = struct.unpack_from("<H", eq, 0)[0]
            rec["pal"]    = struct.unpack_from("<H", eq, 2)[0]
            rec["group"]  = eq[5]
            rec["statA"]  = struct.unpack_from("<H", eq, 6)[0]
            rec["statB"]  = struct.unpack_from("<H", eq, 8)[0]
            rec["grade"]  = eq[0x0c]
            rec["level"]  = eq[0x0d]
        rows.append(rec)
    return rows

GRADE = {0:"노멀",1:"레어",2:"에픽",3:"영웅",4:"전설"}
def grade_name(g):
    if g is None: return "?"
    return GRADE.get(g, f"세트({g-5})" if g >= 5 else f"?{g}")

KINDNAME = {0:"대검/워리어",1:"단검/로그",2:"권총/건슬링어",3:"장창/나이트",
            4:"스태프",5:"투구",6:"갑옷",7:"장갑",8:"장화",9:"방패",10:"장신구",
            11:"전투소모품",12:"오브",13:"재료",14:"소모품",18:"캐시"}

def droptable():
    d = entry("c/csv/droptable.dat")
    count = struct.unpack_from("<H", d, 0)[0]
    recs = []
    for i in range(count):
        at = 2 + i*13
        rl = struct.unpack_from("<H", d, at)[0]
        body = d[at+2:at+13]
        recs.append({"rec": i, "reclen": rl, "group": body[0], "kind": body[7],
                     "index": body[8], "body": body})
    return recs

# `enemy_<N>.dat` record offsets, all measured from the byte after the name.
# The ladder and the two the host itself reads come from the Rust; the other
# two are their neighbours, because the record mirrors `Monster+0x272..+0x275`
# in order (base index, drop group, variance, roll count).
LADDER_AFTER_NAME = rust_ints("rates.rs", "LADDER_AFTER_NAME")[0]     # 39
GROUP_AFTER_NAME = rust_ints("bossdrop.rs", "GROUP_AFTER_NAME")[0]    # 70
ROLLS_AFTER_NAME = rust_ints("bossdrop.rs", "ROLLS_AFTER_NAME")[0]    # 72
BASE_AFTER_NAME = GROUP_AFTER_NAME - 1
VARIANCE_AFTER_NAME = GROUP_AFTER_NAME + 1

def enemies(diff):
    d = entry(f"c/csv/enemy_{diff}.dat")
    count = struct.unpack_from("<H", d, 0)[0]
    pos, rows = 2, []
    for i in range(count):
        rl = struct.unpack_from("<H", d, pos)[0]; pos += 2
        body = d[pos:pos+rl]; pos += rl
        nlen = body[0]
        name = body[1:1+nlen]
        a = 1 + nlen
        lad = a + LADDER_AFTER_NAME
        t = list(struct.unpack_from("<7I", body, lad)) if lad+28 <= len(body) else [0]*7
        g = lambda off: body[a+off] if a+off < len(body) else None
        rows.append({"row": i, "name": dec(name), "raw_name": name, "T": t,
                     "base_index": g(BASE_AFTER_NAME), "group": g(GROUP_AFTER_NAME),
                     "variance": g(VARIANCE_AFTER_NAME), "rolls": g(ROLLS_AFTER_NAME),
                     "body": body, "after": a, "reclen": rl})
    return rows
