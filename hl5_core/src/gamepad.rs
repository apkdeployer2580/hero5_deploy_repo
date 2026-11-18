//! Gamepad input — what a pad button does to this guest, and why the stick
//! cannot be a key.
//!
//! # The two halves, and the measurement that split them
//!
//! The in-game touch UI in this game is **a key synthesiser**. Every one of
//! its buttons calls the state's own `KeyPress` / `KeyRelease` with a WIPI
//! code and nothing else (`StateGame::TouchPressed` 0xE249C.., release side
//! 0xE19E4..):
//!
//! | on-screen control | WIPI key | where |
//! |---|---|---|
//! | OK | −5 | 0xE1B94 press / 0xE19A8 release |
//! | belt slot 1..5 | `'1' '3' '9' '*' '0'` | 0xE2514 / 0xE24FC / 0xE24CC / 0xE24B4 / 0xE249C |
//! | combo | `'7'` | 0xE24E4 |
//! | belt page | `'#'` | 0xE285C |
//!
//! So a pad **button** needs no hook at all: send the same key and the game
//! cannot tell a thumb from a trigger. [`default_button_action`] is that
//! table.
//!
//! The **stick is different, and this is the finding this module exists for.**
//! Walking is not key-driven in this build. `WARRIOR::Proc+0x228` (and the
//! same three instructions in `ROGUE`/`GUNNER`/`KNIGHT`) passes
//! `(s16)gv+0x278` to `HERO::WorkMoveHero`, and that function's **first**
//! test is `cmn r3, #1` → `pop {pc}` (0x8D024): an angle of −1 moves the hero
//! zero pixels. The four class `KeyPress` handlers hand
//! `HERO::SetAngleMoveKeyChk` that same `gv+0x278` (0x147858..0x147868), not
//! the key they were given — they use the key only for the facing and the
//! double-tap dash. And `gv+0x278` is written in exactly four places, all of
//! them touch: `StateGame::UpdateNaviAngle` (whose only two callers are
//! `TouchMoved+0xfc` and `TouchPressed+0x700`), `TouchMoved`, `TouchReleased`,
//! `TouchPressed`, plus `HERO::SetStandMotion+0x104` which clears it to −1.
//!
//! > **A direction key therefore animates a walk that never moves.** That is
//! > not a shell bug: the shipped Android port drives movement from the touch
//! > joystick's angle, and a pad that sends −1..−4 walks on the spot.
//!
//! The fix is to give the guest what it actually reads: **a synthetic finger
//! on its own joystick.** [`Pad::axes`] presses at the point both touch modes
//! treat as the stick centre and then drags it, so `UpdateNaviAngle` writes a
//! real 5°-quantised angle, `TouchMoved` emits the direction `KeyPress` the
//! walk state needs, the handle is drawn where the stick is pointing, and the
//! dash arming the guest does on every angle change keeps happening. Nothing
//! is hooked and nothing in guest memory is written by us; the events go in
//! through [`crate::Vm::touch`], the same door a thumb uses.
//!
//! # Why the pad has its own pointer id
//!
//! The joystick has an owning finger (`gv+0x124`, ANALYSIS.md §37.5) and only
//! that finger may let go of it. The synthetic finger takes [`PAD_POINTER`], a
//! slot the shell's real-finger allocator counts up from 0 and will never
//! reach, so a real thumb can press a skill button without releasing the pad's
//! stick and vice versa.
//!
//! # Why events are queued instead of sent
//!
//! Guest input is **one mailbox slot per kind** — `gv+0xE4` key-down,
//! `gv+0xE5` key-up, `gv+0xF0/0xF1/0xF2` touch press/release/move — and
//! `BaseState::KeyPress` (0x44CBC) records only while its slot is still empty,
//! dropping anything that arrives second in the same tick (§37.8). A pad can
//! easily produce two edges inside one 36 ms tick, and a dropped skill press
//! is exactly the "패드가 씹힌다" a player would report. So every pad edge is
//! queued and [`crate::Vm::pad_pump`] hands over **one per free slot**, which
//! makes the queue drain at the guest's own rate instead of overwriting
//! itself.
//!
//! # Mapping is data
//!
//! [`button_action_in`] consults the player's `hl5_gamepad.txt` before the
//! defaults, so any pad layout is one text file away from working without a
//! rebuild — and because the HUD badges are a *reverse* lookup of the same
//! table ([`pos_for_key`]), a remap moves the badge with the binding, as far
//! as a badge can go: [`pos_for_action`] says where that stops.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::padhud::{pos_for_code, PadPos};
use crate::Touch;

/// Android `KeyEvent` keycodes. API-frozen constants, and **positional**
/// despite the names: Android's generic key layout maps `BTN_SOUTH` to
/// `BUTTON_A`, `BTN_EAST` to `BUTTON_B`, `BTN_NORTH` to `BUTTON_Y` and
/// `BTN_WEST` to `BUTTON_X`, so 96 is the bottom face button on every
/// standard pad — printed `A` on an Xbox pad, `B` on a Switch pad and `✕` on
/// a PlayStation pad. That is why the badges draw a position and never a
/// letter (see [`crate::padhud`]).
pub const KEYCODE_BACK: i32 = 4;
pub const KEYCODE_DPAD_UP: i32 = 19;
pub const KEYCODE_DPAD_DOWN: i32 = 20;
pub const KEYCODE_DPAD_LEFT: i32 = 21;
pub const KEYCODE_DPAD_RIGHT: i32 = 22;
pub const KEYCODE_DPAD_CENTER: i32 = 23;
pub const KEYCODE_BUTTON_A: i32 = 96;
pub const KEYCODE_BUTTON_B: i32 = 97;
pub const KEYCODE_BUTTON_X: i32 = 99;
pub const KEYCODE_BUTTON_Y: i32 = 100;
pub const KEYCODE_BUTTON_L1: i32 = 102;
pub const KEYCODE_BUTTON_R1: i32 = 103;
pub const KEYCODE_BUTTON_L2: i32 = 104;
pub const KEYCODE_BUTTON_R2: i32 = 105;
pub const KEYCODE_BUTTON_THUMBL: i32 = 106;
pub const KEYCODE_BUTTON_THUMBR: i32 = 107;
pub const KEYCODE_BUTTON_START: i32 = 108;
pub const KEYCODE_BUTTON_SELECT: i32 = 109;

/// The WIPI keys this game answers in the field, named after what the control
/// that sends them is called on screen. Codes are the guest's own: the
/// negatives are WIPI navigation / soft keys, the positives are the phone
/// keypad's ASCII.
pub const KEY_UP: i32 = -1;
pub const KEY_DOWN: i32 = -2;
pub const KEY_LEFT: i32 = -3;
pub const KEY_RIGHT: i32 = -4;
/// OK — confirm in menus, attack in the field.
pub const KEY_OK: i32 = -5;
/// The map cell of the in-game bar. `Battle::KeyPressed+0x158` →
/// `BaseState::StateMgr(2, 7)`.
pub const KEY_MAP: i32 = -6;
/// The shop cell of the in-game bar (0x48EE8).
pub const KEY_SHOP: i32 = -7;
/// The save cell of the in-game bar: `HERO::SaveAll` + the guest's own
/// "저장했습니다" system message (0x49010).
pub const KEY_SAVE: i32 = -10;
/// CLEAR / BACK — cancel in menus, the character sheet in the field.
pub const KEY_CLEAR: i32 = -16;
/// Belt slots 1..5, in the order `HERO::GetHotKeyBeltType`'s first argument
/// takes them: `'1'`→0, `'3'`→1, `'9'`→2, `'*'`→3, `'0'`→4
/// (`WARRIOR::KeyPress` 0x147754 / 0x14770C / 0x147620 / 0x1477E4 / 0x14779C).
pub const KEY_SLOT: [i32; 5] = [0x31, 0x33, 0x39, 0x2a, 0x30];
/// The combo / secondary skill (`HERO::GetComboSkillBelt`, 0x1476B4).
pub const KEY_COMBO: i32 = 0x37;
/// Flip the belt page. `HERO::ChangeHotKeyToggle` (0x88B80) is
/// `toggle = 1 - toggle`, and every slot reads
/// `HERO[0x1b4c + slot*2 + toggle]` — so the belt is **5 slots × 2 pages**
/// and this is the only key that moves between them.
pub const KEY_PAGE: i32 = 0x23;

/// The synthetic finger's pointer id.
///
/// The guest keeps the joystick's owner in a signed byte and compares it
/// sign-extended, with −99 as "nobody" (`StateGame::TouchReleased+0xdc`), so
/// this has to be a small non-negative number. The shell hands real fingers
/// slots counting up from 0 (`App::touch_slot`), which is why this sits at the
/// top of the range instead of at 1: a phone would have to report 113
/// simultaneous fingers to collide with it.
pub const PAD_POINTER: i32 = 0x70;

/// How far from the stick centre a fully deflected stick is injected, in
/// canvas pixels.
///
/// The guest latches a walking direction only past a ±10 px deadzone
/// (`StateGame::TouchMoved+0x130`: `cmn dy, #0xa` / `cmp dx, #0xa`), so the
/// radius has to be big enough that even [`RELEASE`] clears it — 48 × 0.25 =
/// 12 px — and small enough to stay on the canvas from a centre 65 px in.
const RADIUS: f32 = 48.0;
/// The guest's own joystick deadzone, as a fraction of [`RADIUS`]: it latches
/// a direction only past ±10 px (`StateGame::TouchMoved+0x130`).
const GUEST_DEAD: f32 = 10.0 / RADIUS;
/// Deflection that puts the synthetic finger down, and the smaller one that
/// keeps it down. Two values so a stick resting on the threshold cannot
/// chatter the joystick on and off.
///
/// # Why [`RELEASE`] is not small
///
/// **The lower number is a deadzone, not a nicety.** Java hands us
/// `MotionEvent.getAxisValue` and a great many Bluetooth pads never report a
/// resting stick as 0 — they sit at a tenth or two of full deflection for
/// ever, and the ones that do declare their own slop in
/// `InputDevice.MotionRange.getFlat()` are exactly the ones the Java side can
/// already flatten. So [`RELEASE`] has to clear the *undeclared* drift of the
/// rest, or a stick that is let go keeps the synthetic finger down and the
/// hero walks until something else releases it. That is the failure mode to
/// test for first on a device: not "does the stick walk" but **"does it
/// stop"**.
///
/// Both numbers also sit above [`GUEST_DEAD`], which buys a second property:
/// while the finger is down it is always past the guest's own ±10 px
/// threshold, so a finger that is down always steers. There is no band where
/// the hero is holding a direction it cannot act on.
const RELEASE: f32 = GUEST_DEAD + 0.04;
const ENGAGE: f32 = RELEASE + 0.10;
/// Where the finger presses. Both touch schemes end up treating this as the
/// stick centre: the fixed one writes it (`gv+0x274/0x276 = 0x41, H−0x41`,
/// 0xE2150) and the floating one clamps a press to `x ≥ 0x2d`, `y ≥ 0x41`
/// (0xE1C68) and then writes the press point there itself. Pressing exactly
/// on it means the two schemes see the same gesture.
const CENTRE_X: i32 = 0x41;
const CENTRE_INSET_Y: i32 = 0x41;

/// Where the stick centre is on a canvas `h` tall.
pub fn stick_centre(h: i32) -> (i32, i32) {
    (CENTRE_X, h - CENTRE_INSET_Y)
}

/// What a mapped control does.
///
/// Unlike the sibling projects' pad layer there is no host-side action here:
/// the belt page, the map and the save this game does itself, and the host
/// only has to name the key. [`PadAction::Dash`] is the one exception, and it
/// is still the guest's own mechanism — see [`Pad::button`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadAction {
    /// Press and release a guest key.
    Key(i32),
    /// Dash, through the guest's own double-tap detector.
    Dash,
}

/// Every keycode this module can bind or badge, in PHYSICAL order.
/// [`pos_for_action`] scans it in this order, so a key reachable from two
/// codes is badged with the first — OK is on both the bottom face button and
/// the hat's centre, and the face button is the one a player pictures.
const ALL_CODES: [i32; 18] = [
    KEYCODE_BUTTON_A,
    KEYCODE_BUTTON_B,
    KEYCODE_BUTTON_X,
    KEYCODE_BUTTON_Y,
    KEYCODE_BUTTON_L1,
    KEYCODE_BUTTON_R1,
    KEYCODE_BUTTON_L2,
    KEYCODE_BUTTON_R2,
    KEYCODE_BUTTON_SELECT,
    KEYCODE_BUTTON_START,
    KEYCODE_BUTTON_THUMBL,
    KEYCODE_BUTTON_THUMBR,
    KEYCODE_DPAD_UP,
    KEYCODE_DPAD_DOWN,
    KEYCODE_DPAD_LEFT,
    KEYCODE_DPAD_RIGHT,
    KEYCODE_DPAD_CENTER,
    KEYCODE_BACK,
];

