//! 배치변경 — moving and resizing the landscape field HUD's controls.
//!
//! The row reads 기본 or 커스텀 on both option lists ([`crate::options`] on the
//! title screen, [`crate::igoptions`] in game), right under 패드키, and behaves
//! the way that row does: moving it to 커스텀 asks in the guest's two-button
//! popup, 아니오 leaves it on 기본, 예 opens the editor ([`crate::hudedit`]).
//! 기본 stops applying the layout and keeps the file.
//!
//! # Why the HUD cannot simply be told where to go
//!
//! hero2/hero3 draw their own HUD; this game draws its own (ANALYSIS.md §68).
//! Its positions are spread over two `.rodata` tables (the belt's, one copy
//! for drawing and one for touching), `StateGame` fields that
//! `SetTouchKeypad` refills at every touch, and immediates in
//! `StateGame::DrawTouchUi` / `TouchPressed` / `mainKeyPressed`. Moving the
//! art by editing those would be a dozen guest edits — and **resizing** cannot
//! be done that way at all: no blitter the HUD uses scales.
//!
//! # So the host draws it again, from the game's own pixels
//!
//! The two functions that draw the HUD are hooked at entry, both pure (no
//! store outside their stack, read off the disassembly):
//!
//! * `GMenu::DrawSkillBelt` when `Battle::DrawGameUI` calls it — the six
//!   belt slots and the page flip;
//! * `StateGame::DrawTouchUi` — the joystick, OK, the dash button, the 취소
//!   button at the top-right edge, and the `map shop save` bar.
//!
//! While a layout is in force each is run **twice into an offscreen frame
//! buffer** the guest itself made (`MC_grpCreateOffScreenFrameBuffer`), once
//! over black and once over white, with `Graphic`'s frame-buffer pointer
//! swapped for the call. Black gives `B = αF`, white gives `B + (1 − α)`, so
//! every pixel's colour *and* transparency come back, translucent plates
//! included. The pixels are then cut per control (a Voronoi cut for the belt,
//! whose slots sit close; fixed regions for the rest, which never meet) and
//! composited into the real frame buffer at the player's place and size with
//! an area-weighted resampler — `out = B' + T'·D`, which at 100% and no move
//! is the guest's own blend back to the last bit of an opaque pixel.
//!
//! Nothing of the guest is rewritten. The original call is skipped only while
//! a layout is in force; at 기본 the hooks keep a snapshot of the field for
//! the editor and decline.
//!
//! # Touches follow the pictures
//!
//! The guest still tests its own rectangles, so [`route`] translates real
//! fingers before the guest sees them: a press on a control's new body is
//! handed over at the corresponding point of its old one (and its moves and
//! release go through the same map), a press on a spot a control *left* is
//! swallowed, the joystick moves its whole zone with it, and everything else
//! passes untouched. Only while the field HUD is what is on screen — the same
//! four tests `DrawTouchUi`'s early-outs and `TouchPressed`'s dispatch make —
//! and never for the pad's synthetic finger, which speaks guest coordinates.

use std::collections::HashMap;

use arm32_cpu::{reg, Mode};

use crate::gamepad::{PadAction, TouchUi, BUTTON, BUTTON_KEYS, KEY_OK, KEY_PAGE};
use crate::mem::RETURN_MAGIC;
use crate::options::{self, Canvas};
use crate::padmap::Scene;
use crate::{Touch, Vm};

/// `<files>/hl5_hudlayout.txt` — the mode and one line per control.
pub const LAYOUT_FILE: &str = "hl5_hudlayout.txt";

/// The size range the player may pick, in percent.
pub const MIN_PCT: u16 = 50;
pub const MAX_PCT: u16 = 150;

// ---- the controls ------------------------------------------------------------

/// One movable control.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Ctl {
    /// A belt button, in the guest's order (`gamepad::BUTTON_KEYS`): 퀵1, 퀵2,
    /// 콤보, 퀵3, 퀵4, 퀵5.
    Slot(u8),
    /// The belt page flip (`'#'`).
    Page,
    /// The OK disc.
    Ok,
    /// The dash button at the right edge.
    Dash,
    /// The 취소 button at the top-right edge (`mainKeyPressed`'s CLEAR).
    Cancel,
    /// The touch joystick.
    Stick,
    /// The `map shop save` bar.
    Bar,
}

/// Every control, in file order.
pub const CONTROLS: [Ctl; 12] = [
    Ctl::Slot(0),
    Ctl::Slot(1),
    Ctl::Slot(2),
    Ctl::Slot(3),
    Ctl::Slot(4),
    Ctl::Slot(5),
    Ctl::Page,
    Ctl::Ok,
    Ctl::Dash,
    Ctl::Cancel,
    Ctl::Stick,
    Ctl::Bar,
];

/// The belt's members — what one `DrawSkillBelt` call draws.
const BELT: [Ctl; 7] =
    [Ctl::Slot(0), Ctl::Slot(1), Ctl::Slot(2), Ctl::Slot(3), Ctl::Slot(4), Ctl::Slot(5), Ctl::Page];
/// `DrawTouchUi`'s members, in the order the guest draws them (the joystick
/// first, the dash button last), which is the order they are put back in.
const TOUCH: [Ctl; 5] = [Ctl::Stick, Ctl::Ok, Ctl::Bar, Ctl::Cancel, Ctl::Dash];

impl Ctl {
    pub fn index(self) -> usize {
        match self {
            Ctl::Slot(i) => i as usize,
            Ctl::Page => 6,
            Ctl::Ok => 7,
            Ctl::Dash => 8,
            Ctl::Cancel => 9,
            Ctl::Stick => 10,
            Ctl::Bar => 11,
        }
    }

    /// The file's name for it.
    pub fn token(self) -> &'static str {
        match self {
            Ctl::Slot(0) => "slot1",
            Ctl::Slot(1) => "slot2",
            Ctl::Slot(2) => "combo",
            Ctl::Slot(3) => "slot3",
            Ctl::Slot(4) => "slot4",
            Ctl::Slot(_) => "slot5",
            Ctl::Page => "page",
            Ctl::Ok => "ok",
            Ctl::Dash => "dash",
            Ctl::Cancel => "cancel",
            Ctl::Stick => "stick",
            Ctl::Bar => "bar",
        }
    }

    fn from_token(t: &str) -> Option<Ctl> {
        CONTROLS.iter().copied().find(|c| c.token() == t)
    }

    /// The editor's name for it, EUC-KR, NUL-terminated.
    pub fn label(self) -> &'static [u8] {
        match self {
            Ctl::Slot(0) => &[0xc4, 0xfc, 0x31, 0x00],          // 퀵1
            Ctl::Slot(1) => &[0xc4, 0xfc, 0x32, 0x00],          // 퀵2
            Ctl::Slot(2) => &[0xc4, 0xde, 0xba, 0xb8, 0x00],    // 콤보
            Ctl::Slot(3) => &[0xc4, 0xfc, 0x33, 0x00],          // 퀵3
            Ctl::Slot(4) => &[0xc4, 0xfc, 0x34, 0x00],          // 퀵4
            Ctl::Slot(_) => &[0xc4, 0xfc, 0x35, 0x00],          // 퀵5
            Ctl::Page => &[0xc6, 0xe4, 0xc0, 0xcc, 0xc1, 0xf6, 0x00], // 페이지
            Ctl::Ok => &[0xc8, 0xae, 0xc0, 0xce, 0x00],         // 확인
            Ctl::Dash => &[0xb4, 0xeb, 0xbd, 0xc3, 0x00],       // 대시
            Ctl::Cancel => &[0xc3, 0xeb, 0xbc, 0xd2, 0x00],     // 취소
            Ctl::Stick => &[0xc1, 0xb6, 0xc0, 0xcc, 0xbd, 0xba, 0xc6, 0xbd, 0x00], // 조이스틱
            Ctl::Bar => &[0xb8, 0xde, 0xb4, 0xba, 0xb9, 0xd9, 0x00], // 메뉴바
        }
    }

    /// The pad action a badge on it names, if any.
    fn action(self) -> Option<PadAction> {
        Some(match self {
            Ctl::Slot(i) => PadAction::Key(BUTTON_KEYS[i as usize]),
            Ctl::Page => PadAction::Key(KEY_PAGE),
            Ctl::Ok => PadAction::Key(KEY_OK),
            Ctl::Dash => PadAction::Dash,
            Ctl::Cancel | Ctl::Stick | Ctl::Bar => return None,
        })
    }
}

// ---- the layout --------------------------------------------------------------

/// Where one control goes: its pivot moved by `(dx, dy)` canvas pixels and its
/// art scaled by `pct` percent about that pivot.
///
/// Offsets and not positions, because the guest anchors every control to a
/// canvas edge: the same offsets keep their meaning when the canvas is
/// FULL / 90% / 80% wide.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Place {
    pub dx: i32,
    pub dy: i32,
    pub pct: u16,
}

impl Place {
    pub const HOME: Place = Place { dx: 0, dy: 0, pct: 100 };

    pub fn is_home(self) -> bool {
        self == Place::HOME
    }

    pub fn scale(self) -> f32 {
        f32::from(self.pct) / 100.0
    }
}

/// A place for every control.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Layout {
    pub places: [Place; 12],
}

impl Default for Layout {
    fn default() -> Self {
        Layout { places: [Place::HOME; 12] }
    }
}

impl Layout {
    pub fn get(&self, c: Ctl) -> Place {
        self.places[c.index()]
    }

    pub fn set(&mut self, c: Ctl, p: Place) {
        self.places[c.index()] = p;
    }

    pub fn is_home(&self) -> bool {
        self.places.iter().all(|p| p.is_home())
    }

    fn group_moved(&self, group: &[Ctl]) -> bool {
        group.iter().any(|c| !self.get(*c).is_home())
    }

    /// The file: `mode custom|default`, then `<control> <dx> <dy> <percent>`.
    /// Every control is written, home or not, so the file says the whole
    /// layout and not a diff against a default that might change.
    pub fn file_text(&self, custom: bool) -> String {
        let mut s = String::from("# written by the 배치변경 row; <control> <dx> <dy> <percent>\n");
        s.push_str(if custom { "mode custom\n" } else { "mode default\n" });
        for c in CONTROLS {
            let p = self.get(c);
            s.push_str(&format!("{} {} {} {}\n", c.token(), p.dx, p.dy, p.pct));
        }
        s
    }

    /// Read a file. Lines it does not understand are skipped — a hand edit
    /// must not keep the game from starting — and every number is clamped to
    /// what the editor could have written.
    pub fn parse(text: &str) -> (Option<bool>, Layout) {
        let mut mode = None;
        let mut out = Layout::default();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let mut w = line.split_whitespace();
            let Some(head) = w.next() else { continue };
            if head == "mode" {
                mode = match w.next() {
                    Some("custom") => Some(true),
                    Some("default") => Some(false),
                    _ => mode,
                };
                continue;
            }
            let Some(c) = Ctl::from_token(head) else { continue };
            let nums: Vec<i32> = w.filter_map(|t| t.parse().ok()).collect();
            if nums.len() != 3 {
                continue;
            }
            out.set(
                c,
                Place {
                    dx: nums[0].clamp(-2000, 2000),
                    dy: nums[1].clamp(-2000, 2000),
                    pct: (nums[2].clamp(i32::from(MIN_PCT), i32::from(MAX_PCT))) as u16,
                },
            );
        }
        (mode, out)
    }
}

// ---- geometry -----------------------------------------------------------------

/// `(x, y, w, h)`.
pub type Rect = (i32, i32, i32, i32);

fn contains(r: Rect, x: i32, y: i32) -> bool {
    x >= r.0 && y >= r.1 && x < r.0 + r.2 && y < r.1 + r.3
}

fn grow(r: Rect, m: i32) -> Rect {
    (r.0 - m, r.1 - m, r.2 + 2 * m, r.3 + 2 * m)
}

fn clip(r: Rect, w: i32, h: i32) -> Rect {
    let x0 = r.0.max(0);
    let y0 = r.1.max(0);
    let x1 = (r.0 + r.2).min(w);
    let y1 = (r.1 + r.3).min(h);
    (x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
}

fn union(a: Rect, b: Rect) -> Rect {
    if a.2 <= 0 || a.3 <= 0 {
        return b;
    }
    if b.2 <= 0 || b.3 <= 0 {
        return a;
    }
    let x0 = a.0.min(b.0);
    let y0 = a.1.min(b.1);
    let x1 = (a.0 + a.2).max(b.0 + b.2);
    let y1 = (a.1 + a.3).max(b.1 + b.3);
    (x0, y0, x1 - x0, y1 - y0)
}

/// The sizes of the HUD's own images, read from the `GbmImage`s the guest
/// loaded (`+2` width, `+4` height). The defaults are what this binary's
/// images measure, for the moment before they are loaded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Dims {
    pub ok: (i32, i32),
    pub dash: (i32, i32),
    pub cancel: (i32, i32),
    pub stick: (i32, i32),
    pub knob: (i32, i32),
    pub page: (i32, i32),
}

