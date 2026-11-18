//! Which pad button fires which on-screen control — drawn on the control.
//!
//! # Why positions and not letters
//!
//! Android reports face buttons by POSITION: the kernel's
//! `BTN_SOUTH/EAST/NORTH/WEST` become `KEYCODE_BUTTON_A/B/Y/X` through the
//! generic key layout, so keycode 96 is "the bottom face button" on every
//! standard pad — printed `A` on an Xbox pad, `B` on a Switch pad (Nintendo's
//! letters are mirrored) and `✕` with no letter at all on a PlayStation pad. A
//! badge that printed `A` would be wrong on two of those three families; a
//! badge that draws the position is right on all of them. Left/right and
//! upper/lower are the only vocabulary every pad family agrees on, so that is
//! the whole vocabulary here. (This is the sibling projects' conclusion too,
//! reached from the same operator request: "제조사별로 패드가 다르니까 위치로
//! 표시하는게 나을거같은데".)
//!
//! # Where the badges are painted, and why there
//!
//! This game draws its own touch UI, so there is no host HUD to hang a badge
//! off — the controls belong to the guest. The badges therefore go onto the
//! **host's copy of the finished picture**: `Vm::gl_draw_body` fills
//! `GlState::fb` (the canvas, `0x00RRGGBB`) from the texture the guest just
//! uploaded, and [`paint`] runs immediately after that loop. Three
//! consequences, all of them wanted:
//!
//! * **Guest memory is never written.** The guest cannot read its own HUD
//!   annotation back, so no blitter, blend or frame-buffer copy can pick it up.
//! * It lands on top of everything, because it lands after everything.
//! * It shows up in a `--dump` PNG, which is how this is proven offline.
//!
//! Coordinates need no mapping: the capture layer's framebuffer *is* the
//! logical canvas (`Vm::boot` sizes it from `wipi_w`/`wipi_h`), so a guest
//! rect is already in framebuffer pixels.
//!
//! # Sized for a 30 px control
//!
//! The guest's action buttons are 30x30 *canvas* pixels (`#0x1e` in both hit
//! tests), which at the shipped 4.0x magnification is 120 device pixels. The
//! badge is therefore drawn INSIDE the control's lower-left corner rather than
//! on its rim as in the sibling projects, whose controls are drawn by the host
//! at window resolution: on the rim it would collide with the neighbouring
//! button, and at this scale there is no room to float it outside.

use crate::gamepad::{PadAction, BUTTON, BUTTON_KEYS, KEY_OK, KEY_PAGE};

/// The selection outline on the key-mapping screen.
const OUTLINE: u32 = 0xff_8c1a;

/// Which of the four face positions. `Down` is Xbox `A` / Switch `B` / PS `✕`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FacePos {
    Up,
    Down,
    Left,
    Right,
}

/// Where a pad control sits, in the only terms every pad family shares.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PadPos {
    /// One of the four face buttons — drawn as a diamond of four dots with the
    /// bound one lit.
    Face(FacePos),
    /// A shoulder: which side, and which row. `lower` is the trigger
    /// (L2/LT/ZL), `!lower` the bumper (L1/LB/L).
    Shoulder { right: bool, lower: bool },
    /// The two small middle buttons: SELECT / back / `−` / SHARE on the left,
    /// START / `+` / OPTIONS on the right.
    Middle { right: bool },
    /// Pressing a stick in (L3 / R3).
    StickPress { right: bool },
    /// The d-pad / hat as a whole.
    Dpad,
}

