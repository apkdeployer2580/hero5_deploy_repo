//! 시스템 > 환경설정 inside the game — the in-game menu's copy of the option
//! list, carrying the same two host rows as the title screen's.
//!
//! `StateInGameMenu::DrawSystemMenu` (0xEB338) draws, in its substate 1, a
//! list that is the title screen's `DrawOption` almost line for line: the
//! same `.rodata` type table, the same label ids (`0xcc..0xd1, 0x120,
//! 0x155`), the same `gv[0x84+idx]` values, the same row pitch (0x17) and
//! panel geometry — eight rows instead of nine (no 데이터 관리), the cursor
//! bytes swapped (`this+0x0f` is the visible row here, `this+0x10` the
//! index), and the keys in `KeySystemMenu` (0x103E20) / the taps in
//! `TouchSystemMenu` (0xFE3A4). So the same device as [`crate::options`]:
//! a display order with host rows in it, translated to guest indices at the
//! places the guest reads one.
//!
//! ```text
//!   display  0 배경음 1 효과음 2 게임속도 3 진동 4 화면효과 5 [화면필터]
//!            6 오토타겟 7 투구보기 8 터치방식 9 [패드키] 10 [배치변경]
//!           11 [세이브 내보내기] 12 [세이브 불러오기] 13 [세이브 되돌리기]
//! ```
//!
//! No 해상도 here: its restart prompt is the title screen's 게임종료 dialog,
//! and the canvas cannot change mid-game anyway.
//!
//! | where | what |
//! |---|---|
//! | `DrawSystemMenu+0x5c8` (first row) and `+0x77c` (every later row) | guest row: `r5`, the label pointer `r8` and the cached `first` at `[sp+0x54]` become the guest index; host row: drawn, resume at the loop tail `+0x764` |
//! | `CommonUi::DrawScroll` from `DrawSystemMenu` | ten rows, not eight (shared hook in `options`) |
//! | `KeySystemMenu` entry, substate 1 | UP/DOWN in display space; LEFT/RIGHT/SELECT on a guest row run the guest with `this+0x10` set to the guest index |
//! | `TouchSystemMenu+0x200` | the down arrow's clamp (`first + 1 > 2` wraps to the top) |
//! | `TouchSystemMenu+0x604` | the up arrow's wrap past the top to the last row |
//! | `StateInGameMenu::Process+0x9cc` | the answer to popup id 0x16 — the 타이틀로 question's two-button popup, borrowed for the 패드키 question and read only while it is ours |
//! | `StateInGameMenu::Paint` entry | the key-mapping screen, in the menu's place while it is up |

use arm32_cpu::{reg, Mode};

use crate::options::{
    self, draw_row, entered_normally, HostRow, Row, K_DOWN, K_LEFT, K_RIGHT, K_SELECT, K_UP,
};
use crate::padmap::Scene;
use crate::Vm;

/// The list as displayed, top to bottom.
pub const ORDER: [Row; 14] = [
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
    Row::Host(HostRow::SaveExport),
    Row::Host(HostRow::SaveImport),
    Row::Host(HostRow::SaveRestore),
];
pub const ROWS: i32 = ORDER.len() as i32;
const VISIBLE_ROWS: i32 = 6;
pub const GUEST_ROWS: i32 = 8;
const LAST: i32 = ROWS - 1;
const MAX_FIRST: i32 = ROWS - VISIBLE_ROWS;

pub fn row_at(display: i32) -> Option<Row> {
    usize::try_from(display).ok().and_then(|i| ORDER.get(i).copied())
}

