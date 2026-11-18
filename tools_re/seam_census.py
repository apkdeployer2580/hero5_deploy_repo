#!/usr/bin/env python3
"""Measure the seam between a 32-bit guest binary and the world around it.

This is the first command to run on a rehosting candidate, and its output is
the go/no-go decision. A rehost re-implements everything the binary touches
*outside itself*; nothing else. So the only number that decides whether the
project is a weekend or a year is **how many distinct things that is**.

    python seam_census.py libgame.so
    python seam_census.py game.apk            # picks the 32-bit lib(s) out
    python seam_census.py libgame.so --json   # machine-readable

Reports, in order:

  identity     ELF class/machine, architecture tags, compiler, stripped or not
  size         .text bytes — the amount of code you are NOT rewriting
  symbols      whether function names survived (the single biggest cost factor)
  imports      undefined dynamic symbols, grouped by the library that owns them
  jni          exported Java_* entry points, i.e. what the Java side called
  relocations  histogram by type — exotic types mean a harder loader
  hazards      TLS, init_array, C++ ABI, Thumb/VFP/NEON, unusual sections
  verdict      what those numbers mean for the effort

Needs `pip install pyelftools`.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import zipfile
from collections import Counter
from pathlib import Path

# A Korean/Japanese Windows console defaults to a legacy code page and dies on
# the first em dash. Every script here prints UTF-8 regardless.
for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")

try:
    from elftools.elf.elffile import ELFFile
    from elftools.elf.sections import SymbolTableSection
    from elftools.elf.relocation import RelocationSection
except ImportError:  # pragma: no cover
    sys.exit("pip install pyelftools")

try:
    from elftools.elf.sections import ARMAttributesSection
except ImportError:  # pragma: no cover
    ARMAttributesSection = ()


# Relocation type names we care about, per architecture. Anything not in here
# prints as its raw number, which is itself the signal: an exotic relocation is
# loader work you have not planned for.
R_ARM = {
    2: "R_ARM_ABS32",
    3: "R_ARM_REL32",
    17: "R_ARM_TLS_DTPMOD32",
    18: "R_ARM_TLS_DTPOFF32",
    19: "R_ARM_TLS_TPOFF32",
    20: "R_ARM_COPY",
    21: "R_ARM_GLOB_DAT",
    22: "R_ARM_JUMP_SLOT",
    23: "R_ARM_RELATIVE",
    24: "R_ARM_GOTOFF32",
    25: "R_ARM_BASE_PREL",
    26: "R_ARM_GOT_BREL",
    28: "R_ARM_CALL",
    29: "R_ARM_JUMP24",
    38: "R_ARM_TARGET1",
    102: "R_ARM_THM_JUMP11",
    103: "R_ARM_THM_JUMP8",
}
R_386 = {
    1: "R_386_32",
    2: "R_386_PC32",
    5: "R_386_COPY",
    6: "R_386_GLOB_DAT",
    7: "R_386_JMP_SLOT",
    8: "R_386_RELATIVE",
}

# Which library an undefined symbol most likely comes from. Order matters: the
# first pattern that matches wins.
LIB_RULES = [
    ("GLES", re.compile(r"^(gl[A-Z]|egl[A-Z])")),
    ("libm", re.compile(r"^(sin|cos|tan|asin|acos|atan|atan2|exp|log|log10|pow|sqrt|"
                        r"ceil|floor|fmod|fabs|ldexp|frexp|modf|round|trunc|hypot)f?$")),
    ("liblog", re.compile(r"^__android_log")),
    ("libdl", re.compile(r"^(dlopen|dlsym|dlclose|dlerror|dladdr)$")),
    ("libstdc++", re.compile(r"^(_Z|__cxa_|__gxx_|_Unwind_|__gnu_)")),
    ("libjnigraphics", re.compile(r"^AndroidBitmap_")),
    ("libandroid", re.compile(r"^(AAsset|ANative|AInput|ALooper|AConfiguration)")),
    ("libOpenSLES", re.compile(r"^(sl|SL_)")),
    ("libz", re.compile(r"^(deflate|inflate|compress|uncompress|crc32|adler32|gz)")),
]


def classify(name: str) -> str:
    for lib, pat in LIB_RULES:
        if pat.match(name):
            return lib
    return "libc"


# Tag_CPU_arch values, from the ARM ABI "Addenda to, and Errata in, the ABI for
# the ARM Architecture". This one number decides which instruction encodings the
# interpreter has to implement, and a wrong guess is silent wrong answers rather
# than a crash: an ARMv4T decoder fed ARMv5TE code will mis-decode the DSP
# multiplies as data processing and quietly produce garbage.
CPU_ARCH = {
    0: "Pre-v4", 1: "ARMv4", 2: "ARMv4T", 3: "ARMv5T", 4: "ARMv5TE",
    5: "ARMv5TEJ", 6: "ARMv6", 7: "ARMv6KZ", 8: "ARMv6T2", 9: "ARMv6K",
    10: "ARMv7", 11: "ARMv6-M", 12: "ARMv6S-M", 13: "ARMv7E-M", 14: "ARMv8-A",
}


def read_arm_attrs(elf) -> dict:
    """Tag_CPU_arch and friends. These decide which instructions the
    interpreter must implement — the difference between ARMv4T and ARMv5TE is
    six encodings, and getting it wrong is silent wrong answers, not a crash."""
    out = {}
    for sec in elf.iter_sections():
        if ARMAttributesSection and isinstance(sec, ARMAttributesSection):
            for sub in sec.iter_subsections():
                for ss in sub.iter_subsubsections():
                    for attr in ss.iter_attributes():
                        out[str(attr.tag)] = attr.value
    if "TAG_CPU_ARCH" in out:
        out["_arch_name"] = CPU_ARCH.get(out["TAG_CPU_ARCH"],
                                         f"unknown ({out['TAG_CPU_ARCH']})")
    if out:
        return out
    # Fallback: the CPU name is a plain NUL-terminated string in the section.
    sec = elf.get_section_by_name(".ARM.attributes")
    if sec:
        blob = sec.data()
        m = re.search(rb"([0-9A-Za-z._-]{2,16})\x00", blob[16:])
        if m:
            out["Tag_CPU_name"] = m.group(1).decode("ascii", "replace")
    return out


def census(path: Path) -> dict:
    data = path.read_bytes()
    with path.open("rb") as fh:
        elf = ELFFile(fh)

        hdr = elf.header
        bits = 32 if elf.elfclass == 32 else 64
        machine = hdr["e_machine"]
        etype = hdr["e_type"]

        # ---- sections and segments -------------------------------------
        sec_sizes = {s.name: s["sh_size"] for s in elf.iter_sections() if s.name}
        text = sec_sizes.get(".text", 0)
        load = sum(seg["p_memsz"] for seg in elf.iter_segments()
                   if seg["p_type"] == "PT_LOAD")
        has_tls = any(seg["p_type"] == "PT_TLS" for seg in elf.iter_segments())
        exidx = any(seg["p_type"] == "PT_ARM_EXIDX" for seg in elf.iter_segments())

        # ---- dynamic ----------------------------------------------------
        needed, soname, flags = [], None, []
        dyn = elf.get_section_by_name(".dynamic")
        if dyn is not None and hasattr(dyn, "iter_tags"):
            for tag in dyn.iter_tags():
                t = tag.entry.d_tag
                if t == "DT_NEEDED":
                    needed.append(tag.needed)
                elif t == "DT_SONAME":
                    soname = tag.soname
                elif t in ("DT_SYMBOLIC", "DT_TEXTREL", "DT_BIND_NOW"):
                    flags.append(t)
                elif t == "DT_FLAGS":
                    flags.append("DT_FLAGS=%#x" % tag.entry.d_val)

        # ---- symbols ----------------------------------------------------
        symtab = elf.get_section_by_name(".symtab")
        dynsym = elf.get_section_by_name(".dynsym")
        stripped = symtab is None

        n_func = n_obj = 0
        if isinstance(symtab, SymbolTableSection):
            for s in symtab.iter_symbols():
                if s["st_shndx"] == "SHN_UNDEF" or not s.name:
                    continue
                t = s["st_info"]["type"]
                n_func += t == "STT_FUNC"
                n_obj += t == "STT_OBJECT"

        imports, exports, jni = [], [], []
        if isinstance(dynsym, SymbolTableSection):
            for s in dynsym.iter_symbols():
                if not s.name:
                    continue
                if s["st_shndx"] == "SHN_UNDEF":
                    imports.append(s.name)
                else:
                    exports.append(s.name)
                    if s.name.startswith("Java_") or s.name in (
                        "JNI_OnLoad", "JNI_OnUnload"
                    ):
                        jni.append(s.name)
        imports = sorted(set(imports))
        jni = sorted(set(jni))

        # ---- relocations -------------------------------------------------
        names = R_ARM if machine == "EM_ARM" else R_386
        relocs = Counter()
        for sec in elf.iter_sections():
            if not isinstance(sec, RelocationSection):
                continue
            for r in sec.iter_relocations():
                kind = r["r_info_type"]
                relocs[names.get(kind, f"type {kind}")] += 1

        # ---- init/fini ----------------------------------------------------
        def count_array(name, entry=4 if bits == 32 else 8):
            return sec_sizes.get(name, 0) // entry

        attrs = read_arm_attrs(elf) if machine == "EM_ARM" else {}

    producer = ""
    m = re.search(rb"GCC:? ?\(?[^\x00]{0,60}", data)
    if m:
        producer = m.group(0).decode("ascii", "replace").strip()
    if not producer:
        m = re.search(rb"clang version [^\x00]{0,40}", data)
        producer = m.group(0).decode("ascii", "replace") if m else "unknown"

    by_lib = Counter(classify(n) for n in imports)

    return {
        "file": str(path),
        "bytes": len(data),
        "bits": bits,
        "machine": machine,
        "type": etype,
        "soname": soname,
        "needed": needed,
        "dyn_flags": flags,
        "producer": producer,
        "arm_attrs": {k: v for k, v in attrs.items()},
        "text_bytes": text,
        "load_bytes": load,
        "stripped": stripped,
        "symtab_funcs": n_func,
        "symtab_objects": n_obj,
        "dynsym_exports": len(exports),
        "imports": imports,
        "imports_by_lib": dict(by_lib),
        "jni_entry_points": jni,
        "relocations": dict(relocs),
        "init_array": count_array(".init_array"),
        "fini_array": count_array(".fini_array"),
        "has_tls": has_tls,
        "has_exidx": exidx,
        "sections": sec_sizes,
    }


# --------------------------------------------------------------------------
# reporting


def bar(label, value, width=40, total=None):
    if not total:
        return f"  {label:<28} {value}"
    n = int(width * value / total) if total else 0
    return f"  {label:<28} {value:>7}  {'#' * n}"


def report(c: dict) -> int:
    p = print
    p("=" * 72)
    p(f"  {Path(c['file']).name}   {c['bytes']:,} bytes")
    p("=" * 72)

    p("\n-- identity ------------------------------------------------------")
    p(f"  {c['bits']}-bit {c['machine']} {c['type']}"
      + (f", soname {c['soname']}" if c["soname"] else ""))
    p(f"  built by         {c['producer']}")
    a = c["arm_attrs"]
    if a:
        if "_arch_name" in a:
            p(f"  architecture     {a['_arch_name']}"
              + (f" (\"{a['TAG_CPU_NAME']}\")" if "TAG_CPU_NAME" in a else ""))
        p(f"  Thumb            {'yes' if a.get('TAG_THUMB_ISA_USE') else 'no — ARM only'}")
        p(f"  VFP              {'yes' if a.get('TAG_VFP_ARCH') else 'no — soft float'}")
        p(f"  NEON             {'yes' if a.get('TAG_ADVANCED_SIMD_ARCH') else 'no'}")
    p(f"  DT_NEEDED        {', '.join(c['needed']) or '(none)'}")
    if c["dyn_flags"]:
        p(f"  dynamic flags    {', '.join(c['dyn_flags'])}")

    p("\n-- size ----------------------------------------------------------")
    p(f"  .text            {c['text_bytes']:,} bytes of machine code")
    p(f"  PT_LOAD total    {c['load_bytes']:,} bytes")

    p("\n-- symbols -------------------------------------------------------")
    if c["stripped"]:
        p("  .symtab          ABSENT — the binary is stripped")
        p("                   Every guest function will be an address, not a")
        p("                   name. Budget several times the analysis effort.")
    else:
        p(f"  .symtab          present — {c['symtab_funcs']:,} named functions, "
          f"{c['symtab_objects']:,} objects")
        p("                   This is the single biggest cost multiplier in your")
        p("                   favour: profiles, faults and hooks can all speak")
        p("                   the original's own names.")
    p(f"  exports          {c['dynsym_exports']:,} dynamic symbols")

    p("\n-- the seam ------------------------------------------------------")
    total_imports = len(c["imports"])
    p(f"  imported symbols {total_imports}")
    for lib, n in sorted(c["imports_by_lib"].items(), key=lambda kv: -kv[1]):
        p(bar(lib, n, total=total_imports))
    p(f"  JNI entry points {len(c['jni_entry_points'])}")
    for n in c["jni_entry_points"][:40]:
        p(f"      {n}")
    if len(c["jni_entry_points"]) > 40:
        p(f"      ... and {len(c['jni_entry_points']) - 40} more")

    p("\n-- relocations ---------------------------------------------------")
    known_easy = {"R_ARM_RELATIVE", "R_ARM_GLOB_DAT", "R_ARM_JUMP_SLOT",
                  "R_ARM_ABS32", "R_386_RELATIVE", "R_386_GLOB_DAT",
                  "R_386_JMP_SLOT", "R_386_32"}
    exotic = []
    for k, n in sorted(c["relocations"].items(), key=lambda kv: -kv[1]):
        p(bar(k, n))
        if k not in known_easy:
            exotic.append(k)

    p("\n-- hazards -------------------------------------------------------")
    haz = []
    if c["has_tls"]:
        haz.append("PT_TLS — thread-local storage; the loader must model it")
    if c["init_array"]:
        haz.append(f".init_array with {c['init_array']} entries — C++ static "
                   "constructors must run before the entry point")
    if c["fini_array"]:
        haz.append(f".fini_array with {c['fini_array']} entries")
    if exotic:
        haz.append("relocation types outside the easy set: " + ", ".join(exotic))
    if c["arm_attrs"].get("TAG_THUMB_ISA_USE"):
        haz.append("Thumb code present — the interpreter needs both decoders")
    if c["arm_attrs"].get("TAG_VFP_ARCH"):
        haz.append("VFP present — hardware floating point to implement")
    if c["arm_attrs"].get("TAG_ADVANCED_SIMD_ARCH"):
        haz.append("NEON present — SIMD to implement, and it is usually in the "
                   "hot loops")
    if any(n.startswith("_Z") or n.startswith("__cxa_") for n in c["imports"]):
        haz.append("C++ ABI imports — exceptions/RTTI may cross the seam")
    if not haz:
        p("  none of the usual ones")
    for h in haz:
        p(f"  ! {h}")

    p("\n-- verdict -------------------------------------------------------")
    score = []
    if total_imports <= 120:
        score.append(("+", f"{total_imports} imports is a small seam; a hand-written "
                           "HLE layer is a few days of work"))
    elif total_imports <= 400:
        score.append(("~", f"{total_imports} imports is a real but finite seam; "
                           "implement lazily, aborting loudly on the first call "
                           "to anything unimplemented"))
    else:
        score.append(("-", f"{total_imports} imports is a large seam; check whether "
                           "most of it is one library you can shim wholesale"))
    if not c["stripped"]:
        score.append(("+", "symbols survived — profile output, fault reports and "
                           "host hooks can all be keyed by name"))
    else:
        score.append(("-", "stripped — everything downstream costs more"))
    if c["text_bytes"] and c["text_bytes"] < 4 << 20:
        score.append(("+", f"{c['text_bytes']:,} bytes of code is small enough that "
                           "an interpreter can carry it at playable speed once the "
                           "hot functions are hosted"))
    if not c["has_tls"] and not c["init_array"] and not exotic:
        score.append(("+", "textbook loader: no TLS, no static constructors, no "
                           "exotic relocations"))
    if len(c["jni_entry_points"]) <= 40:
        score.append(("+", f"{len(c['jni_entry_points'])} JNI entry points — the "
                           "platform side is small enough to reimplement exactly"))
    for mark, line in score:
        p(f"  {mark} {line}")
    p("")
    p("  Remember what this does NOT measure: everything the binary calls")
    p("  *inside itself* is free — you are not rewriting it. Count the calls")
    p("  that leave the binary, not the ones that stay.")
    p("")
    return 0


def libs_in_apk(apk: Path):
    out = []
    with zipfile.ZipFile(apk) as z:
        for n in z.namelist():
            if n.startswith("lib/") and n.endswith(".so"):
                out.append(n)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("target", help=".so file, or an .apk to look inside")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument("--imports", action="store_true", help="list every import")
    ap.add_argument("--extract", metavar="DIR",
                    help="when given an .apk, write the libs here first")
    a = ap.parse_args()

    target = Path(a.target)
    if not target.exists():
        sys.exit(f"no such file: {target}")

    paths = [target]
    if target.suffix.lower() in (".apk", ".zip", ".jar"):
        entries = libs_in_apk(target)
        if not entries:
            sys.exit("no lib/**/*.so inside that archive")
        outdir = Path(a.extract or ".")
        outdir.mkdir(parents=True, exist_ok=True)
        paths = []
        with zipfile.ZipFile(target) as z:
            for e in entries:
                dst = outdir / Path(e).name
                dst.write_bytes(z.read(e))
                paths.append(dst)
                print(f"extracted {e} -> {dst}")

    results = [census(p) for p in paths]
    if a.json:
        print(json.dumps(results if len(results) > 1 else results[0], indent=2))
        return 0
    for c in results:
        report(c)
        if a.imports:
            print("-- every imported symbol -----------------------------------------")
            for n in c["imports"]:
                print(f"  {classify(n):<12} {n}")
            print("")
    return 0


if __name__ == "__main__":
    sys.exit(main())
