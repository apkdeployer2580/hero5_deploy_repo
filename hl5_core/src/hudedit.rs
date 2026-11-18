//! 배치변경's editor — the field HUD where the player can take hold of it.
//!
//! Opened by 예 on the 배치변경 question ([`crate::hudlayout`]), in the place
//! of the option list it was asked from: `StateMainMenu::DrawOption` on the
//! title screen, `StateInGameMenu::Paint` in game — the same two places the
//! 패드키 screen ([`crate::padmap`]) uses.
//!
//! * **In game** the backdrop is the field as the player last saw it, sharp
//!   (the menu's own backdrop is the blurred copy `BoxBlur565` made when the
//!   menu opened): the touch hook keeps a copy of every field frame, with the
//!   belt's region taken from before the belt was drawn, so the status bars
//!   are there and the controls are not. On the title screen there is no
//!   field and the HUD sits on the darkened title, as on the 패드키 screen.
//! * The controls are the game's own art, drawn by [`hudlayout::draw_hud`]
//!   where the layout being edited puts them — exactly what the field will
//!   show.
//! * A tap picks a control (it is outlined in orange); a finger that moves
//!   off its press point drags the control it went down on. **Two fingers
//!   anywhere** — the phone's own zoom gesture — size the control that is
//!   picked (50–150%): the first finger may land on empty ground or on
//!   another control, and the pinch still sizes the one that was picked
//!   before it (ANALYSIS.md §68.8). `−` / `+` step it by 5%. A tap on empty
//!   ground lets go of the pick. 초기화 puts every control home; 완료 (or
//!   BACK, or the pad's B) keeps the layout and makes it the one in force.
//!
//! The outlines are the host's, on the finished canvas like the pad badges.

use std::collections::HashMap;

use crate::hudlayout::{self, clamp_place, user_face, Ctl, Layout, Place, CONTROLS, MAX_PCT, MIN_PCT};
use crate::options::{self, canvas, label, Canvas, K_BACK, FG};
use crate::padmap::Scene;
use crate::{Touch, Vm};

/// The first finger of a gesture, until it is a drag, a tap or half a pinch.
#[derive(Clone, Copy, Debug)]
struct First {
    pointer: i32,
    at: (i32, i32),
    /// The control it went down on.
    hit: Option<Ctl>,
    /// What was picked before it went down — given back if a second finger
    /// turns the gesture into a pinch before this one moved.
    prev: Option<Ctl>,
    /// Past [`MOVE_SLOP`]: a drag (or nothing, on empty ground), not a tap.
    moved: bool,
}

/// A finger dragging a control.
#[derive(Clone, Copy, Debug)]
struct Drag {
    pointer: i32,
    ctl: Ctl,
    /// Where the finger was, and where the control was, when it went down.
    start: (i32, i32),
    from: Place,
}

/// Two fingers sizing `ctl`.
#[derive(Clone, Copy, Debug)]
struct Pinch {
    a: i32,
    b: i32,
    d0: f32,
    pct0: u16,
    ctl: Ctl,
}

/// How far a finger travels, in canvas pixels, before it is a drag and not a
/// tap — and before a second finger can no longer take the gesture over.
const MOVE_SLOP: f32 = 4.0;

/// What a finger did to the layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Change {
    None,
    Picked,
    Moved(Ctl, Place),
    Resized(Ctl, Place),
}

/// The editor's state.
#[derive(Default)]
pub struct Edit {
    pub active: bool,
    pub scene: Scene,
    /// The layout being edited.
    pub layout: Layout,
    pub selected: Option<Ctl>,
    first: Option<First>,
    drag: Option<Drag>,
    pinch: Option<Pinch>,
    fingers: HashMap<i32, (i32, i32)>,
    /// Eight guest bytes for the size label, allocated on first paint.
    pct_buf: u32,
    pub opened: u64,
    pub moved: u64,
    pub resized: u64,
}

/// The editor's own buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Widget {
    Reset,
    Done,
    Minus,
    Plus,
}

