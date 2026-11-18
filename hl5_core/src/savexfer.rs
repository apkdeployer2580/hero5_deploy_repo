//! The three rows at the bottom of 환경설정 — 세이브 내보내기, 세이브
//! 불러오기, 세이브 되돌리기 — on the title screen and in the in-game 시스템
//! menu (ANALYSIS.md §70). The file and everything that touches the disk is
//! [`crate::savebundle`]; this is the conversation.
//!
//! ```text
//!   내보내기   (in game: the game's own HERO::SaveAll first, unless it forbids
//!              saving here) -> bundle -> the phone's "save as" -> OK popup
//!   불러오기   question -> the phone's "open" -> checked -> question naming the
//!              file's date and characters -> 예: staged, the game closes; the
//!              next start swaps the files and keeps the old ones in the vault
//!   되돌리기   question naming the newest vault generation -> 예: staged, closes
//! ```
//!
//! # Every popup is the game's own
//!
//! | | title (`StateMainMenu`) | in game (`StateInGameMenu`) |
//! |---|---|---|
//! | message | id 1 — one button, `common_text 0x38` = OK; `Process`'s arm (0x124A84) clears it on that answer | id 1 — the one `저장되었습니다.` uses (0x10AE60); its arm (0x10A77C) clears it |
//! | question | id 8, 예/아니오 — the answer is read in `options`' hook | id 0x16, 예/아니오 — read in `igoptions`' hook |
//!
//! # Raising one later
//!
//! The phone's file screens answer asynchronously, so their result is handed
//! to whichever menu asked at the entry of its next `Process` — and the popup
//! raised there is followed by an immediate return. Letting `Process` carry on
//! would be wrong: its first act is the switch on `GetPopupState`, and our id
//! with the fresh result 0 would read as "answered" and clear it in the same
//! call. The guest itself never raises a popup ahead of that switch; skipping
//! one tick of the menu's `Process` (its particles, its network poll) is the
//! whole cost. A `Process` entered with a popup of the guest's own already
//! answered is left alone that tick — that call is there to read its answer.

use std::path::PathBuf;

use crate::options::{self, entered_normally};
use crate::padmap::Scene;
use crate::savebundle as sb;
use crate::Vm;

/// What the shell is asked to do with the phone's document screens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocRequest {
    /// Copy `file` to a document the player names, suggesting `name`.
    Export { file: PathBuf, name: String },
    /// Copy a document the player picks to `file`, at most
    /// [`sb::BUNDLE_MAX`] + 1 bytes.
    Import { file: PathBuf },
}

/// How the shell's document screen ended. The codes are `Hl5Save.java`'s.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocStatus {
    Done,
    Cancelled,
    Failed,
}

impl DocStatus {
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => DocStatus::Done,
            1 => DocStatus::Cancelled,
            _ => DocStatus::Failed,
        }
    }
}

/// Which row was pressed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Export,
    Import,
    Restore,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Flow {
    #[default]
    Idle,
    /// Waiting for the shell's save-as. `fresh` is whether the game saved
    /// just before — false where it forbids saving, so the export is the last
    /// save and the message says so.
    Exporting { fresh: bool },
    /// The first question is up.
    AskImport,
    /// Waiting for the shell's open.
    Picking,
    /// The question naming the picked file is up.
    AskApply,
    /// The question naming the vault generation is up.
    AskRestore,
}

/// Per-`Vm` state.
pub struct SaveXfer {
    flow: Flow,
    scene: Scene,
    pub(crate) request: Option<DocRequest>,
    result: Option<DocStatus>,
    /// The bundle the last question is about.
    ready: Option<Vec<u8>>,
    /// A message waiting for the next `Process` entry — one raised from an
    /// answer hook, whose resumption clears the popup it would land in.
    message: Option<&'static [u8]>,
    /// A guest buffer for the two questions that carry a date.
    scratch: Option<u32>,
    /// The phone's offset from UTC, for the dates in the questions and the
    /// suggested file name. The shell sets it from `TimeZone`; the probe keeps
    /// Korea's.
    pub utc_offset_min: i32,
    pub exports: u64,
    pub picks: u64,
    pub staged: u64,
    pub messages: u64,
    pub questions: u64,
    pub saved_first: u64,
}

