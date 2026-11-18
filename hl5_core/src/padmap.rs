//! 패드키 — the 기본 / 커스텀 row on 환경설정, and the key-mapping screen
//! behind 커스텀. On both lists: the title screen's (`StateMainMenu`,
//! [`crate::options`]) and the in-game menu's 시스템 > 환경설정
//! (`StateInGameMenu`, [`crate::igoptions`]).
//!
//! # What the player sees
//!
//! The row reads 기본 or 커스텀. Moving it to 커스텀 asks, in the guest's own
//! two-button popup:
//!
//! ```text
//!   패드키 설정을
//!   진행하시겠습니까?
//!   아니오를 누르면
//!   기본 상태로 진행됩니다.
//!            [예]  [아니오]
//! ```
//!
//! 아니오 puts the row back on 기본. 예 opens the mapping screen in the menu's
//! place. **In game** it is the field HUD itself: the skill belt drawn by the
//! guest's own `GMenu::DrawSkillBelt`, the OK disc, the dash button and the
//! joystick plate through the same `Graphic::DrawImageBlending` calls
//! `StateGame::DrawTouchUi` makes, the `map shop save` bar — every control
//! where the player sees it in play, each wearing the badge of the pad button
//! that fires it ([`crate::padhud`]) — plus three boxes for what has no
//! on-screen control: 취소, and the screen's own 초기화 and 완료. On the title
//! screen there is no field to show (the HUD images live in `StateGame`, which
//! does not exist there), so the controls are boxes at the field positions,
//! labelled with the keypad glyphs the game itself prints over the belt in
//! 세로 (`1 3 7 9 * 0`, `#`, `OK`). In 세로 the belt is one row at the bottom
//! and OK/dash are keypad keys, so those join the grid.
//!
//! Tap a control, press a pad button, and that button now fires the control;
//! whatever the button used to fire is left unbound, and so is whatever other
//! button used to fire the control — one button per control, one control per
//! button ([`assign`]). 완료 (or BACK, or the pad's B with nothing selected)
//! closes the screen and writes `hl5_gamepad.txt`; 초기화 goes back to the
//! built-in layout.
//!
//! # Where the state lives
//!
//! `<files>/hl5_gamepad.txt` keeps the bindings (the same file a player could
//! always write by hand; [`crate::gamepad::parse_map_reporting`] is still the
//! one definition of what it means) and `<files>/hl5_padmode.txt` says
//! whether they are in force — `custom` or `default`. 기본 does not delete the
//! file, it only stops reading it, so a layout is never one accidental RIGHT
//! away from being lost.
//!
//! # Who draws what
//!
//! The screen replaces the menu's own paint (`StateMainMenu::DrawOption` or
//! `StateInGameMenu::Paint`, hooked at entry while [`PadMap::active`]) and
//! draws with the guest's primitives, so it is the game's own chrome, font and
//! HUD art. The badges and the selection outline are the host's, painted onto
//! the finished canvas exactly as the field badges are, so one function draws
//! both and a remap moves the field badge and the mapping-screen badge
//! together.

use std::collections::HashMap;

use crate::gamepad::{
    button_action_in, pos_for_action, PadAction, TouchUi, KEYCODE_BACK, KEYCODE_BUTTON_A,
    KEYCODE_BUTTON_B, KEYCODE_BUTTON_L1, KEYCODE_BUTTON_L2, KEYCODE_BUTTON_R1, KEYCODE_BUTTON_R2,
    KEYCODE_BUTTON_SELECT, KEYCODE_BUTTON_START, KEYCODE_BUTTON_THUMBL, KEYCODE_BUTTON_THUMBR,
    KEYCODE_BUTTON_X, KEYCODE_BUTTON_Y, KEY_CLEAR, KEY_COMBO, KEY_MAP, KEY_OK, KEY_PAGE, KEY_SAVE,
    KEY_SHOP, KEY_SLOT, BUTTON,
};
use crate::options::{self, canvas, deref, label, plate, Canvas, K_BACK, FG};
use crate::{Touch, Vm};

/// `<files>/hl5_padmode.txt` — `custom` or `default`.
pub const MODE_FILE: &str = "hl5_padmode.txt";
/// `<files>/hl5_gamepad.txt` — the bindings.
pub const MAP_FILE: &str = "hl5_gamepad.txt";

/// The pad buttons a control can be put on. Directions are not here: they
/// steer (see [`crate::gamepad`]), and a d-pad bound to a skill would leave
/// the hero unable to walk. `KEYCODE_BACK` is here because some pads send it
/// for their B or SELECT button — a player holding such a pad has to be able
/// to put something on it.
pub const ASSIGNABLE: [i32; 13] = [
    KEYCODE_BACK,
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
];

/// The hat's centre press, which fires OK by default. It is not a button a
/// player can be asked to press on the mapping screen (most pads have no
/// such control), but while the custom table is in force it is **unbound**:
/// otherwise unbinding 확인 from every face button would leave the hat centre
/// still firing it, and the badge pass — a reverse lookup over every code —
/// would show the d-pad glyph on 확인 as if something had been assigned.
/// That was the first report against this screen ("끊었는데 남은 키로 자동
/// 할당된다"); the same hole existed for 취소 through the gamepad-source BACK,
/// which is why BACK is assignable above.
const HAT_CENTRE: i32 = crate::gamepad::KEYCODE_DPAD_CENTER;

/// Everything the custom table decides: the assignable buttons plus the hat
/// centre. The file spells out every one of these, so no built-in default can
/// reach the field while the table is in force.
fn table_codes() -> impl Iterator<Item = i32> {
    ASSIGNABLE.iter().copied().chain(std::iter::once(HAT_CENTRE))
}

/// Which menu the screen is open over.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scene {
    /// The title screen's 환경설정 (`StateMainMenu`).
    #[default]
    Title,
    /// 시스템 > 환경설정 inside the game (`StateInGameMenu`).
    InGame,
}

/// What a box on the screen does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum What {
    /// A control a pad button can be bound to.
    Act(PadAction),
    /// 초기화 — back to the built-in layout.
    Reset,
    /// 완료 — save and close.
    Done,
}

/// How a control is shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// A host-labelled box drawn with the option list's primitives.
    Box,
    /// The guest's own HUD art, already on the screen; the host adds only the
    /// badge and, when selected, an outline.
    Hud,
}

/// One control on the mapping screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Control {
    pub rect: (i32, i32, i32, i32),
    pub what: What,
    pub label: &'static [u8],
    pub shape: Shape,
}