/// Where on the pad a keycode is. `None` for a code this HUD has no picture
/// for. Arms are in physical order so a reader can check them against a pad
/// in their hand.
pub fn pos_for_code(code: i32) -> Option<PadPos> {
    use crate::gamepad::*;
    Some(match code {
        KEYCODE_DPAD_UP | KEYCODE_DPAD_DOWN | KEYCODE_DPAD_LEFT | KEYCODE_DPAD_RIGHT
        | KEYCODE_DPAD_CENTER => PadPos::Dpad,
        KEYCODE_BUTTON_A => PadPos::Face(FacePos::Down),
        KEYCODE_BUTTON_B => PadPos::Face(FacePos::Right),
        KEYCODE_BUTTON_X => PadPos::Face(FacePos::Left),
        KEYCODE_BUTTON_Y => PadPos::Face(FacePos::Up),
        KEYCODE_BUTTON_L1 => PadPos::Shoulder { right: false, lower: false },
        KEYCODE_BUTTON_R1 => PadPos::Shoulder { right: true, lower: false },
        KEYCODE_BUTTON_L2 => PadPos::Shoulder { right: false, lower: true },
        KEYCODE_BUTTON_R2 => PadPos::Shoulder { right: true, lower: true },
        KEYCODE_BUTTON_THUMBL => PadPos::StickPress { right: false },
        KEYCODE_BUTTON_THUMBR => PadPos::StickPress { right: true },
        // Some pads send BACK from the left middle button instead of SELECT.
        KEYCODE_BUTTON_SELECT | KEYCODE_BACK => PadPos::Middle { right: false },
        KEYCODE_BUTTON_START => PadPos::Middle { right: true },
        _ => return None,
    })
}

/// Korean name of a position, for the boot log and the legend. Says the
/// POSITION and then, in brackets, the label the most common pad prints there:
/// the position is the truth, the letter is the hint.
pub fn pos_name_ko(p: PadPos) -> &'static str {
    match p {
        PadPos::Face(FacePos::Up) => "면버튼 위 (Xbox Y)",
        PadPos::Face(FacePos::Down) => "면버튼 아래 (Xbox A)",
        PadPos::Face(FacePos::Left) => "면버튼 왼쪽 (Xbox X)",
        PadPos::Face(FacePos::Right) => "면버튼 오른쪽 (Xbox B)",
        PadPos::Shoulder { right: false, lower: false } => "왼쪽 어깨 위 (L1/LB)",
        PadPos::Shoulder { right: false, lower: true } => "왼쪽 트리거 (L2/LT)",
        PadPos::Shoulder { right: true, lower: false } => "오른쪽 어깨 위 (R1/RB)",
        PadPos::Shoulder { right: true, lower: true } => "오른쪽 트리거 (R2/RT)",
        PadPos::Middle { right: false } => "가운데 왼쪽 (SELECT)",
        PadPos::Middle { right: true } => "가운데 오른쪽 (START)",
        PadPos::StickPress { right: false } => "왼스틱 누르기 (L3)",
        PadPos::StickPress { right: true } => "오른스틱 누르기 (R3)",
        PadPos::Dpad => "십자키 / 왼스틱",
    }
}

/// Badge preference (`<files>/hl5_pad_hud.txt`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PadHud {
    /// Show them while a pad is attached. The default, and what the operator
    /// asked for: "패드가 연결된 경우에만 표시".
    #[default]
    Auto,
    /// Always.
    On,
    /// Never.
    Off,
}

impl PadHud {
    pub fn as_token(self) -> &'static str {
        match self {
            PadHud::Auto => "auto",
            PadHud::On => "on",
            PadHud::Off => "off",
        }
    }

    /// Anything unrecognised parses back to [`PadHud::Auto`] — a hand-edited
    /// file must not change behaviour in silence.
    pub fn parse(s: &str) -> PadHud {
        match s.trim().to_ascii_lowercase().as_str() {
            "on" | "always" | "1" => PadHud::On,
            "off" | "never" | "0" => PadHud::Off,
            _ => PadHud::Auto,
        }
    }

    /// Are the badges showing? `present` = a pad is attached.
    pub fn active(self, present: bool) -> bool {
        match self {
            PadHud::Auto => present,
            PadHud::On => true,
            PadHud::Off => false,
        }
    }
}

/// A badge: a small disc stuck in a control's lower-left corner, holding the
/// position glyph.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Badge {
    pub cx: f32,
    pub cy: f32,
    pub r: f32,
}