impl Default for Dims {
    fn default() -> Self {
        // Measured on this binary (`hudaudit`, ANALYSIS.md §68).
        Dims { ok: (55, 54), dash: (36, 54), cancel: (36, 32), stick: (75, 73), knob: (45, 44), page: (25, 21) }
    }
}

/// `gv` offsets of the HUD's images, as `DrawTouchUi` addresses them:
/// OK idle / glyph / pressed glyph, 취소, dash idle / glyph / pressed,
/// joystick plate and handle.
pub(crate) const GV_IMG_OK: u32 = 0x17c;
pub(crate) const GV_IMG_OK_GLYPH: u32 = 0x194;
const GV_IMG_OK_PRESSED: u32 = 0x1ac;
pub(crate) const GV_IMG_CANCEL: u32 = 0x1c4;
pub(crate) const GV_IMG_DASH: u32 = 0x1f4;
pub(crate) const GV_IMG_DASH_GLYPH: u32 = 0x20c;
const GV_IMG_DASH_PRESSED: u32 = 0x224;
pub(crate) const GV_IMG_JOY: u32 = 0x23c;
pub(crate) const GV_IMG_HANDLE: u32 = 0x254;
/// The `GMenu`'s page-flip image (`DrawSkillBelt` 0x83914).
pub(crate) const GMENU_IMG_PAGE: u32 = 0x228;

fn image_dims(vm: &Vm, img: u32) -> Option<(i32, i32)> {
    let w = i32::from(vm.mem.read_u16(img + 2));
    let h = i32::from(vm.mem.read_u16(img + 4));
    (w > 0 && h > 0 && w < 400 && h < 400).then_some((w, h))
}

fn read_dims(vm: &Vm, gmenu: Option<u32>) -> Dims {
    let d = Dims::default();
    let Some(gv) = vm.symbol(options::GV) else { return d };
    let max2 = |a: Option<(i32, i32)>, b: Option<(i32, i32)>, c: Option<(i32, i32)>, dflt: (i32, i32)| {
        let mut out: Option<(i32, i32)> = None;
        for v in [a, b, c].into_iter().flatten() {
            out = Some(match out {
                None => v,
                Some(o) => (o.0.max(v.0), o.1.max(v.1)),
            });
        }
        out.unwrap_or(dflt)
    };
    Dims {
        ok: max2(
            image_dims(vm, gv + GV_IMG_OK),
            image_dims(vm, gv + GV_IMG_OK_GLYPH),
            image_dims(vm, gv + GV_IMG_OK_PRESSED),
            d.ok,
        ),
        dash: max2(
            image_dims(vm, gv + GV_IMG_DASH),
            image_dims(vm, gv + GV_IMG_DASH_GLYPH),
            image_dims(vm, gv + GV_IMG_DASH_PRESSED),
            d.dash,
        ),
        cancel: image_dims(vm, gv + GV_IMG_CANCEL).unwrap_or(d.cancel),
        stick: image_dims(vm, gv + GV_IMG_JOY).unwrap_or(d.stick),
        knob: image_dims(vm, gv + GV_IMG_HANDLE).unwrap_or(d.knob),
        page: gmenu.and_then(|g| image_dims(vm, g + GMENU_IMG_PAGE)).unwrap_or(d.page),
    }
}

/// One control at home.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Geo {
    /// The point its art scales about and moves with — the art's centre.
    pub pivot: (i32, i32),
    /// The body a player sees and presses: what the editor outlines, a badge
    /// sits in, and a press is tested against once moved.
    pub face: Rect,
    /// Everything the guest may draw for it, a little generously: where its
    /// pixels are captured from.
    pub region: Rect,
    /// Where the guest itself tests a press on it, in drawn coordinates. For
    /// the joystick, the zone a press grabs it in.
    pub touch: Rect,
}

/// The landscape belt, as `SetTouchKeypad` (0xE12E4) and the wide branch of
/// `DrawSkillBelt` (0x84194) both lay it out: `(W - xs[i], H - 6 - ys[i])`.
const WIDE_XS: [i32; 6] = [0xa0, 0x80, 0x60, 0x41, 0x2e, 0x24];
const WIDE_YS: [i32; 6] = [0x2a, 0x37, 0x4c, 0x65, 0x86, 0xaa];
/// The joystick's centre, as both touch schemes treat it (`gamepad`).
const STICK_X: i32 = 0x41;
const STICK_INSET: i32 = 0x41;
/// How far the handle travels from the centre (`DrawTouchUi` asks
/// `GetTouchJoyXY(angle, 0xc)`).
const KNOB_REACH: i32 = 0xc;
/// The joystick's press zones in `TouchPressed` (both `Check_Rect` calls,
/// inclusive bounds): `(0, 30)..(W-210, H)` and `(0, 30)..(W-80, H-80)`.
fn stick_zones(w: i32, h: i32) -> [Rect; 2] {
    [(0, 30, w - 0xd2 + 1, h - 30 + 1), (0, 30, w - 0x50 + 1, h - 0x6e + 1)]
}

/// Every control at home on a `w` x `h` landscape canvas; `bar_dx` is how far
/// `hud.rs` moves the bar.
pub fn geometry(w: i32, h: i32, bar_dx: i32, d: &Dims) -> [Geo; 12] {
    let mut out = [Geo { pivot: (0, 0), face: (0, 0, 0, 0), region: (0, 0, 0, 0), touch: (0, 0, 0, 0) }; 12];
    for i in 0..6 {
        let (tx, ty) = (w - WIDE_XS[i], h - 6 - WIDE_YS[i]);
        // The plate is drawn six below the touch rect (`gamepad::BELT_DRAW_DY`)
        // and the icon centred on (x + 15, y + 21).
        let face = (tx, ty + 6, BUTTON, BUTTON);
        out[i] = Geo { pivot: (tx + 15, ty + 21), face, region: grow(face, 3), touch: (tx, ty, BUTTON, BUTTON) };
    }
    // The page flip: touched at (W-0xbf, H-0x2a) 30x30, drawn centred on
    // (W-0xb0, H-0x1b) — the same centre.
    let page_c = (w - 0xb0, h - 0x1b);
    let page_img = (page_c.0 - d.page.0 / 2, page_c.1 - d.page.1 / 2, d.page.0, d.page.1);
    let page_touch = (w - 0xbf, h - 0x2a, BUTTON, BUTTON);
    out[Ctl::Page.index()] = Geo { pivot: page_c, face: page_touch, region: grow(page_img, 2), touch: page_touch };
    // OK: the IRect (W-0x3d, H-0x46, 60, 60), the disc centred on (W-0x1f, H-0x28).
    let ok_c = (w - 0x1f, h - 0x28);
    let ok_img = (ok_c.0 - d.ok.0 / 2, ok_c.1 - d.ok.1 / 2, d.ok.0, d.ok.1);
    let ok_touch = (w - 0x3d, h - 0x46, 0x3c, 0x3c);
    out[Ctl::Ok.index()] = Geo { pivot: ok_c, face: ok_touch, region: grow(ok_img, 1), touch: ok_touch };
    // Dash: right-aligned on the edge, centred on H-0x56 (anchor 0x22);
    // touched at x >= W-0x23, H-0x65 <= y < H-0x47.
    let dash_img = (w - d.dash.0, h - 0x56 - d.dash.1 / 2, d.dash.0, d.dash.1);
    let dash_touch = (w - 0x23, h - 0x65, 0x23, 0x1e);
    out[Ctl::Dash.index()] = Geo {
        pivot: (dash_img.0 + dash_img.2 / 2, dash_img.1 + dash_img.3 / 2),
        face: union(dash_img, dash_touch),
        region: grow(dash_img, 1),
        touch: dash_touch,
    };
    // 취소: right-aligned on the edge, top at 30 (anchor 0x21); touched at
    // x >= W-0x24, 30 <= y <= 60 (`mainKeyPressed` 0x9CC50).
    let cancel_img = (w - d.cancel.0, 0x1e, d.cancel.0, d.cancel.1);
    let cancel_touch = (w - 0x24, 0x1e, 0x24, 0x1f);
    out[Ctl::Cancel.index()] = Geo {
        pivot: (cancel_img.0 + cancel_img.2 / 2, cancel_img.1 + cancel_img.3 / 2),
        face: union(cancel_img, cancel_touch),
        region: grow(cancel_img, 1),
        touch: cancel_touch,
    };
    // The joystick at rest, centred on (0x41, H-0x41).
    let sc = (STICK_X, h - STICK_INSET);
    let reach = (d.stick.0 / 2).max(KNOB_REACH + d.knob.0 / 2).max(d.stick.1 / 2).max(KNOB_REACH + d.knob.1 / 2);
    let stick_face = (sc.0 - d.stick.0 / 2, sc.1 - d.stick.1 / 2, d.stick.0, d.stick.1);
    let z = stick_zones(w, h);
    out[Ctl::Stick.index()] = Geo {
        pivot: sc,
        face: stick_face,
        region: (sc.0 - reach - 2, sc.1 - reach - 2, 2 * reach + 4, 2 * reach + 4),
        touch: union(z[0], z[1]),
    };
    // The bar: three cells at 0x93 + 0x32k (+ the hud.rs shift), 30 wide; the
    // label runs a few rows past the 13-row plate.
    let bar = (0x93 + bar_dx, 0, 0x32 * 2 + 0x1e, 0x0d + 8);
    out[Ctl::Bar.index()] = Geo { pivot: (bar.0 + bar.2 / 2, bar.1 + bar.3 / 2), face: bar, region: grow(bar, 2), touch: bar };
    out
}

/// The user-space position of a drawn-space point of control `g` placed at `p`.
fn to_user(g: &Geo, p: Place, pt: (f32, f32)) -> (f32, f32) {
    let s = p.scale();
    (
        g.pivot.0 as f32 + p.dx as f32 + (pt.0 - g.pivot.0 as f32) * s,
        g.pivot.1 as f32 + p.dy as f32 + (pt.1 - g.pivot.1 as f32) * s,
    )
}

/// A drawn-space rect of control `g`, where the player sees it.
pub fn map_rect(g: &Geo, p: Place, r: Rect) -> Rect {
    if p.is_home() {
        return r;
    }
    let (x0, y0) = to_user(g, p, (r.0 as f32, r.1 as f32));
    let (x1, y1) = to_user(g, p, ((r.0 + r.2) as f32, (r.1 + r.3) as f32));
    let (x0, y0) = (x0.round() as i32, y0.round() as i32);
    (x0, y0, (x1.round() as i32 - x0).max(1), (y1.round() as i32 - y0).max(1))
}

/// Keep a control's body on the canvas: move `p` the least that does it.
pub fn clamp_place(g: &Geo, p: Place, w: i32, h: i32) -> Place {
    let f = map_rect(g, p, g.face);
    let mut q = p;
    if f.0 < 0 {
        q.dx -= f.0;
    } else if f.0 + f.2 > w {
        q.dx -= (f.0 + f.2 - w).min(f.0);
    }
    if f.1 < 0 {
        q.dy -= f.1;
    } else if f.1 + f.3 > h {
        q.dy -= (f.1 + f.3 - h).min(f.1);
    }
    q
}

// ---- per-Vm state ---------------------------------------------------------------

/// How one finger is being translated.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Route {
    /// `guest = g0 + (user - u0) / s`, the offset clamped to `reach` when set.
    Map { ctl: Ctl, u0: (f32, f32), g0: (f32, f32), s: f32, reach: Option<f32> },
    /// The press landed where a control used to be: the guest never hears of
    /// this finger.
    Swallow,
}

/// The guest frame buffer the capture draws into.
#[derive(Clone, Copy, Debug)]
struct Fb {
    desc: u32,
    base: u32,
    bpl: u32,
    w: i32,
    h: i32,
}

