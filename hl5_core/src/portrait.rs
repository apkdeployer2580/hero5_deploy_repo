//! 세로 — the phone stood upright: a 240x320 canvas over a feature-phone keypad.
//!
//! The fourth value of the 해상도 row ([`crate::screen`]). It is the
//! arrangement of a modified build the user had been playing
//! (`dist/영웅서기5 vc58 해상도추가.apk`, package `kr.heroeslore.five.vc58dual`),
//! carried into this tree so it is one choice on the options list instead of a
//! second installation. That build was three changes on top of vc58, and so is
//! this:
//!
//! | that build | here |
//! |---|---|
//! | `libmain.so`: canvas forced to 240x320 | [`crate::widened_canvas_for`] → [`CANVAS`] |
//! | `libmain.so`: `fit_screen`'s long/short swap removed | `Vm::fit_screen` reads the display the canvas's way round |
//! | a second `libHeroesLore5.so` with 111 words changed | [`apply`] writes the same 111 words into guest memory |
//! | `PhoneLayout`: a 24-key pad that synthesised touches and pad keys | `PhoneKeypad.java` → [`keypad_action`] → the pad layer |
//!
//! # Why 240x320 is not a stretch
//!
//! The guest already has a 240-wide layout. `StateGame::SetTouchKeypad` and
//! `GMenu::DrawSkillBelt` both open with `cmp GetWidth(), #0xf0` and take a
//! separate table of six button offsets when the screen is 240 wide or less —
//! the arc of skill buttons the 2011 QVGA phones had. The modified build kept
//! that and moved things inside it; it did not invent a layout.
//!
//! # What the 111 words do
//!
//! Read off the two binaries, not guessed — `tools_re/disasm.py` on both:
//!
//! * `GMenu::DrawSkillBelt` reads its six belt positions from a `.rodata`
//!   table (`0x157D1C + 0x78`, 24 halfwords: x-offsets from the right edge,
//!   then y-offsets from the bottom, for each of two layouts). The 240 layout's
//!   twelve are rewritten from the arc `W-109..W-40 / H-78..H-253` to one row,
//!   `W-232, W-200 .. W-72` at `H-46`, and the belt's anchor moves with it
//!   (`+0x184`, `+0x19c`). Its twin at 0x159C38 — the copy
//!   `StateGame::SetTouchKeypad` reads for the *touch* rects — is left alone:
//!   on this layout the skill buttons are pressed from the keypad.
//! * `StateGame::DrawTouchUi` stops drawing the joystick plates and the touch
//!   cancel strip (the `bl`s at `+0x114 +0x17c +0x2d8 +0x334 +0x4c4 +0x50c
//!   +0x5ac` become `nop`), and in the space the cancel strip's code occupied
//!   (`+0x1a8`) draws the keypad digit under each belt slot — `1 3 7 9 * 0`,
//!   the keys that fire them — with the guest's own `BFont::DrawText`. The
//!   bottom bar it draws is moved to the corners (`+0x378..+0x410`).
//! * `StateGame::TouchPressed` skips the top bar's hit test (`+0x7bc`) and
//!   answers the corners instead: `y` in 260..272 at `x < 30` is the map
//!   (−6), at `x >= 210` the shop (−7), plus a 3x3 spot at (109, 40) for the
//!   save (−10), each through the state's own `KeyPress` (vptr+0x1c). The
//!   skill buttons' hit radius goes from 30 to 50 (`+0x1b0 +0x694 +0x6a0
//!   +0x980 +0x9d8`).
//! * `mainKeyPressed+0x324` branches through a four-word stub parked in
//!   `TouchPressed+0x844`, and `Map::Map_Draw_Minimap+0x20c` moves the minimap
//!   from `-40` to `-4` so it clears the 240-wide HUD.
//!
//! **This is one of the two places the host writes guest code** (the other is
//! [`crate::tenpages`]). It is still the original `.so` on disk; [`apply`] runs once, before the first guest
//! instruction and before any hook is placed, it checks every word it is
//! about to replace and writes none of them if one disagrees, and it is
//! reachable only from the 세로 setting. The landscape modes never execute a
//! byte of it, so the 1,241-frame A/B is untouched by construction.
//!
//! # The keypad
//!
//! The modified build's pad sent Android gamepad keycodes and fake touches
//! from Java. Here the 24 keys arrive as codes in their own range
//! ([`KEYPAD_CODE_BASE`]) and [`keypad_action`] resolves them **before** the
//! player's `hl5_gamepad.txt`, so remapping a real pad cannot rewire the
//! keypad. Everything else is the pad layer's: the queue that keeps two
//! presses in one tick from overwriting each other, the joystick finger at
//! (65, H−65) — which on a 320-tall canvas is exactly where the modified
//! build pressed — the dash as the guest's own double-tap, and direction keys
//! that steer in the field and move the cursor everywhere else.

