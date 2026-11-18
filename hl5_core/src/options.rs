//! 환경설정 — the guest's nine-row option list, carrying seven host rows and
//! showing eight of the guest's own (데이터 관리 is left out, §68.9).
//!
//! The guest's list is nine rows long and nine is not a variable: it is a
//! nine-byte type table in `.rodata` (one copy for `DrawOption` at 0x15A610,
//! one for `KeyOption` at 0x15A6EC), the wrap arithmetic in `KeyOption`, the
//! literals handed to `CommonUi::DrawScroll`, and the eight-halfword label-id
//! array `DrawOption` builds on its stack (ANALYSIS.md §45.7). The first host
//! row — 해상도, [`crate::screen`] — was appended as index 9 by five hooks.
//! The two rows added later had to go **into the middle** of the list
//! (화면필터 under 화면효과, 패드키 under 터치방식), which the append trick
//! cannot do, so the list is now driven from one table:
//!
//! ```text
//!   display  0 배경음  1 효과음  2 게임속도  3 진동  4 화면효과  5 [화면필터]
//!            6 오토타겟  7 투구보기  8 터치방식  9 [패드키]  10 [배치변경]
//!           11 [해상도]  12 [세이브 내보내기]  13 [세이브 불러오기]
//!           14 [세이브 되돌리기]
//! ```
//!
//! The last three are action rows — a plate and a label, no value box
//! ([`HostRow::is_action`]) — whose conversation is [`crate::savexfer`].
//!
//! The guest's ninth row, 데이터 관리 (guest index 8, the value-less action
//! row), is not in the table, so it is never drawn and the cursor can never
//! reach it — there is no other way into it (§68.9).
//!
//! ```text
//! ```
//!
//! [`ORDER`] maps a *display* index to either one of the guest's own nine
//! (by its guest index) or a host row. The guest keeps running its own code
//! for its own rows — the hooks translate indices at the places the guest
//! reads one:
//!
//! | where | what |
//! |---|---|
//! | `DrawOption+0x234`, the row loop head | guest row: set `r7`, the label pointer `sb` and the cached `first` to the **guest** index and let the iteration run; host row: draw it and resume at the loop tail |
//! | `CommonUi::DrawScroll` from `DrawOption` | twelve rows, not nine |
//! | `KeyOption` entry | UP/DOWN are answered here in display space; LEFT/RIGHT/SELECT/BACK on a guest row run the guest's own `KeyOption` with `this+0x0f` temporarily set to the guest index; every key on a host row is the host's |
//! | `TouchOption+0x24c` | the "second tap on 데이터 관리 is SELECT" test compares a display index against 8 — 데이터 관리 is not shown, so a second tap is always the value arrows |
//! | `TouchOption+0x2ac` / `+0x30c` | the scroll arrows' clamp and wrap, for twelve rows |
//!
//! The cursor bytes `this+0x0e` (visible slot) and `this+0x0f` (index) stay in
//! **display** space the whole time; the guest only ever sees a guest index
//! in `this+0x0f` for the duration of one call it makes itself.
//!
//! Two more hooks belong to the 패드키 row's second half, the key-mapping
//! screen ([`crate::padmap`]): `DrawOption`'s entry (the screen replaces the
//! list while it is up) and `StateMainMenu::Process+0x698`, the arm that reads
//! the answer to popup id 8 — which is also the 게임종료 / restart prompt, so
//! the hook acts only on the question it asked itself.

use arm32_cpu::{reg, Mode};

use crate::mem::RETURN_MAGIC;
use crate::screen::Screen;
use crate::Vm;

/// A row the host owns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostRow {
    /// 화면필터 — Scale2x on the way to the window, `ON`/`OFF`.
    Filter,
    /// 패드키 — 기본 / 커스텀, and the mapping screen behind 커스텀.
    PadKeys,
    /// 배치변경 — 기본 / 커스텀, and the HUD editor behind 커스텀
    /// ([`crate::hudlayout`], [`crate::hudedit`]).
    HudLayout,
    /// 해상도 — see [`crate::screen`].
    Resolution,
    /// 세이브 내보내기 / 불러오기 / 되돌리기 — action rows with no value box,
    /// see [`crate::savexfer`].
    SaveExport,
    SaveImport,
    SaveRestore,
}

impl HostRow {
    /// A row that does something when pressed rather than showing a value —
    /// drawn as a plate and a label, like the guest's own 데이터 관리.
    pub fn is_action(self) -> bool {
        matches!(self, HostRow::SaveExport | HostRow::SaveImport | HostRow::SaveRestore)
    }

    /// The save rows' action.
    pub fn save_action(self) -> Option<crate::savexfer::Action> {
        use crate::savexfer::Action;
        match self {
            HostRow::SaveExport => Some(Action::Export),
            HostRow::SaveImport => Some(Action::Import),
            HostRow::SaveRestore => Some(Action::Restore),
            _ => None,
        }
    }
}

/// One entry of the displayed list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    /// One of the guest's nine, by the index its own tables use.
    Guest(i32),
    Host(HostRow),
}

