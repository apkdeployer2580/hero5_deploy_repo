//! Centring the in-game top bar on a widened canvas.
//!
//! Almost everything this guest draws is laid out from `Graphic::GetWidth()`,
//! which is why [`crate::widened_canvas`] works at all (ANALYSIS.md §27.8).
//! The in-game `map / shop / save` bar is the one exception: it is drawn from
//! a literal base, so it stays where it was while the canvas grows around it
//! and ends up left of centre.
//!
//! `StateGame::DrawTouchUi` (0xE05E0) draws it as one loop, three cells:
//!
//! ```text
//! r6 = 0x93                                   ; 147, the literal base
//! loop:
//!   CommonUi::DrawInfoRect(fb, r6,      0, 0x1e, 0x0d)   ; the plate
//!   BFont::DrawText       (fb, r6+0x0f, 0, "map"/"shop"/"save", …)
//!   r6 += 0x32                                ; 50 per cell
//! ```
//!
//! So two call sites, both taking x as argument 2, and shifting both by the
//! same `dx` moves the whole bar.
//!
//! # Why the taps need help, and why this way
//!
//! Moving the pixels is half the job: the guest's own hit test still answers
//! for the *old* rectangle, and a button you can see but not press is worse
//! than one that is off-centre. That hit test turned out not to be reachable
//! by any of the obvious routes — the bar's taps become WIPI soft keys
//! (`Battle::KeyPressed` sees -7/-10), the coordinates are not immediates
//! anywhere in `.text`, and `StateGame::KeyPressed` has no direct callers.
//!
//! Rather than keep hunting it, this takes the fact the draw hook already
//! knows: **where the bar actually is on screen right now.** Each cell it
//! shifts is recorded, and [`Hud::unshift`] maps a touch inside that recorded
//! strip back by the same `dx` before the guest sees it. The guest then does
//! its own per-cell test against the coordinates it believes in, unchanged.
//!
//! That is self-limiting in the way a coordinate range would not be: the
//! strip only exists while the bar is actually on screen, so no tap is stolen
//! on a screen that has something else up there.
//!
//! # Which clock the record is kept on, and why it is not the frame
//!
//! It used to be cleared at the top of every [`crate::Vm::frame`], and that
//! was the wrong clock. Most `nativeLoop` calls run the guest's timer list,
//! find nothing due and draw nothing: the guest paces itself at ~28 ticks a
//! second while the shell polls it far more often. So a record cleared every
//! frame was absent on the majority of them, a touch arrives from the window
//! whenever the finger lands, and the odds of it finding a record were
//! `tick rate / poll rate` — about one in three (ANALYSIS.md §31.9). Two taps
//! out of three on `map / shop / save` did nothing.
//!
//! The question the translation actually needs answered is *"is the bar in the
//! picture that is on the screen"*, and that changes only when the guest
//! repaints. So the record now survives every frame that drew nothing, and is
//! dropped by the first frame that **did** repaint without drawing the bar —
//! leaving the gameplay screen, exactly the case the old rule protected.
//! [`crate::Vm::frame`] supplies that signal from the guest's own
//! `MC_grpFlushLcd` count.

use arm32_cpu::{reg, Mode};

use crate::mem::RETURN_MAGIC;
use crate::Vm;

/// `CommonUi::DrawInfoRect(int fb, int x, int y, int w, int h)` — 0x78434.
const DRAW_INFO_RECT: &str = "_ZN8CommonUi12DrawInfoRectEiiiii";
/// `BFont::DrawText(int fb, int x, int y, char* s, int align, int colour)` —
/// 0x445E8. Shared with the free-shop price hook, which is why both go
/// through one dispatcher (see [`crate::Vm::install_behaviour_hooks`]).
const DRAW_TEXT_PLAIN: &str = "_ZN5BFont8DrawTextEiiiPcii";
/// The function owning both call sites.
const SITE: &str = "_ZN9StateGame11DrawTouchUiEv";
/// Return addresses inside it: 0xE0D30 and 0xE0D84, relative to 0xE05E0.
const RECT_RET: u32 = 0xe0d30 - 0xe05e0;
const TEXT_RET: u32 = 0xe0d84 - 0xe05e0;

/// Cell height from the `DrawInfoRect` call (0x0d), plus a little slack: the
/// text sits at y 0 and its glyphs run a couple of rows past the plate.
const BAR_H: i32 = 0x0d + 8;