// Labels, EUC-KR / ASCII, NUL-terminated. The belt and page carry the glyphs
// the game itself prints over them in 세로 (`DrawTouchUi`'s relaid label loop:
// `1 3 7 9 * 0`), OK its own `OK` (`common_text` 0x38).
const L_KEYS: [&[u8]; 6] = [b"1\0", b"3\0", b"7\0", b"9\0", b"*\0", b"0\0"];
const L_PAGE: &[u8] = b"#\0";
const L_OK: &[u8] = b"OK\0";
const L_DASH: &[u8] = &[0xb4, 0xeb, 0xbd, 0xc3, 0x00]; // 대시
const L_CLEAR: &[u8] = &[0xc3, 0xeb, 0xbc, 0xd2, 0x00]; // 취소
const L_MAP: &[u8] = b"map\0";
const L_SHOP: &[u8] = b"shop\0";
const L_SAVE: &[u8] = b"save\0";
const L_RESET: &[u8] = &[0xc3, 0xca, 0xb1, 0xe2, 0xc8, 0xad, 0x00]; // 초기화
const L_DONE: &[u8] = &[0xbf, 0xcf, 0xb7, 0xe1, 0x00]; // 완료
/// `버튼을 고른 뒤 패드 키를 누르세요`
const HINT: &[u8] = &[
    0xb9, 0xf6, 0xc6, 0xb0, 0xc0, 0xbb, 0x20, 0xb0, 0xed, 0xb8, 0xa5, 0x20, 0xb5, 0xda, 0x20,
    0xc6, 0xd0, 0xb5, 0xe5, 0x20, 0xc5, 0xb0, 0xb8, 0xa6, 0x20, 0xb4, 0xa9, 0xb8, 0xa3, 0xbc,
    0xbc, 0xbf, 0xe4, 0x00,
];
/// The question, four `;`-separated runs — see [`crate::screen`] for why a
/// popup body is cut into runs the guest's wrap can hold; the first sentence
/// is 29 bytes, two over what one line holds, and the guest broke it before
/// the `?` (`out/shot/padmap-ask.png`, first attempt).
///
/// `패드키 설정을;진행하시겠습니까?;아니오를 누르면;기본 상태로 진행됩니다.`
pub const ASK_MSG: &[u8] = &[
    0xc6, 0xd0, 0xb5, 0xe5, 0xc5, 0xb0, 0x20, 0xbc, 0xb3, 0xc1, 0xa4, 0xc0, 0xbb, 0x3b, 0xc1,
    0xf8, 0xc7, 0xe0, 0xc7, 0xcf, 0xbd, 0xc3, 0xb0, 0xda, 0xbd, 0xc0, 0xb4, 0xcf, 0xb1, 0xee,
    0x3f, 0x3b, 0xbe, 0xc6, 0xb4, 0xcf, 0xbf, 0xc0, 0xb8, 0xa6, 0x20, 0xb4, 0xa9, 0xb8, 0xa3,
    0xb8, 0xe9, 0x3b, 0xb1, 0xe2, 0xba, 0xbb, 0x20, 0xbb, 0xf3, 0xc5, 0xc2, 0xb7, 0xce, 0x20,
    0xc1, 0xf8, 0xc7, 0xe0, 0xb5, 0xcb, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];