/// The list as displayed, top to bottom.
pub const ORDER: [Row; 15] = [
    Row::Guest(0),
    Row::Guest(1),
    Row::Guest(2),
    Row::Guest(3),
    Row::Guest(4),
    Row::Host(HostRow::Filter),
    Row::Guest(5),
    Row::Guest(6),
    Row::Guest(7),
    Row::Host(HostRow::PadKeys),
    Row::Host(HostRow::HudLayout),
    Row::Host(HostRow::Resolution),
    Row::Host(HostRow::SaveExport),
    Row::Host(HostRow::SaveImport),
    Row::Host(HostRow::SaveRestore),
];

/// How many rows the list shows, and how many of them fit in the panel.
pub const ROWS: i32 = ORDER.len() as i32;
const VISIBLE_ROWS: i32 = 6;
const GUEST_ROWS: i32 = 9;
const LAST: i32 = ROWS - 1;
/// The largest `first = idx - row` the window can scroll to.
const MAX_FIRST: i32 = ROWS - VISIBLE_ROWS;
/// The same for the guest's own nine — the clamp its scroll arrow applies.
const GUEST_MAX_FIRST: i32 = GUEST_ROWS - VISIBLE_ROWS;
/// The guest's 데이터 관리 — the one guest row the list does not show.
#[cfg(test)]
const DATA_MANAGEMENT: i32 = 8;

/// What a display index is.
pub fn row_at(display: i32) -> Option<Row> {
    usize::try_from(display).ok().and_then(|i| ORDER.get(i).copied())
}

/// Where a guest index is displayed.
pub fn display_of(guest: i32) -> Option<i32> {
    ORDER.iter().position(|r| *r == Row::Guest(guest)).map(|i| i as i32)
}

/// Where a host row is displayed.
pub fn display_of_host(h: HostRow) -> Option<i32> {
    ORDER.iter().position(|r| *r == Row::Host(h)).map(|i| i as i32)
}

// ---- guest symbols and offsets ------------------------------------------

/// `StateMainMenu::DrawOption` (0x11D8A0): the head of its six-iteration row
/// loop (0x11DAD4) and the tail every arm converges on (0x11DC94).
const DRAW_OPTION: &str = "_ZN13StateMainMenu10DrawOptionEv";
const ROW_LOOP_HEAD: u32 = 0x234;
const ROW_LOOP_TAIL: u32 = 0x3f4;
/// Stack slots inside `DrawOption`'s frame, as the loop head sees `sp`:
/// `this` (0x11D8F4 `str r0,[sp,#0x28]`), the cached `first = idx - row`
/// (0x11D980, re-read by the selected-row arm at 0x11DE68), and the base of
/// the eight-halfword label-id array (0x11D908 `add r7, sp, #0x5c`).
const SP_THIS: u32 = 0x28;
const SP_FIRST: u32 = 0x48;
const SP_LABELS: u32 = 0x5c;
/// The return address of `DrawOption`'s one `bl CommonUi::DrawScroll`.
const DRAW_SCROLL_RET: u32 = 0x208;

/// `StateMainMenu::KeyOption(int key)` — 0x122228.
const KEY_OPTION: &str = "_ZN13StateMainMenu9KeyOptionEi";
const GET_SUB_STATE: &str = "_ZN13StateMainMenu11GetSubStateEv";
/// `StateMainMenu::TouchOption(int x, int y)` — 0x11BD9C — and the four
/// places inside it this module answers for:
///
/// * `+0x24c` (0x11BFE8) `ldrsb r3,[r6,#0xf]; cmp r3,#8` — a second tap on
///   the cursor row: SELECT if the row is 데이터 관리, else the value arrows;
/// * `+0x2ac` (0x11C048) `cmp ip,#3` — the down arrow's clamp on `first + 1`;
/// * `+0x30c` (0x11C0A8) `mov r3,#8` — the up arrow's wrap past the top.
const TOUCH_OPTION: &str = "_ZN13StateMainMenu11TouchOptionEii";
const TOUCH_SECOND_TAP: u32 = 0x24c;
const TOUCH_SECOND_TAP_ARROWS: u32 = 0x258;
const TOUCH_SCROLL_DOWN: u32 = 0x2ac;
const TOUCH_SCROLL_DOWN_OK: u32 = 0x2c0;
const TOUCH_SCROLL_UP_WRAP: u32 = 0x30c;
const TOUCH_ARROW_DONE: u32 = 0x144;

/// `StateMainMenu::Process` (0x124898) and the arm that reads popup id 8's
/// answer (0x124F30): `ldrb r6,[r5,#0x15]; cmp r6,#0; bne 0x124EAC; b
/// 0x124DC4`. 0x124EAC is the 아니오 path — clear the popup and return.
const PROCESS: &str = "_ZN13StateMainMenu7ProcessEv";
const PROCESS_POPUP8: u32 = 0x698;
const PROCESS_POPUP8_NO: u32 = 0x614;
/// `this+0x15` — where the popup writes its answer, 0 = first button.
const POPUP_RESULT: u32 = 0x15;

const NUM_KEY_TO_NAVI: &str = "_ZN10StaticUtil15NumKeyToNaviKeyEi";

/// WIPI navigation keys, as `NumKeyToNaviKey` reports them.
pub(crate) const K_UP: i32 = -1;
pub(crate) const K_DOWN: i32 = -2;
pub(crate) const K_LEFT: i32 = -3;
pub(crate) const K_RIGHT: i32 = -4;
pub(crate) const K_SELECT: i32 = -5;
pub(crate) const K_BACK: i32 = -16;