/// Per-`Vm` state for the bar.
pub struct Hud {
    /// How far right to move the bar so it keeps the place it had relative to
    /// the canvas centre. Zero at the authored width, which is what keeps the
    /// 400x240 A/B byte-identical.
    pub dx: i32,
    /// Horizontal span of the bar as it appears in the picture currently on
    /// screen, in screen-side (already shifted) guest coordinates.
    span: Option<(i32, i32)>,
    /// Whether the bar has been drawn during the frame now running. Resets at
    /// every [`Hud::frame_start`]; it is what lets a frame's first cell
    /// replace the previous span rather than widen it.
    seen: bool,
    /// Cells shifted, and taps translated — both reported by the probe so a
    /// silent no-op is visible.
    pub shifted: u64,
    pub taps: u64,
}

impl Hud {
    pub fn new(wipi_w: i32) -> Self {
        Self { dx: (wipi_w - crate::WIPI_W) / 2, span: None, seen: false, shifted: 0, taps: 0 }
    }

    /// Begin a frame. `painted` says whether the frame that just finished put
    /// a new picture on the screen.
    ///
    /// A frame that painted and did not draw the bar is the one and only
    /// reason to forget where it was: the player has left the gameplay screen.
    /// A frame that painted nothing changes nothing about what is on the
    /// screen, so the record stands — see the module header for why that
    /// distinction is worth making.
    pub fn frame_start(&mut self, painted: bool) {
        if painted && !self.seen {
            self.span = None;
        }
        self.seen = false;
    }

    fn saw(&mut self, x: i32, w: i32) {
        // The frame's first cell replaces; the rest of its cells widen. Doing
        // it here rather than in `frame_start` is what lets the span survive a
        // frame that drew nothing at all.
        if !self.seen {
            self.seen = true;
            self.span = None;
        }
        let (lo, hi) = self.span.unwrap_or((x, x + w));
        self.span = Some((lo.min(x), hi.max(x + w)));
    }

    /// Map a touch back onto the coordinates the guest lays the bar out in.
    ///
    /// Only points inside the strip the bar occupies **in the picture now on
    /// screen** move; everything else is handed over untouched.
    pub fn unshift(&mut self, x: i32, y: i32) -> (i32, i32) {
        if self.dx == 0 {
            return (x, y);
        }
        let Some((lo, hi)) = self.span else { return (x, y) };
        if y >= 0 && y < BAR_H && x >= lo && x < hi {
            self.taps += 1;
            return (x - self.dx, y);
        }
        (x, y)
    }
}

/// Install the bar hooks, or report that there is nothing to do.
///
/// Returns the symbols hooked. An unwidened canvas installs nothing at all —
/// no hook, no per-instruction bitmap entry, no behaviour to reason about.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    if vm.hud.dx == 0 {
        return Ok(Vec::new());
    }
    if vm.symbol(SITE).is_none() {
        return Err(format!("hud bar hook needs `{SITE}`, not in this binary"));
    }
    // Only the plate. The label goes through `DrawText(iiiPcii)`, which the
    // free-shop price line also wants, so `Vm::boot` owns that address and
    // hands it to `bar_text_hook` through a dispatcher.
    let addr = vm
        .symbol(DRAW_INFO_RECT)
        .ok_or_else(|| format!("hud bar hook target `{DRAW_INFO_RECT}` is not in this binary"))?;
    vm.install_hook(DRAW_INFO_RECT, addr, bar_rect_hook)?;
    Ok(vec![DRAW_INFO_RECT, DRAW_TEXT_PLAIN])
}

/// Whether this entry is the given call site inside `DrawTouchUi`.
fn fired_at(vm: &Vm, ret: u32) -> bool {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if lr == RETURN_MAGIC {
        return false;
    }
    vm.symbol(SITE).is_some_and(|site| lr == site + ret)
}

/// The cell plate. `(this, fb, x, y, w, h)`.
pub(crate) fn bar_rect_hook(vm: &mut Vm) -> Result<bool, String> {
    if !fired_at(vm, RECT_RET) {
        return Ok(false);
    }
    let mut a: Vec<u32> = (0..6).map(|i| vm.arg(i)).collect();
    let (x, w) = (a[2] as i32, a[4] as i32);
    vm.hud.saw(x + vm.hud.dx, w);
    vm.hud.shifted += 1;
    a[2] = (x + vm.hud.dx) as u32;
    let f = vm.symbol(DRAW_INFO_RECT).ok_or("hud: DrawInfoRect vanished")?;
    let ret = vm.call(f, &a)?;
    vm.cpu.reg_set(Mode::User, 0, ret);
    Ok(true)
}

