//! 렌의 아바타 — the spirit's wardrobe, behind a button on the 정령 page.
//!
//! The player asked for the character page's `*아바타` (see [`crate::avatar`])
//! on the in-game 정령 menu as well, dressing the spirit 렌 in outfits drawn
//! for her own sprite. This module is the button, her wardrobe and the
//! substitution that dresses her; the outfits are [`crate::costume`] sheets
//! like the hero's, flagged [`crate::costume::SPIRIT`].
//!
//! # She is one sprite, named in one place
//!
//! `Battle::LoadSpiritRes` (0x4908C) writes `_szSprFile[0] = /c/sp/img0/042.mgr`
//! (her head), `[1] = /c/sp/img1/025.mgr` (arms, torso, skirt, legs and the
//! two light-tails at her hips, 46 images), `empty.mgr` for 2..4 and
//! `/c/sp/img5/021.mgr` for her effects, then loads them with
//! `SPRITE_IMAGE::LoadSprImg` at `+0x110` into the image at `gv+0x147C`
//! (data `gv+0x1478`, the `Spirit` object `gv+0x1484`). That one sprite is
//! what the field draws (`Spirit::Draw` from `Battle::Paint`), what the 정령
//! page draws (`DrawSpiritMenu+0x11FC`: `GetSpritePtr` of `gv+0x1484`, then
//! `SPRITE::DrawSprite` at `(x0+0x22, y0+0x57)`), and what the skill cut-in
//! draws. [`load_hook`] rewrites the first two paths on that one call and
//! declines, so the guest's own loader reads our sheets; a choice made on the
//! page goes through the guest's own `SPRITE_IMAGE::ChangeSprImg` on the same
//! image, so the figure on the page changes as the row is picked. The
//! palette slot that function reads (`_nSprPalFile[layer]`) is the hero's by
//! then, so it is held at −1 for the call and put back.
//!
//! `ChangeSprImg` reloads into the image array it already has, sized by the
//! old sheet: an outfit keeps the base sheet's image count, which
//! `costume::build` guarantees for every costume.
//!
//! In a story scene she is not this sprite but an NPC built from `npc_g.dat`
//! record 1, which names the same two sheets; [`crate::npcbody`] dresses
//! that copy from the same `s=` entry.
//!
//! # The button is a free key on a free strip
//!
//! `KeySpiritMenu` (0x10527C) on the 기본/장비 page (sub-state 0) dispatches
//! on `key + 0x10` through a 0x44-entry table, and `'*'` and `'0'` land on its
//! default arm: nothing. The strip at `y0+0xa2` — where the character page's
//! `*취소` sits — is empty on this page, between the equipment row and the
//! menu's icon bar. So the button has the character page's rectangle and its
//! label, `*` (or `'0'`, what a pad's L2 sends) opens it, and
//! [`touch_hook`] answers a tap on it, because unlike `TouchCharacterMenu` the
//! guest's `TouchSpiritMenu` has no hit box there.
//!
//! # The wardrobe leaves her in view
//!
//! Opened, it covers the page right of her figure and below it, and lists
//! `기본 외형` and her outfits in the guest's plates and font. The figure
//! above stays the guest's own (her three-quarter side); under it we draw her
//! twice more through the same `SPRITE::DrawSprite`, from the front and from
//! behind, so an outfit is judged from three sides without leaving the page.

use std::collections::HashMap;

use arm32_cpu::{reg, Mode};

use crate::avatar::{
    entered_normally, gv, hero, label, origin, pen, plate, text, Pen, SpiritLook, BAR, BAR_H, BAR_LABEL,
    BAR_W, BAR_Y, CHANGE_SPR_IMG, DRAG_SLOP, FG, FG_DIM, FG_WORN, FILL_RECT, K_CLEAR, K_DOWN, K_HASH, K_LEFT,
    K_OK, K_RIGHT, K_STAR, K_UP, K_ZERO, LABEL_BUTTON, LABEL_NONE, LABEL_WORN, LIT, PANEL_BG, PLATE_H, ROW,
    ROW_PITCH, SCROLL_W, SPR_FILE, SPR_PAL, THUMB, TRACK,
};
use crate::costume::Costume;
use crate::Vm;