/// The guest primitives a row is drawn with.
pub(crate) const GET_HALF_WIDTH: &str = "_ZN7Graphic12GetHalfWidthEv";
pub(crate) const GET_HALF_HEIGHT: &str = "_ZN7Graphic13GetHalfHeightEv";
pub(crate) const GET_FB: &str = "_ZN7Graphic5GetFBEv";
pub(crate) const DRAW_TEXT: &str = "_ZN5BFont8DrawTextEiiiPcii";
pub(crate) const DRAW_HALF_BOX: &str = "_ZN8CommonUi11DrawHalfBoxEiiiiiahh";
pub(crate) const FILL_ROUND_RECT: &str = "_ZN7Graphic13FillRoundRectEiiiiiihi";
pub(crate) const FILL_RECT_ALPHA: &str = "_ZN7Graphic13FillRectAlphaEiiiiihi";
pub(crate) const DRAW_SUBJECT_BOX: &str = "_ZN8CommonUi14DrawSubjectBoxEiiiPch";
const DRAW_IMAGE: &str = "_ZN7Graphic9DrawImageEiiiP8GbmImagei";
const DRAW_IMAGE_MIRROR: &str = "_ZN7Graphic15DrawImageMirrorEiiiP8GbmImagehi";
const DRAW_SCROLL: &str = "_ZN8CommonUi10DrawScrollEiiiiaaaaii";
const DRAW_SCROLL_ARGS: u32 = 11;
const DRAW_SCROLL_TOTAL: usize = 8;

pub(crate) const LCD_G: &str = "_lcdG";
pub(crate) const P_CFONT: &str = "_pCFont";
pub(crate) const GV: &str = "gv";
pub(crate) const GV_COMMON_UI: u32 = 0x15fc;
const COMMON_UI_ARROWS: u32 = 0x1b84;

/// Row geometry, every number read off `DrawOption` (ANALYSIS.md §45.7).
const PLATE_DX: i32 = -0x4c;
const PLATE_W: i32 = 0x8a;
const PLATE_H: i32 = 0x11;
const LABEL_DX: i32 = -0x3f;
const BOX_DX: i32 = 0x0e;
const BOX_W: i32 = 0x1f;
const BOX_H: i32 = 0x0d;
const VALUE_DX: i32 = 0x1e;
const ARROW_L_DX: i32 = 0x0c;
const ARROW_R_DX: i32 = 0x2f;
/// Three hangul are 33 px and the guest's value box is 31: the 커스텀 row
/// gets a box this much wider and its right chevron moved by the same.
const WIDE_EXTRA: i32 = 4;

pub(crate) const FG: u32 = 0x00ff_ffff;
pub(crate) const FG_DIM: u32 = 0x0080_8080;

/// Labels, EUC-KR (CLAUDE.md: a guest string never goes through `String`).
const LABEL_FILTER: &[u8] = &[0xc8, 0xad, 0xb8, 0xe9, 0xc7, 0xca, 0xc5, 0xcd, 0x00]; // 화면필터
const LABEL_PADKEYS: &[u8] = &[0xc6, 0xd0, 0xb5, 0xe5, 0xc5, 0xb0, 0x00]; // 패드키
const LABEL_HUD: &[u8] = &[0xb9, 0xe8, 0xc4, 0xa1, 0xba, 0xaf, 0xb0, 0xe6, 0x00]; // 배치변경
/// 배치변경 where it cannot apply (세로, or a canvas too narrow for the
/// landscape HUD): the row is there and says so.
const VALUE_NONE: &[u8] = b"-\0";
const VALUE_ON: &[u8] = b"ON\0";
const VALUE_OFF: &[u8] = b"OFF\0";
const VALUE_DEFAULT: &[u8] = &[0xb1, 0xe2, 0xba, 0xbb, 0x00]; // 기본
const VALUE_CUSTOM: &[u8] = &[0xc4, 0xbf, 0xbd, 0xba, 0xc5, 0xd2, 0x00]; // 커스텀

/// Per-`Vm` state of the list.
#[derive(Default)]
pub struct Options {
    /// Host rows drawn, for the probe's summary.
    pub drawn: u64,
    /// The host strings, once each in the guest heap, keyed by the address
    /// of the `'static` slice they came from.
    strings: std::collections::HashMap<usize, u32>,
}

