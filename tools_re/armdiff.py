#!/usr/bin/env python3
"""Differential test: your ARM interpreter vs Unicorn (QEMU's ARM core).

    cargo build --release --bin armdiff          # build your helper first
    python armdiff.py --helper target/release/armdiff --n 8000 --class all

Generates random ARM instructions with random register and memory state, runs
**one** instruction through both implementations, and reports every
disagreement in registers, flags and memory.

Why you cannot skip this: an interpreter bug does not announce itself. It does
not crash, it does not assert, it returns a slightly wrong number that flows
into a pointer or a loop bound thirty thousand instructions later, and what you
see is a black screen or a missing sprite. Debugging that from the symptom is
days. Debugging it from "instruction 0xe1a03412 disagrees with the reference"
is minutes.

Two real classes of bug this catches, both of which shipped silently in a
working-looking build:

  * an encoding upstream does not decode at all, which falls through to a
    *different* pattern and executes as a no-op or as the wrong operation. The
    worst version of this is an indirect-call encoding: every call through a
    function pointer silently does nothing.
  * flag effects that are almost right — a multiply that clobbers C and V when
    the architecture says preserve, or an immediate rotate of zero that
    overwrites the carry when it should leave it alone. GCC emits those forms
    constantly, so "almost right" means "wrong everywhere".

--- helper protocol -------------------------------------------------------

Your interpreter needs a tiny binary that reads cases from stdin and writes the
resulting state to stdout. All little-endian:

    in:   u32 count
          per case: u32 instruction, 16 x u32 registers, u32 CPSR, MEM bytes
    out:  per case: 16 x u32 registers, u32 CPSR, MEM bytes

The case's code goes at address 0, the memory block at DATA (0x1000) in a
0x2000-byte flat window; execute exactly one instruction from PC = 0.
`armdiff_helper.rs` next to this script is a working implementation to copy.

Needs `pip install unicorn capstone`.
"""

from __future__ import annotations

import argparse
import random
import struct
import subprocess
import sys
from pathlib import Path

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")

try:
    from unicorn import Uc, UC_ARCH_ARM, UC_MODE_ARM, UC_PROT_ALL
    from unicorn import arm_const as _uc
except ImportError:  # pragma: no cover
    sys.exit("pip install unicorn")

# Do NOT write this as `UC_ARM_REG_R0 + i`. Unicorn's ARM register ids are not
# contiguous: R0..R12 are 66..78 but R13/R14/R15 are 12/10/11. Adding to R0
# therefore reads three unrelated registers, and the bug is invisible until you
# test an instruction that actually writes SP, LR or PC — at which point every
# `blx <reg>` case reports a false mismatch and sends you hunting in the
# interpreter for a bug that is in the harness.
UC_REGS = [getattr(_uc, f"UC_ARM_REG_R{i}") for i in range(16)]
UC_CPSR = _uc.UC_ARM_REG_CPSR
UC_PC = _uc.UC_ARM_REG_PC

try:
    from capstone import Cs, CS_ARCH_ARM, CS_MODE_ARM
    MD = Cs(CS_ARCH_ARM, CS_MODE_ARM)
except ImportError:  # pragma: no cover
    MD = None

SIZE = 0x2000
MEM = 256
DATA = 0x1000


# Each generator returns (instruction, {reg: value}). The fixups pin the base
# register (and any index register) so the access lands inside the scratch page
# and is correctly aligned. An UNPREDICTABLE encoding proves nothing: the
# reference is free to do anything with it, so a mismatch there is noise.


def gen_smul(rnd):
    """SMLA<x><y> / SMLAW<y> / SMULW<y> / SMLAL<x><y> / SMUL<x><y> — the ARMv5TE
    DSP multiplies. An ARMv4T decoder does not have these and matches them as
    data processing instead, which is the silent-garbage case."""
    op = rnd.randrange(4)
    x, y = rnd.randrange(2), rnd.randrange(2)
    rd, rs, rm = (rnd.randrange(12) for _ in range(3))
    rn = rnd.randrange(12)
    if op == 0b10:  # SMLAL<x><y>: RdHi and RdLo must differ
        while rn == rd:
            rn = rnd.randrange(12)
    inst = (0xE0000000 | (0b0001 << 24) | (op << 21) | (rd << 16) | (rn << 12)
            | (rs << 8) | (1 << 7) | (y << 6) | (x << 5) | rm)
    return inst, {}