const DRAW_SYSTEM_MENU: &str = "_ZN15StateInGameMenu14DrawSystemMenuEv";
/// The first iteration's test (0xEB900 `cmp r3, sb` — after `mov sb, #0` and
/// `mov r5, r2` have set the slot and the index; hooking the `ldrsb` two
/// instructions earlier read the row byte as the slot and left the loop
/// counter unset, which drew one row and stopped), the test every later
/// iteration comes back to (0xEBAB4 `ldr r1,[sp,#0x50]`), and the tail both
/// resume at for a host row (0xEBA9C `add sb,#1`).
const ROW_FIRST_HEAD: u32 = 0x5c8;
const ROW_TAIL_TEST: u32 = 0x77c;
const ROW_TAIL: u32 = 0x764;
/// Stack slots as the loop sees `sp`: `this` (0xEB8BC), the cached `first`
/// the selected arm adds the slot back to (0xEBC0C), the eight-halfword label
/// array (0xEB3A4 `add r5, sp, #0x58`).
const SP_THIS: u32 = 0x50;
const SP_FIRST: u32 = 0x54;
const SP_LABELS: u32 = 0x58;
/// The return address of `DrawSystemMenu`'s `bl CommonUi::DrawScroll`
/// (0xEB8B0), for the shared scrollbar hook.
pub const DRAW_SCROLL_RET: u32 = 0x578;

const KEY_SYSTEM_MENU: &str = "_ZN15StateInGameMenu13KeySystemMenuEi";
const GET_SUB_STATE: &str = "_ZN15StateInGameMenu11GetSubStateEv";
/// `GetSubState()` while the 환경설정 page is the one on the system tab —
/// measured with a `--watch`-style log, not read off the jump tables, which
/// start one past the fall-through branch (CLAUDE.md's `addls pc` trap).
const OPTION_SUBSTATE: u32 = 1;
const NUM_KEY_TO_NAVI: &str = "_ZN10StaticUtil15NumKeyToNaviKeyEi";

const TOUCH_SYSTEM_MENU: &str = "_ZN15StateInGameMenu15TouchSystemMenuEii";
/// `+0x200` (0xFE5A4) `cmp r3, #2`: the down arrow of the 환경설정 page,
/// `first + 1` against the guest's last window (eight rows, six shown: first
/// 2); past it 0xFE7A8 puts the cursor back on the top row. `+0x208`
/// (0xFE5AC) is the scroll it otherwise does.
///
/// The function is one jump table on `GetSubState()` (0xFE4B8) whose arms
/// share tails, and substate 2's list (the one counted in `TextMgr+0x104`)
/// has a down arrow and an up-wrap of the same shape at `+0x3fc` / `+0x69c`.
/// The first version of this file hooked those — it changed that list and
/// left this one wrapping on the third scroll (ANALYSIS.md §68.8). The arms
/// are told apart by which row loop they come back from: this page's
/// (0xFE7DC) ends at 0xFE554 and falls into 0xFE564..
const TOUCH_DOWN_CLAMP: u32 = 0x200;
const TOUCH_DOWN_SCROLL: u32 = 0x208;
/// `+0x604` (0xFE9A8) `mov r3, #7`: the up arrow's wrap, index 7 row 5 —
/// reached from the same page's arrow code (0xFE580 → 0xFE990).
const TOUCH_UP_WRAP_A: u32 = 0x604;
/// The guest's own last window, eight rows less the six shown.
const GUEST_MAX_FIRST: i32 = GUEST_ROWS - VISIBLE_ROWS;

const PROCESS: &str = "_ZN15StateInGameMenu7ProcessEv";
/// The arm for popup id 0x16 (0x10AFFC — table slot 0x15 counted from the
/// case-0 entry *after* the fall-through branch, CLAUDE.md's `addls pc`
/// trap; 0x10AE14 one slot up is id 0x15's): result 0 (예) goes to the title
/// through `vptr+0x68`; otherwise 0x10B538 clears the popup when `r3 == 1`.
const PROCESS_POPUP_ARM: u32 = 0x9cc;
const PROCESS_POPUP_NO: u32 = 0xf08;
/// `this+0x19` — where the in-game popup writes its answer (0xE67CC).
const POPUP_RESULT: u32 = 0x19;
const SET_POPUP_STATE: &str = "_ZN15StateInGameMenu13SetPopupStateEaPciiiP8ItemInfo";
/// The id `KeySystemMenu` uses for its 타이틀로 question (0x104584): two
/// buttons, 예 / 아니오 (`SetPopupState+0x138`).
const POPUP_ID: u32 = 0x16;