/// Per-`Vm` state.
#[derive(Default)]
pub struct HudLayout {
    /// The hooks are in (options rows on, a landscape canvas wider than 320).
    pub installed: bool,
    /// The layout is in force (the row reads 커스텀).
    pub custom: bool,
    /// The question has been asked and not yet answered.
    pub pending: bool,
    /// Which menu asked.
    pub scene: Scene,
    /// The layout in force while [`HudLayout::custom`].
    pub layout: Layout,
    /// The capture buffer, made on first use; `Some(None)` = this binary
    /// cannot make one and the layout is never drawn.
    scratch: Option<Option<Fb>>,
    routes: HashMap<i32, Route>,
    /// `DrawImageBlending` return addresses a capture pass leaves out.
    skip: Vec<u32>,
    /// The last capture of each group — see [`Cached`].
    cache: [Option<Cached>; SLOTS],
    /// Captures whose draw over white was not needed.
    pub reused: u64,
    /// 취소 was drawn where the layout put it during the frame now running /
    /// in the picture on screen — the repaint-clock latch [`route`] reads off
    /// the field (see [`crate::hud`] for why it is not the poll clock).
    cancel_drawn: bool,
    cancel_on_screen: bool,
    /// Menu-page belt layouts rewritten for a touch.
    pub menu_slots: u64,
    /// Belts drawn with a registration cursor mark ([`belt_cursor`]).
    pub marked: u64,
    /// The field as the player last saw it — with the status bars, without
    /// the belt and the touch controls — for the in-game editor's backdrop.
    pub(crate) snapshot: Option<(i32, i32, Vec<u16>)>,
    /// The belt's region before the belt was drawn, kept from the belt hook
    /// to the touch hook of the same frame.
    pre_belt: Option<(Rect, Vec<u16>)>,
    /// Counters for the probe's summary line.
    pub belts: u64,
    pub touch_uis: u64,
    pub routed: u64,
    pub swallowed: u64,
    pub saved: u64,
    /// The editor's state ([`crate::hudedit`]).
    pub edit: crate::hudedit::Edit,
}

impl HudLayout {
    /// Begin a frame: `painted` says whether the frame that just finished put
    /// a new picture on the screen.
    pub(crate) fn frame_start(&mut self, painted: bool) {
        if painted {
            self.cancel_on_screen = std::mem::take(&mut self.cancel_drawn);
        }
    }

    /// What the row reads.
    pub fn shows_custom(&self) -> bool {
        self.custom || self.pending
    }

    /// The layout the field draws with right now.
    pub fn in_force(&self) -> Option<&Layout> {
        (self.installed && self.custom && !self.layout.is_home()).then_some(&self.layout)
    }
}

// ---- guest symbols -----------------------------------------------------------------

const DRAW_SKILL_BELT: &str = "_ZN5GMenu13DrawSkillBeltEiiihas";
const DRAW_GAME_UI: &str = "_ZN6Battle10DrawGameUIEi";
/// `Battle::DrawGameUI+0xa98` is its one `bl DrawSkillBelt` (0x471B0).
const DRAW_GAME_UI_BELT_RET: u32 = 0x471b4 - 0x46718;
const DRAW_TOUCH_UI: &str = "_ZN9StateGame11DrawTouchUiEv";
pub(crate) const DRAW_TOUCH_CANCEL_UI: &str = "_ZN8CommonUi17DrawTouchCancelUiEv";
const CREATE_OFFSCREEN: &str = "MC_grpCreateOffScreenFrameBuffer";
/// `mainPaint`'s one `bl CommonUi::DrawTouchCancelUi` (0x9C7A8).
const MAIN_PAINT: &str = "_Z9mainPaintv";
const MAIN_PAINT_CANCEL_RET: u32 = 0x9c7ac - 0x9c6e8;
pub(crate) const DRAW_IMAGE_BLENDING: &str = "_ZN7Graphic17DrawImageBlendingEiiiP8GbmImagehhhi";
const DRAW_INFO_RECT: &str = "_ZN8CommonUi12DrawInfoRectEiiiii";
const GET_COMMON_TEXT: &str = "_ZN7TextMgr13GetCommonTextEi";
const P_TEXT_MGR: &str = "_pTextMgr";
/// `common_text` 0x112..0x114 = `map` `shop` `save`.
const BAR_TEXT: i32 = 0x112;
/// The `GMenu*` at `gv + 0x15F8`.
pub(crate) const GV_GMENU: u32 = 0x15f8;

/// `gv` bytes the touch UI reads.
const GV_STATE: u32 = 0x27c;
const GV_TOUCH_SCHEME: u32 = 0x8f;
const GV_STICK_HELD: u32 = 0x125;
const GV_STICK_OWNER: u32 = 0x124;
const GV_STICK_ORIGIN: u32 = 0x120;
const GV_EVENT: u32 = 0x148c;
const GV_BATTLE: u32 = 0x15f0;
const GV_HERO: u32 = 0x1474;
const HERO_SPIRIT_MENU: u32 = 0x1fb6;

/// Install the hooks — only where there is a layout to draw: the option rows
/// are on (they are the only way in) and the canvas is landscape and wider
/// than 320 (the 240-wide `세로` HUD is laid out differently and the narrow
/// bar variants are not handled). Reads the file either way.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    let text = std::fs::read_to_string(vm.cfg.storage.join(LAYOUT_FILE)).ok();
    if let Some(t) = text.as_deref() {
        let (mode, layout) = Layout::parse(t);
        vm.hudlayout.layout = layout;
        vm.hudlayout.custom = mode == Some(true);
    }
    if !vm.cfg.screen_option || vm.cfg.screen_mode.is_portrait() || vm.cfg.wipi_w <= 320 {
        vm.hudlayout.custom = false;
        return Ok(Vec::new());
    }
    for s in [DRAW_SKILL_BELT, DRAW_GAME_UI, DRAW_TOUCH_UI, DRAW_TOUCH_CANCEL_UI, CREATE_OFFSCREEN, MAIN_PAINT, options::GV] {
        if vm.symbol(s).is_none() {
            return Err(format!("배치변경 needs `{s}`, not in this binary"));
        }
    }
    // A layout made on a wider canvas (FULL, then 80%) keeps its offsets —
    // they are from the edge each control is anchored to — but a control
    // pushed far from that edge may now hang off the other one. Pull every
    // body back on before anything is drawn.
    let g = geometry(vm.cfg.wipi_w, vm.cfg.wipi_h, vm.hud.dx, &Dims::default());
    for c in CONTROLS {
        let p = clamp_place(&g[c.index()], vm.hudlayout.layout.get(c), vm.cfg.wipi_w, vm.cfg.wipi_h);
        vm.hudlayout.layout.set(c, p);
    }
    let belt = vm.symbol(DRAW_SKILL_BELT).expect("checked above");
    vm.install_hook(DRAW_SKILL_BELT, belt, belt_hook)?;
    let touch = vm.symbol(DRAW_TOUCH_UI).expect("checked above");
    vm.install_hook(DRAW_TOUCH_UI, touch, touch_ui_hook)?;
    let cancel = vm.symbol(DRAW_TOUCH_CANCEL_UI).expect("checked above");
    vm.install_hook(DRAW_TOUCH_CANCEL_UI, cancel, cancel_ui_hook)?;
    let mut names = vec![DRAW_SKILL_BELT, DRAW_TOUCH_UI, DRAW_TOUCH_CANCEL_UI];
    for m in &MENU_BELTS {
        let f = vm.symbol(m.sym).ok_or_else(|| format!("배치변경 needs `{}`, not in this binary", m.sym))?;
        vm.install_hook(m.sym, f + m.at, menu_belt_hook)?;
        names.push(m.sym);
    }
    vm.hudlayout.installed = true;
    Ok(names)
}

/// The geometry of this canvas, with the images' own sizes. The touch images
/// are loaded by `onStartApp`; the page image belongs to a `GMenu`, the
/// field's (`gv+0x15F8`) or, on the title screen, the key-mapping screen's.
/// A handful of halfword reads, so not cached.
pub(crate) fn geo(vm: &Vm) -> [Geo; 12] {
    let gm = vm
        .symbol(options::GV)
        .map(|gv| vm.mem.read_u32(gv + GV_GMENU))
        .filter(|g| *g != 0)
        .or(vm.padmap.gmenu());
    let d = read_dims(vm, gm);
    geometry(vm.cfg.wipi_w, vm.cfg.wipi_h, vm.hud.dx, &d)
}

fn entered_normally(vm: &Vm) -> bool {
    vm.cpu.reg_get(Mode::User, reg::LR) != RETURN_MAGIC
}

fn graphic(vm: &Vm) -> Option<u32> {
    options::deref(vm, options::LCD_G)
}

fn fb_of(vm: &Vm, desc: u32) -> Option<Fb> {
    if desc == 0 {
        return None;
    }
    let w = vm.mem.read_u32(desc) as i32;
    let h = vm.mem.read_u32(desc + 4) as i32;
    let bpl = vm.mem.read_u32(desc + 8);
    let bpp = vm.mem.read_u32(desc + 0xc);
    let base = vm.mem.read_u32(desc + 0x10);
    // RGB565 only: the display is initialised 5/6/5 and every blitter port
    // in this tree assumes it.
    (bpp == 16 && w > 0 && h > 0 && base != 0 && bpl >= (w as u32) * 2).then_some(Fb { desc, base, bpl, w, h })
}

fn real_fb(vm: &Vm) -> Option<Fb> {
    let g = graphic(vm)?;
    fb_of(vm, vm.mem.read_u32(g))
}

fn scratch(vm: &mut Vm) -> Result<Option<Fb>, String> {
    if let Some(s) = vm.hudlayout.scratch {
        return Ok(s);
    }
    let Some(f) = vm.symbol(CREATE_OFFSCREEN) else {
        vm.hudlayout.scratch = Some(None);
        return Ok(None);
    };
    let desc = vm.call(f, &[vm.cfg.wipi_w as u32, vm.cfg.wipi_h as u32])?;
    let fb = fb_of(vm, desc).filter(|fb| fb.w == vm.cfg.wipi_w && fb.h == vm.cfg.wipi_h);
    if fb.is_none() {
        vm.log.push("배치변경: no RGB565 offscreen buffer — the layout will not be drawn".into());
    }
    vm.hudlayout.scratch = Some(fb);
    Ok(fb)
}

fn read_rect(vm: &Vm, fb: &Fb, r: Rect) -> Vec<u16> {
    let mut out = Vec::with_capacity((r.2 * r.3).max(0) as usize);
    for y in r.1..r.1 + r.3 {
        let at = fb.base + y as u32 * fb.bpl + r.0 as u32 * 2;
        match vm.mem.slice(at, r.2 as u32 * 2) {
            Some(row) => out.extend(row.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]]))),
            None => out.extend(std::iter::repeat(0).take(r.2 as usize)),
        }
    }
    out
}

fn write_rect(vm: &mut Vm, fb: &Fb, r: Rect, px: &[u16]) {
    for (j, y) in (r.1..r.1 + r.3).enumerate() {
        let at = fb.base + y as u32 * fb.bpl + r.0 as u32 * 2;
        if let Some(row) = vm.mem.slice_mut(at, r.2 as u32 * 2) {
            for (i, b) in row.chunks_exact_mut(2).enumerate() {
                b.copy_from_slice(&px[j * r.2 as usize + i].to_le_bytes());
            }
        }
    }
}

fn fill_rect(vm: &mut Vm, fb: &Fb, r: Rect, v: u16) {
    let le = v.to_le_bytes();
    for y in r.1..r.1 + r.3 {
        let at = fb.base + y as u32 * fb.bpl + r.0 as u32 * 2;
        if let Some(row) = vm.mem.slice_mut(at, r.2 as u32 * 2) {
            for b in row.chunks_exact_mut(2) {
                b.copy_from_slice(&le);
            }
        }
    }
}

// ---- capture and composite -------------------------------------------------------

/// One captured rect: the guest's drawing over black and over white.
#[derive(Clone)]
struct Plane {
    rect: Rect,
    black: Vec<u16>,
    white: Vec<u16>,
}

/// The last capture of one group (the belt, or one of the touch UI's two
/// passes), and who owns its pixels.
///
/// The HUD changes far less often than it is drawn: between two frames the
/// belt's icons, the plates and the bar are usually the same pixels. So the
/// draw over black — which every frame needs — is compared with the last
/// one, and when nothing changed the draw over white is not repeated. That
/// halves the guest work the layout costs in steady state. The owner maps
/// depend only on the rects, so they are kept as long as those are.
struct Cached {
    rects: Vec<Rect>,
    planes: Vec<Plane>,
    owners: Option<Vec<Vec<u8>>>,
}

