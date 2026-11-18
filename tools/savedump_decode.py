#!/usr/bin/env python3
"""Reassemble a save that a `-DumpSave` build printed to logcat.

    adb -s <serial> logcat -d -s hl5 > out/save.log
    python tools/savedump_decode.py out/save.log out/shore-storage

The second argument is a probe `--storage` directory. The files land where the
guest itself will look for them, which is *not* the top of that directory:
`MIDASKernelManager::getFullFilePath` prefixes the internal storage root, so
the save lives at

    <storage>/data/data/co.kr.eamobile.HeroesLore5/files/{kickass,LOCAL.sav}

Putting them at `<storage>/kickass` instead produces a probe run that boots
happily to the slot screen with no character in it, which reads as "the save
did not survive" rather than "the save is in the wrong place". That is the same
trap `hl5_core::rates` documents for the data-file shadow.

Why logcat and not a file pull: the release APK is not `debuggable`, so
`adb shell run-as` is refused, and `/sdcard/Android/data/<pkg>/` is gated on
recent Android and Samsung builds in a way that is indistinguishable from the
file being absent. See `hl5_core/src/savedump.rs`.

Chunk lines look like:

    savedump: kickass 1180 B in 3 chunks
    savedump: kickass 0/3 <base64>
    ...
    savedump: end kickass

The header's chunk count is checked against what actually arrived, because a
logcat ring buffer that dropped the middle of a save is the failure this tool
exists to catch. Use `logcat -d` on a fresh boot; if chunks are missing, raise
the buffer (`adb logcat -G 16M`) and boot again.
"""

from __future__ import annotations

import base64
import re
import sys
from pathlib import Path

GUEST_FILES_DIR = "data/data/co.kr.eamobile.HeroesLore5/files"

HEADER = re.compile(r"savedump: (\S+) (\d+) B in (\d+) chunks")
CHUNK = re.compile(r"savedump: (\S+) (\d+)/(\d+) ([A-Za-z0-9+/=]+)")


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(__doc__)
        return 2
    log = Path(argv[1]).read_text(encoding="utf-8", errors="replace")
    out_root = Path(argv[2]) / GUEST_FILES_DIR

    # Later boots win: a capture usually spans several launches and the most
    # recent one is the one the reporter meant.
    headers: dict[str, tuple[int, int]] = {}
    chunks: dict[str, dict[int, str]] = {}
    for line in log.splitlines():
        m = HEADER.search(line)
        if m:
            name, size, total = m.group(1), int(m.group(2)), int(m.group(3))
            headers[name] = (size, total)
            chunks[name] = {}
            continue
        m = CHUNK.search(line)
        if m:
            name, idx = m.group(1), int(m.group(2))
            chunks.setdefault(name, {})[idx] = m.group(4)

    if not headers:
        print("no `savedump:` header lines in that capture.")
        print("  * was the build made with `build_android.ps1 -DumpSave on`?")
        print("  * `adb logcat -s hl5` filters to this app's tag; -d dumps and exits.")
        return 1

    rc = 0
    out_root.mkdir(parents=True, exist_ok=True)
    for name, (size, total) in headers.items():
        got = chunks.get(name, {})
        missing = [i for i in range(total) if i not in got]
        if missing:
            print(f"{name}: MISSING chunks {missing} of {total} — not written.")
            print("  raise the log buffer (`adb logcat -G 16M`) and boot again.")
            rc = 1
            continue
        blob = "".join(got[i] for i in range(total))
        try:
            raw = base64.b64decode(blob, validate=True)
        except Exception as e:  # noqa: BLE001 - report, do not write a partial save
            print(f"{name}: base64 did not decode ({e}) — not written.")
            rc = 1
            continue
        if len(raw) != size:
            print(f"{name}: decoded {len(raw)} B but the header said {size} B — not written.")
            rc = 1
            continue
        dst = out_root / name
        dst.write_bytes(raw)
        print(f"{name}: {size} B -> {dst}")

    if rc == 0:
        print()
        print("replay it with:")
        print(f"  cargo run -q -p hl5_probe -- --boot --storage {argv[2]} \\")
        print("      --boot-mode skip-network --frames 300 --dump out/shore.png")
    return rc


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