def _addr_fixups(rnd, rn, align, rm=None):
    fix = {rn: DATA + rnd.randrange(0, 128, align)}
    if rm is not None and rm != rn:
        fix[rm] = rnd.randrange(0, 64, align)
    return fix


def gen_ldrd(rnd):
    """LDRD / STRD. Rd must be even; Rn must not overlap Rd/Rd+1 on writeback."""
    store = rnd.randrange(2)
    p, u = rnd.randrange(2), 1
    imm = rnd.randrange(2)
    rd = rnd.randrange(0, 10, 2)
    rn = rnd.choice([r for r in range(12) if r not in (rd, rd + 1)])
    w = rnd.randrange(2) if p else 0
    sh = 0b11 if store else 0b10
    base = (0xE0000000 | (p << 24) | (u << 23) | (imm << 22) | (w << 21)
            | (rn << 16) | (rd << 12))
    if imm:
        off = rnd.randrange(0, 64, 8)
        inst = base | ((off >> 4) << 8) | (1 << 7) | (sh << 5) | (1 << 4) | (off & 0xF)
        return inst, _addr_fixups(rnd, rn, 8)
    rm = rnd.choice([r for r in range(12) if r not in (rd, rd + 1, rn)])
    inst = base | (1 << 7) | (sh << 5) | (1 << 4) | rm
    return inst, _addr_fixups(rnd, rn, 8, rm)


def gen_halfword(rnd):
    """LDRH / STRH / LDRSB / LDRSH, aligned."""
    l = rnd.randrange(2)
    sh = rnd.choice([0b01, 0b10, 0b11]) if l else 0b01
    p, u, imm = rnd.randrange(2), 1, rnd.randrange(2)
    rd = rnd.randrange(12)
    rn = rnd.choice([r for r in range(12) if r != rd])
    w = rnd.randrange(2) if p else 1
    if w and rn == rd:
        w = 0
    base = (0xE0000000 | (p << 24) | (u << 23) | (imm << 22) | (w << 21) | (l << 20)
            | (rn << 16) | (rd << 12))
    if imm:
        off = rnd.randrange(0, 64, 2)
        inst = base | ((off >> 4) << 8) | (1 << 7) | (sh << 5) | (1 << 4) | (off & 0xF)
        return inst, _addr_fixups(rnd, rn, 2)
    rm = rnd.choice([r for r in range(12) if r not in (rd, rn)])
    inst = base | (1 << 7) | (sh << 5) | (1 << 4) | rm
    return inst, _addr_fixups(rnd, rn, 2, rm)


def gen_dataproc(rnd):
    """Data processing, all three operand forms, S bit on and off.

    Opcodes 8..11 with S clear are not data processing: on ARMv5 that encoding
    is the MSR/MRS space and on ARMv6T2+ it is MOVW/MOVT. Unicorn models an
    ARMv7 core and will execute them as MOVW/MOVT, so including them compares
    two architectures rather than testing anything. Skip them unless your guest
    is actually v6T2 or later."""
    form = rnd.randrange(3)
    opcode = rnd.randrange(16)
    s = rnd.randrange(2)
    if 8 <= opcode <= 11:
        s = 1
    rn, rd = rnd.randrange(12), rnd.randrange(12)
    if form == 2:
        rot, imm = rnd.randrange(16), rnd.randrange(256)
        inst = (0xE0000000 | (1 << 25) | (opcode << 21) | (s << 20) | (rn << 16)
                | (rd << 12) | (rot << 8) | imm)
        return inst, {}
    rm, st = rnd.randrange(12), rnd.randrange(4)
    if form == 0:
        inst = (0xE0000000 | (opcode << 21) | (s << 20) | (rn << 16) | (rd << 12)
                | (rnd.randrange(32) << 7) | (st << 5) | rm)
        return inst, {}
    rs = rnd.randrange(12)
    inst = (0xE0000000 | (opcode << 21) | (s << 20) | (rn << 16) | (rd << 12)
            | (rs << 8) | (st << 5) | (1 << 4) | rm)
    return inst, {}