const PAINT: &str = "_ZN15StateInGameMenu5PaintEv";

/// The cursor, `(visible row, display index)` — `this+0x0f` / `this+0x10`.
fn cursor(vm: &Vm, this: u32) -> (i32, i32) {
    (vm.mem.read_u8(this + 0x0f) as i8 as i32, vm.mem.read_u8(this + 0x10) as i8 as i32)
}

fn set_cursor(vm: &mut Vm, this: u32, row: i32, index: i32) {
    vm.mem.write_bytes(this + 0x0f, &[row as u8]);
    vm.mem.write_bytes(this + 0x10, &[index as u8]);
}

fn first_after(row: i32, index: i32, target: i32) -> i32 {
    (index - row).clamp(target - (VISIBLE_ROWS - 1), target).clamp(0, MAX_FIRST)
}

fn move_to(vm: &mut Vm, this: u32, target: i32) {
    let (row, index) = cursor(vm, this);
    let first = first_after(row, index, target);
    set_cursor(vm, this, target - first, target);
}

/// Install the in-game rows, or report that this binary cannot carry them.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for s in [
        DRAW_SYSTEM_MENU,
        KEY_SYSTEM_MENU,
        GET_SUB_STATE,
        NUM_KEY_TO_NAVI,
        TOUCH_SYSTEM_MENU,
        PROCESS,
        SET_POPUP_STATE,
        PAINT,
    ] {
        if vm.symbol(s).is_none() {
            return Err(format!("in-game option rows need `{s}`, not in this binary"));
        }
    }
    let draw = vm.symbol(DRAW_SYSTEM_MENU).expect("checked above");
    vm.install_hook(DRAW_SYSTEM_MENU, draw + ROW_FIRST_HEAD, row_hook)?;
    vm.install_hook(DRAW_SYSTEM_MENU, draw + ROW_TAIL_TEST, row_hook)?;
    let key = vm.symbol(KEY_SYSTEM_MENU).expect("checked above");
    vm.install_hook(KEY_SYSTEM_MENU, key, key_hook)?;
    let touch = vm.symbol(TOUCH_SYSTEM_MENU).expect("checked above");
    vm.install_hook(TOUCH_SYSTEM_MENU, touch + TOUCH_DOWN_CLAMP, down_clamp_hook)?;
    vm.install_hook(TOUCH_SYSTEM_MENU, touch + TOUCH_UP_WRAP_A, up_wrap_a_hook)?;
    let process = vm.symbol(PROCESS).expect("checked above");
    vm.install_hook(PROCESS, process + PROCESS_POPUP_ARM, popup_answer_hook)?;
    let paint = vm.symbol(PAINT).expect("checked above");
    vm.install_hook(PAINT, paint, paint_hook)?;
    Ok(vec![DRAW_SYSTEM_MENU, KEY_SYSTEM_MENU, TOUCH_SYSTEM_MENU, PROCESS, PAINT])
}

/// The menu's paint: the mapping screen while it is up over this menu.
fn paint_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    if vm.hudlayout.edit.active && vm.hudlayout.edit.scene == Scene::InGame {
        crate::hudedit::paint(vm)?;
        return Ok(true);
    }
    if !vm.padmap.active || vm.padmap.scene != Scene::InGame {
        return Ok(false);
    }
    let this = vm.arg(0);
    crate::padmap::paint(vm, this)?;
    Ok(true)
}