impl Default for SaveXfer {
    fn default() -> Self {
        Self {
            flow: Flow::Idle,
            scene: Scene::Title,
            request: None,
            result: None,
            ready: None,
            message: None,
            scratch: None,
            utc_offset_min: 9 * 60,
            exports: 0,
            picks: 0,
            staged: 0,
            messages: 0,
            questions: 0,
            saved_first: 0,
        }
    }
}

impl SaveXfer {
    /// One of our questions is the popup on `scene`'s screen.
    pub fn asking(&self, scene: Scene) -> bool {
        self.scene == scene && matches!(self.flow, Flow::AskImport | Flow::AskApply | Flow::AskRestore)
    }
}

// ---- guest symbols ------------------------------------------------------------

const MAIN_PROCESS: &str = "_ZN13StateMainMenu7ProcessEv";
const MAIN_GET_POPUP: &str = "_ZN13StateMainMenu13GetPopupStateEv";
const GAME_PROCESS: &str = "_ZN15StateInGameMenu7ProcessEv";
const GAME_GET_POPUP: &str = "_ZN15StateInGameMenu13GetPopupStateEv";
const GAME_SET_POPUP: &str = "_ZN15StateInGameMenu13SetPopupStateEaPciiiP8ItemInfo";
/// `HERO::SaveAll(signed char slot)` — what 시스템 > 저장하기 runs
/// (`StateInGameMenu::Process+0x810`), on `gv+0x1474` with `(i8)gv[0x1970]`.
const SAVE_ALL: &str = "_ZN4HERO7SaveAllEa";
const GV_HERO: u32 = 0x1474;
const GV_SLOT: u32 = 0x1970;
/// Non-zero where the game forbids saving: `KeySystemMenu+0x7c4` tests it
/// before it asks 저장하시겠습니까?, and shows `common_text 0xf7` — 저장/조합/
/// 합성/분해를 할 수 없는 지역입니다. — instead.
const GV_NO_SAVE: u32 = 0x1988;

const POPUP_MESSAGE: u32 = 1;
const GAME_POPUP_QUESTION: u32 = 0x16;

// ---- strings (EUC-KR; `;` is the popup's line break) ---------------------------

/// `세이브 내보내기`
pub(crate) const LABEL_EXPORT: &[u8] = &[
    0xbc, 0xbc, 0xc0, 0xcc, 0xba, 0xea, 0x20, 0xb3, 0xbb, 0xba, 0xb8, 0xb3, 0xbb, 0xb1, 0xe2,
    0x00,
];
/// `세이브 불러오기`
pub(crate) const LABEL_IMPORT: &[u8] = &[
    0xbc, 0xbc, 0xc0, 0xcc, 0xba, 0xea, 0x20, 0xba, 0xd2, 0xb7, 0xaf, 0xbf, 0xc0, 0xb1, 0xe2,
    0x00,
];
/// `세이브 되돌리기`
pub(crate) const LABEL_RESTORE: &[u8] = &[
    0xbc, 0xbc, 0xc0, 0xcc, 0xba, 0xea, 0x20, 0xb5, 0xc7, 0xb5, 0xb9, 0xb8, 0xae, 0xb1, 0xe2,
    0x00,
];
/// `불러온 세이브로 바뀝니다.;지금 세이브는 보관됩니다.;파일을 고르시겠습니까?`
const Q_IMPORT: &[u8] = &[
    0xba, 0xd2, 0xb7, 0xaf, 0xbf, 0xc2, 0x20, 0xbc, 0xbc, 0xc0, 0xcc, 0xba, 0xea, 0xb7, 0xce,
    0x20, 0xb9, 0xd9, 0xb2, 0xf2, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xc1, 0xf6, 0xb1, 0xdd,
    0x20, 0xbc, 0xbc, 0xc0, 0xcc, 0xba, 0xea, 0xb4, 0xc2, 0x20, 0xba, 0xb8, 0xb0, 0xfc, 0xb5,
    0xcb, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xc6, 0xc4, 0xc0, 0xcf, 0xc0, 0xbb, 0x20, 0xb0,
    0xed, 0xb8, 0xa3, 0xbd, 0xc3, 0xb0, 0xda, 0xbd, 0xc0, 0xb4, 0xcf, 0xb1, 0xee, 0x3f, 0x00,
];
/// `세이브를 내보냈습니다.`
const M_EXPORTED: &[u8] = &[
    0xbc, 0xbc, 0xc0, 0xcc, 0xba, 0xea, 0xb8, 0xa6, 0x20, 0xb3, 0xbb, 0xba, 0xb8, 0xb3, 0xc2,
    0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];
