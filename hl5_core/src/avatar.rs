//! 아바타 — a wardrobe behind a new button on the character page.
//!
//! The player asked for an `아바타` button at the left end of the character
//! page's bottom bar, the one that holds `#상세`, opening costumes drawn for
//! this game's sprites the way the sibling hero3 has them. This module is the
//! button, the wardrobe and the mechanism that dresses the hero; the costumes
//! themselves are data ([`Look`]).
//!
//! # The button is the guest's own slot
//!
//! `StateInGameMenu::DrawCharacterMenu` (0xFBC9C) lays the page out from
//! `x0 = GetHalfWidth() - 0x58`, `y0 = GetHalfHeight() - 0x66`, and its bottom
//! bar has two `CommonUi::DrawHalfBox` plates, `0x45 x 0x11` at `y0 + 0xa2`:
//! `*취소` at `x0` and `#상세` at `x0 + 0x6b`. The left one is drawn **only
//! while stat points are pending** (`this+0x20..0x26`, four s16, any `> 0`),
//! but `TouchCharacterMenu` (0x103ACC) answers a tap there on both sub-pages
//! whether or not it is drawn, with `KeyPress('*')`. So the slot is free, it is
//! already tappable, and it already speaks a key the keypad and the phone have:
//! the button is drawn there whenever `*취소` is not, and [`key_hook`] takes
//! `'*'` exactly when the guest's `CancelApplayStat` would have had nothing to
//! cancel. The label is `*아바타`, the same form as the guest's `*취소` and
//! `#상세`.
//!
//! # The wardrobe sits over the stat block
//!
//! Opened, it paints over the page between the header's rule and the bar
//! (`y0+0x40 .. y0+0xa0`): six slot tabs and a four-row list, with the guest's
//! own `DrawHalfBox` plates and `BFont::DrawText`, so it is the game's chrome
//! and the game's font. The header stays — and the header is where the guest
//! draws the hero (`CHAR::GetSpritePtr` + `SPRITE::DrawSprite`), so picking a
//! row re-dresses that figure on the spot.
//!
//! # A drag scrolls the list
//!
//! The guest's menus never scroll by drag: `StateInGameMenu::TouchMoved`
//! (0xE649C) and `TouchReleased` (0xE6498) are a lone `bx lr` each, the
//! vtable's `+0x2c` / `+0x28`, and `mainKeyPressed` still dispatches to them
//! once per tick from the move and release mailboxes. So a press on the list
//! only *arms* ([`touch_hook`]): a move past [`DRAG_SLOP`] scrolls by whole
//! rows ([`move_hook`]), and a release that never moved is the tap that wears
//! the row ([`release_hook`]). Both hooks decline — the guest's `bx lr` runs.
//! `BaseState::TouchMove` keeps the *first* move of a tick, not the last, and
//! `onHandleEvent` posts one at the press point with every press, so the
//! scroll is computed from where the finger is against where it went down,
//! never by accumulating steps, and the release point has the last word.
//!
//! # Dressing is a substitution at the one place the hero's sheets are named
//!
//! `HERO::SetLoadImgPath` (0x8DE58) is the only builder of the hero's layer
//! paths: it clears `_szSprFile` (ten `0x100`-byte strings) and `_nSprPalFile`
//! (ten s16), then writes `/c/sp/img<N>/%03d.mgr` and the palette from each
//! equipped item's `ItemInfo+0x150` / `+0x152`. Its callers are the four
//! `<CLASS>::LoadSprite` (game load) and `HERO::ChangeSpriteImage` (every
//! re-equip), and the sprite it feeds is the one the field *and* the menu
//! preview draw. [`load_hook`] runs it and then overwrites the layers the
//! player has dressed. Nothing is written to an item — `+0x150` is not saved
//! (§45) and an avatar is not an item.
//!
//! The 투구보기 toggle is the one path that bypasses it: `KeySystemMenu` writes
//! layer 0 itself and calls `SPRITE_IMAGE::ChangeSprImg` (0xDB01C) directly.
//! [`swap_hook`] rewrites that layer before the call and declines. A story
//! scene bypasses it another way — it rebuilds the hero as an NPC body through
//! `Map::NpcPathSet`, which names the same layers from the equipment itself;
//! [`crate::npcbody`] dresses that copy.
//!
//! # A rigged costume brings its own animation table
//!
//! The astra costumes carry pose images after the game's own (hair that lags
//! the bob, a hem that swings with the stride — [`crate::costume`]) and only
//! move when the hero's cif names them: a variant served at boot. The cif is
//! read once, in `<CLASS>::LoadSprite`, by `SPRITE_DATA::LoadSprFile` on the
//! hero's `SPRITE_DATA` (`gv+0x1468`), right after `SetLoadImgPath`; a re-dress
//! (`HERO::ChangeSpriteImage`) reloads only the sheets. So [`load_hook`]
//! records which of the head and body it dressed with a rigged sheet,
//! [`cif_hook`] hands `LoadSprFile` the matching variant, and [`change_hook`]
//! re-reads the cif after a re-dress whenever that pair changed —
//! `FreeSpriteData` + `LoadSprFile` on the same object, so every pointer to
//! it stays good, and every count and offset in it is the archive's. The
//! variant and the sheets change together inside one guest call, so no frame
//! is ever drawn with a variant index and a sheet that lacks it.
//!
//! # What is in the wardrobe
//!
//! Every look is a `(sheet, palette)` pair for one layer. The first catalogue
//! is built from the game's own tables at boot ([`Avatar::new`]): the fourteen
//! sets' helmet/armour/glove/boot looks, and the legendary and set weapons and
//! shields — the art the game already has, under the names it already uses.
//! Costume sheets drawn for this project join the same list as [`Look`]s whose
//! sheet number is one the archive does not ship and whose file is served from
//! the host shadow like every other added resource.
//!
//! # Persistence
//!
//! `<files>/hl5_avatar.txt`, one line per save slot (the slot is
//! `(i8)gv[0x1970]`, the argument `Battle::KeyPressed` hands `HERO::SaveAll`),
//! recording the class it was dressed for. A slot whose hero is now another
//! class — a new character, or a story scene that hands the player someone
//! else — is not dressed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use arm32_cpu::{reg, Mode};

use crate::mem::RETURN_MAGIC;
use crate::Vm;

/// `<files>/hl5_avatar.txt`.
pub const AVATAR_FILE: &str = "hl5_avatar.txt";

const DRAW_CHARACTER_MENU: &str = "_ZN15StateInGameMenu17DrawCharacterMenuEv";
const KEY_CHARACTER_MENU: &str = "_ZN15StateInGameMenu16KeyCharacterMenuEi";
const TOUCH_CHARACTER_MENU: &str = "_ZN15StateInGameMenu18TouchCharacterMenuEii";
const TOUCH_MOVED: &str = "_ZN15StateInGameMenu10TouchMovedEiii";
const TOUCH_RELEASED: &str = "_ZN15StateInGameMenu13TouchReleasedEiii";
const SET_SUB_STATE: &str = "_ZN15StateInGameMenu11SetSubStateEa";
const SET_LOAD_IMG_PATH: &str = "_ZN4HERO14SetLoadImgPathEh";
const CHANGE_SPRITE_IMAGE: &str = "_ZN4HERO17ChangeSpriteImageEh";
pub(crate) const CHANGE_SPR_IMG: &str = "_ZN12SPRITE_IMAGE12ChangeSprImgEP11SPRITE_DATAaPc";
const IS_NONE_EQUIP_WEAPON: &str = "_ZN4HERO17IsNoneEquipWeaponEv";
const LOAD_SPR_FILE: &str = "_ZN11SPRITE_DATA11LoadSprFileEPch";
const FREE_SPRITE_DATA: &str = "_ZN11SPRITE_DATA14FreeSpriteDataEv";
pub(crate) const SPR_FILE: &str = "_szSprFile";
pub(crate) const SPR_PAL: &str = "_nSprPalFile";