const L_RESET: &[u8] = &[0xc3, 0xca, 0xb1, 0xe2, 0xc8, 0xad, 0x00]; // 초기화
const L_DONE: &[u8] = &[0xbf, 0xcf, 0xb7, 0xe1, 0x00]; // 완료
const L_MINUS: &[u8] = b"-\0";
const L_PLUS: &[u8] = b"+\0";
/// `끌어서 옮기고, 고른 뒤 두 손가락으로 크기 조절`
const HINT: &[u8] = &[
    0xb2, 0xf8, 0xbe, 0xee, 0xbc, 0xad, 0x20, 0xbf, 0xc5, 0xb1, 0xe2, 0xb0, 0xed, 0x2c, 0x20, 0xb0, 0xed,
    0xb8, 0xa5, 0x20, 0xb5, 0xda, 0x20, 0xb5, 0xce, 0x20, 0xbc, 0xd5, 0xb0, 0xa1, 0xb6, 0xf4, 0xc0, 0xb8,
    0xb7, 0xce, 0x20, 0xc5, 0xa9, 0xb1, 0xe2, 0x20, 0xc1, 0xb6, 0xc0, 0xfd, 0x00,
];
/// `123%` as a C string.
fn pct_text(pct: u16) -> Vec<u8> {
    let mut v = format!("{}%", pct.clamp(MIN_PCT, MAX_PCT)).into_bytes();
    v.push(0);
    v
}

/// The step `−` / `+` take, and what a pinch rounds to.
const STEP: u16 = 5;

/// Where the editor's buttons are: a column at the left, under the status
/// bars and above the joystick on every landscape canvas (the joystick's
/// plate starts 110 px above the bottom; the column ends 116 px down).
fn widgets(selected: bool) -> Vec<(Widget, (i32, i32, i32, i32))> {
    let mut v = vec![(Widget::Reset, (8, 40, 56, 16)), (Widget::Done, (8, 60, 56, 16))];
    if selected {
        v.push((Widget::Minus, (8, 100, 18, 16)));
        v.push((Widget::Plus, (46, 100, 18, 16)));
    }
    v
}

fn hit_widget(selected: bool, x: i32, y: i32) -> Option<Widget> {
    widgets(selected).into_iter().find(|(_, r)| x >= r.0 && y >= r.1 && x < r.0 + r.2 && y < r.1 + r.3).map(|(w, _)| w)
}

/// The control under a finger, topmost first — the order the field puts
/// them back in, reversed.
fn hit_control(geo: &[hudlayout::Geo; 12], layout: &Layout, x: i32, y: i32) -> Option<Ctl> {
    const ORDER: [Ctl; 12] = [
        Ctl::Dash,
        Ctl::Cancel,
        Ctl::Bar,
        Ctl::Ok,
        Ctl::Stick,
        Ctl::Page,
        Ctl::Slot(5),
        Ctl::Slot(4),
        Ctl::Slot(3),
        Ctl::Slot(2),
        Ctl::Slot(1),
        Ctl::Slot(0),
    ];
    ORDER.into_iter().find(|c| {
        let f = user_face(geo, layout, *c);
        x >= f.0 && y >= f.1 && x < f.0 + f.2 && y < f.1 + f.3
    })
}

/// Open the editor on the saved layout.
pub fn open(vm: &mut Vm, scene: Scene) {
    let geo = hudlayout::geo(vm);
    let (w, h) = (vm.cfg.wipi_w, vm.cfg.wipi_h);
    let mut layout = vm.hudlayout.layout;
    for c in CONTROLS {
        let p = clamp_place(&geo[c.index()], layout.get(c), w, h);
        layout.set(c, p);
    }
    vm.pad.release_all();
    let e = &mut vm.hudlayout.edit;
    e.active = true;
    e.scene = scene;
    e.layout = layout;
    e.selected = None;
    e.first = None;
    e.drag = None;
    e.pinch = None;
    e.fingers.clear();
    e.opened += 1;
    vm.log.push(format!("배치변경: editor open ({scene:?})"));
    vm.gl.forget_last_draw();
}

/// Keep the layout, make it the one in force, and leave.
pub fn close(vm: &mut Vm) {
    let layout = vm.hudlayout.edit.layout;
    vm.hudlayout.layout = layout;
    vm.hudlayout.custom = true;
    hudlayout::write_file(vm, &layout, true);
    let e = &mut vm.hudlayout.edit;
    e.active = false;
    e.selected = None;
    e.first = None;
    e.drag = None;
    e.pinch = None;
    e.fingers.clear();
    vm.log.push(format!(
        "배치변경: editor closed, {} control(s) off home",
        CONTROLS.iter().filter(|c| !layout.get(**c).is_home()).count()
    ));
    vm.gl.forget_last_draw();
}

