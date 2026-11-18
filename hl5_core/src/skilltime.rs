//! Buff skills: the seconds the skill text prints become real, and the
//! operator's rebalance on top (`ANALYSIS.md` §71).
//!
//! # The text and the clock disagree, in the shipped game too
//!
//! Every skill timer counts **game ticks**: `HeroSkillInfo::DecreaseNowCoolTime`
//! (0xd8d54) is called once per `HERO::ProcHeroSkill`, and the buff slots lose
//! one per `HERO::HeroBuffEndProc` / `BATTLER::BattlerBuffEndProc` through
//! `BATTLER::DecreaseCount` (0x49f68, `sub #1`). Both run once per class
//! `Proc`/`EndProc`, i.e. once per tick — measured 145 : 144 on `ingame.txt`.
//!
//! The skill text, though, prints the formula value **divided by ten** as
//! seconds: `CommonUi::GetSkillDiscription` (0x7a7b4) turns `#07` (SP, formula
//! `info+0xc`), `#08` (duration, `info+0x3d`) and `#09` (cooldown, `info+0x14`)
//! into `value/10` with one decimal (`smull 0x66666667`). So the text assumes
//! ten ticks a second. This shell runs the guest at a fixed 24 (`TICK_CAP_HZ`
//! in the shell, §44), so a buff that reads "34초" ended after 14.2 s.
//!
//! # Lever one: convert at the moment a timer is armed ([`Config::skill_time_hz`])
//!
//! The formula values stay what they are — tenths of a second, which is what the
//! text already says — and the host turns them into ticks where a buff timer or
//! a buff cooldown is set:
//!
//! | site | what | argument |
//! |---|---|---|
//! | `BATTLER::AddBuffSkill` (0x4b198) | buff, mode 2 and the talent-triggered mode 4 | 5th, `[sp]` (i16) |
//! | `HERO::AddStanceSkill` (0x91d7c) | 태세 / 문장, mode 3 | `r3` |
//! | `HERO::AddSpiritSkill` (0x91d1c) | the nine 정령 buffs | `r3` |
//! | `HeroSkillInfo::SetNowCoolTime` (0xd8d38) | cooldown, from `ProcHeroSkill+0xd84` only, mode 2/3 class rows only | `r1` |
//!
//! Each `Add*Skill` has one kind of caller — `ProcHeroSkill` for the first two,
//! `TargetEffect::Proc` (nine sites, one per spirit buff) for the third — so
//! every call is a skill buff. Debuffs go through `BATTLER::AddCurseSkill` and
//! are not touched. Attack-skill cooldowns are filtered out by mode; spirit
//! cooldowns by row (the gauge, not the timer, is what limits those, and
//! `UseSpiritSkill+0x23c` mirrors the value into the global `SetSpiritCoolTime`
//! too). Durations of 0 and 1 pass through: 긴급회복's 1 is "instant, no effect
//! sprite" to `AddBuffSkill` (`cmpne r5,#1 / ble`).
//!
//! # Lever two: the rebalance ([`Config::buff_rebalance`], [`REBALANCE`])
//!
//! Operator, 2026-10-27: the buffs that cost nothing and recharge in a second
//! get **twice the duration**; every other class buff gets **twice the SP,
//! twice the cooldown and twice the duration** — except 긴급회피 and 긴급회복,
//! whose cooldown is real seconds only, and 무적, whose cooldown is 100 s.
//! Spirit buffs are neither — they spend the whole gauge. [`REBALANCE`] states
//! every new value.
//!
//! * **SP and cooldown** are the per-skill base values in `skill_0N.dat`
//!   (`v168` / `v170`), so they are rewritten in a file shadow. Every reader —
//!   `HERO::UseSkill`, the skill text, the skill-book text — computes from that
//!   field, and talent reductions (모독, 회피충전) keep applying on top.
//! * **Duration** comes from formulas several skills share (`calc_sk.dat` 43
//!   "300+40·Lv" serves fourteen buffs, 59 and 66 dozens of unrelated fields),
//!   and that file is DES-encrypted. So the duration is doubled by the host at
//!   the two arm hooks above **and** at the two places the text computes `#08`
//!   — after `bl Formula::calc` in `GetSkillDiscription` (+0x598) and in
//!   `DrawSkillBookDiscription` (+0x830). Every row in [`REBALANCE`] has its own
//!   buff type (`info+0x3a`), checked unique across all five skill files, so the
//!   type alone says which skill a call is for.
//!
//! # Lever three: the rest of the text tells the truth (with lever one)
//!
//! Operator, 2026-10-27: everything lever one does **not** arm in real seconds
//! — attack-skill cooldowns, curses, 기절·경직·넉다운·블록 — keeps playing as
//! shipped, and its text shows the real seconds instead. The two formatters
//! (`GetSkillDiscription`, `DrawSkillBookDiscription` — copies of each other,
//! the only places a skill text is built) get a hook after each `bl
//! Formula::calc` for `#08`, `#09`, `#12` and `#13` ([`TEXT_SITES`]); where the
//! placeholder is followed by `초` the tick count is turned into tenths of a
//! real second. Two literals the placeholders cannot reach are rewritten in the
//! skill-file shadow ([`TEXT_EDITS`]: 포대 "15초", 그림자밟기's shipped
//! "#13초" that is really a percent), and 독바르기's "중독 10초" is made true at
//! runtime: `Monster::AddEffectPoison`'s fixed 200 ticks become 10 s.
//!
//! The icon bar under a buff is `cur / max` (`GMenu::DrawBuildupIcon`) and both
//! come from the one argument scaled here; the skill belt draws cooldowns as a
//! sweep (`Graphic::DrawCoolTime`), never as a number. Every hook declines.