const DRAW_SPIRIT_MENU: &str = "_ZN15StateInGameMenu14DrawSpiritMenuEv";
const KEY_SPIRIT_MENU: &str = "_ZN15StateInGameMenu13KeySpiritMenuEi";
const TOUCH_SPIRIT_MENU: &str = "_ZN15StateInGameMenu15TouchSpiritMenuEii";
const LOAD_SPR_IMG: &str = "_ZN12SPRITE_IMAGE10LoadSprImgEP11SPRITE_DATA";
const LOAD_SPIRIT_RES: &str = "_ZN6Battle13LoadSpiritResEv";
const GET_SPRITE_PTR: &str = "_ZN4CHAR12GetSpritePtrEv";
const DRAW_SPRITE: &str = "_ZN6SPRITE10DrawSpriteEiiiaash";

/// `LoadSpiritRes`'s `bl LoadSprImg` for 렌 is at `+0x110` (0x4919C); the
/// second one, at `+0x248`, loads the shared emotion balloons (`imo`).
const LOAD_SPIRIT_RES_LR: u32 = 0x114;

/// `gv` fields `LoadSpiritRes` fills: her `SPRITE_DATA*`, `SPRITE_IMAGE*` and
/// the `Spirit` object.
const GV_SPIRIT_DATA: u32 = 0x1478;
const GV_SPIRIT_IMAGE: u32 = 0x147c;
const GV_SPIRIT: u32 = 0x1484;

/// `StateInGameMenu + 0x0d`: the page's sub-state; 0 is 기본/장비.
const MENU_SUBPAGE: u32 = 0x0d;

/// Her own sheets, for undressing.
const HEAD_DEFAULT: u16 = 42;
const BODY_DEFAULT: u16 = 25;

/// Her own (head, body) sheet numbers — what an NPC record of hers names
/// (`npcbody`).
pub(crate) fn own_sheets() -> (u8, u8) {
    (HEAD_DEFAULT as u8, BODY_DEFAULT as u8)
}

// ---- geometry, relative to the page's (x0, y0) = (halfW-0x58, halfH-0x66) --

/// The guest draws her at `(x0+0x22, y0+0x57)` (feet); her box is left of
/// `x0+0x3e` and above `y0+0x5c`. The wardrobe covers the rest.
const RIGHT: (i32, i32, i32, i32) = (0x3e, 0x2a, 0x70, 0x76);
const UNDER: (i32, i32, i32, i32) = (0x02, 0x5c, 0x3c, 0x44);
/// Rows: six plates down the right column.
const ROW_X: i32 = 0x42;
const ROW_Y: i32 = 0x2c;
const ROW_W: i32 = 0x66;
const ROWS: usize = 6;
/// The scroll bar right of the rows, as on the character page.
const SCROLL_X: i32 = ROW_X + ROW_W + 2;
/// The two extra views under her figure: feet at these points.
const FRONT_AT: (i32, i32) = (0x12, 0x9c);
const BACK_AT: (i32, i32) = (0x2e, 0x9c);
/// `SPRITE::DrawSprite`'s direction argument for those two views: 0 up
/// (her back), 1 down (facing the viewer). The page itself passes 2, her
/// three-quarter side.
const DIR_FRONT: u32 = 1;
const DIR_BACK: u32 = 0;

/// Her wardrobe's state.
#[derive(Default)]
pub(crate) struct Wardrobe {
    open: bool,
    cursor: usize,
    first: usize,
    /// A finger down on the list: (y, first then, the row under it, moved).
    drag: Option<(i32, usize, Option<usize>, bool)>,
    /// `guest_repaints()` at the page's last draw: a page left and re-entered
    /// starts closed.
    last_draw: u64,
    /// Outfit names in the guest heap, by outfit id.
    names: HashMap<u32, u32>,
    /// A guest buffer `ChangeSprImg` reads its path from.
    path_buf: u32,
    pub buttons_drawn: u64,
    pub opened: u64,
    pub redressed: u64,
    /// Times her sheets were rewritten on their way into `LoadSprImg`.
    pub dressed: u64,
    /// NPC-body copies of her dressed in a scene (`npcbody`).
    pub bodies_dressed: u64,
}

