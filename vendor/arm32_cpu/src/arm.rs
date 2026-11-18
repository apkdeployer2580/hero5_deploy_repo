use log::*;

use crate::util::arm::*;
use crate::util::bit::{combine64, split64, BitUtilExt};

use crate::exception::Exception;
use crate::mode::Mode;
use crate::reg::{self, cpsr, Reg};
use crate::{Cpu, Memory};

#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(u8)]
enum Instruction {
    BranchEx, // Branch and exchange (i.e. switch to THUMB)
    Branch,
    Clz,
    DataProc0,
    DataProc1,
    DataProc2,
    PsrImm,
    PsrReg,
    Multiply,
    MulLong,
    // ★ chronosSword fork — ARMv5TE "enhanced DSP" multiplies: SMLA<x><y>,
    // SMLAW<y>/SMULW<y>, SMLAL<x><y>, SMUL<x><y>. The upstream (ARMv4T/GBA)
    // table has no entry for these, so `libchronosMian.so`'s 66 `smulbb` sites
    // silently fell through to `DataProc0` and computed garbage.
    MulHalf,
    SingleXferI, // Single data transfer, immediate offset
    SingleXferR, // Single data transfer, register offset
    HwSgnXferR,  // Halfword and signed, register offset
    HwSgnXferI,  // Halfword and signed, immediate offset
    BlockXfer,
    Swap,
    SoftwareInt,
    CoprocReg,
    Undefined,
}

const INST_MATCH_ORDER: [Instruction; 20] = [
    Instruction::Branch,
    Instruction::BranchEx,
    Instruction::Clz,
    Instruction::Swap,
    // ★ must precede DataProc0: SMUL<x><y> has bits 27..25 = 000 and bit 4 = 0,
    // which is exactly DataProc0's pattern.
    Instruction::MulHalf,
    Instruction::PsrImm,
    Instruction::PsrReg,
    Instruction::DataProc0,
    Instruction::DataProc1,
    Instruction::DataProc2,
    Instruction::Multiply,
    Instruction::MulLong,
    Instruction::SingleXferI,
    Instruction::SingleXferR,
    Instruction::HwSgnXferR,
    Instruction::HwSgnXferI,
    Instruction::BlockXfer,
    Instruction::SoftwareInt,
    Instruction::CoprocReg,
    Instruction::Undefined,
];

impl Instruction {
    #[inline]
    #[rustfmt::skip]
    const fn pattern(self) -> (u32, u32) {
        use self::Instruction::*;
        match self {
            // ★ chronosSword fork — bit 5 freed so this covers BLX <reg>
            // (bits 7..4 == 0b0011) as well as BX (0b0001). Upstream pinned bit
            // 5 to 0, so `blx ip` fell through to DataProc1, whose pattern it
            // also fits, and ran as `teq` with S=0 — a no-op that writes no
            // register and sets no flag, i.e. the indirect call silently did not
            // happen. This binary reaches it in `DM_DrawImg`/`DM_DrawImgEx`,
            // where the tail call *is* the pixel loop.
            BranchEx    => (0x0fff_ffd0, 0x012f_ff10),
            Branch      => (0x0e00_0000, 0x0a00_0000),
            Clz         => (0x0fff_0ff0, 0x016f_0f10),
            DataProc0   => (0x0e00_0010, 0x0000_0000),
            DataProc1   => (0x0e00_0090, 0x0000_0010),
            DataProc2   => (0x0e00_0000, 0x0200_0000),
            PsrImm      => (0x0fb0_0000, 0x0320_0000),
            PsrReg      => (0x0f90_0ff0, 0x0100_0000),
            Multiply    => (0x0fc0_00f0, 0x0000_0090),
            MulLong     => (0x0f80_00f0, 0x0080_0090),
            MulHalf     => (0x0f90_0090, 0x0100_0080),
            SingleXferI => (0x0e00_0000, 0x0400_0000),
            SingleXferR => (0x0e00_0010, 0x0600_0000),
            HwSgnXferR  => (0x0e40_0f90, 0x0000_0090),
            HwSgnXferI  => (0x0e40_0090, 0x0040_0090),
            BlockXfer   => (0x0e00_0000, 0x0800_0000),
            Swap        => (0x0fb0_0ff0, 0x0100_0090),
            SoftwareInt => (0x0f00_0000, 0x0f00_0000),
            CoprocReg   => (0x0f00_0010, 0x0e00_0010),
            Undefined   => (0x0000_0000, 0x0000_0000),
        }
    }

    #[inline]
    fn decode_linear(inst: u32) -> Instruction {
        for typ in INST_MATCH_ORDER.iter() {
            let (mask, test) = typ.pattern();
            if inst.mask_match(mask, test) {
                return *typ;
            }
        }
        Instruction::Undefined
    }