/// Radius as a fraction of the control's inscribed radius, and the floor and
/// ceiling in canvas pixels. A 30x30 button gives `15 * 0.46 = 6.9`, i.e. 28
/// device pixels across at the shipped 4.0x — the smallest a five-part glyph
/// stays a glyph at.
const BADGE_FRAC: f32 = 0.46;
const BADGE_MIN: f32 = 5.0;
const BADGE_MAX: f32 = 9.0;

/// Badge for the control occupying `rect`, in its lower-LEFT corner, clamped
/// inside the canvas.
///
/// Lower-left because of how this guest lays its HUD out: the action buttons
/// climb up and to the right of the OK disc, and the belt page button sits at
/// the bottom right, so the down-left corner is the one direction that is
/// inside every control and away from its neighbour.
pub fn badge_at(x: i32, y: i32, w: i32, h: i32, cw: i32, ch: i32) -> Option<Badge> {
    if w <= 0 || h <= 0 || cw <= 0 || ch <= 0 {
        return None;
    }
    let rad = w.min(h) as f32 / 2.0;
    let r = (rad * BADGE_FRAC).clamp(BADGE_MIN.min(rad), BADGE_MAX);
    let cx = x as f32 + r + 1.0;
    let cy = (y + h) as f32 - r - 1.0;
    // A control at the very edge of a narrowed canvas still shows its whole
    // badge. `max` before `min` so a canvas smaller than the badge yields the
    // middle rather than a negative.
    let cx = cx.max(r).min(cw as f32 - r).max(0.0);
    let cy = cy.max(r).min(ch as f32 - r).max(0.0);
    Some(Badge { cx, cy, r })
}

/// Badge fill and rim. Near-opaque so the glyph reads over bright skill art,
/// in the same slate/steel family as the game's own plates.
const FILL: u32 = 0x0a_0e18;
const RIM: u32 = 0x9f_b6d4;
/// The bound member of a set, and the unbound ones.
const LIT: u32 = 0xff_e0a0;
const UNLIT: u32 = 0x76_84a8;
const FILL_A: u32 = 216;
const RIM_A: u32 = 200;

/// Blend `src` over the canvas pixel at `(x, y)` with alpha `a` (0..255).
fn blend(fb: &mut [u32], cw: i32, ch: i32, x: i32, y: i32, src: u32, a: u32) {
    if x < 0 || y < 0 || x >= cw || y >= ch {
        return;
    }
    let i = (y as usize) * (cw as usize) + x as usize;
    let Some(px) = fb.get_mut(i) else { return };
    if a >= 255 {
        *px = src & 0x00ff_ffff;
        return;
    }
    let mix = |s: u32, d: u32| -> u32 { (s * a + d * (255 - a)) / 255 };
    let d = *px;
    let r = mix((src >> 16) & 0xff, (d >> 16) & 0xff);
    let g = mix((src >> 8) & 0xff, (d >> 8) & 0xff);
    let b = mix(src & 0xff, d & 0xff);
    *px = (r << 16) | (g << 8) | b;
}

fn disc(fb: &mut [u32], cw: i32, ch: i32, cx: f32, cy: f32, r: f32, col: u32, a: u32) {
    if r <= 0.0 {
        return;
    }
    let rr = r * r;
    let x0 = (cx - r).floor() as i32;
    let x1 = (cx + r).ceil() as i32;
    let y0 = (cy - r).floor() as i32;
    let y1 = (cy + r).ceil() as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            // Pixel centres, the same convention the display scaler was fixed
            // to use (ANALYSIS.md §32.10) — sampling corners here would shift
            // every glyph half a pixel up and left.
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= rr {
                blend(fb, cw, ch, x, y, col, a);
            }
        }
    }
}