/// Install the list, or report that this binary cannot carry it.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for s in [
        DRAW_OPTION,
        KEY_OPTION,
        TOUCH_OPTION,
        PROCESS,
        GET_SUB_STATE,
        NUM_KEY_TO_NAVI,
        GET_HALF_WIDTH,
        GET_HALF_HEIGHT,
        GET_FB,
        DRAW_TEXT,
        DRAW_HALF_BOX,
        FILL_ROUND_RECT,
        FILL_RECT_ALPHA,
        DRAW_SUBJECT_BOX,
        DRAW_IMAGE,
        DRAW_IMAGE_MIRROR,
        DRAW_SCROLL,
        LCD_G,
        P_CFONT,
        GV,
        crate::screen::SET_POPUP_STATE,
    ] {
        if vm.symbol(s).is_none() {
            return Err(format!("option rows need `{s}`, not in this binary"));
        }
    }
    let draw = vm.symbol(DRAW_OPTION).expect("checked above");
    vm.install_hook(DRAW_OPTION, draw, draw_option_entry_hook)?;
    vm.install_hook(DRAW_OPTION, draw + ROW_LOOP_HEAD, row_hook)?;
    let key = vm.symbol(KEY_OPTION).expect("checked above");
    vm.install_hook(KEY_OPTION, key, key_hook)?;
    let touch = vm.symbol(TOUCH_OPTION).expect("checked above");
    vm.install_hook(TOUCH_OPTION, touch + TOUCH_SECOND_TAP, second_tap_hook)?;
    vm.install_hook(TOUCH_OPTION, touch + TOUCH_SCROLL_DOWN, scroll_down_hook)?;
    vm.install_hook(TOUCH_OPTION, touch + TOUCH_SCROLL_UP_WRAP, scroll_up_hook)?;
    let scroll = vm.symbol(DRAW_SCROLL).expect("checked above");
    vm.install_hook(DRAW_SCROLL, scroll, scroll_bar_hook)?;
    let process = vm.symbol(PROCESS).expect("checked above");
    vm.install_hook(PROCESS, process + PROCESS_POPUP8, popup_answer_hook)?;
    Ok(vec![DRAW_OPTION, KEY_OPTION, TOUCH_OPTION, DRAW_SCROLL, PROCESS])
}

/// Read a guest global pointer: the symbol names the slot, the slot holds the
/// object.
pub(crate) fn deref(vm: &Vm, sym: &str) -> Option<u32> {
    let p = vm.symbol(sym)?;
    let v = vm.mem.read_u32(p);
    (v != 0).then_some(v)
}

/// Put one `'static` string in the guest heap once and keep the address.
pub(crate) fn text(vm: &mut Vm, bytes: &'static [u8]) -> Option<u32> {
    let key = bytes.as_ptr() as usize;
    if let Some(p) = vm.options.strings.get(&key) {
        return Some(*p);
    }
    let p = vm.heap.alloc(bytes.len() as u32)?;
    vm.mem.write_bytes(p, bytes);
    vm.options.strings.insert(key, p);
    Some(p)
}

/// Whether the interpreter reached this address on its own rather than
/// through [`Vm::call`] — the guard every hook here shares.
pub(crate) fn entered_normally(vm: &Vm) -> bool {
    vm.cpu.reg_get(Mode::User, reg::LR) != RETURN_MAGIC
}

/// The guest's drawing handles.
pub(crate) struct Canvas {
    pub graphic: u32,
    pub fb: u32,
    pub font: u32,
    pub common_ui: u32,
    pub half_w: i32,
}

pub(crate) fn canvas(vm: &mut Vm) -> Result<Option<Canvas>, String> {
    let (Some(graphic), Some(font), Some(gv)) =
        (deref(vm, LCD_G), deref(vm, P_CFONT), vm.symbol(GV))
    else {
        return Ok(None);
    };
    let common_ui = vm.mem.read_u32(gv + GV_COMMON_UI);
    let (Some(hw), Some(fbf)) = (vm.symbol(GET_HALF_WIDTH), vm.symbol(GET_FB))
    else {
        return Ok(None);
    };
    let half_w = vm.call(hw, &[graphic])? as i32;
    let fb = vm.call(fbf, &[graphic])?;
    if fb == 0 {
        return Ok(None);
    }
    Ok(Some(Canvas { graphic, fb, font, common_ui, half_w }))
}

/// `CommonUi::DrawHalfBox` — the label plate every option row sits on.
/// `selected` picks the cursor style (`1,1,0xff`) over the plain one
/// (`4,0,0xff`), the two the guest's own rows use.
pub(crate) fn plate(
    vm: &mut Vm,
    c: &Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    selected: bool,
) -> Result<(), String> {
    if c.common_ui == 0 {
        return Ok(());
    }
    let Some(f) = vm.symbol(DRAW_HALF_BOX) else { return Ok(()) };
    let (a, b) = if selected { (1u32, 1u32) } else { (4, 0) };
    vm.call(f, &[c.common_ui, c.fb, x as u32, y as u32, w as u32, h as u32, a, b, 0xff])?;
    Ok(())
}

/// `BFont::DrawText(font, fb, x, y, str, align, colour)` — align 0 left,
/// 1 centred on `x`.
pub(crate) fn label(
    vm: &mut Vm,
    c: &Canvas,
    x: i32,
    y: i32,
    s: &'static [u8],
    align: u32,
    colour: u32,
) -> Result<(), String> {
    if let (Some(f), Some(p)) = (vm.symbol(DRAW_TEXT), text(vm, s)) {
        vm.call(f, &[c.font, c.fb, x as u32, y as u32, p, align, colour])?;
    }
    Ok(())
}