fn set_place(vm: &mut Vm, c: Ctl, p: Place) {
    let geo = hudlayout::geo(vm);
    let p = clamp_place(&geo[c.index()], p, vm.cfg.wipi_w, vm.cfg.wipi_h);
    if vm.hudlayout.edit.layout.get(c) != p {
        vm.hudlayout.edit.layout.set(c, p);
        vm.gl.forget_last_draw();
    }
}

fn step(vm: &mut Vm, up: bool) {
    let Some(c) = vm.hudlayout.edit.selected else { return };
    let mut p = vm.hudlayout.edit.layout.get(c);
    // To the next multiple of the step, so a pinched 113% steps to 115%.
    p.pct = if up { (p.pct / STEP + 1) * STEP } else { p.pct.div_ceil(STEP).saturating_sub(1) * STEP };
    p.pct = p.pct.clamp(MIN_PCT, MAX_PCT);
    set_place(vm, c, p);
    vm.hudlayout.edit.resized += 1;
}

fn dist(a: (i32, i32), b: (i32, i32)) -> f32 {
    let (dx, dy) = ((a.0 - b.0) as f32, (a.1 - b.1) as f32);
    (dx * dx + dy * dy).sqrt()
}

impl Edit {
    /// A finger down that is not on one of the editor's buttons; `hit` is the
    /// control under it.
    fn finger_down(&mut self, pointer: i32, at: (i32, i32), hit: Option<Ctl>) -> Change {
        self.fingers.insert(pointer, at);
        if self.pinch.is_some() {
            return Change::None;
        }
        if let Some(f) = self.first {
            if f.pointer != pointer && self.fingers.contains_key(&f.pointer) {
                // A second finger: the gesture is a pinch on what is picked.
                // If the first finger only just went down on another control,
                // that was not a pick — give the old one back.
                if !f.moved && f.prev.is_some() {
                    self.selected = f.prev;
                }
                self.drag = None;
                self.first = None;
                let Some(c) = self.selected else { return Change::None };
                let a = self.fingers.get(&f.pointer).copied().unwrap_or(f.at);
                self.pinch = Some(Pinch { a: f.pointer, b: pointer, d0: dist(a, at).max(8.0), pct0: self.layout.get(c).pct, ctl: c });
                return Change::Picked;
            }
        }
        let prev = self.selected;
        if hit.is_some() {
            self.selected = hit;
        }
        self.drag = None;
        self.first = Some(First { pointer, at, hit, prev, moved: false });
        if hit.is_some() && hit != prev {
            Change::Picked
        } else {
            Change::None
        }
    }

    /// A finger moved. The new place is unclamped; the caller clamps it.
    fn finger_move(&mut self, pointer: i32, at: (i32, i32)) -> Change {
        self.fingers.insert(pointer, at);
        if let Some(p) = self.pinch {
            if pointer != p.a && pointer != p.b {
                return Change::None;
            }
            let (Some(a), Some(b)) = (self.fingers.get(&p.a).copied(), self.fingers.get(&p.b).copied()) else {
                return Change::None;
            };
            let pct = (f32::from(p.pct0) * dist(a, b) / p.d0).round() as i32;
            let mut q = self.layout.get(p.ctl);
            q.pct = pct.clamp(i32::from(MIN_PCT), i32::from(MAX_PCT)) as u16;
            return Change::Resized(p.ctl, q);
        }
        let Some(mut f) = self.first else { return Change::None };
        if f.pointer != pointer {
            return Change::None;
        }
        if !f.moved && dist(f.at, at) >= MOVE_SLOP {
            f.moved = true;
            self.first = Some(f);
            if let Some(c) = f.hit {
                self.drag = Some(Drag { pointer, ctl: c, start: f.at, from: self.layout.get(c) });
            }
        }
        match self.drag {
            Some(d) if d.pointer == pointer => {
                let mut q = self.layout.get(d.ctl);
                q.dx = d.from.dx + (at.0 - d.start.0);
                q.dy = d.from.dy + (at.1 - d.start.1);
                Change::Moved(d.ctl, q)
            }
            _ => Change::None,
        }
    }