/// Cache slots: the belt, the touch UI's two passes, `mainPaint`'s 취소, and
/// the belt without its registration tail ([`belt_cursor`]).
const SLOT_BELT: usize = 0;
const SLOT_TOUCH: [usize; 2] = [1, 2];
const SLOT_CANCEL: usize = 3;
const SLOT_BELT_BARE: usize = 4;
const SLOTS: usize = 5;

/// Run `draw` into the capture buffer over black — and over white, unless the
/// black picture is the one `slot` already holds — and return what landed in
/// each of `rects`. `draw` gets the buffer's descriptor and must draw with
/// it; `Graphic`'s own frame-buffer pointer is swapped for the call too, for
/// the draws that ask `Graphic::GetFB` themselves.
fn capture(
    vm: &mut Vm,
    slot: Option<usize>,
    rects: &[Rect],
    draw: &mut dyn FnMut(&mut Vm, u32) -> Result<(), String>,
) -> Result<Option<Vec<Plane>>, String> {
    let Some(fb) = scratch(vm)? else { return Ok(None) };
    let Some(g) = graphic(vm) else { return Ok(None) };
    let rects: Vec<Rect> = rects.iter().map(|r| clip(*r, fb.w, fb.h)).filter(|r| r.2 > 0 && r.3 > 0).collect();
    let mut pass = |vm: &mut Vm, bg: u16| -> Result<Vec<Vec<u16>>, String> {
        for r in &rects {
            fill_rect(vm, &fb, *r, bg);
        }
        let old = vm.mem.read_u32(g);
        vm.mem.write_u32(g, fb.desc);
        let r = draw(vm, fb.desc);
        vm.mem.write_u32(g, old);
        r?;
        Ok(rects.iter().map(|r| read_rect(vm, &fb, *r)).collect())
    };
    let black = pass(vm, 0x0000)?;
    if let Some(i) = slot {
        if let Some(c) = vm.hudlayout.cache[i].as_ref() {
            if c.rects == rects && c.planes.iter().zip(&black).all(|(p, b)| p.black == *b) {
                vm.hudlayout.reused += 1;
                return Ok(Some(c.planes.clone()));
            }
        }
    }
    let white = pass(vm, 0xffff)?;
    let planes: Vec<Plane> = rects
        .iter()
        .zip(black.into_iter().zip(white))
        .map(|(rect, (black, white))| Plane { rect: *rect, black, white })
        .collect();
    if let Some(i) = slot {
        let owners = vm.hudlayout.cache[i].take().filter(|c| c.rects == rects).and_then(|c| c.owners);
        vm.hudlayout.cache[i] = Some(Cached { rects, planes: planes.clone(), owners });
    }
    Ok(Some(planes))
}

/// The owner maps for `planes`, from the cache when `slot` has them for
/// these rects.
fn cached_owners(
    vm: &mut Vm,
    slot: usize,
    planes: &[Plane],
    owner: &dyn Fn(i32, i32) -> Option<usize>,
) -> Vec<Vec<u8>> {
    let rects: Vec<Rect> = planes.iter().map(|p| p.rect).collect();
    if let Some(c) = vm.hudlayout.cache[slot].as_ref() {
        if c.rects == rects {
            if let Some(o) = c.owners.as_ref() {
                return o.clone();
            }
        }
    }
    let o = owner_maps(planes, owner);
    if let Some(c) = vm.hudlayout.cache[slot].as_mut() {
        if c.rects == rects {
            c.owners = Some(o.clone());
        }
    }
    o
}

fn unpack(p: u16) -> [f32; 3] {
    [
        f32::from((p >> 11) & 0x1f) / 31.0,
        f32::from((p >> 5) & 0x3f) / 63.0,
        f32::from(p & 0x1f) / 31.0,
    ]
}

fn pack(c: [f32; 3]) -> u16 {
    let q = |v: f32, m: f32| (v.clamp(0.0, 1.0) * m + 0.5) as u16;
    (q(c[0], 31.0) << 11) | (q(c[1], 63.0) << 5) | q(c[2], 31.0)
}

/// The taps of one output pixel `o` along one axis: source pixels and their
/// weights (summing to 1), for a piece whose source coordinate `src` lands at
/// user coordinate `u0 + (src - p0) * s`.
fn taps(o: i32, u0: f32, p0: f32, s: f32, out: &mut Vec<(i32, f32)>) {
    out.clear();
    let a = p0 + (o as f32 - u0) / s;
    let b = p0 + (o as f32 + 1.0 - u0) / s;
    let mut i = a.floor() as i32;
    while (i as f32) < b {
        let lo = a.max(i as f32);
        let hi = b.min(i as f32 + 1.0);
        if hi > lo {
            out.push((i, (hi - lo) * s));
        }
        i += 1;
    }
}

/// Who owns each pixel of each plane, by index into the caller's member list
/// (`NOBODY` for none) — worked out once per capture, not once per tap.
const NOBODY: u8 = 0xff;

fn owner_maps(planes: &[Plane], owner: &dyn Fn(i32, i32) -> Option<usize>) -> Vec<Vec<u8>> {
    planes
        .iter()
        .map(|p| {
            let mut m = Vec::with_capacity((p.rect.2 * p.rect.3) as usize);
            for y in p.rect.1..p.rect.1 + p.rect.3 {
                for x in p.rect.0..p.rect.0 + p.rect.2 {
                    m.push(owner(x, y).map_or(NOBODY, |i| i as u8));
                }
            }
            m
        })
        .collect()
}

/// Put member `me`'s pixels of `planes` into the real frame buffer, pivot
/// `pivot` landing on `user` and scaled by `s`, with area-weighted
/// resampling on the premultiplied pair. Pixels outside the piece are
/// transparent. Returns how many output pixels changed.
fn composite(
    vm: &mut Vm,
    real: &Fb,
    planes: &[Plane],
    owners: &[Vec<u8>],
    me: u8,
    pivot: (i32, i32),
    user: (f32, f32),
    s: f32,
) -> u32 {
    // The piece's covered bounds.
    let mut bb: Option<Rect> = None;
    for (p, own) in planes.iter().zip(owners) {
        let (x0, y0, w) = (p.rect.0, p.rect.1, p.rect.2);
        for (i, o) in own.iter().enumerate() {
            if *o == me && (p.black[i] != 0 || p.white[i] != 0xffff) {
                let (x, y) = (x0 + i as i32 % w, y0 + i as i32 / w);
                bb = Some(union(bb.unwrap_or((0, 0, 0, 0)), (x, y, 1, 1)));
            }
        }
    }
    let Some(src) = bb else { return 0 };
    let sample = |x: i32, y: i32| -> ([f32; 3], [f32; 3]) {
        if contains(src, x, y) {
            for (p, own) in planes.iter().zip(owners) {
                if !contains(p.rect, x, y) {
                    continue;
                }
                let i = ((y - p.rect.1) * p.rect.2 + (x - p.rect.0)) as usize;
                if own[i] != me {
                    continue;
                }
                let bc = unpack(p.black[i]);
                let wc = unpack(p.white[i]);
                return (bc, [(wc[0] - bc[0]).max(0.0), (wc[1] - bc[1]).max(0.0), (wc[2] - bc[2]).max(0.0)]);
            }
        }
        ([0.0; 3], [1.0; 3])
    };
    let (px, py) = (pivot.0 as f32, pivot.1 as f32);
    let ux0 = user.0 + (src.0 as f32 - px) * s;
    let ux1 = user.0 + ((src.0 + src.2) as f32 - px) * s;
    let uy0 = user.1 + (src.1 as f32 - py) * s;
    let uy1 = user.1 + ((src.1 + src.3) as f32 - py) * s;
    let ox0 = (ux0.floor() as i32).max(0);
    let ox1 = (ux1.ceil() as i32).min(real.w);
    let oy0 = (uy0.floor() as i32).max(0);
    let oy1 = (uy1.ceil() as i32).min(real.h);
    if ox0 >= ox1 || oy0 >= oy1 {
        return 0;
    }
    let cols: Vec<Vec<(i32, f32)>> = (ox0..ox1)
        .map(|o| {
            let mut t = Vec::with_capacity(3);
            taps(o, user.0, px, s, &mut t);
            t
        })
        .collect();
    let mut rows = Vec::with_capacity(3);
    let mut changed = 0;
    for oy in oy0..oy1 {
        taps(oy, user.1, py, s, &mut rows);
        let at = real.base + oy as u32 * real.bpl + ox0 as u32 * 2;
        let n = (ox1 - ox0) as usize;
        let Some(row) = vm.mem.slice(at, n as u32 * 2).map(|r| r.to_vec()) else { continue };
        let mut out = row.clone();
        let mut dirty = false;
        for (k, col) in cols.iter().enumerate() {
            let mut acc_b = [0.0f32; 3];
            let mut acc_t = [0.0f32; 3];
            for &(iy, wy) in rows.iter() {
                for &(ix, wx) in col {
                    let wgt = wx * wy;
                    let (b, t) = sample(ix, iy);
                    for c in 0..3 {
                        acc_b[c] += wgt * b[c];
                        acc_t[c] += wgt * t[c];
                    }
                }
            }
            if acc_t.iter().all(|t| *t > 0.999) && acc_b.iter().all(|b| *b < 0.001) {
                continue;
            }
            let d = unpack(u16::from_le_bytes([row[2 * k], row[2 * k + 1]]));
            let o = pack([acc_b[0] + acc_t[0] * d[0], acc_b[1] + acc_t[1] * d[1], acc_b[2] + acc_t[2] * d[2]]);
            out[2 * k..2 * k + 2].copy_from_slice(&o.to_le_bytes());
            dirty = true;
            changed += 1;
        }
        if dirty {
            vm.mem.write_bytes(at, &out);
        }
    }
    changed
}

/// Which of `members` owns a belt pixel: the nearest pivot.
fn nearest(geo: &[Geo; 12], members: &[Ctl], x: i32, y: i32) -> Ctl {
    let mut best = members[0];
    let mut bd = i64::MAX;
    for &c in members {
        let (px, py) = geo[c.index()].pivot;
        let d = i64::from(x - px).pow(2) + i64::from(y - py).pow(2);
        if d < bd {
            bd = d;
            best = c;
        }
    }
    best
}

/// The belt's capture rect: every member's region.
fn belt_rect(geo: &[Geo; 12]) -> Rect {
    BELT.iter().fold((0, 0, 0, 0), |a, c| union(a, geo[c.index()].region))
}

/// `DrawSkillBelt`'s registration tail. While a menu page picks a belt slot
/// for a skill or an item (`[sp]`, the mode byte, nonzero), the call ends by
/// marking the cursor slot `[sp+4]` from that slot's table position: a
/// highlight, a 20x20 plate and the icon being placed, reaching about 10 px
/// outside the slot (0x83D08..0x83E10, 0x8491C, 0x849B0). A Voronoi cut
/// splits that mark between the slot and its neighbours, so the belt is
/// captured once more without it — `[sp]` = 0; nothing else in the function
/// reads that byte — and every pixel the tail changes goes with the cursor
/// slot. Returns that slot's index in [`BELT`]: the cursor counts the five
/// assignable slots, the table skips the combo at index 2.
fn belt_cursor(args: &[u32]) -> Option<usize> {
    if args[4] & 0xff == 0 {
        return None;
    }
    match args[5] as u8 as i8 {
        c @ 0..=1 => Some(c as usize),
        c @ 2..=4 => Some(c as usize + 1),
        _ => None,
    }
}