const GET_HALF_WIDTH: &str = "_ZN7Graphic12GetHalfWidthEv";
const GET_HALF_HEIGHT: &str = "_ZN7Graphic13GetHalfHeightEv";
const GET_FB: &str = "_ZN7Graphic5GetFBEv";
pub(crate) const FILL_RECT: &str = "_ZN7Graphic8FillRectEiiiiii";
const DRAW_HALF_BOX: &str = "_ZN8CommonUi11DrawHalfBoxEiiiiiahh";
const DRAW_TEXT: &str = "_ZN5BFont8DrawTextEiiiPcii";
const LCD_G: &str = "_lcdG";
const P_CFONT: &str = "_pCFont";
const GV: &str = "gv";

/// `gv + 0x15FC` is the `CommonUi*` every plate is drawn through.
const GV_COMMON_UI: u32 = 0x15fc;
/// `gv + 0x1474` is the player's `HERO*`.
const GV_HERO: u32 = 0x1474;
/// `gv + 0x1468` is the hero's `SPRITE_DATA*` — `<CLASS>::LoadSprite` stores
/// it there before `LoadSprFile`, `HERO::ChangeSpriteImage` reads it.
const GV_HERO_SPRITE_DATA: u32 = 0x1468;
/// `(i8)gv[0x1970]` is the save slot in play (`Battle::KeyPressed+0x1a0`).
const GV_SLOT: u32 = 0x1970;
/// `HERO + 0x22c` is the class: 0 워리어, 1 로그, 2 건슬링어, 3 나이트 — the
/// same byte `SetLoadImgPath` picks the class effect sheet `img5/00N` with
/// (sword trails, flames, the buff labels — not the body: the body is the
/// head, torso, arms and legs layers 0..3, and those are what one hero shares).
const HERO_CLASS: u32 = 0x22c;

/// `StateInGameMenu` fields: the page (1 = 캐릭터), its sub-page (0 기본,
/// 1 상세), and the four pending stat points that make the guest draw `*취소`.
const MENU_STATE: u32 = 0x0c;
const MENU_SUBPAGE: u32 = 0x0d;
const MENU_PENDING: [u32; 4] = [0x20, 0x22, 0x24, 0x26];
const PAGE_CHARACTER: u8 = 1;

/// WIPI keys this page answers.
pub(crate) const K_UP: i32 = -1;
pub(crate) const K_DOWN: i32 = -2;
pub(crate) const K_LEFT: i32 = -3;
pub(crate) const K_RIGHT: i32 = -4;
pub(crate) const K_OK: i32 = -5;
pub(crate) const K_CLEAR: i32 = -16;
pub(crate) const K_STAR: i32 = 0x2a;
pub(crate) const K_HASH: i32 = 0x23;
/// `'0'`: unhandled by `KeyCharacterMenu`, and what a pad's L2 sends — the
/// second way in for a pad, whose `*` becomes `#` off the field.
pub(crate) const K_ZERO: i32 = 0x30;

// ---- geometry, every number relative to the page's own (x0, y0) -----------

/// The bottom bar's left plate — the guest's own `*취소` rectangle.
pub(crate) const BAR_Y: i32 = 0xa2;
pub(crate) const BAR_W: i32 = 0x45;
pub(crate) const BAR_H: i32 = 0x11;
/// `TouchCharacterMenu`'s band for both bar slots: `y0+0xa1 < y <= y0+0xb3`.
const BAR_HIT: (i32, i32) = (0xa2, 0xb3);
/// Label: centred at `x0 + 0x23`, `y0 + 0xa5`, like `*취소`.
pub(crate) const BAR_LABEL: (i32, i32) = (0x23, 0xa5);
/// The block the wardrobe covers: under the header's rule, over the bar.
const AREA: (i32, i32, i32, i32) = (2, 0x40, 0xac, 0x60);
/// Tabs: six plates in a row.
const TAB_Y: i32 = 0x44;
const TAB_X: i32 = 4;
const TAB_PITCH: i32 = 28;
const TAB_W: i32 = 27;
/// Rows: four plates under the tabs.
const ROW_Y: i32 = 0x58;
pub(crate) const ROW_PITCH: i32 = 18;
const ROW_W: i32 = 0xa4;
const ROWS: usize = 4;
pub(crate) const PLATE_H: i32 = 0x11;
/// The scroll bar right of the rows: its track spans the four plates.
const BAR_X: i32 = TAB_X + ROW_W + 2;
pub(crate) const SCROLL_W: i32 = 3;
pub(crate) const TRACK: u32 = 0x0028_3a50;
pub(crate) const THUMB: u32 = 0x00a8_bccf;
/// How far a finger travels before a press on the list is a drag and not a
/// tap, in canvas pixels (four device pixels each on the shipped canvases).
pub(crate) const DRAG_SLOP: i32 = 6;
/// The page's own background, measured off the panel: (16, 28, 41).
pub(crate) const PANEL_BG: u32 = 0x0010_1c29;

/// White for what is under the cursor, grey for the rest — the two the
/// guest's own lists use — and the guest's orange for "worn".
pub(crate) const FG: u32 = 0x00ff_ffff;
pub(crate) const FG_DIM: u32 = 0x0080_8080;
pub(crate) const FG_WORN: u32 = 0x00ff_a500;

// ---- strings, EUC-KR, NUL-terminated --------------------------------------

pub(crate) const LABEL_BUTTON: &[u8] = &[0x2a, 0xbe, 0xc6, 0xb9, 0xd9, 0xc5, 0xb8, 0x00]; // *아바타
pub(crate) const LABEL_NONE: &[u8] = &[0xb1, 0xe2, 0xba, 0xbb, 0x20, 0xbf, 0xdc, 0xc7, 0xfc, 0x00]; // 기본 외형
pub(crate) const LABEL_WORN: &[u8] = &[0xc2, 0xf8, 0xbf, 0xeb, 0x00]; // 착용

/// One tab: the `_szSprFile` layer it dresses, the `img<N>` directory that
/// layer's sheets live in, and its label.
pub(crate) struct Tab {
    pub(crate) layer: u8,
    dir: u8,
    label: &'static [u8],
}

pub(crate) const TABS: [Tab; 6] = [
    Tab { layer: 0, dir: 0, label: &[0xc5, 0xf5, 0xb1, 0xb8, 0x00] }, // 투구
    Tab { layer: 1, dir: 1, label: &[0xb0, 0xa9, 0xbf, 0xca, 0x00] }, // 갑옷
    Tab { layer: 2, dir: 2, label: &[0xc0, 0xe5, 0xb0, 0xa9, 0x00] }, // 장갑
    Tab { layer: 3, dir: 3, label: &[0xbd, 0xc5, 0xb9, 0xdf, 0x00] }, // 신발
    Tab { layer: 4, dir: 4, label: &[0xb9, 0xab, 0xb1, 0xe2, 0x00] }, // 무기
    Tab { layer: 6, dir: 6, label: &[0xb9, 0xe6, 0xc6, 0xd0, 0x00] }, // 방패
];

/// How many layers `_szSprFile` has room for.
const LAYERS: usize = 10;
const WEAPON_LAYER: u8 = 4;
/// `img5/00<class>.mgr`: the class's slashes, flames and buff labels.
const FX_LAYER: u8 = 5;
const SHIELD_LAYER: u8 = 6;