use crate::patch::PatchFn;
use crate::Vm;
use arm32_cpu::Mode;
use std::path::Path;

const ADD_BUFF_SKILL: &str = "_ZN7BATTLER12AddBuffSkillEPS_ass";
const ADD_STANCE_SKILL: &str = "_ZN4HERO14AddStanceSkillEass";
const ADD_SPIRIT_SKILL: &str = "_ZN4HERO14AddSpiritSkillEassP6SPRITEa";
const SET_NOW_COOL_TIME: &str = "_ZN13HeroSkillInfo14SetNowCoolTimeEs";
const PROC_HERO_SKILL: &str = "_ZN4HERO13ProcHeroSkillEP13HeroSkillInfo";
const SKILL_TEXT: &str = "_ZN8CommonUi19GetSkillDiscriptionEPcaP13HeroSkillInfo";
const BOOK_TEXT: &str = "_ZN8CommonUi24DrawSkillBookDiscriptionEiiiP17SkillBookItemInfo";

/// The return address of `ProcHeroSkill`'s `bl SetNowCoolTime` (+0xd84). The
/// other two callers are `UseSpiritSkill` (spirit, left alone) and
/// `HERO::Revival` (clears).
const COOL_CALL_RET_OFF: u32 = 0xd88;

/// `HeroSkillInfo` fields, from `HERO::LoadResSkillInfo` (0x8bba4).
mod info {
    /// The description, EUC-KR with `#NN` placeholders.
    pub const DESC: u32 = 0x40;
    /// Row index in the hero's table: 0..=42 the class, 43.. the spirit.
    pub const INDEX: u32 = 0x00;
    /// What `ProcHeroSkill`'s 0x9a398 jump table does with the skill: 0/1
    /// curse a target, 2 and 4 `AddBuffSkill`, 3 `AddStanceSkill`.
    pub const MODE: u32 = 0x28;
    /// The buff type the `Add*Skill` call carries.
    pub const BUFF_TYPE: u32 = 0x3a;
}

/// The last class row; spirit rows follow (`cmp r3,#0x2a` at 0x99fdc).
const LAST_CLASS_ROW: i8 = 0x2a;

/// The modes whose cooldown the skill text calls a buff's: 버프 and 태세/문장.
const BUFF_MODES: [i8; 2] = [2, 3];

/// One rebalanced skill: the shipped SP and cooldown bases the shadow refuses
/// to touch without, the bases it writes instead, and whether the duration
/// doubles.
#[derive(Clone, Copy, Debug)]
pub struct Rebalanced {
    /// `skill_0<N>.dat`: 0 워리어, 1 로그, 2 건슬링어, 3 나이트.
    pub file: u8,
    pub row: u8,
    pub name: &'static str,
    /// `info+0x3a`, unique across all five skill files.
    pub buff_type: i8,
    /// `info+0x28`.
    pub mode: i8,
    /// SP base (`v168`, ×10 of the SP the text prints): shipped, new.
    pub sp: (i16, i16),
    /// Cooldown base (`v170`, tenths of a second): shipped, new.
    pub cool: (i16, i16),
    /// Twice the duration ([`FACTOR`]).
    pub dur_x2: bool,
}

const fn r(
    file: u8,
    row: u8,
    name: &'static str,
    buff_type: i8,
    mode: i8,
    sp: (i16, i16),
    cool: (i16, i16),
    dur_x2: bool,
) -> Rebalanced {
    Rebalanced { file, row, name, buff_type, mode, sp, cool, dur_x2 }
}

/// The operator's table (2026-10-27).
///
/// The first thirteen are the "SP 0 · 재사용 1초" answer's three groups (워리어
/// SP 0 / 1초, 로그 SP 10 / 1초, 건슬링어 SP 0 / none): duration ×2 only. The
/// rest is every other class buff: SP, cooldown and duration ×2 — except
/// 긴급회피 and 긴급회복, whose cooldown stays the shipped number (real
/// seconds only), 긴급회복's instant "duration", and 무적's cooldown, which is
/// 100 s.
pub const REBALANCE: [Rebalanced; 24] = [
    r(0, 13, "전투태세", 63, 3, (0, 0), (10, 10), true),
    r(0, 23, "방어태세", 64, 3, (0, 0), (10, 10), true),
    r(0, 31, "발악", 65, 2, (0, 0), (10, 10), true),
    r(0, 34, "광폭태세", 66, 3, (0, 0), (10, 10), true),
    r(0, 41, "무통증", 67, 2, (0, 0), (10, 10), true),
    r(0, 42, "광전사", 68, 2, (0, 0), (10, 10), true),
    r(1, 12, "칼날폭풍", 69, 2, (100, 100), (10, 10), true),
    r(1, 17, "급소공격", 70, 2, (100, 100), (10, 10), true),
    r(1, 20, "강공태세", 71, 3, (100, 100), (10, 10), true),
    r(1, 23, "독바르기", 72, 2, (100, 100), (10, 10), true),
    r(2, 12, "쐐기탄", 79, 2, (0, 0), (0, 0), true),
    r(2, 16, "철갑탄", 80, 2, (0, 0), (0, 0), true),
    r(2, 19, "연사", 98, 2, (0, 0), (0, 0), true),
    r(0, 6, "도발", 95, 2, (300, 600), (240, 480), true),
    r(1, 4, "긴급회피", 74, 2, (100, 200), (500, 500), true),
    r(1, 31, "호흡집중", 97, 2, (200, 400), (300, 600), true),
    r(1, 34, "질주", 73, 2, (200, 400), (300, 600), true),
    r(1, 38, "독무", 76, 3, (200, 400), (300, 600), true),
    r(3, 13, "증오의문장", 82, 3, (300, 600), (250, 500), true),
    r(3, 17, "보복의문장", 83, 3, (300, 600), (250, 500), true),
    r(3, 35, "방어의문장", 85, 3, (300, 600), (250, 500), true),
    r(3, 39, "저항의문장", 86, 3, (300, 600), (250, 500), true),
    r(3, 42, "무적", 87, 2, (300, 600), (800, 1000), true),
    r(3, 27, "긴급회복", 84, 2, (300, 600), (500, 500), false),
];

