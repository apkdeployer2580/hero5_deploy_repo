//! 해상도 — a tenth row on the game's own 환경설정 list.
//!
//! The user's report was that the picture is uncomfortably wide on a large
//! phone. The lever is the logical canvas: [`crate::widened_canvas`] hands the
//! guest a screen shaped like the display, and handing it a *narrower* one
//! makes the game occupy less of the window while every pixel keeps the same
//! whole-number magnification. Three landscape settings, `FULL`, `90%` and
//! `80%` of the full width, with what is left of the window black on both
//! sides — and a fourth, `세로`, which is not a width at all: the phone turns
//! upright, the guest is handed the 240x320 screen its own 240-wide layout
//! tables were authored for, and a feature-phone keypad fills the rest of the
//! window. See [`crate::portrait`].
//!
//! The value box used to read `A`/`B`/`C`; it reads the percentage now, which
//! is what the player is choosing. The switch file keeps the letters (plus `P`)
//! so an installation that already chose `B` still boots on `B`.
//!
//! # Why the canvas and not the presenter
//!
//! Scaling the finished picture down instead would need no restart, and was
//! rejected: the canvas is chosen so the window is an exact integer multiple
//! of it (ANALYSIS.md §33 — 2340/585 = 1080/270 = 4), and a 0.9 factor on the
//! *presenter* turns that into 3.6, which is the one thing a nearest-neighbour
//! upscale cannot place. Narrowing the canvas instead keeps the magnification
//! at exactly 4 and pays for it in field of view, which is the trade the
//! setting is *for*. See [`crate::widened_canvas_for`].
//!
//! The cost is that `nativeInitDisplay` runs once per process and the guest
//! sizes its framebuffer from what it hears there, so a change takes effect on
//! the next start.
//!
//! # The row is the guest's tenth, not a strip under the panel
//!
//! An earlier version drew a strip below the six visible rows. It worked and
//! it looked wrong — detached, in its own style, and floating under 오토타겟
//! whenever the list was scrolled up. What the list actually is:
//!
//! ```text
//!   index  0 배경음   1 효과음   2 게임속도  3 진동   4 화면효과
//!          5 오토타겟 6 투구보기 7 터치방식  8 데이터 관리
//!   six visible slots, cursor row `this+0x0e`, cursor index `this+0x0f`,
//!   so the window is `first = idx - row` .. `first + 5`.
//! ```
//!
//! Nine is not a variable — it is spelled out in `.rodata` (a nine-byte
//! per-row type table at 0x15A610 for `DrawOption` and another at 0x15A6EC for
//! `KeyOption`), in `KeyOption`'s wrap arithmetic, in the literals
//! `CommonUi::DrawScroll` is handed, and in the eight-halfword label-id array
//! `DrawOption` builds on its stack. And none of the nine is spare: their
//! values are `gv[0x84..0x8c]` and every one is live, down to the type-3 row,
//! whose real value is `gv[0x8f]` and which
//! `StateGame::{UpdateNaviAngle, DrawTouchUi, TouchPressed}` all read — it is
//! the 터치방식 selector.
//!
//! So index **9** is the host's. Five hooks make the guest carry it without
//! any of its tables growing:
//!
//! | where | what it does |
//! |---|---|
//! | `DrawOption+0x234` — the row loop head | index 9: draw the row with the guest's own primitives, then resume at the loop tail |
//! | `CommonUi::DrawScroll` (from `DrawOption`) | tell the scrollbar there are ten rows, not nine |
//! | `KeyOption` entry | the two boundaries (8→9 down, 0→9 up) and every key while the cursor is on 9 |
//! | `TouchOption+0x2ac` — the scroll-down clamp | let the window's first row reach 4 |
//! | `TouchOption+0x30c` — the scroll-up wrap | wrap to 9 rather than 8 |
//!
//! `TouchOption` needs nothing else: its row hit test computes
//! `idx = first + slot` (0x11BEB8) with no table lookup, and every activation
//! it performs is a synthesised `BaseState::KeyPress` through vptr+0x1c
//! (LEFT −3, RIGHT −4, SELECT −5, BACK −16), which lands back in `KeyOption`
//! where the hook above already answers for index 9.
//!
//! # The restart prompt is the guest's own 게임종료 dialog
//!
//! Because `nativeInitDisplay` runs once per process, moving the row is a
//! promise about the *next* start, and a setting that appears to do nothing
//! reads as a broken setting. So a change asks:
//!
//! ```text
//!   해상도를 90%로 바꿨습니다.
//!   다시 시작해야 적용됩니다.
//!   지금 종료하시겠습니까?
//!            [예]  [아니오]
//! ```
//!
//! None of which is drawn here. `StateMainMenu::SetPopupState(8, msg, 0, 0, 9)`
//! is the call the 게임종료 menu item makes (`KeyMainMenu+0x154`), and both
//! ends of it are already the wanted behaviour: id 8 takes the two-button arm
//! whose labels are `GetCommonText(0x32)` = 예 and `(0x33)` = 아니오, and
//! `StateMainMenu::Process`'s arm for id 8 (0x124F30) reads the answer back —
//! **0 clears the popup and calls `MC_knlExit(0)`, 1 clears the popup and
//! stops there.** The host writes the sentence in the middle and nothing else;
//! there is no result hook, because there is nothing left for one to do.
//!
//! The file is written *before* the question, so 아니오 keeps the new value and
//! the answer only decides when the canvas is rebuilt, never whether. And a
//! mode that walks back round to the one this process is running
//! ([`Screen::needs_restart`]) asks nothing: the picture on the screen is
//! already the one the row names.