/// One thing the hero can look like in one layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Look {
    /// Stable across builds, because it is what the file records: for a look
    /// taken from the game's tables it is `kind * 1000 + row`.
    pub id: u32,
    /// EUC-KR, NUL-terminated.
    pub name: Vec<u8>,
    pub layer: u8,
    /// `/c/sp/img<dir>/<sheet>.mgr`.
    pub sheet: u16,
    /// What goes into `_nSprPalFile[layer]`.
    pub palette: i16,
    /// Bit `c` set: class `c` may wear it.
    pub classes: u8,
    /// A rigged sheet: animates through the rig's cif variant.
    pub rig: bool,
    /// For a weapon: the class effect sheet (`img5/<fx>.mgr`) dressed with
    /// it, so the slashes and flames take the avatar's colour. 0 = none.
    pub fx: u16,
}

/// Classes, as `HERO+0x22c` numbers them.
const ALL_CLASSES: u8 = 0b1111;
const WARRIOR_KNIGHT: u8 = 0b1001;
const KNIGHT: u8 = 0b1000;

/// What one save slot is wearing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SlotWear {
    /// The class it was dressed as. A slot whose hero is another class now is
    /// not dressed — see the module header.
    class: u8,
    /// Look id per layer, 0 for none.
    worn: [u32; LAYERS],
    /// The outfit the spirit 렌 wears in this slot ([`SpiritLook::id`]), 0
    /// for her own. Kept under the same class rule as the hero's layers: a
    /// slot that now holds another hero starts her undressed too.
    spirit: u32,
}

impl SlotWear {
    fn new(class: u8) -> Self {
        Self { class, worn: [0; LAYERS], spirit: 0 }
    }
}

/// A finger that went down on the list.
#[derive(Clone, Copy, Debug)]
struct Drag {
    /// Where it went down, and the list's first row then.
    y: i32,
    first: usize,
    /// The row under it, if any: worn on release unless it moved.
    row: Option<usize>,
    /// Past [`DRAG_SLOP`]: a scroll, not a tap.
    moved: bool,
}

/// One outfit for the spirit 렌: a head sheet and a body sheet drawn for her,
/// served under one number and worn together (`spirit_avatar`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpiritLook {
    /// The head costume's catalogue id — what `s=` records.
    pub id: u32,
    /// EUC-KR, NUL-terminated.
    pub name: Vec<u8>,
    /// `/c/sp/img0/<sheet>.mgr` and `/c/sp/img1/<sheet>.mgr`.
    pub sheet: u16,
}

/// Per-`Vm` state.
pub struct Avatar {
    pub looks: Vec<Look>,
    /// 렌's outfits, in catalogue order.
    pub spirit_looks: Vec<SpiritLook>,
    /// Her wardrobe on the 정령 page.
    pub(crate) spirit: crate::spirit_avatar::Wardrobe,
    slots: HashMap<i8, SlotWear>,
    file: PathBuf,
    /// The wardrobe is up.
    open: bool,
    tab: usize,
    cursor: usize,
    first: usize,
    drag: Option<Drag>,
    /// `guest_repaints()` when the character page was last drawn, so a page
    /// left and re-entered starts closed.
    last_draw: u64,
    strings: HashMap<usize, u32>,
    names: HashMap<u32, u32>,
    /// The rig's cif variants are served (`costume::install`).
    rig_cifs: bool,
    /// What the last `SetLoadImgPath` for the hero dressed rigged: (head, body).
    dressed_rig: (bool, bool),
    /// What the hero's `SPRITE_DATA` was last loaded with: the object, the
    /// class cif it asked for, and the rigged pair it got.
    cif_loaded: Option<(u32, u8, (bool, bool))>,
    /// Cif paths in the guest heap, once each.
    cif_paths: HashMap<String, u32>,
    /// How many times a cif was read for the rig (load or re-read).
    pub cif_swaps: u64,
    /// For the boot log and the probe.
    pub buttons_drawn: u64,
    pub pages_drawn: u64,
    pub opened: u64,
    pub changes: u64,
    pub dressed: u64,
    /// Rows the list moved by drag.
    pub scrolled: u64,
    /// NPC-body copies of the hero dressed in a scene (`npcbody`).
    pub bodies_dressed: u64,
}

impl Avatar {
    /// Build the catalogue from the archive and read the saved wardrobe.
    pub fn new(vfs: &[u8], storage: &Path) -> (Self, Vec<String>) {
        let mut logs = Vec::new();
        // The costumes drawn for this project first, then the game's own
        // set and legendary looks. A costume joins only if its sheet was
        // actually composed and written — see `costume::install`.
        let mut looks = Vec::new();
        let mut spirit_looks = Vec::new();
        let mut rig_cifs = false;
        if !vfs.is_empty() {
            let (costumes, rig, costume_logs) = crate::costume::install(vfs, storage);
            rig_cifs = rig;
            logs.extend(costume_logs);
            // 렌's outfits are hers alone; the hero never lists them.
            let (spirit, costumes): (Vec<_>, Vec<_>) =
                costumes.into_iter().partition(|c| c.classes == crate::costume::SPIRIT);
            spirit_looks = crate::spirit_avatar::outfits(&spirit);
            looks.extend(costumes.into_iter().map(|c| Look {
                id: c.id,
                name: c.name.to_vec(),
                layer: c.layer,
                sheet: c.sheet,
                // Its own colours: every shipped item carries a `.pal` remap
                // (even `000.pal` moves five colours), and one written for
                // the archive's art has no business on ours.
                palette: -1,
                // One hero wears all four classes' heads, arms and legs; only
                // the weapon and shield layers are a class's own.
                classes: c.classes,
                rig: c.rig,
                fx: c.fx,
            }));
        }
        looks.extend(table_looks(vfs));
        let file = storage.join(AVATAR_FILE);
        let slots = match std::fs::read_to_string(&file) {
            Ok(text) => parse_file(&text),
            Err(_) => HashMap::new(),
        };
        let per_tab: Vec<String> = TABS
            .iter()
            .map(|t| looks.iter().filter(|l| l.layer == t.layer).count().to_string())
            .collect();
        logs.push(format!(
            "avatar: {} look(s) ({} per tab), {} outfit(s) for 렌, {} slot(s) dressed in {}",
            looks.len(),
            per_tab.join("/"),
            spirit_looks.len(),
            slots.len(),
            file.display()
        ));
        (
            Self {
                looks,
                spirit_looks,
                spirit: Default::default(),
                slots,
                file,
                open: false,
                tab: 0,
                cursor: 0,
                first: 0,
                drag: None,
                last_draw: 0,
                strings: HashMap::new(),
                names: HashMap::new(),
                rig_cifs,
                dressed_rig: (false, false),
                cif_loaded: None,
                cif_paths: HashMap::new(),
                cif_swaps: 0,
                buttons_drawn: 0,
                pages_drawn: 0,
                opened: 0,
                changes: 0,
                dressed: 0,
                scrolled: 0,
                bodies_dressed: 0,
            },
            logs,
        )
    }

    /// The rows of the current tab for `class`: `None` (기본 외형) first.
    fn rows(&self, class: u8) -> Vec<Option<usize>> {
        let layer = TABS[self.tab].layer;
        let mut rows = vec![None];
        rows.extend(
            self.looks
                .iter()
                .enumerate()
                .filter(|(_, l)| l.layer == layer && l.classes & (1 << class) != 0)
                .map(|(i, _)| Some(i)),
        );
        rows
    }

    /// What `slot`'s `class` hero wears in `layer`, if anything.
    pub(crate) fn worn(&self, slot: i8, class: u8, layer: u8) -> Option<&Look> {
        let s = self.slots.get(&slot)?;
        if s.class != class {
            return None;
        }
        let id = s.worn[layer as usize];
        if id == 0 {
            return None;
        }
        self.looks.iter().find(|l| l.id == id && l.classes & (1 << class) != 0)
    }

    fn save(&self) -> std::io::Result<()> {
        std::fs::write(&self.file, format_file(&self.slots))
    }

    /// The outfit 렌 wears in `slot` while its hero is `class`, if any.
    pub(crate) fn spirit_worn(&self, slot: i8, class: u8) -> Option<&SpiritLook> {
        let s = self.slots.get(&slot)?;
        if s.class != class || s.spirit == 0 {
            return None;
        }
        self.spirit_looks.iter().find(|l| l.id == s.spirit)
    }