/// What a host row's value box reads right now.
fn value_of(vm: &Vm, h: HostRow) -> &'static [u8] {
    match h {
        HostRow::Filter => {
            if vm.scale2x {
                VALUE_ON
            } else {
                VALUE_OFF
            }
        }
        HostRow::PadKeys => {
            if vm.padmap.shows_custom() {
                VALUE_CUSTOM
            } else {
                VALUE_DEFAULT
            }
        }
        HostRow::HudLayout => {
            if !vm.hudlayout.installed {
                VALUE_NONE
            } else if vm.hudlayout.shows_custom() {
                VALUE_CUSTOM
            } else {
                VALUE_DEFAULT
            }
        }
        HostRow::Resolution => vm.screen.chosen.value_bytes(),
        HostRow::SaveExport | HostRow::SaveImport | HostRow::SaveRestore => b"\0",
    }
}

fn label_of(h: HostRow) -> &'static [u8] {
    match h {
        HostRow::Filter => LABEL_FILTER,
        HostRow::PadKeys => LABEL_PADKEYS,
        HostRow::HudLayout => LABEL_HUD,
        HostRow::Resolution => crate::screen::LABEL,
        HostRow::SaveExport => crate::savexfer::LABEL_EXPORT,
        HostRow::SaveImport => crate::savexfer::LABEL_IMPORT,
        HostRow::SaveRestore => crate::savexfer::LABEL_RESTORE,
    }
}

/// Draw one host row in the slot the loop is on, with the guest's own
/// primitives — the five (or seven) calls one of its value rows is made of.
pub(crate) fn draw_row(vm: &mut Vm, h: HostRow, y: i32, selected: bool) -> Result<(), String> {
    let Some(c) = canvas(vm)? else { return Ok(()) };
    let value = value_of(vm, h);
    // Three hangul do not fit the guest's 31 px box.
    let wide = value.len() > 5;
    let extra = if wide { WIDE_EXTRA } else { 0 };

    // 1. the label plate, 2. the label.
    plate(vm, &c, c.half_w + PLATE_DX, y - 3, PLATE_W, PLATE_H, selected)?;
    let colour = if selected { FG } else { FG_DIM };
    label(vm, &c, c.half_w + LABEL_DX, y, label_of(h), 0, colour)?;
    if h.is_action() {
        return Ok(());
    }

    // 3. the black value box, and 4. the value inside it, centred.
    if let Some(f) = vm.symbol(FILL_ROUND_RECT) {
        vm.call(
            f,
            &[
                c.graphic,
                c.fb,
                (c.half_w + BOX_DX) as u32,
                (y - 1) as u32,
                (BOX_W + extra) as u32,
                BOX_H as u32,
                4,
                0xff,
                0,
            ],
        )?;
    }
    label(vm, &c, c.half_w + VALUE_DX + extra / 2, y, value, 1, FG)?;

    // 5. the two chevrons, on the cursor's row only.
    if selected && c.common_ui != 0 {
        let img = c.common_ui + COMMON_UI_ARROWS;
        if let Some(f) = vm.symbol(DRAW_IMAGE_MIRROR) {
            vm.call(f, &[c.graphic, c.fb, (c.half_w + ARROW_L_DX) as u32, y as u32, img, 8, 0x21])?;
        }
        if let Some(f) = vm.symbol(DRAW_IMAGE) {
            vm.call(
                f,
                &[c.graphic, c.fb, (c.half_w + ARROW_R_DX + extra) as u32, y as u32, img, 9],
            )?;
        }
    }
    Ok(())
}

/// The cursor, in the guest's own two bytes: `(visible row, display index)`.
fn cursor(vm: &Vm, this: u32) -> (i32, i32) {
    (vm.mem.read_u8(this + 0x0e) as i8 as i32, vm.mem.read_u8(this + 0x0f) as i8 as i32)
}

fn set_cursor(vm: &mut Vm, this: u32, row: i32, index: i32) {
    vm.mem.write_bytes(this + 0x0e, &[row as u8]);
    vm.mem.write_bytes(this + 0x0f, &[index as u8]);
}

/// The window's first row after moving the cursor to `target`, the way the
/// guest moves it: keep the window if the target is inside it, otherwise
/// scroll by the least that brings it in — and never outside `0..=MAX_FIRST`.
fn first_after(row: i32, index: i32, target: i32) -> i32 {
    (index - row).clamp(target - (VISIBLE_ROWS - 1), target).clamp(0, MAX_FIRST)
}

/// Move the cursor to `target` (a display index).
fn move_to(vm: &mut Vm, this: u32, target: i32) {
    let (row, index) = cursor(vm, this);
    let first = first_after(row, index, target);
    set_cursor(vm, this, target - first, target);
}

// ---- the hooks ------------------------------------------------------------

/// `DrawOption`'s entry: while the key-mapping screen is up it is drawn in
/// the list's place.
fn draw_option_entry_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    if vm.hudlayout.edit.active {
        crate::hudedit::paint(vm)?;
        return Ok(true);
    }
    if !vm.padmap.active {
        return Ok(false);
    }
    let this = vm.arg(0);
    crate::padmap::paint(vm, this)?;
    Ok(true)
}