/// What the rebalance multiplies a duration by.
pub const FACTOR: i32 = 2;

/// The duration multiplier for a buff type: [`FACTOR`] for a row with
/// `dur_x2`, 1 for everything else.
pub fn duration_factor(buff_type: i8) -> i32 {
    REBALANCE
        .iter()
        .find(|e| e.buff_type == buff_type && e.dur_x2)
        .map_or(1, |_| FACTOR)
}

impl Rebalanced {
    /// Whether the shadow writes this row at all.
    fn rewrites_bases(&self) -> bool {
        self.sp.0 != self.sp.1 || self.cool.0 != self.cool.1
    }
}

/// Tenths of a second → ticks at `hz`, rounded to the nearest tick.
///
/// 0 and 1 pass through untouched (see the module header), and the result is
/// clamped to the `i16` the guest stores it in.
pub fn to_ticks(tenths: i32, hz: u32) -> i32 {
    if tenths <= 1 || hz == 0 {
        return tenths;
    }
    let ticks = (i64::from(tenths) * i64::from(hz) + 5) / 10;
    ticks.min(i64::from(i16::MAX)) as i32
}

/// A buff duration as the arm hooks set it: the rebalance first (in tenths),
/// then the conversion to ticks.
pub fn armed_duration(tenths: i32, buff_type: i8, rebalance: bool, hz: u32) -> i32 {
    let tenths = if rebalance && tenths > 1 {
        (tenths * duration_factor(buff_type)).min(i32::from(i16::MAX))
    } else {
        tenths
    };
    to_ticks(tenths, hz)
}

fn armed(vm: &Vm, dur: i32, buff_type: i8) -> i32 {
    armed_duration(dur, buff_type, vm.cfg.buff_rebalance, vm.cfg.skill_time_hz)
}

/// `AddBuffSkill(this, target, type, value, short duration)`: the duration is
/// the fifth argument, the first stack word at entry (`ldrsh r5,[sp,#0x40]`
/// after its own 0x40 bytes of frame).
fn scale_buff(vm: &mut Vm) -> Result<bool, String> {
    let sp = vm.cpu.reg_get(Mode::User, 13);
    let dur = vm.mem.read_u32(sp) as u16 as i16 as i32;
    let ty = vm.cpu.reg_get(Mode::User, 2) as i8;
    let ticks = armed(vm, dur, ty);
    if ticks != dur {
        vm.mem.write_u32(sp, ticks as u32);
    }
    Ok(false)
}

/// `AddStanceSkill(this, type, value, duration)` and
/// `AddSpiritSkill(this, type, value, duration, sprite, ..)`: `r1`, `r3`.
fn scale_r3(vm: &mut Vm) -> Result<bool, String> {
    let dur = vm.cpu.reg_get(Mode::User, 3) as u16 as i16 as i32;
    let ty = vm.cpu.reg_get(Mode::User, 1) as i8;
    let ticks = armed(vm, dur, ty);
    if ticks != dur {
        vm.cpu.reg_set(Mode::User, 3, ticks as u32);
    }
    Ok(false)
}

/// `SetNowCoolTime(info, short)`, converted only for a class buff armed by
/// `ProcHeroSkill` — see the module header for what is left alone. The
/// rebalance's doubling is already in the value (the base is in the shadow).
fn scale_cooldown(vm: &mut Vm) -> Result<bool, String> {
    let Some(proc) = vm.symbol(PROC_HERO_SKILL) else { return Ok(false) };
    if vm.cpu.reg_get(Mode::User, 14) != proc + COOL_CALL_RET_OFF {
        return Ok(false);
    }
    let skill = vm.cpu.reg_get(Mode::User, 0);
    let index = vm.mem.read_u8(skill + info::INDEX) as i8;
    let mode = vm.mem.read_u8(skill + info::MODE) as i8;
    if !(0..=LAST_CLASS_ROW).contains(&index) || !BUFF_MODES.contains(&mode) {
        return Ok(false);
    }
    let cool = vm.cpu.reg_get(Mode::User, 1) as u16 as i16 as i32;
    let ticks = to_ticks(cool, vm.cfg.skill_time_hz);
    if ticks != cool {
        vm.cpu.reg_set(Mode::User, 1, ticks as u32);
    }
    Ok(false)
}