    /// ★ chronosSword fork — table-driven decode.
    ///
    /// The upstream decoder walks `INST_MATCH_ORDER` and does a mask-and-compare
    /// per entry, so a `BlockXfer` (entry 16 of 20) costs sixteen of them. This
    /// runs on **every guest instruction**, and the game spends around 10 M of
    /// them on a busy frame, so it lands directly on the frame budget.
    ///
    /// Every pattern's mask lives in bits 27..20 and 7..4 except five
    /// (`BranchEx`, `Clz`, `Swap`, `PsrReg`, `HwSgnXferR`, which also constrain
    /// bits 19..8). So a 4096-entry table keyed on exactly those bits answers
    /// most instructions outright; a key where one of those five could still win
    /// is marked ambiguous and falls back to the original scan. Both paths are
    /// pinned to each other by `decode_lut_matches_linear` below.
    #[inline(always)]
    fn decode(inst: u32) -> Instruction {
        let key = (((inst >> 16) & 0xff0) | ((inst >> 4) & 0xf)) as usize;
        let e = DECODE_LUT[key];
        if e == AMBIGUOUS {
            Self::decode_linear(inst)
        } else {
            // SAFETY: the table only ever stores a discriminant produced from an
            // `INST_MATCH_ORDER` entry, or `AMBIGUOUS`, which is excluded above.
            unsafe { core::mem::transmute::<u8, Instruction>(e) }
        }
    }
}

/// Bits a `DECODE_LUT` key pins down: 27..20 and 7..4.
const KEY_MASK: u32 = 0x0ff0_00f0;
/// Sentinel for "the key does not settle it; run the linear scan".
const AMBIGUOUS: u8 = 0xff;

const DECODE_LUT: [u8; 4096] = {
    let mut lut = [AMBIGUOUS; 4096];
    let mut k = 0usize;
    while k < 4096 {
        // The instruction bits this key fixes.
        let known = (((k & 0xff0) << 16) | ((k & 0xf) << 4)) as u32;
        let mut i = 0usize;
        while i < INST_MATCH_ORDER.len() {
            let (mask, test) = INST_MATCH_ORDER[i].pattern();
            // Could an instruction with this key match entry `i`?
            if (test & mask & KEY_MASK) == (known & mask & KEY_MASK) {
                if mask & !KEY_MASK == 0 {
                    // Entry `i` constrains nothing outside the key, so a match
                    // here is certain and no later entry can be reached.
                    lut[k] = INST_MATCH_ORDER[i] as u8;
                } // else: leave AMBIGUOUS — whether it matches depends on bits
                  // the key does not carry.
                break;
            }
            i += 1;
        }
        k += 1;
    }
    lut
};