/// Pair the spirit costumes up: an outfit is a head and a body under one
/// sheet number, and only a complete pair is offered.
impl crate::avatar::Avatar {
    /// (buttons drawn, times opened, re-dresses, loads dressed, scene bodies
    /// dressed) — the probe's summary line.
    pub fn spirit_counters(&self) -> (u64, u64, u64, u64, u64) {
        let w = &self.spirit;
        (w.buttons_drawn, w.opened, w.redressed, w.dressed, w.bodies_dressed)
    }
}

pub(crate) fn outfits(costumes: &[&'static Costume]) -> Vec<SpiritLook> {
    let mut out = Vec::new();
    for head in costumes.iter().filter(|c| c.layer == 0) {
        if costumes.iter().any(|c| c.layer == 1 && c.sheet == head.sheet) {
            out.push(SpiritLook { id: head.id, name: head.name.to_vec(), sheet: head.sheet });
        }
    }
    out
}

/// Install the hooks. Every symbol is required, for the reason `avatar`
/// gives: a half-installed wardrobe would draw a button that does nothing.
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for s in [
        DRAW_SPIRIT_MENU,
        KEY_SPIRIT_MENU,
        TOUCH_SPIRIT_MENU,
        LOAD_SPR_IMG,
        LOAD_SPIRIT_RES,
        GET_SPRITE_PTR,
        DRAW_SPRITE,
        CHANGE_SPR_IMG,
        SPR_FILE,
        SPR_PAL,
        FILL_RECT,
    ] {
        if vm.symbol(s).is_none() {
            return Err(format!("spirit avatar needs `{s}`, not in this binary"));
        }
    }
    let hooks: [(&'static str, crate::patch::PatchFn); 4] = [
        (DRAW_SPIRIT_MENU, draw_hook),
        (KEY_SPIRIT_MENU, key_hook),
        (TOUCH_SPIRIT_MENU, touch_hook),
        (LOAD_SPR_IMG, load_hook),
    ];
    let mut names = Vec::new();
    for (sym, f) in hooks {
        let at = vm.symbol(sym).expect("checked above");
        vm.install_hook(sym, at, f)?;
        names.push(sym);
    }
    Ok(names)
}

/// Her rows: `None` (기본 외형) first, then every outfit.
fn rows(vm: &Vm) -> Vec<Option<usize>> {
    std::iter::once(None).chain((0..vm.avatar.spirit_looks.len()).map(Some)).collect()
}

/// The spirit's sprite image and data, when she has been loaded.
fn spirit_sprite(vm: &Vm) -> Option<(u32, u32)> {
    let gv = gv(vm)?;
    let image = vm.mem.read_u32(gv + GV_SPIRIT_IMAGE);
    let data = vm.mem.read_u32(gv + GV_SPIRIT_DATA);
    (image != 0 && data != 0).then_some((image, data))
}

fn worn_id(vm: &Vm) -> Option<u32> {
    let (_, class, slot) = hero(vm)?;
    vm.avatar.spirit_worn(slot, class).map(|l| l.id)
}

fn sheets_for(look: Option<&SpiritLook>) -> (u16, u16) {
    look.map_or((HEAD_DEFAULT, BODY_DEFAULT), |l| (l.sheet, l.sheet))
}

pub(crate) fn sheet_path(dir: u8, sheet: u16) -> Vec<u8> {
    format!("/c/sp/img{dir}/{sheet:03}.mgr\0").into_bytes()
}

// ---- dressing --------------------------------------------------------------

/// `LoadSprImg` from `LoadSpiritRes`: her two paths are in `_szSprFile` now;
/// point them at her outfit and let the guest load.
fn load_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    let Some(at) = vm.symbol(LOAD_SPIRIT_RES) else { return Ok(false) };
    if lr != at + LOAD_SPIRIT_RES_LR {
        return Ok(false);
    }
    let Some((_, class, slot)) = hero(vm) else { return Ok(false) };
    let Some(look) = vm.avatar.spirit_worn(slot, class).cloned() else { return Ok(false) };
    let files = vm.symbol(SPR_FILE).ok_or("spirit avatar: _szSprFile vanished")?;
    vm.mem.write_bytes(files, &sheet_path(0, look.sheet));
    vm.mem.write_bytes(files + 0x100, &sheet_path(1, look.sheet));
    vm.avatar.spirit.dressed += 1;
    Ok(false)
}