/// Which time a text placeholder prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextCode {
    /// `#08`, the duration formula (`info+0x3d`).
    Duration,
    /// `#09`, the cooldown formula (`info+0x14`).
    Cooldown,
    /// `#12` / `#13`, the add-effect and value formulas (`+0x1e` / `+0x3c`)
    /// with one decimal — 기절·경직·넉다운·블록 times where followed by 초.
    Seconds,
}

/// Whether the arm hooks already turn this value into real seconds, so the
/// text — tenths — is true as it stands.
fn armed_in_real_seconds(code: TextCode, index: i8, mode: i8) -> bool {
    let class = (0..=LAST_CLASS_ROW).contains(&index);
    match code {
        // `AddBuffSkill` (2, 4) and `AddStanceSkill` (3); spirit buffs through
        // `AddSpiritSkill`. Curses (0, 1) and everything else run in ticks.
        TextCode::Duration => (class && matches!(mode, 2..=4)) || (!class && mode == 2),
        TextCode::Cooldown => class && BUFF_MODES.contains(&mode),
        TextCode::Seconds => false,
    }
}

/// What a text placeholder should print, in the tenths the guest divides by
/// ten. The rebalance doubles a listed buff's duration; then, if `seconds` and
/// the value is not armed in real seconds, the guest's tick count becomes
/// tenths of a real second at `hz` — the operator's choice for every other
/// skill: the text tells the truth, the game plays as shipped.
pub fn shown_tenths(
    value: i32,
    code: TextCode,
    seconds: bool,
    index: i8,
    mode: i8,
    buff_type: i8,
    rebalance: bool,
    hz: u32,
) -> i32 {
    let mut v = value;
    if code == TextCode::Duration && rebalance {
        v *= duration_factor(buff_type);
    }
    if seconds && hz > 0 && v > 0 && !armed_in_real_seconds(code, index, mode) {
        let hz = hz as i32;
        v = (v * 20 + hz) / (2 * hz);
    }
    v
}

/// EUC-KR `초`.
const CHO: [u8; 2] = [0xc3, 0xca];

/// Whether the description continues with `초` (after an optional `|`, the
/// colour-end marker: 발악 writes `}#08|초`).
fn followed_by_seconds(vm: &Vm, desc: u32, at: u32) -> bool {
    let mut p = desc + at;
    if vm.mem.read_u8(p) == b'|' {
        p += 1;
    }
    vm.mem.read_u8(p) == CHO[0] && vm.mem.read_u8(p + 1) == CHO[1]
}

/// One text site: right after `bl Formula::calc`, `r0` is the value, the info
/// pointer is in `info_reg` and the description cursor — just past the two
/// digits of `#NN` — in `at_reg`. Both registers are callee-saved.
fn text_site(vm: &mut Vm, info_reg: u8, at_reg: u8, code: TextCode) {
    let skill = vm.cpu.reg_get(Mode::User, info_reg);
    let desc = vm.mem.read_u32(skill + info::DESC);
    let at = vm.cpu.reg_get(Mode::User, at_reg);
    let value = vm.cpu.reg_get(Mode::User, 0) as i32;
    let shown = shown_tenths(
        value,
        code,
        followed_by_seconds(vm, desc, at),
        vm.mem.read_u8(skill + info::INDEX) as i8,
        vm.mem.read_u8(skill + info::MODE) as i8,
        vm.mem.read_u8(skill + info::BUFF_TYPE) as i8,
        vm.cfg.buff_rebalance,
        vm.cfg.skill_time_hz,
    );
    if shown != value {
        vm.cpu.reg_set(Mode::User, 0, shown as u32);
    }
}

// `GetSkillDiscription`: info `r6`, cursor `r4`.
fn skill_text_dur(vm: &mut Vm) -> Result<bool, String> {
    text_site(vm, 6, 4, TextCode::Duration);
    Ok(false)
}
fn skill_text_cool(vm: &mut Vm) -> Result<bool, String> {
    text_site(vm, 6, 4, TextCode::Cooldown);
    Ok(false)
}
fn skill_text_secs(vm: &mut Vm) -> Result<bool, String> {
    text_site(vm, 6, 4, TextCode::Seconds);
    Ok(false)
}
// `DrawSkillBookDiscription`: info `r8`, cursor `r5`.
fn book_text_dur(vm: &mut Vm) -> Result<bool, String> {
    text_site(vm, 8, 5, TextCode::Duration);
    Ok(false)
}
fn book_text_cool(vm: &mut Vm) -> Result<bool, String> {
    text_site(vm, 8, 5, TextCode::Cooldown);
    Ok(false)
}
fn book_text_secs(vm: &mut Vm) -> Result<bool, String> {
    text_site(vm, 8, 5, TextCode::Seconds);
    Ok(false)
}

/// The text sites: (function, offset of the instruction after `bl
/// Formula::calc`, hook). The two functions are copies of one formatter.
const TEXT_SITES: [(&str, u32, PatchFn); 8] = [
    (SKILL_TEXT, 0x598, skill_text_dur),  // #08
    (SKILL_TEXT, 0x67c, skill_text_cool), // #09
    (SKILL_TEXT, 0x378, skill_text_secs), // #12
    (SKILL_TEXT, 0x45c, skill_text_secs), // #13
    (BOOK_TEXT, 0x830, book_text_dur),    // #08
    (BOOK_TEXT, 0x924, book_text_cool),   // #09
    (BOOK_TEXT, 0x4b8, book_text_secs),   // #12
    (BOOK_TEXT, 0x3bc, book_text_secs),   // #13
];

