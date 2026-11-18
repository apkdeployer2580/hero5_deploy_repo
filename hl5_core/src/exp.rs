//! The 슈퍼 성장의 서 — three times experience, built on the guest's own +50%.
//!
//! # The original +50% works. An earlier session said it did not.
//!
//! `ANALYSIS.md` §42.8 and the CLAUDE.md task table both recorded that the
//! 성장의 서 bonus was dead code, on the reading that
//! `Monster::SetDieMotion+0x84` gates it with
//!
//! ```text
//!   0xc0704  bl  HERO::GetExpGoodTime      ; r0 = lo, r1 = hi of a 64-bit ms
//!   0xc0708  cmp r1, #0
//!   0xc070c  ble #0xc08f4                  ; "always taken: 3 h has hi == 0"
//! ```
//!
//! That is wrong, and the player's playtest that said so is right. `0xc08f4`
//! is not a skip — it is the **cold half of a split 64-bit comparison**, and it
//! branches *back* into the bonus:
//!
//! ```text
//!   0xc08f4  bne #0xc0900                  ; hi < 0  -> expired, really skip
//!   0xc08f8  cmp r0, #0
//!   0xc08fc  bne #0xc0710                  ; hi == 0 && lo != 0 -> APPLY
//! ```
//!
//! GCC hoists the second half of a `long long > 0` test out of the hot path and
//! lets it jump back. Only `hi == 0 && lo == 0` (and `hi < 0`) skip. The bonus
//! at `0xc0724` — `add r3, r3, sl, asr #1` — therefore fires for the whole
//! three hours, and the spirit-experience pair at `0xc0764` / `0xc0914` is
//! built the same way and equally alive.
//!
//! The practical consequence is that there is nothing to *fix* about the
//! original book. Only the super book is new work.
//!
//! # The lever
//!
//! One hook at `0xc06ec`, which is the instruction *after*
//!
//! ```text
//!   0xc06e8  bl  Formula::calc              ; r0 = base exp for this kill
//!   0xc06ec  ldr r3, [r8, r7]               ; <- the hook sits here
//!   0xc06f0  mov sl, r0                     ; sl = the value we may have changed
//! ```
//!
//! The hook multiplies `r0` and returns `false`, so the guest's own next
//! instruction runs and every downstream user — the award at `0xc0700`, the
//! `asr #1` bonus at `0xc0724`, the spirit award at `0xc075c` — sees the scaled
//! value with no further help from us.
//!
//! Four other levers were considered and rejected:
//!
//! * **Hook `Formula::calc` with a return-address filter.** 264 call sites, and
//!   changing a *return* value means re-entering the guest through [`Vm::call`]
//!   with six arguments rebuilt, two of them on the stack. All to reach a value
//!   that is already sitting in `r0` one instruction later.
//! * **Shadow the formula table.** `Formula::dataLoad` (0x7583c) reads
//!   `/c/calc/calc_en.dat` through `StaticUtil::LoadResDecrypt`, so a shadow
//!   file would have to be DES-encrypted with the guest's key — the trap
//!   CLAUDE.md flags for save files. It also gives no runtime control, so the
//!   factor could not depend on which book is active.
//! * **Lie about `GetExpGoodTime`.** It gates a *fixed* `asr #1`; no return
//!   value produces an arbitrary factor.
//! * **Hook the bonus itself at `0xc0724`.** It would need `sl` zeroed, and `sl`
//!   is reused for the spirit award at `0xc0788` — zeroing it breaks spirit
//!   experience.
//!
//! # Why the factor is 2 and not 3
//!
//! The guest adds `floor(base/2)` on top of whatever base it was given. So our
//! factor `F` produces `F × 1.5` in total while a book is running, and the
//! arithmetic is exact with no rounding to argue about:
//!
//! | state | `F` | total |
//! |---|---|---|
//! | no book | 1 | 1.0x |
//! | 성장의 서 | 1 | **1.5x** (shipped behaviour, untouched) |
//! | 슈퍼 성장의 서 | 2 | **3.0x** |
//!
//! # How the two books are told apart
//!
//! `HERO::SetExpGoodTime` (0x8d7cc) stores its argument to **both** `+0x330`
//! (the total duration) and `+0x328` (the remaining time), so the duration the
//! book was activated with survives as a marker. The original passes exactly
//! 10,800,000 ms (`0x10bc88: mov r2,#0xa50000` / `sub r2,r2,#0x3480`); the super
//! book passes [`SUPER_DURATION_MS`] = one millisecond more. The guest already
//! saves that field, so the host keeps no state of its own and a reboot cannot
//! lose track of which book is running.
//!
//! One millisecond is not a meaningful difference to a player: the activation
//! message prints `MsToHour`/`MsToMin`/`MsToSec` and the in-game timer rounds
//! remaining time *up* to the next minute, so both books read "03:00:00".
//!
//! # This is a behaviour change, so it is a hook and not a patch
//!
//! `patch.rs`'s contract is "the same bytes out", proved by the 1,241-frame
//! A/B. This deliberately changes what the game does, so it lives in
//! [`Vm::hooks`] beside [`crate::input`] and [`crate::shop`], and it is off in
//! [`Config::default`] so that A/B keeps proving what it proves.