    /// A finger left the glass.
    fn finger_up(&mut self, pointer: i32) -> Change {
        self.fingers.remove(&pointer);
        if let Some(p) = self.pinch {
            if pointer == p.a || pointer == p.b {
                // The other finger stays out of it until it, too, is lifted.
                self.pinch = None;
            }
            return Change::None;
        }
        let Some(f) = self.first else { return Change::None };
        if f.pointer != pointer {
            return Change::None;
        }
        self.first = None;
        self.drag = None;
        // A tap on empty ground lets go of the pick.
        if !f.moved && f.hit.is_none() && self.selected.is_some() {
            self.selected = None;
            return Change::Picked;
        }
        Change::None
    }
}

/// A real finger while the editor is up. Every one is the editor's.
pub fn touch(vm: &mut Vm, kind: Touch, x: i32, y: i32, pointer: i32) -> bool {
    if !vm.hudlayout.edit.active {
        return false;
    }
    let change = match kind {
        Touch::Pressed => {
            // The editor's own buttons, for a first finger only — a pinch's
            // second finger may land anywhere.
            let alone = vm.hudlayout.edit.fingers.is_empty() && vm.hudlayout.edit.pinch.is_none();
            let selected = vm.hudlayout.edit.selected.is_some();
            match hit_widget(selected, x, y).filter(|_| alone) {
                Some(w) => {
                    match w {
                        Widget::Reset => {
                            vm.hudlayout.edit.layout = Layout::default();
                            vm.hudlayout.edit.selected = None;
                            vm.log.push("배치변경: 초기화 — every control home".to_string());
                        }
                        Widget::Done => {
                            close(vm);
                            return true;
                        }
                        Widget::Minus => step(vm, false),
                        Widget::Plus => step(vm, true),
                    }
                    // The finger is still the editor's until it lifts.
                    vm.hudlayout.edit.fingers.insert(pointer, (x, y));
                    Change::Picked
                }
                None => {
                    let geo = hudlayout::geo(vm);
                    let layout = vm.hudlayout.edit.layout;
                    let hit = hit_control(&geo, &layout, x, y);
                    vm.hudlayout.edit.finger_down(pointer, (x, y), hit)
                }
            }
        }
        Touch::Moved => vm.hudlayout.edit.finger_move(pointer, (x, y)),
        Touch::Released => vm.hudlayout.edit.finger_up(pointer),
    };
    match change {
        Change::None => {}
        Change::Picked => vm.gl.forget_last_draw(),
        Change::Moved(c, p) => {
            if vm.hudlayout.edit.layout.get(c) != p {
                set_place(vm, c, p);
                vm.hudlayout.edit.moved += 1;
            }
        }
        Change::Resized(c, p) => {
            if vm.hudlayout.edit.layout.get(c) != p {
                set_place(vm, c, p);
                vm.hudlayout.edit.resized += 1;
            }
        }
    }
    true
}

/// A guest key while the editor is up (already a navigation code): BACK
/// leaves, keeping the layout.
pub fn key(vm: &mut Vm, navi: i32) -> Result<(), String> {
    if navi == K_BACK {
        close(vm);
    }
    Ok(())
}

/// A pad button while the editor is up: B / BACK leave, the rest wait.
pub fn pad_button(vm: &mut Vm, code: i32, down: bool) -> bool {
    if !vm.hudlayout.edit.active {
        return false;
    }
    if down && (code == crate::gamepad::KEYCODE_BUTTON_B || code == crate::gamepad::KEYCODE_BACK) {
        close(vm);
    }
    true
}