/// Draw the belt (one call of `draw`) where `layout` puts it. `tail` is the
/// registration cursor's slot and a `draw` without the tail, when the call
/// has one ([`belt_cursor`]).
fn draw_belt(
    vm: &mut Vm,
    real: &Fb,
    geo: &[Geo; 12],
    layout: &Layout,
    draw: &mut dyn FnMut(&mut Vm, u32) -> Result<(), String>,
    tail: Option<(usize, &mut dyn FnMut(&mut Vm, u32) -> Result<(), String>)>,
) -> Result<bool, String> {
    let rect = belt_rect(geo);
    let Some(planes) = capture(vm, Some(SLOT_BELT), &[rect], draw)? else { return Ok(false) };
    let owners =
        cached_owners(vm, SLOT_BELT, &planes, &|x, y| BELT.iter().position(|c| *c == nearest(geo, &BELT, x, y)));
    let put = |vm: &mut Vm, planes: &[Plane], owners: &[Vec<u8>], k: usize| {
        let c = BELT[k];
        let g = geo[c.index()];
        let p = layout.get(c);
        let user = to_user(&g, p, (g.pivot.0 as f32, g.pivot.1 as f32));
        composite(vm, real, planes, owners, k as u8, g.pivot, user, p.scale());
    };
    let Some((cursor, bare_draw)) = tail else {
        for k in 0..BELT.len() {
            put(vm, &planes, &owners, k);
        }
        return Ok(true);
    };
    let Some(bare) = capture(vm, Some(SLOT_BELT_BARE), &[rect], bare_draw)? else { return Ok(false) };
    // The neighbours come from the picture without the mark, so where it lay
    // over them they are whole; the cursor slot comes from the one with it,
    // owning its cell and every pixel the mark changed, and goes last — the
    // guest draws the mark last.
    let mut marked = owners.clone();
    for ((m, f), b) in marked.iter_mut().zip(&planes).zip(&bare) {
        for (i, o) in m.iter_mut().enumerate() {
            if f.black[i] != b.black[i] || f.white[i] != b.white[i] {
                *o = cursor as u8;
            }
        }
    }
    for k in (0..BELT.len()).filter(|k| *k != cursor) {
        put(vm, &bare, &owners, k);
    }
    put(vm, &planes, &marked, cursor);
    Ok(true)
}

/// Which of `DrawTouchUi`'s pieces a draw puts down. The dash button's half
/// disc and the OK disc meet at the right edge (their rims overlap by a few
/// rows: dash `H-113..H-59`, OK `H-67..H-13`), so no cut of one picture can
/// tell their pixels apart — they are captured apart instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Pass {
    /// Everything: the HUD at home, drawn straight.
    All,
    /// Everything but the dash button.
    NoDash,
    /// The dash button (and anything that does not meet it).
    NoOk,
}

/// Draw `DrawTouchUi`'s members where `layout` puts them, from two calls of
/// `draw` (see [`Pass`]). `stick` is where the joystick is drawn this time,
/// in drawn coordinates, and where it should land, if it is drawn at all.
fn draw_touch(
    vm: &mut Vm,
    real: &Fb,
    geo: &[Geo; 12],
    layout: &Layout,
    stick: Option<((i32, i32), (f32, f32))>,
    draw: &mut dyn FnMut(&mut Vm, u32, Pass) -> Result<(), String>,
) -> Result<bool, String> {
    // The regions, the joystick's around where it is drawn this frame.
    let region = |c: Ctl| -> Option<Rect> {
        if c == Ctl::Stick {
            let ((ox, oy), _) = stick?;
            let g = geo[Ctl::Stick.index()];
            let (hw, hh) = (g.region.2 / 2, g.region.3 / 2);
            return Some((ox - hw, oy - hh, g.region.2, g.region.3));
        }
        Some(geo[c.index()].region)
    };
    for (slot, pass) in SLOT_TOUCH.into_iter().zip([Pass::NoDash, Pass::NoOk]) {
        let members: Vec<Ctl> =
            TOUCH.iter().copied().filter(|c| (*c == Ctl::Dash) == (pass == Pass::NoOk)).collect();
        let regions: Vec<(Ctl, Rect)> = members.iter().filter_map(|c| region(*c).map(|r| (*c, r))).collect();
        if regions.is_empty() {
            continue;
        }
        let rects: Vec<Rect> = regions.iter().map(|(_, r)| *r).collect();
        let Some(planes) = capture(vm, Some(slot), &rects, &mut |vm, fb| draw(vm, fb, pass))? else {
            return Ok(false);
        };
        // A pixel belongs to the first region that holds it, in draw order.
        let owners = cached_owners(vm, slot, &planes, &|x, y| regions.iter().position(|(_, r)| contains(*r, x, y)));
        for (k, (c, _)) in regions.iter().enumerate() {
            let c = *c;
            let g = geo[c.index()];
            let p = layout.get(c);
            let (pivot, user) = match (c, stick) {
                (Ctl::Stick, Some((o, u))) => (o, u),
                _ => (g.pivot, to_user(&g, p, (g.pivot.0 as f32, g.pivot.1 as f32))),
            };
            composite(vm, real, &planes, &owners, k as u8, pivot, user, p.scale());
        }
    }
    Ok(true)
}

/// `DrawTouchUi`'s `DrawImageBlending` calls for the OK disc (plate, glyph —
/// the pressed glyph joins the second at 0xE074C) and for the dash button
/// (plate, glyph — the pressed one joins at 0xE083C), as return addresses
/// relative to the function.
const OK_BLEND_RETS: [u32; 2] = [0xe06f8 - 0xe05e0, 0xe0760 - 0xe05e0];
const DASH_BLEND_RETS: [u32; 2] = [0xe07f0 - 0xe05e0, 0xe0850 - 0xe05e0];

/// The `Graphic::DrawImageBlending` arm: while a capture pass leaves a piece
/// out, its calls are skipped. Empty — one length test — at every other time.
pub(crate) fn blend_arm(vm: &mut Vm) -> Result<bool, String> {
    if vm.hudlayout.skip.is_empty() {
        return Ok(false);
    }
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    Ok(vm.hudlayout.skip.contains(&lr))
}

// ---- the field hooks -------------------------------------------------------------

fn snapshots_wanted(vm: &Vm) -> bool {
    // The in-game editor's backdrop. Only where the editor can open.
    vm.hudlayout.installed
}

/// `GMenu::DrawSkillBelt` — the belt, wherever the game draws it: the field
/// (`Battle::DrawGameUI`, also run by the in-game menu's constructors for its
/// backdrop), every page of the in-game menu (`StateInGameMenu::Paint+0xb4`)
/// and the 스킬 / 가방 / 특성 pages' registration (`DrawSkillMenu`,
/// `DrawBagMenu`, `DrawPropertyMenu`). All of them draw the same belt from
/// the same table, so the layout puts it in the same place everywhere — the
/// menu's own taps follow through [`menu_belt_hook`] (ANALYSIS.md §68.10).
fn belt_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    let field = vm.symbol(DRAW_GAME_UI).map(|f| f + DRAW_GAME_UI_BELT_RET) == Some(lr);
    let Some(real) = real_fb(vm) else { return Ok(false) };
    let geo = geo(vm);
    if field && snapshots_wanted(vm) {
        let r = clip(belt_rect(&geo), real.w, real.h);
        let px = read_rect(vm, &real, r);
        vm.hudlayout.pre_belt = Some((r, px));
    }
    let Some(layout) = vm.hudlayout.in_force().copied() else { return Ok(false) };
    if !layout.group_moved(&BELT) {
        return Ok(false);
    }
    let args: Vec<u32> = (0..7).map(|i| vm.arg(i)).collect();
    let f = vm.symbol(DRAW_SKILL_BELT).ok_or("배치변경: DrawSkillBelt vanished")?;
    let mut draw = |vm: &mut Vm, fb: u32| -> Result<(), String> {
        let mut a = args.clone();
        a[1] = fb;
        vm.call(f, &a).map(|_| ())
    };
    let mut bare = |vm: &mut Vm, fb: u32| -> Result<(), String> {
        let mut a = args.clone();
        a[1] = fb;
        a[4] &= !0xff;
        vm.call(f, &a).map(|_| ())
    };
    let tail = belt_cursor(&args)
        .map(|k| (k, &mut bare as &mut dyn FnMut(&mut Vm, u32) -> Result<(), String>));
    if tail.is_some() {
        vm.hudlayout.marked += 1;
    }
    if !draw_belt(vm, &real, &geo, &layout, &mut draw, tail)? {
        return Ok(false);
    }
    vm.hudlayout.belts += 1;
    Ok(true)
}

/// Where the joystick is drawn this frame (drawn coordinates), as
/// `DrawTouchUi` decides it: the fixed scheme always at the centre, the
/// floating one at the press origin while it is held and not at all at rest.
fn stick_origin(vm: &Vm, gv: u32) -> Option<(i32, i32)> {
    let centre = (STICK_X, vm.cfg.wipi_h - STICK_INSET);
    if vm.mem.read_u8(gv + GV_TOUCH_SCHEME) & 1 != 0 {
        return Some(centre);
    }
    if vm.mem.read_u8(gv + GV_STICK_HELD) == 0 {
        return None;
    }
    let s16 = |a: u32| i32::from(vm.mem.read_u16(a) as i16);
    Some((s16(gv + GV_STICK_ORIGIN), s16(gv + GV_STICK_ORIGIN + 2)))
}

/// Where a joystick drawn at `o` should land: under the finger that holds it
/// when a real finger does (its route says where it went down), else moved
/// with the control.
fn stick_user(vm: &Vm, gv: u32, g: &Geo, p: Place, o: (i32, i32)) -> (f32, f32) {
    if vm.mem.read_u8(gv + GV_STICK_HELD) != 0 {
        let owner = i32::from(vm.mem.read_u8(gv + GV_STICK_OWNER) as i8);
        if let Some(Route::Map { ctl: Ctl::Stick, u0, g0, s, .. }) = vm.hudlayout.routes.get(&owner).copied() {
            return (u0.0 + (o.0 as f32 - g0.0) * s, u0.1 + (o.1 as f32 - g0.1) * s);
        }
    }
    to_user(g, p, (o.0 as f32, o.1 as f32))
}

/// `StateGame::DrawTouchUi` — its only caller is the tail of
/// `StateGame::Paint`, so every arrival that is not ours is the field.
fn touch_ui_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let Some(real) = real_fb(vm) else { return Ok(false) };
    if snapshots_wanted(vm) {
        let full = (0, 0, real.w, real.h);
        let mut px = read_rect(vm, &real, full);
        if let Some((r, pre)) = vm.hudlayout.pre_belt.take() {
            for j in 0..r.3 {
                for i in 0..r.2 {
                    px[((r.1 + j) * real.w + r.0 + i) as usize] = pre[(j * r.2 + i) as usize];
                }
            }
        }
        vm.hudlayout.snapshot = Some((real.w, real.h, px));
    }
    let Some(layout) = vm.hudlayout.in_force().copied() else { return Ok(false) };
    if !layout.group_moved(&TOUCH) {
        return Ok(false);
    }
    let Some(gv) = vm.symbol(options::GV) else { return Ok(false) };
    let geo = geo(vm);
    let this = vm.arg(0);
    let stick = stick_origin(vm, gv)
        .map(|o| (o, stick_user(vm, gv, &geo[Ctl::Stick.index()], layout.get(Ctl::Stick), o)));
    let f = vm.symbol(DRAW_TOUCH_UI).ok_or("배치변경: DrawTouchUi vanished")?;
    let mut draw = |vm: &mut Vm, _fb: u32, pass: Pass| -> Result<(), String> {
        let rets: &[u32] = match pass {
            Pass::All => &[],
            Pass::NoDash => &DASH_BLEND_RETS,
            Pass::NoOk => &OK_BLEND_RETS,
        };
        vm.hudlayout.skip = rets.iter().map(|r| f + r).collect();
        let r = vm.call(f, &[this]).map(|_| ());
        vm.hudlayout.skip.clear();
        r
    };
    if !draw_touch(vm, &real, &geo, &layout, stick, &mut draw)? {
        return Ok(false);
    }
    vm.hudlayout.touch_uis += 1;
    vm.hudlayout.cancel_drawn = true;
    Ok(true)
}

/// `CommonUi::DrawTouchCancelUi` from `mainPaint` (0x9C7A8) — the 취소
/// button every state but the field draws (`mainPaint` skips state ids
/// <= 1). Over the editor, and over the key-mapping screen, which draw their
/// own, it is skipped; everywhere else, once 취소 has moved, it is drawn where
/// the layout put it — and [`route`] answers its taps there.
fn cancel_ui_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let Some(layout) = vm.hudlayout.in_force().copied() else {
        return Ok(vm.hudlayout.edit.active);
    };
    let moved = !layout.get(Ctl::Cancel).is_home();
    if vm.hudlayout.edit.active || (vm.padmap.active && moved) {
        return Ok(true);
    }
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if !moved || vm.symbol(MAIN_PAINT).map(|f| f + MAIN_PAINT_CANCEL_RET) != Some(lr) {
        return Ok(false);
    }
    let Some(real) = real_fb(vm) else { return Ok(false) };
    let geo = geo(vm);
    let this = vm.arg(0);
    let f = vm.symbol(DRAW_TOUCH_CANCEL_UI).ok_or("배치변경: DrawTouchCancelUi vanished")?;
    let g = geo[Ctl::Cancel.index()];
    let Some(planes) = capture(vm, Some(SLOT_CANCEL), &[g.region], &mut |vm, _| vm.call(f, &[this]).map(|_| ()))? else {
        return Ok(false);
    };
    let owners = cached_owners(vm, SLOT_CANCEL, &planes, &|x, y| contains(g.region, x, y).then_some(0));
    let p = layout.get(Ctl::Cancel);
    let user = to_user(&g, p, (g.pivot.0 as f32, g.pivot.1 as f32));
    composite(vm, &real, &planes, &owners, 0, g.pivot, user, p.scale());
    vm.hudlayout.cancel_drawn = true;
    Ok(true)
}

