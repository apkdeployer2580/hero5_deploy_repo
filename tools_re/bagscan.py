#!/usr/bin/env python3
"""Every instruction that reaches a field of the hero's `BagItem`, by data
flow rather than by immediate — and which instructions built the offset.

    python bagscan.py libgame.so                       # all functions
    python bagscan.py libgame.so --funcs 'BagItem|InGameMenu'
    python bagscan.py libgame.so --region stack        # only +0x188..+0x1e8

Why it exists (ANALYSIS.md §65): moving the bag's stack-count and filter
arrays means changing every immediate that encodes `+0x188`, `+0x1e8`,
`HERO+0x1948`, `HERO+0x19A8` or an index bias of `0x7a`, and none of those
numbers is unique to the bag — `ItemInfo` has fields at the same offsets and
`gv+0x1940..` is a run of globals. A census of immediates drowns; this follows
the pointer instead.

How: a forward data-flow analysis over each function's CFG. Values are
constants, pointers (`HERO`/`BAG`/`GV`/the entry stack pointer, plus a known
offset), indexed pointers (base + offset + scale*unknown) and biased indices
(unknown + offset). Sources: `this` of `HERO`/subclass and `BagItem` methods,
`HERO::GetBagItemPtr()`'s return, `gv` from the GOT and `gv+0x1474` (the
player's `HERO*`). Every value carries the set of instruction addresses whose
immediates went into its offset, so an access to the stack array prints the
words to rewrite. Joins at CFG merges keep a value only if both sides agree.

It is a lower bound on the code that knows the layout (a pointer laundered
through a heap object is lost), which is why the probe's `--memwatch bag`
cross-checks it at run time.
"""
from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from disasm import Image  # noqa: E402

from capstone import CS_ARCH_ARM, CS_MODE_ARM, Cs  # noqa: E402
from capstone.arm import (  # noqa: E402
    ARM_CC_AL, ARM_OP_IMM, ARM_OP_MEM, ARM_OP_REG, ARM_REG_PC, ARM_REG_SP,
    ARM_SFT_LSL,
)

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")

HERO_BAG = 0x17C0
GV_HERO = 0x1474
REGIONS = {
    "ptr": (0x008, 0x188),
    "stack": (0x188, 0x1E8),
    "filter": (0x1E8, 0x368),
    "head": (0x000, 0x008),
    "fcount": (0x368, 0x36C),
}
HERO_CLASSES = ("_ZN4HERO", "_ZN7WARRIOR", "_ZN5ROGUE", "_ZN6GUNNER", "_ZN6KNIGHT")
GET_BAG = "_ZN4HERO13GetBagItemPtrEv"

# value: None (unknown) or a tuple
#   ('c', v, prov)                constant
#   ('p', base, off, prov)        base + off
#   ('x', base, off, scale, prov) base + off + scale * unknown
#   ('i', off, prov)              unknown + off
#   ('sx', inner, k)              inner << k, waiting for asr #k


TOP = frozenset([None])
MAX_ATOMS = 8


def normalize(vs):
    """Widen a set of possible values so loops terminate: pointers on one
    base with different offsets become one indexed pointer at the lowest
    offset ("somewhere in the array that starts there"), biased indices keep
    the lowest bias, and a set that grows past MAX_ATOMS becomes unknown."""
    if not vs:
        return TOP
    out = set()
    by_base = {}
    ints = {}
    consts = set()
    for v in vs:
        if v is None:
            out.add(None)
        elif v[0] in ("p", "x"):
            by_base.setdefault(v[1], []).append(v)
        elif v[0] == "i":
            ints.setdefault(0, []).append(v)
        elif v[0] == "c":
            consts.add(v)
        else:
            out.add(v)
    for base, vals in by_base.items():
        if len(vals) == 1:
            out.add(vals[0])
            continue
        offs = {v[2] for v in vals}
        scales = {v[3] for v in vals if v[0] == "x"}
        pv = frozenset().union(*(prov(v) for v in vals))
        if len(offs) == 1 and all(v[0] == "p" for v in vals):
            out.add(("p", base, min(offs), pv))
        elif len(offs) == 1 and len(scales) == 1 and all(v[0] == "x" for v in vals):
            out.add(("x", base, min(offs), scales.pop(), pv))
        else:
            out.add(("x", base, min(offs), 1, pv))
    for vals in ints.values():
        pv = frozenset().union(*(v[2] for v in vals))
        out.add(("i", min(v[1] for v in vals), pv))
    if len(consts) > 4:
        out.add(None)
    else:
        # identical numbers from different instructions: one atom
        merged = {}
        for c in consts:
            merged.setdefault(c[1], set()).update(c[2])
        for v, pv in merged.items():
            out.add(("c", v, frozenset(pv)))
    if len(out) > MAX_ATOMS:
        return TOP
    return frozenset(out)