use crate::gamepad::{
    PadAction, KEY_CLEAR, KEY_COMBO, KEY_DOWN, KEY_LEFT, KEY_MAP, KEY_OK, KEY_PAGE, KEY_RIGHT,
    KEY_SAVE, KEY_SLOT, KEY_UP,
};
use crate::Vm;

/// The guest's screen in the upright arrangement.
pub const CANVAS: (i32, i32) = (240, 320);

/// One contiguous rewrite: `was` must be what is there, `now` replaces it.
struct Run {
    /// The function the run sits in, or `None` for the one `.rodata` run.
    sym: Option<&'static str>,
    /// Offset into `sym`, or the image offset when `sym` is `None`.
    at: u32,
    was: &'static [u32],
    now: &'static [u32],
}

const DRAW_SKILL_BELT: &str = "_ZN5GMenu13DrawSkillBeltEiiihas";
const MAIN_KEY_PRESSED: &str = "_Z14mainKeyPressedv";
const MAP_DRAW_MINIMAP: &str = "_ZN3Map16Map_Draw_MinimapEiah";
const DRAW_TOUCH_UI: &str = "_ZN9StateGame11DrawTouchUiEv";
const TOUCH_PRESSED: &str = "_ZN9StateGame12TouchPressedEiii";

/// `NOP` (`mov r0, r0`).
const NOP: u32 = 0xe1a0_0000;