/// The built-in layout: hero3's, with **two deliberate moves** and the reason
/// for both in one sentence — *a pad may have no sticks to press.*
///
/// hero3's layout assumes a full pad: it puts its quickbar page on R3 and its
/// 빠른저장 on L3. Plenty of Bluetooth pads have no analog sticks at all, and
/// on those `KEYCODE_BUTTON_THUMBL/THUMBR` do not exist — so anything living
/// only there is *unreachable*. The belt page is the one control the operator
/// asked for by name, which makes "unreachable on some pads" the wrong place
/// for it. So:
///
/// * **SELECT is the belt page** (hero3: the map), because SELECT exists on
///   every pad family.
/// * **START is the dash** (hero3: a second CLEAR, which B and BACK already
///   cover), because it is then the only control left that costs nothing.
/// * R3 takes the map and L3 keeps the save — both a *bonus* for pads that
///   have sticks, and neither is time-critical: the in-game top bar draws
///   `map shop save` on every field screen, so a pad without sticks reaches
///   them with a tap and loses nothing but the convenience.
///
/// Everything else is hero3 button for button: A confirms and attacks, B
/// cancels, the four remaining face/shoulder buttons are belt slots 1-4, L2 is
/// slot 5 and R2 the secondary (here: combo) skill. What changed is only what
/// the key *is*: hero3's quickbar hotkeys were `1/3/9/0/#`, hero5's are
/// `1/3/9/*/0`, and `#` is the page flip — a host feature in hero3, the
/// game's own `HERO::ChangeHotKeyToggle` here.
///
/// [`the_whole_field_kit_avoids_the_stick_presses`] is the test that keeps
/// this property true; see it before moving anything back.
pub fn default_button_action(code: i32) -> Option<PadAction> {
    Some(match code {
        KEYCODE_DPAD_UP => PadAction::Key(KEY_UP),
        KEYCODE_DPAD_DOWN => PadAction::Key(KEY_DOWN),
        KEYCODE_DPAD_LEFT => PadAction::Key(KEY_LEFT),
        KEYCODE_DPAD_RIGHT => PadAction::Key(KEY_RIGHT),
        KEYCODE_DPAD_CENTER | KEYCODE_BUTTON_A => PadAction::Key(KEY_OK),
        // BACK here is the gamepad-source BACK some pads send for B or
        // SELECT. The phone's own back key does not come through this table.
        KEYCODE_BUTTON_B | KEYCODE_BACK => PadAction::Key(KEY_CLEAR),
        KEYCODE_BUTTON_X => PadAction::Key(KEY_SLOT[0]),
        KEYCODE_BUTTON_Y => PadAction::Key(KEY_SLOT[1]),
        KEYCODE_BUTTON_L1 => PadAction::Key(KEY_SLOT[2]),
        KEYCODE_BUTTON_R1 => PadAction::Key(KEY_SLOT[3]),
        KEYCODE_BUTTON_L2 => PadAction::Key(KEY_SLOT[4]),
        KEYCODE_BUTTON_R2 => PadAction::Key(KEY_COMBO),
        KEYCODE_BUTTON_SELECT => PadAction::Key(KEY_PAGE),
        KEYCODE_BUTTON_START => PadAction::Dash,
        KEYCODE_BUTTON_THUMBR => PadAction::Key(KEY_MAP),
        // A key that writes the save file should be hard to hit by accident,
        // which is why hero3 chose a stick press for it and why it stays here.
        KEYCODE_BUTTON_THUMBL => PadAction::Key(KEY_SAVE),
        _ => return None,
    })
}

/// The spirit wheel's compass points, by the button that stands under each.
///
/// `HERO::UseSpiritSkill` is reached by picking one of four directions while
/// the wheel is up: `<CLASS>::KeyPress` calls `HERO::IsSpiritSkillMenu` and,
/// when it answers yes, accepts **only** WIPI −1..−4 (into
/// `HERO::SetSpiritSkillIndex`, 0x94AC8) and −16 (cancel). Every other key —
/// OK, the belt, the combo — returns on the spot (0x147490 and the same three
/// lines in the other classes), so a button borrowed here costs nothing: it
/// did nothing while the wheel was up.
///
/// The wheel draws its four skills north/south/east/west, so the mapping is
/// positional. **B keeps CLEAR**, because that is the cancel the player
/// already has, which is why east is on the right shoulder and not on the
/// right face button.
///
/// The four skills themselves live at `HERO+0x1792..+0x1795` in
/// UP/DOWN/LEFT/RIGHT order, and picking one *casts* it — this is not a
/// cursor. An empty slot is a no-op and the wheel stays up.
fn spirit_face(code: i32) -> Option<i32> {
    Some(match code {
        KEYCODE_BUTTON_Y => KEY_UP,
        KEYCODE_BUTTON_A => KEY_DOWN,
        KEYCODE_BUTTON_X => KEY_LEFT,
        KEYCODE_BUTTON_R1 => KEY_RIGHT,
        _ => return None,
    })
}

/// Resolve an Android keycode against the player's overrides, then the
/// built-in layout.
pub fn button_action_in(
    code: i32,
    overrides: &HashMap<i32, Option<PadAction>>,
) -> Option<PadAction> {
    match overrides.get(&code) {
        Some(over) => *over,
        None => default_button_action(code),
    }
}

/// Which pad position fires `action` under `overrides` — the reverse of
/// [`button_action_in`], and the whole basis of the HUD badges. Reverse
/// lookup and not a second table, so a badge can never claim a button that
/// does something else.
///
/// # What a badge cannot say
///
/// The search is over [`ALL_CODES`], which is every keycode
/// [`pos_for_code`] knows a *place* for. Bind an action to some other
/// keycode — a pad's extra paddle, a keyboard key — and it will work, because
/// [`button_action_in`] resolves any code at all; it simply gets no badge,
/// there being no position on a pad to point at. So "a remap moves the badge
/// with the binding" is true within the standard pad, and outside it the
/// binding moves alone.
pub fn pos_for_action(
    action: PadAction,
    overrides: &HashMap<i32, Option<PadAction>>,
) -> Option<PadPos> {
    ALL_CODES
        .iter()
        .filter(|c| button_action_in(**c, overrides) == Some(action))
        .find_map(|c| pos_for_code(*c))
}

/// Which pad position sends this guest key.
pub fn pos_for_key(key: i32, overrides: &HashMap<i32, Option<PadAction>>) -> Option<PadPos> {
    pos_for_action(PadAction::Key(key), overrides)
}

/// One `hl5_gamepad.txt` line: `<android keycode>=<TOKEN>`, with `#` starting
/// a comment **only at the beginning of a line** — `#` is also a token, being
/// the belt page key.
///
/// Tokens: `UP DOWN LEFT RIGHT OK CLR MAP SHOP SAVE DASH NONE`, the digits
/// `0`..`9`, `#` and `*`. `NONE` swallows the button. Garbage is ignored
/// rather than fatal: a typo in a text file must never stop the game booting.
pub fn parse_map_line(line: &str) -> Option<(i32, Option<PadAction>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (code, token) = line.split_once('=')?;
    let code: i32 = code.trim().parse().ok()?;
    let token = token.trim();
    let action = match token.to_ascii_uppercase().as_str() {
        "UP" => Some(PadAction::Key(KEY_UP)),
        "DOWN" => Some(PadAction::Key(KEY_DOWN)),
        "LEFT" => Some(PadAction::Key(KEY_LEFT)),
        "RIGHT" => Some(PadAction::Key(KEY_RIGHT)),
        "OK" => Some(PadAction::Key(KEY_OK)),
        "CLR" | "CLEAR" | "BACK" => Some(PadAction::Key(KEY_CLEAR)),
        "MAP" => Some(PadAction::Key(KEY_MAP)),
        "SHOP" => Some(PadAction::Key(KEY_SHOP)),
        "SAVE" => Some(PadAction::Key(KEY_SAVE)),
        "PAGE" => Some(PadAction::Key(KEY_PAGE)),
        "DASH" => Some(PadAction::Dash),
        "NONE" => None,
        // The keypad, by the character on the phone's own button.
        other => {
            let mut it = other.chars();
            let c = it.next()?;
            if it.next().is_some() {
                return None;
            }
            match c {
                '0'..='9' | '#' | '*' => Some(PadAction::Key(c as i32)),
                _ => return None,
            }
        }
    };
    Some((code, action))
}

/// Parse a whole `hl5_gamepad.txt`, and say which lines could not be read.
///
/// **This is the only definition of what the file means**, and it has to stay
/// that way: the shell used to re-implement it, splitting the bindings and the
/// `NONE`s into two collections and applying one after the other, which
/// quietly made `NONE` win over any binding for the same code no matter what
/// order the player wrote them in. One map, filled in file order, so the last
/// line about a code is the one in force — the rule a text file leads anyone
/// to expect.
///
/// Rejected lines come back as `(1-based line number, the line)` for the
/// caller to log. They are never fatal: a typo must not stop the game booting.
pub fn parse_map_reporting(text: &str) -> (HashMap<i32, Option<PadAction>>, Vec<(usize, String)>) {
    let mut out = HashMap::new();
    let mut bad = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match parse_map_line(line) {
            Some((code, action)) => {
                out.insert(code, action);
            }
            None => bad.push((n + 1, trimmed.to_string())),
        }
    }
    (out, bad)
}

/// Parse a whole `hl5_gamepad.txt`, ignoring what could not be read.
pub fn parse_map(text: &str) -> HashMap<i32, Option<PadAction>> {
    parse_map_reporting(text).0
}

/// Analog-trigger digital edge with hysteresis. Many Bluetooth pads never
/// send `BUTTON_L2/R2` key events at all — the triggers exist only as analog
/// axes — so the axes are thresholded here and routed through the same button
/// table as a synthetic `KEYCODE_BUTTON_L2/R2`, which keeps a remap of 104/105
/// working for them too.
pub fn trigger_held(was_held: bool, value: f32) -> bool {
    const ON: f32 = 0.5;
    const OFF: f32 = 0.25;
    if was_held {
        value > OFF
    } else {
        value >= ON
    }
}

/// The four-way latch, for the routes where the guest wants keys: menus, and
/// any screen that is not the field.
///
/// Same shape as the touch joystick's own quantiser and hero3's `joy_dir`:
/// engage past 30 % of full deflection, keep until 15 %, and a challenging
/// axis must beat the held one by 1.2x to take the turn, so a thumb resting
/// near a diagonal cannot flutter between two menu rows.
pub fn menu_dir(dx: f32, dy: f32, active: Option<i32>) -> Option<i32> {
    // Engage where the joystick route does, and keep down to the same floor:
    // a resting stick that drifts (see [`RELEASE`]) must let go of a cursor
    // key as surely as it lets go of the finger, or a menu scrolls by itself.
    const DEAD_ENGAGE: f32 = ENGAGE;
    const DEAD_KEEP: f32 = RELEASE;
    const TURN_RATIO: f32 = 1.2;

    let dist = (dx * dx + dy * dy).sqrt();
    let dead = if active.is_some() { DEAD_KEEP } else { DEAD_ENGAGE };
    if dist <= dead {
        return None;
    }
    let comp = |k: i32| match k {
        KEY_UP => -dy,
        KEY_DOWN => dy,
        KEY_LEFT => -dx,
        _ => dx,
    };
    let cand = if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            KEY_RIGHT
        } else {
            KEY_LEFT
        }
    } else if dy >= 0.0 {
        KEY_DOWN
    } else {
        KEY_UP
    };
    if let Some(cur) = active {
        if cur != cand && comp(cur) > 0.0 && comp(cand) < comp(cur) * TURN_RATIO {
            return Some(cur); // not a decisive turn — stay on the held row
        }
    }
    Some(cand)
}

/// The unit vector a direction key points along, or `None` for any other key.
/// Screen coordinates, so DOWN is +y.
fn direction_axis(key: i32) -> Option<(i32, i32)> {
    Some(match key {
        KEY_UP => (0, -1),
        KEY_DOWN => (0, 1),
        KEY_LEFT => (-1, 0),
        KEY_RIGHT => (1, 0),
        _ => return None,
    })
}

/// One thing the host still owes the guest, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadOut {
    /// A guest key edge, through `nativeKeyHandleEvent`.
    Key { code: i32, down: bool },
    /// The synthetic finger on the guest's own joystick, through
    /// `nativeTouchHandleEvent` with [`PAD_POINTER`].
    Finger { kind: Touch, x: i32, y: i32 },
}

impl PadOut {
    /// The guest mailbox byte this event needs empty, as an offset from `gv`.
    /// See the module header: one slot per kind, and a second arrival in the
    /// same tick is dropped in silence.
    pub fn mailbox(self) -> u32 {
        match self {
            PadOut::Key { down: true, .. } => 0xe4,
            PadOut::Key { down: false, .. } => 0xe5,
            PadOut::Finger { kind: Touch::Pressed, .. } => 0xf0,
            PadOut::Finger { kind: Touch::Released, .. } => 0xf1,
            PadOut::Finger { kind: Touch::Moved, .. } => 0xf2,
        }
    }

