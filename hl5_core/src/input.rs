//! Releasing a button stops making the hero stand still.
//!
//! Two behaviour hooks, on two guest functions, for one original-game bug. The
//! user's report was "이동하다가 스킬이나 OK버튼으로 공격하면 이동키가 안먹어서
//! 좀 어색하고" — walk, attack, and the walk is over.
//!
//! # The bug
//!
//! `HERO::KeyRelease` (0x9BDCC) begins by asking which navigation key was
//! released and then throws the answer away:
//!
//! ```text
//!   0x9bdd8  bl  StaticUtil::NumKeyToNaviKey   ; result in r0
//!   0x9bddc  mov r1, #0                       ; overwritten one instruction later
//!   0x9bde8  bl  HERO::SetMoveKeyChk           ; (0, 0)
//!   0x9bdf8  bl  HERO::SetMoveKeyChk           ; (1, 0)
//!   0x9be08  bl  HERO::SetMoveKeyChk           ; (2, 0)
//!   0x9be18  bl  HERO::SetMoveKeyChk           ; (3, 0)
//!   0x9be20  bl  CHAR::GetMainState
//!   0x9be28  cmp (r0 << 24), #0x1000000        ; walking?
//!   0x9be2c  beq 0x9be60                       ; -> SetStandMotion(GetDir())
//!   0x9be30  ...                               ; the tail, see below
//! ```
//!
//! So **every** key release clears all four movement flags and, if the hero
//! was walking, plants the standing animation. The call it discards is the one
//! that would have told it whether the key had anything to do with movement.
//! The author clearly meant the opposite: `WARRIOR::CheckSKillUse+0x1c0`
//! continues only while the main state is 1 (walking), i.e. attacking *while
//! moving* is a designed-for case.
//!
//! All four classes arrive here — `ROGUE::KeyRelease` (0xD8398) and
//! `WARRIOR::KeyRelease` (0x147444) are 4-byte `b HERO::KeyRelease` tail-call
//! thunks; `GUNNER::KeyRelease` (0x87E8C) and `KNIGHT::KeyRelease` (0xAAAC8)
//! `bl` it and then only correct one motion id afterwards. Nothing above
//! clears movement either: `Battle::KeyReleased` (0x4759C) just guards on
//! `IsSpiritSkillCutIn` and sets a byte, and `StateGame::KeyReleased`
//! (0xE291C) only forwards. One hook covers the whole game.
//!
//! # What the hooks do
//!
//! Two sites, because a tap-attack never reaches the first one's walking
//! branch. ANALYSIS.md §38.5 measured that and left the visible half to the
//! device; the device then showed the same "wiggle the stick to walk again"
//! report this file was opened for.
//!
//! 1. `HERO::KeyRelease`. It asks the guest the question the guest asked and
//!    ignored. If `StaticUtil::NumKeyToNaviKey` (0x1441C4) maps the key into
//!    −1..−4 it **declines**, so a direction release runs the shipped code,
//!    instruction for instruction. Otherwise it skips the four `SetMoveKeyChk`
//!    calls and the `SetStandMotion` branch, and reproduces the tail.
//! 2. `HERO::SetStandMotion`. A tap of OK / skill / dash puts the hero in
//!    attack (main state 2) *before* the matching release arrives, so
//!    KeyRelease's walking test is false and this function is who actually
//!    plants the stand — from the attack-end path, not from KeyRelease. The
//!    joystick is still held (`gv+0x125 == 1`) with a direction in
//!    `gv+0x11c`, but `TouchMoved` only sends `KeyPress` when that direction
//!    *changes*, which is why the player has to flick the stick to walk
//!    again.
//!
//!    The first version of this hook converted **every** `SetStandMotion`
//!    while the stick was held, and passed `gv+0x11c` (−1..−4) straight to
//!    `SetWalkMotion`. Both were wrong, and together they are the
//!    "dash-mash ghost" and "cannot walk for a moment after an attack":
//!
//!    * `SetWalkMotion` / `SetDir` / `SPRITE::GetLastFrame` take facing
//!      **0..3** (`GetInvertDir` is 0↔1, 2↔3; `KeyPress` writes that
//!      through `GetAngleToDir`). −1..−4 is the WIPI key, not a sprite
//!      facing. A facing of −1 has no frames, so the hero draws as empty
//!      (the "transparent" report) while still taking hits.
//!    * `SetStandMotion` is also how stun, hit-reaction, revival, and
//!      events plant a stand. Skipping those leaves combo / skill-index /
//!      extra-frame colour / feint / shadow-step dirty, and
//!      `WorkMoveHero` no-ops when `gv+0x278` is −1 (the first instruction
//!      is `cmn r3, #1; beq return`).
//!
//!    The hook now (a) answers only the class `EndProc` / `HERO::EndProc`
//!    sites that mean "this action finished", (b) runs the original
//!    `SetStandMotion` first so its cleanup happens, (c) restores a real
//!    walk angle, and (d) calls `SetWalkMotion` with 0..3.
//!
//! The touch UI matters here because it is all key codes underneath
//! (ANALYSIS.md §37.7): the joystick releases with `gv+0x11c`, the direction it
//! is currently in, so the stick's own release still stops the walk. The
//! buttons release with OK = −5, skill slots `'1' '3' '9' '*' '0'`, and combo
//! `'7'`. Checked against the mapping instruction by instruction: only
//! `'2' '4' '5' '6' '8'` are remapped at all (to −1, −3, −5, −4, −2), so no
//! button code can be mistaken for a direction, and `'3'` — a skill slot —
//! comes back unchanged as 0x33.
//!
//! # The tail is load-bearing
//!
//! ```text
//!   0x9be30  bl  HERO::GetPlayerClass
//!   0x9be3c  cmp (r0 << 24), #0x2000000        ; class 2
//!   0x9be40  bne 0x9be5c                       ; return
//!   0x9be44  r3 = 0x1f80 + 0x2d = 0x1fad
//!   0x9be4c  ldrb r2,[r4,r3]; cmp r2,#0; movne r2,#0; strbne r2,[r4,r3]
//! ```
//!
//! This runs on every key release regardless of the walking test, so skipping
//! the body wholesale would skip it too. A census of `.text` — every memory
//! access whose total constant displacement from its base register is 0x1fad,
//! at 100% decode coverage — finds exactly four sites, and they say what the
//! byte is:
//!
//! ```text
//!   GUNNER::KeyPress+0x4f4   (0x883B4)  strb #1  — set, entering the combo
//!   GUNNER::EndProc+0x2c0    (0x8733C)  ldrb, cmp #1 — read every frame
//!   HERO::SetStandMotion+0x100 (0x990E0) strb #0 — cleared on standing
//!   HERO::KeyRelease+0x80    (0x9BE4C)  strb #0 — cleared here
//! ```
//!
//! It is the gunslinger's "combo in progress" flag, and class 2 is the
//! gunslinger. Leave it set and `GUNNER::EndProc` takes its other branch for
//! ever after the first attack — a new bug in exchange for the one being
//! fixed. So the hook clears it exactly where and when the original does.

