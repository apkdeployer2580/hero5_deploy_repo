#!/usr/bin/env python3
"""Read `assets/data.vfs.mp3` — the game's asset archive, despite the extension.

    python vfs.py list                       # every entry, by hash
    python vfs.py get /c/csv/cash_single_android.dat out.dat
    python vfs.py cat /c/csv/cash_single_android.dat  --enc euc-kr

The container is a flat sequential list, no encryption (ANALYSIS.md):

    [u32 hash][u32 size][size bytes]   repeated, little-endian

`hash` is `MIDASKernelManager::hash` (0x1516a4) = djb2 (`h = h*33 + c`, seed
5381, 32-bit wrap) over the path **with the leading `/` removed** —
`tempLocalGetExactFilename` (0x387d0) skips a leading `/` or `\\`, so
`/c/font/table.dat` hashes as `c/font/table.dat`.

The host never parses this at runtime; the guest finds its own bytes. This is
purely an analysis aid.
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")

DEFAULT_VFS = Path(__file__).resolve().parent.parent / "payload" / "data.vfs.mp3"


def djb2(name: str) -> int:
    """`MIDASKernelManager::hash`. The key is the path without its leading /."""
    h = 5381
    for b in name.lstrip("/\\").encode("ascii", "replace"):
        h = (h * 33 + b) & 0xFFFFFFFF
    return h


def entries(blob: bytes):
    """Walk the archive, yielding (hash, offset_of_payload, size)."""
    pos, n = 0, len(blob)
    while pos + 8 <= n:
        h, size = struct.unpack_from("<II", blob, pos)
        pos += 8
        if size > n - pos:
            raise SystemExit(f"entry at {pos - 8} claims {size} bytes, {n - pos} left")
        yield h, pos, size
        pos += size


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["list", "get", "cat", "hash"])
    ap.add_argument("name", nargs="?")
    ap.add_argument("out", nargs="?")
    ap.add_argument("--vfs", type=Path, default=DEFAULT_VFS)
    ap.add_argument("--enc", default="euc-kr", help="text decode for `cat`")
    a = ap.parse_args()

    if a.cmd == "hash":
        print(f"{djb2(a.name):#010x}  {a.name}")
        return 0

    blob = a.vfs.read_bytes()

    if a.cmd == "list":
        count = 0
        for h, off, size in entries(blob):
            print(f"{h:#010x}  off={off:9d}  size={size:8d}")
            count += 1
        print(f"-- {count} entries, {len(blob)} bytes", file=sys.stderr)
        return 0

    want = djb2(a.name)
    for h, off, size in entries(blob):
        if h != want:
            continue
        data = blob[off : off + size]
        if a.cmd == "get":
            dest = Path(a.out or Path(a.name).name)
            dest.write_bytes(data)
            print(f"{a.name} -> {dest} ({size} bytes)")
        else:
            sys.stdout.write(data.decode(a.enc, "replace"))
        return 0

    print(f"{a.name} (hash {want:#010x}) is not in {a.vfs}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
