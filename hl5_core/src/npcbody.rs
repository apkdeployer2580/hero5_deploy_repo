//! 컷신의 아바타 — the hero's and 렌's NPC-body copies wear the wardrobe too.
//!
//! The player dressed the hero and 렌 ([`crate::avatar`], [`crate::spirit_avatar`])
//! and then watched both change back into their equipment the moment a quest
//! scene opened its dialogue. A story scene does not move the hero's own
//! sprite about: the script function `EventProc::Event_PlayerChange(1)`
//! (0x6DA78) builds the hero **anew as an NPC body**. `Map::MapNpcG_set(map,
//! &rec, 0)` reads record 0 of `/c/csv/npc_g.dat` — type byte 0, the one
//! record that means "the player" — `Map::NpcPathSet(&rec)` (0xAE9F4) writes
//! `_szSprFile` and `_nSprPalFile` for it, and `SPRITE_IMAGE::LoadNpcSprImg`
//! reads the sheets from those two tables under cif `000`. For a type-0 record
//! `NpcPathSet` names layers 0..3 from the equipped helmet (subject to
//! 투구보기), armour, gloves and boots, exactly as `HERO::SetLoadImgPath`
//! does — but it is not `SetLoadImgPath`, so [`crate::avatar::load_hook`]
//! never saw it, and the hero walked into every dialogue in the clothes of
//! his equipment. `Event_PlayerChange(0)`, at the scene's end or from the
//! script, puts the hero's own (dressed) sprite back: that is the change the
//! player saw.
//!
//! 렌 is an NPC in scenes as well: record 1 (type 53) names her own sheets,
//! `img0/042` and `img1/025` — the pair `Battle::LoadSpiritRes` loads for
//! the `Spirit` that follows the hero — and `Map::NpcCreate` / `NpcChange`
//! reach `NpcPathSet` for her like for every other NPC. So does
//! `QuestMgr::LoadNpcBody`, the figure in the quest window. Four callers,
//! one function, and it writes exactly the two tables the wardrobe knows.
//!
//! [`path_hook`] runs the guest's `NpcPathSet` and then dresses the record it
//! was handed: type 0 → the hero's worn looks, on every layer the guest gave a
//! sheet (the player record leaves the weapon and shield at `empty.mgr`, and
//! that stays — a scene's hero is unarmed by the record's say-so; the effect
//! layer is one `LoadNpcSprImg` skips); her head-and-body pair → her outfit.
//! Nothing else is touched. The NPC cif is the guest's (`000` for the player,
//! the record's type for her), so a rigged sheet shows its first images — the
//! archive's own poses — which is what every NPC body shows.
//!
//! The record is read **after** the guest ran, from the caller's own
//! `_NPC_G_DATA` (`+0` type, `+2..+10` the nine layer ids); the hook dresses
//! from what is in the tables, never from what it assumes the guest wrote.

use crate::avatar::{dress_layer, entered_normally, hero, SPR_FILE, TABS};
use crate::Vm;

pub(crate) const NPC_PATH_SET: &str = "_ZN3Map10NpcPathSetEP11_NPC_G_DATA";

/// `_NPC_G_DATA`: the type byte, then the nine layer image numbers.
const REC_TYPE: u32 = 0;
const REC_IDS: u32 = 2;
/// Type 0 is the player (record 0 of `npc_g.dat`; `Event_PlayerChange(1)`
/// asks for index 0 by a literal).
const TYPE_PLAYER: i8 = 0;

/// Whose body a record describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    Hero,
    Spirit,
    Other,
}

/// Classify a record by its type byte and first two layer ids: the player by
/// type, 렌 by her sheets (her record's type is 53, but the sheets are what
/// the costume replaces, so they are the test).
pub fn body_of(kind: i8, head: u8, body: u8) -> Body {
    if kind == TYPE_PLAYER {
        Body::Hero
    } else if (head, body) == crate::spirit_avatar::own_sheets() {
        Body::Spirit
    } else {
        Body::Other
    }
}

/// Install the hook; `avatar` and `spirit_avatar` must already be up (their
/// tables are what this dresses from).
pub fn install(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for s in [NPC_PATH_SET, SPR_FILE] {
        if vm.symbol(s).is_none() {
            return Err(format!("cutscene avatar needs `{s}`, not in this binary"));
        }
    }
    let at = vm.symbol(NPC_PATH_SET).expect("checked above");
    vm.install_hook(NPC_PATH_SET, at, path_hook)?;
    Ok(vec![NPC_PATH_SET])
}