fn ring(fb: &mut [u32], cw: i32, ch: i32, cx: f32, cy: f32, r: f32, col: u32, a: u32) {
    let inner = (r - 1.2).max(0.0);
    let (rr, ii) = (r * r, inner * inner);
    let x0 = (cx - r).floor() as i32;
    let x1 = (cx + r).ceil() as i32;
    let y0 = (cy - r).floor() as i32;
    let y1 = (cy + r).ceil() as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = dx * dx + dy * dy;
            if d <= rr && d >= ii {
                blend(fb, cw, ch, x, y, col, a);
            }
        }
    }
}

fn bar(fb: &mut [u32], cw: i32, ch: i32, cx: f32, cy: f32, hw: f32, hh: f32, col: u32) {
    let x0 = (cx - hw).round() as i32;
    let x1 = (cx + hw).round() as i32;
    let y0 = (cy - hh).round() as i32;
    let y1 = (cy + hh).round() as i32;
    for y in y0..=y1.max(y0) {
        for x in x0..=x1.max(x0) {
            blend(fb, cw, ch, x, y, col, 255);
        }
    }
}

/// Draw one position badge on the control at `rect`.
pub fn draw_badge(fb: &mut [u32], cw: i32, ch: i32, rect: (i32, i32, i32, i32), pos: PadPos) {
    let Some(b) = badge_at(rect.0, rect.1, rect.2, rect.3, cw, ch) else {
        return;
    };
    disc(fb, cw, ch, b.cx, b.cy, b.r, FILL, FILL_A);
    ring(fb, cw, ch, b.cx, b.cy, b.r, RIM, RIM_A);
    glyph(fb, cw, ch, b, pos);
}

/// One visual language for every pad control: **draw the set, light the bound
/// member.** Four face buttons are a diamond of four dots; the shoulders a 2x2
/// of bars (side x row); the two middle buttons two pills; the sticks two
/// discs; the hat a cross. No letters anywhere, which is the point.
fn glyph(fb: &mut [u32], cw: i32, ch: i32, b: Badge, pos: PadPos) {
    let r = b.r;
    let dot = |fb: &mut [u32], dx: f32, dy: f32, lit: bool| {
        let rr = r * if lit { 0.34 } else { 0.16 };
        disc(fb, cw, ch, b.cx + dx, b.cy + dy, rr.max(0.8), if lit { LIT } else { UNLIT }, 255);
    };
    match pos {
        PadPos::Face(f) => {
            let d = r * 0.48;
            dot(fb, 0.0, -d, f == FacePos::Up);
            dot(fb, 0.0, d, f == FacePos::Down);
            dot(fb, -d, 0.0, f == FacePos::Left);
            dot(fb, d, 0.0, f == FacePos::Right);
        }
        PadPos::Shoulder { right, lower } => {
            // The unlit three are drawn as well, and visibly: without them the
            // badge says "something at the lower right" instead of "the lower
            // right ONE OF FOUR", which is the whole reading. `dx` is wide
            // enough that left and right cannot be confused at 30 canvas px.
            let dx = r * 0.46;
            let dy = r * 0.40;
            let hw = r * 0.26;
            for (rr, ll) in [(false, false), (true, false), (false, true), (true, true)] {
                let lit = rr == right && ll == lower;
                let hh = if lit { r * 0.17 } else { r * 0.055 };
                bar(
                    fb,
                    cw,
                    ch,
                    b.cx + if rr { dx } else { -dx },
                    b.cy + if ll { dy } else { -dy },
                    hw,
                    hh,
                    if lit { LIT } else { UNLIT },
                );
            }
        }
        PadPos::Middle { right } => {
            let dx = r * 0.42;
            let hw = r * 0.30;
            bar(fb, cw, ch, b.cx - dx, b.cy, hw, r * 0.12, if right { UNLIT } else { LIT });
            bar(fb, cw, ch, b.cx + dx, b.cy, hw, r * 0.12, if right { LIT } else { UNLIT });
        }
        PadPos::StickPress { right } => {
            let dx = r * 0.44;
            for side in [false, true] {
                let lit = side == right;
                let cx = b.cx + if side { dx } else { -dx };
                if lit {
                    disc(fb, cw, ch, cx, b.cy, r * 0.36, LIT, 255);
                } else {
                    ring(fb, cw, ch, cx, b.cy, r * 0.34, UNLIT, 255);
                }
            }
        }
        PadPos::Dpad => {
            let arm = r * 0.60;
            let t = (r * 0.16).max(0.5);
            bar(fb, cw, ch, b.cx, b.cy, arm, t, LIT);
            bar(fb, cw, ch, b.cx, b.cy, t, arm, LIT);
        }
    }
}