    /// Record 렌's outfit for `slot` (0 = her own) and write the file. False
    /// when nothing changed.
    pub(crate) fn set_spirit(&mut self, slot: i8, class: u8, id: u32) -> Result<bool, String> {
        let entry = self.slots.entry(slot).or_insert(SlotWear::new(class));
        if entry.class != class {
            *entry = SlotWear::new(class);
        }
        if entry.spirit == id {
            return Ok(false);
        }
        entry.spirit = id;
        self.changes += 1;
        self.save().map_err(|e| format!("avatar: cannot write {}: {e}", self.file.display()))?;
        Ok(true)
    }
}

/// `slot class layer=id ...` per line.
fn format_file(slots: &HashMap<i8, SlotWear>) -> String {
    let mut out = String::from("# 아바타: slot class layer=look ... [s=렌의 의상]\n");
    let mut keys: Vec<_> = slots.keys().copied().collect();
    keys.sort();
    for k in keys {
        let s = &slots[&k];
        out.push_str(&format!("{k} {}", s.class));
        for (layer, id) in s.worn.iter().enumerate() {
            if *id != 0 {
                out.push_str(&format!(" {layer}={id}"));
            }
        }
        if s.spirit != 0 {
            out.push_str(&format!(" s={}", s.spirit));
        }
        out.push('\n');
    }
    out
}

fn parse_file(text: &str) -> HashMap<i8, SlotWear> {
    let mut slots = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(slot), Some(class)) = (it.next(), it.next()) else { continue };
        let (Ok(slot), Ok(class)) = (slot.parse::<i8>(), class.parse::<u8>()) else { continue };
        let mut wear = SlotWear::new(class);
        for pair in it {
            let Some((l, id)) = pair.split_once('=') else { continue };
            if l == "s" {
                // 렌's outfit. A build without her wardrobe skips this pair
                // (`s` is not a layer number) and keeps the rest.
                if let Ok(id) = id.parse::<u32>() {
                    wear.spirit = id;
                }
                continue;
            }
            if let (Ok(l), Ok(id)) = (l.parse::<usize>(), id.parse::<u32>()) {
                if l < LAYERS {
                    wear.worn[l] = id;
                }
            }
        }
        slots.insert(slot, wear);
    }
    slots
}

/// The catalogue the game's own tables give: every set piece and every
/// legendary or set weapon and shield, one entry per distinct look.
fn table_looks(vfs: &[u8]) -> Vec<Look> {
    // (kind, layer, classes for the whole table, or None to take them from
    // the wearer-group byte)
    let tables: [(u8, u8, Option<u8>); 9] = [
        (5, 0, None),
        (6, 1, None),
        (7, 2, None),
        (8, 3, None),
        (0, WEAPON_LAYER, Some(1 << 0)),
        (1, WEAPON_LAYER, Some(1 << 1)),
        (2, WEAPON_LAYER, Some(1 << 2)),
        (3, WEAPON_LAYER, Some(1 << 3)),
        (9, SHIELD_LAYER, Some(KNIGHT)),
    ];
    let mut looks = Vec::new();
    for (kind, layer, classes) in tables {
        let path = format!("c/csv/item_{kind:02}.dat");
        let Some(data) = crate::shop::vfs_entry(vfs, &path) else { continue };
        let mut seen = std::collections::HashSet::new();
        for (row, (name, block)) in table_rows(data).into_iter().enumerate() {
            let grade = block[0x0c];
            // 전설 and the fourteen sets.
            if grade < 4 {
                continue;
            }
            let sheet = u16::from_le_bytes([block[0], block[1]]);
            let palette = i16::from_le_bytes([block[2], block[3]]);
            let classes = match classes {
                Some(c) => c,
                None => match block[5] {
                    5 => ALL_CLASSES,
                    6 => WARRIOR_KNIGHT,
                    _ => continue,
                },
            };
            if sheet == 0 || !seen.insert((sheet, palette)) {
                continue;
            }
            let mut name = name;
            name.push(0);
            looks.push(Look {
                id: kind as u32 * 1000 + row as u32,
                name,
                layer,
                sheet,
                palette,
                classes,
                rig: false,
                fx: 0,
            });
        }
    }
    looks
}

/// `(name, last 21 bytes)` of every row of an `item_%02d.dat`.
fn table_rows(d: &[u8]) -> Vec<(Vec<u8>, [u8; 21])> {
    let mut rows = Vec::new();
    let Some(count) = d.get(..2).map(|b| u16::from_le_bytes([b[0], b[1]])) else { return rows };
    let mut pos = 2usize;
    for _ in 0..count {
        let Some(rlen) = d.get(pos..pos + 2).map(|b| u16::from_le_bytes([b[0], b[1]])) else {
            break;
        };
        pos += 2;
        let Some(body) = d.get(pos..pos + rlen as usize) else { break };
        pos += rlen as usize;
        let Some(&nlen) = body.get(2) else { break };
        let Some(name) = body.get(3..3 + nlen as usize) else { break };
        let Some(block) = body.len().checked_sub(21).and_then(|s| body.get(s..)) else {
            rows.push((name.to_vec(), [0; 21]));
            continue;
        };
        rows.push((name.to_vec(), <[u8; 21]>::try_from(block).unwrap_or([0; 21])));
    }
    rows
}

/// Install the hooks. Every symbol is required: a half-installed wardrobe
/// would draw a button that does nothing, or dress a hero nobody can undress.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for s in [
        DRAW_CHARACTER_MENU,
        KEY_CHARACTER_MENU,
        TOUCH_CHARACTER_MENU,
        TOUCH_MOVED,
        TOUCH_RELEASED,
        SET_SUB_STATE,
        SET_LOAD_IMG_PATH,
        CHANGE_SPRITE_IMAGE,
        CHANGE_SPR_IMG,
        IS_NONE_EQUIP_WEAPON,
        LOAD_SPR_FILE,
        FREE_SPRITE_DATA,
        SPR_FILE,
        SPR_PAL,
        GET_HALF_WIDTH,
        GET_HALF_HEIGHT,
        GET_FB,
        FILL_RECT,
        DRAW_HALF_BOX,
        DRAW_TEXT,
        LCD_G,
        P_CFONT,
        GV,
    ] {
        if vm.symbol(s).is_none() {
            return Err(format!("avatar needs `{s}`, not in this binary"));
        }
    }
    let hooks: [(&'static str, crate::patch::PatchFn); 9] = [
        (DRAW_CHARACTER_MENU, draw_hook),
        (KEY_CHARACTER_MENU, key_hook),
        (TOUCH_CHARACTER_MENU, touch_hook),
        (TOUCH_MOVED, move_hook),
        (TOUCH_RELEASED, release_hook),
        (SET_LOAD_IMG_PATH, load_hook),
        (CHANGE_SPR_IMG, swap_hook),
        (LOAD_SPR_FILE, cif_hook),
        (CHANGE_SPRITE_IMAGE, change_hook),
    ];
    let mut names = Vec::new();
    for (sym, f) in hooks {
        let at = vm.symbol(sym).expect("checked above");
        vm.install_hook(sym, at, f)?;
        names.push(sym);
    }
    Ok(names)
}

pub(crate) fn entered_normally(vm: &Vm) -> bool {
    vm.cpu.reg_get(Mode::User, reg::LR) != RETURN_MAGIC
}

pub(crate) fn deref(vm: &Vm, sym: &str) -> Option<u32> {
    let p = vm.symbol(sym)?;
    let v = vm.mem.read_u32(p);
    (v != 0).then_some(v)
}

pub(crate) fn gv(vm: &Vm) -> Option<u32> {
    vm.symbol(GV)
}