use crate::patch::PatchFn;
use crate::Vm;
use arm32_cpu::Mode;

/// `Monster::SetDieMotion`, whose `+0x68` is the hook site.
const SET_DIE_MOTION: &str = "_ZN7Monster12SetDieMotionEv";

/// Offset of `ldr r3, [r8, r7]`, the instruction after `bl Formula::calc`.
///
/// Named as an offset from the symbol rather than as `0xc06ec` because this
/// host never speaks in bare addresses — the guest's symbols are all present,
/// so a raw address would be the one fact a reader could not check.
const AWARD_SITE_OFF: u32 = 0x68;

/// `HERO::SetExpGoodTime`, for the duration marker and for [`activate`].
const SET_EXP_GOOD_TIME: &str = "_ZN4HERO14SetExpGoodTimeEx";

/// Guest field offsets on `HERO`, all read straight out of
/// `SetExpGoodTime` (0x8d7cc) and `ProcAutoExpGoodTime` (0x8d5a4).
mod hero {
    /// Remaining time, 64-bit, recomputed every tick by `ProcAutoExpGoodTime`.
    pub const EXP_REMAINING: u32 = 0x328;
    /// The duration the book was activated with, 64-bit. Written once by
    /// `SetExpGoodTime` and never touched again, which is what makes it usable
    /// as a marker for *which* book is running.
    pub const EXP_TOTAL: u32 = 0x330;
    /// The guest's own "a book is active" latch, set by `ProcAutoExpGoodTime`
    /// from the full 64-bit remaining time.
    pub const EXP_ACTIVE: u32 = 0x338;
}

/// `gv + HERO_PTR` is the `HERO*`. The hook does not need to look this up: the
/// guest's very next instruction is `ldr r3, [r8, r7]`, so `r8` already holds
/// `gv` and `r7` already holds this offset. The constant is here only so the
/// assertion in [`hero_ptr`] can say what it is checking.
const HERO_PTR: u32 = 0x1474;

/// Three hours plus one millisecond — the super book's duration, and the marker
/// that distinguishes it from the original's exactly-three-hours.
pub const SUPER_DURATION_MS: i64 = 10_800_001;

/// The original 성장의 서's duration, hard-coded at `StateInGameMenu::Process+0x1668`.
pub const NORMAL_DURATION_MS: i64 = 10_800_000;

/// What the super book multiplies the *base* award by, before the guest adds
/// its own `floor(base/2)`. See the module header's table: 2 x 1.5 = 3.0.
const SUPER_FACTOR: i64 = 2;

/// Experience is stored as a signed 32-bit at `HERO+0x230`, the guest adds
/// `floor(r0/2)` on top of whatever we return, and the level-up path
/// (`CalcStatusComputation+0x2004..`) does *signed* compares against the
/// exp-to-next-level. A product that wrapped would make experience negative and
/// break levelling, so the factor is applied with a clamp well below `i32::MAX`.
const EXP_CLAMP: i64 = 0x3fff_ffff;

/// Read the `HERO*` the way the guest is about to.
///
/// `r7`/`r8` are set up before the `bl` (0xc06bc `ldr r8,[r4,r6]` and
/// 0xc06b8/0xc06c4 building `r7 = 0x1474`) and both are callee-saved, which the
/// guest itself relies on — `0xc06ec` dereferences them immediately. Reading
/// them is therefore cheaper *and* more faithful than resolving `gv` by symbol.
fn hero_ptr(vm: &Vm) -> Option<u32> {
    let gv = vm.cpu.reg_get(Mode::User, 8);
    let off = vm.cpu.reg_get(Mode::User, 7);
    // If the registers are not what the disassembly says, do nothing rather
    // than scale by a value read from the wrong place.
    if off != HERO_PTR {
        return None;
    }
    let hero = vm.mem.read_u32(gv.wrapping_add(off));
    if hero == 0 {
        return None;
    }
    Some(hero)
}