/// The cell label. `(this, fb, x, y, str, align, colour)`.
pub(crate) fn bar_text_hook(vm: &mut Vm) -> Result<bool, String> {
    if !fired_at(vm, TEXT_RET) {
        return Ok(false);
    }
    let mut a: Vec<u32> = (0..7).map(|i| vm.arg(i)).collect();
    a[2] = (a[2] as i32 + vm.hud.dx) as u32;
    let f = vm.symbol(DRAW_TEXT_PLAIN).ok_or("hud: DrawText vanished")?;
    let ret = vm.call(f, &a)?;
    vm.cpu.reg_set(Mode::User, 0, ret);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unwidened_canvas_shifts_nothing() {
        let mut h = Hud::new(crate::WIPI_W);
        assert_eq!(h.dx, 0);
        h.saw(147, 30);
        h.frame_start(false);
        assert_eq!(h.unshift(161, 11), (161, 11), "the authored width must be untouched");
    }

    #[test]
    fn a_widened_canvas_moves_the_bar_by_half_the_gain() {
        let h = Hud::new(520);
        assert_eq!(h.dx, 60, "520-400 = 120, half of it keeps the bar's place about the centre");
        // The width that actually ships, once `widened_canvas` started picking
        // a height that makes the magnification whole (S25: 585x270).
        assert_eq!(Hud::new(585).dx, 92);
        assert_eq!(Hud::new(555).dx, 77, "S9");
        assert_eq!(Hud::new(crate::WIPI_W).dx, 0, "the authored width must not shift the bar");

        // `dx` is unclamped, so a canvas narrower than the authored 400 would
        // shift the bar *left* and the hook would steal taps from the wrong
        // strip. Nothing can ask for one -- `widened_canvas` floors at
        // `WIPI_W` -- and this is the assertion that keeps that true.
        for sh in (240..=2160).step_by(60) {
            for sw in [sh, sh * 4 / 3, sh * 16 / 9, sh * 195 / 90, sh * 21 / 9] {
                let (cw, _) = crate::widened_canvas(sw, sh);
                assert!(Hud::new(cw).dx >= 0, "{sw}x{sh} -> canvas {cw}");
            }
        }
    }

    #[test]
    fn only_taps_inside_the_drawn_strip_move() {
        let mut h = Hud::new(520);
        // Three cells at 147/197/247 + dx, 30 wide.
        for k in 0..3 {
            h.saw(147 + k * 50 + h.dx, 30);
        }
        // The shifted shop cell: 197+60 = 257..287, so its centre is 272.
        assert_eq!(h.unshift(272, 11), (212, 11), "maps back onto the guest's own 212");
        // Same column, but below the bar — the gameplay area.
        assert_eq!(h.unshift(272, 90), (272, 90));
        // Same row, but left of the bar — where the HP plate lives.
        assert_eq!(h.unshift(40, 11), (40, 11));
        assert_eq!(h.taps, 1);
    }

    #[test]
    fn a_painted_frame_without_the_bar_steals_no_taps() {
        let mut h = Hud::new(520);
        h.saw(147 + h.dx, 30);
        h.frame_start(true);
        assert_eq!(
            h.unshift(272, 11),
            (272, 11),
            "the guest repainted and the bar was not in it -- the player left that screen"
        );
    }

    #[test]
    fn frames_that_painted_nothing_keep_the_bar_pressable() {
        // The shell polls the guest several times per guest tick, and all but
        // one of those polls draw nothing. A touch that arrives on one of them
        // must still land on the bar the player can see.
        let mut h = Hud::new(520);
        // The map cell: the guest lays it out at 147..177, the hook draws it
        // shifted to 207..237.
        h.saw(147 + h.dx, 30);
        for _ in 0..8 {
            h.frame_start(false);
            assert_eq!(h.unshift(220, 11), (160, 11), "the bar is still on the screen");
        }
        assert_eq!(h.taps, 8);
    }

    #[test]
    fn a_repainted_bar_replaces_the_previous_span_rather_than_widening_it() {
        let mut h = Hud::new(520);
        h.saw(0, 400); // an implausible span, to make a leak obvious
        h.frame_start(true);
        h.saw(147 + h.dx, 30);
        // 147+60 = 207..237. A tap at 300 was inside the old span and must not
        // be inside this one.
        assert_eq!(h.unshift(300, 11), (300, 11));
        assert_eq!(h.unshift(220, 11), (160, 11));
    }
}