/// The player hero, its class and the save slot in play.
pub(crate) fn hero(vm: &Vm) -> Option<(u32, u8, i8)> {
    let gv = gv(vm)?;
    let hero = vm.mem.read_u32(gv + GV_HERO);
    if hero == 0 {
        return None;
    }
    let class = vm.mem.read_u8(hero + HERO_CLASS);
    let slot = vm.mem.read_u8(gv + GV_SLOT) as i8;
    (class < 4).then_some((hero, class, slot))
}

/// Put a static string in the guest heap once.
pub(crate) fn text(vm: &mut Vm, bytes: &'static [u8]) -> Option<u32> {
    let key = bytes.as_ptr() as usize;
    if let Some(p) = vm.avatar.strings.get(&key) {
        return Some(*p);
    }
    let p = vm.heap.alloc(bytes.len() as u32)?;
    vm.mem.write_bytes(p, bytes);
    vm.avatar.strings.insert(key, p);
    Some(p)
}

/// A look's name in the guest heap, once.
fn look_name(vm: &mut Vm, i: usize) -> Option<u32> {
    let id = vm.avatar.looks[i].id;
    if let Some(p) = vm.avatar.names.get(&id) {
        return Some(*p);
    }
    let bytes = vm.avatar.looks[i].name.clone();
    let p = vm.heap.alloc(bytes.len() as u32)?;
    vm.mem.write_bytes(p, &bytes);
    vm.avatar.names.insert(id, p);
    Some(p)
}

/// The page's origin, as `DrawCharacterMenu` computes it.
pub(crate) fn origin(vm: &mut Vm) -> Option<(i32, i32)> {
    let g = deref(vm, LCD_G)?;
    let hw = vm.call(vm.symbol(GET_HALF_WIDTH)?, &[g]).ok()? as i32;
    let hh = vm.call(vm.symbol(GET_HALF_HEIGHT)?, &[g]).ok()? as i32;
    Some((hw - 0x58, hh - 0x66))
}

fn pending_stats(vm: &Vm, this: u32) -> bool {
    MENU_PENDING.iter().any(|o| vm.mem.read_u16(this + o) as i16 > 0)
}

// ---- drawing ---------------------------------------------------------------

pub(crate) struct Pen {
    pub(crate) graphic: u32,
    pub(crate) font: u32,
    pub(crate) fb: u32,
    pub(crate) common_ui: u32,
}

pub(crate) fn pen(vm: &mut Vm) -> Option<Pen> {
    let graphic = deref(vm, LCD_G)?;
    let font = deref(vm, P_CFONT)?;
    let common_ui = vm.mem.read_u32(gv(vm)? + GV_COMMON_UI);
    let fb = vm.call(vm.symbol(GET_FB)?, &[graphic]).ok()?;
    (fb != 0 && common_ui != 0).then_some(Pen { graphic, font, fb, common_ui })
}

/// `CommonUi::DrawHalfBox` style bytes. `LIT` and `ROW` are the pair the
/// option rows use for "under the cursor" and "not" (screen.rs: 0x11DEA8 /
/// 0x11DB34); `BAR` is what this page's own `*취소` / `#상세` plates use
/// (0xFC8D4 / 0xFC994): the dark half-box.
pub(crate) const LIT: (u32, u32) = (1, 1);
pub(crate) const ROW: (u32, u32) = (4, 0);
pub(crate) const BAR: (u32, u32) = (0, 0);

pub(crate) fn plate(vm: &mut Vm, p: &Pen, x: i32, y: i32, w: i32, h: i32, style: (u32, u32)) -> Result<(), String> {
    let f = vm.symbol(DRAW_HALF_BOX).ok_or("avatar: DrawHalfBox vanished")?;
    vm.call(f, &[p.common_ui, p.fb, x as u32, y as u32, w as u32, h as u32, style.0, style.1, 0xff])?;
    Ok(())
}

pub(crate) fn label(vm: &mut Vm, p: &Pen, x: i32, y: i32, s: u32, align: u32, colour: u32) -> Result<(), String> {
    let f = vm.symbol(DRAW_TEXT).ok_or("avatar: DrawText vanished")?;
    vm.call(f, &[p.font, p.fb, x as u32, y as u32, s, align, colour])?;
    Ok(())
}

/// Run the guest's page, then add the button — and the wardrobe over it when
/// it is up.
fn draw_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.arg(0);
    let f = vm.symbol(DRAW_CHARACTER_MENU).ok_or("avatar: DrawCharacterMenu vanished")?;
    vm.call(f, &[this])?;
    if this == 0 || vm.mem.read_u8(this + MENU_STATE) != PAGE_CHARACTER {
        return Ok(true);
    }
    // A page that was left and has come back starts closed.
    let now = vm.guest_repaints();
    if vm.avatar.open && now.saturating_sub(vm.avatar.last_draw) > 2 {
        vm.avatar.open = false;
    }
    vm.avatar.last_draw = now;
    let subpage = vm.mem.read_u8(this + MENU_SUBPAGE);
    if subpage != 0 {
        vm.avatar.open = false;
    }
    let pending = subpage == 0 && pending_stats(vm, this);
    if pending {
        // `*취소` owns the slot, and the wardrobe would hide what it cancels.
        vm.avatar.open = false;
        return Ok(true);
    }
    let (Some((x0, y0)), Some(p)) = (origin(vm), pen(vm)) else { return Ok(true) };
    let open = vm.avatar.open;
    if open {
        draw_page(vm, &p, x0, y0)?;
        vm.avatar.pages_drawn += 1;
    }
    plate(vm, &p, x0, y0 + BAR_Y, BAR_W, BAR_H, if open { LIT } else { BAR })?;
    if let Some(s) = text(vm, LABEL_BUTTON) {
        let colour = if open { FG } else { FG_DIM };
        label(vm, &p, x0 + BAR_LABEL.0, y0 + BAR_LABEL.1, s, 1, colour)?;
    }
    vm.avatar.buttons_drawn += 1;
    Ok(true)
}

fn draw_page(vm: &mut Vm, p: &Pen, x0: i32, y0: i32) -> Result<(), String> {
    let Some((_, class, slot)) = hero(vm) else { return Ok(()) };
    let fill = vm.symbol(FILL_RECT).ok_or("avatar: FillRect vanished")?;
    vm.call(
        fill,
        &[
            p.graphic,
            p.fb,
            (x0 + AREA.0) as u32,
            (y0 + AREA.1) as u32,
            AREA.2 as u32,
            AREA.3 as u32,
            PANEL_BG,
        ],
    )?;
    for (i, tab) in TABS.iter().enumerate() {
        let x = x0 + TAB_X + TAB_PITCH * i as i32;
        let lit = i == vm.avatar.tab;
        plate(vm, p, x, y0 + TAB_Y, TAB_W, PLATE_H, if lit { LIT } else { ROW })?;
        if let Some(s) = text(vm, tab.label) {
            label(vm, p, x + TAB_W / 2 + 1, y0 + TAB_Y + 3, s, 1, if lit { FG } else { FG_DIM })?;
        }
    }
    let rows = vm.avatar.rows(class);
    let layer = TABS[vm.avatar.tab].layer;
    let worn_id = vm.avatar.worn(slot, class, layer).map(|l| l.id);
    let first = vm.avatar.first;
    for r in 0..ROWS {
        let Some(&row) = rows.get(first + r) else { break };
        let y = y0 + ROW_Y + ROW_PITCH * r as i32;
        let lit = first + r == vm.avatar.cursor;
        plate(vm, p, x0 + TAB_X, y, ROW_W, PLATE_H, if lit { LIT } else { ROW })?;
        let worn = match row {
            None => worn_id.is_none(),
            Some(i) => Some(vm.avatar.looks[i].id) == worn_id,
        };
        let name = match row {
            None => text(vm, LABEL_NONE),
            Some(i) => look_name(vm, i),
        };
        if let Some(s) = name {
            label(vm, p, x0 + TAB_X + 6, y + 3, s, 0, if lit { FG } else { FG_DIM })?;
        }
        if worn {
            if let Some(s) = text(vm, LABEL_WORN) {
                label(vm, p, x0 + TAB_X + ROW_W - 14, y + 3, s, 1, FG_WORN)?;
            }
        }
    }
    if let Some((at, h)) = thumb(rows.len(), first) {
        let (track_y, track_h) = (y0 + ROW_Y, ROW_PITCH * (ROWS as i32 - 1) + PLATE_H);
        for (y, h, colour) in [(track_y, track_h, TRACK), (track_y + at, h, THUMB)] {
            vm.call(
                fill,
                &[p.graphic, p.fb, (x0 + BAR_X) as u32, y as u32, SCROLL_W as u32, h as u32, colour],
            )?;
        }
    }
    Ok(())
}