/// Both row tests. The slot is in `sb`, the y in `sl`; the display index is
/// recomputed from the cursor each time (see `options::row_hook`).
fn row_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let slot = vm.cpu.reg_get(Mode::User, 9) as i32;
    let y = vm.cpu.reg_get(Mode::User, 10) as i32;
    let sp = vm.cpu.reg_get(Mode::User, reg::SP);
    let this = vm.mem.read_u32(sp + SP_THIS);
    if this == 0 {
        return Ok(false);
    }
    let (row, index) = cursor(vm, this);
    let display = (index - row) + slot;
    match row_at(display) {
        Some(Row::Guest(g)) => {
            vm.cpu.reg_set(Mode::User, 5, g as u32);
            vm.cpu.reg_set(Mode::User, 8, sp + SP_LABELS + 2 * g as u32);
            vm.mem.write_u32(sp + SP_FIRST, (g - slot) as u32);
            Ok(false)
        }
        Some(Row::Host(h)) => {
            draw_row(vm, h, y, row == slot)?;
            vm.options.drawn += 1;
            let tail =
                vm.symbol(DRAW_SYSTEM_MENU).ok_or("igoptions: DrawSystemMenu vanished")? + ROW_TAIL;
            vm.cpu.reg_set(Mode::User, reg::LR, tail);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Every key on the in-game list.
fn key_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.arg(0);
    let key = vm.arg(1) as i32;
    if this == 0 {
        return Ok(false);
    }
    let navi = match vm.symbol(NUM_KEY_TO_NAVI) {
        Some(f) => vm.call(f, &[key as u32])? as i32,
        None => key,
    };
    if vm.padmap.active && vm.padmap.scene == Scene::InGame {
        crate::padmap::key(vm, navi)?;
        return Ok(true);
    }
    if vm.hudlayout.edit.active && vm.hudlayout.edit.scene == Scene::InGame {
        crate::hudedit::key(vm, navi)?;
        return Ok(true);
    }
    let Some(gss) = vm.symbol(GET_SUB_STATE) else { return Ok(false) };
    if vm.call(gss, &[this])? & 0xff != OPTION_SUBSTATE {
        return Ok(false);
    }
    let (_, index) = cursor(vm, this);
    match navi {
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
                match h {
                    HostRow::Filter => options::set_scale2x(vm, !vm.scale2x),
                    HostRow::PadKeys => {
                        if vm.padmap.shows_custom() {
                            crate::padmap::set_default(vm);
                        } else {
                            crate::padmap::ask_ingame(vm, this)?;
                        }
                    }
                    HostRow::HudLayout => {
                        if !vm.hudlayout.installed {
                            // 세로: nothing to lay out.
                        } else if vm.hudlayout.shows_custom() {
                            crate::hudlayout::set_default(vm);
                        } else {
                            crate::hudlayout::ask_ingame(vm, this)?;
                        }
                    }
                    HostRow::Resolution => {}
                    HostRow::SaveExport | HostRow::SaveImport | HostRow::SaveRestore => {
                        if let Some(a) = h.save_action() {
                            crate::savexfer::start(vm, Scene::InGame, this, a)?;
                        }
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        },
        Some(Row::Guest(g)) => {
            vm.mem.write_bytes(this + 0x10, &[g as u8]);
            let f = vm.symbol(KEY_SYSTEM_MENU).ok_or("igoptions: KeySystemMenu vanished")?;
            vm.call(f, &[this, key as u32])?;
            if vm.mem.read_u8(this + 0x10) as i8 as i32 == g {
                vm.mem.write_bytes(this + 0x10, &[index as u8]);
            }
            Ok(true)
        }
        None => Ok(false),
    }
}

/// The down arrow: `r3` is `first + 1`, which the guest lets go up to its own
/// last window and wraps past. A window past the guest's but inside ours
/// resumes at the scroll; everything else is the guest's.
fn down_clamp_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let next = vm.cpu.reg_get(Mode::User, 3) as i32;
    if !(GUEST_MAX_FIRST + 1..=MAX_FIRST).contains(&next) {
        return Ok(false);
    }
    let go = vm.symbol(TOUCH_SYSTEM_MENU).ok_or("igoptions: TouchSystemMenu vanished")? + TOUCH_DOWN_SCROLL;
    vm.cpu.reg_set(Mode::User, reg::LR, go);
    Ok(true)
}

/// An up-arrow wrap that writes index 7: write the last of ours instead.
fn up_wrap_a_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    vm.cpu.reg_set(Mode::User, 3, LAST as u32);
    let go = vm.symbol(TOUCH_SYSTEM_MENU).ok_or("igoptions: TouchSystemMenu vanished")?
        + TOUCH_UP_WRAP_A
        + 4;
    vm.cpu.reg_set(Mode::User, reg::LR, go);
    Ok(true)
}