/// The three in-game menu pages that take a belt slot by touch — 스킬, 가방,
/// 특성 — each lay the slots out on their own stack, from a five-entry copy
/// of the belt table (slots 0, 1, 3, 4, 5: the combo slot is not one) and
/// the page flip at `(W-0xbf, H-0x2a)`, and test `(x, y, 31, 31)` in their
/// registration substates (`TouchSkillMenu+0xbc0`: a first tap picks the
/// slot, `this+0x11`, a second one is OK). The hook sits where both table
/// branches have joined and rewrites the moved slots on that stack so the
/// page's own test — and its own priority — runs at the player's layout,
/// centred on where each slot is drawn now.
struct MenuBelt {
    sym: &'static str,
    /// The instruction after the arrays are complete.
    at: u32,
    /// `sp` offsets: slot x[5], slot y[5] (i16 each), page x, page y (words).
    xs: u32,
    ys: u32,
    page_x: u32,
    page_y: u32,
}

const MENU_BELTS: [MenuBelt; 3] = [
    // 0x101AEC: both branches rejoin (the wide one `b`s back from 0x101C70).
    MenuBelt { sym: "_ZN15StateInGameMenu14TouchSkillMenuEii", at: 0x328, xs: 0xac, ys: 0xa0, page_x: 0x1c, page_y: 0x2c },
    // 0x103478: the wide branch's `bl GetSubState`, after 0x103474 stored the page y.
    MenuBelt { sym: "_ZN15StateInGameMenu12TouchBagMenuEii", at: 0x594, xs: 0x8c, ys: 0x80, page_x: 0x08, page_y: 0x18 },
    // 0x101088: both branches rejoin (the wide one `b`s back from 0x101374).
    MenuBelt { sym: "_ZN15StateInGameMenu17TouchPropertyMenuEii", at: 0x1ec, xs: 0x7c, ys: 0x70, page_x: 0x10, page_y: 0x2c },
];
/// Which belt slot each of a menu's five is.
const MENU_SLOTS: [u8; 5] = [0, 1, 3, 4, 5];

/// The top-left of a 31x31 test square centred on where `c` is drawn now.
fn menu_square(geo: &[Geo; 12], layout: &Layout, c: Ctl) -> (i32, i32) {
    let g = geo[c.index()];
    let p = layout.get(c);
    (g.pivot.0 + p.dx - 15, g.pivot.1 + p.dy - 15)
}

fn menu_belt_hook(vm: &mut Vm) -> Result<bool, String> {
    let Some(layout) = vm.hudlayout.in_force().copied() else { return Ok(false) };
    if !layout.group_moved(&BELT) {
        return Ok(false);
    }
    let pc = vm.cpu.reg_get(Mode::User, reg::PC);
    let Some(m) = MENU_BELTS.iter().find(|m| vm.symbol(m.sym).map(|f| f + m.at) == Some(pc)) else {
        return Ok(false);
    };
    let sp = vm.cpu.reg_get(Mode::User, reg::SP);
    let geo = geo(vm);
    for (k, b) in MENU_SLOTS.iter().enumerate() {
        let c = Ctl::Slot(*b);
        if layout.get(c).is_home() {
            continue;
        }
        let (x, y) = menu_square(&geo, &layout, c);
        vm.mem.write_u16(sp + m.xs + 2 * k as u32, x as i16 as u16);
        vm.mem.write_u16(sp + m.ys + 2 * k as u32, y as i16 as u16);
    }
    if !layout.get(Ctl::Page).is_home() {
        let (x, y) = menu_square(&geo, &layout, Ctl::Page);
        vm.mem.write_u32(sp + m.page_x, u32::from(x as i16 as u16));
        vm.mem.write_u32(sp + m.page_y, u32::from(y as i16 as u16));
    }
    vm.hudlayout.menu_slots += 1;
    // Watched, not taken: the page's own test runs next.
    Ok(false)
}

// ---- the HUD off the field: the editor and the key-mapping screen ---------------------

/// Draw the whole landscape HUD with the guest's own art onto `c`, where
/// `layout` puts it — the editor's picture and the key-mapping screen's.
/// `scene` says where the belt comes from: the hero's own `GMenu` in game,
/// the title screen's empty frames from a `GMenu` of the host's.
pub(crate) fn draw_hud(vm: &mut Vm, c: &Canvas, scene: Scene, layout: &Layout) -> Result<(), String> {
    let Some(gv) = vm.symbol(options::GV) else { return Ok(()) };
    let (w, h) = (vm.cfg.wipi_w, vm.cfg.wipi_h);
    let geo = geo(vm);
    let Some(real) = fb_of(vm, c.fb) else { return Ok(()) };
    let belt_args = match scene {
        Scene::InGame => {
            let gmenu = vm.mem.read_u32(gv + GV_GMENU);
            (gmenu != 0).then_some(gmenu)
        }
        Scene::Title => crate::padmap::title_gmenu(vm),
    };
    if let Some(gmenu) = belt_args {
        let half_w = c.half_w;
        let slots: [(i32, i32); 6] = std::array::from_fn(|i| (w - WIDE_XS[i], h - 6 - WIDE_YS[i]));
        let mut draw = |vm: &mut Vm, fb: u32| -> Result<(), String> {
            match scene {
                Scene::InGame => {
                    let f = vm.symbol(DRAW_SKILL_BELT).ok_or("배치변경: DrawSkillBelt vanished")?;
                    vm.call(f, &[gmenu, fb, (half_w - 0x67) as u32, (h - 0x2c) as u32, 0, u32::MAX, u32::MAX])?;
                }
                Scene::Title => {
                    let cv = Canvas { graphic: c.graphic, fb, font: c.font, common_ui: c.common_ui, half_w };
                    crate::padmap::draw_belt_frames(vm, &cv, gmenu, &slots, (w - 0xb0, h - 0x1b))?;
                }
            }
            Ok(())
        };
        if layout.group_moved(&BELT) {
            draw_belt(vm, &real, &geo, layout, &mut draw, None)?;
        } else {
            draw(vm, c.fb)?;
        }
    }
    let common_ui = c.common_ui;
    let font = c.font;
    let graphic_obj = c.graphic;
    let bar_dx = vm.hud.dx;
    let mut draw = |vm: &mut Vm, fb: u32, pass: Pass| -> Result<(), String> {
        let Some(blend) = vm.symbol(DRAW_IMAGE_BLENDING) else { return Ok(()) };
        let (cx, cy) = (STICK_X, h - STICK_INSET);
        vm.call(blend, &[graphic_obj, fb, cx as u32, cy as u32, gv + GV_IMG_JOY, 0x10, 0, 0x58, 0x12])?;
        vm.call(blend, &[graphic_obj, fb, cx as u32, cy as u32, gv + GV_IMG_HANDLE, 0x10, 0, 0xff, 0x12])?;
        if pass != Pass::NoOk {
            vm.call(blend, &[graphic_obj, fb, (w - 0x1f) as u32, (h - 0x28) as u32, gv + GV_IMG_OK, 0x10, 0, 0x5a, 0x12])?;
            vm.call(blend, &[graphic_obj, fb, (w - 0x1f) as u32, (h - 0x28) as u32, gv + GV_IMG_OK_GLYPH, 0x10, 4, 0xff, 0x12])?;
        }
        if common_ui != 0 {
            // The bar: three plates and `map shop save`, where hud.rs puts them.
            if let (Some(rect), Some(gct), Some(tm), Some(dt)) =
                (vm.symbol(DRAW_INFO_RECT), vm.symbol(GET_COMMON_TEXT), options::deref(vm, P_TEXT_MGR), vm.symbol(options::DRAW_TEXT))
            {
                for k in 0..3 {
                    let x = 0x93 + 0x32 * k + bar_dx;
                    vm.call(rect, &[common_ui, fb, x as u32, 0, 0x1e, 0xd])?;
                    let s = vm.call(gct, &[tm, (BAR_TEXT + k) as u32])?;
                    if s != 0 {
                        vm.call(dt, &[font, fb, (x + 0xf) as u32, 0, s, 1, options::FG])?;
                    }
                }
            }
            if let Some(f) = vm.symbol(DRAW_TOUCH_CANCEL_UI) {
                vm.call(f, &[common_ui])?;
            }
        }
        if pass != Pass::NoDash {
            vm.call(blend, &[graphic_obj, fb, w as u32, (h - 0x56) as u32, gv + GV_IMG_DASH, 0x10, 0, 0x5a, 0x22])?;
            vm.call(blend, &[graphic_obj, fb, w as u32, (h - 0x56) as u32, gv + GV_IMG_DASH_GLYPH, 0x10, 4, 0xff, 0x22])?;
        }
        Ok(())
    };
    if layout.group_moved(&TOUCH) {
        let g = geo[Ctl::Stick.index()];
        let p = layout.get(Ctl::Stick);
        let user = to_user(&g, p, (g.pivot.0 as f32, g.pivot.1 as f32));
        draw_touch(vm, &real, &geo, layout, Some((g.pivot, user)), &mut draw)?;
    } else {
        draw(vm, c.fb, Pass::All)?;
    }
    Ok(())
}

/// Put the field as the player last saw it onto the frame buffer `fb_desc`
/// — the in-game editor's backdrop. False when there is none of this size.
pub(crate) fn put_snapshot(vm: &mut Vm, fb_desc: u32) -> bool {
    let Some(fb) = fb_of(vm, fb_desc) else { return false };
    let Some((w, h, px)) = vm.hudlayout.snapshot.take() else { return false };
    let fits = w == fb.w && h == fb.h;
    if fits {
        write_rect(vm, &fb, (0, 0, w, h), &px);
    }
    vm.hudlayout.snapshot = Some((w, h, px));
    fits
}

/// The probe's `hudaudit`: capture both groups over the **whole** canvas from
/// the live field and say, per control, what it drew inside its region —
/// and every drawn pixel no region holds, which would be lost when a layout
/// redraws the group. Needs the field (a `StateGame` and the hero's `GMenu`).
pub fn audit(vm: &mut Vm) -> Result<Vec<String>, String> {
    let gv = vm.symbol(options::GV).ok_or("no gv")?;
    let st = vm.mem.read_u32(gv + GV_STATE);
    let gmenu = vm.mem.read_u32(gv + GV_GMENU);
    if st == 0 || gmenu == 0 {
        return Err("hudaudit: not in the field".into());
    }
    let (w, h) = (vm.cfg.wipi_w, vm.cfg.wipi_h);
    let c = options::canvas(vm)?.ok_or("hudaudit: no canvas")?;
    let geo = geo(vm);
    let full = (0, 0, w, h);
    let mut out = Vec::new();
    let belt = vm.symbol(DRAW_SKILL_BELT).ok_or("no DrawSkillBelt")?;
    let half_w = c.half_w;
    let Some(bp) = capture(vm, None, &[full], &mut |vm, fb| {
        vm.call(belt, &[gmenu, fb, (half_w - 0x67) as u32, (h - 0x2c) as u32, 0, u32::MAX, u32::MAX]).map(|_| ())
    })?
    else {
        return Err("hudaudit: no capture buffer".into());
    };
    let br = belt_rect(&geo);
    report(&bp[0], &BELT, &|x, y| contains(br, x, y).then(|| BELT.iter().position(|m| *m == nearest(&geo, &BELT, x, y))).flatten(), "belt", &mut out);
    let touch = vm.symbol(DRAW_TOUCH_UI).ok_or("no DrawTouchUi")?;
    let Some(tp) = capture(vm, None, &[full], &mut |vm, _| vm.call(touch, &[st]).map(|_| ()))? else {
        return Err("hudaudit: no capture buffer".into());
    };
    let stick = stick_origin(vm, gv);
    let regions: Vec<(Ctl, Rect)> = TOUCH
        .iter()
        .filter_map(|c| {
            if *c == Ctl::Stick {
                let (ox, oy) = stick?;
                let g = geo[Ctl::Stick.index()];
                Some((*c, (ox - g.region.2 / 2, oy - g.region.3 / 2, g.region.2, g.region.3)))
            } else {
                Some((*c, geo[c.index()].region))
            }
        })
        .collect();
    let members: Vec<Ctl> = regions.iter().map(|(c, _)| *c).collect();
    report(&tp[0], &members, &|x, y| regions.iter().position(|(_, r)| contains(*r, x, y)), "touch UI", &mut out);
    Ok(out)
}