    /// The mailboxes that must not hold an earlier event of the pad's before
    /// this one may go in — because the guest would handle the two in the
    /// wrong order.
    ///
    /// `mainKeyPressed` empties the mailboxes once per tick in a fixed order
    /// (0x9CA5C): press `gv+0xF0` — and a tick that took a press **skips the
    /// move** `gv+0xF2` (its arm rejoins at 0x9CA74) — else the move; then the
    /// release `gv+0xF1`, the keydown `gv+0xE4`, the keyup `gv+0xE5`. Filling
    /// every free slot as the pad produced events reorders three pairs:
    ///
    /// * press, move, release inside one tick: press, then release, and the
    ///   move a tick later to a joystick nobody holds — a short d-pad tap
    ///   never walks. A release waits for the press and the move.
    /// * release, then a new press (the next tap; one pointer id for both):
    ///   press first, and the old release lets go of the new finger — the
    ///   second tap is lost. A press waits for the release.
    /// * keyup, then a keydown: down first, then up — the key reads released
    ///   while it is held. A keydown waits for the keyup.
    pub fn waits_for(self) -> &'static [u32] {
        match self {
            PadOut::Key { down: true, .. } => &[0xe5],
            PadOut::Key { down: false, .. } => &[],
            PadOut::Finger { kind: Touch::Pressed, .. } => &[0xf1],
            PadOut::Finger { kind: Touch::Released, .. } => &[0xf0, 0xf2],
            PadOut::Finger { kind: Touch::Moved, .. } => &[],
        }
    }
}

/// What a guest mailbox holds, as [`Pad::next_ready_in_order`] needs to know
/// it: nothing, an event of the pad's own, or someone else's (a real finger —
/// a different pointer the joystick owner test already tells apart, so the
/// pad does not have to wait behind it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Free,
    Pad,
    Other,
}

/// The in-game touch UI as the guest last drew it.
///
/// Recorded by the observer hook on `StateGame::GetTouchJoyXY`, which
/// `StateGame::DrawTouchUi+0x84` is the only caller of — and which sits past
/// all three of that function's early-outs (`gv[0x27c]->[4] == 1`,
/// `gv[0x148c]->[0x79] == 0`, `Battle::IsGameUIView() == 1`). So its being
/// called *is* the statement "the field HUD is in the picture", which is both
/// the badge trigger and how [`Pad::axes`] knows whether the stick should be
/// a joystick or a pair of menu keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TouchUi {
    /// The six 30x30 action buttons, `(x, y)` from `this+0xe0` / `this+0xec`,
    /// in the order `TouchReleased`'s jump table reads them.
    pub buttons: [(i32, i32); 6],
    /// The belt page button, `this+0xf8` / `this+0xfa`, also 30x30.
    pub page: (i32, i32),
    /// The OK button: the `IRect` at `this+0xb8`, `(x, y, w, h)`.
    pub ok: (i32, i32, i32, i32),
    /// The dash button at the right edge: `StateGame::TouchPressed+0x304..
    /// +0x358` takes `x >= W-0x23, y in [H-0x65, H-0x47)` as a dash, and
    /// `DrawTouchUi` draws it right-aligned on the edge, centred on `H-0x56`.
    /// Not in `StateGame` as a rect, so computed from the canvas. `None` only
    /// where it is not drawn (the 240-wide 세로 HUD).
    pub dash: Option<(i32, i32, i32, i32)>,
}

/// The dash button's rect on a `w` x `h` canvas — see [`TouchUi::dash`].
pub fn dash_rect(w: i32, h: i32) -> (i32, i32, i32, i32) {
    (w - 0x23, h - 0x65, 0x23, 0x65 - 0x47)
}

// `gv[0x1971]` is NOT the dash button's gate, as this file once said
// (ANALYSIS.md §68.2): `DrawTouchUi` (0xE0764) draws the `map shop save` bar
// only while it is 0 and then joins the 취소 / dash code at 0xE0778 either
// way, and `TouchPressed` (0xE1D7C) tests the bar first only while it is 0
// and the dash rect always. So the dash button is part of every landscape
// field HUD, and reading the byte as "has a dash button" left the dash
// without its badge on exactly the screens that show the bar — the usual
// field.

/// Side of a 30x30 touch button, the size every one of them is tested at
/// (`add ip, r1, #0x1e` in both `TouchPressed` and `TouchReleased`).
pub const BUTTON: i32 = 0x1e;

/// How far below its table position the belt **draws** a slot:
/// `GMenu::DrawSkillBelt` puts the plate at `(x, y + 6)` and the icon centred
/// on `(x + 15, y + 21)` (0x83880 `add r3, sb, #6`; 0x84240/0x8424c) — the
/// same code for both layouts, the 240-wide one included (its branch at
/// 0x83748 only fills the position arrays and rejoins at 0x837C8) — while
/// `SetTouchKeypad` tests `(x, y, 30, 30)` from the same table. A badge or
/// an outline that follows the table position sits six pixels above the
/// picture, in landscape and in 세로 alike; both were reported.
pub const BELT_DRAW_DY: i32 = 6;

/// A belt slot where the player *sees* it, from its table position.
pub fn drawn_button_rect(x: i32, y: i32) -> (i32, i32, i32, i32) {
    (x, y + BELT_DRAW_DY, BUTTON, BUTTON)
}

/// Which guest key each of the six action buttons sends, in the order
/// `TouchReleased+0x1ac`'s jump table takes them: slot 1, slot 2, combo,
/// slot 3, slot 4, slot 5.
pub const BUTTON_KEYS: [i32; 6] =
    [KEY_SLOT[0], KEY_SLOT[1], KEY_COMBO, KEY_SLOT[2], KEY_SLOT[3], KEY_SLOT[4]];

/// Everything the pad layer remembers between events.
pub struct Pad {
    /// The player's `hl5_gamepad.txt`, empty when there is none.
    pub overrides: HashMap<i32, Option<PadAction>>,
    /// Whether a pad is attached, as the shell last reported it.
    pub present: bool,
    /// Badge preference — see [`crate::padhud::PadHud`].
    pub hud: crate::padhud::PadHud,
    /// Buttons currently held, and the action each was resolved to when it
    /// went down. Kept per code so a release sends the same key its press did
    /// even if the table changed underneath, and so the system's auto-repeat
    /// cannot latch the guest twice.
    held: HashMap<i32, PadAction>,
    /// Codes in [`Pad::held`] whose press really was sent to the guest as a
    /// direction KEY — the spirit wheel's, and only those. Everywhere else a
    /// direction steers instead of being sent, which is why [`Pad::release_all`]
    /// skips directions; these are the exception it must not skip.
    keyed: HashSet<i32>,
    /// Digital state of the two analog triggers.
    triggers: (bool, bool),
    /// Where the analog stick is, −1..1 per axis, the HAT *axes* folded in.
    stick: (f32, f32),
    /// Directions held as KEY events rather than as axes — the d-pad, on the
    /// pads that report it that way, and any button a player has bound to a
    /// direction. Summed with [`Pad::stick`] before quantising, so both kinds
    /// of pad steer the same way. See [`Pad::button`] for why a direction can
    /// never be sent to the guest as a key while the field HUD is up.
    dhat: (i32, i32),
    /// Whether the synthetic finger is down, and where it was last put.
    finger: Option<(i32, i32)>,
    /// The four-way latch, for the key route.
    menu_key: Option<i32>,
    /// What the host still owes the guest.
    queue: VecDeque<PadOut>,
    /// The field HUD as last drawn.
    pub ui: Option<TouchUi>,
    /// Set by the observer hook, taken by the badge painter: "the picture the
    /// host is about to finish has the field HUD in it".
    paint_pending: bool,
    /// Set by the observer hook, cleared by every frame that painted: "the HUD
    /// was drawn during the frame now ending".
    drawn: bool,
    /// Whether the guest's spirit-skill wheel is up (`HERO+0x1FB6`). It is a
    /// field-time modal that takes cursor keys, so while it is set the stick
    /// takes the key route even though the field HUD is still drawn, and the
    /// face buttons stand in for its compass points — see [`spirit_face`].
    spirit: bool,
    /// Whether the field HUD is in the picture on screen — the latch that
    /// decides whether the stick is a joystick or a pair of menu keys. Kept on
    /// the guest's repaint clock and not the poll clock, for the reason
    /// [`crate::hud::Hud::frame_start`] spells out: most polls draw nothing,
    /// and a latch cleared on those would be wrong two polls out of three.
    on_field: bool,
    /// Counters for the boot/periodic line: edges resolved, edges with no
    /// binding, events actually handed over, and badges painted.
    pub edges: u64,
    pub unbound: u64,
    pub delivered: u64,
    pub badges: u64,
    /// Drags folded into the guest's own press-time move (see `Vm::pad_pump`).
    pub merged: u64,
    /// Polls where the front event waited for an earlier one to be consumed
    /// ([`PadOut::waits_for`]).
    pub held_back: u64,
}

impl Default for Pad {
    fn default() -> Self {
        Self {
            overrides: HashMap::new(),
            present: false,
            hud: crate::padhud::PadHud::default(),
            held: HashMap::new(),
            keyed: HashSet::new(),
            triggers: (false, false),
            stick: (0.0, 0.0),
            dhat: (0, 0),
            finger: None,
            menu_key: None,
            queue: VecDeque::new(),
            ui: None,
            paint_pending: false,
            drawn: false,
            spirit: false,
            on_field: false,
            edges: 0,
            unbound: 0,
            delivered: 0,
            badges: 0,
            merged: 0,
            held_back: 0,
        }
    }
}

impl Pad {
    /// Resolve a keycode under the table in force. The 세로 keypad's codes
    /// come first and are not the player's to remap — `hl5_gamepad.txt`
    /// describes a pad, and a typo in it must not rewire the on-screen keys.
    pub fn action(&self, code: i32) -> Option<PadAction> {
        if let Some(a) = crate::portrait::keypad_action(code) {
            return a;
        }
        button_action_in(code, &self.overrides)
    }

    /// Are the badges showing?
    pub fn badges_visible(&self) -> bool {
        self.hud.active(self.present)
    }

    /// The field HUD was drawn. Called by the observer hook with the rects it
    /// read out of the guest's own `StateGame`.
    pub fn saw_touch_ui(&mut self, ui: TouchUi) {
        self.ui = Some(ui);
        self.paint_pending = true;
        self.drawn = true;
    }

    /// Take the "drawn since the last paint" flag. The badge painter runs only
    /// when the host's own pixel loop rewrote the canvas, so this being true
    /// means the picture just built has the HUD in it.
    pub fn take_seen(&mut self) -> Option<TouchUi> {
        if !std::mem::take(&mut self.paint_pending) {
            return None;
        }
        self.ui
    }

    /// What the frame now finishing should stamp badges on, if anything.
    ///
    /// **The flag is taken whether or not the badges show, and that ordering
    /// is the whole function.** `paint_pending` says "the guest drew the field
    /// HUD into *this* picture", which is a fact that expires with the
    /// picture. Skipping the take while the badges are off — which is every
    /// frame until a pad is plugged in — keeps a fact about some long-gone
    /// picture alive: connect a pad while the character sheet is open and the
    /// first painted frame stamps the field HUD's badges onto a screen that
    /// has no controls under them, at coordinates from another scene.
    pub fn take_badge_target(&mut self) -> Option<TouchUi> {
        let ui = self.take_seen();
        if self.badges_visible() {
            ui
        } else {
            None
        }
    }

    /// Begin a frame. `painted` says whether the frame that just finished put
    /// a new picture on the screen; only such a frame can tell us the HUD has
    /// gone. Returns whether the answer changed, i.e. whether the caller must
    /// re-steer a stick that is still deflected.
    pub fn frame_start(&mut self, painted: bool) -> bool {
        if !painted {
            return false;
        }
        let was = self.on_field;
        self.on_field = std::mem::take(&mut self.drawn);
        was != self.on_field
    }

    /// Is the field HUD in the picture on screen?
    pub fn field(&self) -> bool {
        self.on_field
    }

    /// The guest's spirit wheel opened or closed. Answers whether that
    /// changed, because a stick already deflected has to switch routes
    /// without moving: joystick while the wheel is down, cursor keys while it
    /// is up.
    pub fn set_spirit(&mut self, on: bool) -> bool {
        if self.spirit == on {
            return false;
        }
        self.spirit = on;
        true
    }

    /// Is the spirit wheel up?
    pub fn spirit(&self) -> bool {
        self.spirit
    }

    fn push(&mut self, out: PadOut) {
        // Coalesce stick drags: only the newest position of a finger that has
        // not been handed over yet can matter, and the guest would drop the
        // earlier one anyway (`BaseState::TouchMove` records nothing while
        // `gv+0xF2` is set).
        if let PadOut::Finger { kind: Touch::Moved, .. } = out {
            if let Some(PadOut::Finger { kind: Touch::Moved, .. }) = self.queue.back() {
                self.queue.pop_back();
            }
        }
        self.queue.push_back(out);
    }