/// Swap her two sheets in place through the guest's `ChangeSprImg`.
fn redress(vm: &mut Vm) -> Result<(), String> {
    let Some((image, data)) = spirit_sprite(vm) else { return Ok(()) };
    let (head, body) = {
        let id = worn_id(vm);
        let look = id.and_then(|id| vm.avatar.spirit_looks.iter().find(|l| l.id == id));
        sheets_for(look)
    };
    if vm.avatar.spirit.path_buf == 0 {
        vm.avatar.spirit.path_buf = vm.heap.alloc(0x20).ok_or("spirit avatar: guest heap full")?;
    }
    let buf = vm.avatar.spirit.path_buf;
    let change = vm.symbol(CHANGE_SPR_IMG).ok_or("spirit avatar: ChangeSprImg vanished")?;
    let pals = vm.symbol(SPR_PAL).ok_or("spirit avatar: _nSprPalFile vanished")?;
    for (layer, sheet) in [(0u8, head), (1u8, body)] {
        vm.mem.write_bytes(buf, &sheet_path(layer, sheet));
        // The palette slot is the hero's by now; hers is none.
        let slot = pals + layer as u32 * 2;
        let kept = vm.mem.read_u16(slot);
        vm.mem.write_u16(slot, 0xffff);
        let r = vm.call(change, &[image, data, layer as u32, buf]);
        vm.mem.write_u16(slot, kept);
        r?;
    }
    vm.avatar.spirit.redressed += 1;
    Ok(())
}

// ---- drawing ---------------------------------------------------------------

fn outfit_name(vm: &mut Vm, i: usize) -> Option<u32> {
    let id = vm.avatar.spirit_looks[i].id;
    if let Some(p) = vm.avatar.spirit.names.get(&id) {
        return Some(*p);
    }
    let bytes = vm.avatar.spirit_looks[i].name.clone();
    let p = vm.heap.alloc(bytes.len() as u32)?;
    vm.mem.write_bytes(p, &bytes);
    vm.avatar.spirit.names.insert(id, p);
    Some(p)
}

fn fill(vm: &mut Vm, p: &Pen, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
    let f = vm.symbol(FILL_RECT).ok_or("spirit avatar: FillRect vanished")?;
    vm.call(f, &[p.graphic, p.fb, x as u32, y as u32, w as u32, h as u32, PANEL_BG])?;
    Ok(())
}

fn draw_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.arg(0);
    let f = vm.symbol(DRAW_SPIRIT_MENU).ok_or("spirit avatar: DrawSpiritMenu vanished")?;
    vm.call(f, &[this])?;
    if this == 0 || vm.avatar.spirit_looks.is_empty() || spirit_sprite(vm).is_none() {
        return Ok(true);
    }
    let now = vm.guest_repaints();
    if vm.avatar.spirit.open && now.saturating_sub(vm.avatar.spirit.last_draw) > 2 {
        vm.avatar.spirit.open = false;
    }
    vm.avatar.spirit.last_draw = now;
    if vm.mem.read_u8(this + MENU_SUBPAGE) != 0 {
        vm.avatar.spirit.open = false;
        return Ok(true);
    }
    let (Some((x0, y0)), Some(p)) = (origin(vm), pen(vm)) else { return Ok(true) };
    let open = vm.avatar.spirit.open;
    if open {
        draw_wardrobe(vm, &p, x0, y0, now)?;
    }
    plate(vm, &p, x0, y0 + BAR_Y, BAR_W, BAR_H, if open { LIT } else { BAR })?;
    if let Some(s) = text(vm, LABEL_BUTTON) {
        label(vm, &p, x0 + BAR_LABEL.0, y0 + BAR_LABEL.1, s, 1, if open { FG } else { FG_DIM })?;
    }
    vm.avatar.spirit.buttons_drawn += 1;
    Ok(true)
}

