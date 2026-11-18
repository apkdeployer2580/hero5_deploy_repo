#!/usr/bin/env python3
"""Follow every list position through the code that pages a filtered bag list,
and print each instruction whose meaning depends on it being a signed byte —
with the word that replaces it (ANALYSIS.md §65.11).

    python widen.py libgame.so                    # every list screen
    python widen.py libgame.so --only Refine      # functions whose name matches
    python widen.py libgame.so --rust             # the reviewed table, as Rust

The 정련 / 오브결합 / 도감 / 장비 / 스킬 / 정령 / 가방 lists are one row of five
icons paged through by a cursor the screen keeps in a **signed byte**, and the
refine/orb screens copy the chosen position into another signed byte,
`gv+0x1B3E`. With 96 bag slots no list could pass 127; with 172 one can.
"Widening" makes every position a byte read *unsigned* with arithmetic in full
registers: each comparison the guest makes stays exact up to 255, and for a
list of 127 or fewer every value is bit-for-bit what it was.

How. A forward taint analysis per function over its CFG, with two passes:

1. **Discovery.** Every byte load of `this+off` (found through `this` by
   `bagscan.py`'s pointer data flow) is a seed. The fields whose value reaches
   an index into the filter array (`ldr rX, [bag, rI, lsl #2]` after the
   `+0x7a` bias — `bagscan.py`'s `filter` region) are this screen's list
   positions. `gv+0x1B3E` and the filter count's low byte are positions
   everywhere.
2. **Report.** Only those seeds. A tainted value carries its *form* — `I`
   (the position), `S24`/`S16` (shifted left: the first half of a
   `lsl`/`asr` sign extension), `SIGN` (`asr #31` of one) — and follows
   registers and stack slots. Every instruction that reads one is printed
   with a verdict and, where the fix is mechanical, the new word.

Each verdict is checked against a second, **must** pass (a location is
tainted on *every* path): a rewrite of an instruction that some path reaches
with an unrelated value would change that value's meaning too, so those are
printed as `MAY` and left for a person.

Verdicts:

    ldrsb     seed loaded signed — becomes `ldrb`
    sx        `lsl #24` / `asr #24` / `lsr #24` of a position — becomes a move
    and255    `and #0xff` of a position — becomes `add #0`
    tst80     `tst #0x80` — becomes `tst #0x80000000` (the int's sign)
    cmpS      a compare against `lsl #24` / a constant `<< 24` — unshifted
    cmp       a compare of positions: fine unless the field holds −1 (see
              the store census) and the branch is signed
    call      a position in r0–r3: the callee's parameter type decides
    store     stored outside this frame — where, and whether that is a field
    index     an array index — fine
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import bagscan  # noqa: E402
from bagscan import Image, Scan, Fn  # noqa: E402
from capstone import CS_ARCH_ARM, CS_MODE_ARM, Cs  # noqa: E402
from capstone.arm import (  # noqa: E402
    ARM_CC_AL, ARM_OP_IMM, ARM_OP_MEM, ARM_OP_REG, ARM_REG_SP, ARM_SFT_ASR,
    ARM_SFT_LSL, ARM_SFT_LSR,
)

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")

NPC = "_ZN14StateInNpcMenu"
GAME = "_ZN15StateInGameMenu"
# Screens: the functions of one screen share its list fields (a touch handler
# may only ever compare the cursor with the count, never index with it).
FAMILIES = {
    "refine": [NPC + "9KeyRefineEih", NPC + "11TouchRefineEii", NPC + "10DrawRefineEh"],
    "orb": [NPC + "13KeyOrbCombineEi", NPC + "15TouchOrbCombineEii", NPC + "14DrawOrbCombineEv"],
    "collection": [NPC + "13KeyCollectionEi", NPC + "15TouchCollectionEii", NPC + "14DrawCollectionEv"],
    "npcprocess": [NPC + "7ProcessEv"],
    "equip": [GAME + "12KeyEquipMenuEi", GAME + "14TouchEquipMenuEii", GAME + "13DrawEquipMenuEv"],
    "skill": [GAME + "12KeySkillMenuEi", GAME + "14TouchSkillMenuEii", GAME + "13DrawSkillMenuEv"],
    "spirit": [GAME + "13KeySpiritMenuEi", GAME + "15TouchSpiritMenuEii", GAME + "14DrawSpiritMenuEv"],
    "bag": [GAME + "10KeyBagMenuEi", GAME + "12TouchBagMenuEii", GAME + "11DrawBagMenuEv"],
    "gameprocess": [GAME + "7ProcessEv"],
    "apply": ["_ZN10RefineItem15ApplyItemRefineEP13EquipItemInfosaihPP8ItemInfo",
              "_ZN10RefineItem15ApplyOrbCombineEP13EquipItemInfosaaPP8ItemInfo"],
}
FUNCS = [n for fam in FAMILIES.values() for n in fam]
GV_FIELDS = {0x1B3E}
# Fields that hold -1 ("nothing chosen"): read unsigned that is 0xFF, so
# `cmp rX, #0` + LT/MI ("none") becomes `cmp rX, #0xff` + EQ, GE/PL becomes NE.
SENTINEL = {"gv+0x1b3e"}
FLIP = {0xB: 0x0, 0x4: 0x0, 0xA: 0x1, 0x5: 0x1}  # LT, MI -> EQ; GE, PL -> NE
COND = r"(eq|ne|cs|hs|cc|lo|mi|pl|vs|vc|hi|ls|ge|lt|gt|le)$"


class Seeds(Scan):
    """bagscan's pointer data flow, recording byte loads/stores of `this`,
    `gv` and the bag count, and the filter-array index sites."""

    def __init__(self, img):
        super().__init__(img)
        self.loads, self.stores, self.filter_idx = {}, {}, set()
        self.count_loads = set()

    def entry_state(self, name):
        st = super().entry_state(name)
        st["r0"] = frozenset([("p", "THIS", 0, frozenset())])
        return st

    def note(self, f, i, a, w, store, hits):
        if a is None or a[0] not in ("p", "x"):
            return
        base, off = a[1], a[2]
        if base == "HERO":
            base, off = "BAG", off - bagscan.HERO_BAG
        if a[0] == "x" and base == "BAG" and 0x1E8 <= off < 0x368 and not store:
            self.filter_idx.add(i.address)
            return
        if a[0] != "p" or w != 1:
            return
        if base == "THIS" and off < 0x100:
            lab = f"THIS+{off:#x}"
        elif base == "GV":
            lab = f"gv+{off:#x}"
        elif base == "BAG" and off == 0x368:
            lab = "count.lo"
        else:
            return
        (self.stores if store else self.loads)[i.address] = lab

    def step1(self, f, i, st, sets):
        ex, h = super().step1(f, i, st, sets)
        # The count as a halfword: a list length, tracked so that a field
        # compared with it is recognised as a position.
        if i.mnemonic in ("ldrsh", "ldrh"):
            m = [o for o in i.operands if o.type == ARM_OP_MEM]
            if m:
                b = st.get(i.reg_name(m[0].mem.base))
                off = None
                if b is not None and b[0] == "p" and not m[0].mem.index:
                    off = b[2] + m[0].mem.disp
                elif b is not None and b[0] == "p" and m[0].mem.index:
                    ix = st.get(i.reg_name(m[0].mem.index))
                    if ix is not None and ix[0] == "c":
                        off = b[2] + ix[1]
                if off is not None and ((b[1] == "BAG" and off == 0x368)
                                        or (b[1] == "HERO" and off == bagscan.HERO_BAG + 0x368)):
                    self.count_loads.add(i.address)
        return ex, h


def rn(i, r):
    return i.reg_name(r)


def fb(form):
    """A form without its producer tag: `S24@1142d8` -> `S24`."""
    return form.split("@")[0]


def fold(v):
    return frozenset(v)


class Taint:
    """One function. `may=True` joins with union, else with intersection."""

    def __init__(self, img, md, name, seeds, may=True, fields=frozenset()):
        start, size, _ = img.by_name[name]
        self.img, self.md, self.name, self.start = img, md, name, start
        self.f = Fn(img, md, name, start, size)
        self.seeds = dict(seeds)  # addr -> origin label
        self.may = may
        self.reads = {}     # addr -> {reg: values}
        self.before = {}    # addr -> state
        # Second, independent seed finder (bagscan's pointer analysis can
        # lose `sp` and with it a spilled offset — DrawRefine+0x838 was
        # missed that way): `this` copies, bag pointers from GetBagItemPtr
        # and constants are followed here through registers and stack slots.
        self.fields = fields
        self.get_bag = img.by_name[bagscan.GET_BAG][0]
        self.found = {}

    def run(self):
        f = self.f
        states = {self.start: ({("pt", "r0"): frozenset({"this"})}, 0, {})}
        work = [self.start]
        seen = {}
        while work:
            a = work.pop()
            seen[a] = seen.get(a, 0) + 1
            if seen[a] > 80:
                continue
            i = f.ins.get(a)
            if i is None:
                continue
            st, sp, k = states[a]
            self.before[a] = (st, sp, k)
            nst, nsp, nk = self.step(i, dict(st), sp, dict(k))
            for s in f.succ(i):
                if s not in f.ins:
                    continue
                if s not in states:
                    states[s] = (nst, nsp, nk)
                    work.append(s)
                    continue
                ost, osp, ok = states[s]
                if self.may:
                    m = dict(ost)
                    for key, v in nst.items():
                        m[key] = m.get(key, frozenset()) | v
                else:
                    m = {key: ost[key] | nst[key] for key in ost if key in nst}
                mk = {r: c for r, c in ok.items() if nk.get(r) == c}
                if m != ost or mk != ok:
                    states[s] = (m, osp, mk)
                    work.append(s)
        return self

    def step(self, i, st, sp, k):
        mn = i.mnemonic
        cc = i.cc
        base = re.sub(COND, "", mn) if cc != ARM_CC_AL else mn
        if base.endswith("s") and base[:-1] in ("mov", "add", "sub", "rsb", "and", "orr", "lsl", "lsr",
                                                  "asr", "mul", "mla", "eor", "bic", "mvn"):
            base = base[:-1]
        ops = i.operands
        new, newk = {}, {}
        reads = {}

        def val(op):
            if op.type != ARM_OP_REG:
                return frozenset()
            r = rn(i, op.reg)
            v = st.get(r, frozenset())
            if v:
                reads[r] = v
            sh, n = op.shift.type, op.shift.value
            if not v or not n:
                return v
            out = set()
            for (form, org) in v:
                form = fb(form) if not form.startswith("S24") else form
                if sh == ARM_SFT_LSL:
                    # S24 carries the address of the shift that made it, so
                    # every reader of it can be traced back (see `consistent`).
                    nf = {24: f"S24@{i.address:x}", 16: "S16"}.get(n) if form == "I" else None
                    nf = nf or ("I" if form == "I" and n <= 4 else "X")
                elif sh in (ARM_SFT_ASR, ARM_SFT_LSR):
                    if (fb(form), n) in (("S24", 24), ("S16", 16)):
                        nf = "I"
                    elif n == 31:
                        nf = "SIGN"
                    elif form == "I" and n <= 4:
                        nf = "I"
                    else:
                        nf = "X"
                else:
                    nf = "X"
                out.add((nf, org))
            return fold(out)

        def ptr(r):
            return st.get(("pt", r), frozenset())

        mem = [o for o in ops if o.type == ARM_OP_MEM]
        if mem and (base.startswith("ldr") or base.startswith("str")):
            m = mem[0].mem
            dst = rn(i, ops[0].reg)
            if m.index:
                r = rn(i, m.index)
                if st.get(r):
                    reads[r] = st[r]
            if base in ("ldrb", "ldrsb", "ldrh", "ldrsh") and m.base != ARM_REG_SP:
                b = rn(i, m.base)
                off = None
                if not m.index:
                    off = m.disp
                elif not mem[0].shift.value and not mem[0].subtracted:
                    off = k.get(rn(i, m.index))
                lab = None
                if off is not None:
                    if "this" in ptr(b) and off in self.fields and base in ("ldrb", "ldrsb"):
                        lab = f"THIS+{off:#x}"
                    elif off in GV_FIELDS and base in ("ldrb", "ldrsb") and m.index:
                        lab = f"gv+{off:#x}"
                    elif "bag" in ptr(b) and off == 0x368:
                        lab = "count.lo" if base in ("ldrb", "ldrsb") else "count"
                if lab is not None:
                    self.found[i.address] = lab
                    self.seeds.setdefault(i.address, lab)
            if base.startswith("ldr"):
                if i.address in self.seeds:
                    new[dst] = fold({("I", self.seeds[i.address])})
                elif m.base == ARM_REG_SP and not m.index:
                    new[dst] = st.get(("slot", sp + m.disp), frozenset())
                else:
                    new[dst] = frozenset()
                if base == "ldrd":
                    new[rn(i, ops[1].reg)] = frozenset()
                newk[dst] = None
                new[("pt", dst)] = frozenset()
                if base == "ldr" and m.base == ARM_REG_SP and not m.index:
                    newk[dst] = k.get(("kslot", sp + m.disp))
                    new[("pt", dst)] = st.get(("pt", ("slot", sp + m.disp)), frozenset())
            else:
                v = st.get(dst, frozenset())
                if v:
                    reads[dst] = v
                if m.base == ARM_REG_SP and not m.index:
                    new[("slot", sp + m.disp)] = v
                    if base == "str":
                        newk[("kslot", sp + m.disp)] = k.get(dst)
                        new[("pt", ("slot", sp + m.disp))] = ptr(dst)
            if (i.writeback or len(ops) > 2) and m.base != ARM_REG_SP:
                new[rn(i, m.base)] = frozenset()
        elif base == "push" or (base == "stmdb" and "sp!" in i.op_str):
            regs = [rn(i, o.reg) for o in ops if o.type == ARM_OP_REG][(0 if base == "push" else 1):]
            for n_, r in enumerate(regs):
                new[("slot", sp - 4 * len(regs) + 4 * n_)] = st.get(r, frozenset())
                new[("pt", ("slot", sp - 4 * len(regs) + 4 * n_))] = ptr(r)
            sp -= 4 * len(regs)
        elif base == "pop" or (base == "ldm" and "sp!" in i.op_str):
            regs = [rn(i, o.reg) for o in ops if o.type == ARM_OP_REG][(0 if base == "pop" else 1):]
            for n_, r in enumerate(regs):
                new[r] = st.get(("slot", sp + 4 * n_), frozenset())
                new[("pt", r)] = st.get(("pt", ("slot", sp + 4 * n_)), frozenset())
                newk[r] = None
            sp += 4 * len(regs)
        elif base in ("stm", "stmib") and ops and rn(i, ops[0].reg) == "sp":
            regs = [rn(i, o.reg) for o in ops if o.type == ARM_OP_REG][1:]
            first = 0 if base == "stm" else 4
            for n_, r in enumerate(regs):
                v = st.get(r, frozenset())
                if v:
                    reads[r] = v
                new[("slot", sp + first + 4 * n_)] = v
        elif base in ("sub", "add") and len(ops) == 3 and ops[0].type == ARM_OP_REG and ops[0].reg == ARM_REG_SP \
                and ops[1].type == ARM_OP_REG and ops[1].reg == ARM_REG_SP and ops[2].type == ARM_OP_IMM:
            sp += -ops[2].imm if base == "sub" else ops[2].imm
        elif base in ("bl", "blx"):
            for r in ("r0", "r1", "r2", "r3"):
                if st.get(r):
                    reads[r] = st[r]
            for r in ("r0", "r1", "r2", "r3", "ip", "lr"):
                new[r] = frozenset()
                new[("pt", r)] = frozenset()
                newk[r] = None
            if ops and ops[0].type == ARM_OP_IMM and ops[0].imm == self.get_bag:
                new[("pt", "r0")] = frozenset({"bag"})
        elif base in ("cmp", "cmn", "tst", "teq"):
            val(ops[0])
            val(ops[1])
        elif base in ("mov", "mvn", "lsl", "lsr", "asr") and ops and ops[0].type == ARM_OP_REG:
            d = rn(i, ops[0].reg)
            if base == "mvn":
                new[d] = frozenset()
                newk[d] = (~ops[1].imm) & 0xFFFFFFFF if ops[1].type == ARM_OP_IMM else None
            elif base == "mov" and ops[1].type == ARM_OP_IMM:
                new[d] = frozenset()
                newk[d] = ops[1].imm & 0xFFFFFFFF
            elif len(ops) == 2 and ops[1].type == ARM_OP_REG:
                new[d] = val(ops[1])
                newk[d] = k.get(rn(i, ops[1].reg)) if not ops[1].shift.value else None
                if base == "mov" and not ops[1].shift.value:
                    new[("pt", d)] = ptr(rn(i, ops[1].reg))
            else:
                for o in ops[1:]:
                    val(o)
                new[d] = frozenset()
                newk[d] = None
        elif base in ("add", "sub", "rsb", "and", "orr", "eor", "bic", "mul", "mla", "smull", "umull",
                      "adc", "sbc", "smlal", "umlal"):
            if base in ("smull", "umull", "smlal", "umlal"):
                ins = [val(o) for o in ops[2:]]
                outs = [rn(i, ops[0].reg), rn(i, ops[1].reg)]
            else:
                ins = [val(o) for o in ops[1:] if o.type == ARM_OP_REG]
                outs = [rn(i, ops[0].reg)]
            u = frozenset().union(*ins) if ins else frozenset()
            res = set()
            for (form, org) in u:
                res.add(("I" if fb(form) in ("I", "SIGN") else "X", org))
            for d in outs:
                new[d] = fold(res)
                newk[d] = None
                new[("pt", d)] = frozenset()
            # constant folding: mov rX, #0x1b00 / add rX, rX, #0x3e
            if base in ("add", "sub") and len(ops) == 3 and ops[1].type == ARM_OP_REG \
                    and ops[2].type == ARM_OP_IMM and k.get(rn(i, ops[1].reg)) is not None:
                c = k[rn(i, ops[1].reg)]
                newk[outs[0]] = (c + ops[2].imm if base == "add" else c - ops[2].imm) & 0xFFFFFFFF
        else:
            if ops and ops[0].type == ARM_OP_REG and base not in ("b", "bx"):
                for o in ops[1:]:
                    val(o)
                new[rn(i, ops[0].reg)] = frozenset()
                new[("pt", rn(i, ops[0].reg))] = frozenset()
                newk[rn(i, ops[0].reg)] = None
        if ops and ops[0].type == ARM_OP_REG and rn(i, ops[0].reg) == "pc":
            for r in ("r0", "r1", "r2", "r3", "ip", "lr"):
                new[r] = frozenset()
                newk[r] = None
        if cc != ARM_CC_AL:
            for key, v in list(new.items()):
                new[key] = (v | st.get(key, frozenset())) if self.may else (v & st.get(key, frozenset()) if v else frozenset())
            for key in list(newk):
                if newk[key] != k.get(key):
                    newk[key] = None
        if reads:
            self.reads[i.address] = reads
        st.update(new)
        k.update(newk)
        st = {key: v for key, v in st.items() if v}
        k = {key: v for key, v in k.items() if v is not None}
        return st, sp, k


# ---- callee parameters -------------------------------------------------------

BUILTIN = set("vwbcahstijlmxynofdeg")


def params(mangled):
    """The parameter list of a mangled function as type codes, `this` first
    for a member function; None when the name is not one this reads."""
    s = mangled
    if not s.startswith("_Z"):
        return None
    p = 2
    member = False
    try:
        if s[p] == "N":
            member = True
            p += 1
            while s[p] != "E":
                if s[p] in "CD" and s[p + 1] in "0123":
                    p += 2  # constructor / destructor
                    continue
                m = re.match(r"\d+", s[p:])
                if not m:
                    return None
                p += len(m.group()) + int(m.group())
            p += 1
        else:
            m = re.match(r"\d+", s[p:])
            if not m:
                return None
            p += len(m.group()) + int(m.group())
    except IndexError:
        return None
    out = ["this"] if member else []

    def one(p):
        quals = ""
        while s[p] in "PRK":
            quals += s[p]
            p += 1
        if s[p] in BUILTIN:
            return quals + s[p], p + 1
        m = re.match(r"\d+", s[p:])
        if m:
            n = int(m.group())
            return quals + s[p + len(m.group()):p + len(m.group()) + n], p + len(m.group()) + n
        if s[p] == "N":
            q = p + 1
            while s[q] != "E":
                m = re.match(r"\d+", s[q:])
                if not m:
                    raise ValueError
                q += len(m.group()) + int(m.group())
            return quals + "N", q + 1
        raise ValueError
    try:
        while p < len(s):
            t, p = one(p)
            if t == "v":
                break
            out.append(t)
    except (ValueError, IndexError):
        return None
    return out


C_ARITY = {"__divsi3": ["i", "i"], "__aeabi_idiv": ["i", "i"], "__modsi3": ["i", "i"],
           "MC_knlSprintk": ["Pc", "Pc", "...", "..."]}


# ---- rewrites ---------------------------------------------------------------

def enc_imm(v):
    for rot in range(16):
        x = ((v << (2 * rot)) | (v >> (32 - 2 * rot))) & 0xFFFFFFFF if rot else v
        if x < 0x100:
            return (rot << 8) | x
    return None


def word(img, a):
    return int.from_bytes(img.read(a, 4), "little")


def ldrsb_to_ldrb(w):
    cond = w & 0xF0000000
    p, u, wb = (w >> 24) & 1, (w >> 23) & 1, (w >> 21) & 1
    rn_, rt = (w >> 16) & 0xF, (w >> 12) & 0xF
    if (w >> 22) & 1:  # immediate
        imm = ((w >> 4) & 0xF0) | (w & 0xF)
        return cond | 0x04500000 | p << 24 | u << 23 | wb << 21 | rn_ << 16 | rt << 12 | imm
    rm = w & 0xF
    return cond | 0x06500000 | p << 24 | u << 23 | wb << 21 | rn_ << 16 | rt << 12 | rm


def unshift(w):
    """Data-processing register form: clear the shift (type and amount)."""
    return w & ~0x00000FE0


def verdict(img, i, reads, must_regs):
    """(kind, new word or None, note)"""
    mn = i.mnemonic
    w = word(img, i.address)
    forms = set()
    for r, v in reads.items():
        forms |= {fb(f) for f, _ in v}
    must = all(r in must_regs for r in reads)
    base = re.sub(COND, "", mn) if i.cc != ARM_CC_AL else mn
    ops = i.operands
    if base in ("ldr", "ldrb", "ldrh", "ldrsh", "ldrsb") and any(o.type == ARM_OP_MEM for o in ops):
        return ("index", None, "", must)
    if base in ("str", "strb", "strh"):
        return ("store", None, "", must)
    if base in ("bl", "blx"):
        return ("call", None, "", must)
    if base in ("lsl", "lsls") and len(ops) == 2 and ops[1].shift.value == 24 and forms <= {"I"}:
        return ("sx", unshift(w), "lsl #24 -> mov", must)
    if base in ("asr", "lsr", "asrs", "lsrs") and len(ops) == 2 and ops[1].shift.value == 24 and forms <= {"S24"}:
        return ("sx", unshift(w), f"{base} #24 -> mov", must)
    if base in ("asr", "asrs") and len(ops) == 2 and ops[1].shift.value == 31:
        return ("sign", None, "", must)
    if base in ("lsl", "asr", "lsr") and len(ops) == 2 and ops[1].shift.value == 16 and forms <= {"I", "S16"}:
        return ("sx16", None, "", must)
    if base == "and" and ops[-1].type == ARM_OP_IMM and ops[-1].imm == 0xFF and forms <= {"I"}:
        # AND (0000) -> ADD (0100), operand2 = #0
        return ("and255", (w & ~0x01E00FFF) | 0x00800000, "and #0xff -> add #0", must)
    if base == "tst" and ops[1].type == ARM_OP_IMM and ops[1].imm == 0x80 and forms <= {"I"}:
        return ("tst80", (w & ~0xFFF) | enc_imm(0x80000000), "tst #0x80 -> #0x80000000", must)
    # an operand that is a shifted position, unshifted in its own operand
    last = ops[-1] if ops else None
    if last is not None and last.type == ARM_OP_REG and last.shift.type in (ARM_SFT_ASR, ARM_SFT_LSR) \
            and last.shift.value == 24 and {fb(f) for f, _ in reads.get(rn(i, last.reg), ())} == {"S24"}:
        return ("opsx", unshift(w), "operand asr/lsr #24 -> plain", must)
    if base in ("cmp", "cmn"):
        a_forms = {fb(f) for f, _ in reads.get(rn(i, ops[0].reg), ())}
        if a_forms == {"S24"} and ops[1].type == ARM_OP_IMM and ops[1].imm & 0xFFFFFF == 0:
            e = enc_imm(ops[1].imm >> 24)
            return ("cmpS", (w & ~0xFFF) | e, f"#{ops[1].imm:#x} -> #{ops[1].imm >> 24:#x}", must)
        # `==`, not `<=`: an untainted left side (the empty set) is someone
        # else's `x << 24` — the cell loop's `i << 24` reloaded from a stack
        # slot in TouchEquipMenu/TouchSpiritMenu — and dropping only the right
        # side's shift made `i << 24 == cursor % 5`, true only for cell 0, so
        # a second tap on any other cell never equipped it (2026-10-16).
        if a_forms == {"S24"} and ops[1].type == ARM_OP_REG and ops[1].shift.type == ARM_SFT_LSL \
                and ops[1].shift.value == 24:
            return ("cmpS", unshift(w), "operand lsl #24 -> plain", must)
        if "S24" in forms or "X" in forms:
            return ("?", None, "", must)
        return ("cmp", None, "", must)
    if base.rstrip("s") in ("add", "sub", "rsb", "mul", "mla", "smull", "orr") and forms <= {"I", "SIGN"}:
        return ("arith", None, "", must)
    if base in ("lsl", "lsls", "asr", "asrs", "lsr", "lsrs") and len(ops) == 2 and forms <= {"I"} \
            and (ops[1].shift.value <= 4 or ops[1].shift.value == 30):
        return ("arith", None, "small shift of a non-negative position", must)
    if base == "tst" and ops[1].type == ARM_OP_IMM and ops[1].imm < 0x80 and forms <= {"I"}:
        return ("arith", None, "low-bit test", must)
    if base == "cmp" and forms <= {"X"} and ops[1].type == ARM_OP_IMM and ops[1].imm == 0xC0000000:
        return ("arith", None, "low-bit test", must)
    if base in ("mov",) and len(ops) == 2 and not ops[1].shift.value:
        return ("move", None, "", must)
    return ("?", None, "", must)


def discover(img, md, name, sc_all):
    """THIS fields that index the filter array or meet the count in a compare."""
    seeds_all = dict(sc_all.loads)
    for a in sc_all.count_loads:
        seeds_all[a] = "count"
    t = Taint(img, md, name, seeds_all, may=True).run()
    fields = set()
    for a in sc_all.filter_idx:
        i = t.f.ins.get(a)
        if i is None:
            continue
        m = [o for o in i.operands if o.type == ARM_OP_MEM]
        if m and m[0].mem.index:
            v = t.reads.get(a, {}).get(rn(i, m[0].mem.index), ())
            fields |= {org for _, org in v if org.startswith("THIS+")}
    for a, reads in t.reads.items():
        i = t.f.ins[a]
        if i.mnemonic.startswith("cmp") or i.mnemonic.startswith("cmn"):
            orgs = {org for v in reads.values() for _, org in v}
            if orgs & {"count", "count.lo"}:
                fields |= {o for o in orgs if o.startswith("THIS+")}
    return fields


HANDLED = {"sx", "opsx", "cmpS", "sign"}
BAD = {}


def callee_takes(img, i, r):
    """Does the callee of `bl` read register `r` (r0..r3) as an argument?
    Unknown callees are assumed to."""
    tgt = int(i.op_str.lstrip("#"), 0) if i.op_str.startswith("#") else None
    callee = img.syms.get(tgt, ("?",))[0] if tgt else "?"
    if callee == "?" and tgt in img.plt:
        callee = img.plt[tgt]
    ps = params(callee) if callee.startswith("_Z") else C_ARITY.get(callee)
    if ps is None or r not in ("r0", "r1", "r2", "r3"):
        return r in ("r0", "r1", "r2", "r3")
    return int(r[1]) < len(ps) or (ps and ps[-1] == "...")


def consistent(img, f, may, must):
    """Which `lsl #24` producers may become moves: every reader of the
    shifted value (through registers and stack slots) must be a shape that
    is rewritten to match — `asr/lsr #24`, an operand `asr #24`, a compare
    against `k << 24` / `x, lsl #24`, or `asr #31`. One reader of another
    shape (`DrawSpiritMenu`'s `asr #8` that makes `x << 16`) keeps the whole
    chain as it is, and so does a reader fed by two producers that disagree.
    Returns (ok: producer -> bool, prods: reader -> producers)."""
    cons, prods = {}, {}
    for adr, reads in may.reads.items():
        for v in reads.values():
            for form, _ in v:
                if form.startswith("S24@"):
                    p = int(form[4:], 16)
                    cons.setdefault(p, set()).add(adr)
                    prods.setdefault(adr, set()).add(p)
    ok = {}
    for p, cs in cons.items():
        good = True
        for c in cs:
            i = f.ins[c]
            if i.mnemonic.startswith("str") and i.operands[-1].type == ARM_OP_MEM \
                    and i.operands[-1].mem.base == ARM_REG_SP:
                continue  # a spill: its reload's readers are checked themselves
            kind = verdict(img, i, may.reads[c], set(must.reads.get(c, {})))[0]
            if kind == "call":
                regs = [r for r, v in may.reads[c].items() if any(fm == f"S24@{p:x}" for fm, _ in v)]
                if not any(callee_takes(img, i, r) for r in regs):
                    continue  # left over in a register the callee does not read
            if kind not in HANDLED:
                good = False
                BAD.setdefault(p, []).append(f"{c:#x} {i.mnemonic} {i.op_str} [{kind}]")
        ok[p] = good
    changed = True
    while changed:
        changed = False
        for c, ps in prods.items():
            if not all(ok.get(p, True) for p in ps):
                for p in ps:
                    if ok.get(p, True):
                        ok[p] = False
                        changed = True
    return ok, prods


def analyse(img, md, name, sc_all, fields):
    keep = fields | {f"gv+{o:#x}" for o in GV_FIELDS} | {"count.lo", "count"}
    seeds = {a: lab for a, lab in sc_all.loads.items() if lab in keep}
    for a in sc_all.count_loads:
        seeds[a] = "count"
    offs = frozenset(int(x[5:], 16) for x in fields if x.startswith("THIS+"))
    probe = Taint(img, md, name, seeds, may=True, fields=offs).run()
    extra = {a: lab for a, lab in probe.found.items() if a not in seeds}
    seeds = probe.seeds
    may = Taint(img, md, name, seeds, may=True, fields=offs).run()
    must = Taint(img, md, name, seeds, may=False, fields=offs).run()
    return seeds, may, must, extra


def sentinel_rewrites(img, f, i, reads):
    """For `cmp rX, #0` on a sentinel field: the compare and every flag
    reader up to the next flag writer. None if one of them cannot be flipped."""
    orgs = {org for v in reads.values() for _, org in v}
    if not (orgs & SENTINEL) or i.mnemonic != "cmp" or i.operands[1].type != ARM_OP_IMM \
            or i.operands[1].imm != 0:
        return None
    w = word(img, i.address)
    out = [(i.address, w, (w & ~0xFFF) | 0xFF, "sentinel: cmp #0 -> #0xff")]
    a = i.address + 4
    while a in f.ins:
        j = f.ins[a]
        jw = word(img, a)
        cond = jw >> 28
        if cond != 0xE:
            if cond not in FLIP:
                return out + [(a, jw, None, f"sentinel: cannot flip cond {cond:#x}")]
            out.append((a, jw, (jw & 0x0FFFFFFF) | FLIP[cond] << 28, f"sentinel: cond {cond:#x} -> {FLIP[cond]:#x}"))
        if j.mnemonic.startswith(("cmp", "cmn", "tst", "teq", "bl")) or (j.mnemonic.endswith("s") and len(j.mnemonic) > 3
                                                                        and j.mnemonic[:-1] in ("add", "sub", "mov", "and", "orr", "lsl", "asr", "lsr", "rsb")):
            break
        if j.mnemonic in ("b", "bx", "pop") or (cond == 0xE and j.mnemonic.startswith("b")):
            break
        a += 4
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("so", type=Path)
    ap.add_argument("--only", default=".")
    ap.add_argument("--all", action="store_true", help="also print fine instructions")
    ap.add_argument("--rust", action="store_true", help="print the rewrites as a Rust table")
    ap.add_argument("--seeds", action="store_true", help="print every seed load (address label)")
    a = ap.parse_args()
    img = Image(a.so)
    md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
    md.detail = True
    md.skipdata = True
    rx = re.compile(a.only)
    totals = {}
    scans, fam_fields = {}, {}
    for fam, names in FAMILIES.items():
        fam_fields[fam] = set()
        for name in names:
            start, size, _ = img.by_name[name]
            sc = Seeds(img)
            sc.run(name, start, size)
            scans[name] = sc
            fam_fields[fam] |= discover(img, md, name, sc)
    todo = [(fam, name) for fam, names in FAMILIES.items() for name in names]
    rewrites = []  # (name, start, addr, was, new, why)
    for fam, name in todo:
        if not rx.search(name):
            continue
        start, size, _ = img.by_name[name]
        sc = scans[name]
        fields = fam_fields[fam]
        seeds, may, must, extra = analyse(img, md, name, sc, fields)
        f = may.f
        if a.seeds:
            for adr, lab in sorted(seeds.items()):
                print(f"SEED {adr:#x} {lab} {name}")
            continue
        print(f"===== {name}  [{fam}] list fields {sorted(fields)}  ({len(seeds)} seed load(s))")
        for adr, lab in sorted(extra.items()):
            print(f"  {adr:#08x} +{adr - start:#06x} EXTRA  {f.ins[adr].mnemonic + ' ' + f.ins[adr].op_str:42} {lab}"
                  "  (found by the second seed finder only)")
        keep = fields | {f"gv+{o:#x}" for o in GV_FIELDS}
        for adr, lab in sorted(sc.stores.items()):
            if lab not in keep or adr not in may.before:
                continue
            i = f.ins[adr]
            st, _, k = may.before[adr]
            r = rn(i, i.operands[0].reg)
            c = k.get(r)
            tainted = st.get(r)
            what = (f"const {c:#x}" if c is not None else
                    "position" if tainted else "OTHER")
            if c is not None and c & 0x80:
                what += "  <-- NEGATIVE / >= 0x80"
            print(f"  {adr:#08x} +{adr - start:#06x} STORE  {i.mnemonic + ' ' + i.op_str:42} {lab:10} {what}")
        sx_ok, sx_prods = consistent(img, f, may, must)
        for adr in sorted(set(may.reads) | set(seeds)):
            i = f.ins[adr]
            if adr in seeds:
                txt = f"{i.mnemonic} {i.op_str}"
                if i.mnemonic.startswith("ldrsb"):
                    w = word(img, adr)
                    print(f"  {adr:#08x} +{adr - start:#06x} ldrsb  {txt:42} {w:08x} -> {ldrsb_to_ldrb(w):08x}  {seeds[adr]}")
                    totals["ldrsb"] = totals.get("ldrsb", 0) + 1
                    rewrites.append((name, start, adr, w, ldrsb_to_ldrb(w), "ldrsb -> ldrb " + seeds[adr]))
                elif a.all:
                    print(f"  {adr:#08x} +{adr - start:#06x} seed   {txt:42} {seeds[adr]}")
                if adr not in may.reads:
                    continue
            reads = may.reads[adr]
            mreads = must.reads.get(adr, {})
            sen = sentinel_rewrites(img, f, i, reads)
            if sen is not None:
                for (sa, sw, snw, why) in sen:
                    print(f"  {sa:#08x} +{sa - start:#06x} SENT   {f.ins[sa].mnemonic + ' ' + f.ins[sa].op_str:42} "
                          f"{sw:08x} -> {snw:08x}  {why}" if snw is not None else f"  {sa:#08x} ?? {why}")
                    if snw is not None:
                        rewrites.append((name, start, sa, sw, snw, why))
                    totals["sentinel"] = totals.get("sentinel", 0) + 1
                continue
            kind, nw, note, ok = verdict(img, i, reads, set(mreads))
            totals[kind] = totals.get(kind, 0) + 1
            kept = (adr in sx_ok and not sx_ok[adr]) or \
                (adr in sx_prods and not all(sx_ok.get(p_, True) for p_ in sx_prods[adr]))
            if kept and nw is not None:
                print(f"  {adr:#08x} +{adr - start:#06x} KEPT   {i.mnemonic + ' ' + i.op_str:42} "
                      f"(a reader of this shifted value is not a rewritable shape) {BAD.get(adr, '')}")
                totals["kept"] = totals.get("kept", 0) + 1
                nw = None
                continue
            if nw is not None:
                rewrites.append((name, start, adr, word(img, adr), nw, f"{kind}: {note}" + ("" if ok else " (may)")))
            if kind in ("index", "arith", "move", "sign", "sx16") and not a.all:
                continue
            if kind == "cmp" and not a.all:
                # only compares that a sentinel could matter to: against 0/−1
                o = i.operands
                if not (len(o) == 2 and o[1].type == ARM_OP_IMM and o[1].imm in (0, 1, 0xFFFFFFFF)):
                    continue
            orgs = sorted({org for v in reads.values() for _, org in v})
            frm = sorted({fb(fm) for v in reads.values() for fm, _ in v})
            extra = ""
            if kind == "call":
                tgt = int(i.op_str.lstrip("#"), 0) if i.op_str.startswith("#") else None
                callee = img.syms.get(tgt, ("?",))[0] if tgt else "?"
                if callee == "?" and tgt in img.plt:
                    callee = img.plt[tgt]
                ps = params(callee) if callee.startswith("_Z") else C_ARITY.get(callee)
                hit = []
                for r in sorted(reads):
                    n = int(r[1])
                    if ps is None:
                        ty = "?"
                    elif n < len(ps):
                        ty = ps[n]
                    elif ps and ps[-1] == "...":
                        ty = "..."
                    else:
                        continue  # not an argument of this callee
                    hit.append(f"{r}:{ty}")
                if not hit:
                    continue
                kind = "call" if all(h.split(":")[1] in ("i", "s", "j", "t", "...", "l", "m") for h in hit) else "CALL"
                if kind == "call" and not a.all:
                    continue
                extra = callee + " " + ",".join(hit)
            if kind == "store":
                if i.operands[-1].type == ARM_OP_MEM and i.operands[-1].mem.base == ARM_REG_SP:
                    continue
                extra = sc.stores.get(adr, "elsewhere")
            w = word(img, adr)
            nws = f"{w:08x} -> {nw:08x}" if nw is not None else f"{w:08x}"
            flag = "" if ok else "MAY "
            print(f"  {adr:#08x} +{adr - start:#06x} {flag}{kind:6} {i.mnemonic + ' ' + i.op_str:42} {nws:20} "
                  f"{'/'.join(frm)} {','.join(orgs)} {note} {extra}")
        print()
    print("# " + ", ".join(f"{k} {v}" for k, v in sorted(totals.items())), file=sys.stderr)
    seen = {}
    for r in rewrites:
        if r[2] in seen and seen[r[2]][4] != r[4]:
            print(f"!! conflicting rewrites at {r[2]:#x}: {seen[r[2]][4]:08x} vs {r[4]:08x}", file=sys.stderr)
        seen[r[2]] = r
    print(f"# {len(seen)} word(s) to rewrite", file=sys.stderr)
    if a.rust:
        syms = {}
        for (name, start, adr, w, nw, why) in sorted(seen.values(), key=lambda r: r[2]):
            syms.setdefault(name, []).append((adr - start, w, nw, why))
        for name, rows in syms.items():
            print(f"    // {name}")
            for (off, w, nw, why) in rows:
                print(f"    wd({short(name)}, {off:#x}, 0x{w >> 16:04x}_{w & 0xffff:04x}, 0x{nw >> 16:04x}_{nw & 0xffff:04x}), // {why}")


SHORT = {
    NPC + "9KeyRefineEih": "KEY_REFINE", NPC + "11TouchRefineEii": "TOUCH_REFINE",
    NPC + "10DrawRefineEh": "DRAW_REFINE", NPC + "13KeyOrbCombineEi": "KEY_ORB",
    NPC + "15TouchOrbCombineEii": "TOUCH_ORB", NPC + "14DrawOrbCombineEv": "DRAW_ORB",
    NPC + "13KeyCollectionEi": "KEY_COLLECTION", NPC + "15TouchCollectionEii": "TOUCH_COLLECTION",
    NPC + "14DrawCollectionEv": "DRAW_COLLECTION", NPC + "7ProcessEv": "NPC_PROCESS",
    GAME + "12KeyEquipMenuEi": "KEY_EQUIP", GAME + "14TouchEquipMenuEii": "TOUCH_EQUIP",
    GAME + "13DrawEquipMenuEv": "DRAW_EQUIP", GAME + "12KeySkillMenuEi": "KEY_SKILL",
    GAME + "14TouchSkillMenuEii": "TOUCH_SKILL", GAME + "13DrawSkillMenuEv": "DRAW_SKILL",
    GAME + "13KeySpiritMenuEi": "KEY_SPIRIT", GAME + "15TouchSpiritMenuEii": "TOUCH_SPIRIT",
    GAME + "14DrawSpiritMenuEv": "DRAW_SPIRIT", GAME + "10KeyBagMenuEi": "KEY_BAG",
    GAME + "12TouchBagMenuEii": "TOUCH_BAG", GAME + "11DrawBagMenuEv": "DRAW_BAG",
    GAME + "7ProcessEv": "GAME_PROCESS",
    "_ZN10RefineItem15ApplyItemRefineEP13EquipItemInfosaihPP8ItemInfo": "APPLY_REFINE",
    "_ZN10RefineItem15ApplyOrbCombineEP13EquipItemInfosaaPP8ItemInfo": "APPLY_ORB",
}


def short(name):
    return SHORT.get(name, '"' + name + '"')


if __name__ == "__main__":
    main()