def gen_mullong(rnd):
    u, a, s = rnd.randrange(2), rnd.randrange(2), rnd.randrange(2)
    lo, rs, rm = (rnd.randrange(12) for _ in range(3))
    hi = rnd.choice([r for r in range(12) if r != lo])
    inst = (0xE0000000 | (1 << 23) | (u << 22) | (a << 21) | (s << 20) | (hi << 16)
            | (lo << 12) | (rs << 8) | (0b1001 << 4) | rm)
    return inst, {}


def gen_mul(rnd):
    a, s = rnd.randrange(2), rnd.randrange(2)
    rd, rn, rs, rm = (rnd.randrange(12) for _ in range(4))
    while rm == rd:
        rm = rnd.randrange(12)
    inst = (0xE0000000 | (a << 21) | (s << 20) | (rd << 16) | (rn << 12) | (rs << 8)
            | (0b1001 << 4) | rm)
    return inst, {}


def gen_block(rnd):
    """LDM / STM, word-aligned base, list clear of PC and of the base on
    writeback."""
    p, u, w, l = (rnd.randrange(2) for _ in range(4))
    rn = rnd.randrange(12)
    regs = rnd.randrange(1, 0x1000)
    if w:
        regs &= ~(1 << rn) & 0xFFF
        if regs == 0:
            regs = 1 if rn != 0 else 2
    inst = (0xE0000000 | (0b100 << 25) | (p << 24) | (u << 23) | (w << 21) | (l << 20)
            | (rn << 16) | regs)
    return inst, {rn: DATA + 128}


def gen_single(rnd):
    """LDR / STR / LDRB / STRB, aligned."""
    i, p, u, b = rnd.randrange(2), rnd.randrange(2), 1, rnd.randrange(2)
    l = rnd.randrange(2)
    rd = rnd.randrange(12)
    rn = rnd.choice([r for r in range(12) if r != rd])
    w = rnd.randrange(2) if p else 1
    if w and rn == rd:
        w = 0
    align = 1 if b else 4
    base = (0xE0000000 | (0b01 << 26) | (i << 25) | (p << 24) | (u << 23) | (b << 22)
            | (w << 21) | (l << 20) | (rn << 16) | (rd << 12))
    if i:
        rm = rnd.choice([r for r in range(12) if r not in (rd, rn)])
        return base | rm, _addr_fixups(rnd, rn, align, rm)
    return base | rnd.randrange(0, 64, align), _addr_fixups(rnd, rn, align)


def gen_branchex(rnd):
    """BX / BLX <reg>. These two differ in bits 7..4 only (0001 vs 0011), and a
    decode pattern that pins bit 5 to zero catches BX and silently drops BLX.
    Every indirect call in the guest then does nothing at all."""
    blx = rnd.randrange(2)
    rm = rnd.randrange(12)
    op = 0b0011 if blx else 0b0001
    inst = 0xE12FFF00 | (op << 4) | rm
    # Keep the target in-window and ARM-aligned so both sides agree on the mode.
    return inst, {rm: rnd.randrange(0, SIZE - 8, 4)}


GENERATORS = {
    "smul": gen_smul,
    "ldrd": gen_ldrd,
    "half": gen_halfword,
    "dataproc": gen_dataproc,
    "mul": gen_mul,
    "mullong": gen_mullong,
    "block": gen_block,
    "single": gen_single,
    "branchex": gen_branchex,
}


def make_cases(n, seed, klass):
    rnd = random.Random(seed)
    gens = list(GENERATORS.values()) if klass == "all" else [GENERATORS[klass]]
    cases = []
    for _ in range(n):
        inst, fix = rnd.choice(gens)(rnd)
        regs = [rnd.randrange(1 << 32) for _ in range(15)]
        regs.append(0)
        for r, v in fix.items():
            regs[r] = v
        cpsr = 0x10 | (rnd.randrange(16) << 28)
        mem = bytes(rnd.randrange(256) for _ in range(MEM))
        cases.append((inst, regs, cpsr, mem))
    return cases