use crate::Vm;

/// How wide a canvas the player asked for — or that they asked for the
/// upright one.
///
/// The percentages are the whole definition of the three landscape modes;
/// everything else follows from [`crate::widened_canvas_for`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum ScreenMode {
    /// Full width — the canvas [`crate::widened_canvas`] picks on its own, and
    /// the behaviour every measurement in ANALYSIS.md was taken with.
    #[default]
    Full,
    /// 90% of it.
    Narrow10,
    /// 80% of it.
    Narrow20,
    /// 세로: the phone upright, a 240x320 canvas, a keypad underneath. See
    /// [`crate::portrait`].
    Portrait,
}

impl ScreenMode {
    /// All four, in the order the row cycles them.
    pub const ALL: [ScreenMode; 4] =
        [ScreenMode::Full, ScreenMode::Narrow10, ScreenMode::Narrow20, ScreenMode::Portrait];

    /// The three that are a width of the landscape canvas.
    pub const LANDSCAPE: [ScreenMode; 3] =
        [ScreenMode::Full, ScreenMode::Narrow10, ScreenMode::Narrow20];

    /// Percent of the full-width landscape canvas this mode keeps; `None` for
    /// [`ScreenMode::Portrait`], whose canvas is not derived from the display.
    pub fn width_pct(self) -> Option<i32> {
        match self {
            ScreenMode::Full => Some(100),
            ScreenMode::Narrow10 => Some(90),
            ScreenMode::Narrow20 => Some(80),
            ScreenMode::Portrait => None,
        }
    }

    /// Whether this is the upright arrangement.
    pub fn is_portrait(self) -> bool {
        self == ScreenMode::Portrait
    }

    /// The token in `<files>/hl5_resolution.txt`. Letters, because that is
    /// what the file held before the row showed percentages, and an
    /// installation that already chose `B` must not boot on something else.
    ///
    /// **Java reads this file too** — `MainActivity` has to know about `P`
    /// before the window exists, to stand the activity upright — so the
    /// portrait token is spelled in two places. [`ScreenMode::parse`] accepts
    /// `portrait` as well, and so does the Java side.
    pub fn letter(self) -> char {
        match self {
            ScreenMode::Full => 'A',
            ScreenMode::Narrow10 => 'B',
            ScreenMode::Narrow20 => 'C',
            ScreenMode::Portrait => 'P',
        }
    }