/// The scroll bar's thumb, `(offset, height)` down the track, for a list of
/// `len` rows showing from `first`; `None` when everything fits.
fn thumb(len: usize, first: usize) -> Option<(i32, i32)> {
    if len <= ROWS {
        return None;
    }
    let track = ROW_PITCH * (ROWS as i32 - 1) + PLATE_H;
    let h = (track * ROWS as i32 / len as i32).max(6);
    let span = (len - ROWS) as i32;
    Some(((track - h) * first.min(len - ROWS) as i32 / span, h))
}

/// The list's first row for a finger that went down at `from_y` with the
/// list at `first` and is now at `y`: whole rows, nearest, kept in range.
fn scrolled_first(first: usize, from_y: i32, y: i32, len: usize) -> usize {
    let rows = ((from_y - y) as f32 / ROW_PITCH as f32).round() as i64;
    (first as i64 + rows).clamp(0, len.saturating_sub(ROWS) as i64) as usize
}

// ---- input -----------------------------------------------------------------

/// Re-dress the hero now, through the guest's own reload.
fn redress(vm: &mut Vm) -> Result<(), String> {
    let Some((hero, ..)) = hero(vm) else { return Ok(()) };
    let f = vm.symbol(CHANGE_SPRITE_IMAGE).ok_or("avatar: ChangeSpriteImage vanished")?;
    vm.call(f, &[hero, 0])?;
    sync_cif(vm)
}

fn open(vm: &mut Vm, this: u32) -> Result<(), String> {
    if vm.mem.read_u8(this + MENU_SUBPAGE) != 0 {
        let f = vm.symbol(SET_SUB_STATE).ok_or("avatar: SetSubState vanished")?;
        vm.call(f, &[this, 0])?;
    }
    vm.avatar.open = true;
    vm.avatar.opened += 1;
    // Start on the worn row of the current tab, so the cursor says what the
    // hero has on.
    if let Some((_, class, slot)) = hero(vm) {
        place_cursor_on_worn(vm, class, slot);
    }
    Ok(())
}

fn place_cursor_on_worn(vm: &mut Vm, class: u8, slot: i8) {
    let rows = vm.avatar.rows(class);
    let layer = TABS[vm.avatar.tab].layer;
    let worn = vm.avatar.worn(slot, class, layer).map(|l| l.id);
    let at = rows
        .iter()
        .position(|r| match r {
            None => worn.is_none(),
            Some(i) => Some(vm.avatar.looks[*i].id) == worn,
        })
        .unwrap_or(0);
    move_cursor(vm, at, rows.len());
}

fn move_cursor(vm: &mut Vm, to: usize, len: usize) {
    let to = to.min(len.saturating_sub(1));
    vm.avatar.cursor = to;
    let a = &mut vm.avatar;
    if to < a.first {
        a.first = to;
    } else if to >= a.first + ROWS {
        a.first = to + 1 - ROWS;
    }
}

/// Wear the row under the cursor: `None` takes the layer off.
fn wear_cursor(vm: &mut Vm) -> Result<(), String> {
    let Some((_, class, slot)) = hero(vm) else { return Ok(()) };
    let rows = vm.avatar.rows(class);
    let Some(&row) = rows.get(vm.avatar.cursor) else { return Ok(()) };
    let layer = TABS[vm.avatar.tab].layer as usize;
    let id = row.map_or(0, |i| vm.avatar.looks[i].id);
    let a = &mut vm.avatar;
    let entry = a.slots.entry(slot).or_insert(SlotWear::new(class));
    if entry.class != class {
        *entry = SlotWear::new(class);
    }
    if entry.worn[layer] == id {
        return Ok(());
    }
    entry.worn[layer] = id;
    a.changes += 1;
    if let Err(e) = a.save() {
        vm.log.push(format!("avatar: cannot write {}: {e}", vm.avatar.file.display()));
    }
    redress(vm)
}

fn step_tab(vm: &mut Vm, back: bool) {
    vm.avatar.drag = None;
    let n = TABS.len();
    vm.avatar.tab = if back { (vm.avatar.tab + n - 1) % n } else { (vm.avatar.tab + 1) % n };
    vm.avatar.cursor = 0;
    vm.avatar.first = 0;
    if let Some((_, class, slot)) = hero(vm) {
        place_cursor_on_worn(vm, class, slot);
    }
}

/// `'*'` (or `'0'`) on a free slot opens the wardrobe; while it is up every
/// key is the wardrobe's.
fn key_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.arg(0);
    let key = vm.arg(1) as i32;
    if this == 0 {
        return Ok(false);
    }
    let subpage = vm.mem.read_u8(this + MENU_SUBPAGE);
    if !vm.avatar.open {
        let free = subpage != 0 || !pending_stats(vm, this);
        if (key == K_STAR || key == K_ZERO) && free {
            open(vm, this)?;
            return Ok(true);
        }
        return Ok(false);
    }
    let Some((_, class, _)) = hero(vm) else { return Ok(false) };
    let len = vm.avatar.rows(class).len();
    match key {
        K_UP => {
            let c = vm.avatar.cursor;
            move_cursor(vm, if c == 0 { len - 1 } else { c - 1 }, len);
        }
        K_DOWN => {
            let c = vm.avatar.cursor;
            move_cursor(vm, if c + 1 >= len { 0 } else { c + 1 }, len);
        }
        K_LEFT => step_tab(vm, true),
        K_RIGHT => step_tab(vm, false),
        K_OK => wear_cursor(vm)?,
        K_STAR | K_ZERO | K_CLEAR => vm.avatar.open = false,
        K_HASH => {
            // 상세: close, and let the guest flip the page.
            vm.avatar.open = false;
            return Ok(false);
        }
        _ => {}
    }
    Ok(true)
}

/// While the wardrobe is up, taps on it are its own; a tap anywhere else
/// closes it and goes to the guest. A press on the list arms a drag: the row
/// is lit now and worn on release ([`release_hook`]), unless the finger moved.
fn touch_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    vm.avatar.drag = None;
    if !vm.avatar.open {
        return Ok(false);
    }
    let (x, y) = (vm.arg(1) as i32, vm.arg(2) as i32);
    let Some((x0, y0)) = origin(vm) else { return Ok(false) };
    let (dx, dy) = (x - x0, y - y0);
    // The button itself: close.
    if (0..=BAR_W).contains(&dx) && dy > BAR_HIT.0 - 1 && dy <= BAR_HIT.1 {
        vm.avatar.open = false;
        return Ok(true);
    }
    let in_area = (AREA.0..AREA.0 + AREA.2).contains(&dx) && (AREA.1..AREA.1 + AREA.3).contains(&dy);
    if !in_area {
        vm.avatar.open = false;
        return Ok(false);
    }
    if (TAB_Y..TAB_Y + PLATE_H).contains(&dy) {
        let i = (dx - TAB_X) / TAB_PITCH;
        if (0..TABS.len() as i32).contains(&i) && dx >= TAB_X {
            vm.avatar.tab = i as usize;
            vm.avatar.cursor = 0;
            vm.avatar.first = 0;
            if let Some((_, class, slot)) = hero(vm) {
                place_cursor_on_worn(vm, class, slot);
            }
        }
        return Ok(true);
    }
    if dy >= ROW_Y {
        let Some((_, class, _)) = hero(vm) else { return Ok(true) };
        let len = vm.avatar.rows(class).len();
        let r = ((dy - ROW_Y) / ROW_PITCH) as usize;
        let to = vm.avatar.first + r;
        let row = (r < ROWS && dx >= TAB_X && dx < TAB_X + ROW_W && to < len).then_some(to);
        if let Some(to) = row {
            move_cursor(vm, to, len);
        }
        vm.avatar.drag = Some(Drag { y, first: vm.avatar.first, row, moved: false });
    }
    Ok(true)
}

