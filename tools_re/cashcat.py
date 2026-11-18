#!/usr/bin/env python3
"""Decode the cash-shop catalogue, `/c/csv/cash_single_android.dat`.

    python cashcat.py                # the table
    python cashcat.py --raw          # + description, counts, all three prices

This answers "which shop row grants what": the row's `id` is the byte the
client puts on the wire as category 6 subcmd 2 — `testserver.py` prints it as
`(approving purchase of item N)`.

The layout is not guessed. `CashShop::LoadCashShopData` (0xde07c) reads the
file field by field into a 24-byte `CashItemInfo`, and this mirrors it:

    u16 count
    per record:
        u16 len                  the rest of this record
        u8  id       -> +0x00    what category 6 carries
        u16 param    -> +0x02    read with ByteToInt16
        u8  namelen ; name       -> +0x04, malloc'd and memcpy'd   EUC-KR
        u8  a        -> +0x08
        u8  count    -> +0x09
        u8  desclen ; desc       -> +0x0c, same treatment          EUC-KR
        u16 price    -> +0x10
        u16 price2   -> +0x12
        u16 price3   -> +0x14

Two fields are cross-checked against the running game rather than inferred:

* `id` — the replay buys 엘릭서(20), row id 4, and the server logs
  `approving purchase of item 4`.
* `price` — 부활의 부적 says 100, and the S25's in-game prompt reads
  "실제 현금 100원".

`count` is the quantity: it is exactly the number in the parenthesised names
(엘릭서(20) -> 20, 부활의 부적(5) -> 5) and 1 for every row without one.

`price2`/`price3` are the discount tiers (traced — see below). `param` and `a`
are read but not yet tied to observed behaviour. Notes, so the next session
does not re-derive them:

* `param` is a u16 and reaches 273, so it is **not** `_NET_ITEM_.kind`, which
  is a u8. It is also shared by unrelated rows (창고 확장 and all five 공허
  weapons are 222), so it is a group/icon, not an item identity.
* `a` is unique per row except for the two 부활의 부적 rows, which are the
  same item in different quantities — that makes it the best candidate for
  the granted item, but nothing observed confirms it yet.
* `price2`/`price3` are the **discount tiers**, and this is traced, not
  guessed. `StateCashShop::DrawAutoCashShopInfo` (0xdc1cc) calls
  `CashShop::GetDiscount` twice and picks the field from its answer:

      GetDiscount == 1  ->  ldrsh [r7, #0x12]    (0xdc5c4)   price2
      GetDiscount == 2  ->  ldrsh [r7, #0x14]    (0xdc5e0)   price3
      otherwise         ->  ldrsh [r7, #0x10]    (0xdc3d8)   price

  All three converge on one `MC_knlSprintk` + `BFont::DrawText` at 0xdc464.
  `GetDiscount` itself reads `gv[0x740 + n*4 + 4]`, a global the server
  fills in, so which tier shows is a server decision — but the numbers it
  chooses between are these.
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vfs  # noqa: E402

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")

PATH = "c/csv/cash_single_android.dat"


def load(vfs_path: Path) -> bytes:
    blob = vfs_path.read_bytes()
    want = vfs.djb2(PATH)
    for h, off, size in vfs.entries(blob):
        if h == want:
            return blob[off : off + size]
    raise SystemExit(f"{PATH} is not in {vfs_path}")


def records(d: bytes):
    (count,) = struct.unpack_from("<H", d, 0)
    pos = 2
    for _ in range(count):
        (rlen,) = struct.unpack_from("<H", d, pos)
        pos += 2
        body, pos = d[pos : pos + rlen], pos + rlen
        p = 0
        item_id = body[p]
        param = struct.unpack_from("<H", body, p + 1)[0]
        p += 3
        nlen = body[p]
        p += 1
        name = body[p : p + nlen].decode("euc-kr", "replace")
        p += nlen
        a, qty = body[p], body[p + 1]
        p += 2
        dlen = body[p]
        p += 1
        desc = body[p : p + dlen].decode("euc-kr", "replace")
        p += dlen
        prices = list(struct.unpack_from("<3H", body, p))
        yield {
            "id": item_id,
            "param": param,
            "name": name,
            "a": a,
            "count": qty,
            "desc": desc,
            "prices": prices,
        }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--raw", action="store_true")
    ap.add_argument(
        "--vfs",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "payload" / "data.vfs.mp3",
    )
    args = ap.parse_args()

    d = load(args.vfs)
    print(f"{PATH}  {len(d)} bytes")
    print(f"{'id':>3} {'param':>6} {'a':>4} {'cnt':>4} {'price':>6} {'p2':>5} {'p3':>5}  name")
    print("-" * 78)
    n = 0
    for r in records(d):
        n += 1
        p1, p2, p3 = r["prices"]
        print(
            f"{r['id']:>3} {r['param']:>6} {r['a']:>4} {r['count']:>4} "
            f"{p1:>6} {p2:>5} {p3:>5}  {r['name']}"
        )
        if args.raw:
            print(f"        {r['desc'].replace(';', ' / ')}")
    print(f"-- {n} rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