    /// One button edge. `repeat` is the system's auto-repeat, which is
    /// dropped: the guest has no held-key state to refresh, and a second
    /// keydown would re-latch it mid-hold.
    ///
    /// `dir` is the direction the hero is currently walking (−1..−4), for
    /// [`PadAction::Dash`] — the caller reads it from `gv+0x11c` because that
    /// is where the guest itself keeps it.
    pub fn button(&mut self, code: i32, down: bool, repeat: bool, dir: Option<i32>, centre: (i32, i32)) {
        if repeat {
            return;
        }
        self.edges += 1;
        let Some(mut action) = self.action(code) else {
            self.unbound += 1;
            return;
        };
        // Two screen-dependent substitutions, both made here and not in the
        // table, so the table stays what the player wrote and the badges —
        // which are a reverse lookup of it — keep naming the same buttons.
        //
        // 1. The spirit wheel, whose compass points are the only thing it
        //    reads. [`spirit_face`] says why borrowing these costs nothing.
        //    `as_key` because this direction really is *sent*: it is the one
        //    place in this game where a WIPI direction key does something.
        // 2. Off the field, whatever is bound to belt slot 4 becomes the
        //    game's own info toggle. `'*'` is dead on every menu screen but
        //    one (`StateMainMenu::KeyStart`, where it deletes a character —
        //    still a tap away), while `'#'` is what all 22 of the menu key
        //    handlers hand to `CommonUi::TogleAutoInfo` (0x775AC). In the
        //    field the same control stays belt slot 4.
        //
        // Neither applies to the 세로 keypad, which has its own `#` key and
        // its own arrows, and whose `*` should be `*` (crate::portrait).
        let mut as_key = false;
        if self.spirit {
            if let Some(k) = spirit_face(code) {
                action = PadAction::Key(k);
                as_key = true;
            }
        } else if !self.field()
            && action == PadAction::Key(KEY_SLOT[3])
            && !crate::portrait::is_keypad(code)
        {
            action = PadAction::Key(KEY_PAGE);
        }
        // A control bound to a DIRECTION never sends that direction as a key
        // while the field HUD is up, because a WIPI direction key does not
        // move this hero — `WorkMoveHero` returns on the spot unless
        // `gv+0x278` holds a real angle, and only touch writes that (see the
        // module header). It steers instead, exactly like the stick, and
        // [`Pad::steer`] is what turns steering back into cursor keys on the
        // screens where the guest does want them.
        //
        // This is not a special case for the d-pad: many pads report their
        // d-pad as HAT axes, which arrive through [`Pad::axes`], and the rest
        // report it as `KEYCODE_DPAD_*`. One path for both.
        if let (false, PadAction::Key(k)) = (as_key, action) {
            if let Some(axis) = direction_axis(k) {
                let held = self.held.contains_key(&code);
                if down && !held {
                    self.held.insert(code, action);
                    self.dhat = (self.dhat.0 + axis.0, self.dhat.1 + axis.1);
                } else if !down && held {
                    self.held.remove(&code);
                    self.dhat = (self.dhat.0 - axis.0, self.dhat.1 - axis.1);
                } else {
                    return;
                }
                self.steer(centre);
                return;
            }
        }
        if down {
            if self.held.contains_key(&code) {
                return;
            }
            self.held.insert(code, action);
            if as_key {
                self.keyed.insert(code);
            }
            match action {
                PadAction::Key(k) => self.push(PadOut::Key { code: k, down: true }),
                PadAction::Dash => {
                    // The guest's own double-tap: `<CLASS>::KeyPress` arms
                    // `SetDashFlag(dir, 1)` + `SetDashCheckFrame(10)` on a
                    // direction press and dashes on the next one inside those
                    // ten frames (0x147880 / 0x147A24). Two presses, one per
                    // tick, therefore dash from a standstill as well as from a
                    // walk. Press-only on purpose: the touch joystick also
                    // emits `KeyPress` without a matching release when the
                    // direction changes, and a release here would end the walk.
                    if let Some(d) = dir {
                        self.push(PadOut::Key { code: d, down: true });
                        self.push(PadOut::Key { code: d, down: true });
                    }
                }
            }
        } else if let Some(action) = self.held.remove(&code) {
            // The action stored at press time, so a wheel that closed under
            // the finger still gets the release its press asked for.
            self.keyed.remove(&code);
            if let PadAction::Key(k) = action {
                self.push(PadOut::Key { code: k, down: false });
            }
        }
    }

    /// One stick / hat / trigger sample. Stick and hat are summed before
    /// quantising: the hat is digital ±1 so it dominates whenever it is
    /// pressed, and nobody deflects both on purpose.
    ///
    /// `centre` is where the synthetic finger presses, from
    /// [`stick_centre`]; `dir` feeds a queued [`PadAction::Dash`] the same way
    /// [`Pad::button`] takes it.
    pub fn axes(
        &mut self,
        x: f32,
        y: f32,
        hx: f32,
        hy: f32,
        lt: f32,
        rt: f32,
        centre: (i32, i32),
        dir: Option<i32>,
    ) {
        let dx = (x + hx).clamp(-1.0, 1.0);
        let dy = (y + hy).clamp(-1.0, 1.0);
        self.stick = (dx, dy);
        self.steer(centre);
        // The triggers go through the button table as synthetic L2/R2, so a
        // pad that reports them only as axes behaves like one that reports
        // them as buttons — remaps included.
        let (l, r) = self.triggers;
        let now_l = trigger_held(l, lt);
        let now_r = trigger_held(r, rt);
        if now_l != l {
            self.triggers.0 = now_l;
            self.button(KEYCODE_BUTTON_L2, now_l, false, dir, centre);
        }
        if now_r != r {
            self.triggers.1 = now_r;
            self.button(KEYCODE_BUTTON_R2, now_r, false, dir, centre);
        }
    }

    /// Turn the current stick position into whatever the screen in front of
    /// the player understands. Called on every sample and whenever the field
    /// HUD appears or disappears, because that changes the answer without the
    /// stick moving.
    pub fn steer(&mut self, centre: (i32, i32)) {
        let (dx, dy) = self.deflection();
        let mag = (dx * dx + dy * dy).sqrt();
        // `!self.spirit`: the wheel is drawn over a field that still has its
        // HUD, but it is a modal that reads cursor keys, so the stick and the
        // d-pad have to leave the joystick and become keys for as long as it
        // is up. That is also the four compass points, for free.
        if self.field() && !self.spirit {
            // The joystick route. Let go of any menu key first: the two
            // routes must never both hold a direction.
            if let Some(k) = self.menu_key.take() {
                self.push(PadOut::Key { code: k, down: false });
            }
            let want = match self.finger {
                Some(_) => mag >= RELEASE,
                None => mag >= ENGAGE,
            };
            match (want, self.finger) {
                (true, None) => {
                    // Press on the centre, then drag: in the floating scheme
                    // the press point *becomes* the centre (0xE1C8C), so
                    // pressing where the deflection points would put the
                    // origin under the thumb and every later angle would be
                    // measured from the wrong place.
                    //
                    // Pressing already deflected under the *fixed* scheme was
                    // tried and does not buy a tick: `TouchPressed` there sets
                    // the angle (0xE217C) but never the direction `KeyPress`
                    // that starts the walk — only `TouchMoved` does (0xE166C),
                    // and `mainKeyPressed` handles the press mailbox or the
                    // move mailbox in one tick, press first (0x9CA5C). So a
                    // thumb, too, walks one tick after it lands. What the pad
                    // used to add on top of that — a second tick, because the
                    // guest's own press-time `TouchMove` held the move slot —
                    // `Vm::pad_pump` removes by merging the drag into it.
                    self.push(PadOut::Finger { kind: Touch::Pressed, x: centre.0, y: centre.1 });
                    self.finger = Some(centre);
                    self.drag(centre);
                }
                (true, Some(_)) => self.drag(centre),
                (false, Some(at)) => {
                    self.push(PadOut::Finger { kind: Touch::Released, x: at.0, y: at.1 });
                    self.finger = None;
                }
                (false, None) => {}
            }
            return;
        }
        // The key route: menus, shops, the title screen. Release the finger
        // if the HUD went away under it, then latch a direction.
        if let Some(at) = self.finger.take() {
            self.push(PadOut::Finger { kind: Touch::Released, x: at.0, y: at.1 });
        }
        let next = menu_dir(dx, dy, self.menu_key);
        if next != self.menu_key {
            // Release the old before pressing the new: the guest latches a
            // direction on keydown and keeps one at a time (`gv+0x11c`).
            if let Some(k) = self.menu_key {
                self.push(PadOut::Key { code: k, down: false });
            }
            if let Some(k) = next {
                self.push(PadOut::Key { code: k, down: true });
            }
            self.menu_key = next;
        }
    }

    /// Stick plus digital hat, clamped. A d-pad is ±1, so it dominates a
    /// half-deflected stick, which is what a player pressing one expects.
    fn deflection(&self) -> (f32, f32) {
        (
            (self.stick.0 + self.dhat.0 as f32).clamp(-1.0, 1.0),
            (self.stick.1 + self.dhat.1 as f32).clamp(-1.0, 1.0),
        )
    }

    fn drag(&mut self, centre: (i32, i32)) {
        let (dx, dy) = self.deflection();
        let to = (
            centre.0 + (dx * RADIUS).round() as i32,
            centre.1 + (dy * RADIUS).round() as i32,
        );
        if self.finger == Some(to) {
            return;
        }
        self.finger = Some(to);
        self.push(PadOut::Finger { kind: Touch::Moved, x: to.0, y: to.1 });
    }

    /// Let go of everything. The activity losing focus stops Java dispatching,
    /// so a still-held button or stick would leave the hero walking into a
    /// wall for the rest of the session.
    pub fn release_all(&mut self) {
        let held: Vec<(i32, PadAction)> = self.held.drain().collect();
        let keyed = std::mem::take(&mut self.keyed);
        for (code, a) in held {
            // Directions are skipped: they were never *sent* as a key (they
            // steer — see [`Pad::button`]), so a release for one would be a
            // release the guest has no press for. Zeroing the hat below and
            // letting go of the finger is what stops the walk.
            if let PadAction::Key(k) = a {
                if direction_axis(k).is_none() || keyed.contains(&code) {
                    self.push(PadOut::Key { code: k, down: false });
                }
            }
        }
        self.triggers = (false, false);
        self.stick = (0.0, 0.0);
        self.dhat = (0, 0);
        if let Some(k) = self.menu_key.take() {
            self.push(PadOut::Key { code: k, down: false });
        }
        if let Some(at) = self.finger.take() {
            self.push(PadOut::Finger { kind: Touch::Released, x: at.0, y: at.1 });
        }
    }

    /// The next event, if the guest's mailbox for it is free. `free` answers
    /// "is `gv+off` zero"; a slot that is not is taken to hold one of the
    /// pad's own events (the strictest reading — see
    /// [`Pad::next_ready_in_order`]).
    pub fn next_ready(&mut self, free: impl Fn(u32) -> bool) -> Option<PadOut> {
        self.next_ready_in_order(|off| if free(off) { Slot::Free } else { Slot::Pad })
    }

