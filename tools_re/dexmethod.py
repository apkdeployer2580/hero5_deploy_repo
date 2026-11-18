#!/usr/bin/env python3
"""Dalvik disassembler for one class' methods. No dependencies.

    python dexmethod.py payload/classes.dex CletEntry onKeyHandleEvent
    python dexmethod.py payload/classes.dex CletEntry            # every method

`dexconst.py` tells you *which* constants a method mentions; this tells you
what it does with them. That difference matters at the seam: the boot order,
the event codes, and the argument order of every `native*` call live in these
method bodies and nowhere else -- the `.so` only sees them after the JVM has
already applied them.

Output is one line per instruction: code-unit offset, opcode name, then the
operands with string/type/field/method indices already resolved, so a call
reads as `invoke-static CletEntry.nativeKeyHandleEvent(III)V` rather than as
a number. Unknown opcodes print their raw code unit instead of guessing.
"""

from __future__ import annotations

import struct
import sys


# ---- container ------------------------------------------------------------


class Dex:
    def __init__(self, path: str):
        self.d = open(path, "rb").read()
        (self.map_off, self.n_str, self.off_str, self.n_type, self.off_type,
         self.n_proto, self.off_proto, self.n_field, self.off_field,
         self.n_meth, self.off_meth, self.n_class, self.off_class,
         _dsz, _doff) = struct.unpack_from("<15I", self.d, 52)
        self.str_off = struct.unpack_from("<%dI" % self.n_str, self.d, self.off_str)
        self.type_ids = struct.unpack_from("<%dI" % self.n_type, self.d, self.off_type)

    def uleb(self, p: int):
        n = shift = 0
        while True:
            b = self.d[p]
            p += 1
            n |= (b & 0x7F) << shift
            shift += 7
            if not b & 0x80:
                return n, p

    def s(self, i: int) -> str:
        _, p = self.uleb(self.str_off[i])
        return self.d[p:self.d.index(b"\0", p)].decode("utf-8", "replace")

    def t(self, i: int) -> str:
        return self.s(self.type_ids[i])

    def proto(self, i: int) -> str:
        _sh, ret, params = struct.unpack_from("<3I", self.d, self.off_proto + i * 12)
        args = []
        if params:
            n = struct.unpack_from("<I", self.d, params)[0]
            args = [self.t(x) for x in struct.unpack_from("<%dH" % n, self.d, params + 4)]
        return "(%s)%s" % ("".join(args), self.t(ret))

    def meth(self, i: int) -> str:
        c, p, n = struct.unpack_from("<HHI", self.d, self.off_meth + i * 8)
        return "%s.%s%s" % (short(self.t(c)), self.s(n), self.proto(p))

    def field(self, i: int) -> str:
        c, t_, n = struct.unpack_from("<3H", self.d, self.off_field + i * 8)
        return "%s.%s:%s" % (short(self.t(c)), self.s(n), self.t(t_))


def short(desc: str) -> str:
    return desc.strip("L;").split("/")[-1] if desc.startswith("L") else desc


# ---- opcode table ---------------------------------------------------------
#
# (name, size in 16-bit code units, format). Only the formats this dex uses
# are spelled out; anything else falls back to a raw dump, which is honest.

OPS = {}


def _op(code, name, size, fmt):
    OPS[code] = (name, size, fmt)


_op(0x00, "nop", 1, "10x")
_op(0x01, "move", 1, "12x")
_op(0x02, "move/from16", 2, "22x")
_op(0x03, "move/16", 3, "32x")
_op(0x04, "move-wide", 1, "12x")
_op(0x05, "move-wide/from16", 2, "22x")
_op(0x06, "move-wide/16", 3, "32x")
_op(0x07, "move-object", 1, "12x")
_op(0x08, "move-object/from16", 2, "22x")
_op(0x09, "move-object/16", 3, "32x")
_op(0x0A, "move-result", 1, "11x")
_op(0x0B, "move-result-wide", 1, "11x")
_op(0x0C, "move-result-object", 1, "11x")
_op(0x0D, "move-exception", 1, "11x")
_op(0x0E, "return-void", 1, "10x")
_op(0x0F, "return", 1, "11x")
_op(0x10, "return-wide", 1, "11x")
_op(0x11, "return-object", 1, "11x")
_op(0x12, "const/4", 1, "11n")
_op(0x13, "const/16", 2, "21s")
_op(0x14, "const", 3, "31i")
_op(0x15, "const/high16", 2, "21h")
_op(0x16, "const-wide/16", 2, "21s")
_op(0x17, "const-wide/32", 3, "31i")
_op(0x18, "const-wide", 5, "51l")
_op(0x19, "const-wide/high16", 2, "21h")
_op(0x1A, "const-string", 2, "21c-str")
_op(0x1B, "const-string/jumbo", 3, "31c-str")
_op(0x1C, "const-class", 2, "21c-type")
_op(0x1D, "monitor-enter", 1, "11x")
_op(0x1E, "monitor-exit", 1, "11x")
_op(0x1F, "check-cast", 2, "21c-type")
_op(0x20, "instance-of", 2, "22c-type")
_op(0x21, "array-length", 1, "12x")
_op(0x22, "new-instance", 2, "21c-type")
_op(0x23, "new-array", 2, "22c-type")
_op(0x24, "filled-new-array", 3, "35c-type")
_op(0x25, "filled-new-array/range", 3, "3rc-type")
_op(0x26, "fill-array-data", 3, "31t")
_op(0x27, "throw", 1, "11x")
_op(0x28, "goto", 1, "10t")
_op(0x29, "goto/16", 2, "20t")
_op(0x2A, "goto/32", 3, "30t")
_op(0x2B, "packed-switch", 3, "31t")
_op(0x2C, "sparse-switch", 3, "31t")
for _i, _n in enumerate(["cmpl-float", "cmpg-float", "cmpl-double",
                         "cmpg-double", "cmp-long"]):
    _op(0x2D + _i, _n, 2, "23x")