impl Cpu {
    /// Executes one instruction and returns whether the CPU should continue
    /// executing.
    pub(crate) fn execute_arm(&mut self, mmu: &mut impl Memory) -> bool {
        let pc = self.reg[reg::PC];
        let inst = mmu.r32(pc);
        let inst_type = self::Instruction::decode(inst);

        let cond = inst.extract(28, 4);
        let cpsr = self.reg[reg::CPSR];

        // NB: gated by a cargo feature, not by the log level. `trace!` expands to
        // a runtime `log::max_level()` atomic load + branch that stays in release
        // builds, and it pins `pc`/`inst`/`inst_type` live for a format path that
        // is never taken — on the emulator's hottest loop that is pure overhead.
        // See `insn_trace` in Cargo.toml.
        #[cfg(all(feature = "insn_trace", not(feature = "advanced_disasm")))]
        {
            let cflags = cpsr.extract(28, 4);
            trace!(
                "ARM: pc: {:#010x}, inst: {:#010x}, cond: {:#03x}, cflags: {:04b}",
                pc,
                inst,
                cond,
                cflags
            );
            // NB: reuse the `inst_type` decoded above — do not re-run the
            // ~19-branch linear decode just to format a trace line. This block
            // executes on every ARM instruction; the redundant decode was pure
            // overhead on the emulator's hot path.
            trace!("Instruction: {:?}", inst_type);
        }
        #[cfg(feature = "advanced_disasm")]
        {
            if log_enabled!(log::Level::Trace) {
                let cs = self.cs.as_mut().unwrap();
                cs.set_mode(capstone::Mode::Arm).unwrap();
                if let Ok(inst) = cs.disasm_count(&inst.to_le_bytes(), pc as u64, 1) {
                    let s = format!("{}", inst);
                    trace!("{}", s.trim());
                } else {
                    trace!("failed to disasm instruction");
                }
            }
        }

        self.reg[reg::PC] = self.reg[reg::PC].wrapping_add(4);

        // ★ chronosSword fork — `cond == 0b1111` is not a condition on ARMv5; it
        // opens the unconditional-instruction space. `cond_met` returns true for
        // it, which is right for the one encoding this binary reaches (BLX <imm>,
        // bits 27..25 == 0b101) and wrong for every other: PLD would run as an
        // LDR into PC. Retire the rest as NOPs.
        if cond == 0b1111 && inst.extract(25, 3) != 0b101 {
            return true;
        }

        if !cond_met(cond, cpsr) {
            // Same reasoning as the prologue tracer, and this one was NOT covered
            // by `advanced_disasm`: it sat on the predicated-branch path, which is
            // one of the most frequently taken paths in ARM code.
            #[cfg(feature = "insn_trace")]
            trace!("cond not met");
            return true;
        }

        use self::Instruction::*;
        match inst_type {
            BranchEx => {
                let rn = inst.extract(0, 4) as Reg;
                let new_pc = self.reg[rn];
                // ★ chronosSword fork — BLX <reg> links, BX does not. Read the
                // target first: `blx lr` is legal and would otherwise branch to
                // the return address it just wrote. PC is already advanced past
                // this instruction, so it *is* the return address.
                if inst.get_bit(5) != 0 {
                    self.reg[reg::LR] = self.reg[reg::PC];
                }
                self.reg[reg::PC] = new_pc & !1u32;
                // maybe switch to thumb mode
                self.reg[reg::CPSR] |= new_pc.get_bit(0) << cpsr::T;
            }
            Branch => {
                let offset = inst.extract(0, 24);
                // Shift up to sign extend
                // shift right by 6 (instead of 8) to multiply by 4
                let s_offset = ((offset << 8) as i32 >> 6) as u32;

                if cond == 0b1111 {
                    // blx
                    let h = inst.get_bit(24);

                    let s_offset = s_offset | (h << 1);

                    self.reg[reg::PC] = pc.wrapping_add(s_offset).wrapping_add(8);
                    self.reg[reg::LR] = pc.wrapping_add(4);
                    self.reg[reg::CPSR] |= 1 << cpsr::T;
                } else {
                    let l = inst.get_bit(24);

                    self.reg[reg::PC] = pc.wrapping_add(s_offset).wrapping_add(8);
                    if l != 0 {
                        self.reg[reg::LR] = pc.wrapping_add(4);
                    }
                }
            }
            Clz => {
                let rm = inst.extract(0, 4) as Reg;
                let rd = inst.extract(12, 4) as Reg;

                if rm == reg::PC || rd == reg::PC {
                    return false;
                }

                self.reg[rd] = self.reg[rm].leading_zeros();
            }
            DataProc0 | DataProc1 | DataProc2 => {
                let i = inst.get_bit(25);
                let s = inst.get_bit(20);
                let r = inst.get_bit(4);

                let c = cpsr.get_bit(cpsr::C);
                let v = cpsr.get_bit(cpsr::V);

                let opcode = inst.extract(21, 4);

                let rn = inst.extract(16, 4) as Reg;
                let rd = inst.extract(12, 4) as Reg;

                // ★ heroesLore5 fork — ARMv6T2 MOVW/MOVT. TST/TEQ/CMP/CMN
                // (opcodes 0x8..0xB) only exist with S=1, so the immediate
                // form's S=0 space encodes other instructions: MOVW (0x8),
                // MOVT (0xA), and MSR (0x9/0xB — claimed by PsrImm earlier in
                // the match order, so it never reaches here). Upstream ran
                // `movw r6, #0x1388` as a flagless TST — a no-op that leaves
                // rd holding whatever the caller left there. This binary has
                // exactly one MOVW (`NETWORK::CheckServerConnectLimitTime
                // +0x2c`, the 5,000 ms request deadline) and zero MOVTs; MOVT
                // is covered because it is the same decode hole.
                if inst_type == DataProc2 && s == 0 && (opcode == 0x8 || opcode == 0xA) {
                    let imm16 = (inst.extract(16, 4) << 12) | inst.extract(0, 12);
                    self.reg[rd] = if opcode == 0x8 {
                        imm16
                    } else {
                        (self.reg[rd] & 0xffff) | (imm16 << 16)
                    };
                    return true;
                }

                // Three modes:
                let (valm, shift_carry) = if inst_type == DataProc0 || inst_type == DataProc1 {
                    debug_assert!((r == 1) == (inst_type == DataProc1));
                    let rm = inst.extract(0, 4) as Reg;
                    let shift_type = inst.extract(5, 2);

                    let shift = if inst_type == DataProc0 {
                        inst.extract(7, 5)
                    } else {
                        // ★ chronosSword fork — upstream asserted `rs <= 14`.
                        // A register-shift amount of R15 is UNPREDICTABLE in the
                        // ARM ARM, not impossible, and this game's binary really
                        // does execute one: the assert fires part-way through the
                        // intro on a debug build while release (assertions off)
                        // has always run past it. Reading PC is what an ARM926
                        // does, only the low byte is used, and turning a debug
                        // build into a crash where the shipped build is fine is
                        // strictly worse than emulating it.
                        let rs = inst.extract(8, 4) as Reg;
                        self.reg[rs] & 0xffu32
                    };

                    let valm = self.reg[rm].wrapping_add(((rm == reg::PC) as u32) * (4 + 4 * r));

                    if r == 0 && shift == 0 {
                        arg_shift0(valm, shift_type, c)
                    } else if shift != 0 {
                        arg_shift(valm, shift, shift_type)
                    } else {
                        // [Rs] == 0, so we do nothing
                        (valm, c)
                    }
                } else {
                    // ★ chronosSword fork — a rotate of zero leaves the carry
                    // ALONE; only a non-zero rotate produces a shifter carry-out
                    // (bit 31 of the rotated immediate). Upstream took
                    // `shift_ror(0)`'s carry unconditionally, so every
                    // `ands`/`bics`/`tst`/`movs` against a plain 8-bit immediate
                    // — the single most common flag-setting form GCC emits —
                    // silently cleared C. Anything that then read the carry (an
                    // `adc` chain, a `bcs`) took the wrong path.
                    // Found by tools_re/armdiff.py against QEMU.
                    let shift = inst.extract(8, 4) * 2;
                    let imm = inst.extract(0, 8);
                    if shift == 0 {
                        (imm, c)
                    } else {
                        imm.shift_ror(shift)
                    }
                };

                let valn = self.reg[rn].wrapping_add(if rn == reg::PC {
                    4 + 4 * (i == 0 && r == 1) as u32
                } else {
                    0
                });

                // execute the instruction now
                let (res, new_v, new_c) = match opcode {
                    0x0 /* AND */ |
                    0x8 /* TST */ => (valn & valm, v, shift_carry),
                    0x1 /* EOR */ |
                    0x9 /* TEQ */ => (valn ^ valm, v, shift_carry),
                    0x2 /* SUB */ |
                    0xA /* CMP */ => valn.sub_flags(valm, 0),
                    0x3 /* RSB */ => valm.sub_flags(valn, 0),
                    0x4 /* ADD */ |
                    0xB /* CMN */ => valn.add_flags(valm, 0),
                    0x5 /* ADC */ => valn.add_flags(valm, c),
                    0x6 /* SBC */ => valn.sub_flags(valm, 1-c),
                    0x7 /* RSC */ => valm.sub_flags(valn, 1-c),
                    0xC /* ORR */ => (valn | valm, v, shift_carry),
                    0xD /* MOV */ => (valm, v, shift_carry),
                    0xE /* BIC */ => (valn & !valm, v, shift_carry),
                    0xF /* MVN */ => (!valm, v, shift_carry),
                    _ => unreachable!(),
                };

                if s == 1 {
                    if rd != reg::PC {
                        let new_z = (res == 0) as u32;
                        let new_n = res.is_neg() as u32;
                        let new_flags = build_flags(new_v, new_c, new_z, new_n);
                        self.reg[reg::CPSR] = self.reg[reg::CPSR].set_bit(28, 4, new_flags);
                    } else {
                        self.reg[reg::CPSR] = self.reg[reg::SPSR];
                        self.reg.update_bank();
                    }
                }

                match opcode {
                    0x8 | 0x9 | 0xA | 0xB => (), // no writeback
                    _ => self.reg[rd] = res,
                }
            }
            PsrImm | PsrReg => {
                let p = inst.get_bit(22);
                let rs = if p == 0 { reg::CPSR } else { reg::SPSR };

                let op = inst.get_bit(21);

                if op == 0 {
                    // Move psr to rd
                    let rd = inst.extract(12, 4) as Reg;
                    self.reg[rd] = self.reg[rs];
                } else {
                    let i = inst.get_bit(25);
                    let f = inst.get_bit(19);
                    let c = inst.get_bit(16);

                    // user mode can't change the control bits
                    let ctrl = ((self.reg.mode() != Mode::User) as u32) * c;

                    let mask = 0xf000_0000 * f + 0x0000_00ff * ctrl;

                    let val = if i == 0 {
                        let rm = inst.extract(0, 4) as Reg;
                        self.reg[rm]
                    } else {
                        let rot = inst.extract(8, 4) * 2;
                        let imm = inst.extract(0, 8);
                        imm.rotate_right(rot)
                    };

                    let cur = self.reg[rs];
                    self.reg[rs] = (cur & !mask) | val & mask;
                    if rs == reg::CPSR {
                        self.reg.update_bank();
                    }
                };
            }
            Multiply => {
                let a = inst.get_bit(21);
                let s = inst.get_bit(20);

                let rd = inst.extract(16, 4) as Reg;
                let rn = inst.extract(12, 4) as Reg;
                let rs = inst.extract(8, 4) as Reg;
                let rm = inst.extract(0, 4) as Reg;

                let res = self.reg[rm]
                    .wrapping_mul(self.reg[rs])
                    .wrapping_add(if a == 0 { 0 } else { self.reg[rn] });

                self.reg[rd] = res;

                if s == 1 {
                    // ★ chronosSword fork — C and V are UNAFFECTED by MUL/MLA on
                    // ARMv5 (ARMv4 left C UNPREDICTABLE, which is what upstream
                    // encoded by forcing it to 0). Clearing C is not harmless:
                    // GCC emits `muls` and then branches on the flags it did not
                    // write, so a cleared carry silently takes the wrong branch.
                    // Found by tools_re/armdiff.py against QEMU.
                    let v = cpsr.get_bit(cpsr::V);
                    let c = cpsr.get_bit(cpsr::C);
                    let new_z = (res == 0) as u32;
                    let new_n = res.get_bit(31);
                    let new_flags = build_flags(v, c, new_z, new_n);

                    self.reg[reg::CPSR] = self.reg[reg::CPSR].set_bit(28, 4, new_flags);
                }
            }
            // ★ chronosSword fork — ARMv5TE signed 16x16 multiplies.
            //
            //   cond 0001 0oo0 Rd Rn Rs 1 y x 0 Rm
            //
            // `x` picks the half of Rm, `y` the half of Rs (0 = bottom, 1 = top),
            // and `oo` picks the form. Q-flag saturation is not modelled: the game
            // never reads CPSR[27], and the operands it feeds these (tile indices
            // and fixed-point deltas) cannot overflow a 32-bit accumulator.
            MulHalf => {
                let op = inst.extract(21, 2);
                let x = inst.get_bit(5);
                let y = inst.get_bit(6);

                let rd = inst.extract(16, 4) as Reg;
                let rn = inst.extract(12, 4) as Reg;
                let rs = inst.extract(8, 4) as Reg;
                let rm = inst.extract(0, 4) as Reg;

                // `y` selects the Rs half for every form; `x` selects the Rm half
                // for all but SMLAW/SMULW, where it selects the form instead.
                let half = |v: u32, top: u32| -> i32 {
                    if top == 1 {
                        (v >> 16) as u16 as i16 as i32
                    } else {
                        v as u16 as i16 as i32
                    }
                };
                let vs = half(self.reg[rs], y);

                match op {
                    0b00 => {
                        // SMLA<x><y>: Rd = Rm.x * Rs.y + Rn
                        let prod = half(self.reg[rm], x).wrapping_mul(vs) as u32;
                        self.reg[rd] = prod.wrapping_add(self.reg[rn]);
                    }
                    0b01 => {
                        // SMLAW<y> (x==0) / SMULW<y> (x==1): Rd = (Rm * Rs.y) >> 16
                        let prod = (((self.reg[rm] as i32) as i64) * (vs as i64) >> 16) as u32;
                        self.reg[rd] = if x == 0 {
                            prod.wrapping_add(self.reg[rn])
                        } else {
                            prod
                        };
                    }
                    0b10 => {
                        // SMLAL<x><y>: RdHi:Rn += Rm.x * Rs.y  (Rn is RdLo here)
                        let prod = (half(self.reg[rm], x).wrapping_mul(vs) as i32) as i64;
                        let acc = combine64(self.reg[rd], self.reg[rn]) as i64;
                        let (hi, lo) = split64(acc.wrapping_add(prod) as u64);
                        self.reg[rd] = hi;
                        self.reg[rn] = lo;
                    }
                    _ => {
                        // SMUL<x><y>: Rd = Rm.x * Rs.y
                        self.reg[rd] = half(self.reg[rm], x).wrapping_mul(vs) as u32;
                    }
                }
            }
            MulLong => {
                let u = inst.get_bit(22);
                let a = inst.get_bit(21);
                let s = inst.get_bit(20);

                let rdhi = inst.extract(16, 4) as Reg;
                let rdlo = inst.extract(12, 4) as Reg;
                let rs = inst.extract(8, 4) as Reg;
                let rm = inst.extract(0, 4) as Reg;

                let res: u64 = if u == 0 {
                    let vs = self.reg[rs];
                    let vm = self.reg[rm];

                    let prod = (vs as u64) * (vm as u64);
                    prod.wrapping_add(if a == 0 {
                        0u64
                    } else {
                        combine64(self.reg[rdhi], self.reg[rdlo])
                    })
                } else {
                    let vs = self.reg[rs] as i32;
                    let vm = self.reg[rm] as i32;

                    let prod = (vs as i64) * (vm as i64);
                    prod.wrapping_add(if a == 0 {
                        0i64
                    } else {
                        combine64(self.reg[rdhi], self.reg[rdlo]) as i64
                    }) as u64
                };

                let (reshi, reslo) = split64(res);
                self.reg[rdhi] = reshi;
                self.reg[rdlo] = reslo;

                if s != 0 {
                    // ★ chronosSword fork — same as MUL above: the long
                    // multiplies set N and Z from the 64-bit result and leave C
                    // and V alone. Upstream cleared both.
                    let v = cpsr.get_bit(cpsr::V);
                    let c = cpsr.get_bit(cpsr::C);
                    let new_z = (res == 0) as u32;
                    let new_n = reshi.get_bit(31);
                    let new_flags = build_flags(v, c, new_z, new_n);

                    self.reg[reg::CPSR] = self.reg[reg::CPSR].set_bit(28, 4, new_flags);
                }
            }
            SingleXferI | SingleXferR => {
                let p = inst.get_bit(24);
                let u = inst.get_bit(23);
                let b = inst.get_bit(22);
                let w = inst.get_bit(21);
                let l = inst.get_bit(20);

                let rn = inst.extract(16, 4) as Reg;
                let rd = inst.extract(12, 4) as Reg;

                let offset = if inst_type == SingleXferI {
                    inst.extract(0, 12)
                } else {
                    // We use the same logic here as for DataProc0
                    let shift = inst.extract(7, 5);
                    let shift_type = inst.extract(5, 2);

                    let rm = inst.extract(0, 4) as Reg;

                    let valm = self.reg[rm];

                    let (shifted, _) = if shift == 0 {
                        let c = cpsr.get_bit(cpsr::C);
                        arg_shift0(valm, shift_type, c)
                    } else {
                        arg_shift(valm, shift, shift_type)
                    };
                    shifted
                };

                let base = self.reg[rn].wrapping_add(((rn == reg::PC) as u32) * 4);
                let post_addr = if u == 0 {
                    base.wrapping_sub(offset)
                } else {
                    base.wrapping_add(offset)
                };

                let addr = if p == 0 { base } else { post_addr };

                if l == 0 {
                    // store
                    let val = self.reg[rd].wrapping_add(((rd == reg::PC) as u32) * 8);
                    if b == 0 {
                        // force alignment of the store
                        mmu.w32(addr, val);
                    } else {
                        mmu.w8(addr, val as u8);
                    };
                } else {
                    self.reg[rd] = if b == 0 {
                        mmu.r32(addr)
                    } else {
                        mmu.r8(addr) as u32
                    };
                };

                // post-indexing implies writeback
                // make sure we don't overwrite rd if it was a load
                if (p == 0 || w == 1) && (rd != rn || l == 0) {
                    self.reg[rn] = post_addr;
                }
            }
            HwSgnXferR | HwSgnXferI => {
                let p = inst.get_bit(24);
                let u = inst.get_bit(23);
                let w = inst.get_bit(21);
                let l = inst.get_bit(20);

                let s = inst.get_bit(6);
                let h = inst.get_bit(5);

                let rn = inst.extract(16, 4) as Reg;
                let rd = inst.extract(12, 4) as Reg;

                let offset = if inst_type == HwSgnXferR {
                    let rn = inst.extract(0, 4) as Reg;
                    self.reg[rn]
                } else {
                    (inst.extract(8, 4) << 4) | inst.extract(0, 4)
                };

                let base = self.reg[rn].wrapping_add(((rn == reg::PC) as u32) * 4);
                let post_addr = if u == 0 {
                    base.wrapping_sub(offset)
                } else {
                    base.wrapping_add(offset)
                };

                let addr = if p == 0 { base } else { post_addr };

                // ★ chronosSword fork — on ARMv5TE the `L == 0, S == 1` corner of
                // this encoding is LDRD (H == 0) / STRD (H == 1), not a store.
                // Upstream targets ARMv4T, where that corner does not exist, and
                // asserted it away. GCC 4.4 emits ~290 of these in this binary.
                let is_dword = l == 0 && s == 1;

                if is_dword {
                    // Rd is always even and the pair is Rd, Rd+1.
                    let addr = addr & !3;
                    if h == 0 {
                        // LDRD
                        let lo = mmu.r32(addr);
                        let hi = mmu.r32(addr.wrapping_add(4));
                        self.reg[rd] = lo;
                        self.reg[rd + 1] = hi;
                    } else {
                        // STRD
                        mmu.w32(addr, self.reg[rd]);
                        mmu.w32(addr.wrapping_add(4), self.reg[rd + 1]);
                    }
                } else if l == 0 {
                    // store
                    debug_assert!(s == 0 && h == 1);
                    let val = self.reg[rd].wrapping_add(((rd == reg::PC) as u32) * 8);
                    mmu.w16(addr & !1, val as u16);
                } else {
                    self.reg[rd] = match (s, h) {
                        (0, 0) /* SWP */ => unreachable!(),
                        (0, 1) /* halfword load */  => mmu.r16(addr & !1) as u32,
                        (1, 0) /* signed byte */    => mmu.r8(addr) as i8 as u32,
                        (1, 1) /* signed half */    => mmu.r16(addr & !1) as i16 as u32,
                        _ => unreachable!()
                    };
                };

                // post-indexing implies writeback
                // make sure we don't overwrite rd if it was a load
                // ★ chronosSword fork — LDRD has `l == 0` but still loads, into
                // Rd *and* Rd+1, so the plain `l == 0` escape hatch would let the
                // writeback clobber a just-loaded base register.
                let wrote_rn = if is_dword {
                    h == 0 && (rn == rd || rn == rd + 1)
                } else {
                    l == 1 && rd == rn
                };
                if (p == 0 || w == 1) && !wrote_rn {
                    self.reg[rn] = post_addr;
                }
            }
            BlockXfer => {
                let p = inst.get_bit(24);
                let u = inst.get_bit(23);
                let s = inst.get_bit(22);
                let w = inst.get_bit(21);
                let l = inst.get_bit(20);

                let rn = inst.extract(16, 4) as Reg;

                let reglist = inst.extract(0, 16);

                // FIXME: implement S bit correctly
                // FIXME: if reglist is empty apparently theres weird behaviour
                //        ignore this for now
                let total = reglist.count_ones();

                let orig_base = self.reg[rn];
                let base = orig_base;

                let post_addr = if u == 0 {
                    base.wrapping_sub(total * 4)
                } else {
                    base.wrapping_add(total * 4)
                };

                let addr = if u == 0 { post_addr } else { base };

                // If we are going up, and pre-incrementing,
                // or going down, and post-decrementing,
                // then we will be using the range [addr+4, addr+total*4+4]
                let pre_incr = (p == u) as u32;

                let mut rem = reglist;
                if s == 0 || (rem & (1 << reg::PC)) != 0 {
                    if w == 1 {
                        self.reg[rn] = post_addr;
                    }

                    for i in 0..16 {
                        if rem == 0 {
                            break;
                        }
                        let r = rem.trailing_zeros() as Reg;
                        let idx_addr = addr.wrapping_add((i + pre_incr) * 4);
                        if l == 0 {
                            // store
                            let val = if r == reg::PC {
                                pc.wrapping_add(12)
                            } else if r == rn && w == 1 && i == 0 {
                                orig_base
                            } else {
                                self.reg[r]
                            };
                            mmu.w32(idx_addr, val);
                        } else {
                            // load
                            self.reg[r] = mmu.r32(idx_addr);
                            if r == reg::PC && s == 1 {
                                self.reg[reg::CPSR] = self.reg[reg::SPSR];
                                self.reg.update_bank();
                            }
                        };
                        rem -= 1u32 << r;
                    }
                } else {
                    for i in 0..16 {
                        if rem == 0 {
                            break;
                        }
                        let r = rem.trailing_zeros() as Reg;
                        let idx_addr = addr.wrapping_add((i + pre_incr) * 4);
                        if l == 0 {
                            // store
                            let val = self.reg.get(0, r);
                            mmu.w32(idx_addr, val);
                        } else {
                            // load
                            let val = mmu.r32(idx_addr);
                            self.reg.set(0, r, val);
                        };
                        rem -= 1u32 << r;
                    }
                }
            }
            Swap => {
                let b = inst.get_bit(22);

                let rn = inst.extract(16, 4) as Reg;
                let rd = inst.extract(12, 4) as Reg;
                let rm = inst.extract(0, 4) as Reg;

                // If it is not a byte operation then force word align
                let addr = self.reg[rn] & !((1 - b) * 3);

                let val = match b {
                    0 => mmu.r32(addr),
                    1 => mmu.r8(addr) as u32,
                    _ => unreachable!(),
                };
                let oval = self.reg[rm];
                match b {
                    0 => mmu.w32(addr, oval),
                    1 => mmu.w8(addr, oval as u8),
                    _ => unreachable!(),
                };

                self.reg[rd] = val;
            }
            SoftwareInt => {
                self.exception(Exception::Software);
            }
            CoprocReg => {
                let cpopc = inst.extract(21, 3);
                let d = inst.get_bit(20);
                let cn = inst.extract(16, 4);
                let rd = inst.extract(12, 4) as Reg;
                let pn = inst.extract(8, 4);
                let cpinf = inst.extract(5, 3);
                let cm = inst.extract(0, 4);

                if d == 0 {
                    info!(
                        "Writing {:010x?} to P{},C{},C{},{}, cpinfo {}, mode: {:?}",
                        self.reg[rd],
                        pn,
                        cn,
                        cm,
                        cpopc,
                        cpinf,
                        self.mode()
                    );
                } else {
                    info!(
                        "Reading to R{} from P{},C{},C{},{}, cpinfo {}, mode: {:?}",
                        rd,
                        pn,
                        cn,
                        cm,
                        cpopc,
                        cpinf,
                        self.mode()
                    );
                    self.reg[rd] = 0;
                }
            }
            Undefined => return false,
        };

        true
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// The table-driven decoder must agree with the scan it replaces for every
    /// possible instruction word. 2^32 is too many to enumerate, so this sweeps
    /// every decode-relevant bit (31..4) and pins the low nibble at both ends —
    /// the only bits no pattern in the table looks at.
    #[test]
    fn decode_lut_matches_linear() {
        for hi in 0u32..(1 << 28) {
            let inst = hi << 4;
            assert_eq!(
                Instruction::decode(inst),
                Instruction::decode_linear(inst),
                "decode mismatch at {inst:#010x}"
            );
            let inst = (hi << 4) | 0xf;
            assert_eq!(
                Instruction::decode(inst),
                Instruction::decode_linear(inst),
                "decode mismatch at {inst:#010x}"
            );
        }
    }

    #[test]
    #[rustfmt::skip]
    fn test_decode() {
        use super::Instruction::*;

        macro_rules! check (
            ($inst: expr, $val: expr) => {
                assert_eq!($inst, Instruction::decode($val));
            }
        );
        check!(BranchEx,    0xE12F_FF1C);
        // BLX <reg>: same encoding with bits 7..4 == 0b0011.
        check!(BranchEx,    0xE12F_FF3C);
        check!(BranchEx,    0x112F_FF30);
        check!(Branch,      0xEB00_00F8);
        check!(Clz,         0xE16F_6F12);
        check!(DataProc0,   0xE1A0_816C);
        check!(DataProc1,   0xE092_3011);
        check!(DataProc1,   0xC092_3011);
        check!(DataProc2,   0xE2A2_3AFF);
        check!(PsrImm,      0x1329_F000);
        check!(PsrReg,      0xE10F_A000);
        check!(Multiply,    0x8004_0393);
        check!(MulLong,     0xE083_4192);
        check!(SingleXferI, 0x85C6_7011);
        check!(SingleXferR, 0xB796_5100);
        check!(HwSgnXferR,  0xE10B_00B1);
        check!(HwSgnXferI,  0xE1EB_10B4);
        check!(BlockXfer,   0xE8BF_8006);
        check!(Swap,        0xE10D_1090);
        check!(SoftwareInt, 0xEF00_0000);
        check!(Undefined,   0xE7DE_AD10);
    }

    /// ★ heroesLore5 fork — MOVW/MOVT execute as 16-bit immediate moves, not
    /// as the flagless TST/CMP their opcode bits alias in the S=0 space.
    #[test]
    fn movw_movt() {
        use crate::ExampleMem;

        // movw r6, #0x1388 ; movt r6, #0xabcd
        let prog: Vec<u8> = [0xE301_6388u32, 0xE34A_6BCD]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        let mut mmu = ExampleMem::new_with_data(&prog);
        let mut cpu = Cpu::new();
        cpu.reg_set(Mode::User, reg::PC, 0x00);
        cpu.reg_set(Mode::User, reg::CPSR, 0x10);
        cpu.reg_set(Mode::User, 6, 0xdead_beef); // caller garbage MOVW must replace

        assert!(cpu.step(&mut mmu));
        assert_eq!(cpu.reg_get(Mode::User, 6), 0x0000_1388, "movw writes rd");
        assert!(cpu.step(&mut mmu));
        assert_eq!(cpu.reg_get(Mode::User, 6), 0xabcd_1388, "movt keeps the low half");
        assert_eq!(cpu.reg_get(Mode::User, reg::CPSR), 0x10, "flags untouched");
    }

    macro_rules! emutest {
        ($name:ident, $mem_checks: expr) => {
            #[test]
            fn $name() {
                use crate::ExampleMem;

                let prog = include_bytes!(concat!("../tests/data/", stringify!($name), ".bin"));
                let mut mmu = ExampleMem::new_with_data(prog);
                let mut cpu = Cpu::new();
                cpu.reg_set(Mode::User, reg::PC, 0x00);
                cpu.reg_set(Mode::User, reg::CPSR, 0x10);

                while cpu.step(&mut mmu) {}

                for &(addr, val) in ($mem_checks).iter() {
                    assert_eq!(val, mmu.r32(addr), "addr: {:#010x}", addr);
                }
            }
        };
    }

    emutest!(emutest_arm0, [(0x100, 5), (0x104, 0)]);
    emutest!(emutest_arm1, [(0x100, 5), (0x104, 5), (0x108, 5)]);
    emutest!(
        emutest_arm2,
        [(0x100, 6), (0x104, 0x2000_00e1), (0x108, 0xe100_001c)]
    );
    emutest!(emutest_arm3, [(0x100, 64)]);
    emutest!(
        emutest_arm4,
        [
            (0x100, 6),
            (0x104, 0x2000_00e1),
            (0x108, 0xe100_001c),
            (0x10c, 6),
            (0x110, 6 * 0x100),
        ]
    );
    emutest!(
        emutest_arm5,
        [(0x100, 0xf000), (0x104, 0xfff0), (0x108, 0x104)]
    );
    emutest!(
        emutest_arm6,
        [
            (0x1f4, 0xa),
            (0x1f8, 0xc),
            (0x1fc, 0x10),
            (0x200, 6),
            (0x204, 0x200),
        ]
    );
    emutest!(emutest_arm7, [(0x1fc, 1), (0x200, 1), (0x204, 0x200)]);
    emutest!(emutest_arm8, [(0x200, 10), (0x204, 83)]);
    emutest!(emutest_arm9, [(0x200, 55), (0x204, 66), (0x208, 77)]);
    emutest!(emutest_arm10, [(0x100, 24)]);
}