    /// The next event, if its own mailbox is free **and** the guest would not
    /// handle it ahead of an earlier event of the pad's still waiting in
    /// another one ([`PadOut::waits_for`]). `slot` answers what `gv+off`
    /// holds — the caller owns the guest memory.
    pub fn next_ready_in_order(&mut self, slot: impl Fn(u32) -> Slot) -> Option<PadOut> {
        let front = *self.queue.front()?;
        if slot(front.mailbox()) != Slot::Free {
            return None;
        }
        if front.waits_for().iter().any(|off| slot(*off) == Slot::Pad) {
            self.held_back += 1;
            return None;
        }
        self.queue.pop_front();
        self.delivered += 1;
        Some(front)
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// The front of the queue, unpopped.
    pub(crate) fn peek(&self) -> Option<PadOut> {
        self.queue.front().copied()
    }

    /// Drop the front of the queue as delivered — for the caller that
    /// delivered it by other means (see `Vm::pad_pump`).
    pub(crate) fn pop_delivered(&mut self) {
        if self.queue.pop_front().is_some() {
            self.delivered += 1;
        }
    }
}

/// The pad half of a diagnostic legend: every bound position, in Korean, with
/// what it does. Built from the same reverse lookup the badges use, so it
/// describes the configured pad and not the defaults.
pub fn legend(overrides: &HashMap<i32, Option<PadAction>>) -> String {
    let rows: [(PadAction, &str); 12] = [
        (PadAction::Key(KEY_OK), "확인 / 공격"),
        (PadAction::Key(KEY_CLEAR), "취소 / 캐릭터창"),
        (PadAction::Key(KEY_SLOT[0]), "퀵슬롯 1"),
        (PadAction::Key(KEY_SLOT[1]), "퀵슬롯 2"),
        (PadAction::Key(KEY_SLOT[2]), "퀵슬롯 3"),
        (PadAction::Key(KEY_SLOT[3]), "퀵슬롯 4 / 메뉴에서는 정보(#)"),
        (PadAction::Key(KEY_SLOT[4]), "퀵슬롯 5"),
        (PadAction::Key(KEY_COMBO), "콤보 스킬 (7)"),
        (PadAction::Key(KEY_PAGE), "퀵슬롯 페이지 (#)"),
        (PadAction::Key(KEY_MAP), "지도"),
        (PadAction::Key(KEY_SAVE), "저장"),
        (PadAction::Dash, "대시"),
    ];
    let mut out = String::from("게임패드 (위치 = 모든 패드 공통)\n");
    for (action, what) in rows {
        match pos_for_action(action, overrides) {
            Some(p) => out.push_str(&format!("  {} = {what}\n", crate::padhud::pos_name_ko(p))),
            None => out.push_str(&format!("  (없음) = {what}\n")),
        }
    }
    out.push_str("이동: 왼쪽 스틱 / 십자키 (게임 자신의 조이스틱으로 들어간다)\n");
    out.push_str("정령 스킬창: 위=북 아래=남 왼쪽=서, 오른쪽 어깨=동 (취소는 그대로)\n");
    out.push_str("지도·저장은 스틱 누르기라 없는 패드도 있다 — 화면 상단 바로 대신한다.\n");
    out
}


// ---- the observer hook, and the host-facing entry points -------------------

/// `StateGame::GetTouchJoyXY(short angle, int, int*, int*)` — 0xDE578.
///
/// Hooked purely as an observer. It has **exactly one caller**,
/// `StateGame::DrawTouchUi+0x84`, and that call sits past all three of
/// `DrawTouchUi`'s early-outs, so its being reached is the guest telling us
/// "the field HUD is going into this picture". The hook reads the button rects
/// out of the `StateGame` it was handed and **declines**, so the guest's own
/// function runs, unmodified, and no frame changes because of it.
const GET_TOUCH_JOY_XY: &str = "_ZN9StateGame13GetTouchJoyXYEsiPiS0_";
/// The function that call sits in, and the return address it leaves behind
/// (the `bl` is at 0xE0664, so `lr` is 0xE0668).
const DRAW_TOUCH_UI: &str = "_ZN9StateGame11DrawTouchUiEv";
const DRAW_TOUCH_UI_RET: u32 = 0xe0668 - 0xe05e0;

/// Guest field offsets inside `StateGame`, each confirmed against the hit test
/// that reads it:
///
/// * `+0xb8` — the OK button's `IRect` (`TouchReleased+0xa4` calls
///   `IRect::GetX` on `this+0xb8`; an `IRect` is a vptr then x/y/w/h at
///   +4/+8/+0xc/+0x10).
/// * `+0xe0` / `+0xec` — the six action buttons' x and y, a `short` each
///   (`TouchReleased+0x60`: six iterations, 30 px square).
/// * `+0xf8` / `+0xfa` — the belt page button (`TouchPressed+0x278`, same
///   size, and 0xE285C shows it sends `'#'`).
const OK_RECT: u32 = 0xb8;
const BTN_X: u32 = 0xe0;
const BTN_Y: u32 = 0xec;
const PAGE_X: u32 = 0xf8;
const PAGE_Y: u32 = 0xfa;
/// `gv+0x1474` — the `HERO`, and `CHAR+0x2d` its facing. Read for
/// [`PadAction::Dash`], which needs a direction to dash along.
///
/// The facing and not `gv+0x11c` (the last direction the guest latched),
/// because the guest's own dash arm compares against **exactly this**:
/// `WARRIOR::KeyPress+0x5f0` dashes only when `GetDir() == dir`
/// (`CHAR::GetDir` is `ldrsb [this+0x2d]`, 0x49B84), so any other value
/// cannot dash however plausible it looks. It covers both cases too — while
/// the hero walks, `WorkMoveHero` calls `CHAR::SetDir` from the angle on
/// every tick, so the facing *is* the direction of travel.
const GV_HERO: u32 = 0x1474;
const CHAR_DIR: u32 = 0x2d;
/// `HERO::IsSpiritSkillMenu` (0x8921C) is `ldrsb [this + 0x1FB6]; > 0`, and
/// `HERO::InitSpiritSkillMenu` (0x89198) is what writes the 1. Reading the
/// byte is therefore the same question the guest asks itself, with no hook.
const HERO_SPIRIT_MENU: u32 = 0x1FB6;
/// The guest's touch-move mailbox (`BaseState::TouchMove`, 0x44DCC): the flag
/// at `gv+0xF2`, then x `gv+0x104`, y `gv+0x108`, pointer id `gv+0x114`.
/// `mainKeyPressed+0x210` reads exactly these three back.
const MAILBOX_MOVE: u32 = 0xf2;
const MAILBOX_MOVE_X: u32 = 0x104;
const MAILBOX_MOVE_Y: u32 = 0x108;
const MAILBOX_MOVE_ID: u32 = 0x114;

/// The pointer id beside each touch mailbox flag — what
/// `mainKeyPressed+0xcc` / `+0x1d4` / `+0x210` hand `TouchPressed`,
/// `TouchReleased` and `TouchMoved` as their third argument. The key
/// mailboxes carry none.
fn mailbox_pointer(off: u32) -> Option<u32> {
    match off {
        0xf0 => Some(0x10c),
        0xf1 => Some(0x110),
        MAILBOX_MOVE => Some(MAILBOX_MOVE_ID),
        _ => None,
    }
}

/// What `gv+off` holds, for [`Pad::next_ready_in_order`]. A key mailbox that
/// is full counts as the pad's: a key edge has no pointer to tell owners apart.
fn mailbox_slot(mem: &crate::mem::GuestMem, gv: u32, off: u32) -> Slot {
    if mem.read_u8(gv + off) == 0 {
        return Slot::Free;
    }
    match mailbox_pointer(off) {
        Some(id) if mem.read_u32(gv + id) != PAD_POINTER as u32 => Slot::Other,
        _ => Slot::Pad,
    }
}

/// Facing 0..3 → the WIPI key that means it. `HERO::GetAngleToDir` and
/// `input::navi_to_face` are the same map the other way round.
fn face_to_navi(face: i32) -> Option<i32> {
    Some(match face {
        0 => KEY_UP,
        1 => KEY_DOWN,
        2 => KEY_RIGHT,
        3 => KEY_LEFT,
        _ => return None,
    })
}

/// Install the observer hook. Nothing else in this feature touches the guest.
pub fn install(vm: &mut crate::Vm) -> Result<Vec<&'static str>, String> {
    for name in [GET_TOUCH_JOY_XY, DRAW_TOUCH_UI, crate::warehouse::GV] {
        if vm.symbol(name).is_none() {
            return Err(format!("gamepad hook needs `{name}`, not in this binary"));
        }
    }
    let addr = vm.symbol(GET_TOUCH_JOY_XY).expect("checked above");
    let f: crate::patch::PatchFn = touch_ui_observer;
    vm.install_hook(GET_TOUCH_JOY_XY, addr, f)?;
    Ok(vec![GET_TOUCH_JOY_XY])
}

/// Read the field HUD's rects out of a live `StateGame`.
fn read_touch_ui(vm: &crate::Vm, this: u32) -> TouchUi {
    // 세로: the touch rects still hold the 240-wide arc, but the relaid HUD
    // draws the belt as one row at the bottom and skips the joystick plates
    // and OK — the badges go where the player sees buttons.
    if vm.cfg.screen_mode.is_portrait() {
        return crate::portrait::drawn_hud(vm.cfg.wipi_w, vm.cfg.wipi_h);
    }
    let s16 = |a: u32| vm.mem.read_u16(a) as i16 as i32;
    let s32 = |a: u32| vm.mem.read_u32(a) as i32;
    let mut buttons = [(0, 0); 6];
    for (i, b) in buttons.iter_mut().enumerate() {
        let o = 2 * i as u32;
        *b = (s16(this + BTN_X + o), s16(this + BTN_Y + o));
    }
    TouchUi {
        buttons,
        page: (s16(this + PAGE_X), s16(this + PAGE_Y)),
        ok: (
            s32(this + OK_RECT + 4),
            s32(this + OK_RECT + 8),
            s32(this + OK_RECT + 0xc),
            s32(this + OK_RECT + 0x10),
        ),
        dash: Some(dash_rect(vm.cfg.wipi_w, vm.cfg.wipi_h)),
    }
}

fn touch_ui_observer(vm: &mut crate::Vm) -> Result<bool, String> {
    use arm32_cpu::{reg, Mode};
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if lr == crate::mem::RETURN_MAGIC {
        return Ok(false);
    }
    let site = match vm.symbol(DRAW_TOUCH_UI) {
        Some(s) => s,
        None => return Ok(false),
    };
    if lr != site + DRAW_TOUCH_UI_RET {
        return Ok(false);
    }
    let this = vm.arg(0);
    let ui = read_touch_ui(vm, this);
    vm.pad.saw_touch_ui(ui);
    // Never handled: this hook exists to watch, and the guest goes on drawing
    // its own joystick handle exactly as it always did.
    Ok(false)
}

impl crate::Vm {
    /// Report whether a pad is attached. Drives the badges in
    /// [`crate::padhud::PadHud::Auto`], and lets go of anything the pad was
    /// holding when it disappears.
    pub fn pad_present(&mut self, present: bool) {
        if !self.cfg.gamepad || self.pad.present == present {
            return;
        }
        self.pad.present = present;
        if !present {
            self.pad.release_all();
        }
        self.log.push(format!(
            "pad: {} — badges {}",
            if present { "attached" } else { "gone" },
            if self.pad.badges_visible() { "on" } else { "off" }
        ));
        // The badges are part of the picture, so a change to whether they show
        // has to invalidate the host's "same draw as last time" shortcut, or
        // the canvas would keep the ones it has until the guest repaints.
        self.gl.forget_last_draw();
    }

    /// One pad button edge, by Android keycode.
    pub fn pad_button(&mut self, code: i32, down: bool, repeat: bool) {
        if !self.cfg.gamepad {
            return;
        }
        // The key-mapping screen takes every button while it is up (and the
        // release of one it took the press of); so does the 배치변경 editor.
        if crate::padmap::pad_button(self, code, down) || crate::hudedit::pad_button(self, code, down) {
            return;
        }
        let dir = self.pad_walk_dir();
        let centre = stick_centre(self.cfg.wipi_h);
        self.pad_sync_spirit(centre);
        self.pad.button(code, down, repeat, dir, centre);
    }

    /// One pad axis sample: left stick, hat, and the two analog triggers.
    pub fn pad_axes(&mut self, x: f32, y: f32, hx: f32, hy: f32, lt: f32, rt: f32) {
        if !self.cfg.gamepad || self.padmap.active || self.hudlayout.edit.active {
            return;
        }
        let centre = stick_centre(self.cfg.wipi_h);
        let dir = self.pad_walk_dir();
        self.pad_sync_spirit(centre);
        self.pad.axes(x, y, hx, hy, lt, rt, centre, dir);
    }

    /// Let go of everything the pad holds — focus loss, suspend, teardown.
    /// Anything still held would otherwise walk the hero into a wall for the
    /// rest of the session, because Java stops dispatching the moment the
    /// activity is not focused.
    pub fn pad_release(&mut self) {
        if !self.cfg.gamepad {
            return;
        }
        self.pad.release_all();
    }

    /// Is the guest's spirit-skill wheel up? The same byte
    /// `HERO::IsSpiritSkillMenu` reads, so this is an observation and not a
    /// second opinion. `false` before there is a hero to ask.
    fn pad_spirit_menu(&self) -> bool {
        let Some(gv) = self.symbol(crate::warehouse::GV) else {
            return false;
        };
        let hero = self.mem.read_u32(gv + GV_HERO);
        if hero == 0 {
            return false;
        }
        (self.mem.read_u8(hero + HERO_SPIRIT_MENU) as i8) > 0
    }

    /// Keep the pad's copy of that answer current, and re-steer when it
    /// turned over: a stick deflected as the wheel opens has to let go of the
    /// joystick and take a cursor key instead, without the player moving it.
    fn pad_sync_spirit(&mut self, centre: (i32, i32)) {
        let on = self.pad_spirit_menu();
        if self.pad.set_spirit(on) {
            self.pad.steer(centre);
        }
    }

    /// The direction a dash would go: the hero's own facing, as the WIPI key
    /// that means it. `None` before there is a hero to ask.
    fn pad_walk_dir(&self) -> Option<i32> {
        let gv = self.symbol(crate::warehouse::GV)?;
        let hero = self.mem.read_u32(gv + GV_HERO);
        if hero == 0 {
            return None;
        }
        face_to_navi(self.mem.read_u8(hero + CHAR_DIR) as i8 as i32)
    }

    /// Hand the guest as much of the pad queue as its mailboxes will take.
    /// Called once per poll, before `nativeLoop`.
    pub(crate) fn pad_pump(&mut self) -> Result<(), String> {
        if !self.cfg.gamepad || self.pad.queued() == 0 {
            return Ok(());
        }
        let gv = match self.symbol(crate::warehouse::GV) {
            Some(g) => g,
            None => return Ok(()),
        };
        // At most five events per poll — one per mailbox — and the loop stops
        // the moment the front of the queue wants a slot that is already
        // taken, which is what preserves the order the pad produced them in.
        for _ in 0..5 {
            // The guest files a TouchMove of its own at every press
            // (`onHandleEvent+0x11c` calls `TouchPress` and then `TouchMove`
            // with the same point), so the press that puts the synthetic
            // finger on the joystick fills the move slot with the stick
            // centre — and the drag that gives the walk its direction would
            // wait a whole tick behind it. When the pending move is that one
            // (the pad's own pointer id), its point is replaced with ours:
            // the mailbox is the shell's input channel, and this is what the
            // guest would have been told had the two arrived as one event.
            // One tick of latency (36 ms) off every first step.
            if let Some(PadOut::Finger { kind: Touch::Moved, x, y }) = self.pad.peek() {
                if self.mem.read_u8(gv + MAILBOX_MOVE) != 0
                    && self.mem.read_u32(gv + MAILBOX_MOVE_ID) == PAD_POINTER as u32
                {
                    let (x, y) = self.hud.unshift(x, y);
                    let x = x.clamp(0, self.cfg.wipi_w - 1) as u32;
                    let y = y.clamp(0, self.cfg.wipi_h - 1) as u32;
                    self.mem.write_u32(gv + MAILBOX_MOVE_X, x);
                    self.mem.write_u32(gv + MAILBOX_MOVE_Y, y);
                    self.pad.pop_delivered();
                    self.pad.merged += 1;
                    continue;
                }
            }
            let mem = &self.mem;
            let next = self.pad.next_ready_in_order(|off| mailbox_slot(mem, gv, off));
            let out = match next {
                Some(o) => o,
                None => break,
            };
            match out {
                PadOut::Key { code, down } => self.key(code, down)?,
                PadOut::Finger { kind, x, y } => self.touch(kind, x, y, PAD_POINTER)?,
            }
        }
        Ok(())
    }