/// `저장할 수 없는 지역이라;마지막으로 저장한 세이브를;내보냈습니다.`
const M_EXPORTED_LAST: &[u8] = &[
    0xc0, 0xfa, 0xc0, 0xe5, 0xc7, 0xd2, 0x20, 0xbc, 0xf6, 0x20, 0xbe, 0xf8, 0xb4, 0xc2, 0x20,
    0xc1, 0xf6, 0xbf, 0xaa, 0xc0, 0xcc, 0xb6, 0xf3, 0x3b, 0xb8, 0xb6, 0xc1, 0xf6, 0xb8, 0xb7,
    0xc0, 0xb8, 0xb7, 0xce, 0x20, 0xc0, 0xfa, 0xc0, 0xe5, 0xc7, 0xd1, 0x20, 0xbc, 0xbc, 0xc0,
    0xcc, 0xba, 0xea, 0xb8, 0xa6, 0x3b, 0xb3, 0xbb, 0xba, 0xb8, 0xb3, 0xc2, 0xbd, 0xc0, 0xb4,
    0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];
/// `내보내지 못했습니다.`
const M_EXPORT_FAILED: &[u8] = &[
    0xb3, 0xbb, 0xba, 0xb8, 0xb3, 0xbb, 0xc1, 0xf6, 0x20, 0xb8, 0xf8, 0xc7, 0xdf, 0xbd, 0xc0,
    0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];
/// `내보낼 세이브가 없습니다.`
const M_NO_SAVE: &[u8] = &[
    0xb3, 0xbb, 0xba, 0xb8, 0xb3, 0xbe, 0x20, 0xbc, 0xbc, 0xc0, 0xcc, 0xba, 0xea, 0xb0, 0xa1,
    0x20, 0xbe, 0xf8, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];
/// `세이브 파일이 아닙니다.`
const M_BAD_FILE: &[u8] = &[
    0xbc, 0xbc, 0xc0, 0xcc, 0xba, 0xea, 0x20, 0xc6, 0xc4, 0xc0, 0xcf, 0xc0, 0xcc, 0x20, 0xbe,
    0xc6, 0xb4, 0xd5, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];
/// `캐릭터가 없는 세이브입니다.`
const M_NO_CHARACTER: &[u8] = &[
    0xc4, 0xb3, 0xb8, 0xaf, 0xc5, 0xcd, 0xb0, 0xa1, 0x20, 0xbe, 0xf8, 0xb4, 0xc2, 0x20, 0xbc,
    0xbc, 0xc0, 0xcc, 0xba, 0xea, 0xc0, 0xd4, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];
/// `파일을 읽지 못했습니다.`
const M_READ_FAILED: &[u8] = &[
    0xc6, 0xc4, 0xc0, 0xcf, 0xc0, 0xbb, 0x20, 0xc0, 0xd0, 0xc1, 0xf6, 0x20, 0xb8, 0xf8, 0xc7,
    0xdf, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];