/// `Monster::AddEffectPoison` (0xbb4dc) — the only way a monster is poisoned,
/// from `BATTLER::ApplyAddEffect`. It hands `AddBadSkill` a fixed 200 ticks
/// (`mov r4,#0xc8 / str r4,[sp]`).
const ADD_EFFECT_POISON: &str = "_ZN7Monster15AddEffectPoisonEs";
/// Its `bl BATTLER::AddBadSkill`, where `[sp]` holds the 200.
const POISON_CALL_OFF: u32 = 0x40;
const SHIPPED_POISON_TICKS: u32 = 200;
/// What 독바르기's text promises: "중독된 적은 10초간". Operator: make it so.
const POISON_TEXT_TENTHS: i32 = 100;

fn poison_real_seconds(vm: &mut Vm) -> Result<bool, String> {
    let sp = vm.cpu.reg_get(Mode::User, 13);
    if vm.mem.read_u32(sp) == SHIPPED_POISON_TICKS {
        vm.mem.write_u32(sp, to_ticks(POISON_TEXT_TENTHS, vm.cfg.skill_time_hz) as u32);
    }
    Ok(false)
}

/// Install the hooks the levers need. Returns the names they attached to, for
/// the boot log.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    if vm.symbol(PROC_HERO_SKILL).is_none() {
        return Err(format!("skill-time hooks need `{PROC_HERO_SKILL}`, not in this binary"));
    }
    let mut sites: Vec<(&'static str, u32, PatchFn)> = vec![
        (ADD_BUFF_SKILL, 0, scale_buff),
        (ADD_STANCE_SKILL, 0, scale_r3),
        (ADD_SPIRIT_SKILL, 0, scale_r3),
    ];
    if vm.cfg.skill_time_hz > 0 {
        sites.push((SET_NOW_COOL_TIME, 0, scale_cooldown));
        sites.push((ADD_EFFECT_POISON, POISON_CALL_OFF, poison_real_seconds));
    }
    sites.extend(TEXT_SITES);
    let mut names = Vec::new();
    for (name, off, f) in sites {
        let base = vm
            .symbol(name)
            .ok_or_else(|| format!("skill-time hook needs `{name}`, not in this binary"))?;
        vm.install_hook(name, base + off, f)?;
        names.push(name);
    }
    names.dedup();
    Ok(names)
}

/// `c/csv/skill_0<N>.dat`.
fn skill_path(file: u8) -> String {
    format!("c/csv/skill_{file:02}.dat")
}

/// Offsets in a record's 48-byte block (`LoadResSkillInfo`): the block starts
/// after `[u16 len][u8 nameLen][name]`.
mod block {
    pub const SP_BASE: usize = 0x05;
    pub const COOL_BASE: usize = 0x0b;
    pub const MODE: usize = 0x1a;
    pub const BUFF_TYPE: usize = 0x2b;
    pub const LEN: usize = 0x30;
}

/// Where row `row`'s 48-byte block starts in a skill file, if it parses.
fn block_at(src: &[u8], row: usize) -> Option<usize> {
    let count = u16::from_le_bytes([*src.first()?, *src.get(1)?]) as usize;
    if row >= count {
        return None;
    }
    let mut p = 2usize;
    for _ in 0..row {
        let len = u16::from_le_bytes([*src.get(p)?, *src.get(p + 1)?]) as usize;
        p += 2 + len;
    }
    let name_len = *src.get(p + 2)? as usize;
    let b = p + 3 + name_len;
    (b + block::LEN <= src.len()).then_some(b)
}

fn i16_at(src: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([src[at], src[at + 1]])
}

/// A literal in a description that the `#NN` hooks cannot reach.
#[derive(Clone, Copy, Debug)]
pub struct TextEdit {
    pub file: u8,
    pub row: u8,
    pub name: &'static str,
    /// The exact bytes, which must occur once in that row's description.
    pub from: &'static [u8],
    /// What replaces them: `prefix`, then — with `ticks` — the real seconds
    /// those ticks last at the shell's rate, one decimal, then `suffix`.
    pub prefix: &'static [u8],
    pub ticks: Option<i32>,
    pub suffix: &'static [u8],
}

/// The literals the operator's "설명문을 실제 초로" covers (2026-10-27).
///
/// * 포대 "}15초|동안 유지": the turret effect is created with a lifetime of 150
///   ticks (`GUNNER::NewSentryBodyEffect`, `mov r4,#0x96` → `NewTargetEffect`'s
///   short at `[sp,#0x18]`), the game's own ÷10 reading of which is the "15".
/// * 그림자밟기 "}#13초| 단축": a shipped mistake — the value (5·Lv, formula 77)
///   is a **percent** in 축지's cooldown formula (52: `(100-v125)-(5*v177)`),
///   not seconds. `#10` prints the same formula as an integer: "5%".
///
/// Not here, because the tick count behind them is not pinned down: 지옥소환
/// "30초" (fire zone) and 유도탄 "0.8초간 경직" (`Monster::AddEffectKnockBack`
/// feeds `SetDecreaseDamagedFrame`, whose unit is not seconds). 독바르기 "10초"
/// is made true at runtime instead (the poison hook).
pub const TEXT_EDITS: [TextEdit; 2] = [
    TextEdit { file: 2, row: 7, name: "포대", from: b"}15\xc3\xca|", prefix: b"}", ticks: Some(150), suffix: b"\xc3\xca|" },
    TextEdit { file: 1, row: 41, name: "그림자밟기", from: b"#13\xc3\xca|", prefix: b"#10%|", ticks: None, suffix: b"" },
];