fn report(p: &Plane, members: &[Ctl], owner: &dyn Fn(i32, i32) -> Option<usize>, what: &str, out: &mut Vec<String>) {
    let mut bb: Vec<Option<Rect>> = vec![None; members.len()];
    let mut n = vec![0u32; members.len()];
    let (mut lost, mut lost_bb) = (0u32, None::<Rect>);
    for y in p.rect.1..p.rect.1 + p.rect.3 {
        for x in p.rect.0..p.rect.0 + p.rect.2 {
            let i = ((y - p.rect.1) * p.rect.2 + (x - p.rect.0)) as usize;
            if p.black[i] == 0 && p.white[i] == 0xffff {
                continue;
            }
            match owner(x, y) {
                Some(k) => {
                    n[k] += 1;
                    bb[k] = Some(union(bb[k].unwrap_or((0, 0, 0, 0)), (x, y, 1, 1)));
                }
                None => {
                    lost += 1;
                    lost_bb = Some(union(lost_bb.unwrap_or((0, 0, 0, 0)), (x, y, 1, 1)));
                }
            }
        }
    }
    for (k, c) in members.iter().enumerate() {
        out.push(format!("{what}: {:<6} {:5} px drawn in {:?}", c.token(), n[k], bb[k]));
    }
    out.push(format!("{what}: {lost} px drawn outside every region {lost_bb:?}"));
}

/// The image sizes this canvas is laid out with, for the probe's summary.
pub fn dims_report(vm: &Vm) -> String {
    let gm = vm.symbol(options::GV).map(|gv| vm.mem.read_u32(gv + GV_GMENU)).filter(|g| *g != 0);
    let d = read_dims(vm, gm);
    format!(
        "ok {:?} dash {:?} cancel {:?} stick {:?} knob {:?} page {:?}{}",
        d.ok,
        d.dash,
        d.cancel,
        d.stick,
        d.knob,
        d.page,
        if gm.is_none() { " (no field GMenu: page is the default)" } else { "" }
    )
}

/// The layout the key-mapping screen and the badges should show.
pub(crate) fn shown_layout(vm: &Vm) -> Layout {
    vm.hudlayout.in_force().copied().unwrap_or_default()
}

/// A field control's body where the player sees it.
pub(crate) fn user_face(geo: &[Geo; 12], layout: &Layout, c: Ctl) -> Rect {
    let g = geo[c.index()];
    map_rect(&g, layout.get(c), g.face)
}

/// Move a rect the key-mapping screen got from the guest's touch UI to where
/// the layout in force puts the control that owns it.
pub(crate) fn place_rect(vm: &Vm, c: Ctl, r: Rect) -> Rect {
    let Some(layout) = vm.hudlayout.in_force().copied() else { return r };
    let geo = geo(vm);
    map_rect(&geo[c.index()], layout.get(c), r)
}

/// The badge targets while a layout is in force: every control a pad button
/// fires, at its new place and size. `None` when there is no layout, so the
/// caller keeps the guest's own rects.
pub(crate) fn badge_controls(vm: &Vm, ui: &TouchUi) -> Option<Vec<(Rect, PadAction)>> {
    let layout = vm.hudlayout.in_force().copied()?;
    let geo = geo(vm);
    let mut out = Vec::with_capacity(9);
    for c in [Ctl::Slot(0), Ctl::Slot(1), Ctl::Slot(2), Ctl::Slot(3), Ctl::Slot(4), Ctl::Slot(5), Ctl::Page, Ctl::Ok] {
        if let Some(a) = c.action() {
            out.push((user_face(&geo, &layout, c), a));
        }
    }
    if ui.dash.is_some() {
        out.push((user_face(&geo, &layout, Ctl::Dash), PadAction::Dash));
    }
    Some(out)
}

// ---- touches -----------------------------------------------------------------------

/// Is the field HUD what a touch lands on? The tests `DrawTouchUi`'s three
/// early-outs make (the state is `StateGame`, no event owns the screen, the
/// battle shows its UI), `TouchPressed`'s dispatch (`GetState() == 0`), and
/// the spirit wheel, which takes presses at the screen centre.
fn field_input(vm: &Vm) -> bool {
    let Some(gv) = vm.symbol(options::GV) else { return false };
    let st = vm.mem.read_u32(gv + GV_STATE);
    if st == 0 || vm.mem.read_u32(st + 4) != 1 || vm.mem.read_u8(st + 0xc) != 0 {
        return false;
    }
    let ev = vm.mem.read_u32(gv + GV_EVENT);
    if ev == 0 || vm.mem.read_u8(ev + 0x79) != 0 {
        return false;
    }
    let b = vm.mem.read_u32(gv + GV_BATTLE);
    if b == 0 || vm.mem.read_u8(b + 4) != 1 {
        return false;
    }
    let hero = vm.mem.read_u32(gv + GV_HERO);
    !(hero != 0 && (vm.mem.read_u8(hero + HERO_SPIRIT_MENU) as i8) > 0)
}

/// The joystick's grab radius in guest pixels — past the guest's ±10 px
/// deadzone, short of any zone edge from the centre.
const STICK_REACH: f32 = 60.0;

fn clamp_into(r: Rect, x: f32, y: f32) -> (f32, f32) {
    (x.clamp(r.0 as f32, (r.0 + r.2 - 1) as f32), y.clamp(r.1 as f32, (r.1 + r.3 - 1) as f32))
}

/// A press on control `c`'s new body, handed to the guest at the matching
/// point of its old touch rect.
fn control_route(geo: &[Geo; 12], layout: &Layout, c: Ctl, x: i32, y: i32) -> Route {
    let (fx, fy) = (x as f32, y as f32);
    let g = geo[c.index()];
    let p = layout.get(c);
    let s = p.scale();
    let gx = g.pivot.0 as f32 + (fx - (g.pivot.0 + p.dx) as f32) / s;
    let gy = g.pivot.1 as f32 + (fy - (g.pivot.1 + p.dy) as f32) / s;
    let g0 = clamp_into(g.touch, gx, gy);
    Route::Map { ctl: c, u0: (fx, fy), g0, s, reach: None }
}

/// Off the field: only 취소 is still on screen (`mainPaint` draws it in
/// every other state) and still tested by `mainKeyPressed` at its old rect.
fn cancel_route(geo: &[Geo; 12], layout: &Layout, x: i32, y: i32) -> Option<Route> {
    if contains(user_face(geo, layout, Ctl::Cancel), x, y) {
        return Some(control_route(geo, layout, Ctl::Cancel, x, y));
    }
    contains(geo[Ctl::Cancel.index()].touch, x, y).then_some(Route::Swallow)
}

/// Decide what a press at `(x, y)` is.
fn press_route(vm: &mut Vm, layout: &Layout, x: i32, y: i32) -> Option<Route> {
    let geo = geo(vm);
    let (w, h) = (vm.cfg.wipi_w, vm.cfg.wipi_h);
    let (fx, fy) = (x as f32, y as f32);
    // Topmost first: the touch UI over the belt, each group last-drawn first.
    for c in TOUCH.iter().rev().chain(BELT.iter().rev()) {
        let c = *c;
        if c == Ctl::Stick {
            continue;
        }
        if contains(user_face(&geo, layout, c), x, y) {
            return Some(control_route(&geo, layout, c, x, y));
        }
    }
    // The joystick's zone, carried with it.
    let g = geo[Ctl::Stick.index()];
    let p = layout.get(Ctl::Stick);
    let s = p.scale();
    let in_zone = |x: i32, y: i32| stick_zones(w, h).iter().any(|z| contains(*z, x, y));
    if in_zone(x - p.dx, y - p.dy) {
        let fixed = vm.symbol(options::GV).is_some_and(|gv| vm.mem.read_u8(gv + GV_TOUCH_SCHEME) & 1 != 0);
        let centre = (g.pivot.0 as f32, g.pivot.1 as f32);
        return Some(if fixed {
            // The fixed stick steers by the angle about its centre.
            Route::Map {
                ctl: Ctl::Stick,
                u0: ((g.pivot.0 + p.dx) as f32, (g.pivot.1 + p.dy) as f32),
                g0: centre,
                s,
                reach: Some(STICK_REACH),
            }
        } else {
            // The floating one starts where the finger lands, in the zone.
            let z = stick_zones(w, h)[0];
            let g0 = clamp_into(z, fx - p.dx as f32, fy - p.dy as f32);
            Route::Map { ctl: Ctl::Stick, u0: (fx, fy), g0, s, reach: Some(STICK_REACH) }
        });
    }
    // Where a control used to be.
    for c in CONTROLS {
        let p = layout.get(c);
        if p.is_home() {
            continue;
        }
        let g = geo[c.index()];
        let vacated = match c {
            Ctl::Stick => (p.dx != 0 || p.dy != 0) && stick_zones(w, h).iter().any(|z| contains(*z, x, y)),
            _ => contains(g.touch, x, y),
        };
        if vacated {
            return Some(Route::Swallow);
        }
    }
    None
}

fn apply(r: Route, x: i32, y: i32, w: i32, h: i32) -> Option<(i32, i32)> {
    match r {
        Route::Swallow => None,
        Route::Map { u0, g0, s, reach, .. } => {
            let mut d = ((x as f32 - u0.0) / s, (y as f32 - u0.1) / s);
            if let Some(m) = reach {
                let len = (d.0 * d.0 + d.1 * d.1).sqrt();
                if len > m {
                    d = (d.0 * m / len, d.1 * m / len);
                }
            }
            let gx = (g0.0 + d.0).round() as i32;
            let gy = (g0.1 + d.1).round() as i32;
            Some((gx.clamp(0, w - 1), gy.clamp(0, h - 1)))
        }
    }
}

/// Translate a real finger for the guest. `None` swallows the event.
pub(crate) fn route(vm: &mut Vm, kind: Touch, x: i32, y: i32, pointer: i32) -> Option<(i32, i32)> {
    let (w, h) = (vm.cfg.wipi_w, vm.cfg.wipi_h);
    match kind {
        Touch::Pressed => {
            vm.hudlayout.routes.remove(&pointer);
            let Some(layout) = vm.hudlayout.in_force().copied() else { return Some((x, y)) };
            let r = if field_input(vm) {
                press_route(vm, &layout, x, y)
            } else if vm.hudlayout.cancel_on_screen && !layout.get(Ctl::Cancel).is_home() {
                cancel_route(&geo(vm), &layout, x, y)
            } else {
                None
            };
            match r {
                Some(r) => {
                    vm.hudlayout.routes.insert(pointer, r);
                    if r == Route::Swallow {
                        vm.hudlayout.swallowed += 1;
                    } else {
                        vm.hudlayout.routed += 1;
                    }
                    apply(r, x, y, w, h)
                }
                None => Some((x, y)),
            }
        }
        Touch::Moved => match vm.hudlayout.routes.get(&pointer).copied() {
            Some(r) => apply(r, x, y, w, h),
            None => Some((x, y)),
        },
        Touch::Released => match vm.hudlayout.routes.remove(&pointer) {
            Some(r) => apply(r, x, y, w, h),
            None => Some((x, y)),
        },
    }
}

// ---- the row ------------------------------------------------------------------------

/// `버튼 배치를;변경하시겠습니까?;아니오를 누르면;기본 상태로 진행됩니다.`
pub const ASK_MSG: &[u8] = &[
    0xb9, 0xf6, 0xc6, 0xb0, 0x20, 0xb9, 0xe8, 0xc4, 0xa1, 0xb8, 0xa6, 0x3b, 0xba, 0xaf, 0xb0,
    0xe6, 0xc7, 0xcf, 0xbd, 0xc3, 0xb0, 0xda, 0xbd, 0xc0, 0xb4, 0xcf, 0xb1, 0xee, 0x3f, 0x3b,
    0xbe, 0xc6, 0xb4, 0xcf, 0xbf, 0xc0, 0xb8, 0xa6, 0x20, 0xb4, 0xa9, 0xb8, 0xa3, 0xb8, 0xe9,
    0x3b, 0xb1, 0xe2, 0xba, 0xbb, 0x20, 0xbb, 0xf3, 0xc5, 0xc2, 0xb7, 0xce, 0x20, 0xc1, 0xf8,
    0xc7, 0xe0, 0xb5, 0xcb, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];