/// `되돌릴 세이브가 없습니다.`
const M_NO_VAULT: &[u8] = &[
    0xb5, 0xc7, 0xb5, 0xb9, 0xb8, 0xb1, 0x20, 0xbc, 0xbc, 0xc0, 0xcc, 0xba, 0xea, 0xb0, 0xa1,
    0x20, 0xbe, 0xf8, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];
/// `적용하지 못했습니다.`
const M_STAGE_FAILED: &[u8] = &[
    0xc0, 0xfb, 0xbf, 0xeb, 0xc7, 0xcf, 0xc1, 0xf6, 0x20, 0xb8, 0xf8, 0xc7, 0xdf, 0xbd, 0xc0,
    0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x00,
];
/// ` 캐릭터 ` — the apply question's first line is `<date> 캐릭터 <n>명`,
/// exactly 27 bytes with a one-digit count.
const APPLY_CHARS: &[u8] = &[0x20, 0xc4, 0xb3, 0xb8, 0xaf, 0xc5, 0xcd, 0x20];
/// `명`
const APPLY_COUNT: &[u8] = &[0xb8, 0xed];
/// `;적용하려면 종료해야 합니다.;지금 종료하시겠습니까?`
const APPLY_TAIL: &[u8] = &[
    0x3b, 0xc0, 0xfb, 0xbf, 0xeb, 0xc7, 0xcf, 0xb7, 0xc1, 0xb8, 0xe9, 0x20, 0xc1, 0xbe, 0xb7,
    0xe1, 0xc7, 0xd8, 0xbe, 0xdf, 0x20, 0xc7, 0xd5, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xc1,
    0xf6, 0xb1, 0xdd, 0x20, 0xc1, 0xbe, 0xb7, 0xe1, 0xc7, 0xcf, 0xbd, 0xc3, 0xb0, 0xda, 0xbd,
    0xc0, 0xb4, 0xcf, 0xb1, 0xee, 0x3f,
];
/// ` 상태로` — the restore question's first line is `<date> 상태로`.
const RESTORE_SUFFIX: &[u8] = &[0x20, 0xbb, 0xf3, 0xc5, 0xc2, 0xb7, 0xce];
/// `;되돌리려면 종료해야 합니다.;지금 종료하시겠습니까?`
const RESTORE_TAIL: &[u8] = &[
    0x3b, 0xb5, 0xc7, 0xb5, 0xb9, 0xb8, 0xae, 0xb7, 0xc1, 0xb8, 0xe9, 0x20, 0xc1, 0xbe, 0xb7,
    0xe1, 0xc7, 0xd8, 0xbe, 0xdf, 0x20, 0xc7, 0xd5, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, 0x3b, 0xc1,
    0xf6, 0xb1, 0xdd, 0x20, 0xc1, 0xbe, 0xb7, 0xe1, 0xc7, 0xcf, 0xbd, 0xc3, 0xb0, 0xda, 0xbd,
    0xc0, 0xb4, 0xcf, 0xb1, 0xee, 0x3f,
];
/// The scratch buffer's size: the longest question is 27 + 52 bytes.
const SCRATCH: u32 = 128;

/// The apply question for a bundle exported at `stamp` with `characters`.
fn apply_question(stamp: &str, characters: usize) -> Vec<u8> {
    let mut v = stamp.as_bytes().to_vec();
    v.extend_from_slice(APPLY_CHARS);
    v.push(b'0' + characters.min(9) as u8);
    v.extend_from_slice(APPLY_COUNT);
    v.extend_from_slice(APPLY_TAIL);
    v
}

/// The restore question for a generation taken at `stamp`.
fn restore_question(stamp: &str) -> Vec<u8> {
    let mut v = stamp.as_bytes().to_vec();
    v.extend_from_slice(RESTORE_SUFFIX);
    v.extend_from_slice(RESTORE_TAIL);
    v
}