/// Read a guest `long long` field.
fn read_i64(vm: &Vm, base: u32, off: u32) -> i64 {
    let lo = vm.mem.read_u32(base.wrapping_add(off)) as u64;
    let hi = vm.mem.read_u32(base.wrapping_add(off).wrapping_add(4)) as u64;
    ((hi << 32) | lo) as i64
}

/// Whether a book is running, and which one.
///
/// The active test is the guest's own latch at `+0x338` rather than a fresh
/// comparison, so the host and the guest can never disagree about whether the
/// buff is on: `ProcAutoExpGoodTime` sets that byte from the full 64-bit
/// remaining time every tick, and it is what the exp-bar overlay keys off too.
/// A belt-and-braces check on the remaining time is included because the latch
/// is only refreshed while `HERO::Proc` runs.
fn active_factor(vm: &Vm, hero: u32) -> i64 {
    if vm.mem.read_u8(hero.wrapping_add(hero::EXP_ACTIVE)) == 0 {
        return 1;
    }
    if read_i64(vm, hero, hero::EXP_REMAINING) <= 0 {
        return 1;
    }
    if read_i64(vm, hero, hero::EXP_TOTAL) == SUPER_DURATION_MS {
        SUPER_FACTOR
    } else {
        1
    }
}

/// The hook. Scales `r0` and **always declines**.
///
/// Returning `true` here would be a disaster and it is worth naming: the
/// dispatcher treats a `true` as "this hook replaced a function" and sets
/// `PC := LR` (`lib.rs`), which from `+0x68` would return out of
/// `Monster::SetDieMotion` and skip `QuestMgr::QuestCheck`, the hunting
/// counters, `setMonState(4)`, `SetMotion(5)` and `SetDecreaseDieFrame` — the
/// monster would take the damage and never die.
fn scale_kill_exp(vm: &mut Vm) -> Result<bool, String> {
    let base = vm.cpu.reg_get(Mode::User, 0) as i32 as i64;
    // A kill worth nothing stays worth nothing, and this is the common case for
    // the `Formula::calc` guard that returns 0.
    if base <= 0 {
        return Ok(false);
    }
    let Some(hero) = hero_ptr(vm) else { return Ok(false) };
    let factor = active_factor(vm, hero);
    if factor != 1 {
        let scaled = base.saturating_mul(factor).min(EXP_CLAMP);
        vm.cpu.reg_set(Mode::User, 0, scaled as u32);
    }
    Ok(false)
}

/// Install the hook. Returns the names it attached to, for the boot log.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    let Some(base) = vm.symbol(SET_DIE_MOTION) else {
        return Err(format!("super-growth hook needs `{SET_DIE_MOTION}`, not in this binary"));
    };
    if vm.symbol(SET_EXP_GOOD_TIME).is_none() {
        return Err(format!("super-growth hook needs `{SET_EXP_GOOD_TIME}`, not in this binary"));
    }
    let f: PatchFn = scale_kill_exp;
    vm.install_hook(SET_DIE_MOTION, base + AWARD_SITE_OFF, f)?;
    Ok(vec![SET_DIE_MOTION])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_super_marker_is_one_millisecond_past_the_original() {
        // The whole scheme rests on these two being different and on the
        // difference being invisible to a player: both render as 03:00:00.
        assert_eq!(SUPER_DURATION_MS, NORMAL_DURATION_MS + 1);
        assert_eq!(NORMAL_DURATION_MS, 3 * 60 * 60 * 1000);
        // And on the original being exactly the constant the guest builds at
        // 0x10bc88: mov r2,#0xa50000 / sub r2,r2,#0x3480.
        assert_eq!(NORMAL_DURATION_MS, 0x00a5_0000 - 0x3480);
    }

    #[test]
    fn the_factor_composes_with_the_guest_bonus_to_exactly_three() {
        // The guest does `base + floor(base/2)` on whatever we hand it. Check
        // the composition on values where the floor actually bites, so a
        // future change to SUPER_FACTOR cannot quietly become 2.9x.
        for base in [1i64, 2, 3, 7, 99, 1000, 12_345] {
            let scaled = base * SUPER_FACTOR;
            let total = scaled + scaled / 2;
            assert_eq!(total, base * 3, "base {base}");
        }
    }

    #[test]
    fn the_clamp_keeps_the_product_a_positive_i32() {
        // The award is stored as a signed 32-bit and the guest then adds half
        // of it again, so the clamp has to leave room for that too.
        let clamped = i64::MAX.saturating_mul(SUPER_FACTOR).min(EXP_CLAMP);
        assert_eq!(clamped, EXP_CLAMP);
        assert!(clamped + clamped / 2 <= i64::from(i32::MAX));
        assert!(i32::try_from(clamped).is_ok());
    }
}