/// The row loop's head. Every iteration passes here with the slot in `r6`,
/// the y in `r5`, and — on the first iteration only — the display index in
/// `r7`; after that `r7` and `sb` hold whatever the previous iteration left
/// plus one, so the display index is recomputed from the cursor each time.
fn row_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let slot = vm.cpu.reg_get(Mode::User, 6) as i32;
    let y = vm.cpu.reg_get(Mode::User, 5) as i32;
    let sp = vm.cpu.reg_get(Mode::User, reg::SP);
    let this = vm.mem.read_u32(sp + SP_THIS);
    if this == 0 {
        return Ok(false);
    }
    let (row, index) = cursor(vm, this);
    let display = (index - row) + slot;
    match row_at(display) {
        Some(Row::Guest(g)) => {
            // Point the iteration at the guest's own row `g`: the loop index,
            // the label-id pointer, and the cached `first` the selected-row
            // arm adds the slot back to (0x11DE68).
            vm.cpu.reg_set(Mode::User, 7, g as u32);
            vm.cpu.reg_set(Mode::User, 9, sp + SP_LABELS + 2 * g as u32);
            vm.mem.write_u32(sp + SP_FIRST, (g - slot) as u32);
            Ok(false)
        }
        Some(Row::Host(h)) => {
            draw_row(vm, h, y, row == slot)?;
            vm.options.drawn += 1;
            if h == HostRow::Resolution {
                vm.screen.drawn += 1;
            }
            // `PC := LR` on `true`, so the loop tail is the address to resume
            // at: one iteration's bookkeeping, done by the guest.
            let tail =
                vm.symbol(DRAW_OPTION).ok_or("options: DrawOption vanished")? + ROW_LOOP_TAIL;
            vm.cpu.reg_set(Mode::User, reg::LR, tail);
            Ok(true)
        }
        // A window the guest cannot be showing. Leave it alone.
        None => Ok(false),
    }
}

/// The scrollbar counts the host's rows, not the guest's — on `DrawOption`'s
/// call (nine → twelve) and `DrawSystemMenu`'s (eight → ten, see
/// [`crate::igoptions`]) only; `DrawScroll` draws every other list in the
/// game too.
fn scroll_bar_hook(vm: &mut Vm) -> Result<bool, String> {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if lr == RETURN_MAGIC {
        return Ok(false);
    }
    let main = vm.symbol(DRAW_OPTION).map(|a| a + DRAW_SCROLL_RET);
    let ingame = vm
        .symbol("_ZN15StateInGameMenu14DrawSystemMenuEv")
        .map(|a| a + crate::igoptions::DRAW_SCROLL_RET);
    let (was, now) = if Some(lr) == main {
        (GUEST_ROWS, ROWS)
    } else if Some(lr) == ingame {
        (crate::igoptions::GUEST_ROWS, crate::igoptions::ROWS)
    } else {
        return Ok(false);
    };
    let mut a: Vec<u32> = (0..DRAW_SCROLL_ARGS).map(|i| vm.arg(i as usize)).collect();
    if a[DRAW_SCROLL_TOTAL] == was as u32 {
        a[DRAW_SCROLL_TOTAL] = now as u32;
    }
    let f = vm.symbol(DRAW_SCROLL).ok_or("options: DrawScroll vanished")?;
    let ret = vm.call(f, &a)?;
    vm.cpu.reg_set(Mode::User, 0, ret);
    Ok(true)
}

/// Every key on the list.
fn key_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.arg(0);
    let key = vm.arg(1) as i32;
    if this == 0 {
        return Ok(false);
    }
    // A popup over the panel owns the keys; the guest tests the same thing
    // (0x122290) before it looks at the cursor at all.
    if let Some(f) = vm.symbol(GET_SUB_STATE) {
        if vm.call(f, &[this])? & 0xff != 0 {
            return Ok(false);
        }
    }
    let navi = match vm.symbol(NUM_KEY_TO_NAVI) {
        Some(f) => vm.call(f, &[key as u32])? as i32,
        None => key,
    };
    // The mapping screen takes every key while it is up; so does the
    // 배치변경 editor.
    if vm.padmap.active {
        crate::padmap::key(vm, navi)?;
        return Ok(true);
    }
    if vm.hudlayout.edit.active {
        crate::hudedit::key(vm, navi)?;
        return Ok(true);
    }
    let (_, index) = cursor(vm, this);
    match navi {
        // The guest's own UP/DOWN wrap at nine; these wrap at twelve, with the
        // same "move inside the window, else scroll, else wrap" shape.
        K_DOWN => {
            move_to(vm, this, if index >= LAST { 0 } else { index + 1 });
            return Ok(true);
        }
        K_UP => {
            move_to(vm, this, if index <= 0 { LAST } else { index - 1 });
            return Ok(true);
        }
        _ => {}
    }
    match row_at(index) {
        Some(Row::Host(h)) => match navi {
            K_LEFT | K_RIGHT | K_SELECT => {
                host_key(vm, this, h, navi)?;
                Ok(true)
            }
            // BACK above all is the guest's: its arm does not read the index.
            _ => Ok(false),
        },
        Some(Row::Guest(g)) => {
            // Run the guest's own handler with the index it expects, then put
            // the display index back unless it moved the cursor itself.
            vm.mem.write_bytes(this + 0x0f, &[g as u8]);
            let f = vm.symbol(KEY_OPTION).ok_or("options: KeyOption vanished")?;
            vm.call(f, &[this, key as u32])?;
            if vm.mem.read_u8(this + 0x0f) as i8 as i32 == g {
                vm.mem.write_bytes(this + 0x0f, &[index as u8]);
            }
            Ok(true)
        }
        None => Ok(false),
    }
}