/// `영웅서기5_세이브_20261025_1430.h5save`: what the save-as screen offers.
fn suggested_name(stamp: &str) -> String {
    let digits: String = stamp.chars().filter(|c| c.is_ascii_digit()).collect();
    let (date, time) = digits.split_at(digits.len().min(8));
    format!("영웅서기5_세이브_{date}_{time}.{}", sb::EXT)
}

// ---- install ---------------------------------------------------------------------

/// Hook both menus' `Process` entries.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for s in [MAIN_PROCESS, MAIN_GET_POPUP, GAME_PROCESS, GAME_GET_POPUP, GAME_SET_POPUP, SAVE_ALL, options::GV] {
        if vm.symbol(s).is_none() {
            return Err(format!("save rows need `{s}`, not in this binary"));
        }
    }
    let main = vm.symbol(MAIN_PROCESS).expect("checked above");
    vm.install_hook(MAIN_PROCESS, main, main_process_hook)?;
    let game = vm.symbol(GAME_PROCESS).expect("checked above");
    vm.install_hook(GAME_PROCESS, game, game_process_hook)?;
    Ok(vec![MAIN_PROCESS, GAME_PROCESS])
}

fn main_process_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.arg(0);
    process_entry(vm, Scene::Title, this)
}

fn game_process_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.arg(0);
    process_entry(vm, Scene::InGame, this)
}

// ---- popups -------------------------------------------------------------------------

fn popup_state(vm: &mut Vm, scene: Scene, this: u32) -> Result<i8, String> {
    let sym = if scene == Scene::Title { MAIN_GET_POPUP } else { GAME_GET_POPUP };
    let f = vm.symbol(sym).ok_or("savexfer: GetPopupState vanished")?;
    Ok(vm.call(f, &[this])? as u8 as i8)
}

fn raise(vm: &mut Vm, scene: Scene, this: u32, id: u32, msg: u32) -> Result<(), String> {
    match scene {
        Scene::Title => {
            let f = vm.symbol(crate::screen::SET_POPUP_STATE).ok_or("savexfer: SetPopupState vanished")?;
            vm.call(f, &[this, id, msg, 0, 0, 9])?;
        }
        Scene::InGame => {
            let f = vm.symbol(GAME_SET_POPUP).ok_or("savexfer: SetPopupState vanished")?;
            vm.call(f, &[this, id, msg, 0, 0, 9, 0])?;
        }
    }
    Ok(())
}

fn question_id(scene: Scene) -> u32 {
    match scene {
        Scene::Title => crate::screen::POPUP_RESTART,
        Scene::InGame => GAME_POPUP_QUESTION,
    }
}

/// The one-button popup.
fn tell(vm: &mut Vm, scene: Scene, this: u32, msg: &'static [u8]) -> Result<(), String> {
    let Some(p) = options::text(vm, msg) else { return Ok(()) };
    raise(vm, scene, this, POPUP_MESSAGE, p)?;
    vm.savexfer.messages += 1;
    Ok(())
}

/// A two-button popup with a fixed body.
fn ask(vm: &mut Vm, scene: Scene, this: u32, msg: &'static [u8]) -> Result<bool, String> {
    let Some(p) = options::text(vm, msg) else { return Ok(false) };
    raise(vm, scene, this, question_id(scene), p)?;
    vm.savexfer.questions += 1;
    Ok(true)
}

/// A two-button popup with a built body, through the one scratch buffer —
/// one popup is ever up, so one buffer is enough, and a built string never
/// leaks a heap block per press.
fn ask_built(vm: &mut Vm, scene: Scene, this: u32, mut body: Vec<u8>) -> Result<bool, String> {
    let p = match vm.savexfer.scratch {
        Some(p) => p,
        None => match vm.heap.alloc(SCRATCH) {
            Some(p) => {
                vm.savexfer.scratch = Some(p);
                p
            }
            None => return Ok(false),
        },
    };
    body.truncate(SCRATCH as usize - 1);
    body.push(0);
    vm.mem.write_bytes(p, &body);
    raise(vm, scene, this, question_id(scene), p)?;
    vm.savexfer.questions += 1;
    Ok(true)
}