/// Draw the editor in the menu's place.
pub fn paint(vm: &mut Vm) -> Result<(), String> {
    let Some(c) = canvas(vm)? else { return Ok(()) };
    let (w, h) = (vm.cfg.wipi_w, vm.cfg.wipi_h);
    let scene = vm.hudlayout.edit.scene;
    // In game: the field, sharp, as the player last saw it. On the title
    // screen (or before any field frame): the darkened menu, as 패드키 does.
    let sharp = scene == Scene::InGame && hudlayout::put_snapshot(vm, c.fb);
    if !sharp {
        if let Some(f) = vm.symbol(options::FILL_RECT_ALPHA) {
            vm.call(f, &[c.graphic, c.fb, 0, 0, w as u32, h as u32, 0xb4, 0])?;
        }
    }
    let layout = vm.hudlayout.edit.layout;
    hudlayout::draw_hud(vm, &c, scene, &layout)?;
    // The hint, on a dark strip under the bar's home.
    if let Some(f) = vm.symbol(options::FILL_RECT_ALPHA) {
        vm.call(f, &[c.graphic, c.fb, (c.half_w - 132) as u32, 23, 264, 15, 0xa0, 0])?;
    }
    label(vm, &c, c.half_w, 25, HINT, 1, FG)?;
    let selected = vm.hudlayout.edit.selected;
    for (wd, r) in widgets(selected.is_some()) {
        let l = match wd {
            Widget::Reset => L_RESET,
            Widget::Done => L_DONE,
            Widget::Minus => L_MINUS,
            Widget::Plus => L_PLUS,
        };
        draw_box(vm, &c, r, l)?;
    }
    if let Some(sel) = selected {
        // The control's name, and its size between − and +.
        let pct = layout.get(sel).pct;
        if let Some(f) = vm.symbol(options::FILL_RECT_ALPHA) {
            vm.call(f, &[c.graphic, c.fb, 8, 82, 56, 15, 0xa0, 0])?;
            vm.call(f, &[c.graphic, c.fb, 26, 100, 20, 16, 0xa0, 0])?;
        }
        label(vm, &c, 36, 84, sel.label(), 1, FG)?;
        if vm.hudlayout.edit.pct_buf == 0 {
            vm.hudlayout.edit.pct_buf = vm.heap.alloc(8).unwrap_or(0);
        }
        let buf = vm.hudlayout.edit.pct_buf;
        if let (true, Some(dt)) = (buf != 0, vm.symbol(options::DRAW_TEXT)) {
            vm.mem.write_bytes(buf, &pct_text(pct));
            vm.call(dt, &[c.font, c.fb, 36, 103, buf, 1, FG])?;
        }
    }
    Ok(())
}

/// A black value box with a centred label — the option list's own.
fn draw_box(vm: &mut Vm, c: &Canvas, r: (i32, i32, i32, i32), l: &'static [u8]) -> Result<(), String> {
    if let Some(f) = vm.symbol(options::FILL_ROUND_RECT) {
        vm.call(f, &[c.graphic, c.fb, r.0 as u32, r.1 as u32, r.2 as u32, r.3 as u32, 4, 0xff, 0])?;
    }
    label(vm, c, r.0 + r.2 / 2, r.1 + (r.3 - 11) / 2, l, 1, FG)
}