for _i, _n in enumerate(["if-eq", "if-ne", "if-lt", "if-ge", "if-gt", "if-le"]):
    _op(0x32 + _i, _n, 2, "22t")
for _i, _n in enumerate(["if-eqz", "if-nez", "if-ltz", "if-gez", "if-gtz", "if-lez"]):
    _op(0x38 + _i, _n, 2, "21t")
for _i, _n in enumerate(["aget", "aget-wide", "aget-object", "aget-boolean",
                         "aget-byte", "aget-char", "aget-short",
                         "aput", "aput-wide", "aput-object", "aput-boolean",
                         "aput-byte", "aput-char", "aput-short"]):
    _op(0x44 + _i, _n, 2, "23x")
for _i, _n in enumerate(["iget", "iget-wide", "iget-object", "iget-boolean",
                         "iget-byte", "iget-char", "iget-short",
                         "iput", "iput-wide", "iput-object", "iput-boolean",
                         "iput-byte", "iput-char", "iput-short"]):
    _op(0x52 + _i, _n, 2, "22c-field")
for _i, _n in enumerate(["sget", "sget-wide", "sget-object", "sget-boolean",
                         "sget-byte", "sget-char", "sget-short",
                         "sput", "sput-wide", "sput-object", "sput-boolean",
                         "sput-byte", "sput-char", "sput-short"]):
    _op(0x60 + _i, _n, 2, "21c-field")
for _i, _n in enumerate(["invoke-virtual", "invoke-super", "invoke-direct",
                         "invoke-static", "invoke-interface"]):
    _op(0x6E + _i, _n, 3, "35c-meth")
for _i, _n in enumerate(["invoke-virtual/range", "invoke-super/range",
                         "invoke-direct/range", "invoke-static/range",
                         "invoke-interface/range"]):
    _op(0x74 + _i, _n, 3, "3rc-meth")
for _i, _n in enumerate(["neg-int", "not-int", "neg-long", "not-long", "neg-float",
                         "neg-double", "int-to-long", "int-to-float",
                         "int-to-double", "long-to-int", "long-to-float",
                         "long-to-double", "float-to-int", "float-to-long",
                         "float-to-double", "double-to-int", "double-to-long",
                         "double-to-float", "int-to-byte", "int-to-char",
                         "int-to-short"]):
    _op(0x7B + _i, _n, 1, "12x")

_BIN = ["add-int", "sub-int", "mul-int", "div-int", "rem-int", "and-int", "or-int",
        "xor-int", "shl-int", "shr-int", "ushr-int", "add-long", "sub-long",
        "mul-long", "div-long", "rem-long", "and-long", "or-long", "xor-long",
        "shl-long", "shr-long", "ushr-long", "add-float", "sub-float", "mul-float",
        "div-float", "rem-float", "add-double", "sub-double", "mul-double",
        "div-double", "rem-double"]
for _i, _n in enumerate(_BIN):
    _op(0x90 + _i, _n, 2, "23x")
for _i, _n in enumerate(_BIN):
    _op(0xB0 + _i, _n + "/2addr", 1, "12x")
for _i, _n in enumerate(["add-int/lit16", "rsub-int", "mul-int/lit16", "div-int/lit16",
                         "rem-int/lit16", "and-int/lit16", "or-int/lit16",
                         "xor-int/lit16"]):
    _op(0xD0 + _i, _n, 2, "22s")