    /// What the player reads: the row's value, and the boot log.
    pub fn label(self) -> &'static str {
        match self {
            ScreenMode::Full => "FULL",
            ScreenMode::Narrow10 => "90%",
            ScreenMode::Narrow20 => "80%",
            ScreenMode::Portrait => "세로",
        }
    }

    /// [`ScreenMode::label`] as the NUL-terminated EUC-KR the value box draws.
    /// `세로` is two hangul, the widest of the four at 22 px, inside the 31 px
    /// box the guest draws for its own values.
    pub(crate) fn value_bytes(self) -> &'static [u8] {
        match self {
            ScreenMode::Full => b"FULL\0",
            ScreenMode::Narrow10 => b"90%\0",
            ScreenMode::Narrow20 => b"80%\0",
            ScreenMode::Portrait => &[0xbc, 0xbc, 0xb7, 0xce, 0x00],
        }
    }

    /// The body of the restart prompt this mode raises. See
    /// [`RESTART_MSG_FULL`].
    fn restart_message(self) -> &'static [u8] {
        match self {
            ScreenMode::Full => RESTART_MSG_FULL,
            ScreenMode::Narrow10 => RESTART_MSG_NARROW10,
            ScreenMode::Narrow20 => RESTART_MSG_NARROW20,
            ScreenMode::Portrait => RESTART_MSG_PORTRAIT,
        }
    }

    /// Parse the switch file. Anything else is `None`, which the reader turns
    /// into [`ScreenMode::Full`] with a warning rather than a silent guess.
    pub fn parse(s: &str) -> Option<ScreenMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "a" | "full" | "100" | "100%" => Some(ScreenMode::Full),
            "b" | "90" | "90%" => Some(ScreenMode::Narrow10),
            "c" | "80" | "80%" => Some(ScreenMode::Narrow20),
            "p" | "portrait" => Some(ScreenMode::Portrait),
            _ => None,
        }
    }

    /// The next one round the cycle — what RIGHT and SELECT do on the row;
    /// `back` is what LEFT does.
    pub(crate) fn cycle(self, back: bool) -> ScreenMode {
        let i = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        let n = Self::ALL.len();
        Self::ALL[if back { (i + n - 1) % n } else { (i + 1) % n }]
    }
}

/// Where a canvas of `canvas_w` lands across a `surface_w`-wide window, in
/// window pixels: `(left edge, width)`.
///
/// **One owner for the letterbox.** The presenter scales into this span, the
/// EGL viewport is set from the same ratio, and the touch layer inverts it; a
/// second copy of the arithmetic anywhere would eventually put the picture and
/// the finger in different places, and that is a bug nothing offline can catch
/// — the probe has no window.
///
/// `full_canvas_w` is the width [`crate::widened_canvas`] would have chosen on
/// its own. When the two agree — [`ScreenMode::Full`], which is what ships —
/// this returns `(0, surface_w)`, the expression the shell has always used, so
/// mode `A` is not merely equivalent to the old behaviour but identical to it.
///
/// The ratio is `canvas_w / full_canvas_w` rather than the mode's percentage
/// because the canvas has already been floored to a whole number of pixels:
/// scaling the window by the percentage instead would leave the picture up to
/// a device pixel away from where the canvas says it is.
pub fn letterbox(surface_w: i32, canvas_w: i32, full_canvas_w: i32) -> (i32, i32) {
    let sw = surface_w.max(1);
    if full_canvas_w <= 0 || canvas_w <= 0 || canvas_w >= full_canvas_w {
        return (0, sw);
    }
    let w = ((sw as i64 * canvas_w as i64) / full_canvas_w as i64).clamp(1, sw as i64) as i32;
    ((sw - w) / 2, w)
}

/// `<files>/hl5_resolution.txt` — where the row's choice is kept.
///
/// Written by the row itself and read by the shell before the guest exists,
/// because the canvas has to be decided before `nativeInitDisplay`. A plain
/// letter, so a human looking at the file can see what the game thinks it is
/// set to.
pub const RESOLUTION_FILE: &str = "hl5_resolution.txt";