fn draw_wardrobe(vm: &mut Vm, p: &Pen, x0: i32, y0: i32, now: u64) -> Result<(), String> {
    for (x, y, w, h) in [RIGHT, UNDER] {
        fill(vm, p, x0 + x, y0 + y, w, h)?;
    }
    let rows = rows(vm);
    let worn = worn_id(vm);
    let first = vm.avatar.spirit.first;
    for r in 0..ROWS {
        let Some(&row) = rows.get(first + r) else { break };
        let x = x0 + ROW_X;
        let y = y0 + ROW_Y + ROW_PITCH * r as i32;
        let lit = first + r == vm.avatar.spirit.cursor;
        plate(vm, p, x, y, ROW_W, PLATE_H, if lit { LIT } else { ROW })?;
        let (name, on) = match row {
            None => (text(vm, LABEL_NONE), worn.is_none()),
            Some(i) => (outfit_name(vm, i), Some(vm.avatar.spirit_looks[i].id) == worn),
        };
        if let Some(s) = name {
            label(vm, p, x + 5, y + 3, s, 0, if lit { FG } else { FG_DIM })?;
        }
        if on {
            if let Some(s) = text(vm, LABEL_WORN) {
                label(vm, p, x + ROW_W - 14, y + 3, s, 1, FG_WORN)?;
            }
        }
    }
    if let Some((at, h)) = thumb(rows.len(), first) {
        let f = vm.symbol(FILL_RECT).ok_or("spirit avatar: FillRect vanished")?;
        let track_y = y0 + ROW_Y;
        for (y, h, colour) in [(track_y, TRACK_H, TRACK), (track_y + at, h, THUMB)] {
            vm.call(f, &[p.graphic, p.fb, (x0 + SCROLL_X) as u32, y as u32, SCROLL_W as u32, h as u32, colour])?;
        }
    }
    // Her front and her back, from the same sprite the page draws.
    let Some(g) = gv(vm) else { return Ok(()) };
    let spirit = vm.mem.read_u32(g + GV_SPIRIT);
    if spirit != 0 {
        let get = vm.symbol(GET_SPRITE_PTR).ok_or("spirit avatar: GetSpritePtr vanished")?;
        let draw = vm.symbol(DRAW_SPRITE).ok_or("spirit avatar: DrawSprite vanished")?;
        let sprite = vm.call(get, &[spirit])?;
        if sprite != 0 {
            let frame = (now / 2 % 6) as u32;
            for ((x, y), dir) in [(FRONT_AT, DIR_FRONT), (BACK_AT, DIR_BACK)] {
                vm.call(draw, &[sprite, p.fb, (x0 + x) as u32, (y0 + y) as u32, 0, dir, frame, 0])?;
            }
        }
    }
    Ok(())
}

// ---- input -----------------------------------------------------------------

fn open(vm: &mut Vm) {
    let rows = rows(vm);
    let worn = worn_id(vm);
    let at = rows
        .iter()
        .position(|r| match r {
            None => worn.is_none(),
            Some(i) => Some(vm.avatar.spirit_looks[*i].id) == worn,
        })
        .unwrap_or(0);
    let w = &mut vm.avatar.spirit;
    w.open = true;
    w.opened += 1;
    w.first = 0;
    move_cursor(w, at, rows.len());
}

fn move_cursor(w: &mut Wardrobe, to: usize, len: usize) {
    let to = to.min(len.saturating_sub(1));
    w.cursor = to;
    if to < w.first {
        w.first = to;
    } else if to >= w.first + ROWS {
        w.first = to + 1 - ROWS;
    }
}

/// Wear the row under the cursor; `None` is her own look.
fn wear_cursor(vm: &mut Vm) -> Result<(), String> {
    let Some((_, class, slot)) = hero(vm) else { return Ok(()) };
    let rows = rows(vm);
    let Some(&row) = rows.get(vm.avatar.spirit.cursor) else { return Ok(()) };
    let id = row.map_or(0, |i| vm.avatar.spirit_looks[i].id);
    match vm.avatar.set_spirit(slot, class, id) {
        Ok(false) => return Ok(()),
        Ok(true) => {}
        Err(e) => vm.log.push(e),
    }
    redress(vm)
}