for _i, _n in enumerate(["add-int/lit8", "rsub-int/lit8", "mul-int/lit8",
                         "div-int/lit8", "rem-int/lit8", "and-int/lit8",
                         "or-int/lit8", "xor-int/lit8", "shl-int/lit8",
                         "shr-int/lit8", "ushr-int/lit8"]):
    _op(0xD8 + _i, _n, 2, "22b")


def s16(v):
    return v - 0x10000 if v & 0x8000 else v


def s8(v):
    return v - 0x100 if v & 0x80 else v


def payload(dx, base, j, origin):
    """A switch/array payload sitting inline in the code, or None.

    These are data, not instructions, and a naive linear disassembler walks
    straight into them and prints nonsense (that is how `getWIPIKeyCode`'s
    whole key table reads as `move-wide/16`). The first code unit identifies
    them: 0x0100 packed-switch, 0x0200 sparse-switch, 0x0300 fill-array-data.
    Switch targets are relative to the *switch instruction*, not to the
    payload, so `origin` is the offset of the branch that reaches this one.
    Returns (lines, size_in_code_units).
    """
    ident = struct.unpack_from("<H", dx.d, base + j * 2)[0]
    size = struct.unpack_from("<H", dx.d, base + (j + 1) * 2)[0]
    if ident == 0x0100:  # packed-switch: first_key, then `size` targets
        first = struct.unpack_from("<i", dx.d, base + (j + 2) * 2)[0]
        tg = struct.unpack_from("<%di" % size, dx.d, base + (j + 4) * 2)
        lines = ["packed-switch-payload  %d entries, first key %d" % (size, first)]
        lines += ["    %8d -> %#06x" % (first + i, origin + t) for i, t in enumerate(tg)]
        return lines, 4 + size * 2
    if ident == 0x0200:  # sparse-switch: `size` keys, then `size` targets
        ks = struct.unpack_from("<%di" % size, dx.d, base + (j + 2) * 2)
        tg = struct.unpack_from("<%di" % size, dx.d, base + (j + 2 + size * 2) * 2)
        lines = ["sparse-switch-payload  %d entries" % size]
        lines += ["    %8d -> %#06x" % (k, origin + t) for k, t in zip(ks, tg)]
        return lines, 2 + size * 4
    if ident == 0x0300:  # fill-array-data: element width, count, raw bytes
        n = struct.unpack_from("<I", dx.d, base + (j + 2) * 2)[0]
        return ["fill-array-data-payload  %d x %d bytes" % (n, size)], \
            4 + (size * n + 1) // 2
    return None