/// Every control of the field HUD, with the pad action that fires it.
///
/// One list so the badge pass and the mapping screen share it, and so the
/// order is the guest's own: the six action buttons in jump-table order, then
/// the belt page button, OK, and the dash button when the touch scheme draws
/// one — the one control whose action is not a key.
pub fn controls(ui: &crate::gamepad::TouchUi) -> Vec<((i32, i32, i32, i32), PadAction)> {
    let mut out: Vec<((i32, i32, i32, i32), PadAction)> = Vec::with_capacity(9);
    // Table positions in both layouts; the picture is six pixels lower.
    for (i, &(x, y)) in ui.buttons.iter().enumerate() {
        out.push((crate::gamepad::drawn_button_rect(x, y), PadAction::Key(BUTTON_KEYS[i])));
    }
    out.push(((ui.page.0, ui.page.1, BUTTON, BUTTON), PadAction::Key(KEY_PAGE)));
    out.push((ui.ok, PadAction::Key(KEY_OK)));
    if let Some(d) = ui.dash {
        out.push((d, PadAction::Dash));
    }
    out
}

/// A 2 px outline just inside `rect` — the mapping screen's "this one is
/// selected" on a control the guest draws.
pub fn outline(fb: &mut [u32], cw: i32, ch: i32, rect: (i32, i32, i32, i32)) {
    let (x, y, w, h) = rect;
    if w <= 0 || h <= 0 {
        return;
    }
    for t in 0..2 {
        for xx in x + t..x + w - t {
            blend(fb, cw, ch, xx, y + t, OUTLINE, 255);
            blend(fb, cw, ch, xx, y + h - 1 - t, OUTLINE, 255);
        }
        for yy in y + t..y + h - t {
            blend(fb, cw, ch, x + t, yy, OUTLINE, 255);
            blend(fb, cw, ch, x + w - 1 - t, yy, OUTLINE, 255);
        }
    }
}

/// A 1 px frame just inside `rect`, `col` at alpha `a` — the 배치변경
/// editor's "this is a control" on everything it can move.
pub fn frame(fb: &mut [u32], cw: i32, ch: i32, rect: (i32, i32, i32, i32), col: u32, a: u32) {
    let (x, y, w, h) = rect;
    if w <= 0 || h <= 0 {
        return;
    }
    for xx in x..x + w {
        blend(fb, cw, ch, xx, y, col, a);
        blend(fb, cw, ch, xx, y + h - 1, col, a);
    }
    for yy in y + 1..y + h - 1 {
        blend(fb, cw, ch, x, yy, col, a);
        blend(fb, cw, ch, x + w - 1, yy, col, a);
    }
}

/// Badge every field control whose key a pad button sends. Returns how many
/// were drawn, which is what the periodic line reports — a zero there means
/// the HUD was found but nothing is bound to it, not that the pass never ran.
pub fn paint(
    fb: &mut [u32],
    cw: i32,
    ch: i32,
    ui: &crate::gamepad::TouchUi,
    overrides: &std::collections::HashMap<i32, Option<PadAction>>,
) -> u32 {
    paint_controls(fb, cw, ch, &controls(ui), overrides)
}