/// `ticks` as the guest prints tenths: "6.3" for 150 at 24 Hz.
pub fn seconds_text(ticks: i32, hz: u32) -> String {
    let hz = hz.max(1) as i32;
    let tenths = (ticks * 20 + hz) / (2 * hz);
    if tenths % 10 == 0 { format!("{}", tenths / 10) } else { format!("{}.{}", tenths / 10, tenths % 10) }
}

impl TextEdit {
    fn replacement(&self, hz: u32) -> Vec<u8> {
        let mut v = self.prefix.to_vec();
        if let Some(t) = self.ticks {
            v.extend_from_slice(seconds_text(t, hz).as_bytes());
        }
        v.extend_from_slice(self.suffix);
        v
    }
}

/// Replace `from` — exactly once — in row `row`'s description, fixing the
/// description's length byte (block +0x2f) and the record's `u16` length.
pub fn edit_desc(src: &[u8], row: usize, from: &[u8], to: &[u8]) -> Result<Vec<u8>, String> {
    let bad = || format!("row {row}: the file does not parse");
    let count = u16::from_le_bytes([*src.first().ok_or_else(bad)?, *src.get(1).ok_or_else(bad)?]) as usize;
    if row >= count {
        return Err(bad());
    }
    let mut out = src[..2].to_vec();
    let mut p = 2usize;
    for i in 0..count {
        let len = u16::from_le_bytes([*src.get(p).ok_or_else(bad)?, *src.get(p + 1).ok_or_else(bad)?]) as usize;
        let rec = src.get(p + 2..p + 2 + len).ok_or_else(bad)?;
        p += 2 + len;
        if i != row {
            out.extend_from_slice(&src[p - 2 - len..p]);
            continue;
        }
        let b = 1 + *rec.first().ok_or_else(bad)? as usize;
        let dlen_at = b + block::LEN - 1;
        let dlen = *rec.get(dlen_at).ok_or_else(bad)? as usize;
        let desc = rec.get(b + block::LEN..b + block::LEN + dlen).ok_or_else(bad)?;
        let hits: Vec<usize> = (0..desc.len().saturating_sub(from.len() - 1))
            .filter(|&k| desc[k..].starts_with(from))
            .collect();
        if hits.len() != 1 {
            return Err(format!("row {row}: the text to replace occurs {} times", hits.len()));
        }
        let mut nd = desc[..hits[0]].to_vec();
        nd.extend_from_slice(to);
        nd.extend_from_slice(&desc[hits[0] + from.len()..]);
        let mut nrec = rec[..b + block::LEN].to_vec();
        nrec[dlen_at] = u8::try_from(nd.len()).map_err(|_| format!("row {row}: description too long"))?;
        nrec.extend_from_slice(&nd);
        nrec.extend_from_slice(&rec[b + block::LEN + dlen..]);
        let nlen = u16::try_from(nrec.len()).map_err(|_| format!("row {row}: record too long"))?;
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(&nrec);
    }
    out.extend_from_slice(&src[p..]);
    Ok(out)
}

/// `skill_0<file>.dat` with the table's new SP and cooldown bases written into
/// its rows. `Err` — and nothing written — if any of them is not the shipped
/// skill the table names.
pub fn rebalance_file(src: &[u8], file: u8) -> Result<Vec<u8>, String> {
    let mut out = src.to_vec();
    for e in REBALANCE.iter().filter(|e| e.file == file && e.rewrites_bases()) {
        let b = block_at(src, usize::from(e.row))
            .ok_or_else(|| format!("{}: row {} ({}) is not in the file", skill_path(file), e.row, e.name))?;
        let found = (
            src[b + block::BUFF_TYPE] as i8,
            src[b + block::MODE] as i8,
            i16_at(src, b + block::SP_BASE),
            i16_at(src, b + block::COOL_BASE),
        );
        if found != (e.buff_type, e.mode, e.sp.0, e.cool.0) {
            return Err(format!(
                "{}: row {} is not the shipped {} (type/mode/sp/cool {found:?})",
                skill_path(file),
                e.row,
                e.name
            ));
        }
        out[b + block::SP_BASE..b + block::SP_BASE + 2].copy_from_slice(&e.sp.1.to_le_bytes());
        out[b + block::COOL_BASE..b + block::COOL_BASE + 2].copy_from_slice(&e.cool.1.to_le_bytes());
    }
    Ok(out)
}

/// The shadow for `skill_0<file>.dat`: the rebalance's bases when `rebalance`,
/// the [`TEXT_EDITS`] when `hz` is set. `Ok(None)` when neither touches the file.
pub fn build_file(src: &[u8], file: u8, rebalance: bool, hz: u32) -> Result<Option<Vec<u8>>, String> {
    let bases = rebalance && REBALANCE.iter().any(|e| e.file == file && e.rewrites_bases());
    let edits: Vec<&TextEdit> =
        if hz > 0 { TEXT_EDITS.iter().filter(|e| e.file == file).collect() } else { Vec::new() };
    if !bases && edits.is_empty() {
        return Ok(None);
    }
    let mut out = if bases { rebalance_file(src, file)? } else { src.to_vec() };
    for e in edits {
        out = edit_desc(&out, usize::from(e.row), e.from, &e.replacement(hz))
            .map_err(|m| format!("{} {}: {m}", skill_path(file), e.name))?;
    }
    Ok(Some(out))
}