/// LEFT / RIGHT / SELECT on a host row.
fn host_key(vm: &mut Vm, this: u32, h: HostRow, navi: i32) -> Result<(), String> {
    match h {
        HostRow::Filter => set_scale2x(vm, !vm.scale2x),
        HostRow::PadKeys => {
            if vm.padmap.shows_custom() {
                crate::padmap::set_default(vm);
            } else {
                crate::padmap::ask(vm, this)?;
            }
        }
        HostRow::HudLayout => {
            if !vm.hudlayout.installed {
                // 세로 or a narrow canvas: nothing to lay out.
            } else if vm.hudlayout.shows_custom() {
                crate::hudlayout::set_default(vm);
            } else {
                crate::hudlayout::ask(vm, this)?;
            }
        }
        HostRow::Resolution => {
            let next = vm.screen.chosen.cycle(navi == K_LEFT);
            Screen::choose(vm, this, next)?;
        }
        HostRow::SaveExport | HostRow::SaveImport | HostRow::SaveRestore => {
            if let Some(a) = h.save_action() {
                crate::savexfer::start(vm, crate::padmap::Scene::Title, this, a)?;
            }
        }
    }
    Ok(())
}

/// `<files>/hl5_scale.txt` — the display filter, as the shell reads it at
/// start and as the 화면필터 row writes it. The shell picks the change up on
/// the next tick through [`Vm::take_scale2x_change`].
pub const SCALE_FILE: &str = "hl5_scale.txt";

/// Switch the display filter and remember the choice.
pub fn set_scale2x(vm: &mut Vm, on: bool) {
    if vm.scale2x == on {
        return;
    }
    vm.scale2x = on;
    vm.scale2x_dirty = true;
    let path = vm.cfg.storage.join(SCALE_FILE);
    let token = if on { "scale2x" } else { "nearest" };
    match std::fs::write(&path, format!("{token}\n")) {
        Ok(()) => vm.log.push(format!("filter: {token} written to {}", path.display())),
        Err(e) => vm.log.push(format!("filter: cannot write {}: {e}", path.display())),
    }
}

/// A second tap on the cursor row. The guest SELECTs when the display index
/// is 8 — its 데이터 관리 — and steps the value otherwise; 데이터 관리 is not
/// on the list, and display 8 is 터치방식, so the answer is always the value
/// arrows.
fn second_tap_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let base = vm.symbol(TOUCH_OPTION).ok_or("options: TouchOption vanished")?;
    vm.cpu.reg_set(Mode::User, reg::LR, base + TOUCH_SECOND_TAP_ARROWS);
    Ok(true)
}

/// The scroll-down arrow may reach a window starting at row `MAX_FIRST`.
///
/// The instruction this sits on is `cmp ip, #3` (0x11C048), the clamp that
/// makes the arrow wrap to the top instead of scrolling further; `ip` is
/// `first + 1`. A value the guest would accept is left to it; one past its
/// clamp but inside ours resumes at the arm's "scrolled" continuation.
fn scroll_down_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let ip = vm.cpu.reg_get(Mode::User, 12) as i32;
    if !(GUEST_MAX_FIRST + 1..=MAX_FIRST).contains(&ip) {
        return Ok(false);
    }
    let go =
        vm.symbol(TOUCH_OPTION).ok_or("options: TouchOption vanished")? + TOUCH_SCROLL_DOWN_OK;
    vm.cpu.reg_set(Mode::User, reg::LR, go);
    Ok(true)
}

/// The scroll-up arrow wraps to the last row, which is ours.
fn scroll_up_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.cpu.reg_get(Mode::User, 6);
    if this == 0 {
        return Ok(false);
    }
    set_cursor(vm, this, LAST - MAX_FIRST, LAST);
    let go =
        vm.symbol(TOUCH_OPTION).ok_or("options: TouchOption vanished")? + TOUCH_ARROW_DONE;
    vm.cpu.reg_set(Mode::User, reg::LR, go);
    Ok(true)
}

/// Popup id 8's answer. Seven questions share the id — 게임종료, the
/// resolution restart prompt, the 패드키 one, the 배치변경 one and the three of
/// the save rows — and the first two want the guest's own reading (예 exits).
/// Only while one of the host's questions is the one on screen does this take
/// the answer, and it then resumes at the guest's 아니오 path so the popup is
/// cleared the way the guest clears it.
fn popup_answer_hook(vm: &mut Vm) -> Result<bool, String> {
    let save = vm.savexfer.asking(crate::padmap::Scene::Title);
    if !entered_normally(vm) || !(vm.padmap.pending || vm.hudlayout.pending || save) {
        return Ok(false);
    }
    let this = vm.cpu.reg_get(Mode::User, 5);
    if this == 0 {
        return Ok(false);
    }
    let yes = vm.mem.read_u8(this + POPUP_RESULT) == 0;
    if save {
        crate::savexfer::answered(vm, yes)?;
    } else if vm.padmap.pending {
        crate::padmap::answered(vm, yes)?;
    } else {
        crate::hudlayout::answered(vm, yes)?;
    }
    vm.cpu.reg_set(Mode::User, 6, 1);
    let go = vm.symbol(PROCESS).ok_or("options: Process vanished")? + PROCESS_POPUP8_NO;
    vm.cpu.reg_set(Mode::User, reg::LR, go);
    Ok(true)
}