/// The six HUD action buttons in the guest's own order (`BUTTON_KEYS`), as
/// `(action, label)`.
fn hud_actions() -> [(PadAction, &'static [u8]); 6] {
    [
        (PadAction::Key(KEY_SLOT[0]), L_KEYS[0]),
        (PadAction::Key(KEY_SLOT[1]), L_KEYS[1]),
        (PadAction::Key(KEY_COMBO), L_KEYS[2]),
        (PadAction::Key(KEY_SLOT[2]), L_KEYS[3]),
        (PadAction::Key(KEY_SLOT[3]), L_KEYS[4]),
        (PadAction::Key(KEY_SLOT[4]), L_KEYS[5]),
    ]
}

/// The landscape HUD, as `StateGame::SetTouchKeypad` lays it out for a
/// canvas wider than 240 (0xE12E4..): six buttons at `(W - xs[i], H - 6 -
/// ys[i])`, the page flip at `(W - 0xbf, H - 0x2a)`, OK at `(W - 0x3d,
/// H - 0x46)` 60x60. The two tables are the second half of the 24 halfwords
/// at 0x159C38 (`tools_re`: `[0x18..0x24)` x, `[0x24..0x30)` y).
const WIDE_XS: [i32; 6] = [0xa0, 0x80, 0x60, 0x41, 0x2e, 0x24];
const WIDE_YS: [i32; 6] = [0x2a, 0x37, 0x4c, 0x65, 0x86, 0xaa];

/// The field HUD of a landscape canvas, computed the way the guest computes
/// it — for the title screen, where no `StateGame` exists to read it from.
pub fn computed_hud(w: i32, h: i32) -> TouchUi {
    let mut buttons = [(0, 0); 6];
    for (i, b) in buttons.iter_mut().enumerate() {
        *b = (w - WIDE_XS[i], h - 6 - WIDE_YS[i]);
    }
    TouchUi {
        buttons,
        page: (w - 0xbf, h - 0x2a),
        ok: (w - 0x3d, h - 0x46, 0x3c, 0x3c),
        dash: Some(crate::gamepad::dash_rect(w, h)),
    }
}

/// The `map shop save` bar's three cells, as `DrawTouchUi` draws them for a
/// canvas wider than 320 (0xE0CBC: `DrawInfoRect(x, 0, 0x1e, 0xd)` at `x =
/// 0x93 + 0x32 k`) and as `hud.rs` moves them by `dx` on a wide canvas.
pub fn bar_cells(dx: i32) -> [(i32, i32, i32, i32); 3] {
    let mut out = [(0, 0, 0, 0); 3];
    for (k, c) in out.iter_mut().enumerate() {
        *c = (0x93 + 0x32 * k as i32 + dx, 0, 0x1e, 0x15);
    }
    out
}
const BAR_ACTIONS: [(PadAction, &[u8]); 3] =
    [(PadAction::Key(KEY_MAP), L_MAP), (PadAction::Key(KEY_SHOP), L_SHOP), (PadAction::Key(KEY_SAVE), L_SAVE)];

/// Every control on a `w` x `h` canvas.
///
/// `hud` is the field HUD to put the controls on — read from the live
/// `StateGame` in game, computed on the title screen. Every field control
/// is [`Shape::Hud`]: the art under it is the guest's own in both scenes
/// (see [`draw_real_hud`]). `dx` is the bar shift from `hud.rs`.
pub fn layout(w: i32, h: i32, portrait: bool, hud: &TouchUi, dx: i32) -> Vec<Control> {
    let mut out = Vec::with_capacity(20);
    let shape = Shape::Hud;
    let acts = hud_actions();
    if portrait {
        // Where the relaid HUD draws the belt (one row) and the page flip.
        let belt = crate::portrait::belt_row(w, h);
        for (i, (a, l)) in acts.iter().enumerate() {
            let (x, y) = belt[i];
            out.push(Control { rect: crate::gamepad::drawn_button_rect(x, y), what: What::Act(*a), label: l, shape });
        }
        let p = crate::portrait::PAGE_RECT;
        out.push(Control { rect: p, what: What::Act(PadAction::Key(KEY_PAGE)), label: L_PAGE, shape });
    } else {
        for (i, (a, l)) in acts.iter().enumerate() {
            let (x, y) = hud.buttons[i];
            // Where the slot is drawn, not where it is touched — see
            // `gamepad::BELT_DRAW_DY`; the outline and the badge sit on the
            // picture.
            out.push(Control {
                rect: crate::gamepad::drawn_button_rect(x, y),
                what: What::Act(*a),
                label: l,
                shape,
            });
        }
        out.push(Control {
            rect: (hud.page.0, hud.page.1, BUTTON, BUTTON),
            what: What::Act(PadAction::Key(KEY_PAGE)),
            label: L_PAGE,
            shape,
        });
        out.push(Control { rect: hud.ok, what: What::Act(PadAction::Key(KEY_OK)), label: L_OK, shape });
        // The dash button is part of every landscape HUD (ANALYSIS.md §68.2:
        // `gv[0x1971]` gates the bar, not the dash).
        let d = hud.dash.unwrap_or_else(|| crate::gamepad::dash_rect(w, h));
        out.push(Control { rect: d, what: What::Act(PadAction::Dash), label: L_DASH, shape });
        if w > 320 {
            for (cell, (a, l)) in bar_cells(dx).iter().zip(BAR_ACTIONS) {
                out.push(Control { rect: *cell, what: What::Act(a), label: l, shape });
            }
        }
    }
    // The grid: what has no on-screen counterpart, then the screen's own two
    // buttons — 초기화 / 완료 kept a pair on the last row.
    let on_screen = |a: PadAction| out.iter().any(|c| c.what == What::Act(a));
    let mut grid: Vec<(What, &'static [u8])> = Vec::with_capacity(8);
    for (a, l) in [
        (PadAction::Key(KEY_OK), L_OK),
        (PadAction::Key(KEY_CLEAR), L_CLEAR),
        (PadAction::Dash, L_DASH),
        (PadAction::Key(KEY_MAP), L_MAP),
        (PadAction::Key(KEY_SHOP), L_SHOP),
        (PadAction::Key(KEY_SAVE), L_SAVE),
    ] {
        if !on_screen(a) {
            grid.push((What::Act(a), l));
        }
    }
    if grid.len() % 2 == 1 {
        grid.push((What::Reset, L_RESET));
        grid.push((What::Done, L_DONE));
    } else {
        grid.push((What::Reset, L_RESET));
        grid.push((What::Done, L_DONE));
    }
    let (ox, oy, cw, ch, px, py) = if portrait { (24, 56, 90, 20, 102, 36) } else { (20, 46, 60, 16, 68, 24) };
    // With an odd count the pair would split across rows; leave the last
    // cell of the row before it empty instead.
    let mut i = 0;
    for (what, l) in grid {
        if matches!(what, What::Reset) && i % 2 == 1 {
            i += 1;
        }
        let (col, row) = ((i % 2) as i32, (i / 2) as i32);
        out.push(Control { rect: (ox + col * px, oy + row * py, cw, ch), what, label: l, shape: Shape::Box });
        i += 1;
    }
    out
}

/// Per-`Vm` state.
#[derive(Default)]
pub struct PadMap {
    /// The mapping screen is up.
    pub active: bool,
    /// Which menu asked, and therefore which menu the screen is drawn over.
    pub scene: Scene,
    /// The custom bindings are in force (the row reads 커스텀).
    pub custom: bool,
    /// The question has been asked and not yet answered.
    pub pending: bool,
    /// Which box the player tapped last, waiting for a pad button.
    selected: Option<usize>,
    /// A finger that went down on the screen is still down; its move and
    /// release are the screen's too, not the guest's.
    swallow: bool,
    pub opened: u64,
    pub assigned: u64,
    /// A `GMenu` of the host's own, for the title screen: the belt frames and
    /// the page flip are `GMenu` images, and the game's `GMenu` is made by
    /// `Battle::Init` — there is none until the field. Built once, on the
    /// first title-screen open, from the guest's own constructor, and kept.
    gmenu: Option<u32>,
}

impl PadMap {
    /// The title screen's `GMenu`, once one has been made.
    pub(crate) fn gmenu(&self) -> Option<u32> {
        self.gmenu
    }

    /// What the row reads: 커스텀 while the bindings are in force, and also
    /// while the question is up — the player moved the value there and the
    /// popup is about to say whether it stays.
    pub fn shows_custom(&self) -> bool {
        self.custom || self.pending
    }
}

/// Read the mode at boot. A `hl5_gamepad.txt` with no mode file beside it is
/// the layout every player who hand-wrote one before this row existed, so it
/// counts as 커스텀; the mode file then says so from here on.
pub fn install(vm: &mut Vm) -> Vec<String> {
    let mut notes = Vec::new();
    let mode = std::fs::read_to_string(vm.cfg.storage.join(MODE_FILE))
        .map(|s| s.trim().to_ascii_lowercase())
        .ok();
    let custom = match mode.as_deref() {
        Some("custom") => true,
        Some("default") => false,
        _ => !vm.pad.overrides.is_empty(),
    };
    vm.padmap.custom = custom;
    if !custom && !vm.pad.overrides.is_empty() {
        notes.push(format!(
            "padmap: {} binding(s) in {MAP_FILE} not in force — {MODE_FILE} says default",
            vm.pad.overrides.len()
        ));
        vm.pad.overrides.clear();
    }
    if custom {
        custom_baseline(&mut vm.pad.overrides);
    }
    notes.push(format!("padmap: {}", if custom { "커스텀 (custom)" } else { "기본 (default)" }));
    notes
}

fn write_mode(vm: &mut Vm, custom: bool) {
    let path = vm.cfg.storage.join(MODE_FILE);
    if let Err(e) = std::fs::write(&path, if custom { "custom\n" } else { "default\n" }) {
        vm.log.push(format!("padmap: cannot write {}: {e}", path.display()));
    }
}

/// The row moved to 기본: the built-in layout, the file kept for later.
pub fn set_default(vm: &mut Vm) {
    vm.padmap.custom = false;
    vm.padmap.pending = false;
    vm.pad.overrides.clear();
    write_mode(vm, false);
    vm.log.push("padmap: 기본 — built-in layout".to_string());
    vm.gl.forget_last_draw();
}

/// The title-screen row moved to 커스텀: ask, in `StateMainMenu`'s popup.
pub fn ask(vm: &mut Vm, this: u32) -> Result<(), String> {
    vm.padmap.scene = Scene::Title;
    vm.padmap.pending = true;
    if !options::ask_two_buttons(vm, this, ASK_MSG)? {
        // No popup could be raised (guest heap): nothing changes.
        vm.padmap.pending = false;
    }
    Ok(())
}

/// The in-game row moved to 커스텀: ask, in `StateInGameMenu`'s popup.
pub fn ask_ingame(vm: &mut Vm, this: u32) -> Result<(), String> {
    vm.padmap.scene = Scene::InGame;
    vm.padmap.pending = true;
    if !crate::igoptions::ask_two_buttons(vm, this, ASK_MSG)? {
        vm.padmap.pending = false;
    }
    Ok(())
}

/// The popup was answered.
pub fn answered(vm: &mut Vm, yes: bool) -> Result<(), String> {
    vm.padmap.pending = false;
    if yes {
        let scene = vm.padmap.scene;
        open(vm, scene);
    } else {
        vm.log.push("padmap: 아니오 — stays 기본".to_string());
    }
    Ok(())
}

/// Open the mapping screen over the menu, with the saved bindings if there
/// are any.
pub fn open(vm: &mut Vm, scene: Scene) {
    if let Ok(t) = std::fs::read_to_string(vm.cfg.storage.join(MAP_FILE)) {
        let (map, _) = crate::gamepad::parse_map_reporting(&t);
        vm.pad.overrides = map;
    }
    custom_baseline(&mut vm.pad.overrides);
    vm.pad.release_all();
    vm.padmap.scene = scene;
    vm.padmap.active = true;
    vm.padmap.selected = None;
    vm.padmap.swallow = false;
    vm.padmap.opened += 1;
    let n = controls(vm).len();
    vm.log.push(format!(
        "padmap: mapping screen open ({scene:?}), {} binding(s), {n} control(s)",
        vm.pad.overrides.len()
    ));
    vm.gl.forget_last_draw();
}

/// The `hl5_gamepad.txt` token for an action — the inverse of
/// [`crate::gamepad::parse_map_line`].
pub fn token_of(action: Option<PadAction>) -> String {
    match action {
        None => "NONE".to_string(),
        Some(PadAction::Dash) => "DASH".to_string(),
        Some(PadAction::Key(k)) => match k {
            crate::gamepad::KEY_UP => "UP".to_string(),
            crate::gamepad::KEY_DOWN => "DOWN".to_string(),
            crate::gamepad::KEY_LEFT => "LEFT".to_string(),
            crate::gamepad::KEY_RIGHT => "RIGHT".to_string(),
            KEY_OK => "OK".to_string(),
            KEY_CLEAR => "CLR".to_string(),
            KEY_MAP => "MAP".to_string(),
            KEY_SHOP => "SHOP".to_string(),
            KEY_SAVE => "SAVE".to_string(),
            KEY_PAGE => "PAGE".to_string(),
            c @ 0x30..=0x39 | c @ 0x2a => char::from(c as u8).to_string(),
            other => format!("{other}"),
        },
    }
}

/// The whole table as the file spells it: one line per code the table
/// decides, `NONE` for an unbound one, so the built-in default for a button
/// the player unbound cannot come back on the next boot.
pub fn file_text(overrides: &HashMap<i32, Option<PadAction>>) -> String {
    let mut s = String::from("# written by the 패드키 row; <android keycode>=<token>\n");
    for code in table_codes() {
        s.push_str(&format!("{code}={}\n", token_of(button_action_in(code, overrides))));
    }
    s
}

/// What a custom table starts from: the hat centre unbound (see
/// [`HAT_CENTRE`]). A file that names it keeps its say.
pub fn custom_baseline(overrides: &mut HashMap<i32, Option<PadAction>>) {
    overrides.entry(HAT_CENTRE).or_insert(None);
}

/// Save and close.
pub fn close(vm: &mut Vm) {
    let text = file_text(&vm.pad.overrides);
    let path = vm.cfg.storage.join(MAP_FILE);
    match std::fs::write(&path, text) {
        Ok(()) => vm.log.push(format!("padmap: {} line(s) written to {}", table_codes().count(), path.display())),
        Err(e) => vm.log.push(format!("padmap: cannot write {}: {e}", path.display())),
    }
    vm.padmap.custom = true;
    write_mode(vm, true);
    vm.padmap.active = false;
    vm.padmap.selected = None;
    vm.padmap.swallow = false;
    vm.gl.forget_last_draw();
}

/// Bind `code` to `action`: the button loses what it fired before, and the
/// control loses every other button that fired it.
///
/// And the control the button *used* to fire is left **unbound**, not handed
/// to whatever else fired it by default. The built-in layout has two such
/// shadows — the hat centre also fires OK, the gamepad-source BACK also fires
/// 취소 — and without this step moving A off 확인 left the hat centre on it,
/// which the badge pass drew as a d-pad glyph that looked assigned by itself
/// (the first report against this screen). Only *defaults* are swept: a code
/// the player put on that control themselves (present in `overrides`) stays.
pub fn assign(overrides: &mut HashMap<i32, Option<PadAction>>, code: i32, action: PadAction) {
    let old = button_action_in(code, overrides);
    for other in table_codes() {
        if other != code && button_action_in(other, overrides) == Some(action) {
            overrides.insert(other, None);
        }
    }
    overrides.insert(code, Some(action));
    if let Some(old) = old {
        if old != action {
            for other in table_codes() {
                if !overrides.contains_key(&other) && button_action_in(other, overrides) == Some(old) {
                    overrides.insert(other, None);
                }
            }
        }
    }
}

/// The controls as laid out for the screen that is open.
fn controls(vm: &Vm) -> Vec<Control> {
    let (w, h) = (vm.cfg.wipi_w, vm.cfg.wipi_h);
    let portrait = vm.cfg.screen_mode.is_portrait();
    let mut c = match vm.padmap.scene {
        Scene::Title => layout(w, h, portrait, &computed_hud(w, h), vm.hud.dx),
        // The HUD as the guest last drew it; a menu opened before the field
        // was ever drawn falls back to the computed one.
        Scene::InGame => {
            let hud = vm.pad.ui.filter(|u| u.ok.2 > 0).unwrap_or_else(|| computed_hud(w, h));
            layout(w, h, portrait, &hud, vm.hud.dx)
        }
    };
    // Where the 배치변경 layout in force puts each field control — the art
    // is drawn there too (`hudlayout::draw_hud`).
    if !portrait {
        for ctl in c.iter_mut().filter(|c| c.shape == Shape::Hud) {
            if let What::Act(a) = ctl.what {
                if let Some(k) = hud_ctl(a) {
                    ctl.rect = crate::hudlayout::place_rect(vm, k, ctl.rect);
                }
            }
        }
    }
    c
}

/// The 배치변경 control a field action lives on.
fn hud_ctl(a: PadAction) -> Option<crate::hudlayout::Ctl> {
    use crate::hudlayout::Ctl;
    Some(match a {
        PadAction::Dash => Ctl::Dash,
        PadAction::Key(KEY_OK) => Ctl::Ok,
        PadAction::Key(KEY_PAGE) => Ctl::Page,
        PadAction::Key(KEY_MAP) | PadAction::Key(KEY_SHOP) | PadAction::Key(KEY_SAVE) => Ctl::Bar,
        PadAction::Key(k) => Ctl::Slot(crate::gamepad::BUTTON_KEYS.iter().position(|b| *b == k)? as u8),
    })
}

/// A pad button while the screen is up. Returns whether it was consumed
/// (always, while the screen is up).
pub fn pad_button(vm: &mut Vm, code: i32, down: bool) -> bool {
    if !vm.padmap.active {
        return false;
    }
    if !down {
        return true;
    }
    if crate::portrait::is_keypad(code) {
        // The 세로 keypad is not remappable; its CLR leaves, the rest wait.
        if crate::portrait::keypad_action(code) == Some(Some(PadAction::Key(KEY_CLEAR))) {
            back(vm);
        }
        return true;
    }
    match vm.padmap.selected {
        Some(i) if ASSIGNABLE.contains(&code) => {
            let c = controls(vm);
            if let Some(What::Act(a)) = c.get(i).map(|c| c.what) {
                assign(&mut vm.pad.overrides, code, a);
                vm.padmap.assigned += 1;
                vm.log.push(format!("padmap: keycode {code} -> {}", token_of(Some(a))));
            }
            vm.padmap.selected = None;
            vm.gl.forget_last_draw();
        }
        Some(_) => {}
        None => {
            if code == KEYCODE_BUTTON_B || code == KEYCODE_BACK {
                close(vm);
            }
        }
    }
    true
}

/// BACK: drop the selection, or leave.
fn back(vm: &mut Vm) {
    if vm.padmap.selected.take().is_some() {
        vm.gl.forget_last_draw();
    } else {
        close(vm);
    }
}

/// A guest key while the screen is up (already resolved to a navigation
/// code by the menu's key hook).
pub fn key(vm: &mut Vm, navi: i32) -> Result<(), String> {
    if navi == K_BACK {
        back(vm);
    }
    Ok(())
}

fn hit(c: &[Control], x: i32, y: i32) -> Option<usize> {
    c.iter().position(|c| {
        let (rx, ry, rw, rh) = c.rect;
        x >= rx && x < rx + rw && y >= ry && y < ry + rh
    })
}

/// A real finger while the screen is up. Returns whether the event was the
/// screen's; a release for a press the guest saw is still the guest's.
pub fn touch(vm: &mut Vm, kind: Touch, x: i32, y: i32) -> bool {
    if !vm.padmap.active {
        return false;
    }
    match kind {
        Touch::Pressed => {
            vm.padmap.swallow = true;
            let c = controls(vm);
            match hit(&c, x, y) {
                Some(i) => match c[i].what {
                    What::Act(_) => {
                        vm.padmap.selected = if vm.padmap.selected == Some(i) { None } else { Some(i) };
                    }
                    What::Reset => {
                        vm.pad.overrides.clear();
                        custom_baseline(&mut vm.pad.overrides);
                        vm.padmap.selected = None;
                        vm.log.push("padmap: 초기화 — built-in layout".to_string());
                    }
                    What::Done => close(vm),
                },
                None => vm.padmap.selected = None,
            }
            vm.gl.forget_last_draw();
            true
        }
        Touch::Moved => vm.padmap.swallow,
        Touch::Released => std::mem::take(&mut vm.padmap.swallow),
    }
}

// ---- drawing ----------------------------------------------------------------

/// `Graphic::DrawImageBlending(fb, x, y, GbmImage*, uchar, uchar, uchar, int)`
/// — the call `StateGame::DrawTouchUi` draws its plates with. The three bytes
/// and the int are copied from its call sites verbatim; the second byte is
/// the frame (0 idle, 4 pressed on the OK disc).
const DRAW_IMAGE_BLENDING: &str = "_ZN7Graphic17DrawImageBlendingEiiiP8GbmImagehhhi";
/// `GMenu::DrawSkillBelt(fb, x, y, uchar, signed char, short)` — the belt,
/// drawn by `Battle::DrawGameUI+0xa98` and `StateInGameMenu::Paint+0xb4`
/// alike at `(halfW - 0x67, H - 0x2c)` with `(0, -1, -1)`.
const DRAW_SKILL_BELT: &str = "_ZN5GMenu13DrawSkillBeltEiiihas";
/// The `GMenu*` at `gv + 0x15F8` (0x10A344).
const GV_GMENU: u32 = 0x15f8;
/// `Graphic::CopyFrameBuffer(fb, src)` + `Graphic::GetFBOff()` — how the
/// in-game menu's `Paint` starts: the blurred field copied back as the
/// backdrop (0x10A32C).
const COPY_FRAME_BUFFER: &str = "_ZN7Graphic15CopyFrameBufferEii";
const GET_FB_OFF: &str = "_ZN7Graphic8GetFBOffEv";
/// `CommonUi::DrawInfoRect(fb, x, y, w, h)` and `TextMgr::GetCommonText(int)`
/// for the bar; `_pTextMgr` holds the `TextMgr*`.
const DRAW_INFO_RECT: &str = "_ZN8CommonUi12DrawInfoRectEiiiii";
const GET_COMMON_TEXT: &str = "_ZN7TextMgr13GetCommonTextEi";
const P_TEXT_MGR: &str = "_pTextMgr";
/// `common_text` 0x112..0x114 = `map` `shop` `save`.
const BAR_TEXT: i32 = 0x112;
/// The HUD images inside `gv`, as `DrawTouchUi` addresses them: the OK disc
/// idle (`+0x17c`), the dash button idle (`+0x1f4`), the joystick plate
/// (`+0x23c`) and its handle (`+0x254`).
const GV_IMG_OK: u32 = 0x17c;
/// …and the glyph each wears on top (`+0x194` frame 4 on OK, `+0x20c` frame 4
/// on dash — the second `DrawImageBlending` of each pair in `DrawTouchUi`).
const GV_IMG_OK_GLYPH: u32 = 0x194;
const GV_IMG_DASH: u32 = 0x1f4;
const GV_IMG_DASH_GLYPH: u32 = 0x20c;
/// `CommonUi::DrawTouchCancelUi()` — the 취소 button, a half disc right-
/// aligned on the top-right edge at y 30 (anchor 0x21; `mainKeyPressed`
/// tests `x >= W-0x24, 30 <= y <= 60` for it). The `DASH` lettering is the
/// dash glyph's frame 4, not this (ANALYSIS.md §68.2).
const DRAW_TOUCH_CANCEL_UI: &str = "_ZN8CommonUi17DrawTouchCancelUiEv";
const GV_IMG_JOY: u32 = 0x23c;
const GV_IMG_HANDLE: u32 = 0x254;

/// Draw the screen with the guest's own primitives, in the menu's place.
pub fn paint(vm: &mut Vm, _this: u32) -> Result<(), String> {
    let Some(c) = canvas(vm)? else { return Ok(()) };
    let (w, h) = (vm.cfg.wipi_w, vm.cfg.wipi_h);
    let scene = vm.padmap.scene;
    let portrait = vm.cfg.screen_mode.is_portrait();
    if scene == Scene::InGame {
        // The menu's own backdrop: the blurred field.
        if let (Some(cp), Some(off)) = (vm.symbol(COPY_FRAME_BUFFER), vm.symbol(GET_FB_OFF)) {
            let src = vm.call(off, &[c.graphic])?;
            vm.call(cp, &[c.graphic, c.fb, src])?;
        }
    }
    // A dark sheet, so the controls read.
    if let Some(f) = vm.symbol(options::FILL_RECT_ALPHA) {
        vm.call(f, &[c.graphic, c.fb, 0, 0, w as u32, h as u32, 0xb4, 0])?;
    }
    draw_real_hud(vm, &c, w, h, portrait, scene)?;
    // The hint, under the `map shop save` bar that owns the top edge (in game
    // the guest's, on the title screen three boxes at the same place). No
    // title box: it sat exactly over that bar.
    label(vm, &c, c.half_w, 26, HINT, 1, FG)?;
    let controls = controls(vm);
    for (i, ctl) in controls.iter().enumerate() {
        if ctl.shape == Shape::Box {
            draw_box(vm, &c, ctl, vm.padmap.selected == Some(i))?;
        }
    }
    Ok(())
}

/// `GMenu::GMenu()` and the size `Battle::Init` allocates for it (0x494E4:
/// `0x460 + 8`). The constructor loads the 47 `GMenu` images from their
/// merged archive — belt frames, page flip, dialog chrome.
const GMENU_CTOR: &str = "_ZN5GMenuC1Ev";
const GMENU_SIZE: u32 = 0x468;
/// `Graphic::DrawEffect(fb, x, y, GbmImage*, uchar, int, int, int, uchar,
/// uchar, int)` — what `DrawSkillBelt` draws the slot frames with.
const DRAW_EFFECT: &str = "_ZN7Graphic10DrawEffectEiiiP8GbmImagehiiihhi";
/// The `GMenu` images the belt frames are made of, as `DrawSkillBelt`
/// addresses them (0x837E0..): the slot plate (`+0x210`), the skill-slot
/// overlay (`+0x1e0`), the combo-slot overlay (`+0x1f8`), the page flip
/// (`+0x228`).
const GMENU_IMG_PLATE: u32 = 0x210;
const GMENU_IMG_SKILL: u32 = 0x1e0;
const GMENU_IMG_COMBO: u32 = 0x1f8;
const GMENU_IMG_PAGE: u32 = 0x228;

/// The title screen's `GMenu`, made on first use.
pub(crate) fn title_gmenu(vm: &mut Vm) -> Option<u32> {
    if let Some(g) = vm.padmap.gmenu {
        return Some(g);
    }
    let ctor = vm.symbol(GMENU_CTOR)?;
    let obj = vm.heap.alloc(GMENU_SIZE)?;
    vm.mem.write_bytes(obj, &vec![0u8; GMENU_SIZE as usize]);
    if let Err(e) = vm.call(ctor, &[obj]) {
        vm.log.push(format!("padmap: GMenu constructor failed: {e}"));
        return None;
    }
    vm.padmap.gmenu = Some(obj);
    vm.log.push("padmap: title-screen GMenu built for the belt frames".to_string());
    Some(obj)
}

/// The six empty slot frames and the page flip, exactly as `DrawSkillBelt`
/// draws them before it asks the hero what is in the slots (0x83854..0x83958
/// — the same code for both layouts, fed the layout's own positions).
pub(crate) fn draw_belt_frames(
    vm: &mut Vm,
    c: &Canvas,
    gmenu: u32,
    slots: &[(i32, i32); 6],
    page: (i32, i32),
) -> Result<(), String> {
    let Some(f) = vm.symbol(DRAW_EFFECT) else { return Ok(()) };
    for (k, &(x, y)) in slots.iter().enumerate() {
        vm.call(f, &[c.graphic, c.fb, x as u32, (y + 6) as u32, gmenu + GMENU_IMG_PLATE, 0x10, 0, 0x64, 0x64, 3, 0xff, 9])?;
        let overlay = if k == 2 { GMENU_IMG_COMBO } else { GMENU_IMG_SKILL };
        vm.call(f, &[c.graphic, c.fb, (x + 2) as u32, (y + 8) as u32, gmenu + overlay, 0x10, 0, 0x64, 0x64, 0, 0xff, 9])?;
    }
    vm.call(f, &[c.graphic, c.fb, page.0 as u32, page.1 as u32, gmenu + GMENU_IMG_PAGE, 0x10, 0, 0x64, 0x64, 0, 0xff, 0x12])?;
    Ok(())
}

/// The field HUD, with the guest's own art and the guest's own calls.
fn draw_real_hud(vm: &mut Vm, c: &Canvas, w: i32, h: i32, portrait: bool, scene: Scene) -> Result<(), String> {
    // With 배치변경 in this build the HUD is drawn where its layout in force
    // puts it, by the code the editor and the field use.
    if vm.hudlayout.installed && !portrait {
        let l = crate::hudlayout::shown_layout(vm);
        return crate::hudlayout::draw_hud(vm, c, scene, &l);
    }
    let Some(gv) = vm.symbol(options::GV) else { return Ok(()) };
    match scene {
        // The belt with what is in it:
        // `GMenu::DrawSkillBelt(gmenu, fb, halfW-0x67, H-0x2c, 0, -1, -1)`.
        Scene::InGame => {
            let gmenu = vm.mem.read_u32(gv + GV_GMENU);
            if gmenu != 0 {
                if let Some(f) = vm.symbol(DRAW_SKILL_BELT) {
                    vm.call(f, &[gmenu, c.fb, (c.half_w - 0x67) as u32, (h - 0x2c) as u32, 0, u32::MAX, u32::MAX])?;
                }
            }
        }
        // No hero and no `GMenu` on the title screen: the empty frames, from
        // a `GMenu` of our own, at the positions the game would use.
        Scene::Title => {
            if let Some(gmenu) = title_gmenu(vm) {
                let (slots, page) = if portrait {
                    // The page flip art centred on where the relaid HUD
                    // draws it.
                    let p = crate::portrait::PAGE_RECT;
                    (crate::portrait::belt_row(w, h), (p.0 + p.2 / 2, p.1 + p.3 / 2))
                } else {
                    (computed_hud(w, h).buttons, (w - 0xb0, h - 0x1b))
                };
                draw_belt_frames(vm, c, gmenu, &slots, page)?;
            }
        }
    }
    if portrait {
        return Ok(());
    }
    let Some(blend) = vm.symbol(DRAW_IMAGE_BLENDING) else { return Ok(()) };
    // Joystick plate and handle at rest (0xE0A74 / 0xE0AC4).
    let (cx, cy) = crate::gamepad::stick_centre(h);
    vm.call(blend, &[c.graphic, c.fb, cx as u32, cy as u32, gv + GV_IMG_JOY, 0x10, 0, 0x58, 0x12])?;
    vm.call(blend, &[c.graphic, c.fb, cx as u32, cy as u32, gv + GV_IMG_HANDLE, 0x10, 0, 0xff, 0x12])?;
    // The OK disc and its glyph, idle (0xE06F4, 0xE075C).
    vm.call(blend, &[c.graphic, c.fb, (w - 0x1f) as u32, (h - 0x28) as u32, gv + GV_IMG_OK, 0x10, 0, 0x5a, 0x12])?;
    vm.call(blend, &[c.graphic, c.fb, (w - 0x1f) as u32, (h - 0x28) as u32, gv + GV_IMG_OK_GLYPH, 0x10, 4, 0xff, 0x12])?;
    // The dash button at the right edge (0xE07EC), part of every landscape
    // HUD.
    vm.call(blend, &[c.graphic, c.fb, w as u32, (h - 0x56) as u32, gv + GV_IMG_DASH, 0x10, 0, 0x5a, 0x22])?;
    vm.call(blend, &[c.graphic, c.fb, w as u32, (h - 0x56) as u32, gv + GV_IMG_DASH_GLYPH, 0x10, 4, 0xff, 0x22])?;
    if c.common_ui != 0 {
        if let Some(f) = vm.symbol(DRAW_TOUCH_CANCEL_UI) {
            vm.call(f, &[c.common_ui])?;
        }
    }
    // The bar (0xE0CBC..): three plates and `map shop save`, shifted as the
    // field shifts them.
    if w > 320 && c.common_ui != 0 {
        let (Some(rect), Some(gct), Some(tm)) = (vm.symbol(DRAW_INFO_RECT), vm.symbol(GET_COMMON_TEXT), deref(vm, P_TEXT_MGR))
        else {
            return Ok(());
        };
        for (k, cell) in bar_cells(vm.hud.dx).iter().enumerate() {
            vm.call(rect, &[c.common_ui, c.fb, cell.0 as u32, 0, 0x1e, 0xd])?;
            let s = vm.call(gct, &[tm, (BAR_TEXT + k as i32) as u32])?;
            if s != 0 {
                if let Some(dt) = vm.symbol(options::DRAW_TEXT) {
                    vm.call(dt, &[c.font, c.fb, (cell.0 + 0xf) as u32, 0, s, 1, FG])?;
                }
            }
        }
    }
    Ok(())
}

fn draw_box(vm: &mut Vm, c: &Canvas, ctl: &Control, selected: bool) -> Result<(), String> {
    let (x, y, w, h) = ctl.rect;
    if selected {
        // The cursor plate of the option list.
        plate(vm, c, x, y, w, h, true)?;
    } else if let Some(f) = vm.symbol(options::FILL_ROUND_RECT) {
        // The black value box of the option list. `DrawHalfBox`'s plain
        // style is a translucent light wash that reads dark only over the
        // list's own panel; over the sheet here it came out white-on-white.
        vm.call(f, &[c.graphic, c.fb, x as u32, y as u32, w as u32, h as u32, 4, 0xff, 0])?;
    }
    // The guest's own rows put the text three rows below the plate's top on
    // a 17 px plate; centre it the same way on a plate of any height. On a
    // 30 px box the badge in the lower-left corner would eat the first
    // letter, so the label moves right of it.
    let ty = y + (h - 11) / 2;
    let lx = x + w / 2 + if w <= BUTTON { 5 } else { 0 };
    label(vm, c, lx, ty, ctl.label, 1, FG)
}

/// The badges, onto the host's finished canvas — every control that a pad
/// button fires, with that button's position — and the outline on the
/// selected control (a box gets the cursor plate instead, from the guest).
pub fn paint_badges(vm: &mut Vm) -> u32 {
    if !vm.padmap.active {
        return 0;
    }
    let (w, h) = (vm.gl.fb_w, vm.gl.fb_h);
    let mut n = 0;
    for (i, ctl) in controls(vm).into_iter().enumerate() {
        if ctl.shape == Shape::Hud && vm.padmap.selected == Some(i) {
            crate::padhud::outline(&mut vm.gl.fb, w, h, ctl.rect);
        }
        if let What::Act(a) = ctl.what {
            if let Some(pos) = pos_for_action(a, &vm.pad.overrides) {
                crate::padhud::draw_badge(&mut vm.gl.fb, w, h, ctl.rect, pos);
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamepad::{default_button_action, parse_map};

    fn rects_disjoint(c: &[Control]) {
        for (i, a) in c.iter().enumerate() {
            for b in &c[i + 1..] {
                let (ax, ay, aw, ah) = a.rect;
                let (bx, by, bw, bh) = b.rect;
                let apart = ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay;
                assert!(apart, "{:?} overlaps {:?}", a, b);
            }
        }
    }

    fn all_layouts() -> Vec<(i32, i32, Vec<Control>)> {
        let mut out = Vec::new();
        for (w, h, p) in [(585, 270, false), (526, 270, false), (468, 270, false), (400, 240, false), (240, 320, true)] {
            for real in [false, true] {
                let _ = real;
                let hud = computed_hud(w, h);
                out.push((w, h, layout(w, h, p, &hud, (w - 400).max(0) / 2)));
            }
        }
        out
    }

    #[test]
    fn every_layout_has_every_control_once_and_nothing_overlaps() {
        for (w, h, c) in all_layouts() {
            let acts: Vec<PadAction> = c.iter().filter_map(|c| if let What::Act(a) = c.what { Some(a) } else { None }).collect();
            for want in [
                PadAction::Key(KEY_OK),
                PadAction::Key(KEY_CLEAR),
                PadAction::Key(KEY_PAGE),
                PadAction::Key(KEY_COMBO),
                PadAction::Key(KEY_MAP),
                PadAction::Key(KEY_SHOP),
                PadAction::Key(KEY_SAVE),
                PadAction::Dash,
            ] {
                assert_eq!(acts.iter().filter(|a| **a == want).count(), 1, "{w}x{h}: {want:?}");
            }
            for k in KEY_SLOT {
                assert_eq!(acts.iter().filter(|a| **a == PadAction::Key(k)).count(), 1, "{w}x{h}: slot {k:#x}");
            }
            assert_eq!(c.iter().filter(|c| c.what == What::Reset).count(), 1);
            assert_eq!(c.iter().filter(|c| c.what == What::Done).count(), 1);
            for ctl in &c {
                let (x, y, cw, ch) = ctl.rect;
                assert!(x >= 0 && y >= 0 && x + cw <= w && y + ch <= h, "{w}x{h}: {ctl:?} off canvas");
                assert_eq!(ctl.label.last(), Some(&0));
            }
            rects_disjoint(&c);
        }
    }

    /// In landscape every field control is on the field — including the dash
    /// button and the bar — so the grid holds only 취소 and the two buttons;
    /// in 세로 OK and dash have no drawn control and join the grid.
    #[test]
    fn the_grid_holds_only_what_the_field_does_not_draw() {
        let c = layout(585, 270, false, &computed_hud(585, 270), 92);
        // …and the dash button is there even when the HUD read says it is
        // not (the flag is clear while the menu is up).
        let mut no_dash = computed_hud(585, 270);
        no_dash.dash = None;
        assert!(layout(585, 270, false, &no_dash, 92).iter().any(|c| c.what == What::Act(PadAction::Dash) && c.shape == Shape::Hud));
        let boxes: Vec<&Control> = c.iter().filter(|c| c.shape == Shape::Box).collect();
        assert_eq!(boxes.len(), 3, "취소, 초기화, 완료: {boxes:?}");
        let dash = c.iter().find(|c| c.what == What::Act(PadAction::Dash)).unwrap();
        assert_eq!(dash.shape, Shape::Hud);
        assert_eq!(dash.rect, (585 - 35, 270 - 101, 35, 30));
        let p = layout(240, 320, true, &computed_hud(240, 320), 0);
        assert!(p.iter().any(|c| c.what == What::Act(PadAction::Key(KEY_OK)) && c.shape == Shape::Box));
        assert!(p.iter().any(|c| c.what == What::Act(PadAction::Dash) && c.shape == Shape::Box));
    }

    #[test]
    fn the_landscape_hud_boxes_are_where_the_guest_puts_the_buttons() {
        // 585x270: the page flip at (394, 228) and OK at (524, 200) are what
        // the probe reads out of a live StateGame on that canvas.
        let hud = computed_hud(585, 270);
        assert_eq!(hud.page, (394, 228));
        assert_eq!(hud.ok, (524, 200, 60, 60));
        assert_eq!(hud.buttons[0], (425, 222));
        let c = layout(585, 270, false, &hud, 92);
        let slot1 = c.iter().find(|c| c.what == What::Act(PadAction::Key(KEY_SLOT[0]))).unwrap();
        assert_eq!(slot1.rect, (425, 228, 30, 30), "the drawn frame, six under the touch rect");
        let cells = bar_cells(92);
        assert_eq!(cells[0], (147 + 92, 0, 30, 21), "the map cell, as hud.rs moves it");
    }

    #[test]
    fn the_portrait_belt_boxes_are_the_drawn_belt() {
        let c = layout(240, 320, true, &computed_hud(240, 320), 0);
        let slot1 = c.iter().find(|c| c.what == What::Act(PadAction::Key(KEY_SLOT[0]))).unwrap();
        assert_eq!((slot1.rect.0, slot1.rect.1), (8, 280), "the first belt icon's plate, one row at the bottom");
    }

    #[test]
    fn assigning_moves_the_button_and_frees_the_old_one() {
        let mut ov = HashMap::new();
        // Y fired slot 2 by default; put it on slot 1 (X's).
        assign(&mut ov, KEYCODE_BUTTON_Y, PadAction::Key(KEY_SLOT[0]));
        assert_eq!(button_action_in(KEYCODE_BUTTON_Y, &ov), Some(PadAction::Key(KEY_SLOT[0])));
        assert_eq!(button_action_in(KEYCODE_BUTTON_X, &ov), None, "X lost slot 1");
        // Slot 2 is now nobody's — the file has to say so.
        assert!(ASSIGNABLE.iter().all(|c| button_action_in(*c, &ov) != Some(PadAction::Key(KEY_SLOT[1]))));
        // A and the hat centre both fire OK by default; moving OK to B frees A
        // and the hat centre with it.
        assign(&mut ov, KEYCODE_BUTTON_B, PadAction::Key(KEY_OK));
        assert_eq!(button_action_in(KEYCODE_BUTTON_A, &ov), None);
        assert_eq!(button_action_in(KEYCODE_BUTTON_B, &ov), Some(PadAction::Key(KEY_OK)));
        assert_eq!(button_action_in(HAT_CENTRE, &ov), None);
    }

    /// The report this guards: unbind 확인 from its only face button and it
    /// must have NO badge and NO button — not the hat centre's hidden default,
    /// which the badge pass would have drawn as a d-pad glyph "assigned on
    /// its own". Same for 취소 and the gamepad-source BACK.
    #[test]
    fn an_unbound_control_stays_unbound() {
        let mut ov = HashMap::new();
        custom_baseline(&mut ov);
        // A (OK by default) goes to slot 1: OK is now nobody's.
        assign(&mut ov, KEYCODE_BUTTON_A, PadAction::Key(KEY_SLOT[0]));
        assert_eq!(pos_for_action(PadAction::Key(KEY_OK), &ov), None, "no badge may appear on 확인");
        for code in crate::gamepad::KEYCODE_DPAD_UP..=crate::gamepad::KEYCODE_DPAD_CENTER {
            assert_ne!(button_action_in(code, &ov), Some(PadAction::Key(KEY_OK)), "code {code}");
        }
        // B (CLR by default) goes to slot 2: 취소 is nobody's, BACK included.
        assign(&mut ov, KEYCODE_BUTTON_B, PadAction::Key(KEY_SLOT[1]));
        assert_eq!(button_action_in(KEYCODE_BACK, &ov), None, "BACK is in the table");
        assert_eq!(pos_for_action(PadAction::Key(KEY_CLEAR), &ov), None);
        // The file carries both facts to the next boot.
        let back = parse_map(&file_text(&ov));
        assert_eq!(back.get(&HAT_CENTRE), Some(&None));
        assert_eq!(back.get(&KEYCODE_BACK), Some(&None));
    }

    #[test]
    fn the_file_round_trips_through_the_parser() {
        let mut ov = HashMap::new();
        assign(&mut ov, KEYCODE_BUTTON_START, PadAction::Key(KEY_SLOT[3]));
        assign(&mut ov, KEYCODE_BUTTON_L2, PadAction::Dash);
        let back = parse_map(&file_text(&ov));
        for code in table_codes() {
            assert_eq!(button_action_in(code, &back), button_action_in(code, &ov), "code {code}");
        }
        // A button the player left on its default is still spelled out, so a
        // later default change cannot silently rebind it.
        assert!(back.contains_key(&KEYCODE_BUTTON_A));
        assert_eq!(back[&KEYCODE_BUTTON_A], default_button_action(KEYCODE_BUTTON_A));
    }

    #[test]
    fn every_token_the_file_writes_is_one_the_parser_reads() {
        for code in ASSIGNABLE {
            let a = default_button_action(code);
            let line = format!("{code}={}", token_of(a));
            assert_eq!(crate::gamepad::parse_map_line(&line), Some((code, a)), "{line}");
        }
        assert_eq!(crate::gamepad::parse_map_line("96=NONE"), Some((96, None)));
    }

    #[test]
    fn the_question_is_four_short_runs_of_euc_kr() {
        assert_eq!(ASK_MSG.last(), Some(&0));
        let body = &ASK_MSG[..ASK_MSG.len() - 1];
        let runs: Vec<&[u8]> = body.split(|b| *b == b';').collect();
        assert_eq!(runs.len(), 4);
        for r in &runs {
            assert!(r.len() <= 27, "a run over the popup's width: {} bytes", r.len());
            let mut i = 0;
            while i < r.len() {
                if r[i] >= 0x80 {
                    assert!((0x41..=0xfe).contains(&r[i + 1]));
                    i += 2;
                } else {
                    i += 1;
                }
            }
        }
    }
}