/// Write each class file's shadow, or remove it when nothing applies any more
/// (the switch is off, or the tables stopped touching that file). `LoadRes`
/// prefers the host file forever, so not writing is not undoing (see
/// [`crate::rates::restore_shipped_drop_bands`]).
pub fn install_shadows(vfs: &[u8], storage: &Path, rebalance: bool, hz: u32) -> Vec<String> {
    let mut logs = Vec::new();
    for file in 0..=3u8 {
        let path = skill_path(file);
        let Some(src) = crate::shop::vfs_entry(vfs, &path) else {
            logs.push(format!("skill-time: {path} is not in the archive — left as shipped"));
            continue;
        };
        match build_file(src, file, rebalance, hz) {
            Ok(Some(bytes)) => match crate::rates::write_override(storage, &path, &bytes) {
                Ok(dst) => logs.push(format!("skill-time: rebalanced skills / real-second text -> {}", dst.display())),
                Err(e) => logs.push(format!("skill-time: cannot write {path}: {e}")),
            },
            Ok(None) => match crate::rates::remove_override(storage, &path) {
                Ok(true) => logs.push(format!("skill-time: removed the {path} shadow")),
                Ok(false) => {}
                Err(e) => logs.push(format!("skill-time: cannot remove {path}: {e}")),
            },
            Err(e) => {
                // Refused: make sure an older shadow does not linger either.
                let _ = crate::rates::remove_override(storage, &path);
                logs.push(format!("skill-time: {e} — left as shipped"));
            }
        }
    }
    logs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_buff_values_land_on_the_seconds_the_text_prints() {
        // (formula value, what the text says in seconds), all from §71's table.
        // At 24 ticks a second the timer must run text-seconds × 24 ticks.
        for (tenths, secs) in [(340, 34.0), (300, 30.0), (150, 15.0), (100, 10.0), (60, 6.0), (10, 1.0)] {
            let ticks = to_ticks(tenths, 24);
            assert!((f64::from(ticks) / 24.0 - secs).abs() <= 0.5 / 24.0, "{tenths}: {ticks}");
        }
        assert_eq!(to_ticks(340, 24), 816);
        assert_eq!(to_ticks(10, 24), 24);
        assert_eq!(to_ticks(800, 24), 1920);
    }

    #[test]
    fn instant_and_empty_timers_are_not_scaled() {
        // 긴급회복 passes 1 = "instant, no sprite"; 0 is "no cooldown".
        assert_eq!(to_ticks(0, 24), 0);
        assert_eq!(to_ticks(1, 24), 1);
        assert_eq!(to_ticks(-5, 24), -5);
        assert_eq!(armed_duration(1, 84, true, 24), 1);
    }

    #[test]
    fn the_result_fits_the_guest_short() {
        // The largest clamp any duration formula has is 9999.
        assert_eq!(to_ticks(9999, 24), 23998);
        assert_eq!(to_ticks(i32::from(i16::MAX), 30), i32::from(i16::MAX));
        assert_eq!(armed_duration(9999, 65, true, 24), i32::from(i16::MAX));
    }

    #[test]
    fn zero_hz_means_off() {
        assert_eq!(to_ticks(340, 0), 340);
        assert_eq!(armed_duration(340, 65, false, 0), 340);
    }

    #[test]
    fn the_rebalance_doubles_durations_by_buff_type_and_nothing_else() {
        // 발악 (type 65): 34 s of text, 68 s rebalanced, 1632 ticks at 24 Hz.
        assert_eq!(armed_duration(340, 65, true, 24), 1632);
        assert_eq!(armed_duration(340, 65, false, 24), 816);
        // 긴급회복 is Cost only; a mode-4 talent buff (암습, 75) is not listed.
        assert_eq!(duration_factor(84), 1);
        assert_eq!(duration_factor(75), 1);
        assert_eq!(REBALANCE.iter().filter(|e| duration_factor(e.buff_type) == 2).count(), 23);
    }

    #[test]
    fn the_operator_exceptions_are_in_the_table() {
        let row = |name: &str| *REBALANCE.iter().find(|e| e.name == name).unwrap();
        // 긴급회피 / 긴급회복: the cooldown is real seconds only, not doubled.
        assert_eq!(row("긴급회피").cool, (500, 500));
        assert_eq!(row("긴급회복").cool, (500, 500));
        // 무적: 100 s.
        assert_eq!(row("무적").cool, (800, 1000));
        // Every SP that changes doubles; every other cooldown that changes too.
        for e in REBALANCE {
            assert!(e.sp.1 == e.sp.0 || e.sp.1 == 2 * e.sp.0, "{}", e.name);
            assert!(e.cool.1 == e.cool.0 || e.cool.1 == 2 * e.cool.0 || e.name == "무적", "{}", e.name);
        }
        // The thirteen duration-only rows keep their SP and cooldown.
        assert_eq!(REBALANCE.iter().filter(|e| !e.rewrites_bases()).count(), 13);
    }

    #[test]
    fn the_text_of_everything_not_armed_in_real_seconds_shows_real_seconds() {
        let show = |v, code, seconds, index, mode| shown_tenths(v, code, seconds, index, mode, 0, false, 24);
        // 내려찍기 (warrior 2, mode 0): "16초" is 160 ticks = 6.7 s.
        assert_eq!(show(160, TextCode::Cooldown, true, 2, 0), 67);
        // 고통 (rogue 8, mode 0): the debuff is a curse, armed in ticks.
        assert_eq!(show(220, TextCode::Duration, true, 8, 0), 92);
        // 폭발 (spirit, #12): 30 ticks of knockdown = 1.3 s.
        assert_eq!(show(30, TextCode::Seconds, true, 49, 0), 13);
        // A buff's own numbers are already real: unchanged.
        assert_eq!(show(340, TextCode::Duration, true, 31, 2), 340);
        assert_eq!(show(10, TextCode::Cooldown, true, 31, 2), 10);
        // A spirit buff's duration is armed through AddSpiritSkill.
        assert_eq!(show(300, TextCode::Duration, true, 44, 2), 300);
        // `#13` that is not followed by 초 (SP회복, 약탈) is not a time.
        assert_eq!(show(5, TextCode::Seconds, false, 26, 7), 5);
        // Off: the shipped text.
        assert_eq!(shown_tenths(160, TextCode::Cooldown, true, 2, 0, 0, false, 0), 160);
        // The rebalance still doubles a listed buff in the text.
        assert_eq!(shown_tenths(340, TextCode::Duration, true, 31, 2, 65, true, 24), 680);
    }

    #[test]
    fn literal_seconds_are_written_the_way_the_guest_prints_tenths() {
        assert_eq!(seconds_text(150, 24), "6.3");
        assert_eq!(seconds_text(240, 24), "10");
        assert_eq!(seconds_text(150, 10), "15");
        assert_eq!(TEXT_EDITS[0].replacement(24), b"}6.3\xc3\xca|".to_vec());
        assert_eq!(TEXT_EDITS[1].replacement(24), b"#10%|".to_vec());
        // The poison promised by 독바르기's "10초".
        assert_eq!(to_ticks(POISON_TEXT_TENTHS, 24), 240);
    }

    #[test]
    fn edit_desc_rewrites_one_description_and_both_lengths() {
        let mut f = one_row(2, 0, 0, 0, 0);
        // Give the one row a description: "a}15초|b".
        let desc: &[u8] = b"a}15\xc3\xca|b";
        let rec_len = u16::from_le_bytes([f[2], f[3]]) as usize;
        let b = 4 + 1 + 2;
        f[b + block::LEN - 1] = desc.len() as u8;
        f.extend_from_slice(desc);
        let new_len = (rec_len + desc.len()) as u16;
        f[2..4].copy_from_slice(&new_len.to_le_bytes());
        assert_eq!(f[b + block::LEN - 1] as usize, desc.len());
        let out = edit_desc(&f, 0, b"}15\xc3\xca|", b"}6.3\xc3\xca|").unwrap();
        assert_eq!(out.len(), f.len() + 1);
        assert_eq!(u16::from_le_bytes([out[2], out[3]]) as usize, f.len() + 1 - 4);
        assert_eq!(out[b + block::LEN - 1] as usize, desc.len() + 1);
        assert!(out.ends_with(b"a}6.3\xc3\xca|b"));
        // Twice, or not at all, is refused.
        assert!(edit_desc(&f, 0, b"x", b"y").is_err());
        // Nothing to do for a file no table touches.
        assert!(build_file(&f, 2, true, 0).unwrap().is_none());
    }

    #[test]
    fn every_buff_type_in_the_table_is_its_own() {
        // The hooks see only the type, so two rows must never share one.
        for (i, a) in REBALANCE.iter().enumerate() {
            for b in &REBALANCE[i + 1..] {
                assert_ne!(a.buff_type, b.buff_type, "{} / {}", a.name, b.name);
                assert!((a.file, a.row) != (b.file, b.row), "{} twice", a.name);
            }
        }
    }

    /// A one-record skill file with the given block values.
    fn one_row(name_len: usize, ty: i8, mode: i8, sp: i16, cool: i16) -> Vec<u8> {
        let mut rec = vec![name_len as u8];
        rec.extend(std::iter::repeat(b'x').take(name_len));
        let mut blk = [0u8; block::LEN];
        blk[block::SP_BASE..block::SP_BASE + 2].copy_from_slice(&sp.to_le_bytes());
        blk[block::COOL_BASE..block::COOL_BASE + 2].copy_from_slice(&cool.to_le_bytes());
        blk[block::MODE] = mode as u8;
        blk[block::BUFF_TYPE] = ty as u8;
        // The last block byte is the description's length: empty here.
        rec.extend_from_slice(&blk);
        let mut f = Vec::new();
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&(rec.len() as u16).to_le_bytes());
        f.extend_from_slice(&rec);
        f
    }

    #[test]
    fn the_shadow_refuses_a_row_that_is_not_the_shipped_skill() {
        // A file whose row 0 is not any table row's row is refused as a whole:
        // knight row 13 is not in a one-row file.
        let f = one_row(4, 82, 3, 300, 250);
        assert!(rebalance_file(&f, 3).is_err());
        // A file with no rebalanced rows comes back unchanged.
        assert_eq!(rebalance_file(&f, 2).unwrap(), f);
    }

    #[test]
    fn block_at_walks_records_by_their_length() {
        let f = one_row(6, 87, 2, 300, 800);
        let b = block_at(&f, 0).unwrap();
        assert_eq!(b, 2 + 2 + 1 + 6);
        assert_eq!(i16_at(&f, b + block::COOL_BASE), 800);
        assert!(block_at(&f, 1).is_none());
    }
}