use arm32_cpu::{reg, Mode};

use crate::mem::RETURN_MAGIC;
use crate::patch::PatchFn;
use crate::Vm;

/// `HERO::KeyRelease(int)` — the hook's site, and where every class's key
/// release converges.
const KEY_RELEASE: &str = "_ZN4HERO10KeyReleaseEi";

/// `HERO::SetStandMotion(signed char)` — the function a tap-attack actually
/// reaches when it ends, and the one that clears `gv+0x278` to −1 (0x990EC).
const SET_STAND_MOTION: &str = "_ZN4HERO14SetStandMotionEa";

/// `HERO::SetWalkMotion(signed char)` — main state ← 1 (0x98F84). What the
/// stand hook calls when the joystick is still held.
const SET_WALK_MOTION: &str = "_ZN4HERO13SetWalkMotionEa";

/// `CHAR::GetMainState()` — 1 is walking, 0 standing, 2 attacking. Eight
/// bytes, no `bl`. Used only to refuse a re-entry if we are already walking.
const GET_MAIN_STATE: &str = "_ZN4CHAR12GetMainStateEv";

/// `HERO::GetAngleToDir(short)` — angle → facing 0..3. `KeyPress` feeds
/// `SetWalkMotion` this, never the WIPI key.
const GET_ANGLE_TO_DIR: &str = "_ZN4HERO13GetAngleToDirEs";