/// `StateMainMenu::SetPopupState(signed char id, char* msg, int, int, int)` —
/// 0x11B42C. Every popup this screen raises goes through it, and the id is
/// what decides the shape.
pub(crate) const SET_POPUP_STATE: &str = "_ZN13StateMainMenu13SetPopupStateEaPciii";
/// The id the 게임종료 menu item uses (0x123430), chosen for what its two ends
/// already do rather than for its number — see the module header.
pub(crate) const POPUP_RESTART: u32 = 8;
/// The three trailing ints, as the 게임종료 site passes them. Only the id-**4**
/// arm reads them — it turns the first into a `GetCommonText` index for a
/// third button — and the two-button arm overwrites all three before it calls
/// `CallPopupState`, so these are here to make this call *that* call rather
/// than because the values matter.
///
/// (Id 4, not id 5: the table's case 0 is at `pc+8`, one past the fallthrough
/// branch, so slot *n* is id *n+1*. Id 5 never reaches the table at all — the
/// function's head intercepts it with a `beq`.)
const POPUP_ARGS: [u32; 3] = [0, 0, 9];

/// `해상도`, EUC-KR — the only encoding this guest's font stack reads
/// (CLAUDE.md: a guest string must never go through `String`).
pub(crate) const LABEL: &[u8] = &[0xc7, 0xd8, 0xbb, 0xf3, 0xb5, 0xb5, 0x00];