def join(a, b):
    if a == b:
        return a
    if a is None or b is None:
        return None
    if a[0] == b[0] and a[0] in ("c", "i") and a[1] == b[1]:
        return (a[0], a[1], a[2] | b[2])
    if a[0] == b[0] == "p" and a[1:3] == b[1:3]:
        return ("p", a[1], a[2], a[3] | b[3])
    if a[0] == b[0] == "x" and a[1:4] == b[1:4]:
        return ("x", a[1], a[2], a[3], a[4] | b[4])
    return None


def prov(v):
    if v is None:
        return frozenset()
    if v[0] in ("c", "i"):
        return v[2]
    if v[0] == "p":
        return v[3]
    if v[0] == "x":
        return v[4]
    if v[0] == "sx":
        return prov(v[1])
    return frozenset()


def add(a, b, at=None):
    """a + b, where either may carry provenance; `at` is the instruction
    contributing an immediate (None when both sides are registers)."""
    extra = frozenset([at]) if at is not None else frozenset()
    if a is None or b is None:
        # unknown + const = biased index
        k = b if a is None else a
        if k is not None and k[0] == "c":
            return ("i", k[1], k[2] | extra)
        return None
    if a[0] == "c" and b[0] == "c":
        return ("c", (a[1] + b[1]) & 0xFFFFFFFF, a[2] | b[2] | extra)
    if a[0] == "c":
        a, b = b, a
    if b[0] == "c":
        c = b[1] if b[1] < 0x80000000 else b[1] - 0x100000000
        if a[0] == "p":
            return ("p", a[1], a[2] + c, a[3] | b[2] | extra)
        if a[0] == "x":
            return ("x", a[1], a[2] + c, a[3], a[4] | b[2] | extra)
        if a[0] == "i":
            return ("i", a[1] + c, a[2] | b[2] | extra)
        return None
    return None


def add_index(p, idx, k, at=None):
    """p + (idx << k) where p is a pointer and idx is an index-ish value."""
    extra = frozenset([at]) if at is not None else frozenset()
    if p is None or p[0] != "p":
        return None
    if idx is None:
        return ("x", p[1], p[2], 1 << k, p[3] | extra)
    if idx[0] == "c":
        return ("p", p[1], p[2] + (idx[1] << k), p[3] | idx[2] | extra)
    if idx[0] == "i":
        return ("x", p[1], p[2] + (idx[1] << k), 1 << k, p[3] | idx[2] | extra)
    if idx[0] == "sx":
        return add_index(p, idx[1], k, at)
    return ("x", p[1], p[2], 1 << k, p[3] | extra)


class Fn:
    def __init__(self, img, md, name, start, size):
        self.img, self.name, self.start, self.end = img, name, start, start + size
        code = img.read(start, size) or b""
        # `skipdata` hands literal-pool words back as data (id 0); they are
        # not instructions and never reachable.
        self.ins = {i.address: i for i in md.disasm(code, start) if i.id != 0}

    def succ(self, i):
        """Successor addresses of instruction `i` (None = unknown/exit)."""
        mn, op = i.mnemonic, i.op_str
        cc = i.cc
        nxt = i.address + 4
        # Writes to pc
        if mn in ("bx",) and op == "lr":
            return [nxt] if cc != ARM_CC_AL else []
        if mn.startswith("pop") or (mn.startswith("ldm") and "pc" in op):
            if "pc" in op:
                return [nxt] if cc != ARM_CC_AL else []
        if mn == "b" or (mn.startswith("b") and len(mn) == 3 and mn[1:] in CONDS):
            t = int(op.lstrip("#"), 0)
            out = []
            if self.start <= t < self.end:
                out.append(t)
            if cc != ARM_CC_AL:
                out.append(nxt)
            return out
        if mn in ("add", "addls", "addlo", "addcc") and op.startswith("pc, pc,"):
            # jump table: the default `b` at pc+4, then one `b` per case
            out = [nxt]
            a = nxt + 4
            while a in self.ins and self.ins[a].mnemonic == "b" and len(out) < 256:
                out.append(a)
                a += 4
            return out
        if (mn.startswith("ldr") and op.startswith("pc,")) or (mn.startswith("mov") and op.startswith("pc,")):
            prev = self.ins.get(i.address - 4)
            if prev is not None and prev.mnemonic == "mov" and prev.op_str == "lr, pc":
                # `mov lr, pc; ldr pc, [...]` — an indirect call, not an exit
                return [nxt]
            return [nxt] if cc != ARM_CC_AL else []
        return [nxt]