/// A finger that went down on the list scrolls it by whole rows once it has
/// travelled [`DRAG_SLOP`]. The cursor stays on a visible row, so the keys
/// carry on from what the player scrolled to.
fn move_hook(vm: &mut Vm) -> Result<bool, String> {
    let Some(mut drag) = vm.avatar.drag else { return crate::spirit_avatar::move_hook(vm) };
    if !entered_normally(vm) || !vm.avatar.open {
        return Ok(false);
    }
    let y = vm.arg(2) as i32;
    if !drag.moved && (y - drag.y).abs() < DRAG_SLOP {
        return Ok(false);
    }
    drag.moved = true;
    vm.avatar.drag = Some(drag);
    scroll_to(vm, &drag, y);
    Ok(false)
}

/// Put the list where a finger at `y` has dragged it.
fn scroll_to(vm: &mut Vm, drag: &Drag, y: i32) {
    let Some((_, class, _)) = hero(vm) else { return };
    let len = vm.avatar.rows(class).len();
    let first = scrolled_first(drag.first, drag.y, y, len);
    let a = &mut vm.avatar;
    if first != a.first {
        a.scrolled += first.abs_diff(a.first) as u64;
        a.first = first;
        a.cursor = a.cursor.clamp(first, first + ROWS - 1).min(len.saturating_sub(1));
    }
}

/// The end of a press on the list: a tap wears its row, a drag stops where
/// the finger lifted. That point decides, not the last move: `onHandleEvent`
/// posts a move at the press point along with every press, and the move
/// mailbox keeps the first of a tick, so a short quick drag's own moves can
/// all be dropped behind it. The release always records where it was.
fn release_hook(vm: &mut Vm) -> Result<bool, String> {
    let Some(drag) = vm.avatar.drag.take() else { return crate::spirit_avatar::release_hook(vm) };
    if !entered_normally(vm) || !vm.avatar.open {
        return Ok(false);
    }
    let y = vm.arg(2) as i32;
    if drag.moved || (y - drag.y).abs() >= DRAG_SLOP {
        scroll_to(vm, &drag, y);
        return Ok(false);
    }
    if let Some(row) = drag.row {
        let Some((_, class, _)) = hero(vm) else { return Ok(false) };
        let len = vm.avatar.rows(class).len();
        if row < len {
            move_cursor(vm, row, len);
            wear_cursor(vm)?;
        }
    }
    Ok(false)
}

// ---- dressing --------------------------------------------------------------

fn sheet_path(dir: u8, sheet: u16) -> Vec<u8> {
    format!("/c/sp/img{dir}/{sheet:03}.mgr\0").into_bytes()
}

/// Write one dressed layer into the guest's two tables.
pub(crate) fn dress_layer(vm: &mut Vm, layer: u8, look_sheet: u16, palette: i16) {
    let (Some(files), Some(pals)) = (vm.symbol(SPR_FILE), vm.symbol(SPR_PAL)) else { return };
    let dir = TABS.iter().find(|t| t.layer == layer).map_or(layer, |t| t.dir);
    let path = sheet_path(dir, look_sheet);
    vm.mem.write_bytes(files + layer as u32 * 0x100, &path);
    vm.mem.write_u16(pals + layer as u32 * 2, palette as u16);
    vm.avatar.dressed += 1;
}

/// Run the guest's own path builder, then dress the layers the player chose.
fn load_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.arg(0);
    let empty_weapon = vm.arg(1);
    let f = vm.symbol(SET_LOAD_IMG_PATH).ok_or("avatar: SetLoadImgPath vanished")?;
    vm.call(f, &[this, empty_weapon])?;
    // Whatever this call dresses is what the next cif read must match.
    vm.avatar.dressed_rig = (false, false);
    let Some((hero, class, slot)) = hero(vm) else { return Ok(true) };
    if this != hero {
        return Ok(true);
    }
    let files = vm.symbol(SPR_FILE).ok_or("avatar: _szSprFile vanished")?;
    let mut rigged = (false, false);
    for tab in &TABS {
        let Some(look) = vm.avatar.worn(slot, class, tab.layer).cloned() else { continue };
        if tab.layer == WEAPON_LAYER {
            // A costume weapon is a look for the weapon in hand, not a weapon:
            // an empty hand stays empty.
            if let Some(f) = vm.symbol(IS_NONE_EQUIP_WEAPON) {
                if vm.call(f, &[hero])? & 0xff != 0 {
                    continue;
                }
            }
        }
        if tab.layer == SHIELD_LAYER && vm.mem.read_u8(files + SHIELD_LAYER as u32 * 0x100) == 0 {
            // No shield on, or not a knight: the guest left the layer empty.
            continue;
        }
        dress_layer(vm, tab.layer, look.sheet, look.palette);
        if tab.layer == WEAPON_LAYER && look.fx != 0 {
            // The class's effect layer, in this avatar's colour; its own
            // colours, like every costume sheet.
            dress_layer(vm, FX_LAYER, look.fx, -1);
        }
        if look.rig && vm.avatar.rig_cifs {
            match tab.layer {
                0 => rigged.0 = true,
                1 => rigged.1 = true,
                _ => {}
            }
        }
    }
    vm.avatar.dressed_rig = rigged;
    Ok(true)
}

/// A guest string at `p`, up to `max` bytes, without its NUL.
fn c_string(vm: &Vm, p: u32, max: u32) -> Vec<u8> {
    (0..max).map(|i| vm.mem.read_u8(p + i)).take_while(|b| *b != 0).collect()
}

/// A path in the guest heap, once.
fn cif_path_ptr(vm: &mut Vm, path: &str) -> Option<u32> {
    if let Some(p) = vm.avatar.cif_paths.get(path) {
        return Some(*p);
    }
    let mut bytes = path.as_bytes().to_vec();
    bytes.push(0);
    let p = vm.heap.alloc(bytes.len() as u32)?;
    vm.mem.write_bytes(p, &bytes);
    vm.avatar.cif_paths.insert(path.to_string(), p);
    Some(p)
}

/// `SPRITE_DATA::LoadSprFile(path, wide)` on the hero's sprite data with its
/// class cif: hand it the rig's variant when the dressing just applied is
/// rigged. Declines either way — the guest loads, from the path in r1.
fn cif_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let (this, path) = (vm.arg(0), vm.arg(1));
    let Some(gv) = gv(vm) else { return Ok(false) };
    if this == 0 || this != vm.mem.read_u32(gv + GV_HERO_SPRITE_DATA) {
        return Ok(false);
    }
    let Some(class) = crate::costume::hero_cif_class(&c_string(vm, path, 32)) else { return Ok(false) };
    let mut rigged = vm.avatar.dressed_rig;
    if let Some(variant) = crate::costume::rig_cif_path(class, rigged.0, rigged.1) {
        match cif_path_ptr(vm, &variant) {
            Some(p) => {
                vm.cpu.reg_set(Mode::User, 1, p);
                vm.avatar.cif_swaps += 1;
            }
            // No heap for the path: the archive's cif loads, and that is what
            // is recorded (a rigged sheet still draws its first images).
            None => rigged = (false, false),
        }
    }
    vm.avatar.cif_loaded = Some((this, class, rigged));
    Ok(false)
}