/// The restart prompt's body, one per mode — EUC-KR, and `;` is this guest's
/// line break inside a popup.
///
/// ```text
/// 해상도를 90%로 바꿨습니다.;다시 시작해야 적용됩니다.;지금 종료하시겠습니까?
/// ```
///
/// Three runs rather than one sentence because the guest wraps by width and a
/// run one word too long breaks mid-syllable — `localserver`'s offline notice
/// learned that on the phone. Each of these is inside the eleven-hangul-plus-
/// spaces the game's own three-line popups (`common_text` 0x130, 0x159) use,
/// and the last is the game's own 종료하시겠습니까? with 지금 in front of it.
/// `FULL로` and `세로로` make the first run exactly the 27-byte ceiling.
///
/// The value is spelled out because the popup covers the row that would
/// otherwise be showing it. One constant per mode rather than a format string:
/// [`text`] interns by the address of the slice, so a built string would leak
/// a guest heap block per press.
const RESTART_MSG_FULL: &[u8] = &[
    0xc7, 0xd8, 0xbb, 0xf3, 0xb5, 0xb5, 0xb8, 0xa6, 0x20, 0x46, 0x55, 0x4c, 0x4c, 0xb7, 0xce,
    0x20, 0xb9, 0xd9, 0xb2, 0xe5, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xb4, 0xd9,
    0xbd, 0xc3, 0x20, 0xbd, 0xc3, 0xc0, 0xdb, 0xc7, 0xd8, 0xbe, 0xdf, 0x20, 0xc0, 0xfb, 0xbf,
    0xeb, 0xb5, 0xcb, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xc1, 0xf6, 0xb1, 0xdd, 0x20, 0xc1,
    0xbe, 0xb7, 0xe1, 0xc7, 0xcf, 0xbd, 0xc3, 0xb0, 0xda, 0xbd, 0xc0, 0xb4, 0xcf, 0xb1, 0xee,
    0x3f, 0x00,
];
/// See [`RESTART_MSG_FULL`].
const RESTART_MSG_NARROW10: &[u8] = &[
    0xc7, 0xd8, 0xbb, 0xf3, 0xb5, 0xb5, 0xb8, 0xa6, 0x20, 0x39, 0x30, 0x25, 0xb7, 0xce, 0x20,
    0xb9, 0xd9, 0xb2, 0xe5, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xb4, 0xd9, 0xbd,
    0xc3, 0x20, 0xbd, 0xc3, 0xc0, 0xdb, 0xc7, 0xd8, 0xbe, 0xdf, 0x20, 0xc0, 0xfb, 0xbf, 0xeb,
    0xb5, 0xcb, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xc1, 0xf6, 0xb1, 0xdd, 0x20, 0xc1, 0xbe,
    0xb7, 0xe1, 0xc7, 0xcf, 0xbd, 0xc3, 0xb0, 0xda, 0xbd, 0xc0, 0xb4, 0xcf, 0xb1, 0xee, 0x3f,
    0x00,
];
/// See [`RESTART_MSG_FULL`].
const RESTART_MSG_NARROW20: &[u8] = &[
    0xc7, 0xd8, 0xbb, 0xf3, 0xb5, 0xb5, 0xb8, 0xa6, 0x20, 0x38, 0x30, 0x25, 0xb7, 0xce, 0x20,
    0xb9, 0xd9, 0xb2, 0xe5, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xb4, 0xd9, 0xbd,
    0xc3, 0x20, 0xbd, 0xc3, 0xc0, 0xdb, 0xc7, 0xd8, 0xbe, 0xdf, 0x20, 0xc0, 0xfb, 0xbf, 0xeb,
    0xb5, 0xcb, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xc1, 0xf6, 0xb1, 0xdd, 0x20, 0xc1, 0xbe,
    0xb7, 0xe1, 0xc7, 0xcf, 0xbd, 0xc3, 0xb0, 0xda, 0xbd, 0xc0, 0xb4, 0xcf, 0xb1, 0xee, 0x3f,
    0x00,
];
/// See [`RESTART_MSG_FULL`]. `해상도를 세로로 바꿨습니다.`
const RESTART_MSG_PORTRAIT: &[u8] = &[
    0xc7, 0xd8, 0xbb, 0xf3, 0xb5, 0xb5, 0xb8, 0xa6, 0x20, 0xbc, 0xbc, 0xb7, 0xce, 0xb7, 0xce,
    0x20, 0xb9, 0xd9, 0xb2, 0xe5, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xb4, 0xd9,
    0xbd, 0xc3, 0x20, 0xbd, 0xc3, 0xc0, 0xdb, 0xc7, 0xd8, 0xbe, 0xdf, 0x20, 0xc0, 0xfb, 0xbf,
    0xeb, 0xb5, 0xcb, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xc1, 0xf6, 0xb1, 0xdd, 0x20, 0xc1,
    0xbe, 0xb7, 0xe1, 0xc7, 0xcf, 0xbd, 0xc3, 0xb0, 0xda, 0xbd, 0xc0, 0xb4, 0xcf, 0xb1, 0xee,
    0x3f, 0x00,
];

/// Per-`Vm` state for the row.
pub struct Screen {
    /// What the player has chosen. Starts as the mode the canvas was actually
    /// built with, so a fresh run shows the truth.
    pub chosen: ScreenMode,
    /// The mode this process's canvas was built from. `chosen != applied`
    /// means the picture on screen is not what the row says yet.
    pub applied: ScreenMode,
    /// Rows drawn, changes taken and restart prompts raised, for the boot log
    /// and the probe: a hook that never fires — or a question that is never
    /// asked — is a feature that silently is not there.
    pub drawn: u64,
    pub taps: u64,
    pub asked: u64,
}

impl Screen {
    pub fn new(mode: ScreenMode) -> Self {
        Self {
            chosen: mode,
            applied: mode,
            drawn: 0,
            taps: 0,
            asked: 0,
        }
    }

    /// Whether the picture on the screen is still the one the row names.
    ///
    /// False for the whole life of a process started on the mode the row
    /// shows, and that is exactly when there is nothing for a restart to
    /// apply — so this is both what the prompt is gated on and what the
    /// probe's summary reports.
    pub fn needs_restart(&self) -> bool {
        self.chosen != self.applied
    }