/// Class `EndProc`s whose `bl SetStandMotion` means the current action
/// finished. Offsets are the `bl` itself; the hook sees `lr = site + 4`.
/// GUNNER's function is the 1,532-byte one; the other three share a 1,008-
/// byte shape, which is why their two sites sit at the same offsets.
const END_PROC_STAND_SITES: &[(&str, u32)] = &[
    ("_ZN7WARRIOR7EndProcEv", 0x2c4),
    ("_ZN7WARRIOR7EndProcEv", 0x2f0),
    ("_ZN5ROGUE7EndProcEv", 0x2c4),
    ("_ZN5ROGUE7EndProcEv", 0x2f0),
    ("_ZN6KNIGHT7EndProcEv", 0x2c4),
    ("_ZN6KNIGHT7EndProcEv", 0x2f0),
    ("_ZN6GUNNER7EndProcEv", 0x2f8),
    ("_ZN6GUNNER7EndProcEv", 0x36c),
    ("_ZN4HERO7EndProcEv", 0x64),
];

/// Walking. `SetWalkMotion` writes this (0x98F84); `SetStandMotion` writes 0.
const MAIN_WALK: i32 = 1;

/// `gv+0x125` — touch-joystick latch, 1 while a finger owns it.
const GV_STICK_HELD: u32 = 0x125;

/// `gv+0x11c` — the stick's current direction word: −1..−4, or −999 in the
/// deadzone. The stick's own release sends this word as the key, which is
/// why a real stick-up still stops the walk.
const GV_STICK_DIR: u32 = 0x11c;

/// `gv+0x278` — 5° walk angle. `WorkMoveHero` returns immediately on −1
/// (`cmn r3, #1` at 0x8D024). `SetStandMotion` writes −1 here (0x990EC).
const GV_WALK_ANGLE: u32 = 0x278;

/// `StaticUtil::NumKeyToNaviKey(int)` — static, so the key is r0. A 7-entry
/// jump table over `key - '2'`; every other code, negative ones included,
/// falls through and comes back unchanged.
const NUM_KEY_TO_NAVI: &str = "_ZN10StaticUtil15NumKeyToNaviKeyEi";

/// `HERO::GetPlayerClass()` — `ldrb [this,#0x22c]` sign-extended. Also the
/// class the relic boxes gate their weapon draws on (`shop::hero_class`).
pub(crate) const GET_PLAYER_CLASS: &str = "_ZN4HERO14GetPlayerClassEv";

/// The class whose combo flag the tail clears. See the module comment.
const GUNNER: i32 = 2;

/// `HERO + 0x1fad`, the gunslinger's combo-in-progress flag. Four sites in
/// `.text` touch it and they are all listed in the module comment; there is no
/// guest accessor to call instead.
const GUNNER_COMBO_FLAG: u32 = 0x1fad;

/// WIPI navigation keys: UP −1, DOWN −2, LEFT −3, RIGHT −4. `SELECT` is −5 and
/// deliberately outside this range — it is the OK button.
const NAVI_KEYS: std::ops::RangeInclusive<i32> = -4..=-1;

/// Install the hook, or report that this binary cannot carry it.
///
/// Every symbol is checked up front so a missing one is a sentence at boot
/// rather than a feature that silently is not there.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for name in [
        KEY_RELEASE,
        NUM_KEY_TO_NAVI,
        GET_PLAYER_CLASS,
        SET_STAND_MOTION,
        SET_WALK_MOTION,
        GET_MAIN_STATE,
        GET_ANGLE_TO_DIR,
        crate::warehouse::GV,
    ] {
        if vm.symbol(name).is_none() {
            return Err(format!("keep-walking hook needs `{name}`, not in this binary"));
        }
    }
    for &(name, _) in END_PROC_STAND_SITES {
        if vm.symbol(name).is_none() {
            return Err(format!("keep-walking hook needs `{name}`, not in this binary"));
        }
    }
    let release = vm.symbol(KEY_RELEASE).expect("checked above");
    let stand = vm.symbol(SET_STAND_MOTION).expect("checked above");
    let f_release: PatchFn = keep_walking_on_action_key;
    let f_stand: PatchFn = walk_if_stick_held;
    vm.install_hook(KEY_RELEASE, release, f_release)?;
    vm.install_hook(SET_STAND_MOTION, stand, f_stand)?;
    Ok(vec![KEY_RELEASE, SET_STAND_MOTION])
}