/// [`paint`] for a list of controls already placed — the 배치변경 layout's.
pub fn paint_controls(
    fb: &mut [u32],
    cw: i32,
    ch: i32,
    list: &[((i32, i32, i32, i32), PadAction)],
    overrides: &std::collections::HashMap<i32, Option<PadAction>>,
) -> u32 {
    let mut n = 0;
    for &(rect, action) in list {
        // A rect the guest has not laid out yet (a `StateGame` whose
        // `SetTouchKeypad` has not run) is all zeroes; badging it would put a
        // disc in the top-left corner of the screen.
        // (A belt slot's rect is its table position plus `BELT_DRAW_DY`,
        // so an unlaid one reads `(0, 6)`.)
        if rect.2 <= 0 || rect.3 <= 0 || (rect.0 <= 0 && rect.1 <= crate::gamepad::BELT_DRAW_DY) {
            continue;
        }
        if let Some(pos) = crate::gamepad::pos_for_action(action, overrides) {
            draw_badge(fb, cw, ch, rect, pos);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamepad::{TouchUi, KEY_SLOT};
    use std::collections::HashMap;

    fn ui() -> TouchUi {
        // Roughly where the guest puts them on a 585x270 canvas: the action
        // buttons up the right edge, the page button bottom right, OK left of
        // them.
        TouchUi {
            buttons: [(520, 200), (520, 165), (480, 130), (520, 130), (480, 95), (520, 60)],
            page: (394, 228),
            ok: (33, 146, 65, 32),
            dash: None,
        }
    }

    /// The dash button, when the touch scheme has one, is badged with the
    /// button that dashes — the report was that it never got one.
    #[test]
    fn the_dash_button_gets_the_dash_badge() {
        let mut u = ui();
        u.dash = Some(crate::gamepad::dash_rect(585, 270));
        let mut fb = vec![0u32; 585 * 270];
        let n = paint(&mut fb, 585, 270, &u, &HashMap::new());
        assert_eq!(n, 9, "eight controls and the dash button");
        assert_eq!(
            crate::gamepad::pos_for_action(PadAction::Dash, &HashMap::new()),
            Some(PadPos::Middle { right: true }),
            "START dashes by default"
        );
        let mut ov: HashMap<i32, Option<PadAction>> = HashMap::new();
        ov.insert(crate::gamepad::KEYCODE_BUTTON_START, None);
        let mut fb = vec![0u32; 585 * 270];
        assert_eq!(paint(&mut fb, 585, 270, &u, &ov), 8, "no dash button bound, no badge");
    }

    #[test]
    fn a_thirty_pixel_button_gets_a_badge_that_fits_inside_it() {
        let b = badge_at(520, 200, BUTTON, BUTTON, 585, 270).expect("a badge");
        // …and the badge pass puts a landscape slot's badge on the drawn
        // frame, six pixels under the touch rect.
        assert_eq!(controls(&ui())[0].0, (520, 206, BUTTON, BUTTON));
        assert!(b.r >= 5.0 && b.r <= 9.0, "r = {}", b.r);
        // Inside the control, in its lower-left corner.
        assert!(b.cx - b.r >= 520.0 - 0.01, "left edge {} vs {}", b.cx - b.r, 520);
        assert!(b.cy + b.r <= (200 + BUTTON) as f32 + 0.01);
        assert!(b.cx < 520.0 + BUTTON as f32 / 2.0, "left half");
        assert!(b.cy > 200.0 + BUTTON as f32 / 2.0, "lower half");
    }

    #[test]
    fn a_badge_never_leaves_the_canvas() {
        for (cw, ch) in [(585, 270), (526, 270), (468, 270), (400, 240)] {
            for (x, y) in [(0, 0), (cw - BUTTON, ch - BUTTON), (cw - 2, ch - 2), (-4, -4)] {
                let b = badge_at(x, y, BUTTON, BUTTON, cw, ch).expect("a badge");
                assert!(b.cx - b.r >= -0.01 && b.cx + b.r <= cw as f32 + 0.01, "{b:?} in {cw}");
                assert!(b.cy - b.r >= -0.01 && b.cy + b.r <= ch as f32 + 0.01, "{b:?} in {ch}");
            }
        }
    }

    #[test]
    fn the_default_layout_badges_all_eight_field_controls() {
        let mut fb = vec![0u32; 585 * 270];
        let n = paint(&mut fb, 585, 270, &ui(), &HashMap::new());
        assert_eq!(n, 8, "five slots, the combo, the page flip and OK");
    }

    #[test]
    fn painting_touches_only_the_badge_corners() {
        let mut fb = vec![0u32; 585 * 270];
        paint(&mut fb, 585, 270, &ui(), &HashMap::new());
        let changed = fb.iter().filter(|&&p| p != 0).count();
        assert!(changed > 0, "something was drawn");
        // Eight badges of radius <= 9 cannot cover more than ~8*(19*19) px,
        // and must be nowhere near the whole 158 K canvas.
        assert!(changed < 8 * 20 * 20, "{changed} pixels is more than eight badges");
        // The middle of the play area is untouched.
        assert_eq!(fb[135 * 585 + 290], 0);
    }

    #[test]
    fn an_unbound_key_gets_no_badge() {
        let mut ov: HashMap<i32, Option<PadAction>> = HashMap::new();
        // Unbind every button that could send slot 5.
        for code in [crate::gamepad::KEYCODE_BUTTON_L2] {
            ov.insert(code, None);
        }
        let mut fb = vec![0u32; 585 * 270];
        let n = paint(&mut fb, 585, 270, &ui(), &ov);
        assert_eq!(n, 7, "slot 5 has no button any more, so it has no badge");
    }

    #[test]
    fn a_rect_the_guest_has_not_laid_out_is_skipped() {
        let mut fb = vec![0u32; 585 * 270];
        let n = paint(&mut fb, 585, 270, &TouchUi::default(), &HashMap::new());
        assert_eq!(n, 0);
        assert!(fb.iter().all(|&p| p == 0), "a zeroed rect must not put a disc at 0,0");
    }

    #[test]
    fn every_position_draws_something_distinct() {
        let all = [
            PadPos::Face(FacePos::Up),
            PadPos::Face(FacePos::Down),
            PadPos::Face(FacePos::Left),
            PadPos::Face(FacePos::Right),
            PadPos::Shoulder { right: false, lower: false },
            PadPos::Shoulder { right: true, lower: false },
            PadPos::Shoulder { right: false, lower: true },
            PadPos::Shoulder { right: true, lower: true },
            PadPos::Middle { right: false },
            PadPos::Middle { right: true },
            PadPos::StickPress { right: false },
            PadPos::StickPress { right: true },
            PadPos::Dpad,
        ];
        let mut seen: Vec<Vec<u32>> = Vec::new();
        for p in all {
            let mut fb = vec![0u32; 40 * 40];
            draw_badge(&mut fb, 40, 40, (5, 5, BUTTON, BUTTON), p);
            assert!(fb.iter().any(|&px| px != 0), "{p:?} drew nothing");
            assert!(!seen.contains(&fb), "{p:?} is indistinguishable from another position");
            seen.push(fb);
        }
    }

    #[test]
    fn the_hud_preference_round_trips_and_defaults_to_auto() {
        for p in [PadHud::Auto, PadHud::On, PadHud::Off] {
            assert_eq!(PadHud::parse(p.as_token()), p);
        }
        assert_eq!(PadHud::parse("nonsense"), PadHud::Auto);
        assert!(!PadHud::Auto.active(false), "no pad, no badges");
        assert!(PadHud::Auto.active(true));
        assert!(PadHud::On.active(false));
        assert!(!PadHud::Off.active(true));
    }

    #[test]
    fn the_belt_page_control_is_badged_with_the_page_button() {
        // The operator asked for the page flip specifically; this is the
        // assertion that the on-screen `#` control says which pad button
        // moves it.
        let pos = crate::gamepad::pos_for_key(KEY_PAGE, &HashMap::new());
        assert_eq!(
            pos,
            Some(PadPos::Middle { right: false }),
            "SELECT, and not a stick press: a pad without sticks has to be able to flip the belt"
        );
        let slot0 = crate::gamepad::pos_for_key(KEY_SLOT[0], &HashMap::new());
        assert_eq!(slot0, Some(PadPos::Face(FacePos::Left)));
    }
}