// ---- the rows ---------------------------------------------------------------------------

/// A save row was pressed.
pub fn start(vm: &mut Vm, scene: Scene, this: u32, action: Action) -> Result<(), String> {
    // A row can only be pressed with no popup and no document screen over
    // the list, so whatever the last transfer was waiting for is not coming —
    // a document screen that vanished without an answer would otherwise leave
    // every row dead until the app restarts.
    if vm.savexfer.flow != Flow::Idle {
        vm.log.push(format!("savexfer: dropping a stale {:?}", vm.savexfer.flow));
        vm.savexfer.flow = Flow::Idle;
    }
    vm.savexfer.scene = scene;
    vm.savexfer.result = None;
    vm.savexfer.ready = None;
    match action {
        Action::Export => export(vm, scene, this),
        Action::Import => {
            if ask(vm, scene, this, Q_IMPORT)? {
                vm.savexfer.flow = Flow::AskImport;
            }
            Ok(())
        }
        Action::Restore => restore(vm, scene, this),
    }
}

/// In game, run the game's own save first, so the file holds the moment the
/// row was pressed — unless the game forbids saving here, which the export
/// does not get to overrule.
fn save_first(vm: &mut Vm) -> Result<bool, String> {
    let gv = vm.symbol(options::GV).ok_or("savexfer: gv vanished")?;
    if vm.mem.read_u8(gv + GV_NO_SAVE) != 0 {
        vm.log.push("savexfer: the game forbids saving here; exporting the last save".to_string());
        return Ok(false);
    }
    let hero = vm.mem.read_u32(gv + GV_HERO);
    if hero == 0 {
        return Ok(false);
    }
    let slot = vm.mem.read_u8(gv + GV_SLOT) as i8 as i32 as u32;
    let f = vm.symbol(SAVE_ALL).ok_or("savexfer: SaveAll vanished")?;
    vm.call(f, &[hero, slot])?;
    vm.savexfer.saved_first += 1;
    Ok(true)
}

fn export(vm: &mut Vm, scene: Scene, this: u32) -> Result<(), String> {
    let fresh = match scene {
        Scene::InGame => save_first(vm)?,
        // Nothing is open at the title: the files are the save.
        Scene::Title => true,
    };
    let stamp = sb::local_stamp(sb::wall_ms(), vm.savexfer.utc_offset_min);
    let line = if vm.cfg.vanilla { "vanilla" } else { "main" };
    let snap = match sb::snapshot(&vm.cfg.storage, &stamp, line) {
        Some(s) if s.characters > 0 => s,
        _ => return tell(vm, scene, this, M_NO_SAVE),
    };
    let file = vm.cfg.storage.join(sb::EXPORT_OUT);
    if let Err(e) = sb::write_atomically(&file, &snap.bytes) {
        vm.log.push(format!("savexfer: cannot write the export: {e}"));
        return tell(vm, scene, this, M_EXPORT_FAILED);
    }
    vm.log.push(format!(
        "savexfer: export of {} character(s), {} bytes, ready for the save-as screen",
        snap.characters,
        snap.bytes.len()
    ));
    vm.savexfer.request = Some(DocRequest::Export { file, name: suggested_name(&stamp) });
    vm.savexfer.flow = Flow::Exporting { fresh };
    vm.savexfer.exports += 1;
    Ok(())
}

fn restore(vm: &mut Vm, scene: Scene, this: u32) -> Result<(), String> {
    let Some((ms, path)) = sb::newest_vault(&vm.cfg.storage) else {
        return tell(vm, scene, this, M_NO_VAULT);
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) if sb::parse(&b).is_ok() => b,
        _ => {
            vm.log.push(format!("savexfer: {} is not a usable generation", path.display()));
            return tell(vm, scene, this, M_BAD_FILE);
        }
    };
    let stamp = sb::local_stamp(ms, vm.savexfer.utc_offset_min);
    if ask_built(vm, scene, this, restore_question(&stamp))? {
        vm.savexfer.ready = Some(bytes);
        vm.savexfer.flow = Flow::AskRestore;
    }
    Ok(())
}