/// True when the guest left no sheet in that layer: an empty string or its
/// own `/c/sp/empty/empty.mgr`.
fn layer_blank(vm: &Vm, files: u32, layer: u8) -> bool {
    let p = files + layer as u32 * 0x100;
    let first = vm.mem.read_u8(p);
    if first == 0 {
        return true;
    }
    let s: Vec<u8> = (0..0x20).map(|i| vm.mem.read_u8(p + i)).take_while(|b| *b != 0).collect();
    s.ends_with(b"/empty.mgr")
}

/// `Map::NpcPathSet(this, rec)`: run it, then dress the body it named.
fn path_hook(vm: &mut Vm) -> Result<bool, String> {
    if !entered_normally(vm) {
        return Ok(false);
    }
    let (this, rec) = (vm.arg(0), vm.arg(1));
    let f = vm.symbol(NPC_PATH_SET).ok_or("cutscene avatar: NpcPathSet vanished")?;
    vm.call(f, &[this, rec])?;
    if rec == 0 {
        return Ok(true);
    }
    let kind = vm.mem.read_u8(rec + REC_TYPE) as i8;
    let head = vm.mem.read_u8(rec + REC_IDS);
    let body = vm.mem.read_u8(rec + REC_IDS + 1);
    match body_of(kind, head, body) {
        Body::Hero => dress_hero_body(vm)?,
        Body::Spirit => dress_spirit_body(vm)?,
        Body::Other => {}
    }
    Ok(true)
}

/// The player record: every layer the guest gave a sheet, in what the hero
/// wears there. The weapon stays whatever the record said (empty), so no
/// effect sheet either.
fn dress_hero_body(vm: &mut Vm) -> Result<(), String> {
    let Some((_, class, slot)) = hero(vm) else { return Ok(()) };
    let files = vm.symbol(SPR_FILE).ok_or("cutscene avatar: _szSprFile vanished")?;
    let mut n = 0;
    for tab in &TABS {
        if layer_blank(vm, files, tab.layer) {
            continue;
        }
        let Some(look) = vm.avatar.worn(slot, class, tab.layer).cloned() else { continue };
        dress_layer(vm, tab.layer, look.sheet, look.palette);
        n += 1;
    }
    if n > 0 {
        vm.avatar.bodies_dressed += 1;
        vm.log.push(format!("avatar: the hero's scene body dressed ({n} layer(s))"));
    }
    Ok(())
}

/// 렌's record: her two sheets, as `spirit_avatar::load_hook` rewrites them
/// on the field sprite. The palette slots are already −1 (the record's).
fn dress_spirit_body(vm: &mut Vm) -> Result<(), String> {
    let Some((_, class, slot)) = hero(vm) else { return Ok(()) };
    let Some(look) = vm.avatar.spirit_worn(slot, class).cloned() else { return Ok(()) };
    let files = vm.symbol(SPR_FILE).ok_or("cutscene avatar: _szSprFile vanished")?;
    for layer in [0u8, 1] {
        if layer_blank(vm, files, layer) {
            continue;
        }
        vm.mem.write_bytes(files + layer as u32 * 0x100, &crate::spirit_avatar::sheet_path(layer, look.sheet));
    }
    vm.avatar.spirit.bodies_dressed += 1;
    vm.log.push(format!("avatar: 렌's scene body dressed (sheet {})", look.sheet));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three records at the head of the shipped `npc_g.dat` (81 records,
    /// 25 bytes each): the player, 렌, and an ordinary NPC.
    const PLAYER: (i8, [u8; 9]) = (0, [0, 0, 0, 0, 255, 22, 255, 255, 255]);
    const REN: (i8, [u8; 9]) = (53, [42, 25, 255, 255, 255, 21, 255, 255, 255]);
    const OTHER: (i8, [u8; 9]) = (54, [29, 20, 17, 17, 255, 23, 255, 255, 255]);

    #[test]
    fn the_player_record_is_the_hero_by_type_alone() {
        assert_eq!(body_of(PLAYER.0, PLAYER.1[0], PLAYER.1[1]), Body::Hero);
        // A type-0 record is the hero whatever its ids say: layers 0..3 come
        // from the equipment, not the record.
        assert_eq!(body_of(0, 42, 25), Body::Hero);
    }

    #[test]
    fn ren_is_known_by_her_sheets() {
        assert_eq!(body_of(REN.0, REN.1[0], REN.1[1]), Body::Spirit);
        // Her sheets are the ones `LoadSpiritRes` names and the costumes replace.
        assert_eq!(crate::spirit_avatar::own_sheets(), (42, 25));
        assert_eq!(body_of(OTHER.0, OTHER.1[0], OTHER.1[1]), Body::Other);
        // Half a match is not her.
        assert_eq!(body_of(53, 42, 20), Body::Other);
    }
}