/// Popup id 0x16's answer — ours only while the 패드키, 배치변경 or a save
/// row's question is up.
fn popup_answer_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let pad = vm.padmap.pending && vm.padmap.scene == Scene::InGame;
    let hud = vm.hudlayout.pending && vm.hudlayout.scene == Scene::InGame;
    let save = vm.savexfer.asking(Scene::InGame);
    if !pad && !hud && !save {
        return Ok(false);
    }
    let this = vm.cpu.reg_get(Mode::User, 4);
    if this == 0 {
        return Ok(false);
    }
    let yes = vm.mem.read_u8(this + POPUP_RESULT) == 0;
    if save {
        crate::savexfer::answered(vm, yes)?;
    } else if pad {
        crate::padmap::answered(vm, yes)?;
    } else {
        crate::hudlayout::answered(vm, yes)?;
    }
    vm.cpu.reg_set(Mode::User, 3, 1);
    let go = vm.symbol(PROCESS).ok_or("igoptions: Process vanished")? + PROCESS_POPUP_NO;
    vm.cpu.reg_set(Mode::User, reg::LR, go);
    Ok(true)
}

/// Raise the in-game menu's two-button popup (id 0x16) with `msg`.
pub(crate) fn ask_two_buttons(vm: &mut Vm, this: u32, msg: &'static [u8]) -> Result<bool, String> {
    let Some(p) = options::text(vm, msg) else { return Ok(false) };
    let f = vm.symbol(SET_POPUP_STATE).ok_or("igoptions: SetPopupState vanished")?;
    vm.call(f, &[this, POPUP_ID, p, 0, 0, 9, 0])?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_in_game_list_is_the_guests_eight_plus_six() {
        assert_eq!(ROWS, 14);
        let guests: Vec<i32> =
            ORDER.iter().filter_map(|r| if let Row::Guest(g) = r { Some(*g) } else { None }).collect();
        assert_eq!(guests, (0..GUEST_ROWS).collect::<Vec<_>>());
        assert_eq!(row_at(5), Some(Row::Host(HostRow::Filter)));
        assert_eq!(row_at(9), Some(Row::Host(HostRow::PadKeys)));
        assert_eq!(row_at(10), Some(Row::Host(HostRow::HudLayout)));
        assert_eq!(row_at(11), Some(Row::Host(HostRow::SaveExport)));
        assert_eq!(row_at(12), Some(Row::Host(HostRow::SaveImport)));
        assert_eq!(row_at(13), Some(Row::Host(HostRow::SaveRestore)));
        assert!(!ORDER.contains(&Row::Host(HostRow::Resolution)), "no restart prompt in game");
        assert_eq!(MAX_FIRST, 8);
        assert_eq!(GUEST_MAX_FIRST, 2, "the guest's own clamp: cmp r3, #2 at 0xFE5A4");
    }

    #[test]
    fn no_cursor_move_can_leave_the_window_out_of_range() {
        for first in 0..=MAX_FIRST {
            for row in 0..VISIBLE_ROWS {
                let index = first + row;
                for target in 0..ROWS {
                    let f = first_after(row, index, target);
                    assert!((0..=MAX_FIRST).contains(&f));
                    assert!((0..VISIBLE_ROWS).contains(&(target - f)));
                }
            }
        }
    }
}