/// One of our questions was answered; called from the menus' answer hooks,
/// which then clear the popup the way the guest's 아니오 does.
pub fn answered(vm: &mut Vm, yes: bool) -> Result<(), String> {
    let flow = std::mem::take(&mut vm.savexfer.flow);
    match flow {
        Flow::AskImport if yes => {
            let file = vm.cfg.storage.join(sb::IMPORT_IN);
            let _ = std::fs::remove_file(&file);
            vm.savexfer.request = Some(DocRequest::Import { file });
            vm.savexfer.flow = Flow::Picking;
            vm.savexfer.picks += 1;
        }
        Flow::AskApply | Flow::AskRestore if yes => {
            let bytes = vm.savexfer.ready.take().unwrap_or_default();
            match sb::stage(&vm.cfg.storage, &bytes) {
                Ok(()) => {
                    vm.savexfer.staged += 1;
                    vm.log.push(format!(
                        "savexfer: {} staged for the next start; closing",
                        if flow == Flow::AskApply { "import" } else { "restore" }
                    ));
                    vm.exit_requested = true;
                }
                Err(e) => {
                    vm.log.push(format!("savexfer: cannot stage: {e}"));
                    vm.savexfer.message = Some(M_STAGE_FAILED);
                }
            }
        }
        _ => {
            vm.savexfer.ready = None;
            vm.log.push(format!("savexfer: 아니오 ({flow:?})"));
        }
    }
    Ok(())
}

/// The shell's document screen ended.
pub fn done(vm: &mut Vm, status: DocStatus) {
    if matches!(vm.savexfer.flow, Flow::Exporting { .. } | Flow::Picking) {
        vm.log.push(format!("savexfer: document screen {status:?}"));
        vm.savexfer.result = Some(status);
    } else {
        vm.log.push(format!("savexfer: a document result ({status:?}) with nothing waiting for it"));
    }
}

/// The asking menu's `Process` entry: a waiting message, or the document
/// screen's result. `true` returns from `Process` at once — see the module
/// docs.
fn process_entry(vm: &mut Vm, scene: Scene, this: u32) -> Result<bool, String> {
    let x = &vm.savexfer;
    if this == 0 || x.scene != scene || (x.message.is_none() && x.result.is_none()) {
        return Ok(false);
    }
    if popup_state(vm, scene, this)? != 0 {
        return Ok(false);
    }
    if let Some(m) = vm.savexfer.message.take() {
        tell(vm, scene, this, m)?;
        return Ok(true);
    }
    let Some(status) = vm.savexfer.result.take() else { return Ok(false) };
    let flow = std::mem::take(&mut vm.savexfer.flow);
    match flow {
        Flow::Exporting { fresh } => {
            let _ = std::fs::remove_file(vm.cfg.storage.join(sb::EXPORT_OUT));
            match status {
                DocStatus::Done => tell(vm, scene, this, if fresh { M_EXPORTED } else { M_EXPORTED_LAST })?,
                DocStatus::Cancelled => return Ok(false),
                DocStatus::Failed => tell(vm, scene, this, M_EXPORT_FAILED)?,
            }
        }
        Flow::Picking => match status {
            DocStatus::Done => picked(vm, scene, this)?,
            DocStatus::Cancelled => return Ok(false),
            DocStatus::Failed => tell(vm, scene, this, M_READ_FAILED)?,
        },
        _ => return Ok(false),
    }
    Ok(true)
}