/// The outlines, onto the host's finished canvas: every control's body
/// faintly, the selected one in the mapping screen's orange.
pub(crate) fn paint_overlay(vm: &mut Vm) {
    if !vm.hudlayout.edit.active {
        return;
    }
    let geo = hudlayout::geo(vm);
    let layout = vm.hudlayout.edit.layout;
    let selected = vm.hudlayout.edit.selected;
    let (w, h) = (vm.gl.fb_w, vm.gl.fb_h);
    for c in CONTROLS {
        let f = user_face(&geo, &layout, c);
        if Some(c) == selected {
            crate::padhud::outline(&mut vm.gl.fb, w, h, f);
        } else {
            crate::padhud::frame(&mut vm.gl.fb, w, h, f, 0xc8_d2e6, 110);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_clear_the_joystick_on_every_landscape_canvas() {
        for (w, h, dx) in [(585, 270, 92), (526, 270, 63), (468, 270, 34), (400, 240, 0), (555, 270, 77)] {
            let g = hudlayout::geometry(w, h, dx, &hudlayout::Dims::default());
            let stick = g[Ctl::Stick.index()].face;
            for (_, r) in widgets(true) {
                assert!(r.1 + r.3 <= stick.1, "{w}x{h}: {r:?} reaches the joystick {stick:?}");
                assert!(r.0 + r.2 <= w);
            }
        }
    }

    #[test]
    fn every_percentage_fits_the_label_buffer() {
        for pct in MIN_PCT..=MAX_PCT {
            let t = pct_text(pct);
            assert_eq!(t.last(), Some(&0));
            assert!(t.len() <= 8);
        }
        assert_eq!(pct_text(0), b"50%\0");
        assert_eq!(HINT.last(), Some(&0));
    }

    #[test]
    fn a_step_lands_on_a_multiple_of_five() {
        let mut p = 113u16;
        p = (p / STEP + 1) * STEP;
        assert_eq!(p, 115);
        let q = 113u16.div_ceil(STEP).saturating_sub(1) * STEP;
        assert_eq!(q, 110);
        assert_eq!(110u16.div_ceil(STEP).saturating_sub(1) * STEP, 105);
    }

    fn tap(e: &mut Edit, p: i32, at: (i32, i32), hit: Option<Ctl>) {
        e.finger_down(p, at, hit);
        e.finger_up(p);
    }

    #[test]
    fn a_tap_picks_and_a_tap_on_empty_ground_lets_go() {
        let mut e = Edit::default();
        tap(&mut e, 0, (500, 200), Some(Ctl::Ok));
        assert_eq!(e.selected, Some(Ctl::Ok));
        // A finger that trembles less than the slop is still a tap.
        e.finger_down(0, (100, 100), None);
        e.finger_move(0, (102, 101));
        e.finger_up(0);
        assert_eq!(e.selected, None);
    }

    /// The report: pick a button, then pinch anywhere — the phone's own zoom
    /// gesture — and the picked button changes size.
    #[test]
    fn two_fingers_anywhere_size_the_picked_control() {
        let mut e = Edit::default();
        tap(&mut e, 0, (500, 200), Some(Ctl::Ok));
        e.finger_down(1, (200, 120), None);
        e.finger_down(2, (240, 120), None);
        assert_eq!(e.finger_move(2, (260, 120)), Change::Resized(Ctl::Ok, Place { dx: 0, dy: 0, pct: 150 }));
        assert_eq!(e.finger_move(1, (230, 120)), Change::Resized(Ctl::Ok, Place { dx: 0, dy: 0, pct: 75 }));
        // Below and above the range it holds at the ends.
        assert_eq!(e.finger_move(1, (259, 120)), Change::Resized(Ctl::Ok, Place { dx: 0, dy: 0, pct: MIN_PCT }));
        e.finger_up(2);
        // The finger left behind does not drag anything.
        assert_eq!(e.finger_move(1, (300, 150)), Change::None);
        e.finger_up(1);
        assert_eq!(e.selected, Some(Ctl::Ok), "a pinch is not a tap on empty ground");
    }

    /// The first finger of the pinch lands on another control: that is not a
    /// pick, the pinch still sizes the one picked before.
    #[test]
    fn a_pinch_that_starts_on_another_control_keeps_the_pick() {
        let mut e = Edit::default();
        tap(&mut e, 0, (500, 200), Some(Ctl::Ok));
        e.finger_down(1, (440, 230), Some(Ctl::Slot(0)));
        assert_eq!(e.selected, Some(Ctl::Slot(0)), "shown at once, as a tap would");
        e.finger_down(2, (400, 230), None);
        assert_eq!(e.selected, Some(Ctl::Ok), "…and given back when it became a pinch");
        assert!(matches!(e.finger_move(2, (380, 230)), Change::Resized(Ctl::Ok, _)));
    }

    #[test]
    fn a_finger_that_moves_drags_the_control_under_it() {
        let mut e = Edit::default();
        e.finger_down(0, (500, 200), Some(Ctl::Ok));
        assert_eq!(e.finger_move(0, (480, 190)), Change::Moved(Ctl::Ok, Place { dx: -20, dy: -10, pct: 100 }));
        // Too late for a second finger to take it over: it already moved.
        e.finger_down(1, (100, 100), Some(Ctl::Stick));
        assert_eq!(e.selected, Some(Ctl::Ok));
        assert!(matches!(e.finger_move(1, (60, 100)), Change::Resized(Ctl::Ok, _)));
    }

    #[test]
    fn with_nothing_picked_a_pinch_on_empty_ground_does_nothing() {
        let mut e = Edit::default();
        e.finger_down(0, (100, 100), None);
        e.finger_down(1, (150, 100), None);
        assert_eq!(e.finger_move(1, (200, 100)), Change::None);
        assert_eq!(e.selected, None);
    }

    #[test]
    fn the_topmost_control_wins() {
        let g = hudlayout::geometry(585, 270, 92, &hudlayout::Dims::default());
        let mut l = Layout::default();
        // OK dragged on top of the first belt slot.
        let s = g[0].face;
        let o = g[Ctl::Ok.index()].face;
        l.set(Ctl::Ok, Place { dx: s.0 - o.0, dy: s.1 - o.1, pct: 100 });
        assert_eq!(hit_control(&g, &l, s.0 + 5, s.1 + 5), Some(Ctl::Ok));
        assert_eq!(hit_control(&g, &Layout::default(), s.0 + 5, s.1 + 5), Some(Ctl::Slot(0)));
    }
}