    /// Move to `mode`, remember it, and ask about the restart it now needs.
    ///
    /// Writing the file here rather than at shutdown is deliberate: the
    /// setting then survives a crash, a force-stop, and the player simply
    /// never coming back to this screen. It is also what makes 아니오 safe —
    /// the choice is on disk before the question is asked, so the answer
    /// decides only *when* the canvas is rebuilt, never *whether*.
    pub(crate) fn choose(vm: &mut Vm, this: u32, mode: ScreenMode) -> Result<(), String> {
        vm.screen.taps += 1;
        if mode == vm.screen.chosen {
            return Ok(());
        }
        vm.screen.chosen = mode;
        let path = vm.cfg.storage.join(RESOLUTION_FILE);
        match std::fs::write(&path, format!("{}\n", mode.letter())) {
            Ok(()) => vm.log.push(format!(
                "resolution: {} ({}) written to {}",
                mode.letter(),
                mode.label(),
                path.display()
            )),
            // A choice that cannot be saved is still applied to the row, so
            // the player sees the press register; the log says why it will not
            // survive.
            Err(e) => vm.log.push(format!("resolution: cannot write {}: {e}", path.display())),
        }
        ask_restart(vm, this)
    }
}

/// Ask — in the guest's own two-button popup — whether to close the game now,
/// so the next start builds the canvas the row is asking for.
///
/// **Nothing here reads the answer, because the guest already does.** See
/// [`POPUP_RESTART`]: 예 is `MC_knlExit(0)`, which reaches the shell as
/// `CletEntry.exitApp` and closes the window, and 아니오 clears the popup and
/// returns to this list with the new value still on the row.
fn ask_restart(vm: &mut Vm, this: u32) -> Result<(), String> {
    // Back on the mode this process's canvas was built from — A -> B -> C -> A
    // asks twice and then stops. There is nothing for a restart to apply, and
    // saying otherwise would be a claim the player could act on.
    if !vm.screen.needs_restart() {
        return Ok(());
    }
    let msg = match crate::options::text(vm, vm.screen.chosen.restart_message()) {
        Some(p) => p,
        // Out of guest heap. The setting is saved and the row shows it; a
        // popup with no body would be worse than no popup.
        None => return Ok(()),
    };
    let f = vm.symbol(SET_POPUP_STATE).ok_or("resolution: SetPopupState vanished")?;
    vm.call(f, &[this, POPUP_RESTART, msg, POPUP_ARGS[0], POPUP_ARGS[1], POPUP_ARGS[2]])?;
    vm.screen.asked += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_landscape_modes_are_the_advertised_widths() {
        assert_eq!(ScreenMode::Full.width_pct(), Some(100));
        assert_eq!(ScreenMode::Narrow10.width_pct(), Some(90));
        assert_eq!(ScreenMode::Narrow20.width_pct(), Some(80));
        assert_eq!(ScreenMode::Portrait.width_pct(), None, "세로 is not a width");
        for m in ScreenMode::LANDSCAPE {
            assert!(!m.is_portrait());
        }
        assert!(ScreenMode::Portrait.is_portrait());
    }

    /// What the value box shows is the percentage the player is choosing —
    /// and it is the same string the boot log prints.
    #[test]
    fn the_value_box_reads_the_label() {
        for m in ScreenMode::ALL {
            let shown = m.value_bytes();
            assert_eq!(shown.last(), Some(&0), "{m:?}: a C string");
            let body = &shown[..shown.len() - 1];
            if m.is_portrait() {
                assert_eq!(body, &[0xbc, 0xbc, 0xb7, 0xce], "세로 in EUC-KR");
            } else {
                assert_eq!(body, m.label().as_bytes(), "{m:?}");
            }
        }
    }

    #[test]
    fn the_switch_file_round_trips() {
        for m in ScreenMode::ALL {
            let s = m.letter().to_string();
            assert_eq!(ScreenMode::parse(&s), Some(m), "{s}");
            assert_eq!(ScreenMode::parse(&s.to_lowercase()), Some(m));
        }
        // The letters an installation wrote before the row showed percentages.
        assert_eq!(ScreenMode::parse("A\n"), Some(ScreenMode::Full));
        assert_eq!(ScreenMode::parse("B"), Some(ScreenMode::Narrow10));
        assert_eq!(ScreenMode::parse("c"), Some(ScreenMode::Narrow20));
        assert_eq!(ScreenMode::parse("portrait"), Some(ScreenMode::Portrait));
        assert_eq!(ScreenMode::parse("90%"), Some(ScreenMode::Narrow10));
        assert_eq!(ScreenMode::parse("nonsense"), None);
        assert_eq!(ScreenMode::parse(""), None);
    }

    /// Left and right have to walk the same three values in opposite
    /// directions and come home — a cycle that skipped or stuck would strand
    /// the player on a setting.
    #[test]
    fn the_value_cycles_both_ways() {
        for m in ScreenMode::ALL {
            assert_eq!(m.cycle(false).cycle(true), m, "{m:?} forward then back");
            assert_eq!(m.cycle(true).cycle(false), m, "{m:?} back then forward");
            let mut seen = std::collections::BTreeSet::new();
            let mut c = m;
            for _ in 0..ScreenMode::ALL.len() {
                seen.insert(c.letter());
                c = c.cycle(false);
            }
            assert_eq!(seen.len(), ScreenMode::ALL.len(), "{m:?} visits every mode");
            assert_eq!(c, m, "{m:?} returns after a full cycle");
        }
    }

    /// Every `;`-separated run of a popup body has to fit the popup's width on
    /// its own: the guest wraps by width and a run one word too long breaks
    /// mid-syllable (`localserver`'s offline notice learned that on the
    /// phone). 27 bytes is the game's own widest three-line popup run,
    /// `common_text` 0x130's `^아이템 구매 중 일부 시간이`.
    const POPUP_RUN_MAX: usize = 27;

    /// Walk EUC-KR. A lead byte (>= 0x80) must be followed by a trail byte, so
    /// a message that was cut or edited half a syllable short fails here
    /// rather than on the phone.
    fn euckr_runs(msg: &[u8]) -> Vec<&[u8]> {
        assert_eq!(msg.last(), Some(&0), "the guest reads this as a C string");
        let body = &msg[..msg.len() - 1];
        assert!(!body.contains(&0), "an interior NUL truncates the popup");
        let mut i = 0;
        while i < body.len() {
            if body[i] >= 0x80 {
                let t = *body.get(i + 1).expect("a lead byte with no trail byte");
                assert!((0x41..=0xfe).contains(&t), "trail byte {t:#04x} at {i}");
                i += 2;
            } else {
                i += 1;
            }
        }
        body.split(|b| *b == b';').collect()
    }

    /// The prompt has to name the mode it is about — the popup covers the row
    /// that would otherwise be showing it — and the three bodies have to
    /// differ, which is the copy-paste this catches.
    #[test]
    fn every_mode_names_itself_in_its_restart_message() {
        let says = |msg: &[u8], m: ScreenMode| {
            let v = m.value_bytes();
            let label = &v[..v.len() - 1];
            msg.windows(label.len()).any(|w| w == label)
        };
        let mut seen = std::collections::BTreeSet::new();
        for m in ScreenMode::ALL {
            let msg = m.restart_message();
            assert!(says(msg, m), "{m:?}: the body never says {}", m.label());
            for other in ScreenMode::ALL {
                if other != m {
                    assert!(!says(msg, other), "{m:?}: the body also says {}", other.label());
                }
            }
            assert!(seen.insert(msg.to_vec()), "{m:?} reuses another mode's body");
        }
    }

    /// Three runs, each inside the width the game's own popups prove fits, and
    /// none of them starting with the colour escape. `out/shot/res-prompt-b.png`
    /// is what this is guarding.
    #[test]
    fn the_restart_message_is_three_runs_the_popup_can_hold() {
        for m in ScreenMode::ALL {
            let runs = euckr_runs(m.restart_message());
            assert_eq!(runs.len(), 3, "{m:?}: `;` is the guest's line break");
            for (i, run) in runs.iter().enumerate() {
                assert!(!run.is_empty(), "{m:?}: run {i} is blank");
                assert!(
                    run.len() <= POPUP_RUN_MAX,
                    "{m:?}: run {i} is {} bytes, over {POPUP_RUN_MAX}",
                    run.len()
                );
                // `^` opens a colour escape (CLAUDE.md); one at the head of a
                // run would eat the first characters of the line.
                assert_ne!(run[0], b'^', "{m:?}: run {i} starts with a colour escape");
            }
        }
    }

    /// The prompt is gated on this, and the one case that must *not* ask is
    /// the player walking the value back round to the mode the process is
    /// already running — the picture is then already what the row says.
    #[test]
    fn only_a_mode_other_than_the_running_one_needs_a_restart() {
        for applied in ScreenMode::ALL {
            let mut screen = Screen::new(applied);
            assert!(!screen.needs_restart(), "{applied:?}: a fresh run has nothing to apply");
            for chosen in ScreenMode::ALL {
                screen.chosen = chosen;
                assert_eq!(
                    screen.needs_restart(),
                    chosen != applied,
                    "applied {applied:?}, chosen {chosen:?}"
                );
            }
        }
    }

    /// The letterbox: centred, never wider than the window, and *exactly* the
    /// old expression at full width.
    #[test]
    fn the_letterbox_centres_without_moving_the_full_width_case() {
        for sw in [2340, 2220, 1920, 1280, 960, 800] {
            assert_eq!(letterbox(sw, 585, 585), (0, sw), "equal widths must not letterbox");
            assert_eq!(letterbox(sw, 585, 0), (0, sw), "an unset full width must not letterbox");
        }
        // The two the shipping phone can ask for. Both widths are the canvas
        // times exactly 4 — the magnification the height already fixed, which
        // is the whole reason the width is what moves.
        assert_eq!(letterbox(2340, 526, 585), (118, 2104));
        assert_eq!(letterbox(2340, 468, 585), (234, 1872));
        assert_eq!(2104, 526 * 4);
        assert_eq!(1872, 468 * 4);
        for (canvas, full) in [(526, 585), (468, 585)] {
            let (x0, w) = letterbox(2340, canvas, full);
            assert_eq!(x0 * 2 + w, 2340, "the two bands must be equal");
            assert!(w < 2340 && w > 0);
        }
    }

    /// A tap has to come back to the canvas pixel it was drawn from. This is
    /// the round trip the device would otherwise be the first to test.
    #[test]
    fn a_tap_lands_on_the_canvas_pixel_it_was_drawn_from() {
        for (canvas, full) in [(585, 585), (526, 585), (468, 585)] {
            let (x0, w) = letterbox(2340, canvas, full);
            for cx in [0, 1, canvas / 3, canvas / 2, canvas - 2, canvas - 1] {
                let win = x0 + (cx * w) / canvas + (w / canvas) / 2;
                let back = ((win - x0) as f64 * canvas as f64 / w as f64) as i32;
                assert_eq!(
                    back.clamp(0, canvas - 1),
                    cx,
                    "canvas {canvas}/{full}: column {cx} -> window {win} -> {back}"
                );
            }
        }
    }

    /// No window column, band or picture, may index off the end of a canvas
    /// row.
    #[test]
    fn no_window_column_indexes_off_the_canvas() {
        for (canvas, full) in [(585, 585), (526, 585), (468, 585)] {
            let (x0, w) = letterbox(2340, canvas, full);
            for win in [0, x0, x0 + w / 2, x0 + w - 1, x0 + w, 2339] {
                let back = ((win - x0) as f64 * canvas as f64 / w as f64) as i32;
                assert!((0..canvas).contains(&back.clamp(0, canvas - 1)));
            }
            let past = ((2339 - x0) as f64 * canvas as f64 / w as f64) as i32;
            assert!(past >= canvas - 1, "canvas {canvas}: the right edge must reach the end");
        }
    }

}