CONDS = {"eq", "ne", "cs", "hs", "cc", "lo", "mi", "pl", "vs", "vc", "hi", "ls",
         "ge", "lt", "gt", "le"}


def reg(i, r):
    return i.reg_name(r)


class Scan:
    def __init__(self, img: Image):
        self.img = img
        self.md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
        self.md.detail = True
        # Literal pools sit inside functions; capstone stops at the first word
        # it cannot decode unless told to skip data.
        self.md.skipdata = True
        self.get_bag = img.by_name[GET_BAG][0]
        self.gv = img.by_name["gv"][0]
        self.hits = []  # (addr, fn, base, off, scale, width, store, prov, text)
        self.escapes = []

    def entry_state(self, name):
        st = {"sp": frozenset([("p", "SP", 0, frozenset())])}
        if name.startswith(HERO_CLASSES):
            st["r0"] = frozenset([("p", "HERO", 0, frozenset())])
        elif name.startswith("_ZN7BagItem"):
            st["r0"] = frozenset([("p", "BAG", 0, frozenset())])
        return st

    def run(self, name, start, size):
        f = Fn(self.img, self.md, name, start, size)
        if start not in f.ins:
            return
        states = {start: self.entry_state(name)}
        work = [start]
        seen_count = defaultdict(int)
        recorded = {}
        while work:
            a = work.pop()
            seen_count[a] += 1
            if seen_count[a] > 400:
                continue
            i = f.ins.get(a)
            if i is None:
                continue
            st = dict(states[a])
            out, hits = self.step(f, i, st)
            for h in hits:
                recorded[(h[0], h[2], h[3], h[4])] = h
            for s in f.succ(i):
                if s not in f.ins:
                    continue
                if s not in states:
                    states[s] = dict(out)
                    work.append(s)
                else:
                    old = states[s]
                    new = {k: normalize(old.get(k, TOP) | out.get(k, TOP)) for k in set(old) | set(out)}
                    new = {k: v for k, v in new.items() if v != TOP}
                    if new != old:
                        states[s] = new
                        work.append(s)
        self.hits.extend(recorded.values())

    # ---- one instruction ------------------------------------------------
    def step(self, f, i, sets):
        """Lift `step1` over every combination of the possible values of the
        registers the instruction reads."""
        try:
            read, _ = i.regs_access()
        except Exception:
            read = []
        names = sorted({i.reg_name(r) for r in read} | {"sp"})
        choices = [sorted(sets.get(n, TOP), key=repr) for n in names]
        total = 1
        for c in choices:
            total *= len(c)
        if total > 64:
            # Too many combinations: keep pointers, widen the rest.
            choices = [c if len(c) <= 2 else [x for x in c if x and x[0] in ("p", "x")][:2] + [None]
                       for c in choices]
        import itertools
        acc = {}
        hits = []
        for combo in itertools.product(*choices):
            st1 = dict(zip(names, combo))
            ex, h = self.step1(f, i, st1, sets)
            hits.extend(h)
            for k, v in ex.items():
                acc.setdefault(k, set()).update(v if isinstance(v, frozenset) else {v})
        new = dict(sets)
        for k, vs in acc.items():
            vs = frozenset(vs)
            if i.cc != ARM_CC_AL:
                vs = vs | sets.get(k, TOP)
            vs = normalize(vs)
            if vs == TOP:
                new.pop(k, None)
            else:
                new[k] = vs
        return new, hits

    def step1(self, f, i, st, sets):
        mn = i.mnemonic
        base_mn = re.sub(r"(eq|ne|cs|hs|cc|lo|mi|pl|vs|vc|hi|ls|ge|lt|gt|le)$", "", mn) if i.cc != ARM_CC_AL else mn
        ops = i.operands
        hits = []
        new = dict(st)

        def val(op):
            if op.type == ARM_OP_IMM:
                return ("c", op.imm & 0xFFFFFFFF, frozenset([i.address]))
            if op.type == ARM_OP_REG:
                r = reg(i, op.reg)
                if op.reg == ARM_REG_PC:
                    return ("c", i.address + 8, frozenset())
                v = st.get(r)
                if op.shift.type == ARM_SFT_LSL and op.shift.value:
                    k = op.shift.value
                    if v is not None and v[0] == "c":
                        return ("c", (v[1] << k) & 0xFFFFFFFF, v[2])
                    return ("shl", v, k)
                return v
            return None

        def setr(r, v):
            if v is None or (isinstance(v, tuple) and v[0] == "shl"):
                new.pop(r, None)
            else:
                new[r] = v

        def addr_of(o):
            m = o.mem
            b = st.get(reg(i, m.base)) if m.base != ARM_REG_PC else ("c", i.address + 8, frozenset())
            if m.index:
                idx = st.get(reg(i, m.index))
                # capstone puts the index's shift on the operand, not in mem
                k = o.shift.value if o.shift.type == ARM_SFT_LSL else 0
                if o.shift.type not in (0, ARM_SFT_LSL):
                    return None
                if b is not None and b[0] == "p":
                    a = add_index(b, idx, k)
                elif b is not None and b[0] == "c" and idx is not None and idx[0] == "c":
                    a = ("c", (b[1] + (idx[1] << k)) & 0xFFFFFFFF, b[2] | idx[2])
                else:
                    a = None
                if o.subtracted:
                    a = None
                return a
            disp = m.disp
            if b is None:
                return None
            if disp:
                return add(b, ("c", disp & 0xFFFFFFFF, frozenset([i.address])))
            return b

        width = {"ldr": 4, "str": 4, "ldrb": 1, "strb": 1, "ldrsb": 1, "ldrh": 2,
                 "strh": 2, "ldrsh": 2, "ldrd": 8, "strd": 8}
        mem_ops = [o for o in ops if o.type == ARM_OP_MEM]
        is_mem = base_mn in width
        executed = {}

        if is_mem and mem_ops:
            mo = mem_ops[0]
            m = mo.mem
            post = len(ops) == 3 and ops[2].type in (ARM_OP_IMM, ARM_OP_REG) and base_mn not in ("ldrd", "strd") \
                or (base_mn in ("ldrd", "strd") and len(ops) == 4)
            a = addr_of(mo)
            if post:
                # [rn], #imm — access at rn, then rn += imm
                a = st.get(reg(i, m.base))
            store = base_mn.startswith("str")
            self.note(f, i, a, width[base_mn], store, hits)
            dst = reg(i, ops[0].reg)
            if not store:
                v = None
                if a is not None and a[0] == "p":
                    if a[1] == "SP":
                        v = sets.get(("slot", a[2]), TOP)
                    elif a[1] == "GV" and a[2] == GV_HERO:
                        v = ("p", "HERO", 0, frozenset())
                    elif a[1] == "GOTB":
                        pass
                elif a is not None and a[0] == "c" and base_mn == "ldr":
                    w = self.img.word(a[1])
                    if m.base == ARM_REG_PC:
                        # a literal-pool word
                        v = ("c", w, frozenset([i.address])) if w is not None else None
                    elif w == self.gv:
                        # a GOT slot holding gv's address
                        v = ("p", "GV", 0, frozenset())
                executed[dst] = v
                if base_mn == "ldrd":
                    executed[reg(i, ops[1].reg)] = None
            else:
                sv = st.get(dst)
                if (sv is not None and sv[0] in ("p", "x") and sv[1] in ("BAG", "HERO")
                        and not (a is not None and a[0] == "p" and a[1] == "SP")):
                    # A bag/hero pointer stored somewhere other than this
                    # frame: whoever loads it back is invisible to this scan.
                    self.escapes.append((i.address, f.name, sv[1], sv[2], f"{i.mnemonic} {i.op_str}"))
                if a is not None and a[0] == "p" and a[1] == "SP":
                    executed[("slot", a[2])] = st.get(dst)
                    if base_mn == "strd":
                        executed[("slot", a[2] + 4)] = st.get(reg(i, ops[1].reg))
            # writeback
            if mem_ops and (i.writeback or post):
                rb = reg(i, m.base)
                if post:
                    executed[rb] = add(st.get(rb), val(ops[-1]))
                else:
                    executed[rb] = addr_of(mo)
        elif base_mn in ("mov", "mvn"):
            d = reg(i, ops[0].reg)
            v = val(ops[1])
            if base_mn == "mvn":
                v = ("c", (~v[1]) & 0xFFFFFFFF, v[2]) if v and v[0] == "c" else None
            if isinstance(v, tuple) and v[0] == "shl":
                inner, k = v[1], v[2]
                v = ("sx", inner, k) if inner is not None else None
            executed[d] = v
        elif base_mn in ("lsl", "asr", "lsr") and len(ops) == 3:
            d = reg(i, ops[0].reg)
            s = st.get(reg(i, ops[1].reg))
            k = ops[2].imm if ops[2].type == ARM_OP_IMM else None
            v = None
            if s is not None and k is not None:
                if s[0] == "c":
                    if base_mn == "lsl":
                        v = ("c", (s[1] << k) & 0xFFFFFFFF, s[2])
                    elif base_mn == "lsr":
                        v = ("c", s[1] >> k, s[2])
                    else:
                        sv = s[1] - (1 << 32) if s[1] & 0x80000000 else s[1]
                        v = ("c", (sv >> k) & 0xFFFFFFFF, s[2])
                elif base_mn == "lsl" and s[0] in ("i", "p", "x"):
                    v = ("sx", s, k)
                elif base_mn in ("asr", "lsr") and s[0] == "sx" and s[2] == k:
                    v = s[1]
            executed[d] = v
        elif base_mn in ("add", "sub") and len(ops) == 3:
            d = reg(i, ops[0].reg)
            a = st.get(reg(i, ops[1].reg)) if ops[1].reg != ARM_REG_PC else ("c", i.address + 8, frozenset())
            b = val(ops[2])
            if base_mn == "sub":
                if b is not None and b[0] == "c":
                    b = ("c", (-b[1]) & 0xFFFFFFFF, b[2])
                else:
                    b = "bad"
            if b == "bad":
                v = None
            elif isinstance(b, tuple) and b[0] == "shl":
                v = add_index(a, b[1], b[2]) if a is not None and a[0] == "p" else None
            elif isinstance(a, tuple) and a[0] == "p" and (b is None or b[0] in ("i", "sx")):
                v = add_index(a, b, 0)
            elif isinstance(a, tuple) and a[0] == "x" and (b is None or b[0] in ("i", "sx")):
                # an indexed pointer plus another unknown: still that array
                v = ("x", a[1], a[2] + (b[1] if b is not None and b[0] == "i" else 0), 1,
                     a[4] | (b[2] if b is not None and b[0] == "i" else frozenset()))
            elif isinstance(b, tuple) and b[0] == "x" and (a is None or a[0] in ("i", "sx")):
                v = ("x", b[1], b[2] + (a[1] if a is not None and a[0] == "i" else 0), 1,
                     b[4] | (a[2] if a is not None and a[0] == "i" else frozenset()))
            elif isinstance(b, tuple) and b[0] == "p" and (a is None or a[0] in ("i", "sx")):
                v = add_index(b, a, 0)
            else:
                v = add(a, b, i.address if ops[2].type == ARM_OP_IMM else None)
            executed[d] = v
        elif base_mn in ("push", "stmdb") and (base_mn == "push" or "sp!" in i.op_str):
            n = len([o for o in ops if o.type == ARM_OP_REG]) - (0 if base_mn == "push" else 1)
            executed["sp"] = add(st.get("sp"), ("c", (-4 * n) & 0xFFFFFFFF, frozenset()))
        elif base_mn in ("pop", "ldm") and (base_mn == "pop" or "sp!" in i.op_str):
            regs = [reg(i, o.reg) for o in ops if o.type == ARM_OP_REG]
            if base_mn == "ldm":
                regs = regs[1:]
            for r in regs:
                executed[r] = None
            executed["sp"] = add(st.get("sp"), ("c", 4 * len(regs), frozenset()))
        elif base_mn[:3] in ("stm", "ldm"):
            # Any other block transfer. `stmib sp, {ip, lr}` (stack arguments,
            # no writeback) used to fall into the catch-all below, which
            # clobbers the first operand — `sp` — and every spilled value
            # after it was lost (§65.11).
            regs = [reg(i, o.reg) for o in ops if o.type == ARM_OP_REG]
            rb, lst = regs[0], regs[1:]
            mode = base_mn[3:5] or "ia"
            n = len(lst)
            first = {"ia": 0, "ib": 4, "da": -4 * (n - 1), "db": -4 * n}.get(mode, 0)
            b = st.get(rb)
            for k, r in enumerate(lst):
                slot = None
                if b is not None and b[0] == "p" and b[1] == "SP":
                    slot = ("slot", b[2] + first + 4 * k)
                if base_mn.startswith("stm"):
                    if slot is not None:
                        executed[slot] = st.get(r)
                else:
                    executed[r] = sets.get(slot, TOP) if slot is not None else None
            if "!" in i.op_str:
                d = 4 * n if mode in ("ia", "ib") else -4 * n
                executed[rb] = add(b, ("c", d & 0xFFFFFFFF, frozenset()))
        elif base_mn in ("bl", "blx") or (ops and ops[0].type == ARM_OP_REG and ops[0].reg == ARM_REG_PC):
            for r in ("r0", "r1", "r2", "r3", "ip", "lr"):
                executed[r] = None
            if ops and ops[0].type == ARM_OP_IMM and ops[0].imm == self.get_bag:
                executed["r0"] = ("p", "BAG", 0, frozenset())
        else:
            # Anything else that writes a register: unknown.
            if ops and ops[0].type == ARM_OP_REG and base_mn not in ("cmp", "cmn", "tst", "teq", "b", "bx"):
                executed[reg(i, ops[0].reg)] = None
            if base_mn in ("umull", "smull", "umlal", "smlal") and len(ops) > 1:
                executed[reg(i, ops[1].reg)] = None

        if ops and ops[0].type == ARM_OP_REG and ops[0].reg == ARM_REG_PC:
            # an indirect call (`mov lr, pc; ldr pc, [...]`): the callee owns
            # the argument registers on return
            for r in ("r0", "r1", "r2", "r3", "ip", "lr"):
                executed[r] = None
        for k, v in list(executed.items()):
            if isinstance(v, tuple) and v[0] == "shl":
                executed[k] = None
        return executed, hits

    def note(self, f, i, a, w, store, hits):
        if a is None or a[0] not in ("p", "x"):
            return
        base = a[1]
        if base not in ("BAG", "HERO"):
            return
        off = a[2]
        if base == "HERO":
            off -= HERO_BAG
        scale = a[3] if a[0] == "x" else 0
        if not (-8 <= off < 0x36C + 0x20):
            return
        hits.append((i.address, f.name, base, off, scale, w, store,
                     prov(a), f"{i.mnemonic} {i.op_str}"))