    /// Begin a frame for the pad layer: keep the field latch on the guest's
    /// repaint clock, and re-steer a deflected stick when the answer changed —
    /// walking into a menu that has just opened has to become cursor keys.
    pub(crate) fn pad_frame_start(&mut self, painted: bool) {
        if !self.cfg.gamepad {
            return;
        }
        let centre = stick_centre(self.cfg.wipi_h);
        self.pad_sync_spirit(centre);
        if self.pad.frame_start(painted) {
            self.pad.steer(centre);
        }
    }

    /// Paint the badges onto the canvas the host has just finished building,
    /// so they land on top of the whole picture — see [`crate::padhud`].
    pub(crate) fn pad_paint_badges(&mut self) {
        if !self.cfg.gamepad {
            return;
        }
        // `take_badge_target` and not a `badges_visible` guard here: the
        // pending flag has to be spent on the frame it belongs to even when
        // nothing is drawn from it. That function says why.
        let target = self.pad.take_badge_target();
        // The mapping screen's badges, whatever the field flag said.
        if self.padmap.active {
            let n = crate::padmap::paint_badges(self);
            self.pad.badges += u64::from(n);
            return;
        }
        let ui = match target {
            Some(u) => u,
            None => return,
        };
        let (w, h) = (self.gl.fb_w, self.gl.fb_h);
        // Where the 배치변경 layout put the controls, when one is in force.
        let n = match crate::hudlayout::badge_controls(self, &ui) {
            Some(list) => crate::padhud::paint_controls(&mut self.gl.fb, w, h, &list, &self.pad.overrides),
            None => crate::padhud::paint(&mut self.gl.fb, w, h, &ui, &self.pad.overrides),
        };
        self.pad.badges += u64::from(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_overrides() -> HashMap<i32, Option<PadAction>> {
        HashMap::new()
    }

    /// The stick centre on the canvas the probe replays at.
    const C: (i32, i32) = (0x41, 240 - 0x41);

    /// A field with the HUD drawn, which is where the spirit wheel lives.
    fn on_field() -> Pad {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        assert!(p.field());
        p
    }

    fn drain(p: &mut Pad) -> Vec<PadOut> {
        let mut out = Vec::new();
        while let Some(e) = p.next_ready(|_| true) {
            out.push(e);
        }
        out
    }

    /// The whole point of ①: the four compass points, one button each, sent
    /// as REAL keys — the one place in this game where a WIPI direction key
    /// does something, because `<CLASS>::KeyPress` reads it before anything
    /// else can swallow it.
    #[test]
    fn the_spirit_wheel_puts_a_compass_point_under_each_button() {
        let mut p = on_field();
        p.set_spirit(true);
        for (code, key) in [
            (KEYCODE_BUTTON_Y, KEY_UP),
            (KEYCODE_BUTTON_A, KEY_DOWN),
            (KEYCODE_BUTTON_X, KEY_LEFT),
            (KEYCODE_BUTTON_R1, KEY_RIGHT),
        ] {
            p.button(code, true, false, None, C);
            p.button(code, false, false, None, C);
            assert_eq!(
                drain(&mut p),
                vec![
                    PadOut::Key { code: key, down: true },
                    PadOut::Key { code: key, down: false },
                ],
                "keycode {code} should be WIPI {key} while the wheel is up"
            );
        }
    }

    /// B is not borrowed: it is the cancel the player already has, and the
    /// wheel timestops the game, so losing it would trap them.
    #[test]
    fn the_spirit_wheel_leaves_cancel_on_b() {
        let mut p = on_field();
        p.set_spirit(true);
        p.button(KEYCODE_BUTTON_B, true, false, None, C);
        assert_eq!(drain(&mut p), vec![PadOut::Key { code: KEY_CLEAR, down: true }]);
    }

    /// Off the wheel the same buttons are what they always were — this is a
    /// modal substitution, not a remap.
    #[test]
    fn without_the_wheel_the_face_buttons_are_unchanged() {
        let mut p = on_field();
        p.button(KEYCODE_BUTTON_A, true, false, None, C);
        p.button(KEYCODE_BUTTON_X, true, false, None, C);
        assert_eq!(
            drain(&mut p),
            vec![
                PadOut::Key { code: KEY_OK, down: true },
                PadOut::Key { code: KEY_SLOT[0], down: true },
            ]
        );
    }

    /// Picking a skill closes the wheel, so the release lands after it is
    /// gone. It still has to be the release its own press asked for, or the
    /// guest keeps a direction it never sees let go.
    #[test]
    fn a_wheel_that_closes_under_the_finger_still_releases_its_own_key() {
        let mut p = on_field();
        p.set_spirit(true);
        p.button(KEYCODE_BUTTON_Y, true, false, None, C);
        p.set_spirit(false); // HERO+0x1FB6 cleared by UseSpiritSkill
        p.button(KEYCODE_BUTTON_Y, false, false, None, C);
        assert_eq!(
            drain(&mut p),
            vec![
                PadOut::Key { code: KEY_UP, down: true },
                PadOut::Key { code: KEY_UP, down: false },
            ]
        );
    }

    /// Focus loss skips directions because they steer rather than being sent
    /// — but the wheel's really were sent, so those must be let go.
    #[test]
    fn release_all_lets_go_of_a_spirit_direction() {
        let mut p = on_field();
        p.set_spirit(true);
        p.button(KEYCODE_BUTTON_X, true, false, None, C);
        assert_eq!(drain(&mut p), vec![PadOut::Key { code: KEY_LEFT, down: true }]);
        p.release_all();
        assert_eq!(drain(&mut p), vec![PadOut::Key { code: KEY_LEFT, down: false }]);
    }

    /// The wheel is drawn over a field that still has its HUD, so the stick
    /// would go on being a joystick — and a joystick cannot pick a skill.
    /// It has to take the key route for as long as the wheel is up, and let
    /// go of the finger it was holding when it opened.
    #[test]
    fn the_wheel_turns_the_stick_into_cursor_keys_without_the_hud_leaving() {
        let mut p = on_field();
        p.axes(0.0, 1.0, 0.0, 0.0, 0.0, 0.0, C, None);
        let field = drain(&mut p);
        assert!(
            matches!(field.first(), Some(PadOut::Finger { kind: Touch::Pressed, .. })),
            "a deflected stick on the field is the synthetic finger, got {field:?}"
        );
        assert!(p.set_spirit(true));
        p.steer(C);
        let out = drain(&mut p);
        assert!(
            out.iter().any(|e| matches!(e, PadOut::Finger { kind: Touch::Released, .. })),
            "the finger has to be let go, got {out:?}"
        );
        assert!(
            out.contains(&PadOut::Key { code: KEY_DOWN, down: true }),
            "and the same deflection becomes a cursor key, got {out:?}"
        );
        assert!(p.field(), "the HUD never left; only the route changed");
    }

    /// ②: belt slot 4 in the field, the game's own info toggle everywhere
    /// else. Keyed on the ACTION and not the keycode, so a player who moves
    /// slot 4 moves the info key with it — and the badge, which is the same
    /// reverse lookup, keeps naming the right button.
    #[test]
    fn belt_slot_four_is_the_info_toggle_off_the_field() {
        let mut p = on_field();
        p.button(KEYCODE_BUTTON_R1, true, false, None, C);
        p.button(KEYCODE_BUTTON_R1, false, false, None, C);
        assert_eq!(
            drain(&mut p),
            vec![
                PadOut::Key { code: KEY_SLOT[3], down: true },
                PadOut::Key { code: KEY_SLOT[3], down: false },
            ],
            "in the field it stays the belt slot"
        );

        let mut p = Pad::default(); // no HUD drawn: a menu
        assert!(!p.field());
        p.button(KEYCODE_BUTTON_R1, true, false, None, C);
        p.button(KEYCODE_BUTTON_R1, false, false, None, C);
        assert_eq!(
            drain(&mut p),
            vec![
                PadOut::Key { code: KEY_PAGE, down: true },
                PadOut::Key { code: KEY_PAGE, down: false },
            ],
            "off the field it is '#', which is what TogleAutoInfo hangs on"
        );
    }

    /// A remap moves both halves together, because the substitution reads the
    /// action and not the code.
    #[test]
    fn moving_belt_slot_four_moves_the_info_key_with_it() {
        let mut p = Pad::default();
        p.overrides.insert(KEYCODE_BUTTON_L1, Some(PadAction::Key(KEY_SLOT[3])));
        p.overrides.insert(KEYCODE_BUTTON_R1, Some(PadAction::Key(KEY_SLOT[2])));
        p.button(KEYCODE_BUTTON_L1, true, false, None, C);
        assert_eq!(drain(&mut p), vec![PadOut::Key { code: KEY_PAGE, down: true }]);
        p.button(KEYCODE_BUTTON_R1, true, false, None, C);
        assert_eq!(drain(&mut p), vec![PadOut::Key { code: KEY_SLOT[2], down: true }]);
    }

    #[test]
    fn the_default_layout_reaches_every_field_control() {
        let ov = no_overrides();
        // The five belt slots, the combo, the page flip, OK and cancel: the
        // whole of the guest's own on-screen HUD, plus the bar's map and save.
        for key in [
            KEY_OK,
            KEY_CLEAR,
            KEY_SLOT[0],
            KEY_SLOT[1],
            KEY_SLOT[2],
            KEY_SLOT[3],
            KEY_SLOT[4],
            KEY_COMBO,
            KEY_PAGE,
            KEY_MAP,
            KEY_SAVE,
        ] {
            assert!(
                pos_for_key(key, &ov).is_some(),
                "guest key {key} is not reachable from the default pad layout"
            );
        }
    }

    #[test]
    fn the_default_layout_keeps_hero3s_face_and_shoulder_buttons() {
        assert_eq!(default_button_action(KEYCODE_BUTTON_A), Some(PadAction::Key(KEY_OK)));
        assert_eq!(default_button_action(KEYCODE_BUTTON_B), Some(PadAction::Key(KEY_CLEAR)));
        assert_eq!(default_button_action(KEYCODE_BUTTON_X), Some(PadAction::Key(KEY_SLOT[0])));
        assert_eq!(default_button_action(KEYCODE_BUTTON_Y), Some(PadAction::Key(KEY_SLOT[1])));
        assert_eq!(default_button_action(KEYCODE_BUTTON_L1), Some(PadAction::Key(KEY_SLOT[2])));
        assert_eq!(default_button_action(KEYCODE_BUTTON_R1), Some(PadAction::Key(KEY_SLOT[3])));
        assert_eq!(default_button_action(KEYCODE_BUTTON_L2), Some(PadAction::Key(KEY_SLOT[4])));
        assert_eq!(default_button_action(KEYCODE_BUTTON_R2), Some(PadAction::Key(KEY_COMBO)));
        // The two moves off hero3, and the reason is the test below.
        assert_eq!(default_button_action(KEYCODE_BUTTON_SELECT), Some(PadAction::Key(KEY_PAGE)));
        assert_eq!(default_button_action(KEYCODE_BUTTON_START), Some(PadAction::Dash));
        // The bonus pair, for pads that have sticks to press.
        assert_eq!(default_button_action(KEYCODE_BUTTON_THUMBR), Some(PadAction::Key(KEY_MAP)));
        assert_eq!(default_button_action(KEYCODE_BUTTON_THUMBL), Some(PadAction::Key(KEY_SAVE)));
        assert_eq!(default_button_action(0x1234), None);
    }

    /// **The defect a player's own pad found.** Plenty of Bluetooth pads have
    /// no analog sticks, so `BUTTON_THUMBL/THUMBR` do not exist on them — and
    /// the first cut of this layout put the belt page (the one control the
    /// operator asked for by name) on R3, where such a pad could never reach
    /// it. Every control a player needs during a fight, plus the page flip,
    /// must therefore sit on a button that a stick-less pad has.
    ///
    /// The map and the save are deliberately *not* in this list: they are the
    /// in-game top bar's own cells, drawn on every field screen, so a tap
    /// reaches them and nothing is lost by leaving them on the stick presses.
    #[test]
    fn the_whole_field_kit_avoids_the_stick_presses() {
        let ov = no_overrides();
        let needed: [(PadAction, &str); 10] = [
            (PadAction::Key(KEY_OK), "OK"),
            (PadAction::Key(KEY_CLEAR), "cancel"),
            (PadAction::Key(KEY_SLOT[0]), "slot 1"),
            (PadAction::Key(KEY_SLOT[1]), "slot 2"),
            (PadAction::Key(KEY_SLOT[2]), "slot 3"),
            (PadAction::Key(KEY_SLOT[3]), "slot 4"),
            (PadAction::Key(KEY_SLOT[4]), "slot 5"),
            (PadAction::Key(KEY_COMBO), "combo"),
            (PadAction::Key(KEY_PAGE), "belt page"),
            (PadAction::Dash, "dash"),
        ];
        for (action, what) in needed {
            let pos = pos_for_action(action, &ov)
                .unwrap_or_else(|| panic!("{what} is not on the pad at all"));
            assert!(
                !matches!(pos, PadPos::StickPress { .. }),
                "{what} sits on a stick press, and a pad without sticks cannot reach it"
            );
            assert!(
                !matches!(pos, PadPos::Dpad),
                "{what} sits on the hat, which steers — it would fight the walk"
            );
        }
    }

    #[test]
    fn overrides_win_and_none_unmaps() {
        let mut ov = no_overrides();
        ov.insert(KEYCODE_BUTTON_A, Some(PadAction::Key(KEY_SLOT[4])));
        ov.insert(KEYCODE_BUTTON_B, None);
        assert_eq!(button_action_in(KEYCODE_BUTTON_A, &ov), Some(PadAction::Key(KEY_SLOT[4])));
        assert_eq!(button_action_in(KEYCODE_BUTTON_B, &ov), None);
        assert_eq!(button_action_in(KEYCODE_BUTTON_L1, &ov), Some(PadAction::Key(KEY_SLOT[2])));
        // And the badge follows the binding, not the default.
        assert_eq!(pos_for_key(KEY_SLOT[4], &ov), pos_for_code(KEYCODE_BUTTON_A));
    }

    #[test]
    fn map_lines_parse_and_garbage_is_ignored() {
        assert_eq!(parse_map_line("96=OK"), Some((96, Some(PadAction::Key(KEY_OK)))));
        assert_eq!(parse_map_line(" 107 = PAGE "), Some((107, Some(PadAction::Key(KEY_PAGE)))));
        assert_eq!(parse_map_line("104=#"), Some((104, Some(PadAction::Key(KEY_PAGE)))));
        assert_eq!(parse_map_line("104=*"), Some((104, Some(PadAction::Key(KEY_SLOT[3])))));
        assert_eq!(parse_map_line("108=DASH"), Some((108, Some(PadAction::Dash))));
        assert_eq!(parse_map_line("106=SAVE"), Some((106, Some(PadAction::Key(KEY_SAVE)))));
        assert_eq!(parse_map_line("97=NONE"), Some((97, None)));
        assert_eq!(parse_map_line("# a comment"), None);
        assert_eq!(parse_map_line(""), None);
        assert_eq!(parse_map_line("96=WHAT"), None);
        assert_eq!(parse_map_line("abc=OK"), None);
    }

    /// **The defect this closed.** The shell had its own copy of the file's
    /// meaning, and it put every binding in before every `NONE`, so `96=NONE`
    /// followed by `96=OK` disabled the button — the opposite of what the file
    /// says. One parser, last line wins, and the shell calls it.
    #[test]
    fn the_last_line_about_a_code_is_the_one_in_force() {
        let (map, bad) = parse_map_reporting(
            "# my pad
96=NONE
96=OK
99=SAVE
99=NONE

nonsense
",
        );
        assert_eq!(map.get(&96), Some(&Some(PadAction::Key(KEY_OK))));
        assert_eq!(map.get(&99), Some(&None));
        assert_eq!(bad, vec![(7, "nonsense".to_string())]);
        // And what the game resolves through it agrees.
        assert_eq!(button_action_in(96, &map), Some(PadAction::Key(KEY_OK)));
        assert_eq!(button_action_in(99, &map), None);
    }

    #[test]
    fn trigger_hysteresis_cannot_chatter_on_a_threshold() {
        assert!(!trigger_held(false, 0.4));
        assert!(trigger_held(false, 0.5));
        assert!(trigger_held(true, 0.3));
        assert!(!trigger_held(true, 0.2));
        assert!(!trigger_held(false, 0.3));
    }

    #[test]
    fn a_button_press_and_release_are_one_key_each() {
        let mut p = Pad::default();
        p.button(KEYCODE_BUTTON_A, true, false, None, C);
        p.button(KEYCODE_BUTTON_A, true, true, None, C); // auto-repeat
        p.button(KEYCODE_BUTTON_A, true, false, None, C); // a stray second down
        p.button(KEYCODE_BUTTON_A, false, false, None, C);
        let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
        assert_eq!(
            out,
            vec![
                PadOut::Key { code: KEY_OK, down: true },
                PadOut::Key { code: KEY_OK, down: false }
            ]
        );
    }

    /// The guest's five mailboxes and the order `mainKeyPressed` empties them
    /// once per tick (0x9CA5C): the press, or else the move — a tick that took
    /// a press skips the move — then the release, the keydown, the keyup. A
    /// press also files the guest's own move at the same point
    /// (`onHandleEvent+0x11c`), and `Vm::pad_pump` folds the pad's drag into
    /// that one. `walks` counts moves handled while the finger owns the stick.
    #[derive(Default)]
    struct Guest {
        slots: HashMap<u32, PadOut>,
        handled: Vec<PadOut>,
        owned: bool,
        walks: u32,
    }

    impl Guest {
        fn pump(&mut self, p: &mut Pad) {
            for _ in 0..5 {
                if let (Some(PadOut::Finger { kind: Touch::Moved, x, y }), true) =
                    (p.peek(), self.slots.contains_key(&0xf2))
                {
                    self.slots.insert(0xf2, PadOut::Finger { kind: Touch::Moved, x, y });
                    p.pop_delivered();
                    continue;
                }
                let slots = &self.slots;
                let next = p.next_ready_in_order(|off| if slots.contains_key(&off) { Slot::Pad } else { Slot::Free });
                let Some(e) = next else { break };
                if let PadOut::Finger { kind: Touch::Pressed, x, y } = e {
                    self.slots.insert(0xf2, PadOut::Finger { kind: Touch::Moved, x, y });
                }
                self.slots.insert(e.mailbox(), e);
            }
        }

        fn tick(&mut self) {
            let first = match self.slots.remove(&0xf0) {
                Some(press) => Some(press),
                None => self.slots.remove(&0xf2),
            };
            let rest = [0xf1, 0xe4, 0xe5].map(|off| self.slots.remove(&off));
            for e in first.into_iter().chain(rest.into_iter().flatten()) {
                match e {
                    PadOut::Finger { kind: Touch::Pressed, .. } => self.owned = true,
                    PadOut::Finger { kind: Touch::Moved, .. } if self.owned => self.walks += 1,
                    PadOut::Finger { kind: Touch::Released, .. } => self.owned = false,
                    _ => {}
                }
                self.handled.push(e);
            }
        }

        fn run(&mut self, p: &mut Pad, ticks: usize) {
            for _ in 0..ticks {
                self.pump(p);
                self.tick();
            }
        }
    }

    /// The review's case: a d-pad tap shorter than a tick queues press, drag
    /// and release together. Filled at once, the guest takes the press, skips
    /// the move, takes the release, and the drag arrives a tick later to a
    /// stick nobody holds — the tap never walks. The release has to wait.
    #[test]
    fn a_tap_shorter_than_a_tick_still_walks() {
        let mut p = on_field();
        p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, C, None);
        p.axes(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, C, None);
        let mut g = Guest::default();
        g.run(&mut p, 6);
        assert_eq!(
            g.handled,
            vec![
                PadOut::Finger { kind: Touch::Pressed, x: C.0, y: C.1 },
                PadOut::Finger { kind: Touch::Moved, x: C.0 + 48, y: C.1 },
                PadOut::Finger { kind: Touch::Released, x: C.0 + 48, y: C.1 },
            ]
        );
        assert_eq!(g.walks, 1);
        assert_eq!(p.queued(), 0);
    }

    /// Two quick taps: the first one's release and the second one's press
    /// would share a tick, the guest would take the press first, and the old
    /// release (same pointer id) would let go of the new finger. Each tap has
    /// to walk.
    #[test]
    fn two_quick_taps_both_walk() {
        let mut p = on_field();
        for _ in 0..2 {
            p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, C, None);
            p.axes(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, C, None);
        }
        let mut g = Guest::default();
        g.run(&mut p, 12);
        let kinds: Vec<Touch> = g
            .handled
            .iter()
            .map(|e| match e {
                PadOut::Finger { kind, .. } => *kind,
                PadOut::Key { .. } => panic!("a key on the field stick: {e:?}"),
            })
            .collect();
        use Touch::*;
        assert_eq!(kinds, [Pressed, Moved, Released, Pressed, Moved, Released]);
        assert_eq!(g.walks, 2);
        assert!(!g.owned, "the stick ends let go");
    }