/// The modified build's 111 words, in address order. Generated by diffing
/// its `libHeroesLore5.so` against `libHeroesOrig5.so` — which is byte for
/// byte this tree's `payload/libHeroesLore5.so` — and mapping each run to the
/// function that owns it (`tools_re/guest_functions.txt`).
const RUNS: &[Run] = &[
    Run { sym: Some(DRAW_SKILL_BELT), at: 0x184, was: &[0xe240305f], now: &[0xe240301c] },
    Run { sym: Some(DRAW_SKILL_BELT), at: 0x19c, was: &[0xe2403017], now: &[0xe240301f] },
    Run { sym: Some(MAIN_KEY_PRESSED), at: 0x324, was: &[0xe3a02001], now: &[0xea01155f] },
    Run { sym: Some(MAP_DRAW_MINIMAP), at: 0x20c, was: &[0xe3e02027], now: &[0xe3e02003] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x114, was: &[0xebfdf8b1], now: &[NOP] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x17c, was: &[0xebfdf897], now: &[NOP] },
    Run {
        sym: Some(DRAW_TOUCH_UI),
        at: 0x1a8,
        was: &[
            0xebfe5c4d, 0xe794400a, 0xe3a07010, 0xe3a06022, 0xe5940000, 0xebfdc63d, 0xe1a09000,
            0xe5940000, 0xebfdc646, 0xe5948000, 0xe1a0a000, 0xe1a00008, 0xebfdc644, 0xe285cf7d,
            0xe58dc000, 0xe3a0c000, 0xe2403056, 0xe58dc008, 0xe1a01009, 0xe3a0c05a, 0xe1a00008,
            0xe1a0200a, 0xe58dc00c, 0xe58d7004, 0xe58d6010, 0xebfdf873, 0xe5d53272, 0xe3530001,
            0x0a0000bd, 0xe5940000, 0xebfdc624, 0xe1a0a000, 0xe5940000, 0xebfdc62d, 0xe5944000,
            0xe1a08000, 0xe2855f83, 0xe1a00004, 0xebfdc62a, 0xe1a0100a, 0xe2403056, 0xe1a02008,
            0xe1a00004, 0xe88d00a0, 0xe3a0c004, 0xe58dc008, 0xe3a0c0ff, 0xe58dc00c, 0xe58d6010,
            0xebfdf85b, 0xeaffff6d,
        ],
        // The label loop: `BFont::DrawText(font, fb, 0x17 + 0x20*i, 0x110,
        // "1"/"3"/"7"/"9"/"*"/"0", 1, white)` for i in 0..6, then back to the
        // function's exit; the three words after the loop are the six
        // two-byte strings it walks.
        now: &[
            NOP, 0xe794000a, 0xe5900000, 0xebfdc63f, 0xe1a08000, 0xe5947300, 0xe5977000,
            0xe28f9040, 0xe3a06017, 0xe3a05006, 0xe1a00007, 0xe1a01008, 0xe1a02006, 0xe3a03f44,
            0xe58d9000, 0xe3a0c001, 0xe58dc004, 0xe3e0c4ff, 0xe58dc008, 0xebfd8f83, 0xe2899002,
            0xe2866020, 0xe2555001, 0x1afffff1, 0xeaffff87, 0x00330031, 0x00390037, 0x0030002a,
            NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP, NOP,
            NOP, NOP, NOP, NOP, NOP, NOP,
        ],
    },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x2d8, was: &[0xebfdf840], now: &[NOP] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x334, was: &[0xebfdf829], now: &[NOP] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x378, was: &[0xe3a06096], now: &[0xe3a06000] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x398, was: &[0xe1a03006], now: &[0xe3a03f41] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x3a0, was: &[0xe3a02000], now: &[0xe1a02006] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x3ec, was: &[0xe1a03006], now: &[0xe3a03f41] },
    Run {
        sym: Some(DRAW_TOUCH_UI),
        at: 0x400,
        was: &[0xe2866014, 0xe3a0200f],
        now: &[0xe28660d2, 0xe24620c3],
    },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x410, was: &[0xe35600d2], now: &[0xe3570f45] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x4c4, was: &[0xebfdf7c5], now: &[NOP] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x50c, was: &[0xebfdf7b3], now: &[NOP] },
    Run { sym: Some(DRAW_TOUCH_UI), at: 0x5ac, was: &[0xebfdf78b], now: &[NOP] },
    Run { sym: Some(TOUCH_PRESSED), at: 0x150, was: &[0xe1e03008], now: &[0xea00019a] },
    Run {
        sym: Some(TOUCH_PRESSED),
        at: 0x1b0,
        was: &[0xe242e0d2, 0xe3a0c01e],
        now: &[0xe242e08c, 0xe3a0c032],
    },
    Run { sym: Some(TOUCH_PRESSED), at: 0x694, was: &[0xe243e0d2], now: &[0xe243e08c] },
    Run { sym: Some(TOUCH_PRESSED), at: 0x6a0, was: &[0xe3a0c01e], now: &[0xe3a0c032] },
    Run {
        sym: Some(TOUCH_PRESSED),
        at: 0x7bc,
        was: &[
            0xe1a0c00a, 0xe355001e, 0xe1a02006, 0xe3a0b092, 0xe1a0600c, 0xe3a0a001, 0xe1a0c004,
            0x8a000014, 0xe158000b, 0xda000012, 0xe28b4014, 0xe1580004, 0xca00000f, 0xe21630ff,
            0x05973000, 0x01a00007, 0x03e01005, 0x0a000185, 0xe3530001, 0x0a000180, 0xe58d2010,
            0xe58dc014, 0xe5973000, 0xe1a00007, 0xe3e01009,
        ],
        // `b +0x30c` over the top bar's test, then the corner/save test the
        // `+0x150` branch lands on: (y-260) <= 12 → x < 30 map / x-210 < 30
        // shop; (x-108) <= 2 && (y-39) <= 2 → save; else back to `+0x154`.
        now: &[
            0xeafffed2, 0xe2480f41, 0xe350000c, 0x8a000006, 0xe355001e, 0x33e01005, 0x3a00000c,
            0xe24500d2, 0xe350001e, 0x33e01006, 0x3a000008, 0xe245006c, 0xe3500002, 0x8a000003,
            0xe2480027, 0xe3500002, 0x93e01009, 0x9a000001, 0xe1e03008, 0xeafffe51, 0xe5973000,
            0xe1a00007, 0xe1a0e00f, 0xe593f01c, 0xeafffe19,
        ],
    },
    Run {
        sym: Some(TOUCH_PRESSED),
        at: 0x844,
        was: &[0xe355001e, 0xe2866001, 0xe20aa0ff, 0xe1a0b004],
        // The stub `mainKeyPressed+0x324` jumps to.
        now: &[0xe3520001, 0x0afeea1d, 0xe3a02001, 0xeafeea9b],
    },
    Run { sym: Some(TOUCH_PRESSED), at: 0x980, was: &[0xe3a0b01e], now: &[0xe3a0b032] },
    Run { sym: Some(TOUCH_PRESSED), at: 0x9d8, was: &[0xe3a0b01e], now: &[0xe3a0b032] },
    // `GMenu::DrawSkillBelt`'s position table (GOT + the literal at +0x44 is
    // 0x157D1C; the six-and-six it reads for a 240-wide screen start at +0x78).
    // `.rodata` has no symbol here, so this one run is an image offset.
    Run {
        sym: None,
        at: 0x157d94,
        was: &[0x0050006d, 0x00280028, 0x00280028, 0x0071004e, 0x00a5007d, 0x00fd00d1],
        now: &[0x00c800e8, 0x008800a8, 0x00480068, 0x002e002e, 0x002e002e, 0x002e002e],
    },
];