def region(off, scale):
    for name, (lo, hi) in REGIONS.items():
        if lo <= off < hi:
            return name
    return "?"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("so", type=Path)
    ap.add_argument("--funcs", default=".")
    ap.add_argument("--region", default=None)
    a = ap.parse_args()
    img = Image(a.so)
    rx = re.compile(a.funcs)
    sc = Scan(img)
    text = img.elf.get_section_by_name(".text")
    lo, hi = text["sh_addr"], text["sh_addr"] + text["sh_size"]
    for addr in img.addrs:
        name, size, thumb = img.syms[addr]
        if thumb or not size or not (lo <= addr < hi) or not rx.search(name):
            continue
        try:
            sc.run(name, addr, size)
        except Exception as e:  # keep going; report
            print(f"!! {name}: {e}", file=sys.stderr)
    rows = sorted(sc.hits)
    for (adr, fn, base, off, scale, w, store, pv, txt) in rows:
        rg = region(off, scale)
        if a.region and rg != a.region:
            continue
        func_off = adr - img.by_name[fn][0]
        pvs = ",".join(f"{p:#x}" for p in sorted(pv))
        print(f"{rg:6} {adr:#08x} {fn}+{func_off:#x}  {'W' if store else 'R'}{w} "
              f"{base}+{off:#x}{f' x{scale}' if scale else ''}  [{txt}]  imm@{pvs}")
    print(f"# {len(rows)} access(es)", file=sys.stderr)
    for (adr, fn, base, off, txt) in sorted(set(sc.escapes)):
        func_off = adr - img.by_name[fn][0]
        print(f"escape {adr:#08x} {fn}+{func_off:#x}  {base}+{off:#x} stored  [{txt}]")


if __name__ == "__main__":
    main()