/// The picked file is in [`sb::IMPORT_IN`]: check it and ask.
fn picked(vm: &mut Vm, scene: Scene, this: u32) -> Result<(), String> {
    let file = vm.cfg.storage.join(sb::IMPORT_IN);
    let bytes = std::fs::read(&file);
    let _ = std::fs::remove_file(&file);
    let bytes = match bytes {
        Ok(b) => b,
        Err(e) => {
            vm.log.push(format!("savexfer: the picked file did not arrive: {e}"));
            return tell(vm, scene, this, M_READ_FAILED);
        }
    };
    match sb::parse_import(&bytes) {
        Ok(b) => {
            vm.log.push(format!(
                "savexfer: picked a save exported {:?} — {} character(s), {} file(s)",
                b.exported,
                b.characters,
                b.files.len()
            ));
            if ask_built(vm, scene, this, apply_question(&b.exported, b.characters))? {
                vm.savexfer.ready = Some(bytes);
                vm.savexfer.flow = Flow::AskApply;
            }
            Ok(())
        }
        Err(sb::Reject::NoCharacter) => tell(vm, scene, this, M_NO_CHARACTER),
        Err(sb::Reject::NotBundle(why)) => {
            vm.log.push(format!("savexfer: the picked file is not a save: {why}"));
            tell(vm, scene, this, M_BAD_FILE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walk EUC-KR and split on `;`, failing on a half syllable.
    fn runs(msg: &[u8]) -> Vec<&[u8]> {
        let body = msg.strip_suffix(&[0]).unwrap_or(msg);
        assert!(!body.contains(&0), "an interior NUL truncates the popup");
        let mut i = 0;
        while i < body.len() {
            if body[i] >= 0x80 {
                let t = *body.get(i + 1).expect("a lead byte with no trail byte");
                assert!((0xa1..=0xfe).contains(&t), "trail byte {t:#04x} at {i}");
                i += 2;
            } else {
                i += 1;
            }
        }
        body.split(|b| *b == b';').collect()
    }

    /// The game's widest popup run is 27 bytes (screen.rs's `POPUP_RUN_MAX`);
    /// every message and both built questions stay inside it, with the widest
    /// count a slot table holds.
    #[test]
    fn every_popup_run_fits_the_popup() {
        let fixed: [&[u8]; 10] = [
            Q_IMPORT,
            M_EXPORTED,
            M_EXPORTED_LAST,
            M_EXPORT_FAILED,
            M_NO_SAVE,
            M_BAD_FILE,
            M_NO_CHARACTER,
            M_READ_FAILED,
            M_NO_VAULT,
            M_STAGE_FAILED,
        ];
        let stamp = "2026-10-25 14:30";
        let built = [apply_question(stamp, 9), restore_question(stamp)];
        for m in fixed.iter().copied().chain(built.iter().map(|v| v.as_slice())) {
            for (i, r) in runs(m).iter().enumerate() {
                assert!(!r.is_empty() && r.len() <= 27, "run {i} of {m:02x?} is {} bytes", r.len());
                assert_ne!(r[0], b'^', "a colour escape would eat the line");
            }
        }
        assert_eq!(runs(&apply_question(stamp, 3))[0].len(), 27);
        assert_eq!(runs(&apply_question(stamp, 3)).len(), 3);
        assert_eq!(runs(&restore_question(stamp)).len(), 3);
        for l in [LABEL_EXPORT, LABEL_IMPORT, LABEL_RESTORE] {
            assert_eq!(runs(l).len(), 1);
            assert!(l.len() - 1 <= 15, "seven hangul and a space fit the plate without a value box");
        }
    }

    #[test]
    fn the_suggested_name_carries_the_stamp() {
        assert_eq!(suggested_name("2026-10-25 14:30"), "영웅서기5_세이브_20261025_1430.h5save");
    }

    #[test]
    fn document_status_codes_are_the_java_ones() {
        assert_eq!(DocStatus::from_code(0), DocStatus::Done);
        assert_eq!(DocStatus::from_code(1), DocStatus::Cancelled);
        assert_eq!(DocStatus::from_code(2), DocStatus::Failed);
        assert_eq!(DocStatus::from_code(-7), DocStatus::Failed);
    }
}