/// After a re-dress, read the hero's cif again if the rigged pair changed.
fn sync_cif(vm: &mut Vm) -> Result<(), String> {
    let Some((data, class, loaded)) = vm.avatar.cif_loaded else { return Ok(()) };
    let want = vm.avatar.dressed_rig;
    let Some(gv) = gv(vm) else { return Ok(()) };
    if loaded == want || vm.mem.read_u32(gv + GV_HERO_SPRITE_DATA) != data {
        return Ok(());
    }
    let path = crate::costume::rig_cif_path(class, want.0, want.1)
        .unwrap_or_else(|| crate::costume::hero_cif(class));
    let p = cif_path_ptr(vm, &path).ok_or("avatar: no heap for a cif path")?;
    let free = vm.symbol(FREE_SPRITE_DATA).ok_or("avatar: FreeSpriteData vanished")?;
    let load = vm.symbol(LOAD_SPR_FILE).ok_or("avatar: LoadSprFile vanished")?;
    vm.call(free, &[data])?;
    vm.call(load, &[data, p, 1])?;
    vm.avatar.cif_loaded = Some((data, class, want));
    vm.avatar.cif_swaps += 1;
    Ok(())
}

/// `HERO::ChangeSpriteImage` from the guest (every re-equip): run it, then
/// bring the cif in line with what it dressed.
fn change_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let (this, arg) = (vm.arg(0), vm.arg(1));
    let f = vm.symbol(CHANGE_SPRITE_IMAGE).ok_or("avatar: ChangeSpriteImage vanished")?;
    vm.call(f, &[this, arg])?;
    sync_cif(vm)?;
    Ok(true)
}

/// The 투구보기 toggle writes layer 0 itself and swaps it in place; dress it
/// first and let the swap run.
fn swap_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) || vm.arg(2) as i8 != 0 {
        return Ok(false);
    }
    let Some((_, class, slot)) = hero(vm) else { return Ok(false) };
    if let Some(look) = vm.avatar.worn(slot, class, 0).cloned() {
        dress_layer(vm, 0, look.sheet, look.palette);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_round_trips() {
        let mut slots = HashMap::new();
        let mut w = SlotWear::new(3);
        w.worn[0] = 5111;
        w.worn[6] = 9042;
        slots.insert(0i8, w);
        slots.insert(2i8, SlotWear { spirit: 130010, ..SlotWear::new(1) });
        let text = format_file(&slots);
        assert_eq!(parse_file(&text), slots, "{text}");
        // A line a human broke is skipped, not fatal.
        let broken = parse_file("x y\n1 2 4=abc 99=1 3=7\n");
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[&1].worn[3], 7);
    }

    #[test]
    fn the_tabs_fit_the_page_and_the_rows_fit_above_the_bar() {
        let right = TAB_X + TAB_PITCH * (TABS.len() as i32 - 1) + TAB_W;
        assert!(right <= AREA.0 + AREA.2, "tabs end at {right}");
        assert!(TAB_X + ROW_W < BAR_X);
        assert!(BAR_X + SCROLL_W <= AREA.0 + AREA.2, "the scroll bar ends at {}", BAR_X + SCROLL_W);
        let bottom = ROW_Y + ROW_PITCH * (ROWS as i32 - 1) + PLATE_H;
        assert!(bottom <= BAR_Y, "rows end at {bottom}, the bar starts at {BAR_Y}");
        assert!(TAB_Y + PLATE_H <= ROW_Y);
        // The panel is 0xb0 wide.
        assert!(AREA.0 + AREA.2 <= 0xb0);
    }

    #[test]
    fn a_drag_scrolls_by_whole_rows_and_stays_in_range() {
        // Up by a row's pitch shows the next row; a little less still rounds.
        assert_eq!(scrolled_first(0, 150, 150 - ROW_PITCH, 20), 1);
        assert_eq!(scrolled_first(0, 150, 150 - ROW_PITCH / 2 - 1, 20), 1);
        assert_eq!(scrolled_first(0, 150, 150 - ROW_PITCH / 2 + 1, 20), 0);
        assert_eq!(scrolled_first(5, 100, 100 + 2 * ROW_PITCH, 20), 3);
        // Never past either end, and nothing to scroll in a short list.
        assert_eq!(scrolled_first(2, 100, 100 + 10 * ROW_PITCH, 20), 0);
        assert_eq!(scrolled_first(2, 100, 100 - 100 * ROW_PITCH, 20), 16);
        assert_eq!(scrolled_first(0, 100, 0, 3), 0);
        // The slop is less than half a row: a drag that counts always can.
        assert!(DRAG_SLOP < ROW_PITCH / 2);
    }

    #[test]
    fn the_thumb_runs_the_whole_track() {
        let track = ROW_PITCH * (ROWS as i32 - 1) + PLATE_H;
        assert_eq!(thumb(ROWS, 0), None);
        let (top, h) = thumb(40, 0).unwrap();
        assert_eq!(top, 0);
        let (bottom, h2) = thumb(40, 36).unwrap();
        assert_eq!(h, h2);
        assert_eq!(bottom + h, track);
        assert!(thumb(1000, 0).unwrap().1 >= 6, "a long list still has a thumb to see");
    }

    #[test]
    fn sheet_paths_are_the_guests_format() {
        assert_eq!(sheet_path(1, 5), b"/c/sp/img1/005.mgr\0");
        assert_eq!(sheet_path(0, 123), b"/c/sp/img0/123.mgr\0");
        assert!(sheet_path(4, 999).len() < 0x100, "fits a _szSprFile slot");
    }

    /// The catalogue the shipped tables give: the fourteen sets in each armour
    /// tab, every class's legendary weapons, the knight's shields — each with
    /// a sheet the archive actually has, because a missing sheet is a layer
    /// that is not drawn (§45: a helmet whose sheet did not exist took the
    /// hero's head with it).
    #[test]
    fn the_shipped_tables_give_a_full_wardrobe_of_real_sheets() {
        let Ok(vfs) = std::fs::read("../payload/data.vfs.mp3") else {
            eprintln!("payload/ absent; skipped");
            return;
        };
        let looks = table_looks(&vfs);
        for (layer, at_least) in [(0u8, 14usize), (1, 14), (2, 14), (3, 14), (6, 1)] {
            let n = looks.iter().filter(|l| l.layer == layer).count();
            assert!(n >= at_least, "layer {layer}: {n} looks");
        }
        for class in 0..4u8 {
            let n = looks.iter().filter(|l| l.layer == WEAPON_LAYER && l.classes & (1 << class) != 0).count();
            assert!(n >= 4, "class {class}: {n} weapon looks");
        }
        let mut ids = std::collections::HashSet::new();
        for l in &looks {
            assert!(ids.insert(l.id), "id {} twice", l.id);
            assert_eq!(l.name.last(), Some(&0), "{}: a C string", l.id);
            let dir = TABS.iter().find(|t| t.layer == l.layer).unwrap().dir;
            let path = format!("c/sp/img{dir}/{:03}.mgr", l.sheet);
            assert!(
                crate::shop::vfs_entry(&vfs, &path).is_some(),
                "look {} names {path}, which the archive does not have",
                l.id
            );
            assert!(l.classes != 0 && l.classes <= ALL_CLASSES);
        }
    }

    #[test]
    fn every_tab_label_is_two_hangul() {
        for t in &TABS {
            assert_eq!(t.label.len(), 5);
            assert_eq!(t.label[4], 0);
            assert!(t.label[..4].iter().all(|b| *b >= 0xa1));
        }
    }
}