/// `HERO::KeyRelease(int key)` — `(this, key)`.
///
/// # Why there is no return-address filter
///
/// The other behaviour hooks in this crate key on `lr` to pick out one call
/// site. This one must not: its four callers arrive with three different
/// return addresses. `ROGUE::KeyRelease` and `WARRIOR::KeyRelease` are `b`
/// thunks, so `lr` is still `Battle::KeyReleased`'s 0x475FC, while
/// `GUNNER::KeyRelease` and `KNIGHT::KeyRelease` `bl` and leave 0x87E98 and
/// 0xAAAD4. A single-site test would cover one class in four.
///
/// # Why it cannot recurse
///
/// It calls back into the guest twice, and `Vm::call` would re-enter this hook
/// if either callee could reach `HERO::KeyRelease`. Neither can, and not by
/// luck: `StaticUtil::NumKeyToNaviKey` (0x1441C4, a jump table and seven
/// `bx lr`) and `HERO::GetPlayerClass` (0x88914, four instructions) contain
/// **zero** `bl` between them. Keep it that way — a callee with a call in it
/// would recurse until the interpreter's instruction budget killed the run.
fn keep_walking_on_action_key(vm: &mut Vm) -> Result<bool, String> {
    let this = vm.arg(0);
    let key = vm.arg(1);
    let (Some(navi_fn), Some(class_fn)) =
        (vm.symbol(NUM_KEY_TO_NAVI), vm.symbol(GET_PLAYER_CLASS))
    else {
        // `install` refuses without them, so this is unreachable; declining is
        // still the right answer for "cannot tell", because it runs the game
        // as shipped.
        return Ok(false);
    };
    // The guest's own mapping, called rather than transcribed: which key is a
    // direction is the game's business, and the table has a fall-through that
    // is easy to get wrong by hand.
    let navi = vm.call(navi_fn, &[key])? as i32;
    if NAVI_KEYS.contains(&navi) {
        return Ok(false);
    }
    // Not a direction: everything the original does to movement is skipped.
    // What is left is the tail, which is not about movement at all.
    let class = vm.call(class_fn, &[this])? as i32;
    if class == GUNNER {
        let flag = this + GUNNER_COMBO_FLAG;
        if vm.mem.read_u8(flag) != 0 {
            vm.mem.write_bytes(flag, &[0]);
        }
    }
    Ok(true)
}

/// Direction the touch joystick is currently in, if a finger still owns it.
///
/// `None` is "do not override the stand": stick up, deadzone (−999), or no
/// `gv`. The stand hook declines in all three, so a menu, a death, or a real
/// stick-up still plants the standing animation.
fn held_stick_dir(vm: &Vm) -> Option<i32> {
    let gv = vm.symbol(crate::warehouse::GV)?;
    if vm.mem.read_u8(gv + GV_STICK_HELD) != 1 {
        return None;
    }
    let dir = vm.mem.read_u32(gv + GV_STICK_DIR) as i32;
    NAVI_KEYS.contains(&dir).then_some(dir)
}

/// WIPI navi key → sprite facing 0..3.
///
/// `GetInvertDir` is 0↔1 (vertical) and 2↔3 (horizontal). `WorkMoveHero`
/// plants those same four when the analog angle is near a cardinal: 270°→0
/// (UP), 90°→1 (DOWN), 0°→2 (RIGHT), 180°→3 (LEFT). The joystick writes
/// −1..−4 into `gv+0x11c`; feeding those to `SetWalkMotion` is what made
/// the hero vanish.
fn navi_to_face(navi: i32) -> i32 {
    match navi {
        -1 => 0, // UP
        -2 => 1, // DOWN
        -3 => 3, // LEFT
        -4 => 2, // RIGHT
        other => other,
    }
}

/// Cardinal angle `WorkMoveHero` and `GetAngleToDir` agree on for a facing.
fn face_to_angle(face: i32) -> i16 {
    match face {
        0 => 270,
        1 => 90,
        2 => 0,
        3 => 180,
        _ => 0,
    }
}

/// `lr` is one of the `EndProc` `bl SetStandMotion` return addresses.
fn walk_resume_site(vm: &Vm, lr: u32) -> bool {
    END_PROC_STAND_SITES.iter().any(|&(name, off)| {
        vm.symbol(name).is_some_and(|s| lr == s + off + 4)
    })
}

fn walk_angle(vm: &Vm) -> Option<i16> {
    let gv = vm.symbol(crate::warehouse::GV)?;
    Some(vm.mem.read_u16(gv + GV_WALK_ANGLE) as i16)
}