def decode(dx, base, j, insns):
    """One instruction at code-unit index `j`. Returns (text, size)."""
    def u(k):
        return struct.unpack_from("<H", dx.d, base + (j + k) * 2)[0]

    w = u(0)
    op, aa = w & 0xFF, w >> 8
    if op not in OPS:
        return ".word %#06x" % w, 1
    name, size, fmt = OPS[op]
    if j + size > insns:
        return ".word %#06x" % w, 1
    a, b = aa & 0xF, aa >> 4

    def regs35c():
        n, c = aa >> 4, u(2)
        r = [c & 0xF, (c >> 4) & 0xF, (c >> 8) & 0xF, (c >> 12) & 0xF, aa & 0xF]
        return "{%s}" % ", ".join("v%d" % x for x in r[:n])

    if fmt == "10x":
        text = ""
    elif fmt == "11x":
        text = "v%d" % aa
    elif fmt == "12x":
        text = "v%d, v%d" % (a, b)
    elif fmt == "11n":
        text = "v%d, %d" % (a, b - 16 if b > 7 else b)
    elif fmt == "21s":
        text = "v%d, %d" % (aa, s16(u(1)))
    elif fmt == "21h":
        text = "v%d, %#x" % (aa, s16(u(1)) << 16)
    elif fmt == "31i":
        text = "v%d, %d" % (aa, struct.unpack_from("<i", dx.d, base + (j + 1) * 2)[0])
    elif fmt == "51l":
        text = "v%d, %d" % (aa, struct.unpack_from("<q", dx.d, base + (j + 1) * 2)[0])
    elif fmt == "22x":
        text = "v%d, v%d" % (aa, u(1))
    elif fmt == "32x":
        text = "v%d, v%d" % (u(1), u(2))
    elif fmt == "23x":
        c = u(1)
        text = "v%d, v%d, v%d" % (aa, c & 0xFF, c >> 8)
    elif fmt == "22b":
        c = u(1)
        text = "v%d, v%d, %d" % (aa, c & 0xFF, s8(c >> 8))
    elif fmt == "22s":
        text = "v%d, v%d, %d" % (a, b, s16(u(1)))
    elif fmt == "10t":
        d = aa - 256 if aa > 127 else aa
        text = "-> %#06x" % (j + d)
    elif fmt == "20t":
        text = "-> %#06x" % (j + s16(u(1)))
    elif fmt in ("30t", "31t"):
        d = struct.unpack_from("<i", dx.d, base + (j + 1) * 2)[0]
        text = ("v%d, " % aa if fmt == "31t" else "") + "-> %#06x" % (j + d)
    elif fmt == "21t":
        text = "v%d, -> %#06x" % (aa, j + s16(u(1)))
    elif fmt == "22t":
        text = "v%d, v%d, -> %#06x" % (a, b, j + s16(u(1)))
    elif fmt.endswith("-str"):
        i = u(1) if fmt.startswith("21") else struct.unpack_from(
            "<I", dx.d, base + (j + 1) * 2)[0]
        text = 'v%d, "%s"' % (aa, dx.s(i))
    elif fmt == "21c-type":
        text = "v%d, %s" % (aa, dx.t(u(1)))
    elif fmt == "22c-type":
        text = "v%d, v%d, %s" % (a, b, dx.t(u(1)))
    elif fmt == "21c-field":
        text = "v%d, %s" % (aa, dx.field(u(1)))
    elif fmt == "22c-field":
        text = "v%d, v%d, %s" % (a, b, dx.field(u(1)))
    elif fmt == "35c-meth":
        text = "%s, %s" % (regs35c(), dx.meth(u(1)))
    elif fmt == "35c-type":
        text = "%s, %s" % (regs35c(), dx.t(u(1)))
    elif fmt == "3rc-meth":
        text = "{v%d..v%d}, %s" % (u(2), u(2) + aa - 1, dx.meth(u(1)))
    elif fmt == "3rc-type":
        text = "{v%d..v%d}, %s" % (u(2), u(2) + aa - 1, dx.t(u(1)))
    else:
        text = "?" + fmt
    return ("%s %s" % (name, text)).rstrip(), size


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    dx = Dex(sys.argv[1])
    want_class, want_meths = sys.argv[2], sys.argv[3:]
    for i in range(dx.n_class):
        cidx, _f, sup, _if, _sf, _an, cdata, _sv = struct.unpack_from(
            "<8I", dx.d, dx.off_class + i * 32)
        cname = dx.t(cidx)
        if want_class not in cname or not cdata:
            continue
        print("===== %s  extends %s" % (cname, dx.t(sup)))
        p = cdata
        nsf, p = dx.uleb(p)
        nif, p = dx.uleb(p)
        ndm, p = dx.uleb(p)
        nvm, p = dx.uleb(p)
        for _ in range(nsf + nif):
            _, p = dx.uleb(p)
            _, p = dx.uleb(p)
        midx = 0
        for k in range(ndm + nvm):
            if k == ndm:
                midx = 0
            di, p = dx.uleb(p)
            _, p = dx.uleb(p)
            code_off, p = dx.uleb(p)
            midx += di
            _c, pr, n = struct.unpack_from("<HHI", dx.d, dx.off_meth + midx * 8)
            name = dx.s(n)
            if want_meths and name not in want_meths:
                continue
            print("\n--- %s%s" % (name, dx.proto(pr)))
            if not code_off:
                print("    (abstract / native)")
                continue
            rsz, ins, _outs, _tries, _dbg, insns = struct.unpack_from(
                "<HHHHII", dx.d, code_off)
            base = code_off + 16
            print("    registers=%d ins=%d   (parameters are v%d..v%d)"
                  % (rsz, ins, rsz - ins, rsz - 1))
            # A switch's payload is reachable only through its branch, so
            # collect the offsets first and skip over them on the linear walk.
            payloads = {}
            j = 0
            while j < insns:
                w = struct.unpack_from("<H", dx.d, base + j * 2)[0]
                if (w & 0xFF) in (0x26, 0x2B, 0x2C):
                    d = struct.unpack_from("<i", dx.d, base + (j + 1) * 2)[0]
                    payloads[j + d] = j
                j += max(OPS.get(w & 0xFF, (None, 1, None))[1], 1)

            j = 0
            while j < insns:
                if j in payloads:
                    got = payload(dx, base, j, payloads[j])
                    if got:
                        lines, size = got
                        print("    %#06x  %s" % (j, lines[0]))
                        for extra in lines[1:]:
                            print("            %s" % extra)
                        j += max(size, 1)
                        continue
                text, size = decode(dx, base, j, insns)
                print("    %#06x  %s" % (j, text))
                j += max(size, 1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