fn key_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let this = vm.arg(0);
    let key = vm.arg(1) as i32;
    if this == 0 || vm.avatar.spirit_looks.is_empty() || spirit_sprite(vm).is_none() {
        return Ok(false);
    }
    if vm.mem.read_u8(this + MENU_SUBPAGE) != 0 {
        vm.avatar.spirit.open = false;
        return Ok(false);
    }
    if !vm.avatar.spirit.open {
        if key == K_STAR || key == K_ZERO {
            open(vm);
            return Ok(true);
        }
        return Ok(false);
    }
    if key == K_OK {
        wear_cursor(vm)?;
        return Ok(true);
    }
    let len = rows(vm).len();
    let w = &mut vm.avatar.spirit;
    match key {
        K_UP => {
            let c = w.cursor;
            move_cursor(w, if c == 0 { len - 1 } else { c - 1 }, len);
        }
        K_DOWN => {
            let c = w.cursor;
            move_cursor(w, if c + 1 >= len { 0 } else { c + 1 }, len);
        }
        K_LEFT => {
            let c = w.cursor;
            move_cursor(w, c.saturating_sub(ROWS), len);
        }
        K_RIGHT => {
            let c = w.cursor;
            move_cursor(w, (c + ROWS).min(len - 1), len);
        }
        K_STAR | K_ZERO | K_CLEAR => w.open = false,
        K_HASH => {
            w.open = false;
            return Ok(false);
        }
        _ => {}
    }
    Ok(true)
}

fn inside(d: (i32, i32), r: (i32, i32, i32, i32)) -> bool {
    (r.0..r.0 + r.2).contains(&d.0) && (r.1..r.1 + r.3).contains(&d.1)
}

/// The button answers a tap whether or not the wardrobe is up; while it is
/// up a press on the list arms a drag (the row is lit now, worn on release
/// unless the finger moved — the character page's rule, `avatar` §59), the
/// rest of its area is its own, and a tap anywhere else closes it and goes to
/// the guest.
fn touch_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    vm.avatar.spirit.drag = None;
    let this = vm.arg(0);
    if this == 0 || vm.avatar.spirit_looks.is_empty() || spirit_sprite(vm).is_none() {
        return Ok(false);
    }
    if vm.mem.read_u8(this + MENU_SUBPAGE) != 0 {
        return Ok(false);
    }
    let (x, y) = (vm.arg(1) as i32, vm.arg(2) as i32);
    let Some((x0, y0)) = origin(vm) else { return Ok(false) };
    let d = (x - x0, y - y0);
    if (0..=BAR_W).contains(&d.0) && d.1 > BAR_Y - 1 && d.1 <= BAR_Y + BAR_H {
        if vm.avatar.spirit.open {
            vm.avatar.spirit.open = false;
        } else {
            open(vm);
        }
        return Ok(true);
    }
    if !vm.avatar.spirit.open {
        return Ok(false);
    }
    let list = (ROW_X, ROW_Y, SCROLL_X + SCROLL_W - ROW_X, ROW_PITCH * ROWS as i32);
    if inside(d, list) {
        let r = ((d.1 - ROW_Y) / ROW_PITCH) as usize;
        let len = rows(vm).len();
        let to = vm.avatar.spirit.first + r;
        let row = (d.0 < ROW_X + ROW_W && to < len).then_some(to);
        if let Some(to) = row {
            move_cursor(&mut vm.avatar.spirit, to, len);
        }
        vm.avatar.spirit.drag = Some((y, vm.avatar.spirit.first, row, false));
        return Ok(true);
    }
    if inside(d, RIGHT) || inside(d, UNDER) {
        return Ok(true);
    }
    vm.avatar.spirit.open = false;
    Ok(false)
}