pub(crate) fn write_file(vm: &mut Vm, layout: &Layout, custom: bool) {
    let path = vm.cfg.storage.join(LAYOUT_FILE);
    match std::fs::write(&path, layout.file_text(custom)) {
        Ok(()) => {
            vm.hudlayout.saved += 1;
            vm.log.push(format!("배치변경: {} written ({})", path.display(), if custom { "custom" } else { "default" }))
        }
        Err(e) => vm.log.push(format!("배치변경: cannot write {}: {e}", path.display())),
    }
}

/// The row moved to 기본: the game's own layout, the file kept for later.
pub fn set_default(vm: &mut Vm) {
    vm.hudlayout.custom = false;
    vm.hudlayout.pending = false;
    vm.hudlayout.routes.clear();
    let layout = vm.hudlayout.layout;
    write_file(vm, &layout, false);
    vm.gl.forget_last_draw();
}

/// The row moved to 커스텀 on the title screen's list: ask.
pub fn ask(vm: &mut Vm, this: u32) -> Result<(), String> {
    vm.hudlayout.scene = Scene::Title;
    vm.hudlayout.pending = true;
    if !options::ask_two_buttons(vm, this, ASK_MSG)? {
        vm.hudlayout.pending = false;
    }
    Ok(())
}

/// …on the in-game list.
pub fn ask_ingame(vm: &mut Vm, this: u32) -> Result<(), String> {
    vm.hudlayout.scene = Scene::InGame;
    vm.hudlayout.pending = true;
    if !crate::igoptions::ask_two_buttons(vm, this, ASK_MSG)? {
        vm.hudlayout.pending = false;
    }
    Ok(())
}

/// The popup was answered: 예 opens the editor, 아니오 leaves 기본.
pub fn answered(vm: &mut Vm, yes: bool) -> Result<(), String> {
    vm.hudlayout.pending = false;
    if yes {
        let scene = vm.hudlayout.scene;
        crate::hudedit::open(vm, scene);
    } else {
        vm.log.push("배치변경: 아니오 — stays 기본".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo585() -> [Geo; 12] {
        geometry(585, 270, 92, &Dims::default())
    }

    #[test]
    fn the_file_round_trips_and_clamps() {
        let mut l = Layout::default();
        l.set(Ctl::Ok, Place { dx: -200, dy: -40, pct: 140 });
        l.set(Ctl::Slot(2), Place { dx: 5, dy: 0, pct: 50 });
        let (mode, back) = Layout::parse(&l.file_text(true));
        assert_eq!(mode, Some(true));
        assert_eq!(back, l);
        let (mode, back) = Layout::parse("mode default\nok 1 2 999\nstick x y z\nbogus 1 1 1\ndash 3 4 10\n");
        assert_eq!(mode, Some(false));
        assert_eq!(back.get(Ctl::Ok), Place { dx: 1, dy: 2, pct: MAX_PCT });
        assert_eq!(back.get(Ctl::Dash), Place { dx: 3, dy: 4, pct: MIN_PCT });
        assert!(back.get(Ctl::Stick).is_home());
    }

    #[test]
    fn the_registration_cursor_skips_the_combo_slot() {
        let args = |mode: u32, cursor: i8| [0, 0, 0, 0, mode, cursor as u8 as u32, 1];
        // The field and the menu backdrop draw with mode 0: no mark.
        assert_eq!(belt_cursor(&args(0, 0)), None);
        // The cursor counts 퀵1 퀵2 퀵3 퀵4 퀵5; the table has 콤보 at 2.
        let got: Vec<Ctl> = (0..5).filter_map(|c| belt_cursor(&args(1, c))).map(|k| BELT[k]).collect();
        assert_eq!(got, [Ctl::Slot(0), Ctl::Slot(1), Ctl::Slot(3), Ctl::Slot(4), Ctl::Slot(5)]);
        assert_eq!(got.iter().map(|c| c.index() as u8).collect::<Vec<_>>(), MENU_SLOTS);
        // No cursor, or one past the belt: the plain cut.
        assert_eq!(belt_cursor(&args(1, -1)), None);
        assert_eq!(belt_cursor(&args(1, 5)), None);
        // Only the low byte is the mode (`ldrb [sp]`).
        assert_eq!(belt_cursor(&args(0x100, 0)), None);
    }

    #[test]
    fn every_token_is_distinct_and_every_label_is_euc_kr() {
        for (i, a) in CONTROLS.iter().enumerate() {
            assert_eq!(a.index(), i);
            assert_eq!(Ctl::from_token(a.token()), Some(*a));
            let l = a.label();
            assert_eq!(l.last(), Some(&0));
            let mut k = 0;
            let body = &l[..l.len() - 1];
            while k < body.len() {
                if body[k] >= 0x80 {
                    assert!((0xa1..=0xfe).contains(&body[k + 1]));
                    k += 2;
                } else {
                    k += 1;
                }
            }
        }
    }

    #[test]
    fn the_home_geometry_is_the_guests() {
        let g = geo585();
        // The belt's first slot: touched at (425, 222), plate six below.
        assert_eq!(g[0].touch, (425, 222, 30, 30));
        assert_eq!(g[0].face, (425, 228, 30, 30));
        assert_eq!(g[Ctl::Page.index()].touch, (394, 228, 30, 30));
        assert_eq!(g[Ctl::Ok.index()].touch, (524, 200, 60, 60));
        assert_eq!(g[Ctl::Dash.index()].touch, (550, 169, 35, 30));
        assert_eq!(g[Ctl::Cancel.index()].touch, (549, 30, 36, 31));
        assert_eq!(g[Ctl::Stick.index()].pivot, (65, 205));
        assert_eq!(g[Ctl::Bar.index()].face.0, 147 + 92);
    }

    /// The belt's slots sit close; each one's own body must be its own in the
    /// Voronoi cut, or a slot's icon would travel with its neighbour.
    #[test]
    fn every_belt_face_is_owned_by_its_slot() {
        for (w, h, dx) in [(585, 270, 92), (526, 270, 63), (468, 270, 34), (400, 240, 0), (555, 270, 77)] {
            let g = geometry(w, h, dx, &Dims::default());
            for c in BELT {
                let f = g[c.index()].face;
                for y in f.1 + 2..f.1 + f.3 - 2 {
                    for x in f.0 + 2..f.0 + f.2 - 2 {
                        assert_eq!(nearest(&g, &BELT, x, y), c, "{w}x{h} {c:?} at ({x},{y})");
                    }
                }
            }
        }
    }

    /// Within each capture pass `DrawTouchUi`'s members never meet at home,
    /// so cutting by region is exact there; the one pair that does meet — the
    /// OK disc and the dash button — is never in the same pass.
    #[test]
    fn the_touch_regions_are_disjoint_within_a_pass() {
        for (w, h, dx) in [(585, 270, 92), (400, 240, 0), (468, 270, 34)] {
            let g = geometry(w, h, dx, &Dims::default());
            let ok = g[Ctl::Ok.index()].region;
            let dash = g[Ctl::Dash.index()].region;
            assert!(ok.1 < dash.1 + dash.3, "{w}x{h}: the OK disc and the dash button really do meet");
            let no_dash: Vec<Ctl> = TOUCH.iter().copied().filter(|c| *c != Ctl::Dash).collect();
            for (i, a) in no_dash.iter().enumerate() {
                for b in &no_dash[i + 1..] {
                    let (ra, rb) = (g[a.index()].region, g[b.index()].region);
                    let apart = ra.0 + ra.2 <= rb.0 || rb.0 + rb.2 <= ra.0 || ra.1 + ra.3 <= rb.1 || rb.1 + rb.3 <= ra.1;
                    assert!(apart, "{w}x{h}: {a:?} {ra:?} meets {b:?} {rb:?}");
                }
            }
        }
    }

    #[test]
    fn a_rect_scales_about_the_pivot() {
        let g = geo585()[Ctl::Ok.index()];
        let home = map_rect(&g, Place::HOME, g.face);
        assert_eq!(home, g.face);
        let big = map_rect(&g, Place { dx: 0, dy: 0, pct: 150 }, g.face);
        assert_eq!((big.2, big.3), (90, 90));
        let (cx, cy) = (big.0 + big.2 / 2, big.1 + big.3 / 2);
        assert!((cx - (g.face.0 + 30)).abs() <= 1 && (cy - (g.face.1 + 30)).abs() <= 1);
        let moved = map_rect(&g, Place { dx: -300, dy: -100, pct: 100 }, g.face);
        assert_eq!(moved, (g.face.0 - 300, g.face.1 - 100, 60, 60));
    }

    #[test]
    fn clamping_keeps_the_body_on_the_canvas() {
        let g = geo585()[Ctl::Ok.index()];
        let p = clamp_place(&g, Place { dx: 100, dy: 100, pct: 150 }, 585, 270);
        let f = map_rect(&g, p, g.face);
        assert!(f.0 >= 0 && f.1 >= 0 && f.0 + f.2 <= 585 && f.1 + f.3 <= 270, "{f:?}");
    }

    #[test]
    fn the_resampler_taps_sum_to_one() {
        let mut t = Vec::new();
        for s in [0.5f32, 0.75, 1.0, 1.25, 1.5] {
            for o in -3..40 {
                taps(o, 10.3, 12.0, s, &mut t);
                let sum: f32 = t.iter().map(|(_, w)| *w).sum();
                assert!((sum - 1.0).abs() < 1e-4, "s {s} o {o}: {sum}");
            }
        }
        taps(5, 5.0, 5.0, 1.0, &mut t);
        assert_eq!(t, vec![(5, 1.0)], "at home one output pixel is one source pixel");
    }

    #[test]
    fn pack_inverts_unpack() {
        for p in [0u16, 0xffff, 0xf800, 0x07e0, 0x001f, 0x1234, 0xbeef] {
            assert_eq!(pack(unpack(p)), p);
        }
    }

    fn moved_ok() -> Layout {
        let mut l = Layout::default();
        l.set(Ctl::Ok, Place { dx: -300, dy: -100, pct: 150 });
        l
    }

    #[test]
    fn a_press_on_a_moved_control_reaches_its_old_rect() {
        let g = geo585();
        let l = moved_ok();
        let f = user_face(&g, &l, Ctl::Ok);
        let (x, y) = (f.0 + f.2 / 2, f.1 + f.3 / 2);
        // Through the same arithmetic `press_route` uses.
        let p = l.get(Ctl::Ok);
        let gk = &g[Ctl::Ok.index()];
        let s = p.scale();
        let gx = gk.pivot.0 as f32 + (x as f32 - (gk.pivot.0 + p.dx) as f32) / s;
        let gy = gk.pivot.1 as f32 + (y as f32 - (gk.pivot.1 + p.dy) as f32) / s;
        let g0 = clamp_into(gk.touch, gx, gy);
        assert!(contains(gk.touch, g0.0 as i32, g0.1 as i32));
        let r = Route::Map { ctl: Ctl::Ok, u0: (x as f32, y as f32), g0, s, reach: None };
        let (ax, ay) = apply(r, x, y, 585, 270).unwrap();
        assert!(contains(gk.touch, ax, ay));
        assert_eq!(apply(Route::Swallow, 1, 1, 585, 270), None);
    }

    #[test]
    fn the_stick_reach_keeps_the_angle_and_caps_the_length() {
        let r = Route::Map { ctl: Ctl::Stick, u0: (100.0, 100.0), g0: (65.0, 205.0), s: 0.5, reach: Some(60.0) };
        let (x, y) = apply(r, 100 + 300, 100, 585, 270).unwrap();
        assert_eq!((x, y), (125, 205));
        let (x, y) = apply(r, 100, 100 - 10, 585, 270).unwrap();
        assert_eq!((x, y), (65, 185), "a short drag is scaled by 1/s");
    }

    #[test]
    fn the_question_is_four_short_runs() {
        let body = &ASK_MSG[..ASK_MSG.len() - 1];
        let runs: Vec<&[u8]> = body.split(|b| *b == b';').collect();
        assert_eq!(runs.len(), 4);
        for r in runs {
            assert!(r.len() <= 27, "{} bytes", r.len());
        }
    }
}