def run_ours(helper: Path, cases):
    blob = struct.pack("<I", len(cases))
    for inst, regs, cpsr, mem in cases:
        blob += struct.pack("<I", inst) + struct.pack("<16I", *regs) \
                + struct.pack("<I", cpsr) + mem
    r = subprocess.run([str(helper)], input=blob, stdout=subprocess.PIPE, check=True)
    step = 16 * 4 + 4 + MEM
    if len(r.stdout) != step * len(cases):
        sys.exit(f"helper returned {len(r.stdout)} bytes, expected "
                 f"{step * len(cases)} — check the protocol in this file's docstring")
    out = []
    for i in range(len(cases)):
        b = r.stdout[i * step:(i + 1) * step]
        out.append((list(struct.unpack("<16I", b[:64])),
                    struct.unpack("<I", b[64:68])[0], b[68:]))
    return out


def run_unicorn(cases):
    out = []
    for inst, regs, cpsr, mem in cases:
        uc = Uc(UC_ARCH_ARM, UC_MODE_ARM)
        uc.mem_map(0, SIZE, UC_PROT_ALL)
        uc.mem_write(0, struct.pack("<I", inst))
        uc.mem_write(DATA, mem)
        # CPSR first, registers second. SP and LR are banked per processor mode:
        # writing them while the core is still in its power-on mode and *then*
        # switching CPSR to User swaps in the User bank and throws the values
        # away. Every case would then report r13/r14 as zero on the reference
        # side — a mismatch in the harness that looks exactly like a bug in the
        # interpreter.
        uc.reg_write(UC_CPSR, cpsr)
        for i in range(15):
            uc.reg_write(UC_REGS[i], regs[i])
        try:
            uc.emu_start(0, SIZE, 0, 1)
        except Exception:
            out.append(None)
            continue
        r = [uc.reg_read(UC_REGS[i]) for i in range(15)]
        r.append(uc.reg_read(UC_PC))
        out.append((r, uc.reg_read(UC_CPSR), bytes(uc.mem_read(DATA, MEM))))
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--helper", default="target/release/armdiff",
                    help="your interpreter's differential harness binary")
    ap.add_argument("--n", type=int, default=20000)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--class", dest="klass", default="all",
                    choices=["all", *GENERATORS])
    ap.add_argument("--show", type=int, default=10)
    ap.add_argument("--check-pc", action="store_true",
                    help="also compare r15; off by default because pipeline "
                         "conventions differ harmlessly between cores")
    a = ap.parse_args()

    helper = Path(a.helper)
    if not helper.exists() and helper.with_suffix(".exe").exists():
        helper = helper.with_suffix(".exe")
    if not helper.exists():
        sys.exit(f"missing helper binary: {helper}\n"
                 "Build it from armdiff_helper.rs (see this file's docstring).")

    cases = make_cases(a.n, a.seed, a.klass)
    ours = run_ours(helper, cases)
    theirs = run_unicorn(cases)

    nregs = 16 if a.check_pc else 15
    bad = 0
    for i, (case, o, t) in enumerate(zip(cases, ours, theirs)):
        if t is None:
            continue  # the reference refused the encoding; nothing to compare
        inst = case[0]
        dr = [j for j in range(nregs) if o[0][j] != t[0][j]]
        dflags = (o[1] >> 28) != (t[1] >> 28)
        dmem = o[2] != t[2]
        if dr or dflags or dmem:
            bad += 1
            if bad <= a.show:
                txt = ""
                if MD:
                    d = list(MD.disasm(struct.pack("<I", inst), 0))
                    txt = f"{d[0].mnemonic} {d[0].op_str}" if d else "(undecodable)"
                print(f"\n#{i} inst={inst:#010x}  {txt}")
                print(f"  in regs = {[hex(x) for x in case[1][:15]]}  cpsr={case[2]:#x}")
                for j in dr:
                    print(f"  r{j}: ours={o[0][j]:#010x} reference={t[0][j]:#010x}")
                if dflags:
                    print(f"  NZCV: ours={o[1] >> 28:04b} reference={t[1] >> 28:04b}")
                if dmem:
                    for k in range(0, MEM, 4):
                        if o[2][k:k + 4] != t[2][k:k + 4]:
                            print(f"  mem+{k:#05x}: ours={o[2][k:k + 4].hex()} "
                                  f"reference={t[2][k:k + 4].hex()}")
    checked = sum(1 for t in theirs if t is not None)
    print(f"\n{a.klass}: {checked} comparable of {a.n}, {bad} mismatches")
    if bad:
        print("\nFix these before looking at any rendering bug. An interpreter that")
        print("disagrees with the reference on one encoding will disagree with the")
        print("original game everywhere that encoding appears.")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