    /// A held stick is not slowed down: the drags after the first ride the
    /// move mailbox one per tick, and the release still comes last.
    #[test]
    fn a_held_stick_walks_every_tick_and_lets_go_last() {
        let mut p = on_field();
        let mut g = Guest::default();
        p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, C, None);
        g.run(&mut p, 1);
        for i in 0..4 {
            p.axes(1.0, 0.1 * i as f32, 0.0, 0.0, 0.0, 0.0, C, None);
            g.run(&mut p, 1);
        }
        p.axes(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, C, None);
        g.run(&mut p, 4);
        assert!(matches!(g.handled.last(), Some(PadOut::Finger { kind: Touch::Released, .. })));
        assert!(g.walks >= 4, "{} walks", g.walks);
        assert!(!g.owned);
    }

    /// Keyup then keydown inside one tick would reach the guest as down, up:
    /// the key reads released while held. The keydown waits a tick.
    #[test]
    fn a_re_press_does_not_overtake_its_release() {
        let mut p = Pad::default();
        let mut g = Guest::default();
        p.button(KEYCODE_BUTTON_A, true, false, None, C);
        g.run(&mut p, 1);
        p.button(KEYCODE_BUTTON_A, false, false, None, C);
        p.button(KEYCODE_BUTTON_A, true, false, None, C);
        g.run(&mut p, 3);
        let edges: Vec<bool> = g
            .handled
            .iter()
            .map(|e| match e {
                PadOut::Key { down, .. } => *down,
                PadOut::Finger { .. } => panic!("a finger from a face button: {e:?}"),
            })
            .collect();
        assert_eq!(edges, [true, false, true]);
    }

    /// A real finger's event in a mailbox is a different pointer: the pad does
    /// not wait behind it.
    #[test]
    fn someone_elses_finger_does_not_hold_the_pad_back() {
        let mut p = on_field();
        p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, C, None);
        assert!(matches!(p.next_ready(|_| true), Some(PadOut::Finger { kind: Touch::Pressed, .. })));
        assert!(matches!(p.next_ready(|_| true), Some(PadOut::Finger { kind: Touch::Moved, .. })));
        p.axes(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, C, None);
        let other_moving = |off: u32| if off == 0xf2 { Slot::Other } else { Slot::Free };
        assert!(matches!(
            p.next_ready_in_order(other_moving),
            Some(PadOut::Finger { kind: Touch::Released, .. })
        ));
        let ours_moving = |off: u32| if off == 0xf2 { Slot::Pad } else { Slot::Free };
        p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, C, None);
        assert!(matches!(p.next_ready_in_order(ours_moving), Some(PadOut::Finger { kind: Touch::Pressed, .. })));
    }

    /// The whole point of the queue: the guest drops a second key of the same
    /// kind inside one tick, so the host must hold it back instead.
    #[test]
    fn two_presses_in_one_tick_are_paced_by_the_mailbox() {
        let mut p = Pad::default();
        p.button(KEYCODE_BUTTON_X, true, false, None, C);
        p.button(KEYCODE_BUTTON_Y, true, false, None, C);
        // Tick 1: the down slot is free, one goes.
        assert_eq!(
            p.next_ready(|_| true),
            Some(PadOut::Key { code: KEY_SLOT[0], down: true })
        );
        // Still tick 1: the slot is now taken, so nothing else may go.
        assert_eq!(p.next_ready(|off| off != 0xe4), None);
        // Tick 2: free again.
        assert_eq!(
            p.next_ready(|_| true),
            Some(PadOut::Key { code: KEY_SLOT[1], down: true })
        );
        assert_eq!(p.queued(), 0);
    }

    #[test]
    fn the_stick_presses_the_centre_first_and_then_drags() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        let centre = stick_centre(270);
        assert_eq!(centre, (65, 205));
        p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
        assert_eq!(
            out,
            vec![
                PadOut::Finger { kind: Touch::Pressed, x: 65, y: 205 },
                PadOut::Finger { kind: Touch::Moved, x: 65 + 48, y: 205 },
            ],
            "the press must land on the centre — in the floating scheme the press point \
             becomes the origin, so pressing off-centre skews every later angle"
        );
    }

    /// The deflection that engages the finger must clear the guest's own
    /// ±10 px direction deadzone, or the hero faces a way it never walks.
    #[test]
    fn the_engage_deflection_clears_the_guest_deadzone() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        let centre = stick_centre(270);
        p.axes(ENGAGE, 0.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
        let last = out.last().copied().expect("the finger went down");
        match last {
            PadOut::Finger { x, .. } => assert!(
                x - centre.0 > 10,
                "engage puts the finger {} px out; the guest wants more than 10",
                x - centre.0
            ),
            other => panic!("expected a drag, got {other:?}"),
        }
    }

    /// **The defect this test exists for.** `RELEASE` used to be 0.12, below
    /// the resting drift of a very ordinary Bluetooth pad, so a stick that had
    /// been pushed once and then let go kept the synthetic finger down and the
    /// hero walked for ever. On a device this looks like "the pad is stuck",
    /// and it is the first thing to check — before "does the stick walk".
    #[test]
    fn a_drifting_stick_at_rest_lets_go_of_the_joystick() {
        // Both thresholds must clear the guest's own deadzone, or a finger can
        // be down in a band where it steers nothing.
        assert!(RELEASE > GUEST_DEAD, "a finger that is down must always steer");
        assert!(ENGAGE > RELEASE, "and the pair must still be hysteretic");
        // The threshold is on the MAGNITUDE, so a pad that rests off-centre
        // on both axes has less room than one that rests off on one. Both
        // shapes, out to a drift far past anything the old 0.12 could take.
        let cases: [(f32, f32); 6] =
            [(0.0, 0.0), (0.12, 0.0), (0.20, 0.0), (0.0, -0.22), (0.10, -0.10), (0.15, 0.15)];
        for (dx, dy) in cases {
            assert!(
                (dx * dx + dy * dy).sqrt() < RELEASE,
                "the case itself must be inside the deadzone this test is about"
            );
            let mut p = Pad::default();
            p.saw_touch_ui(TouchUi::default());
            p.frame_start(true);
            let centre = stick_centre(270);
            p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, centre, None);
            while p.next_ready(|_| true).is_some() {}
            // The thumb comes off; the pad settles back to its own idea of
            // centre, which is not zero.
            p.axes(dx, dy, 0.0, 0.0, 0.0, 0.0, centre, None);
            let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
            assert!(
                out.iter()
                    .any(|o| matches!(o, PadOut::Finger { kind: Touch::Released, .. })),
                "a pad resting at ({dx}, {dy}) still holds the joystick: {out:?}"
            );
        }
    }

    /// The same drift off the field must let go of the cursor key, or a menu
    /// scrolls on its own.
    #[test]
    fn a_drifting_stick_at_rest_lets_go_of_the_menu_key() {
        let mut p = Pad::default();
        let centre = stick_centre(270);
        p.axes(0.0, 1.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        while p.next_ready(|_| true).is_some() {}
        p.axes(0.0, 0.20, 0.0, 0.0, 0.0, 0.0, centre, None);
        assert_eq!(
            p.next_ready(|_| true),
            Some(PadOut::Key { code: KEY_DOWN, down: false })
        );
    }

    #[test]
    fn releasing_the_stick_lets_go_of_the_joystick() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        let centre = stick_centre(270);
        p.axes(0.0, -1.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        while p.next_ready(|_| true).is_some() {}
        p.axes(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        assert_eq!(
            p.next_ready(|_| true),
            Some(PadOut::Finger { kind: Touch::Released, x: 65, y: 205 - 48 }),
            "the release must carry the point the finger was last at, like a real lift"
        );
    }

    #[test]
    fn drags_coalesce_so_a_flicked_stick_cannot_flood_the_queue() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        let centre = stick_centre(270);
        p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        // Nothing has been handed over yet; twenty more samples arrive.
        for i in 0..20 {
            let a = i as f32 / 20.0;
            p.axes(a.cos(), a.sin(), 0.0, 0.0, 0.0, 0.0, centre, None);
        }
        assert_eq!(p.queued(), 2, "one press and one drag, whatever the sample rate");
    }

    /// Off the field the same stick has to be keys: menus are key-driven, and
    /// a synthetic finger on the joystick region would press whatever the
    /// screen has there.
    #[test]
    fn off_the_field_the_stick_is_menu_keys() {
        let mut p = Pad::default();
        let centre = stick_centre(270);
        p.axes(0.0, 1.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        assert_eq!(p.next_ready(|_| true), Some(PadOut::Key { code: KEY_DOWN, down: true }));
        // Held: no repeat, the guest needs none.
        p.axes(0.0, 1.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        assert_eq!(p.next_ready(|_| true), None);
        // Turned: release then press, in that order.
        p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        assert_eq!(p.next_ready(|_| true), Some(PadOut::Key { code: KEY_DOWN, down: false }));
        assert_eq!(p.next_ready(|_| true), Some(PadOut::Key { code: KEY_RIGHT, down: true }));
    }

    #[test]
    fn leaving_the_field_with_the_stick_held_lets_go_of_it() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        let centre = stick_centre(270);
        p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        while p.next_ready(|_| true).is_some() {}
        // The HUD stops being drawn (a popup, a menu): the next painted frame
        // reports it, and the host re-steers.
        assert!(p.frame_start(true), "the latch moved, so the caller re-steers");
        p.steer(centre);
        let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
        assert!(
            out.contains(&PadOut::Finger { kind: Touch::Released, x: 65 + 48, y: 205 }),
            "the joystick must be let go of, or the hero walks through the menu: {out:?}"
        );
        assert!(out.contains(&PadOut::Key { code: KEY_RIGHT, down: true }));
    }

    #[test]
    fn release_all_does_not_release_a_direction_the_guest_never_saw_pressed() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        p.button(KEYCODE_DPAD_UP, true, false, None, C);
        while p.next_ready(|_| true).is_some() {}
        p.release_all();
        let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
        assert_eq!(
            out,
            vec![PadOut::Finger { kind: Touch::Released, x: C.0, y: C.1 - 48 }],
            "letting go of the finger is the whole release; a stray KeyRelease(-1) is not: {out:?}"
        );
        assert_eq!(p.dhat, (0, 0));
    }

    #[test]
    fn release_all_lets_go_of_buttons_stick_and_finger() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        let centre = stick_centre(270);
        p.button(KEYCODE_BUTTON_A, true, false, None, C);
        p.axes(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, centre, None);
        while p.next_ready(|_| true).is_some() {}
        p.release_all();
        let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
        assert!(out.contains(&PadOut::Key { code: KEY_OK, down: false }));
        assert!(out.iter().any(|o| matches!(o, PadOut::Finger { kind: Touch::Released, .. })));
    }

    #[test]
    fn dash_is_two_direction_presses_and_no_release() {
        let mut p = Pad::default();
        let mut ov = no_overrides();
        ov.insert(KEYCODE_BUTTON_R1, Some(PadAction::Dash));
        p.overrides = ov;
        p.button(KEYCODE_BUTTON_R1, true, false, Some(KEY_LEFT), C);
        p.button(KEYCODE_BUTTON_R1, false, false, Some(KEY_LEFT), C);
        let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
        assert_eq!(
            out,
            vec![
                PadOut::Key { code: KEY_LEFT, down: true },
                PadOut::Key { code: KEY_LEFT, down: true },
            ],
            "arm then dash, and never a release — a direction release ends the walk"
        );
    }

    #[test]
    fn dash_without_a_direction_does_nothing() {
        let mut p = Pad::default();
        let mut ov = no_overrides();
        ov.insert(KEYCODE_BUTTON_R1, Some(PadAction::Dash));
        p.overrides = ov;
        p.button(KEYCODE_BUTTON_R1, true, false, None, C);
        assert_eq!(p.queued(), 0);
    }

    /// The hole this closed: a pad that reports its d-pad as KEY events (many
    /// do) would otherwise have sent WIPI −1..−4, and those animate a walk
    /// that never moves (see the module header). Both kinds of pad must end up
    /// on the joystick.
    #[test]
    fn a_dpad_key_steers_the_joystick_instead_of_sending_a_direction() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        p.button(KEYCODE_DPAD_RIGHT, true, false, None, C);
        let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
        assert_eq!(
            out,
            vec![
                PadOut::Finger { kind: Touch::Pressed, x: C.0, y: C.1 },
                PadOut::Finger { kind: Touch::Moved, x: C.0 + 48, y: C.1 },
            ],
            "a d-pad press must become a finger on the joystick, not a key: {out:?}"
        );
        p.button(KEYCODE_DPAD_RIGHT, false, false, None, C);
        assert_eq!(
            p.next_ready(|_| true),
            Some(PadOut::Finger { kind: Touch::Released, x: C.0 + 48, y: C.1 })
        );
    }

    #[test]
    fn a_dpad_key_is_still_a_cursor_key_in_a_menu() {
        let mut p = Pad::default();
        p.button(KEYCODE_DPAD_DOWN, true, false, None, C);
        assert_eq!(p.next_ready(|_| true), Some(PadOut::Key { code: KEY_DOWN, down: true }));
        p.button(KEYCODE_DPAD_DOWN, false, false, None, C);
        assert_eq!(p.next_ready(|_| true), Some(PadOut::Key { code: KEY_DOWN, down: false }));
    }

    /// Two directions at once is a diagonal, and letting go of one leaves the
    /// other steering — the same as a stick held between two axes.
    #[test]
    fn two_dpad_directions_make_a_diagonal_and_unwind_cleanly() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        p.button(KEYCODE_DPAD_RIGHT, true, false, None, C);
        p.button(KEYCODE_DPAD_UP, true, false, None, C);
        while p.next_ready(|_| true).is_some() {}
        assert_eq!(p.dhat, (1, -1));
        p.button(KEYCODE_DPAD_UP, false, false, None, C);
        assert_eq!(p.dhat, (1, 0));
        let out: Vec<PadOut> = std::iter::from_fn(|| p.next_ready(|_| true)).collect();
        assert_eq!(
            out,
            vec![PadOut::Finger { kind: Touch::Moved, x: C.0 + 48, y: C.1 }],
            "still steering right, and the finger stays down: {out:?}"
        );
        p.button(KEYCODE_DPAD_RIGHT, false, false, None, C);
        assert_eq!(p.dhat, (0, 0));
        assert!(matches!(
            p.next_ready(|_| true),
            Some(PadOut::Finger { kind: Touch::Released, .. })
        ));
    }

    /// The d-pad's auto-repeat must not double-count into the hat vector, or
    /// releasing it would leave the hero walking for ever.
    #[test]
    fn a_repeating_dpad_cannot_unbalance_the_hat() {
        let mut p = Pad::default();
        p.saw_touch_ui(TouchUi::default());
        p.frame_start(true);
        p.button(KEYCODE_DPAD_LEFT, true, false, None, C);
        for _ in 0..5 {
            p.button(KEYCODE_DPAD_LEFT, true, true, None, C); // system repeat
            p.button(KEYCODE_DPAD_LEFT, true, false, None, C); // a stray re-down
        }
        assert_eq!(p.dhat, (-1, 0));
        p.button(KEYCODE_DPAD_LEFT, false, false, None, C);
        assert_eq!(p.dhat, (0, 0), "the hat must return to rest");
    }

    /// **The defect this closed.** The badge painter asked "are the badges
    /// showing?" before taking the "the HUD is in this picture" flag, so while
    /// no pad was attached the flag was never spent. It then described
    /// whatever scene had last drawn the field HUD — and the first frame after
    /// a pad was plugged in stamped that scene's badges onto the screen the
    /// player was actually looking at.
    #[test]
    fn a_stale_hud_sighting_cannot_be_spent_on_a_later_picture() {
        let field = TouchUi { page: (209, 198), ..TouchUi::default() };
        let mut p = Pad::default();
        p.hud = crate::padhud::PadHud::Auto; // no pad yet, so: off
        assert!(!p.badges_visible());

        // Gameplay with no pad. Every frame sees the HUD and paints nothing,
        // and each sighting must be spent by the frame it belongs to.
        for _ in 0..3 {
            p.saw_touch_ui(field);
            assert_eq!(p.take_badge_target(), None, "nothing to paint with the badges off");
        }
        // The player leaves the field — no more sightings — and then plugs a
        // pad in. The next painted frame must find nothing to stamp.
        p.present = true;
        assert!(p.badges_visible());
        assert_eq!(
            p.take_badge_target(),
            None,
            "a sighting from a scene that is no longer on screen was carried over"
        );
        // And a fresh sighting still works.
        p.saw_touch_ui(field);
        assert_eq!(p.take_badge_target(), Some(field));
        assert_eq!(p.take_badge_target(), None, "one sighting, one paint");
    }

    #[test]
    fn every_mailbox_offset_is_the_one_the_guest_drains() {
        // The five one-slot mailboxes of ANALYSIS.md §37.5, in the order
        // `mainKeyPressed` empties them.
        assert_eq!(PadOut::Finger { kind: Touch::Pressed, x: 0, y: 0 }.mailbox(), 0xf0);
        assert_eq!(PadOut::Finger { kind: Touch::Moved, x: 0, y: 0 }.mailbox(), 0xf2);
        assert_eq!(PadOut::Finger { kind: Touch::Released, x: 0, y: 0 }.mailbox(), 0xf1);
        assert_eq!(PadOut::Key { code: KEY_OK, down: true }.mailbox(), 0xe4);
        assert_eq!(PadOut::Key { code: KEY_OK, down: false }.mailbox(), 0xe5);
    }

    #[test]
    fn the_pointer_id_cannot_collide_with_a_real_finger() {
        assert!(PAD_POINTER > 0 && PAD_POINTER <= 0x7f);
        assert!(PAD_POINTER >= 0x10, "the shell counts real fingers up from 0");
    }

    #[test]
    fn the_button_key_table_matches_the_guest_jump_table() {
        // `TouchReleased+0x1ac`: case 0 '1', 1 '3', 2 '7', 3 '9', 4 '*', 5 '0'.
        assert_eq!(BUTTON_KEYS, [0x31, 0x33, 0x37, 0x39, 0x2a, 0x30]);
    }
}