/// Write the upright layout into guest memory. Returns how many words it
/// wrote.
///
/// **All or nothing.** Every run is located and compared before the first
/// write, so a binary that is not the one these words were read from — or a
/// second call — gets an error that names the first disagreeing word and a
/// guest exactly as shipped, never a mixture of the two.
pub fn apply(vm: &mut Vm) -> Result<usize, String> {
    let mut plan: Vec<(u32, &'static [u32])> = Vec::with_capacity(RUNS.len());
    for run in RUNS {
        debug_assert_eq!(run.was.len(), run.now.len());
        let base = match run.sym {
            Some(s) => vm.symbol(s).ok_or_else(|| format!("portrait: `{s}` is not in this binary"))?,
            None => vm.elf.base,
        };
        let addr = base + run.at;
        for (i, want) in run.was.iter().enumerate() {
            let a = addr + 4 * i as u32;
            let got = vm.mem.read_u32(a);
            if got != *want {
                return Err(format!(
                    "portrait: {}+{:#x} holds {got:#010x}, expected {want:#010x} — \
                     not the binary this layout was read from; nothing written",
                    run.sym.unwrap_or(".rodata"),
                    run.at + 4 * i as u32,
                ));
            }
        }
        plan.push((addr, run.now));
    }
    let mut words = 0;
    for (addr, now) in plan {
        for (i, w) in now.iter().enumerate() {
            vm.mem.write_u32(addr + 4 * i as u32, *w);
            words += 1;
        }
    }
    Ok(words)
}

/// Keypad key `i` arrives as `KEYPAD_CODE_BASE + i`. Far above any Android
/// keycode (they stop in the low 300s), so it can never collide with a real
/// pad button and never appears in `hl5_gamepad.txt`.
pub const KEYPAD_CODE_BASE: i32 = 0x1_0000;

/// The 24 keys, in `PhoneKeypad.java`'s order — **the index is the contract
/// between the two files.** Six columns, four rows: a navigation block on the
/// left and the phone's number block on the right.
///
/// ```text
///    L    ↑    R   |  1   2   3
///    ←   OK    →   |  4   5   6
///         ↓  대시   |  7   8   9
///    ⚙  CLR  저장   |  *   0   #
/// ```
///
/// The number block is the feature phone's own: `1 3 9 * 0` are the belt
/// slots and `7` the combo because those are the keys the guest's belt *is*,
/// `2 4 6 8` walk, `5` is OK and `#` flips the belt page — and off the field
/// `#` is the info toggle and `*` deletes a character on the slot screen,
/// which is what those keys did on the phone this game was written for. `L`
/// opens the map, `R` and `CLR` cancel, as the modified build had them.
/// Index 12 is the blank under `←`: the Java side does not draw it. Index 18
/// was the modified build's `☎`, a second map key; it is now `⚙`, which opens
/// the keypad's layout editor in Java and is never sent here — `None`, so a
/// stray one does nothing either.
pub const KEYPAD: [(&str, Option<PadAction>); 24] = [
    ("L", Some(PadAction::Key(KEY_MAP))),
    ("↑", Some(PadAction::Key(KEY_UP))),
    ("R", Some(PadAction::Key(KEY_CLEAR))),
    ("1", Some(PadAction::Key(KEY_SLOT[0]))),
    ("2", Some(PadAction::Key(KEY_UP))),
    ("3", Some(PadAction::Key(KEY_SLOT[1]))),
    ("←", Some(PadAction::Key(KEY_LEFT))),
    ("OK", Some(PadAction::Key(KEY_OK))),
    ("→", Some(PadAction::Key(KEY_RIGHT))),
    ("4", Some(PadAction::Key(KEY_LEFT))),
    ("5", Some(PadAction::Key(KEY_OK))),
    ("6", Some(PadAction::Key(KEY_RIGHT))),
    ("", None),
    ("↓", Some(PadAction::Key(KEY_DOWN))),
    ("대시", Some(PadAction::Dash)),
    ("7", Some(PadAction::Key(KEY_COMBO))),
    ("8", Some(PadAction::Key(KEY_DOWN))),
    ("9", Some(PadAction::Key(KEY_SLOT[2]))),
    ("설정", None),
    ("CLR", Some(PadAction::Key(KEY_CLEAR))),
    ("저장", Some(PadAction::Key(KEY_SAVE))),
    ("*", Some(PadAction::Key(KEY_SLOT[3]))),
    ("0", Some(PadAction::Key(KEY_SLOT[4]))),
    ("#", Some(PadAction::Key(KEY_PAGE))),
];

/// The relaid HUD's six belt positions on a `w` x `h` canvas: the twelve
/// rewritten halfwords of `GMenu::DrawSkillBelt`'s 240 layout, `(W - xs[i],
/// H - ys[i])`, in the belt's own order (= `BUTTON_KEYS`). Table positions —
/// the plate is drawn six pixels lower (`gamepad::BELT_DRAW_DY`). The
/// touch rects in `StateGame` still hold the 240-wide arc, which this layout
/// no longer draws — a badge there would float over nothing (which is what
/// the first 세로 build did).
pub fn belt_row(w: i32, h: i32) -> [(i32, i32); 6] {
    let run = RUNS.iter().find(|r| r.sym.is_none()).expect("the .rodata run");
    let hw: Vec<i32> =
        run.now.iter().flat_map(|w| [(*w & 0xffff) as i32, (*w >> 16) as i32]).collect();
    let mut out = [(0, 0); 6];
    for (i, o) in out.iter_mut().enumerate() {
        *o = (w - hw[i], h - hw[6 + i]);
    }
    out
}

/// Where the relaid `DrawTouchUi` draws the belt page flip: measured off the
/// 240x320 frame (`out/shot/portrait-field.png`, icon 201..226 x 278..302),
/// as a 27 px box, so it clears the sixth belt icon beside it.
pub const PAGE_RECT: (i32, i32, i32, i32) = (200, 277, 27, 27);

/// The 세로 field HUD as drawn, for the badge pass and the mapping screen. OK
/// is not drawn in this layout (the keypad has it), so its rect is empty.
pub fn drawn_hud(w: i32, h: i32) -> crate::gamepad::TouchUi {
    crate::gamepad::TouchUi {
        buttons: belt_row(w, h),
        page: (PAGE_RECT.0, PAGE_RECT.1),
        ok: (0, 0, 0, 0),
        // 세로's HUD has no dash button either; 대시 is a keypad key.
        dash: None,
    }
}

/// Whether `code` is one of the keypad's.
pub fn is_keypad(code: i32) -> bool {
    (KEYPAD_CODE_BASE..KEYPAD_CODE_BASE + KEYPAD.len() as i32).contains(&code)
}

/// What a keypad code does: `Some(action)` for a keypad code (the inner
/// `None` is the blank key), `None` for anything else, which then goes to the
/// pad's own table.
pub fn keypad_action(code: i32) -> Option<Option<PadAction>> {
    if !is_keypad(code) {
        return None;
    }
    Some(KEYPAD[(code - KEYPAD_CODE_BASE) as usize].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every run replaces exactly the words it checks, and none overlaps
    /// another — two runs on one word would make the result depend on order.
    #[test]
    fn the_runs_are_disjoint_and_balanced() {
        let mut spans: Vec<(&str, u32, u32)> = RUNS
            .iter()
            .map(|r| {
                assert_eq!(r.was.len(), r.now.len(), "{:?}+{:#x}", r.sym, r.at);
                assert!(r.at % 4 == 0, "{:?}+{:#x} is not word aligned", r.sym, r.at);
                (r.sym.unwrap_or(""), r.at, r.at + 4 * r.was.len() as u32)
            })
            .collect();
        spans.sort();
        for w in spans.windows(2) {
            if w[0].0 == w[1].0 {
                assert!(w[0].2 <= w[1].1, "{} runs overlap at {:#x}", w[0].0, w[1].1);
            }
        }
        let words: usize = RUNS.iter().map(|r| r.now.len()).sum();
        assert_eq!(words, 111, "the modified build changed 111 words");
        // And every one of them is a change.
        for r in RUNS {
            for (a, b) in r.was.iter().zip(r.now) {
                assert_ne!(a, b, "{:?}+{:#x} rewrites a word to itself", r.sym, r.at);
            }
        }
    }

    /// The label strings the new `DrawTouchUi` loop walks, two bytes each,
    /// are the six keys that fire the belt — in the belt's own order.
    #[test]
    fn the_belt_labels_are_the_belt_keys() {
        let run = RUNS.iter().find(|r| r.sym == Some(DRAW_TOUCH_UI) && r.at == 0x1a8).unwrap();
        let bytes: Vec<u8> = run.now[25..28].iter().flat_map(|w| w.to_le_bytes()).collect();
        let labels: Vec<&[u8]> = bytes.chunks(2).collect();
        assert_eq!(labels, [b"1\0", b"3\0", b"7\0", b"9\0", b"*\0", b"0\0"]);
        // Each label names the keypad key that does what the slot under it does.
        for (i, l) in labels.iter().enumerate() {
            let key = (l[0] as char).to_string();
            let (_, action) = KEYPAD.iter().find(|(k, _)| *k == key).unwrap();
            let want = [KEY_SLOT[0], KEY_SLOT[1], KEY_COMBO, KEY_SLOT[2], KEY_SLOT[3], KEY_SLOT[4]][i];
            assert_eq!(*action, Some(PadAction::Key(want)), "slot {i} labelled {key}");
        }
    }

    /// The 240 layout's positions: one row at the bottom, 32 px apart, every
    /// button inside a 240x320 screen.
    #[test]
    fn the_belt_lands_in_one_row_on_the_portrait_canvas() {
        let run = RUNS.iter().find(|r| r.sym.is_none()).unwrap();
        let h: Vec<u16> = run
            .now
            .iter()
            .flat_map(|w| [(*w & 0xffff) as u16, (*w >> 16) as u16])
            .collect();
        let (xs, ys) = h.split_at(6);
        let (w, hgt) = CANVAS;
        for i in 0..6 {
            let x = w - xs[i] as i32;
            let y = hgt - ys[i] as i32;
            assert!((0..w - 30).contains(&x), "slot {i} at x {x}");
            assert_eq!(y, 274, "slot {i}: one row");
            if i > 0 {
                assert_eq!(x - (w - xs[i - 1] as i32), 32, "slot {i}: 32 px pitch");
            }
        }
    }

    #[test]
    fn the_drawn_belt_is_one_row_of_six_thirty_two_apart() {
        let b = belt_row(240, 320);
        assert_eq!(b[0], (8, 274));
        for i in 1..6 {
            assert_eq!(b[i].0 - b[i - 1].0, 32);
            assert_eq!(b[i].1, 274);
        }
        let hud = drawn_hud(240, 320);
        assert_eq!(hud.ok, (0, 0, 0, 0), "no OK disc in 세로");
        assert_eq!(hud.page, (200, 277));
    }

    #[test]
    fn keypad_codes_resolve_and_nothing_else_does() {
        assert_eq!(keypad_action(KEYPAD_CODE_BASE + 7), Some(Some(PadAction::Key(KEY_OK))));
        assert_eq!(keypad_action(KEYPAD_CODE_BASE + 12), Some(None), "the blank key");
        assert_eq!(keypad_action(KEYPAD_CODE_BASE + 18), Some(None), "the editor's key");
        assert_eq!(keypad_action(KEYPAD_CODE_BASE + 14), Some(Some(PadAction::Dash)));
        assert_eq!(keypad_action(KEYPAD_CODE_BASE + 24), None);
        assert_eq!(keypad_action(KEYPAD_CODE_BASE - 1), None);
        // No Android keycode is in range.
        for code in 0..1000 {
            assert!(!is_keypad(code), "{code}");
        }
    }

    /// The phone keys say what they are: every digit and `*`/`#` sends
    /// itself or walks, and the four walking digits agree with the arrows.
    #[test]
    fn the_number_block_is_the_phones() {
        let act = |label: &str| KEYPAD.iter().find(|(k, _)| *k == label).unwrap().1;
        for d in ["1", "3", "7", "9", "*", "0", "#"] {
            assert_eq!(act(d), Some(PadAction::Key(d.as_bytes()[0] as i32)), "{d}");
        }
        assert_eq!(act("2"), act("↑"));
        assert_eq!(act("8"), act("↓"));
        assert_eq!(act("4"), act("←"));
        assert_eq!(act("6"), act("→"));
        assert_eq!(act("5"), act("OK"));
        assert_eq!(act("CLR"), Some(PadAction::Key(KEY_CLEAR)));
        assert_eq!(act("저장"), Some(PadAction::Key(KEY_SAVE)));
    }
}