/// Raise the guest's own two-button popup (id 8) with `msg` as its body.
/// The answer arm is the guest's — see [`popup_answer_hook`] for the one case
/// the host reads it instead.
pub(crate) fn ask_two_buttons(vm: &mut Vm, this: u32, msg: &'static [u8]) -> Result<bool, String> {
    let Some(p) = text(vm, msg) else { return Ok(false) };
    let f = vm.symbol(crate::screen::SET_POPUP_STATE).ok_or("options: SetPopupState vanished")?;
    vm.call(f, &[this, crate::screen::POPUP_RESTART, p, 0, 0, 9])?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_the_guests_eight_plus_seven_in_the_asked_order() {
        assert_eq!(ROWS, 15);
        let guests: Vec<i32> = ORDER
            .iter()
            .filter_map(|r| if let Row::Guest(g) = r { Some(*g) } else { None })
            .collect();
        assert_eq!(guests, (0..8).collect::<Vec<_>>(), "every guest row but 데이터 관리, once, in order");
        assert_eq!(display_of(DATA_MANAGEMENT), None, "데이터 관리 is not on the list");
        // 화면필터 directly under 화면효과 (guest 4), 패드키 directly under
        // 터치방식 (guest 7), 해상도 last.
        assert_eq!(display_of_host(HostRow::Filter), Some(display_of(4).unwrap() + 1));
        assert_eq!(display_of_host(HostRow::PadKeys), Some(display_of(7).unwrap() + 1));
        // 배치변경 directly under 패드키.
        assert_eq!(display_of_host(HostRow::HudLayout), Some(display_of_host(HostRow::PadKeys).unwrap() + 1));
        // 해상도, then the three save rows at the very bottom, in order.
        assert_eq!(display_of_host(HostRow::Resolution), Some(LAST - 3));
        assert_eq!(display_of_host(HostRow::SaveExport), Some(LAST - 2));
        assert_eq!(display_of_host(HostRow::SaveImport), Some(LAST - 1));
        assert_eq!(display_of_host(HostRow::SaveRestore), Some(LAST));
        for r in ORDER {
            if let Row::Host(h) = r {
                assert_eq!(h.is_action(), h.save_action().is_some(), "{h:?}");
            }
        }
        assert_eq!(row_at(-1), None);
        assert_eq!(row_at(ROWS), None);
    }

    #[test]
    fn the_scroll_bounds_follow_the_row_count() {
        assert_eq!(MAX_FIRST, 9, "fifteen rows, six visible");
        assert_eq!(GUEST_MAX_FIRST, 3);
        assert_eq!(LAST - MAX_FIRST, 5, "the up-wrap lands with the last row on the bottom slot");
    }

    /// Every cursor move leaves a window `DrawOption` can walk, over every
    /// cursor the list can hold and every target a key can ask for.
    #[test]
    fn no_cursor_move_can_leave_the_window_out_of_range() {
        for first in 0..=MAX_FIRST {
            for row in 0..VISIBLE_ROWS {
                let index = first + row;
                if index > LAST {
                    continue;
                }
                for target in 0..ROWS {
                    let f = first_after(row, index, target);
                    let new_row = target - f;
                    assert!(
                        (0..=MAX_FIRST).contains(&f),
                        "first {first} row {row} -> {target} left first {f}"
                    );
                    assert!(
                        (0..VISIBLE_ROWS).contains(&new_row),
                        "first {first} row {row} -> {target} left row {new_row}"
                    );
                }
            }
        }
    }

    /// The guest's own DOWN keeps the window when it can, scrolls by one
    /// otherwise; the host's answer has to do the same on its rows.
    #[test]
    fn down_from_the_bottom_slot_scrolls_by_one() {
        assert_eq!(first_after(5, 5, 6), 1);
        assert_eq!(first_after(2, 2, 3), 0, "inside the window: no scroll");
        assert_eq!(first_after(0, 6, 5), 5, "up from the top slot scrolls up by one");
        assert_eq!(first_after(0, 0, LAST), MAX_FIRST, "wrapping to the last row shows the bottom window");
        assert_eq!(first_after(5, LAST, 0), 0, "wrapping to the first row shows the top window");
    }

    #[test]
    fn the_labels_and_values_are_c_strings_in_euc_kr() {
        for s in [LABEL_FILTER, LABEL_PADKEYS, LABEL_HUD, VALUE_ON, VALUE_OFF, VALUE_DEFAULT, VALUE_CUSTOM, VALUE_NONE] {
            assert_eq!(s.last(), Some(&0));
            let body = &s[..s.len() - 1];
            let mut i = 0;
            while i < body.len() {
                if body[i] >= 0x80 {
                    assert!((0x41..=0xfe).contains(&body[i + 1]), "trail byte in {s:?}");
                    i += 2;
                } else {
                    i += 1;
                }
            }
        }
        // 커스텀 is the one value wider than the guest's box.
        assert!(VALUE_CUSTOM.len() > 5 && VALUE_DEFAULT.len() <= 5);
    }
}