fn set_walk_angle(vm: &mut Vm, angle: i16) {
    let Some(gv) = vm.symbol(crate::warehouse::GV) else {
        return;
    };
    vm.mem.write_u16(gv + GV_WALK_ANGLE, angle as u16);
}

/// `HERO::SetStandMotion(signed char dir)` — `(this, dir)`.
///
/// Only the `EndProc` sites that mean "this motion finished" are converted,
/// and only while the stick is held. Stun, hit-reaction, revival, events,
/// and `CheckSKillUse` decline so the original stand (and its cleanup) run.
///
/// When it does convert, the original function runs first — that is what
/// clears combo / skill-index / extra-frame colour / feint / shadow-step —
/// then `SetWalkMotion` is called with facing 0..3 and `gv+0x278` is put
/// back so `WorkMoveHero` actually steps.
fn walk_if_stick_held(vm: &mut Vm) -> Result<bool, String> {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if lr == RETURN_MAGIC {
        return Ok(false);
    }
    let this = vm.arg(0);
    let Some(navi) = held_stick_dir(vm) else {
        return Ok(false);
    };
    if !walk_resume_site(vm, lr) {
        return Ok(false);
    }
    let (Some(walk), Some(get_state), Some(stand), Some(angle_to_dir)) = (
        vm.symbol(SET_WALK_MOTION),
        vm.symbol(GET_MAIN_STATE),
        vm.symbol(SET_STAND_MOTION),
        vm.symbol(GET_ANGLE_TO_DIR),
    ) else {
        return Ok(false);
    };
    let state = vm.call(get_state, &[this])? as i32;
    if state == MAIN_WALK {
        return Ok(true);
    }
    let saved_angle = walk_angle(vm).unwrap_or(-1);
    let stand_dir = vm.arg(1);
    vm.call(stand, &[this, stand_dir])?;
    let face = if saved_angle != -1 {
        vm.call(angle_to_dir, &[this, saved_angle as u16 as u32])? as i32
    } else {
        navi_to_face(navi)
    };
    set_walk_angle(
        vm,
        if saved_angle != -1 {
            saved_angle
        } else {
            face_to_angle(face)
        },
    );
    vm.call(walk, &[this, face as u32])?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `StaticUtil::NumKeyToNaviKey` (0x1441C4) transcribed, for the tests
    /// only — the hook itself calls the guest's copy, because which key counts
    /// as a direction is the game's business. Keeping a transcription here is
    /// what lets the classification be pinned without a loaded binary.
    ///
    /// ```text
    ///   0x1441c4  sub   r3, r0, #0x32          ; '2'
    ///   0x1441c8  cmp   r3, #6                 ; UNSIGNED
    ///   0x1441cc  addls pc, pc, r3, lsl #2     ; case 0 is 0x1441d4
    ///   0x1441d0  b     0x1441f4               ; fall-through: bx lr, unchanged
    /// ```
    ///
    /// The unsigned compare is the whole subtlety: a negative key (every WIPI
    /// code) makes `r3` enormous, fails `cmp r3, #6`, and falls through
    /// unchanged. That is why OK (−5) arrives as −5 and not as a direction.
    fn num_key_to_navi_key(key: i32) -> i32 {
        match key.wrapping_sub(0x32) as u32 {
            0 => -1, // '2' -> UP
            2 => -3, // '4' -> LEFT
            3 => -5, // '5' -> SELECT
            4 => -4, // '6' -> RIGHT
            6 => -2, // '8' -> DOWN
            // cases 1 ('3') and 5 ('7') branch to the same `bx lr` as the
            // fall-through, so they come back unchanged.
            _ => key,
        }
    }

    #[test]
    fn the_transcription_matches_the_jump_table() {
        // The five remapped numpad keys, as read off 0x1441d4..0x144214.
        assert_eq!(num_key_to_navi_key(0x32), -1, "'2' is UP");
        assert_eq!(num_key_to_navi_key(0x34), -3, "'4' is LEFT");
        assert_eq!(num_key_to_navi_key(0x35), -5, "'5' is SELECT");
        assert_eq!(num_key_to_navi_key(0x36), -4, "'6' is RIGHT");
        assert_eq!(num_key_to_navi_key(0x38), -2, "'8' is DOWN");
        // The two table slots that share the fall-through's `bx lr`.
        assert_eq!(num_key_to_navi_key(0x33), 0x33, "'3' is not remapped");
        assert_eq!(num_key_to_navi_key(0x37), 0x37, "'7' is not remapped");
    }

    /// The d-pad, and only the d-pad, must make the hook decline.
    #[test]
    fn the_four_directions_are_the_only_declines() {
        for key in [-1, -2, -3, -4] {
            let navi = num_key_to_navi_key(key);
            assert_eq!(navi, key, "a WIPI direction falls through unchanged");
            assert!(NAVI_KEYS.contains(&navi), "key {key} must run the shipped code");
        }
    }

    /// Every key the touch UI can emit, and the two hardware keys that reach
    /// gameplay. None may be mistaken for a direction — if one were, the hook
    /// would decline and the fix would silently not apply to that button.
    ///
    /// Codes from `StateGame::TouchReleased`: OK −5 (0xE19B0), skill slots
    /// `'1' '3' '9' '*' '0'` (0xE1A6C / 0xE1A54 / 0xE1A24 / 0xE1A0C / 0xE19F4)
    /// and the combo key `'7'` (0xE1A3C). `'3'` is the interesting one: it sits
    /// inside the jump table's range and still comes back unchanged.
    #[test]
    fn no_action_key_looks_like_a_direction() {
        let action = [
            -5,   // OK
            -16,  // BACK (WIPI CLEAR)
            b'1' as i32,
            b'3' as i32,
            b'9' as i32,
            b'*' as i32,
            b'0' as i32,
            b'7' as i32, // combo
        ];
        for key in action {
            let navi = num_key_to_navi_key(key);
            assert!(
                !NAVI_KEYS.contains(&navi),
                "key {key} mapped to {navi}, which the hook would treat as movement"
            );
        }
    }

    /// The joystick releases with `gv+0x11c`, its current direction, and the
    /// stick's own release must still stop the walk. Every writer of that word
    /// was swept: −1 (0xE1648), −4 (0xE16A4), −2 (0xE1734), −3 (0xE1774) and
    /// −999 (0xE17FC, 0xE1948). The four directions decline; −999 is the
    /// "never left the deadzone" value, and suppressing that release is
    /// harmless because no direction was ever pressed.
    #[test]
    fn the_joysticks_own_release_still_stops_the_walk() {
        for dir in [-1, -2, -3, -4] {
            assert!(NAVI_KEYS.contains(&num_key_to_navi_key(dir)));
        }
        assert!(!NAVI_KEYS.contains(&num_key_to_navi_key(-999)), "the idle value is not a direction");
    }

    /// The stand hook converts to a walk only for a latched stick in a real
    /// direction. −999 is the deadzone word; treating it as a walk would
    /// plant a walk with no KeyPress and no angle.
    #[test]
    fn stand_becomes_walk_only_for_a_held_direction() {
        for dir in [-1, -2, -3, -4] {
            assert!(NAVI_KEYS.contains(&dir), "stick dir {dir} must resume the walk");
        }
        assert!(!NAVI_KEYS.contains(&-999));
        assert!(!NAVI_KEYS.contains(&0));
    }

    /// `SetWalkMotion` / `GetInvertDir` / `WorkMoveHero` share facing 0..3.
    /// Passing the joystick's WIPI key (−1..−4) is what made the hero
    /// vanish after a dash — `SPRITE::GetLastFrame` has no frames for −1.
    #[test]
    fn navi_keys_become_the_four_facings() {
        assert_eq!(navi_to_face(-1), 0, "UP");
        assert_eq!(navi_to_face(-2), 1, "DOWN");
        assert_eq!(navi_to_face(-3), 3, "LEFT");
        assert_eq!(navi_to_face(-4), 2, "RIGHT");
        // Invert pairs match `CHAR::GetInvertDir`.
        assert_eq!(navi_to_face(-1) ^ 1, navi_to_face(-2));
        assert_eq!(navi_to_face(-4) ^ 1, navi_to_face(-3));
        for navi in [-1, -2, -3, -4] {
            let face = navi_to_face(navi);
            assert!((0..=3).contains(&face), "navi {navi} must not reach SetWalkMotion");
        }
    }

    /// Cardinal angles `GetAngleToDir` and `WorkMoveHero` both snap to.
    #[test]
    fn face_angles_are_the_cardinals_work_move_hero_uses() {
        assert_eq!(face_to_angle(0), 270);
        assert_eq!(face_to_angle(1), 90);
        assert_eq!(face_to_angle(2), 0);
        assert_eq!(face_to_angle(3), 180);
    }
}