/// `StateInGameMenu::TouchMoved`, handed on by `avatar` when its own list
/// holds no finger: scroll by whole rows past the slop.
pub(crate) fn move_hook(vm: &mut Vm) -> Result<bool, String> {
    let Some((y0, first, row, moved)) = vm.avatar.spirit.drag else { return Ok(false) };
    if !entered_normally(vm) || !vm.avatar.spirit.open {
        return Ok(false);
    }
    let y = vm.arg(2) as i32;
    if !moved && (y - y0).abs() < DRAG_SLOP {
        return Ok(false);
    }
    vm.avatar.spirit.drag = Some((y0, first, row, true));
    scroll_to(vm, y0, first, y);
    Ok(false)
}

/// `StateInGameMenu::TouchReleased`, likewise: a tap wears its row, a drag
/// stops where the finger lifted (the release, not the last move, is where
/// it was — `avatar::release_hook` says why).
pub(crate) fn release_hook(vm: &mut Vm) -> Result<bool, String> {
    let Some((y0, first, row, moved)) = vm.avatar.spirit.drag.take() else { return Ok(false) };
    if !entered_normally(vm) || !vm.avatar.spirit.open {
        return Ok(false);
    }
    let y = vm.arg(2) as i32;
    if moved || (y - y0).abs() >= DRAG_SLOP {
        scroll_to(vm, y0, first, y);
        return Ok(false);
    }
    if let Some(row) = row {
        let len = rows(vm).len();
        if row < len {
            move_cursor(&mut vm.avatar.spirit, row, len);
            wear_cursor(vm)?;
        }
    }
    Ok(false)
}

/// The list's first row for a finger that went down at `from_y` over a list
/// at `first` and is now at `y`: whole rows, nearest, in range.
fn scrolled_first(first: usize, from_y: i32, y: i32, len: usize) -> usize {
    let rows = ((from_y - y) as f32 / ROW_PITCH as f32).round() as i64;
    (first as i64 + rows).clamp(0, len.saturating_sub(ROWS) as i64) as usize
}

fn scroll_to(vm: &mut Vm, from_y: i32, first: usize, y: i32) {
    let len = rows(vm).len();
    let to = scrolled_first(first, from_y, y, len);
    let w = &mut vm.avatar.spirit;
    if to != w.first {
        w.first = to;
        w.cursor = w.cursor.clamp(to, to + ROWS - 1).min(len.saturating_sub(1));
    }
}

/// The track's height: six plates.
const TRACK_H: i32 = ROW_PITCH * (ROWS as i32 - 1) + PLATE_H;

/// The scroll thumb, `(offset, height)`, or `None` when every row fits.
fn thumb(len: usize, first: usize) -> Option<(i32, i32)> {
    if len <= ROWS {
        return None;
    }
    let h = (TRACK_H * ROWS as i32 / len as i32).max(6);
    let span = (len - ROWS) as i32;
    Some(((TRACK_H - h) * first.min(len - ROWS) as i32 / span, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wardrobe_fits_the_panel_and_clears_her_figure() {
        // The panel is 0xb0 wide and the bar strip starts at BAR_Y.
        assert!(RIGHT.0 + RIGHT.2 <= 0xb0);
        assert!(RIGHT.1 + RIGHT.3 <= BAR_Y && UNDER.1 + UNDER.3 <= BAR_Y);
        assert!(ROW_X >= RIGHT.0 && ROW_X + ROW_W <= RIGHT.0 + RIGHT.2);
        assert!(ROW_Y + ROW_PITCH * (ROWS as i32 - 1) + PLATE_H <= RIGHT.1 + RIGHT.3);
        // Her figure: feet at (0x22, 0x57), about 12 px either side and 38 up.
        let (fx, fy) = (0x22, 0x57);
        assert!(fx + 12 <= RIGHT.0, "the right column covers her");
        assert!(fy <= UNDER.1, "the lower block covers her feet");
        assert!(fy - 38 >= RIGHT.1 - 0x10);
    }

    #[test]
    fn sheet_paths_name_her_two_directories() {
        assert_eq!(sheet_path(0, HEAD_DEFAULT), b"/c/sp/img0/042.mgr\0");
        assert_eq!(sheet_path(1, BODY_DEFAULT), b"/c/sp/img1/025.mgr\0");
        assert!(sheet_path(1, 999).len() <= 0x20, "fits the path buffer");
        assert_eq!(sheets_for(None), (HEAD_DEFAULT, BODY_DEFAULT));
    }
}
