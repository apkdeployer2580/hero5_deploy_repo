//! The cash shop without a network — 네트워크 없는 캐시샵.
//!
//! The feature has two halves and this module is the second of them.
//!
//! The **delivery** half is [`crate::localserver`]. Unlike the sibling
//! chronosSword, whose guest puts a bought item in the bag with local code,
//! this game's grant path runs through the wire — `ProcPacket_Shop`
//! (0xCD004) reads `_NET_ITEM_` records out of the category-6 subcmd-5
//! response and hands them to `BagItem::NewBagNetItem` (ANALYSIS.md §22.2).
//! So "shop without a server" means the host has to *be* the server for that
//! conversation, which is what the in-process transcription of
//! `hl5_server/server.py` does, byte for byte, under a fixture A/B
//! (`tests/localserver_ab.rs`, ANALYSIS.md §24).
//!
//! What that server could not know by itself was **which `_NET_ITEM_` a given
//! shop row should produce**. That mapping lived in the operator's 2011
//! database and never traveled the wire in a form anyone can still observe —
//! `hl5_server` answers subcmd 5 with zero records for exactly that reason,
//! so buying from the deployed server would not have revealed it either.
//! It was recovered from the guest instead, and [`GrantTable::build`] carries
//! the derivation and its proof.
//!
//! The **display** half is [`install_hooks`]: the user asked for the original
//! price struck through with `0원` beside it. Those are behaviour hooks on
//! the `BFont::DrawText` calls that draw a cash-shop price — three call
//! sites across two overloads, because the shop has two layouts. They live in
//! [`crate::Vm::hooks`], a map *separate* from [`crate::Vm::patches`] on
//! purpose: `patch.rs`'s contract is "ports that produce the same bytes",
//! proven by the offline 905-frame A/B, and these hooks deliberately change
//! what is drawn. Mixing them would make that A/B prove nothing (CLAUDE.md,
//! acceptance criterion 2).

use std::collections::HashMap;
use std::sync::Arc;

use arm32_cpu::{reg, Mode};

use crate::mem::RETURN_MAGIC;
use crate::Vm;

// ---- the cash-shop catalogue ------------------------------------------------
//
// `/c/csv/cash_single_android.dat`, decoded by `tools_re/cashcat.py`; the
// record layout mirrors `CashShop::LoadCashShopData` (0xDE07C) field by field
// (ANALYSIS.md §21). The host reads it only to default a grant's display name
// and quantity — the guest keeps loading its own copy through the VFS.

/// One catalogue row, fields named as in `cashcat.py`. `name` stays EUC-KR
/// bytes: it goes back onto the wire, and `String` would corrupt it.
pub struct CashRow {
    pub id: u8,
    pub param: u16,
    pub a: u8,
    pub count: u8,
    pub name: Vec<u8>,
    pub prices: [u16; 3],
}

/// `MIDASKernelManager::hash` (0x1516a4) — djb2 over the path with the
/// leading `/` already removed.
fn djb2(s: &[u8]) -> u32 {
    s.iter().fold(5381u32, |h, &c| h.wrapping_mul(33).wrapping_add(c as u32))
}

/// Find one entry in the VFS archive: little-endian `[u32 hash][u32 size]
/// [size bytes]` repeated (ANALYSIS.md §21).
pub(crate) fn vfs_entry<'a>(vfs: &'a [u8], path: &str) -> Option<&'a [u8]> {
    let want = djb2(path.as_bytes());
    let mut pos = 0usize;
    while pos + 8 <= vfs.len() {
        let hash = u32::from_le_bytes([vfs[pos], vfs[pos + 1], vfs[pos + 2], vfs[pos + 3]]);
        let size =
            u32::from_le_bytes([vfs[pos + 4], vfs[pos + 5], vfs[pos + 6], vfs[pos + 7]]) as usize;
        let start = pos + 8;
        let end = start.checked_add(size)?;
        if end > vfs.len() {
            return None;
        }
        if hash == want {
            return Some(&vfs[start..end]);
        }
        pos = end;
    }
    None
}

const CATALOG_PATH: &str = "c/csv/cash_single_android.dat";

/// Parse the catalogue. Truncated or malformed data yields the rows that did
/// parse — the archive is trusted input, this is just bounds discipline.
fn parse_catalog(d: &[u8]) -> Vec<CashRow> {
    let rd16 = |p: usize| -> Option<u16> { Some(u16::from_le_bytes([*d.get(p)?, *d.get(p + 1)?])) };
    let mut rows = Vec::new();
    let Some(count) = rd16(0) else { return rows };
    let mut pos = 2usize;
    for _ in 0..count {
        let Some(rlen) = rd16(pos) else { break };
        pos += 2;
        let Some(body) = d.get(pos..pos + rlen as usize) else { break };
        pos += rlen as usize;

        // u8 id, u16 param, u8 namelen + name, u8 a, u8 count,
        // u8 desclen + desc, u16 price, u16 price2, u16 price3.
        let b16 =
            |p: usize| -> Option<u16> { Some(u16::from_le_bytes([*body.get(p)?, *body.get(p + 1)?])) };
        let mut p = 0usize;
        let Some(&id) = body.get(p) else { break };
        let Some(param) = b16(p + 1) else { break };
        p += 3;
        let Some(&nlen) = body.get(p) else { break };
        p += 1;
        let Some(name) = body.get(p..p + nlen as usize) else { break };
        let name = name.to_vec();
        p += nlen as usize;
        let (Some(&a), Some(&qty)) = (body.get(p), body.get(p + 1)) else { break };
        p += 2;
        let Some(&dlen) = body.get(p) else { break };
        p += 1 + dlen as usize; // the description is display-only; skip it
        let (Some(p1), Some(p2), Some(p3)) = (b16(p), b16(p + 2), b16(p + 4)) else { break };
        rows.push(CashRow { id, param, a, count: qty, name, prices: [p1, p2, p3] });
    }
    rows
}

// ---- grants ------------------------------------------------------------------

/// The item table a cash-shop *listing* names.
///
/// `ItemTable::GetItemTableInfo` (0xA2728) switches `kind` over 19 tables
/// loaded from `/c/csv/item_%02d.dat` (`ItemTable::LoadItemTable`, 0xA38E0),
/// and `item_18.dat` is the `CashItemInfo` one. Most cash purchases really do
/// arrive as kind 18 — but not all of them do, and that was a bug for six
/// catalogue rows. See [`UsageClass`].
const CASH_ITEM_KIND: u8 = 18;

/// `item_12.dat` — 오브. Rows 1..13 are the thirteen plain orbs (0 is 빈소켓),
/// 14..26 the 하이퍼*1~3 band, then 2~4 and 3~5 bands of thirteen each.
const ORB_KIND: u8 = 12;
const ORB_PLAIN: (u8, u8) = (1, 13);
const ORB_HYPER: (u8, u8) = (14, 26);

/// `item_10.dat` — 장신구. Row 17 is 공허의 목걸이, the only 공허 accessory in
/// the table, and the target the necklace listing has no guest-side remap for.
const ACCESSORY_KIND: u8 = 10;
const VOID_NECKLACE_ROW: u8 = 17;

/// `item_11.dat` — 전투 소모품 (`BattlelUseItemInfo`), 16 rows. Row 8 is the
/// real 엘릭서: the last-but-one byte of its record is 250, the largest value
/// in the column that holds 4 / 10 / 20 for 포션 / 미들포션 / 하이포션.
///
/// That byte is the heal *parameter*, not the healed amount — the row's own
/// description reads `2500의 HP를 즉시 회복합니다.`, so something downstream
/// scales it, and the arithmetic disappears into `Formula::calc` (0x7749C).
/// The magnitude is deliberately not stated here: what matters for this remap
/// is that row 8 heals at all, and that it is the table's largest.
///
/// # Why the elixir has to arrive as this kind and not as its cash listing
///
/// `StateInGameMenu::KeyBagMenu+0xA2C` (0x109C1C) switches the bag's item menu
/// on the item's `kind` (`ldrsb [r0,#0x14]`, then `addls pc, pc, r3, lsl #2`
/// with case 0 one slot past the fall-through). **단축키등록 — the quickslot
/// entry — is in popup 3, and of the popups this menu can raise, kind 11 is
/// the only kind that reaches it.** Kind 18 can only reach popup 4 (버리기) or
/// popup 5 (사용하기 / 버리기), chosen by `ItemInfo+0x134`. (Popups 15 and 16
/// carry 단축키등록 too, but `KeyBagMenu` never asks for them — it raises only
/// 2, 3, 4, 5 and 10. Those two belong to the skill and property menus.)
///
/// So the original game cannot put a cash elixir in a quickslot either. It is
/// worse than that: `item_18.dat` row 34 has *no effect at all*. The kind-18
/// use path (0x10AEE0) dispatches on the row number and 0x10BB50's
/// `cmp index,#13 / bgt` sends everything above 13 to 0x10BEE8, which prints
/// `을(를) 사용했습니다.` and deletes the item — no heal, no failure message.
/// Granting `item_11.dat` row 8 instead trades a menu entry that does nothing
/// for one that heals 250, and it is the same item by name.
///
/// The wire does not change: `NETWORK::readNetItem` splits on
/// `ldrsb [item+4]; cmp #0xa; ble`, so kind 11 and kind 18 are both the short
/// record (10 bytes + name). Nor does the guest re-map it — `ProcPacket_Shop`
/// (0xCD38C) special-cases `kind == 18 && 44 <= index <= 48` and nothing else.
///
/// Row 10 (제련석) is the one row of this table the bag treats differently
/// (`ItemInfo+0x15 == 10` → popup 4), which is why the row number matters and
/// 8 is not it.
const BATTLE_USE_KIND: u8 = 11;
const ELIXIR_ROW: u8 = 8;

/// `item_05..08` — 투구 / 갑옷 / 장갑 / 장화, the four armour tables that any
/// class can wear, and the only equipment a host-side roll can pick.
///
/// Weapons (kind <= 4) and shields (kind 9) are excluded on purpose:
/// `EquipItemInfo::IsEquipPossible` sends those two to 0xA8248, which compares
/// the item's `+0x155` against **the hero's own class**, and
/// [`GrantTable::records`] is pure — it is answered from a table built at boot
/// and has no live `Vm` to ask which class is playing. The other five kinds go
/// to the common test at 0xA8214, which any class passes with `+0x155 == 5`.
///
/// Accessories (kind 10) would qualify too and are deliberately left out of
/// the relic boxes: `item_10` has 18 rows against these tables' 81+, and row
/// 17 is the 공허의 목걸이 the shop already sells on its own.
const ARMOUR_KINDS: [u8; 4] = [5, 6, 7, 8];

/// The value of `ItemInfo+0x155` that every class accepts.
///
/// Read off `EquipItemInfo::IsEquipPossible` (0xA81AC) rather than chosen:
/// classes 1 and 2 (ROGUE, GUNNER) require exactly 5 (0xA82A0), classes 0 and
/// 3 (WARRIOR, KNIGHT) accept 5 or 6 (0xA8224). Five is the intersection.
const ANY_CLASS: u8 = 5;

/// `item_00..03` — 대검 / 단검 / 권총 / 장창, the four weapon tables. **A
/// weapon's table is its class**: the wearer-group byte at `record[len-16]` is
/// 0 in every row of `item_00`, 1 in every row of `item_01`, and so on
/// (ANALYSIS.md §39.3's census), and `EquipItemInfo::IsEquipPossible` sends
/// weapons to 0xA8248, which compares that byte against the hero's own class.
///
/// §39 excluded them because [`GrantTable::records`] had no class to compare
/// with. It does now — [`hero_class`] asks the guest itself
/// (`HERO::GetPlayerClass`, 0x88914) when a session opens, and the pool draw
/// hands a weapon only to the class whose table it came from
/// ([`class_may_draw`]). A session that could not tell (no HERO loaded, or
/// `free_shop` off) still rolls armour only, which is exactly §39's grant.
const WEAPON_KINDS: [u8; 4] = [0, 1, 2, 3];

/// `item_09` — 방패, 81 rows, wearer group 3 = KNIGHT in every one (§39.3).
/// Same gate as the weapons, so it enters a pool only for class 3.
const SHIELD_KIND: u8 = 9;
const KNIGHT_CLASS: u8 = 3;

/// The last weapon/shield row a relic pool may hold. Rows 81..85 of
/// `item_00..03` are the 공허 1~5급 weapons — cash-shop deliverables with
/// their own listings (`ProcPacket_Shop`'s 44..48 remap), not relic contents.
/// The armour bands stop at 80 for their own reason (81+ is group 7); this
/// keeps the weapon draw inside the same fence.
const RELIC_ROW_MAX: u8 = 80;

/// How many pieces of a relic pool have an option slot the grade reserves but
/// the table leaves empty — i.e. how many used to show `+랜덤옵션` with nothing
/// behind it, and now get a rolled option instead.
///
/// This replaced a dead check. The host used to count rows whose *name* ended
/// in `"+랜덤 옵션"`, on the belief (ANALYSIS.md §40.1) that the phrase came
/// from the item table. It does not: a scan of all nineteen `item_NN.dat`
/// tables finds zero names containing 랜덤, so that counter always reported 0.
/// The phrase is `common_text` 287 — `+랜덤옵션`, and note it has no space,
/// so it was not even the same string — drawn by
/// `CommonUi::DrawItemDiscription` for any option slot whose kind byte is
/// `<= 0`. See [`fill_options`].
fn reserved_option_rows(rows: &[(u8, u8, [u8; EQUIP_BLOCK])]) -> usize {
    rows.iter()
        .filter(|(.., b)| !empty_option_lines(b[EQUIP_GRADE_AT], option_kinds(b)).is_empty())
        .count()
}

// The stat roll that used to live here has been removed. Two measurements
// took the ground out from under it, and both are worth writing down because
// the shape of the mistake is easy to repeat (ANALYSIS.md §45).
//
// # `ItemInfo+0x150` is not a stat — it is the sprite file number
//
// `HERO::SetLoadImgPath` (0x8DE58) builds the worn-equipment sprite paths from
// the equipped items, once per slot:
//
// ```text
//   0x8dee0  bl EquipItem::GetEquipItem(2)
//   0x8df00  mov r3,#0x150 / ldrsh r2,[r0,r3]      ; <- +0x150
//   0x8df18  bl MC_knlSprintk(_szSprFile+0x100, "/c/sp/img1/%03d.mgr", r2)
//   0x8df30  mov r3,#0x150 / add r3,r3,#2 / ldrh r3,[r0,r3]
//   0x8df40  strh r3,[_nSprPalFile, #2]            ; <- +0x152 is the palette
// ```
//
// Six slots, six format strings — `/c/sp/img{0,1,2,3,4,6}/%03d.mgr` — and
// `StateInGameMenu::KeySystemMenu+0x9d0` does the same again for the character
// sheet's preview. Nothing anywhere reads `+0x150` as a number the player sees.
//
// The archive settles it. Every equipment table's `+0x150` range ends exactly
// where its sprite directory's last file does:
//
// ```text
//   item_05 투구  108..123 -> /c/sp/img0/  files 0..127
//   item_06 갑옷    2..56  -> /c/sp/img1/  files 0..56
//   item_07 장갑    2..48  -> /c/sp/img2/  files 0..48
//   item_08 장화    2..48  -> /c/sp/img3/  files 0..48
//   item_00..03 무기 1..64 -> /c/sp/img4/  files 0..67
//   item_09 방패    1..16  -> /c/sp/img6/  files 0..16
// ```
//
// Scaling that word by 10..30% therefore asks for a `.mgr` that is not in the
// archive; the load fails, the layer draws nothing, and a 유물함 helmet takes
// the player's head off. That is the bug this removal fixes. `+0x152` was
// being scaled too — that is the palette — and so was `+0x15a`, which
// `RefineItem::ApplyItemDecompose+0x404` reads back.
//
// # …and the two that really are stats do not survive a restart
//
// `Formula::getValFunc` reads exactly `+0x156` and `+0x158` off an `ItemBase*`
// (0x7602c, 0x7603c) — those two are the item's numbers. But
// `HERO::SaveBagData` (0x8F4F4) does not write them: a saved bag item is
// `kind`, `index`, three int64s, and `+0x15c..+0x16D`. `HERO::LoadBagData`
// (0x8EB4C) rebuilds the rest by calling `ItemTable::GetItemTableInfo`
// (0xA2728) -> `EquipItemInfo::CopyData`, which refills `+0x150..+0x15a`
// **from the table**. A rolled stat therefore lived until the next app start
// and then silently reverted, and an item that quietly gets weaker is worse
// than an item that was never boosted.
//
// # Nothing the player keeps is lost by dropping it
//
// The doc this replaces said the roll sat in the stats only because the option
// slots could not be decoded yet. They can: `/c/csv/option.dat` was read in
// §41.9 and [`fill_options`] rolls the reserved slot the way
// `ItemTable::SetItemOption` (0xA0FF8) does. Option pairs *are* saved
// (`SaveBagData` writes `+0x15f..+0x164`), and so are the sockets and the
// level relief. Every part of a 유물함 draw that survives a restart is still
// rolled; only the part that was both invisible-after-restart and
// head-removing is gone.

/// `ItemTable::MakeSocket`'s own socket-count table, lifted out of
/// `/c/csv/probability.dat` (ANALYSIS.md §41.1 block A, `ItemTable+0x78`).
///
/// Indexed by the item's grade byte `ItemInfo+0x15c`, then walked against
/// `Rand(0,999)`: the first threshold the roll falls **below** is the socket
/// count (0xA1298 `blt`), and running off the end of the six gives 0 (0xA12A8).
/// The loader transposes this block, so one grade's column is what `MakeSocket`
/// reads as six consecutive halfwords at `this + 0x78 + 12*grade`.
///
/// ```text
///   grade 0/1/2 (노멀·레어·에픽):  50 / 20 / 15 / 10 / 4 / 1 %
///   grade 3 (영웅) and 5.. (세트):  70 / 20 / 10 %
/// ```
///
/// Grade 4 (전설) has no column at all — `MakeSocket` refuses it outright
/// (0xA1258 `beq`), so a legendary is always created with zero sockets. That is
/// the game's own rule and it is left alone: a zero-socket item is still
/// socket-**expandable**, which is the thing that was actually broken.
const SOCKET_CDF: [[u16; 6]; 2] = [
    [500, 700, 850, 950, 990, 1000],    // grades 0, 1, 2
    [700, 900, 1000, 1000, 1000, 1000], // grade 3 and the set grades
];

/// Highest socket count the guest can represent: `MakeSocket` clears exactly
/// five slot bytes at `ItemInfo+0x169` (`memset(item+0x169, -1, 5)`, 0xA12BC).
pub(crate) const SOCKET_SLOTS: u8 = 5;

/// Roll a socket count for a granted piece exactly the way
/// `ItemTable::MakeSocket` (0xA1224) rolls one for an item the guest created
/// itself. A wire-granted item runs neither socket roller, so without this it
/// would arrive with a count the game never chose.
///
/// The gates are the guest's, in its order: `kind > 9` refuses (0xA1238),
/// wearer group 7 refuses (0xA1244), grade 4 refuses (0xA1258). Entropy is
/// [`mix`]ed from the draw cursor — the rule every other relic roll follows —
/// so a probe replay reproduces.
///
/// **Zero is a normal answer**, and for most grades the likeliest one. It is
/// not a defect: see [`crate::localserver::net_item_granted`] for why a
/// zero-socket item still has to say so in the right five bytes.
fn roll_sockets(kind: u8, group: u8, grade: u8, salt: u64) -> u8 {
    if kind > 9 || group == 7 || grade == 4 {
        return 0;
    }
    let col = usize::from(grade > 2);
    let roll = (mix(salt.wrapping_mul(8).wrapping_add(5)) % 1000) as u16;
    SOCKET_CDF[col]
        .iter()
        .position(|&t| roll < t)
        .map_or(0, |i| i as u8)
        .min(SOCKET_SLOTS)
}

/// What each 유물함 draws from, by its `item_18.dat` row.
///
/// # Why equipment at all
///
/// The boxes are class-1 listings — the operator's server opened them and sent
/// the result, so the client cannot say what came out, and for a long time
/// they were refused for exactly that reason. But the game does describe them.
/// `/c/csv/tips.dat` tip 24:
///
/// ```text
///   유물함에서는 제한 레벨이 낮아진 아이템을 얻을 수 있습니다.
///   "from a relic chest you can obtain items whose level restriction is lowered"
/// ```
///
/// A level restriction is a property of equipment, and the mechanic is real
/// and named in the binary: `EquipItemInfo::GetRelieveLevelLimit` (0xA835C)
/// sums the item's three option slots whose **type is 0x6c** and
/// `GetLevelLimit` (0xA83B0) subtracts that from the base limit at `+0x15d`.
/// So the boxes sold equipment with the level-relief option rolled on it.
///
/// # Why these bands
///
/// The four tables are tier-ordered by row index, and that is not an
/// inference from the names: a player-kept auction price list from the live
/// game (supplied 2026-08-21) prices the sets in exactly ascending row order —
/// 가시나무 (row 23) cheapest through 지크문드 (row 80) dearest — and 42 of
/// its 46 names resolve to rows of these tables. The four bands follow the
/// four prices, 1,500 / 2,000 / 2,500 / 3,000원, and every index in them was
/// checked for placeholder rows.
///
/// This is the one part of the mapping that is a **decision** rather than a
/// recovered fact, and it is recorded as such: the tip settles *equipment*,
/// the price list settles *which end of the table is better*, and the band
/// edges are ours.
const RELIC_BANDS: [(u8, (u8, u8)); 4] = [
    (15, (20, 30)), // 작은 유물함   1,500원
    (16, (40, 50)), // 유물함       2,000원
    (17, (60, 70)), // 큰 유물함     2,500원
    (18, (74, 80)), // 오래된 유물함  3,000원
];

/// What a cash-shop row *does*, from the last-but-one byte of its
/// `item_18.dat` record.
///
/// `CashItemInfo::CopyData+0x68` keeps that byte at `ItemInfo+0x134`, and
/// `StateInGameMenu::KeyBagMenu+0xAF4` switches the bag's item menu on it: 2
/// gives popup 4 (버리기), 3 gives popup 5 (사용하기 / 버리기), and **anything
/// else falls out with no popup at all**. So the byte is not decoration — it
/// decides whether a bag item can be interacted with, and the values partition
/// the 49 rows exactly:
///
/// ```text
///   0  account service   창고확장(nt), 프리미엄판매, 별의 문장, 유료전환 …
///                        never a bag item
///   1  random box        작은오브원석, 오브원석, 하이퍼오브, 유물함 x4
///                        — every one of the seven says so in its own
///                          description: "랜덤 오브를 5개 얻습니다"
///   2  consumed elsewhere  고급제련석, 부활의 부적, 공허 무기 … → 버리기
///   3  used from the bag   엘릭서, 오토루팅, 혼 …  → 사용하기 / 버리기
/// ```
///
/// # This is the orb bug
///
/// A class-1 row is a **shop listing, not a deliverable**: the operator's
/// server rolled a real item and sent *that*. Handing the player the kind-18
/// listing token instead produces an item no screen in the game accepts — the
/// bag draws its tooltip and then `KeyBagMenu` returns without a popup, so
/// tapping it appears to do nothing at all. That is what the user hit after
/// buying 하이퍼오브: not a missing 사용하기, but a missing *menu*.
///
/// (An orb has no 사용하기 by design even when it is the real thing. Kind 12
/// goes to popup 4, 버리기 only, and orbs are consumed at the engineer's
/// 오브결합 screen — whose picker is `SetItemFilter(6)`, and filter 6 is one
/// instruction: `SetItemFilter+0x3D0 cmp r1,#0xc`, kind 12 and nothing else.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UsageClass {
    /// Not a bag item at all.
    Service,
    /// A box the server was supposed to open.
    RandomBox,
    /// A bag item; 버리기 only.
    Consumed,
    /// A bag item with 사용하기.
    Usable,
    /// A value this build has never seen. Treated like `Service`: refused
    /// rather than granted, because `KeyBagMenu` would give it no menu.
    Unknown,
}

impl UsageClass {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Service,
            1 => Self::RandomBox,
            2 => Self::Consumed,
            3 => Self::Usable,
            _ => Self::Unknown,
        }
    }
}

/// One row of an `item_%02d.dat` table, as much of it as the host needs.
struct ItemRow {
    /// EUC-KR, straight from the archive; it goes back onto the wire.
    name: Vec<u8>,
    /// The last-but-one byte. Only meaningful for `item_18.dat`.
    class: u8,
    /// Who may wear it — the record byte that becomes `ItemInfo+0x155`. Only
    /// meaningful for the equipment tables (`item_00..10`).
    ///
    /// # How the offset was found, since it is not obvious
    ///
    /// `ItemTable::LoadItemTable` walks one cursor: `+0x154` comes from
    /// `sb+4` and `+0x155` from `sb+5` (0xA3EEC / 0xA3F00), and later in the
    /// **same** cursor `+0x15c` comes from `sb+0xc` and `+0x15d` — the level
    /// limit — from `sb+0xd` (0xA3F6C / 0xA3F80). So the group sits exactly
    /// eight bytes before the level. The level itself was pinned from the
    /// other end: `record[len-8]` reproduces fourteen independently known
    /// values (수르트 34, 무스펠 57, 그람 82, 기사의징표 1, …), which puts the
    /// group at `record[len-16]`.
    ///
    /// The result reads as designed data, which is the real check: `item_00`
    /// is 0 in all 86 rows, `item_01` all 1, `item_02` all 2, `item_03` all 3
    /// — a weapon's table *is* its class — `item_09` (방패) is 3 throughout,
    /// so shields are knight-only, and the four armour tables split 41 rows of
    /// 5 against 40 of 6. That split matches a player-kept guide independently:
    /// 푸른비룡 and 지크문드 are 6 and it calls them 워리어 세트, 아체야키 is 5
    /// and it calls that one 공용.
    group: u8,
    /// The base level limit, `ItemInfo+0x15d` — `record[len-8]`. Read for
    /// diagnostics and for the boot log; the wire deliberately sends 0.
    level: u8,
    /// The record's last 21 bytes: everything the guest keeps at
    /// `ItemInfo+0x150..+0x164` — the five stat words, the wearer group, the
    /// level limit and the three option pairs. `None` when the record is too
    /// short to have them, which no equipment row is.
    ///
    /// This block exists because **the wire overwrites all of it**. See
    /// [`EQUIP_BLOCK`].
    equip: Option<[u8; EQUIP_BLOCK]>,
}

/// How many bytes of an equipment record the wire has to carry, and why it has
/// to carry them at all.
///
/// `BagItem::NewBagNetItem`'s equipment arm calls `ItemTable::GetItemTableInfo`
/// (0xA5790) to fill the object from `item_XX.dat` — and then overwrites
/// `ItemInfo+0x150..+0x16d` from the record it was handed (0xA57D8 onward). So
/// a zero-filled tail does not mean "keep the table's values", it means
/// **zero**: no attack, no defence, a wearer group no class accepts, and no
/// options. Sending the row's own bytes back is what makes a granted item
/// identical to one the game made itself.
///
/// `LoadItemTable` parses the record into those same fields from one cursor —
/// `+0x154` from `sb+4`, `+0x155` from `sb+5`, `+0x15c` from `sb+0xc`,
/// `+0x15d` from `sb+0xd` (0xA3EEC / 0xA3F00 / 0xA3F6C / 0xA3F80), then a
/// three-iteration loop taking `[fp+0xf]` to the option **type** and
/// `[fp+0x10]` to its **value** two bytes at a time (0xA3F9C..0xA3FCC). The
/// wire reads the same six option bytes in the same interleaved order
/// (`_NET_ITEM_+0x47..+0x4c`), so the block corresponds one for one:
///
/// ```text
///   record[sb + j]  <->  _NET_ITEM_ + 0x38 + j      j = 0 .. 0x14
/// ```
///
/// and since `+0x15d` is `sb+0xd` and also `record[len-8]`, `sb` is
/// `record[len-21]`.
///
/// **It is not a memcpy.** The five u16 fields are stored little-endian in the
/// file (`StaticUtil::ByteToInt16`, 0x1437EC: `p[i] | p[i+1] << 8`) and
/// big-endian on the wire (`NETWORK::readNet16`, 0xC7C2C: `buf[5] |
/// buf[4] << 8`). Copying the block verbatim byte-swaps every stat.
pub(crate) const EQUIP_BLOCK: usize = 21;

/// Offsets inside [`EQUIP_BLOCK`], as `record[len-21] + n`.
///
/// The five words are `ItemInfo+0x150 / +0x152 / +0x156 / +0x158 / +0x15a`,
/// the group is `+0x155` and the level limit is `+0x15d`.
///
/// **They are not five stats.** `NETWORK::writeNetItem` reads all five with
/// `ldrsh` (0xC88FC, 0xC8914, 0xC895C, 0xC8970, 0xC8988) so all five are
/// halfwords on the wire and all five need the byte swap — that is the only
/// thing this list is for. What they *mean* differs:
///
/// ```text
///   +0x150  sprite file number   HERO::SetLoadImgPath -> /c/sp/imgN/%03d.mgr
///   +0x152  palette number       HERO::SetLoadImgPath -> _nSprPalFile[slot]
///   +0x156  a stat               Formula::getValFunc+0x76c
///   +0x158  a stat               Formula::getValFunc+0x75c
///   +0x15a  RefineItem::ApplyItemDecompose+0x404 reads it back (ldrsb)
/// ```
///
/// Only `+0x156` and `+0x158` are numbers the player is shown. Writing
/// anything but the table row's own value into the other three is what §45
/// removed — see the long note above [`SOCKET_CDF`] for the measurements.
pub(crate) const EQUIP_U16_AT: [usize; 5] = [0, 2, 6, 8, 0x0a];

pub(crate) const EQUIP_GROUP_AT: usize = 5;
pub(crate) const EQUIP_LEVEL_AT: usize = 0x0d;
/// The grade byte, `ItemInfo+0x15c` — 0 노멀, 1 레어, 2 에픽, 3 영웅, 4 전설,
/// 5.. 세트. `MakeSocket` (0xA1224) and `MakeItemOption` (0xA10E8) both branch
/// on it; see [`SOCKET_CDF`].
pub(crate) const EQUIP_GRADE_AT: usize = 0x0c;

/// `ItemRow::group` values `EquipItemInfo::IsEquipPossible` (0xA81AC) accepts.
///
/// 5 passes for every class (0xA82A0 for ROGUE/GUNNER, 0xA8224 for
/// WARRIOR/KNIGHT); 6 passes for WARRIOR and KNIGHT only. **7 passes for
/// nobody** — `item_05` rows 81..89, `item_06` 81..92 and `item_10` row 16 are
/// 7, and `ItemTable::MakeSocket` (0xA123C) also refuses to socket them. A
/// roll must never land on one.
const WEARABLE_GROUPS: [u8; 2] = [5, 6];

/// Walk one item table. Same shape as the catalogue: `[u16 count]` then
/// `[u16 reclen][u8 namelen][name][…]` repeated, and every one of the nineteen
/// tables consumes to its last byte with this rule.
fn parse_item_table(d: &[u8]) -> Vec<ItemRow> {
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
        // The class byte is the record's last-but-one; a record too short to
        // have one is reported as `Unknown` and refused rather than guessed.
        let class = if body.len() >= 2 { body[body.len() - 2] } else { 0xff };
        // Equipment-only, and both are read from the end because the record
        // has two variable-length strings in front of them. A record too
        // short to hold them reads as 0, which `WEARABLE_GROUPS` refuses.
        let at_end = |n: usize| if body.len() >= n { body[body.len() - n] } else { 0 };
        let equip = body
            .len()
            .checked_sub(EQUIP_BLOCK)
            .and_then(|start| body.get(start..))
            .and_then(|b| <[u8; EQUIP_BLOCK]>::try_from(b).ok());
        rows.push(ItemRow {
            name: name.to_vec(),
            class,
            group: at_end(16),
            level: at_end(8),
            equip,
        });
    }
    rows
}

/// One explicit "shop row → `_NET_ITEM_`" mapping, from configuration
/// (probe `--grant ID:KIND:INDEX[:COUNT]`).
///
/// The derivation in [`GrantTable::build`] is what normally answers this; an
/// entry here replaces the derived one for that id, for diagnostics and for
/// the day a row wants something the data does not say.
#[derive(Clone, Debug)]
pub struct ShopGrant {
    /// The catalogue row id — what category 6 subcmd 2/5 carry on the wire.
    pub id: i32,
    pub kind: u8,
    pub index: u8,
    /// `None` takes the catalogue row's quantity (엘릭서(20) → 20).
    pub count: Option<u16>,
}

/// One `_NET_ITEM_` the private server should put in a 6/5 answer. Serialised
/// by [`crate::localserver::net_item`].
pub struct GrantRecord {
    pub kind: u8,
    pub index: u8,
    /// Goes in `_NET_ITEM_+0x00`, which `ProcPacket_Shop` passes as the
    /// repeat count of `BagItem::NewBagNetItem`. Never zero: `readNetItem`
    /// treats a non-positive value there as end-of-list.
    pub count: i32,
    pub name: Vec<u8>,
    /// The item row's own `ItemInfo+0x150..+0x164` bytes, for a kind the wire
    /// carries them for (<= 10). `None` above that, where the record ends at
    /// the name. See [`EQUIP_BLOCK`] — without this the guest overwrites the
    /// item's stats, wearer group and options with zeros.
    pub equip: Option<Vec<u8>>,
    /// How many sockets the piece arrives with, for `ItemInfo+0x168` and the
    /// five slot bytes behind it. Zero for everything that is not a relic
    /// draw, which is what the host has always sent.
    ///
    /// Only ever non-zero on a 유물함 draw: see [`SOCKET_CDF`] for why a
    /// wire-granted item needs this at all (nothing in the guest ever rolls
    /// sockets for it) and [`roll_sockets`] for the roll.
    pub sockets: u8,
}

impl GrantRecord {
    /// `ItemInfo+0x155`, the byte the equip gate tests. Zero means "no
    /// equipment block", which is correct for kinds above 10 and fatal below.
    pub fn wearer_group(&self) -> u8 {
        self.equip.as_ref().map_or(0, |e| e[EQUIP_GROUP_AT])
    }
}

#[derive(Clone, Default)]
struct GrantRec {
    kind: u8,
    index: u8,
    count: i32,
    name: Vec<u8>,
    /// Non-zero makes this a random box: that many records, each with one
    /// index drawn from `index..=roll_hi` and a count of 1. Zero is the
    /// ordinary single record.
    draws: u16,
    roll_hi: u8,
    /// The exact `(kind, index, group)` triples a box may draw, when a band is
    /// not enough to say it. Empty for every box but the 유물함: those roll a
    /// *slot* as well as a row, and each row carries its own wearer group, so
    /// the pool is built once at boot with the unwearable rows already gone.
    /// See [`RELIC_BANDS`].
    ///
    /// Armour entries (kinds 5..8) are eligible for everyone; weapon and
    /// shield entries (kinds 0..3, 9) only for the matching class — the filter
    /// is [`class_may_draw`], applied per draw because the class is per
    /// session, not per table.
    pool: Vec<(u8, u8, [u8; EQUIP_BLOCK])>,
    /// How many pooled rows carry the `+랜덤 옵션` name suffix, for the boot
    /// log — the number a device test should expect to see rolled stats on
    /// items so named. Diagnostic only.
    /// Pieces whose grade reserves an option slot the table left empty — see
    /// [`reserved_option_rows`].
    reserved_options: usize,
    /// The base level limits the pool spans, for the boot log. **The wire
    /// sends zero, not this** — see [`crate::localserver::net_item_granted`]
    /// for why a zero base makes `GetLevelLimit()` come out as 1, which is
    /// what the 유물함 advertise ("제한 레벨이 낮아진 아이템").
    levels: (u8, u8),
    /// The row's equipment block, for a single-record grant. See
    /// [`EQUIP_BLOCK`].
    equip: Option<[u8; EQUIP_BLOCK]>,
}

/// The grant table one run answers purchases from. Built once at `Vm::new`
/// from the catalogue in [`crate::Config::vfs`], with
/// [`crate::Config::shop_grants`] overriding individual rows.
#[derive(Default)]
pub struct GrantTable {
    by_id: HashMap<i32, GrantRec>,
    /// Rows the catalogue named that the host refuses to grant, with the
    /// reason. A refused purchase completes and puts nothing in the bag —
    /// which is the deployed server's behaviour, and better than a token no
    /// screen accepts.
    pub rejected: Vec<(i32, &'static str)>,
    /// Where the next random-box draw starts. One cursor for the whole table
    /// is enough: it only has to *vary*, and stepping it deterministically
    /// keeps a probe replay reproducible, which `rand` would not.
    cursor: std::sync::atomic::AtomicUsize,
    /// `/c/csv/option.dat`, so a relic piece's reserved option slot can be
    /// rolled the way the guest's own factory would. Empty when the archive
    /// has no such file, and then [`fill_options`] does nothing — the piece
    /// arrives exactly as before.
    options: Vec<OptionRow>,
}

impl GrantTable {
    /// Derive the table from the game's own data.
    ///
    /// The base rule is `kind = 18`, `index = ` the catalogue's `a` field,
    /// `count = ` the catalogue's quantity, and it is not inferred from the
    /// shape of the numbers: `ProcPacket_Shop` (0xCD38C) hard-codes
    /// `kind == 18 && 44 <= index <= 48` for the 공허 weapons and remaps them
    /// to `item_00..03.dat` rows 81..85, and `item_18.dat` rows 44..48 are
    /// exactly 공허 1급무기..5급무기 while `item_00..03` rows 81..85 are
    /// 1급공허대검/단검/권총/장창..5급 — those two literals only line up if
    /// `index` is the `item_18` row ordinal. Joining the whole catalogue on
    /// `a` then matches all 30 rows by name.
    ///
    /// Three rules sit on top of it, all from `item_18.dat`'s usage class
    /// (see [`UsageClass`]) and all measured:
    ///
    /// * **class 1 (random box) is substituted or refused.** 오브원석 grants
    ///   five plain orbs and 하이퍼오브 one hyper orb, from `item_12.dat`;
    ///   the four 유물함 are refused, because no target table is derivable
    ///   for them.
    /// * **class 0 and anything unrecognised are refused.** `KeyBagMenu`
    ///   gives those no menu either.
    /// * **공허의 목걸이 is remapped to `item_10.dat` row 17.** It is class 2,
    ///   so it does get a 버리기 menu — but as kind 18 index 43 it can never
    ///   be *worn*: the guest's remap covers 44..48 only. Row 17 of the
    ///   accessory table is the same item by name and the only 공허 accessory
    ///   there. This one is an inference from the tables, not something
    ///   observed on the wire; the five weapons next to it are the precedent.
    /// * **엘릭서 is remapped to `item_11.dat` row 8.** As kind 18 it reaches
    ///   no quickslot and, measured in the disassembly, does nothing at all
    ///   when used. See [`BATTLE_USE_KIND`] for the whole argument.
    pub fn build(
        overrides: &[ShopGrant],
        vfs: &[u8],
        ext: Option<(&[u8], &[u8])>,
    ) -> Self {
        // The extended tables win when they exist: they are the same bytes the
        // guest will read from the file shadow, so the grant table and the
        // guest cannot disagree about which rows exist.
        let (ext_item18, ext_catalog) = match ext {
            Some((i, c)) => (Some(i), Some(c)),
            None => (None, None),
        };
        let catalog = ext_catalog
            .or_else(|| vfs_entry(vfs, CATALOG_PATH))
            .map(parse_catalog)
            .unwrap_or_default();
        let cash_rows = ext_item18
            .or_else(|| vfs_entry(vfs, "c/csv/item_18.dat"))
            .map(parse_item_table)
            .unwrap_or_default();
        let orb_rows = vfs_entry(vfs, "c/csv/item_12.dat")
            .map(parse_item_table)
            .unwrap_or_default();
        let accessory_rows = vfs_entry(vfs, "c/csv/item_10.dat")
            .map(parse_item_table)
            .unwrap_or_default();
        let battle_rows = vfs_entry(vfs, "c/csv/item_11.dat")
            .map(parse_item_table)
            .unwrap_or_default();
        // Through `vfs_entry`, so an `hl5_rates.txt` shadow of option.dat is
        // honoured for free.
        let options = vfs_entry(vfs, "c/csv/option.dat")
            .and_then(parse_option_table)
            .unwrap_or_default();
        let armour: Vec<(u8, Vec<ItemRow>)> = ARMOUR_KINDS
            .iter()
            .filter_map(|&k| {
                vfs_entry(vfs, &format!("c/csv/item_{k:02}.dat"))
                    .map(|d| (k, parse_item_table(d)))
            })
            .collect();
        // The class-gated tables. Absent ones just shrink the pool — a relic
        // box in an archive without them is §39's armour-only box, which is
        // also what keeps the pre-§40 unit fixtures meaning what they meant.
        let class_gated: Vec<(u8, Vec<ItemRow>)> = WEAPON_KINDS
            .iter()
            .chain(std::iter::once(&SHIELD_KIND))
            .filter_map(|&k| {
                vfs_entry(vfs, &format!("c/csv/item_{k:02}.dat"))
                    .map(|d| (k, parse_item_table(d)))
            })
            .collect();

        let mut by_id: HashMap<i32, GrantRec> = HashMap::new();
        let mut rejected = Vec::new();
        for row in &catalog {
            let id = i32::from(row.id);
            // `GetItemTableInfo` multiplies `index` by the row stride with no
            // bounds check (`mla r0, r3, r0, r2`), so an out-of-range index
            // does not fail — it fabricates an item out of whatever memory
            // follows the table. The host checks what the guest does not, and
            // now against the table's real length rather than a constant.
            let Some(listing) = cash_rows.get(usize::from(row.a)) else {
                rejected.push((id, "index past item_18.dat"));
                continue;
            };
            let count = i32::from(row.count.max(1));
            let rec = match UsageClass::from_byte(listing.class) {
                UsageClass::Service => {
                    rejected.push((id, "account service, not a bag item"));
                    continue;
                }
                UsageClass::Unknown => {
                    rejected.push((id, "unknown usage class"));
                    continue;
                }
                UsageClass::RandomBox => match row.a {
                    // 작은오브원석 / 오브원석 — "랜덤 오브를 1개 / 5개 얻습니다".
                    // The quantity is in the Korean description only; the
                    // catalogue's own `count` field is 1 for both, so these two
                    // numbers are cited rather than read.
                    5 | 6 => {
                        let draws = if row.a == 5 { 1 } else { 5 };
                        Self::roll(ORB_KIND, ORB_PLAIN, draws, &orb_rows, &row.name)
                    }
                    // 하이퍼오브 — "하이퍼 오브를 1개 랜덤으로 얻습니다".
                    // Which of the three hyper bands (1~3 / 2~4 / 3~5) it drew
                    // is not recoverable from the client; the lowest is the one
                    // a 3,000원 listing plausibly sold, and it is the band the
                    // 오브결합 screen accepts first.
                    7 => Self::roll(ORB_KIND, ORB_HYPER, 1, &orb_rows, &row.name),
                    // 유물함 x4 — one armour piece, tier by price. The
                    // description says only "랜덤 아이템을 획득합니다"; what
                    // makes it equipment is tips.dat tip 24. See
                    // [`RELIC_BANDS`].
                    a if RELIC_BANDS.iter().any(|(row, _)| *row == a) => {
                        let band = RELIC_BANDS
                            .iter()
                            .find(|(row, _)| *row == a)
                            .map(|(_, b)| *b)
                            .expect("matched just above");
                        match Self::roll_relic(band, &armour, &class_gated, &row.name) {
                            Some(rec) => rec,
                            None => {
                                rejected.push((id, "armour tables are short of the relic band"));
                                continue;
                            }
                        }
                    }
                    _ => {
                        // A class-1 listing this build has never seen.
                        rejected.push((id, "random box with no derivable target"));
                        continue;
                    }
                },
                UsageClass::Consumed | UsageClass::Usable => {
                    // The two listings whose deliverable lives in another
                    // table. Both are the same shape — "this row of
                    // `item_18.dat` is a *name*, the item is over there" — so
                    // they share one code path, which is what gives the
                    // elixir the same range check the necklace was missing.
                    let remap: Option<(u8, u8, &[ItemRow])> = match row.a {
                        VOID_NECKLACE_CASH_ROW => {
                            Some((ACCESSORY_KIND, VOID_NECKLACE_ROW, &accessory_rows))
                        }
                        ELIXIR_CASH_ROW => Some((BATTLE_USE_KIND, ELIXIR_ROW, &battle_rows)),
                        _ => None,
                    };
                    match remap {
                        Some((kind, index, rows)) => {
                            // Same reason the kind-18 lookup above is checked:
                            // `GetItemTableInfo` multiplies the index by the
                            // row stride with no bounds test, so a target row
                            // this archive does not have would be fabricated
                            // out of the memory after the table rather than
                            // failing. Refusing is the honest answer, and the
                            // name is taken from the target table so the
                            // record says what the player actually receives.
                            let Some(target) = rows.get(usize::from(index)) else {
                                rejected.push((id, "remap target row is not in this archive"));
                                continue;
                            };
                            GrantRec {
                                kind,
                                index,
                                count,
                                name: target.name.clone(),
                                // Equipment carries the target row's own
                                // block. Sending zeros — which is what every
                                // grant did until now — hands the player an
                                // item with no stats that no class can wear,
                                // so 공허의 목걸이 has been arriving broken
                                // since that remap landed.
                                equip: if kind <= 10 { target.equip } else { None },
                                ..Default::default()
                            }
                        }
                        None => GrantRec {
                            kind: CASH_ITEM_KIND,
                            index: row.a,
                            count,
                            name: row.name.clone(),
                            ..Default::default()
                        },
                    }
                }
            };
            by_id.insert(id, rec);
        }
        for g in overrides {
            let row = catalog.iter().find(|r| i32::from(r.id) == g.id);
            by_id.insert(
                g.id,
                GrantRec {
                    kind: g.kind,
                    index: g.index,
                    count: i32::from(
                        g.count.unwrap_or_else(|| row.map_or(1, |r| u16::from(r.count.max(1)))),
                    ),
                    name: row.map(|r| r.name.clone()).unwrap_or_default(),
                    // An override names a kind and index and nothing else, so
                    // there is no row to copy a block from. A diagnostic switch
                    // that hands over an unwearable item would be useless, so
                    // it gets the minimum that works: the wearer group every
                    // class accepts, and nothing else.
                    equip: (g.kind <= 10).then(|| {
                        let mut b = [0u8; EQUIP_BLOCK];
                        b[EQUIP_GROUP_AT] = ANY_CLASS;
                        b
                    }),
                    ..Default::default()
                },
            );
            // An override says "grant exactly this", so it also lifts a
            // refusal — that is the point of the flag.
            rejected.retain(|(id, _)| *id != g.id);
        }
        Self { by_id, rejected, cursor: Default::default(), options }
    }

    /// A random box, if its target rows exist in the archive.
    fn roll(
        kind: u8,
        (lo, hi): (u8, u8),
        draws: u16,
        rows: &[ItemRow],
        fallback_name: &[u8],
    ) -> GrantRec {
        let name = rows.get(usize::from(lo)).map_or_else(|| fallback_name.to_vec(), |r| r.name.clone());
        GrantRec {
            kind,
            index: lo,
            count: 1,
            name,
            draws,
            roll_hi: hi,
            // The orb boxes are kind 12, above the equipment range, so this
            // never reaches the wire for them.
            equip: None,
            ..Default::default()
        }
    }

    /// A 유물함: one piece of equipment, slot and row both drawn.
    ///
    /// The four armour tables are **row-aligned** — index `i` names the same
    /// set in `item_05` (투구), `item_06` (갑옷), `item_07` (장갑) and
    /// `item_08` (장화) — so a band is a tier and the slot is a separate draw.
    /// Verified across every index this uses; the smallest of the four tables
    /// has 81 rows, which is why no band goes past 80.
    ///
    /// Refuses rather than rolls if any of the four tables is short of the
    /// band, for the same reason every other lookup here is checked:
    /// `GetItemTableInfo` (0xA27D8) multiplies the index by the row stride with
    /// no bounds test and would fabricate an item out of the memory after the
    /// table.
    ///
    /// # Weapons and the shield are picked by *level*, not by row
    ///
    /// The row bands are an armour fact: the auction price list that proved
    /// row order = tier (§39.5) named armour sets, and nothing equivalent
    /// orders the weapon tables. But every equipment row carries its own
    /// level byte (`record[len-8]`, reproduced against fourteen
    /// independently known values in §39.3), and the parse is one cursor
    /// shared by all nineteen tables, so the byte sits at the same place in
    /// a weapon row. A band's weapon pool is therefore every row of
    /// `item_00..03`/`09` whose level falls inside the span the band's
    /// *wearable armour* spans, rows past [`RELIC_ROW_MAX`] excluded.
    ///
    /// The span rule and the [`RELIC_ROW_MAX`] fence are **decisions**
    /// (ANALYSIS.md §40.4), same standing as §39.5's band edges; which rows
    /// they actually pick from the shipped archive is printed by
    /// [`GrantTable::summary`] at boot, and checking that log against the
    /// game is a §40.6 device item.
    fn roll_relic(
        (lo, hi): (u8, u8),
        tables: &[(u8, Vec<ItemRow>)],
        class_gated: &[(u8, Vec<ItemRow>)],
        fallback_name: &[u8],
    ) -> Option<GrantRec> {
        if tables.len() != ARMOUR_KINDS.len()
            || tables.iter().any(|(_, rows)| rows.len() <= usize::from(hi))
        {
            return None;
        }
        // Every wearable piece in the band, slot by slot. Each keeps its own
        // group rather than being forced to one: a 워리어 세트 piece is group
        // 6 in the shipped data and stays warrior/knight-only, which is the
        // game's own rule and not something to paper over. Group 7 rows would
        // be wearable by nobody — none is inside these bands today, and the
        // filter is what keeps that true if a band ever moves.
        let mut pool: Vec<(u8, u8, [u8; EQUIP_BLOCK])> = tables
            .iter()
            .flat_map(|(kind, rows)| {
                (lo..=hi).filter_map(move |i| {
                    let r = rows.get(usize::from(i))?;
                    let block = r.equip?;
                    WEARABLE_GROUPS.contains(&r.group).then_some((*kind, i, block))
                })
            })
            .collect();
        if pool.is_empty() {
            return None;
        }
        // The class-gated pieces, by the armour band's level span — computed
        // over the *pool*, not the band's raw rows, so an unwearable row a
        // band ever drifts onto cannot stretch the span (the same discipline
        // as the group filter above). Doubles as the boot-log `levels`. The
        // gated rows' group byte must equal the table's class (0..3 for
        // weapons, 3 for the shield) — §39.3's census says it always does,
        // and a row where it does not is a row that census never saw.
        let (band_lo, band_hi) = pool
            .iter()
            .map(|(.., b)| b[EQUIP_LEVEL_AT])
            .fold((u8::MAX, 0u8), |(lo, hi), l| (lo.min(l), hi.max(l)));
        let gated: Vec<(u8, u8, [u8; EQUIP_BLOCK])> = class_gated
            .iter()
            .flat_map(|(kind, rows)| {
                let class = if *kind == SHIELD_KIND { KNIGHT_CLASS } else { *kind };
                rows.iter().enumerate().filter_map(move |(i, r)| {
                    let i = u8::try_from(i).ok().filter(|&i| i <= RELIC_ROW_MAX)?;
                    let block = r.equip?;
                    ((band_lo..=band_hi).contains(&r.level) && r.group == class)
                        .then_some((*kind, i, block))
                })
            })
            .collect();
        // `gated` is a subset of the class-gated pool and `pool` the armour one;
        // both are counted because a relic draw can land in either.
        let reserved = reserved_option_rows(&pool) + reserved_option_rows(&gated);
        pool.extend(gated);
        // The name is cosmetic here, and not by accident: the guest never
        // reads `_NET_ITEM_+0x06` on this path — the object's name comes from
        // `ItemTable::GetItemTableInfo`, called at 0xA5790 with the record's
        // own kind and index — so whatever goes here is replaced by the real
        // row's name before the player sees it.
        let name = tables[0]
            .1
            .get(usize::from(lo))
            .map_or_else(|| fallback_name.to_vec(), |r| r.name.clone());
        Some(GrantRec {
            kind: pool[0].0,
            index: pool[0].1,
            count: 1,
            name,
            draws: 1,
            roll_hi: hi,
            equip: Some(pool[0].2),
            pool,
            reserved_options: reserved,
            levels: (band_lo, band_hi),
            ..Default::default()
        })
    }

    /// What a paid purchase of `id` grants. An id with no mapping yields
    /// nothing, which is the deployed server's answer.
    ///
    /// `hero_class` is what [`hero_class`] read when this purchase's session
    /// connected — `None` when there was nothing to ask (no HERO loaded, or
    /// the reader declined). It gates only the relic pools' weapon and shield
    /// entries; every other grant ignores it.
    pub fn records(&self, id: i32, hero_class: Option<u8>) -> Vec<GrantRecord> {
        let Some(r) = self.by_id.get(&id) else { return Vec::new() };
        if r.draws == 0 {
            return vec![GrantRecord {
                kind: r.kind,
                index: r.index,
                count: r.count,
                name: r.name.clone(),
                equip: r.equip.map(|e| e.to_vec()),
                // Not a draw, so not a relic piece: unchanged from §39.
                sockets: 0,
            }];
        }
        // A box: one record per draw, each a single item. Five records rather
        // than one record with a count of five, because `NewBagNetItem`
        // repeats the *same* item `n` times and "랜덤 오브를 5개" means five
        // rolls.
        let span = usize::from(r.roll_hi.saturating_sub(r.index)) + 1;
        (0..r.draws)
            .map(|_| {
                let n = self.cursor.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // A pool says exactly which pieces are on the table, so it
                // wins; without one this is the band walk the orb boxes have
                // always used, unchanged.
                if !r.pool.is_empty() {
                    // Weapons and the shield leave the table when the class
                    // does not match; armour never does, so the slice is
                    // never empty (the build refuses a band with no armour).
                    let eligible: Vec<&(u8, u8, [u8; EQUIP_BLOCK])> = r
                        .pool
                        .iter()
                        .filter(|(k, ..)| class_may_draw(*k, hero_class))
                        .collect();
                    let (kind, index, block) = *eligible[n % eligible.len()];
                    // The row's own bytes go out unmodified except for the
                    // option slots below and the level limit `localserver`
                    // zeroes. `+0x150` is the sprite file number and `+0x152`
                    // its palette (see the note above [`EQUIP_U16_AT`]): a
                    // piece whose appearance words are not the table's own is a
                    // piece the guest cannot draw.
                    let mut block = block;
                    // The reserved option slot, rolled host-side. This has to
                    // happen while `block[EQUIP_LEVEL_AT]` still holds the
                    // table's real level — the wire byte is zeroed later, and
                    // the guest's own roll scales the value by it. See
                    // [`fill_options`].
                    fill_options(&mut block, &self.options, n as u64);
                    return GrantRecord {
                        kind,
                        index,
                        count: 1,
                        name: r.name.clone(),
                        equip: Some(block.to_vec()),
                        // The guest never rolls sockets for a wire-granted
                        // item, so nothing else would. See `SOCKET_CDF`.
                        sockets: roll_sockets(
                            kind,
                            block[EQUIP_GROUP_AT],
                            block[EQUIP_GRADE_AT],
                            n as u64,
                        ),
                    };
                }
                GrantRecord {
                    kind: r.kind,
                    index: r.index.saturating_add((n % span) as u8),
                    count: 1,
                    name: r.name.clone(),
                    equip: r.equip.map(|e| e.to_vec()),
                    // Orb / consumable boxes: no equipment, no sockets.
                    sockets: 0,
                }
            })
            .collect()
    }

    /// One line per catalogue row the table will actually grant, for the boot
    /// log: `id -> kind/index xN` plus the band and slot set for a box.
    ///
    /// The refusals were already reported and what *is* granted was not, which
    /// is how a granted accessory nobody could wear went unnoticed. Sorted by
    /// id so two runs are comparable.
    pub fn summary(&self) -> Vec<String> {
        let mut ids: Vec<&i32> = self.by_id.keys().collect();
        ids.sort();
        ids.into_iter()
            .map(|id| {
                let r = &self.by_id[id];
                let what = if r.draws == 0 {
                    format!("kind {}/{} x{}", r.kind, r.index, r.count)
                } else if r.pool.is_empty() {
                    format!("kind {} rows {}..={} x{} draw(s)", r.kind, r.index, r.roll_hi, r.draws)
                } else {
                    let kinds: std::collections::BTreeSet<u8> =
                        r.pool.iter().map(|(k, ..)| *k).collect();
                    let groups: std::collections::BTreeSet<u8> =
                        r.pool.iter().map(|(.., b)| b[EQUIP_GROUP_AT]).collect();
                    let gated =
                        r.pool.iter().filter(|(k, ..)| !ARMOUR_KINDS.contains(k)).count();
                    format!(
                        "kinds {:?} rows {}..={}, {} piece(s) ({} class-gated, {} with a reserved option slot, now rolled), \
                         groups {:?}, item level {}..={} (sent as 1, appearance words verbatim)",
                        kinds,
                        r.index,
                        r.roll_hi,
                        r.pool.len(),
                        gated,
                        r.reserved_options,
                        groups,
                        r.levels.0,
                        r.levels.1,
                    )
                };
                // A pool already printed the groups it spans; only a single
                // record needs its own.
                let group = match r.equip {
                    Some(e) if r.pool.is_empty() => format!(", group {}", e[EQUIP_GROUP_AT]),
                    _ => String::new(),
                };
                format!("row {id} -> {what}{group}")
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }
}

/// Whether a pooled piece may go to this session's hero.
///
/// Armour (and anything else the pool might one day hold) passes for
/// everyone — `IsEquipPossible` sends kinds 5..8 and 10 to the common test,
/// which the [`WEARABLE_GROUPS`] filter already satisfied at build time.
/// Weapons and the shield go to 0xA8248, an equality test against the hero's
/// own class, so without a known class they never leave the pool: granting
/// one on a guess would repeat the broken-necklace shape of §39.2 — an item
/// in the bag that its buyer can never wear.
fn class_may_draw(kind: u8, hero_class: Option<u8>) -> bool {
    match kind {
        k if WEAPON_KINDS.contains(&k) => hero_class == Some(k),
        k if k == SHIELD_KIND => hero_class == Some(KNIGHT_CLASS),
        _ => true,
    }
}

/// splitmix64 — the draw cursor is a counter, and neighbouring counters must
/// not give neighbouring rolls. Chosen for being a constant's worth of code;
/// anything that decorrelates would do.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// One row of `/c/csv/option.dat` — the random-option table.
///
/// 25 rows of `[u16 reclen=8][u8 kind][u8 value][u16 cdf0][u16 cdf1][u16 cdf2]`,
/// loaded to `ItemTable+0x74` by `ItemTable::LoadItemOptionTable` (0xa0ba4).
/// The three `cdf`s are one cumulative distribution per band and each ends at
/// exactly 1000.
#[derive(Clone, Copy)]
struct OptionRow {
    kind: u8,
    value: u8,
    cdf: [u16; 3],
}

/// Parse `option.dat`. `None` if the walk does not land exactly on the end,
/// which is the same self-check every other table parse here uses.
fn parse_option_table(d: &[u8]) -> Option<Vec<OptionRow>> {
    let count = usize::from(u16::from_le_bytes([*d.first()?, *d.get(1)?]));
    let mut rows = Vec::with_capacity(count);
    let mut pos = 2usize;
    for _ in 0..count {
        let reclen = usize::from(u16::from_le_bytes([*d.get(pos)?, *d.get(pos + 1)?]));
        let body = d.get(pos + 2..pos + 2 + reclen)?;
        if reclen < 8 {
            return None;
        }
        rows.push(OptionRow {
            kind: body[0],
            value: body[1],
            cdf: [
                u16::from_le_bytes([body[2], body[3]]),
                u16::from_le_bytes([body[4], body[5]]),
                u16::from_le_bytes([body[6], body[7]]),
            ],
        });
        pos += 2 + reclen;
    }
    if pos != d.len() {
        return None;
    }
    Some(rows)
}

/// Which option slots a grade fills, transcribed from `ItemTable::MakeItemOption`
/// (0xa10e8).
///
/// ```text
///   0xa10f8  cmp r3,#0xa / bhi ->return      ; equipment only (kind <= 10)
///   0xa1100  ldrb r3,[r1,#0x15c]             ; the grade
///   0xa110c  cmp r3,#1 / beq -> slot 0
///   0xa1114  cmp r3,#2 / beq -> slot 0 then 1
///   0xa111c  cmp r3,#3 / movne r2,#0 / moveq r2,#1
///   0xa1128  cmp r3,#4 / movle r3,r2 / orrgt r3,r2,#1
///   0xa1134  cmp r3,#0 / bne -> slot 2 ; else return
/// ```
///
/// So grade 0 and 4 get nothing, 1 gets slot 0, 2 gets slots 0 and 1, and 3 and
/// anything >= 5 get **slot 2**. The relic bands hand out grades 0..3 and the
/// set grades, so in practice this is the `[2]` arm; the others are transcribed
/// anyway because they cost nothing and a band move would need them.
fn option_slots(grade: u8) -> &'static [u8] {
    match grade {
        1 => &[0],
        2 => &[0, 1],
        3 => &[2],
        0 | 4 => &[],
        _ => &[2],
    }
}

/// How many option lines `CommonUi::DrawItemDiscription` (0x7b350) draws for
/// a grade — transcribed from 0x7b3c4..0x7b3e8:
///
/// ```text
///   0x7b3c4  ldrbeq r5,[sl,#0x15c]   ; the grade, sign-extended
///   0x7b3d4  cmp r5,#1 / beq -> fp = 1
///   0x7b3dc  cmp r5,#2 / beq -> fp = 2
///   0x7b3e4  movgt fp,#3 / movle fp,#0
/// ```
///
/// Then 0x7b7a4..0x7b83c walks slots `0..fp` and prints `common_text` 287 =
/// `+랜덤옵션` for every slot whose kind byte is `<= 0`. The count is the
/// grade's, **not** the number of filled slots, and it is three for every
/// grade from 영웅 up — 전설 included, even though `MakeItemOption` never
/// touches a 전설.
pub(crate) fn option_lines(grade: u8) -> u8 {
    match grade as i8 {
        g if g <= 0 => 0,
        1 => 1,
        2 => 2,
        _ => 3,
    }
}

/// The slots that would print `+랜덤옵션`: within the grade's
/// [`option_lines`], every one whose kind byte is `<= 0` (the table reserves
/// with `0xFF`, which sign-extends to −1; 0 prints the same).
///
/// This is the invariant every item factory on the host has to leave true
/// as "empty" — whatever door an item comes through (drop, wire grant, extra
/// roll), a slot this names is one the player sees as a promise with nothing
/// behind it. [`option_slots`] is what the guest's own `MakeItemOption` would
/// fill; this is what the *screen* asks for, and the two differ exactly where
/// the shipped rows were never meant to travel this door.
pub(crate) fn empty_option_lines(grade: u8, kinds: [i8; 3]) -> Vec<u8> {
    (0..option_lines(grade))
        .filter(|&s| kinds[usize::from(s)] <= 0)
        .collect()
}

/// Roll one option slot the way `ItemTable::SetItemOption` (0xa0ff8) does.
///
/// # The loop draws a fresh number per row
///
/// ```text
///   0xa1024  (loop head)
///   0xa102c  mov r1,#0x3e4 / add r1,r1,#3    ; 999
///   0xa1038  bl StaticUtil::Rand(0, 999)     ; INSIDE the loop
///   0xa1050  ldrsh r2,[r2,#2]                ; cdf[band] of this row
///   0xa1054  add r6,r6,#8                    ; next row
///   0xa1058  cmp r0,r2 / bge -> loop head
/// ```
///
/// That is not a clean cumulative sample — each row gets its own draw — and
/// reproducing it exactly is the point: an option rolled here has to be
/// indistinguishable from one the guest rolled. Row 24's `cdf` is 1000 in all
/// three bands, so the walk always terminates.
///
/// # The value scales with the level limit, and that is why this lives here
///
/// ```text
///   0xa1064  ldrb r1,[sb,#0x15d]             ; the item's LEVEL LIMIT
///   0xa1080  mul r3,r2,r3                    ; table value x level
///   0xa1094  ... / 100
///   0xa1098  bl Rand(80, 120)
///   0xa10ac  clamp to 1..=100
/// ```
///
/// [`crate::shop`] deliberately sends the level limit as **0** so that
/// `GetLevelLimit()` reads 1 (§39.4, the whole point of the relic box). If the
/// guest rolled the option it would therefore compute `value * 0 / 100` = 0 and
/// clamp to **1** — every option would read `+1%`. That is what ANALYSIS.md
/// §40.2 ran into and recorded as unresolved. Rolling it host-side sidesteps it
/// completely: the table's real level is still in the block at this point
/// (`localserver` zeroes the wire byte later), so the arithmetic gets the number
/// the game intended.
fn roll_option(rows: &[OptionRow], band: usize, level: u8, salt: u64) -> Option<(u8, u8)> {
    let mut draw = 0u64;
    let mut next = |lo: u64, hi: u64| -> u64 {
        draw += 1;
        lo + mix(salt.wrapping_mul(64).wrapping_add(draw)) % (hi - lo + 1)
    };
    let row = rows.iter().find(|r| next(0, 999) < u64::from(r.cdf[band]))?;
    // `mul` then a signed /100, then Rand(80,120), then /100 again.
    let scaled = u64::from(row.value) * u64::from(level) / 100;
    let jittered = scaled * next(80, 120) / 100;
    Some((row.kind, jittered.clamp(1, 100) as u8))
}

/// Fill the option slots the guest would have filled, in place, on the
/// little-endian 21-byte block.
///
/// The block stores the three pairs **interleaved** at `0x0f..0x14` —
/// `block[0x0f + 2*s]` is slot `s`'s kind and `block[0x10 + 2*s]` its value —
/// which `ItemTable::LoadItemTable`'s three-iteration loop (0xa3f98) and
/// `BagItem::NewBagNetItem` (0xa5850..0xa587c) both de-interleave into the
/// *split* arrays on `ItemInfo` (kinds at `+0x15f..+0x161`, values at
/// `+0x162..+0x164`). Writing the split layout here instead of the interleaved
/// one is the single easiest way to get this wrong.
///
/// # Why the shipped item shows `+랜덤옵션` at all
///
/// `CommonUi::DrawItemDiscription` draws as many option lines as the *grade*
/// asks for, not as many as are filled (0x7b3c4 `ldrbeq r5,[sl,#0x15c]`), and
/// per line it tests the slot's kind byte sign-extended: `> 0` names a real
/// option, `<= 0` prints `common_text` 287 = `+랜덤옵션` (0x7b83c `bgt`, and
/// the same predicate at 0x7bf4c for the second layout). The shipped tables
/// reserve the slot with `0xFF`, and the guest's own factory
/// (`ItemTable::NewItemEx` -> `MakeItemOption`) always fills it — so the string
/// essentially never appears in the original game. A wire-granted item is the
/// only thing that can show it, because `BagItem::NewBagNetItem` calls neither
/// `MakeItemOption` nor `MakeSocket`.
///
/// So the text was never "leftover": it honestly said "a random option goes
/// here", and nothing was putting one there.
fn fill_options(block: &mut [u8; EQUIP_BLOCK], rows: &[OptionRow], salt: u64) {
    if rows.is_empty() {
        return;
    }
    let grade = block[EQUIP_GRADE_AT];
    let level = block[EQUIP_LEVEL_AT];
    let roll = |block: &mut [u8; EQUIP_BLOCK], slot: u8| {
        let at = OPTION_PAIRS_AT + 2 * usize::from(slot);
        if at + 1 >= EQUIP_BLOCK {
            return;
        }
        // The band is the slot for grade 3+, and for grades 1/2 the guest uses
        // the slot index as the band too — `SetItemOption`'s `r8` is the slot
        // scaled to the halfword pair, so band == slot throughout.
        let band = usize::from(slot).min(2);
        if let Some((kind, value)) = roll_option(rows, band, level, salt + u64::from(slot)) {
            block[at] = kind;
            block[at + 1] = value;
        }
    };
    // First what the guest's own factory would roll (a re-roll of a slot the
    // row already names, exactly as `MakeItemOption` re-rolls it) ...
    for &slot in option_slots(grade) {
        roll(block, slot);
    }
    // ... then whatever the *screen* would still print `+랜덤옵션` for. The
    // shipped rows never need this — every 영웅/세트 row names slots 0 and 1
    // and every 전설 row names all three — but the rule is the display's,
    // and a row that breaks it must not become a promise with nothing behind it.
    for slot in empty_option_lines(grade, option_kinds(block)) {
        roll(block, slot);
    }
}

/// The three option kind bytes of a 21-byte block, sign-extended the way the
/// guest reads them (`ldrsb`).
pub(crate) fn option_kinds(block: &[u8; EQUIP_BLOCK]) -> [i8; 3] {
    [
        block[OPTION_PAIRS_AT] as i8,
        block[OPTION_PAIRS_AT + 2] as i8,
        block[OPTION_PAIRS_AT + 4] as i8,
    ]
}

/// Offset of the first (kind, value) option pair inside the 21-byte equipment
/// block. Three pairs follow, interleaved.
pub(crate) const OPTION_PAIRS_AT: usize = 0x0f;

/// Ask the guest which class is playing: `HERO::GetPlayerClass` (0x88914,
/// reads `[this+0x22c]` — §38.1) on the HERO the global block points to
/// (`gv+0x1474`, the same field `ProcPacket_Inventory`'s deposit arm reads —
/// [`crate::warehouse::GV_HERO`]).
///
/// Called from the `connect` HLE arm, so the guest is on this thread's own
/// call stack; re-entering it with [`Vm::call`] is the qsort precedent
/// (`hle.rs`), and the callee is four instructions with no `bl` in them.
/// `None` — no gv, no HERO yet (the boot login socket opens before any save
/// is loaded), or a value outside 0..=3 — makes the relic pools armour-only,
/// which is §39's grant and always safe.
pub(crate) fn hero_class(vm: &mut Vm) -> Option<u8> {
    let gv = vm.symbol(crate::warehouse::GV)?;
    let get_class = vm.symbol(crate::input::GET_PLAYER_CLASS)?;
    let hero = vm.mem.read_u32(gv + crate::warehouse::GV_HERO);
    if hero == 0 {
        return None;
    }
    let class = vm.call(get_class, &[hero]).ok()?;
    u8::try_from(class).ok().filter(|c| WEAPON_KINDS.contains(c))
}

/// 공허의 목걸이's row in `item_18.dat`, the one class-2 listing whose target
/// is a different table. See [`GrantTable::build`].
const VOID_NECKLACE_CASH_ROW: u8 = 43;

/// 엘릭서's row in `item_18.dat` — catalogue id 4, 500원, quantity 20, the one
/// class-3 listing that is granted out of another table. See [`BATTLE_USE_KIND`].
const ELIXIR_CASH_ROW: u8 = 34;

/// Per-`Vm` state for the feature: the grant table every
/// [`crate::localserver::Session`] shares, and the price hook's one
/// guest-side allocation.
pub struct ShopHost {
    pub grants: Arc<GrantTable>,
    /// Guest address of the EUC-KR `"0원"` string the price hook draws;
    /// 0 until the hook first fires (the allocation is guest heap, so it is
    /// made lazily and exactly once).
    zero_str: u32,
    /// The price hook logs its geometry inputs once per run, not 754 times a
    /// replay.
    logged_metrics: bool,
    /// Guest addresses of the two assembled modal messages, built on first
    /// use. Cached because one of the two sites is a *draw* and fires every
    /// frame its screen is up — see [`message`].
    messages: std::collections::BTreeMap<Which, u32>,
}

impl ShopHost {
    /// The grant table exists only when the shop is free.
    ///
    /// `Config::local_server` on its own is meant to be the *deployed*
    /// private server moved in-process, and that server grants nothing
    /// (`hl5_server/server.py`, "No item granting yet") — so deriving a table
    /// for it would make the in-process copy quietly generous where the real
    /// one is not, and the fixture A/B would stop meaning "same as the Python
    /// server". Granting is what `Config::free_shop` adds.
    ///
    /// `ext` carries the *extended* `item_18.dat` and catalogue bytes when
    /// [`crate::cashitems`] has added rows. It has to be threaded in here rather
    /// than read back from the file shadow, because this runs before the shadow
    /// is written and because [`GrantTable::build`] refuses any catalogue row
    /// whose `a` is past the row count it found — a new item would otherwise
    /// appear in the shop and be refused at grant time, taking the money and
    /// handing over nothing.
    pub fn new(
        free_shop: bool,
        grants: &[ShopGrant],
        vfs: &[u8],
        ext: Option<(&[u8], &[u8])>,
    ) -> Self {
        let table =
            if free_shop { GrantTable::build(grants, vfs, ext) } else { GrantTable::default() };
        Self {
            grants: Arc::new(table),
            zero_str: 0,
            logged_metrics: false,
            messages: Default::default(),
        }
    }
}

// ---- the price display hook ---------------------------------------------------
//
// The cash shop draws a row's price from **three** call sites across **two**
// `BFont::DrawText` overloads, because it has two layouts:
//
// * the detail box, `StateCashShop::DrawAutoCashShopInfo` (0xDC1CC) ->
//   `DrawText(iiiiPci)` (0x4414C) at 0xDC464, return 0xDC468;
// * the compact overlay, drawn by `StateCashShop::Paint` (0xDC614) itself in
//   two layout variants -> `DrawText(iiiPcii)` (0x445E8) at 0xDCDC0 and
//   0xDD240, returns 0xDCDC4 and 0xDD244.
//
// Each site is the fan-in of the three discount branches: `GetDiscount == 1`
// takes `price2` (+0x12), `== 2` takes `price3` (+0x14), otherwise `price`
// (+0x10), and all three branch to the same `GetCommonText(0x109)` +
// `MC_knlSprintk("%d%s")` + one `DrawText`. So one return address per site
// catches every price on that layout and nothing else.
//
// **The return-address test is mandatory, not tidy.** `Paint` calls the same
// 0x445E8 overload at 0xDCC2C and 0xDD078 to draw the item *name*; hooking
// the function without the `lr` test would strike through "엘릭서(20)".
//
// Only the detail box was known when this was first written, and it is the
// one the project's own replay never reaches — `tools_re/scripts/ingame.txt`
// exercises the compact overlay, where the hook fired zero times. Both are
// covered now.
//
// Transcribing a measurer is the chronosSword trap — two width computations
// that disagree by a pixel per glyph. HL5 avoids it twice over: the host
// calls the guest's measurer instead of copying it, and this binary has only
// **one** measurer to begin with (`BFont::GetWidth(char*,int,int,int)` at
// 0x436A0), which `BFont::DrawText` itself calls for its own alignment.

/// `BFont::DrawText(int fb, int x, int y, int wrapWidth, char* str, int align)`
/// — 0x4414C, the detail box's overload. Observed call site
/// `(this, fb, r5+0xe, r6+0x18, 0x82, str, 0)`; **argument 4 is the wrap
/// width in pixels (0x82 = 130), not a colour** — the colour comes from a
/// separate `BFont::SetColor(0x00FFFFFF)` at 0xDC424.
const DRAW_TEXT_WRAP: &str = "_ZN5BFont8DrawTextEiiiiPci";
/// `BFont::DrawText(int fb, int x, int y, char* str, int align, int colour)`
/// — 0x445E8, the compact overlay's overload. Here the string is argument 4
/// and the colour is argument 6 (0x00FFFFFF at both sites).
const DRAW_TEXT_PLAIN: &str = "_ZN5BFont8DrawTextEiiiPcii";
/// `BFont::GetWidth(char* s, int end)` — 0x43774, a forwarder onto
/// `GetWidth(s, end, 0, 0)` at 0x436A0. `end` is an exclusive **byte** index,
/// not a character count: the measuring cursor advances by 1 on the ASCII
/// path and by 2 on the EUC-KR lead-byte path and is compared against it.
/// `strlen` is therefore the value to pass — and it is literally what the
/// guest does on this very string at 0xDD128/0xDD13C.
const GET_WIDTH: &str = "_ZN5BFont8GetWidthEPci";
/// `BFont::HanFontHeight()` — 0x438E8, a constant accessor.
const HAN_FONT_HEIGHT: &str = "_ZN5BFont13HanFontHeightEv";

/// The detail-box price call site's owner, and the return address inside it
/// (0xDC468 - 0xDC1CC). Keyed to the symbol so a different binary fails the
/// symbol lookup instead of hooking a random address.
const DETAIL_SITE: &str = "_ZN13StateCashShop20DrawAutoCashShopInfoEiiP14_CASHSHOP_INFO";
const DETAIL_RET: u32 = 0xdc468 - 0xdc1cc;

/// The compact overlay's owner, and its two price return addresses
/// (0xDCDC4 and 0xDD244, relative to 0xDC614).
const COMPACT_SITE: &str = "_ZN13StateCashShop5PaintEv";
const COMPACT_RETS: [u32; 2] = [0xdcdc4 - 0xdc614, 0xdd244 - 0xdc614];

/// `"0원"` in EUC-KR, NUL-terminated — the wire and the fonts are EUC-KR and
/// never touch `String` (CLAUDE.md).
const ZERO_WON: [u8; 4] = [0x30, 0xbf, 0xf8, 0x00];

/// Strikethrough colour, RGB565. Red, so the crossed-out price reads as
/// "was"; purely cosmetic and trivially changeable.
const STRIKE_565: u16 = 0xf800;

/// Pixels between the struck-through price and the `0원` that follows.
const GAP_PX: i32 = 3;

/// Where the strikethrough sits, measured down from `DrawText`'s `y`.
///
/// This is the one number in the feature that disassembly cannot settle:
/// `HanFontHeight` is a bare `ldrsb [this+0x45]` and says nothing about where
/// the ink sits, and reading it out of the 1,252-byte
/// `DrawText(iiiiPciiii)` glyph loop is a long way round for one pixel. So it
/// was measured off a rendered price instead — `out/shot/c3-shop.png`, the
/// compact overlay, `--free-shop` replay of `tools_re/scripts/ingame.txt`:
///
/// ```text
/// free-shop: price "500원" at x=150 y=108, width=28, HanFontHeight=11
/// ink rows 108..118 inclusive  ->  exactly HanFontHeight rows, starting at y
/// ```
///
/// So `y` **is** the glyph top here and the ink is exactly one cell tall; the
/// middle is `y + h/2` = 113. An earlier version added 1 for the
/// `0x43E34: add ip, fp, #1` in the draw loop and landed a pixel low — that
/// instruction is not this offset. The hook logs these inputs once per run so
/// the number stays checkable against a screenshot.
fn strike_y(y: i32, han_font_height: i32) -> i32 {
    y + han_font_height / 2
}

/// Install the free-shop behaviour hooks. Called from [`Vm::boot`] when
/// [`crate::Config::free_shop`] is set; a missing symbol is a hard error,
/// exactly like `patch.rs`, so a wrong binary is loud rather than subtly
/// other.
pub fn install_hooks(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for name in
        [GET_WIDTH, HAN_FONT_HEIGHT, DETAIL_SITE, COMPACT_SITE, CONFIRM_SITE, REVIVE_SITE,
         GET_COMMON_TEXT, P_TEXT_MGR]
    {
        if vm.symbol(name).is_none() {
            return Err(format!("free-shop hook needs `{name}`, not in this binary"));
        }
    }
    // Only the detail box's overload is installed here. The compact
    // overlay's `DrawText(iiiPcii)` is shared with the widened-canvas top bar
    // (`hud`) and with the revive nag, so `Vm::boot` owns that address and
    // hands it to each of them through a dispatcher.
    let addr = vm
        .symbol(DRAW_TEXT_WRAP)
        .ok_or_else(|| format!("free-shop hook target `{DRAW_TEXT_WRAP}` is not in this binary"))?;
    vm.install_hook(DRAW_TEXT_WRAP, addr, detail_price_hook)?;
    let addr = vm
        .symbol(SET_POPUP_STATE)
        .ok_or_else(|| format!("free-shop hook target `{SET_POPUP_STATE}` is not in this binary"))?;
    vm.install_hook(SET_POPUP_STATE, addr, confirm_popup_hook)?;
    Ok(vec![DRAW_TEXT_WRAP, DRAW_TEXT_PLAIN, SET_POPUP_STATE])
}

/// The detail box (`DrawAutoCashShopInfo`). Arguments
/// `(this, fb, x, y, wrapWidth, str, align)`.
fn detail_price_hook(vm: &mut Vm) -> Result<bool, String> {
    if !vm.cfg.free_shop {
        return Ok(false);
    }
    let Some(site) = vm.symbol(DETAIL_SITE) else { return Ok(false) };
    if !fired_at(vm, &[site + DETAIL_RET]) {
        return Ok(false);
    }
    let a: Vec<u32> = (0..7).map(|i| vm.arg(i)).collect();
    let (x, y) = (a[2] as i32, a[3] as i32);
    strike_and_append(vm, DRAW_TEXT_WRAP, &a, 5, x, y)
}

/// The compact overlay (`StateCashShop::Paint`). Arguments
/// `(this, fb, x, y, str, align, colour)` — note the string moves to
/// argument 4 and there is no wrap width.
pub(crate) fn compact_price_hook(vm: &mut Vm) -> Result<bool, String> {
    // **Not tidiness.** The dispatcher that offers this address is installed
    // whenever the canvas is widened *or* the shop is free (`Vm::boot`), and
    // the canvas is widened on every phone. Without this test an installation
    // pointed at a real server by `hl5_server.txt` — where prices are real and
    // a purchase is a real charge — would draw a strikethrough and `0원` over
    // them. `detail_price_hook` carries the same test for symmetry, though its
    // own hook is only installed under `free_shop`.
    if !vm.cfg.free_shop {
        return Ok(false);
    }
    let Some(site) = vm.symbol(COMPACT_SITE) else { return Ok(false) };
    let want: Vec<u32> = COMPACT_RETS.iter().map(|o| site + o).collect();
    if !fired_at(vm, &want) {
        return Ok(false);
    }
    let a: Vec<u32> = (0..7).map(|i| vm.arg(i)).collect();
    let (x, y) = (a[2] as i32, a[3] as i32);
    strike_and_append(vm, DRAW_TEXT_PLAIN, &a, 4, x, y)
}

/// Whether this entry is one of the price call sites.
///
/// Recursion is impossible by the same argument chronosSword used: the test
/// is on the return address, and [`Vm::call`] sets `lr = RETURN_MAGIC`, so
/// the `DrawText` calls *made by these hooks* fail it immediately.
fn fired_at(vm: &Vm, want: &[u32]) -> bool {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    lr != RETURN_MAGIC && want.contains(&lr)
}

/// Draw the price the shipped code would have drawn, strike it through, and
/// put `0원` after it.
///
/// `args` is the entry register/stack image of whichever overload fired and
/// is replayed verbatim, so this never has to know what the arguments it is
/// not using *mean*; `str_at` is the one index it does need.
fn strike_and_append(
    vm: &mut Vm,
    overload: &str,
    args: &[u32],
    str_at: usize,
    x: i32,
    y: i32,
) -> Result<bool, String> {
    let draw = vm.symbol(overload).ok_or("free-shop: DrawText vanished")?;
    let get_width = vm.symbol(GET_WIDTH).ok_or("free-shop: GetWidth vanished")?;
    let han_height = vm.symbol(HAN_FONT_HEIGHT).ok_or("free-shop: HanFontHeight vanished")?;
    let this = args[0];
    let fbp = args[1];
    let strp = args[str_at];

    // 1. The original price, drawn by the original code.
    let ret = vm.call(draw, args)?;

    // 2. Its width, measured by the guest's own measurer so the line cannot
    //    disagree with the glyphs about where they sit.
    let len = vm.mem.cstr_bytes(strp).len() as u32;
    let width = vm.call(get_width, &[this, strp, len])? as i32;
    let height = vm.call(han_height, &[this])? as i32;

    // One line, the first time a price is struck: everything the geometry
    // below depends on. The strike y is the only part of this feature that
    // disassembly cannot settle (`HanFontHeight` is a bare field read and
    // says nothing about where the ink sits inside the cell), so the numbers
    // that produced a given screenshot have to be recoverable from the log.
    if !vm.shop.logged_metrics {
        vm.shop.logged_metrics = true;
        let s = String::from_utf8_lossy(&vm.mem.cstr_bytes(strp)).to_string();
        vm.log.push(format!(
            "free-shop: price {s:?} at x={x} y={y}, width={width}, HanFontHeight={height} \
             -> strike y={}",
            strike_y(y, height)
        ));
    }

    // 3. The strikethrough: one row of pixels across the vertical middle of
    //    the line, straight into the frame buffer. The descriptor layout is
    //    the one every ported blitter reads (patch.rs `mod fb`) — the guest
    //    names those fields itself in `FrameBufferInfo::FrameBufferInfo`
    //    (0x51FE0), and `Graphic::GetFB` hands DrawText the same pointer
    //    `MC_grpFillRect` takes. Only the 16-bit format this display is
    //    initialised with is handled; any other leaves the price merely
    //    un-struck rather than corrupt.
    //
    //    The span is `x .. x + width` and must not be widened leftward: the
    //    `●` before the price is a sprite (`Graphic::DrawImage` frame 9), not
    //    part of the string.
    //
    //    The glyphs are clipped twice: by the buffer, and by the live
    //    `MC_GrpContext` at `(*_lcdG)+0xc` that `MX_fntDrawString` installs
    //    from. Today no cash-shop path calls `Graphic::SetClip` and
    //    `ChangeFrameBuff` resets the clip to the full screen, so the two
    //    agree — but `m_mask` is sticky, so a mailbox or quest screen visited
    //    earlier would leave a narrow clip in place and the line would run
    //    past glyphs that were not drawn. Honouring it costs four reads.
    if width > 0 && vm.mem.read_u32(fbp + crate::patch::fb::BPP) == 0x10 {
        let fb_w = vm.mem.read_u32(fbp + crate::patch::fb::WIDTH) as i32;
        let fb_h = vm.mem.read_u32(fbp + crate::patch::fb::HEIGHT) as i32;
        let stride = vm.mem.read_u32(fbp + crate::patch::fb::STRIDE) as i32;
        let base = vm.mem.read_u32(fbp + crate::patch::fb::BASE);

        // The same half-open box `patch.rs`'s ported blitters intersect with.
        let (mut cx0, mut cy0, mut cx1, mut cy1) = (0, 0, fb_w, fb_h);
        if let Some(gp) = vm.symbol("_lcdG") {
            let graphic = vm.mem.read_u32(gp);
            if graphic != 0 {
                let c = graphic + 0x0c;
                if vm.mem.read_u32(c + crate::patch::ctx::MASK) & 1 != 0 {
                    cx0 = cx0.max(vm.mem.read_u32(c + crate::patch::ctx::CLIP_X0) as i32);
                    cy0 = cy0.max(vm.mem.read_u32(c + crate::patch::ctx::CLIP_Y0) as i32);
                    cx1 = cx1.min(vm.mem.read_u32(c + crate::patch::ctx::CLIP_X1) as i32);
                    cy1 = cy1.min(vm.mem.read_u32(c + crate::patch::ctx::CLIP_Y1) as i32);
                }
            }
        }

        let ly = strike_y(y, height);
        if ly >= cy0 && ly < cy1 {
            let row = base.wrapping_add((ly * stride) as u32);
            for lx in x.max(cx0)..(x + width).min(cx1) {
                vm.mem.write_u16(row.wrapping_add((lx * 2) as u32), STRIKE_565);
            }
        }
    }

    // 4. `0원`, in the same font and colour, after the struck-out price: the
    //    same arguments with the string and the x replaced.
    if vm.shop.zero_str == 0 {
        let p = vm.heap.alloc(ZERO_WON.len() as u32).unwrap_or(0);
        if p != 0 {
            vm.mem.write_bytes(p, &ZERO_WON);
        }
        vm.shop.zero_str = p;
    }
    if vm.shop.zero_str != 0 {
        let mut zero: Vec<u32> = args.to_vec();
        zero[2] = (x + width + GAP_PX) as u32;
        zero[str_at] = vm.shop.zero_str;
        vm.call(draw, &zero)?;
    }

    // r0 = what the original call returned; `Vm::call` restored the entry
    // registers, so it has to be put back explicitly.
    vm.cpu.reg_set(Mode::User, 0, ret);
    Ok(true)
}

// ---- the "500원 부과" modals ---------------------------------------------------
//
// Striking the price line through is only half of what a free shop has to say.
// Two places still tell the player, in the game's own words, that real money is
// about to be taken:
//
//   * the **purchase confirmation**, 예/아니오 —
//     `실제 현금 500원의 / 추가 정보이용료가 부과됩니다. / (통화료 부과)`;
//   * the **death-screen revive nag** — the same sentence with the price as a
//     literal 100, drawn straight onto the screen rather than into a popup.
//
// Both assemble their message the same way: five or six `GetCommonText` rows
// through one `MC_knlSprintk`, then one call. So both are one argument to
// substitute, and neither needs a new Korean byte anywhere in this repo — the
// replacement is built from rows the game already ships:
//
// ```text
//   [0x5b]  구입하시겠습니까?
//   [0x104] 부활의 부적을 구매하시겠습니까?
//   [0x109] 원                  [0x10a] 실제 현금
//   [0x14a] (정보료/통화료 무료)
// ```
//
// # Why substitute the sentence rather than strike the number
//
// The confirmation's body is `memcpy`d into `StatePopup+0x24` (1,024 bytes,
// which `PopupInit` memsets) and then drawn *word-wrapped* inside the 14 KB
// `StatePopup::Paint`. Striking the price there means reproducing which line
// the number landed on and where — for one number. Replacing the sentence is
// one pointer (ANALYSIS.md §31.1).
//
// # Why each hook keys on a return address
//
// `StateCashShop::SetPopupState` has ten call sites and one of them
// (`KeyPressed+0x428`) is the *bag is full* error; hooking the function alone
// would rewrite that too. And `BFont::DrawText(iiiPcii)` is already shared by
// the compact price line and the widened-canvas top bar. So all three go
// through the same discipline the price hooks use: test `lr`, decline anything
// that is not this exact site.

/// `StateCashShop::SetPopupState(signed char, char*, int, int, int)` — 0xDBFC8.
/// `(this, kind, msg, …)`: `kind = 2` is the 예/아니오 pair, and the message is
/// argument 2.
const SET_POPUP_STATE: &str = "_ZN13StateCashShop13SetPopupStateEaPciii";
/// The confirmation's owner and the return address of its one call —
/// `StateCashShop::KeyPressed+0x27C` (0xDDBA0). Observed live:
/// `StateCashShop::SetPopupState(…, 0x2, …) <- StateCashShop::KeyPressed+0x27c`.
const CONFIRM_SITE: &str = "_ZN13StateCashShop10KeyPressedEi";
const CONFIRM_RET: u32 = 0xddba0 - 0xdd924;

/// The revive nag's owner and the return address of its single `DrawText`
/// (`StateGame::DrawOverText+0xF8`, 0xDEDC4). Its price is the immediate
/// `mov ip, #0x64` at +0x7C — 100, not a catalogue field — which is why
/// nothing that looks at prices can reach this site.
const REVIVE_SITE: &str = "_ZN9StateGame12DrawOverTextEv";
const REVIVE_RET: u32 = 0xdedc4 - 0xdeccc;

/// `TextMgr::GetCommonText(int)` — 0x143170.
///
/// A **member** function: `this` is r0 and the index is **r1**. Surveying this
/// table on r0 is the standard way to spend a day and learn nothing
/// (CLAUDE.md, "이 바이너리를 읽을 때 매번 걸리는 것").
const GET_COMMON_TEXT: &str = "_ZN7TextMgr13GetCommonTextEi";
/// The global holding the one `TextMgr` — 0x16FCD8, a pointer.
const P_TEXT_MGR: &str = "_pTextMgr";

/// `;` is this guest's line break inside a message (CLAUDE.md).
const NL: u8 = b';';

/// `StatePopup+0x24` is 1,024 bytes and `PopupInit` memsets it; anything at or
/// under that is safe for either site.
const MSG_MAX: usize = 1024;

/// One span of an assembled message. `Text` is an index into the game's own
/// common-text table, so the only bytes this module contributes are ASCII.
enum Piece {
    Text(u32),
    Ascii(&'static [u8]),
    Line,
}

/// The replacement confirmation, as `GetCommonText` indices and literal ASCII.
///
/// ```text
///   실제 현금 0원
///   (정보료/통화료 무료)
///   구입하시겠습니까?
/// ```
const CONFIRM_MSG: &[Piece] = &[
    Piece::Text(0x10a),
    Piece::Ascii(b" 0"),
    Piece::Text(0x109),
    Piece::Line,
    Piece::Text(0x14a),
    Piece::Line,
    Piece::Text(0x5b),
];

/// The replacement revive nag. Keeps its own first line: on that screen it is
/// the only thing naming what is being bought.
///
/// ```text
///   부활의 부적을 구매하시겠습니까?
///   실제 현금 0원
///   (정보료/통화료 무료)
/// ```
const REVIVE_MSG: &[Piece] = &[
    Piece::Text(0x104),
    Piece::Line,
    Piece::Text(0x10a),
    Piece::Ascii(b" 0"),
    Piece::Text(0x109),
    Piece::Line,
    Piece::Text(0x14a),
];

/// Which of the two messages. Also the log label.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Which {
    Confirm,
    Revive,
}

/// `TextMgr::GetCommonText(idx)`, through the guest.
///
/// Reading the table host-side out of `c/csv/common_text.dat` would work and
/// was rejected: it is a second implementation of a walk the guest already
/// does, and this project has a name for that mistake (patch.rs, "transcribing
/// a measurer is the chronosSword trap"). Four guest calls once per run is not
/// worth a second parser.
fn common_text(vm: &mut Vm, idx: u32) -> Result<u32, String> {
    let f = vm.symbol(GET_COMMON_TEXT).ok_or("free-shop: GetCommonText vanished")?;
    let p = vm.symbol(P_TEXT_MGR).ok_or("free-shop: _pTextMgr vanished")?;
    let this = vm.mem.read_u32(p);
    if this == 0 {
        return Err("free-shop: the common-text table is not loaded yet".into());
    }
    vm.call(f, &[this, idx])
}

/// Assemble one message into the guest heap and return its address.
///
/// Cached: the rows come from a file loaded once and never edited, so the bytes
/// cannot change during a run — which matters for the revive nag, whose site is
/// a *draw* and fires on every frame that screen is up.
fn message(vm: &mut Vm, which: Which) -> Result<u32, String> {
    if let Some(p) = vm.shop.messages.get(&which) {
        return Ok(*p);
    }
    let pieces = match which {
        Which::Confirm => CONFIRM_MSG,
        Which::Revive => REVIVE_MSG,
    };
    let mut out: Vec<u8> = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Line => out.push(NL),
            Piece::Ascii(b) => out.extend_from_slice(b),
            Piece::Text(i) => {
                let p = common_text(vm, *i)?;
                if p == 0 {
                    return Err(format!("free-shop: common text {i:#x} is null"));
                }
                out.extend_from_slice(&vm.mem.cstr_bytes(p));
            }
        }
    }
    out.push(0);
    // Three rows come to nothing like the limit, but a table that grew would
    // corrupt a popup silently, so it is checked rather than assumed.
    if out.len() > MSG_MAX {
        return Err(format!(
            "free-shop: assembled message is {} bytes, over the {MSG_MAX}-byte popup body",
            out.len()
        ));
    }
    let p = vm.heap.alloc(out.len() as u32).ok_or("free-shop: no room for the message")?;
    vm.mem.write_bytes(p, &out);
    vm.shop.messages.insert(which, p);
    vm.log.push(format!(
        "free-shop: {which:?} text replaced, {} bytes at {p:#010x}",
        out.len() - 1
    ));
    Ok(p)
}

/// The 예/아니오 confirmation. `(this, kind, msg, a, b, c)`; only `msg` moves,
/// so `kind = 2` — and with it the two buttons — is preserved by construction.
fn confirm_popup_hook(vm: &mut Vm) -> Result<bool, String> {
    if !vm.cfg.free_shop {
        return Ok(false);
    }
    let Some(site) = vm.symbol(CONFIRM_SITE) else { return Ok(false) };
    if !fired_at(vm, &[site + CONFIRM_RET]) {
        return Ok(false);
    }
    let mut a: Vec<u32> = (0..6).map(|i| vm.arg(i)).collect();
    a[2] = message(vm, Which::Confirm)?;
    let f = vm.symbol(SET_POPUP_STATE).ok_or("free-shop: SetPopupState vanished")?;
    let ret = vm.call(f, &a)?;
    vm.cpu.reg_set(Mode::User, 0, ret);
    Ok(true)
}

/// The death-screen revive nag, on the shared `DrawText(iiiPcii)` entry.
/// `(this, fb, x, y, str, align, colour)` — the string is argument 4.
pub(crate) fn revive_nag_hook(vm: &mut Vm) -> Result<bool, String> {
    if !vm.cfg.free_shop {
        return Ok(false);
    }
    let Some(site) = vm.symbol(REVIVE_SITE) else { return Ok(false) };
    if !fired_at(vm, &[site + REVIVE_RET]) {
        return Ok(false);
    }
    let mut a: Vec<u32> = (0..7).map(|i| vm.arg(i)).collect();
    a[4] = message(vm, Which::Revive)?;
    let f = vm.symbol(DRAW_TEXT_PLAIN).ok_or("free-shop: DrawText vanished")?;
    let ret = vm.call(f, &a)?;
    vm.cpu.reg_set(Mode::User, 0, ret);
    Ok(true)
}

// ---- tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {

    /// `option.dat`'s real shape, so the parse is exercised on the same bytes
    /// the game ships: `[u16 count]` + count x `[u16 reclen=8][kind][value][3 x u16]`.
    fn option_file(rows: &[(u8, u8, [u16; 3])]) -> Vec<u8> {
        let mut v = (rows.len() as u16).to_le_bytes().to_vec();
        for (k, val, cdf) in rows {
            v.extend_from_slice(&8u16.to_le_bytes());
            v.push(*k);
            v.push(*val);
            for c in cdf {
                v.extend_from_slice(&c.to_le_bytes());
            }
        }
        v
    }

    #[test]
    fn option_table_parses_and_rejects_a_short_walk() {
        let f = option_file(&[(0x24, 10, [828, 700, 600]), (0x6d, 15, [1000, 1000, 1000])]);
        let rows = parse_option_table(&f).expect("parses");
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].kind, rows[0].value), (0x24, 10));
        assert_eq!(rows[1].cdf, [1000, 1000, 1000]);
        let mut slop = f.clone();
        slop.push(0);
        assert!(parse_option_table(&slop).is_none(), "trailing slop must be refused");
    }

    #[test]
    fn the_grade_to_slot_map_matches_makeitemoption() {
        // Transcribed from 0xa10f8..0xa1170. Grades 0 and 4 get nothing, and
        // that is not an oversight in the guest — it is why a 노멀 or 전설
        // piece never shows a rolled option.
        assert_eq!(option_slots(0), &[] as &[u8]);
        assert_eq!(option_slots(1), &[0]);
        assert_eq!(option_slots(2), &[0, 1]);
        assert_eq!(option_slots(3), &[2]);
        assert_eq!(option_slots(4), &[] as &[u8]);
        for g in 5..=18u8 {
            assert_eq!(option_slots(g), &[2], "set grade {g}");
        }
    }

    #[test]
    fn a_grade_three_piece_gets_its_reserved_slot_filled() {
        // 몰라크투구's real tail: two named options and 0xFF reserving the third.
        let mut block: [u8; EQUIP_BLOCK] = [
            0x75, 0x00, 0xc2, 0x00, 0x01, 0x06, 0x49, 0x00, 0x49, 0x00, 0x1f, 0x00, 0x03, 0x62,
            0x00, 0x24, 0x0a, 0x1f, 0x18, 0xff, 0x00,
        ];
        assert_eq!(block[EQUIP_GRADE_AT], 3);
        assert_eq!(block[EQUIP_LEVEL_AT], 98);
        // The reserved slot before: kind 0xFF prints +랜덤옵션.
        assert_eq!(block[OPTION_PAIRS_AT + 4], 0xff);
        let rows = parse_option_table(&option_file(&[
            (0x1f, 20, [678, 460, 450]),
            (0x6d, 15, [1000, 1000, 1000]),
        ]))
        .expect("parses");
        fill_options(&mut block, &rows, 7);
        // Slot 2 now names a real option, so the draw takes the `bgt` arm.
        let kind = block[OPTION_PAIRS_AT + 4];
        assert!(kind == 0x1f || kind == 0x6d, "kind {kind:#x} is not from the table");
        let value = block[OPTION_PAIRS_AT + 5];
        assert!((1..=100).contains(&value), "value {value} outside the guest's clamp");
        // The two table options must survive untouched — they are what the
        // player already sees, and slot 2 is the only reserved one.
        assert_eq!(&block[OPTION_PAIRS_AT..OPTION_PAIRS_AT + 4], &[0x24, 0x0a, 0x1f, 0x18]);
    }

    #[test]
    fn grade_zero_and_a_named_legendary_are_left_completely_alone() {
        // A 노멀 (0) piece draws no option line, and a 전설 (4) row names all
        // three itself; `MakeItemOption` touches neither, and neither do we.
        let rows = parse_option_table(&option_file(&[(0x1f, 20, [1000, 1000, 1000])])).unwrap();
        let mut normal = [0u8; EQUIP_BLOCK];
        normal[EQUIP_LEVEL_AT] = 50;
        for i in 0..6 {
            normal[OPTION_PAIRS_AT + i] = 0xff;
        }
        let before = normal;
        fill_options(&mut normal, &rows, 3);
        assert_eq!(normal, before, "grade 0 must not be touched");

        let mut legendary = [0u8; EQUIP_BLOCK];
        legendary[EQUIP_GRADE_AT] = 4;
        legendary[EQUIP_LEVEL_AT] = 50;
        legendary[OPTION_PAIRS_AT..OPTION_PAIRS_AT + 6].copy_from_slice(&[0x24, 10, 0x1f, 24, 0x20, 30]);
        let before = legendary;
        fill_options(&mut legendary, &rows, 3);
        assert_eq!(legendary, before, "a fully named 전설 must not be touched");
    }

    #[test]
    fn a_legendary_with_a_hole_gets_the_hole_filled_and_nothing_else() {
        // `MakeItemOption` skips grade 4 — but `DrawItemDiscription` still
        // draws three lines for it (0x7b3e4 `movgt fp,#3`), so a 전설 whose
        // table row left a slot at 0xFF would read `+랜덤옵션` there. No
        // shipped row does; the rule is the screen's, so the host keeps it.
        let rows = parse_option_table(&option_file(&[(0x1f, 20, [1000, 1000, 1000])])).unwrap();
        let mut block = [0u8; EQUIP_BLOCK];
        block[EQUIP_GRADE_AT] = 4;
        block[EQUIP_LEVEL_AT] = 50;
        block[OPTION_PAIRS_AT..OPTION_PAIRS_AT + 6].copy_from_slice(&[0x24, 10, 0xff, 0xff, 0x20, 30]);
        fill_options(&mut block, &rows, 3);
        assert_eq!(&block[OPTION_PAIRS_AT..OPTION_PAIRS_AT + 2], &[0x24, 10]);
        assert_eq!(&block[OPTION_PAIRS_AT + 4..OPTION_PAIRS_AT + 6], &[0x20, 30]);
        assert_eq!(block[OPTION_PAIRS_AT + 2], 0x1f, "the hole is filled from the table");
        assert!((1..=100).contains(&block[OPTION_PAIRS_AT + 3]));
        assert!(empty_option_lines(4, option_kinds(&block)).is_empty());
    }

    #[test]
    fn option_lines_follow_the_grade_not_the_fill() {
        assert_eq!(option_lines(0), 0);
        assert_eq!(option_lines(1), 1);
        assert_eq!(option_lines(2), 2);
        assert_eq!(option_lines(3), 3);
        assert_eq!(option_lines(4), 3);
        for g in 5..=18 {
            assert_eq!(option_lines(g), 3, "set grade {g}");
        }
        assert_eq!(option_lines(0xff), 0, "a negative grade draws nothing");
        assert_eq!(empty_option_lines(3, [0x20, 0x21, -1]), [2]);
        assert_eq!(empty_option_lines(3, [0, 0x21, 0]), [0, 2]);
        assert_eq!(empty_option_lines(2, [0x20, 0x21, -1]), Vec::<u8>::new());
    }

    #[test]
    fn the_roll_varies_with_the_salt_and_stays_in_the_clamp() {
        // Two purchases of the same item should not read identically, and no
        // draw may leave the guest's 1..=100 range.
        let rows = parse_option_table(&option_file(&[
            (0x1f, 20, [500, 500, 500]),
            (0x24, 10, [1000, 1000, 1000]),
        ]))
        .unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for salt in 0..40u64 {
            let (kind, value) = roll_option(&rows, 2, 98, salt).expect("row 1 always hits");
            assert!(kind == 0x1f || kind == 0x24);
            assert!((1..=100).contains(&value));
            seen.insert((kind, value));
        }
        assert!(seen.len() > 1, "every salt produced the same option: {seen:?}");
    }

    #[test]
    fn an_empty_option_table_leaves_the_piece_exactly_as_it_was() {
        // No option.dat in the archive must mean "behave as before", not panic
        // and not invent an option.
        let mut block = [0u8; EQUIP_BLOCK];
        block[EQUIP_GRADE_AT] = 3;
        block[OPTION_PAIRS_AT + 4] = 0xff;
        let before = block;
        fill_options(&mut block, &[], 1);
        assert_eq!(block, before);
    }
    use super::*;

    /// One `item_%02d.dat` record in the shipped layout: `[u16 reclen]` then
    /// `[u16 ?][u8 namelen][name][…tail…]`, where the **last-but-one** byte is
    /// the usage class this module reads.
    fn item_row(name: &[u8], class: u8) -> Vec<u8> {
        let mut b = vec![0, 0]; // the two bytes before the name length
        b.push(name.len() as u8);
        b.extend_from_slice(name);
        b.extend_from_slice(b"desc");
        b.push(class);
        b.push(0xff); // the sub-parameter byte; unread here
        b
    }

    /// An **equipment** record. This module reads two bytes out of the tail —
    /// the wearer group at `len - 16` and the level limit at `len - 8` — so
    /// the filler is sized to put them exactly there. See [`ItemRow::group`]
    /// for where those two offsets come from.
    fn equip_row(name: &[u8], group: u8, level: u8) -> Vec<u8> {
        equip_row_stats(name, group, level, [0; 5])
    }

    /// [`equip_row`] with the five stat words filled in, and the whole
    /// 21-byte tail laid out explicitly ([`EQUIP_BLOCK`]'s offsets), so the
    /// stat-roll tests have known values to compare against.
    fn equip_row_stats(name: &[u8], group: u8, level: u8, stats: [u16; 5]) -> Vec<u8> {
        let mut b = vec![0, 0];
        b.push(name.len() as u8);
        b.extend_from_slice(name);
        b.extend_from_slice(b"desc");
        let mut block = [0u8; EQUIP_BLOCK];
        for (i, &at) in EQUIP_U16_AT.iter().enumerate() {
            block[at..at + 2].copy_from_slice(&stats[i].to_le_bytes());
        }
        block[EQUIP_GROUP_AT] = group;
        block[EQUIP_LEVEL_AT] = level;
        b.extend_from_slice(&block);
        b
    }

    fn item_table(rows: &[Vec<u8>]) -> Vec<u8> {
        let mut t = (rows.len() as u16).to_le_bytes().to_vec();
        for r in rows {
            t.extend_from_slice(&(r.len() as u16).to_le_bytes());
            t.extend_from_slice(r);
        }
        t
    }

    fn vfs_of(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut vfs = Vec::new();
        // A decoy entry first, so the lookup proves it walks the list.
        vfs.extend_from_slice(&djb2(b"c/other.dat").to_le_bytes());
        vfs.extend_from_slice(&3u32.to_le_bytes());
        vfs.extend_from_slice(b"xyz");
        for (path, body) in entries {
            vfs.extend_from_slice(&djb2(path.as_bytes()).to_le_bytes());
            vfs.extend_from_slice(&(body.len() as u32).to_le_bytes());
            vfs.extend_from_slice(body);
        }
        vfs
    }

    /// A synthetic archive holding a catalogue in the `LoadCashShopData`
    /// layout **and the two item tables the derivation now reads**, to prove
    /// the parser and the mapping without depending on the licensed payload.
    ///
    /// The `item_18.dat` stand-in mirrors the real table's shape where it
    /// matters: row 34 is a class-3 usable (엘릭서's slot), row 12 a class-2
    /// consumable, row 6 a class-1 random box (오브원석's slot), row 43 the
    /// class-2 necklace, and row 2 a class-0 account service.
    fn fake_vfs() -> Vec<u8> {
        let row = |id: u8, param: u16, name: &[u8], a: u8, qty: u8, prices: [u16; 3]| {
            let mut b = vec![id];
            b.extend_from_slice(&param.to_le_bytes());
            b.push(name.len() as u8);
            b.extend_from_slice(name);
            b.push(a);
            b.push(qty);
            b.push(4); // desclen
            b.extend_from_slice(b"desc");
            for p in prices {
                b.extend_from_slice(&p.to_le_bytes());
            }
            b
        };
        let mut cat = 9u16.to_le_bytes().to_vec();
        for r in [
            row(4, 250, b"\xbf\xa4\xb8\xae\xbb\xe7(20)", 34, 20, [500, 350, 250]),
            row(9, 222, b"item9", 12, 1, [100, 0, 0]),
            row(6, 219, b"orbstone", 6, 1, [1200, 0, 0]),
            row(46, 211, b"necklace", VOID_NECKLACE_CASH_ROW, 1, [500, 0, 0]),
            row(2, 100, b"service", 2, 1, [900, 0, 0]),
            // The four 유물함, with the shipped catalogue's own prices. All
            // four share one icon (223) and one description, which is why the
            // band has to come from somewhere else — see [`RELIC_BANDS`].
            row(18, 223, b"relic-s", RELIC_BANDS[0].0, 1, [1500, 0, 0]),
            row(19, 223, b"relic-m", RELIC_BANDS[1].0, 1, [2000, 0, 0]),
            row(20, 223, b"relic-l", RELIC_BANDS[2].0, 1, [2500, 0, 0]),
            row(21, 223, b"relic-old", RELIC_BANDS[3].0, 1, [3000, 0, 0]),
        ] {
            cat.extend_from_slice(&(r.len() as u16).to_le_bytes());
            cat.extend_from_slice(&r);
        }

        let mut cash: Vec<Vec<u8>> = (0..49).map(|_| item_row(b"filler", 2)).collect();
        cash[2] = item_row(b"service", 0);
        cash[6] = item_row(b"orbstone", 1);
        cash[12] = item_row(b"consumable", 2);
        cash[34] = item_row(b"usable", 3);
        for (r, _) in RELIC_BANDS {
            cash[usize::from(r)] = item_row(b"relicbox", 1);
        }
        cash[usize::from(VOID_NECKLACE_CASH_ROW)] = item_row(b"necklace", 2);

        let mut orbs: Vec<Vec<u8>> = (0..53).map(|_| item_row(b"orb", 0)).collect();
        orbs[usize::from(ORB_PLAIN.0)] = item_row(b"orb1", 0);

        let mut acc: Vec<Vec<u8>> = (0..18).map(|_| item_row(b"acc", 0)).collect();
        // 공허의 목걸이 is group 5 / level 1 in the shipped `item_10.dat`,
        // which is what makes it wearable at all.
        acc[usize::from(VOID_NECKLACE_ROW)] = equip_row(b"realnecklace", 5, 1);

        // `item_11.dat` really is this short — 16 rows in the shipped archive,
        // which is why the elixir's remap target is worth range-checking.
        let mut battle: Vec<Vec<u8>> = (0..16).map(|_| item_row(b"potion", 0)).collect();
        battle[usize::from(ELIXIR_ROW)] = item_row(b"realelixir", 0);

        // The four armour tables, at the shipped archive's smallest row count
        // (item_07 and item_08 have 81), so a band that runs off the end is a
        // test and not a surprise.
        // Groups alternate 5 / 6 the way the shipped tables mix light and heavy
        // sets, and level rises with the row so a band is a tier. Stats mix
        // zero and non-zero so the roll tests can see both survive correctly.
        let armour = |k: u8| -> (String, Vec<u8>) {
            let rows: Vec<Vec<u8>> = (0..81u16)
                .map(|i| {
                    let group = if i % 2 == 0 { 5 } else { 6 };
                    equip_row_stats(
                        format!("armour{k}-{i}").as_bytes(),
                        group,
                        20 + i as u8,
                        [30 + i, 0, 7, 2, 0],
                    )
                })
                .collect();
            (format!("c/csv/item_{k:02}.dat"), item_table(&rows))
        };
        // The four weapon tables at the shipped shape: 86 rows, the wearer
        // group equal to the table's class in every one, levels rising like
        // the armour's — except rows 81..85, the 공허 stand-ins, whose level
        // is put *inside* the cheapest band's span on purpose: only the
        // `RELIC_ROW_MAX` fence keeps them out, and a test leans on that.
        let weapon = |k: u8| -> (String, Vec<u8>) {
            let rows: Vec<Vec<u8>> = (0..86u16)
                .map(|i| {
                    let level = if i > 80 { 45 } else { 20 + i as u8 };
                    equip_row_stats(
                        format!("weapon{k}-{i}").as_bytes(),
                        k,
                        level,
                        [50 + i, 12, 0, 0, 3],
                    )
                })
                .collect();
            (format!("c/csv/item_{k:02}.dat"), item_table(&rows))
        };
        // The shield table: 81 rows, group 3 (knight) throughout.
        let shield: Vec<Vec<u8>> = (0..81u16)
            .map(|i| {
                equip_row_stats(
                    format!("shield-{i}").as_bytes(),
                    KNIGHT_CLASS,
                    20 + i as u8,
                    [0, 40 + i, 0, 0, 0],
                )
            })
            .collect();

        let mut entries: Vec<(&str, Vec<u8>)> = vec![
            (CATALOG_PATH, cat),
            ("c/csv/item_18.dat", item_table(&cash)),
            ("c/csv/item_12.dat", item_table(&orbs)),
            ("c/csv/item_10.dat", item_table(&acc)),
            ("c/csv/item_11.dat", item_table(&battle)),
            ("c/csv/item_09.dat", item_table(&shield)),
        ];
        // Leaked so the `&str` keys can live alongside the literals above; this
        // is a test helper called a handful of times.
        let owned: Vec<(String, Vec<u8>)> = ARMOUR_KINDS
            .iter()
            .map(|&k| armour(k))
            .chain(WEAPON_KINDS.iter().map(|&k| weapon(k)))
            .collect();
        for (path, body) in owned {
            entries.push((Box::leak(path.into_boxed_str()), body));
        }
        vfs_of(&entries)
    }

    #[test]
    fn catalog_parses() {
        let vfs = fake_vfs();
        let rows = parse_catalog(vfs_entry(&vfs, CATALOG_PATH).expect("catalogue entry"));
        assert_eq!(rows.len(), 9);
        assert_eq!((rows[0].id, rows[0].param, rows[0].a, rows[0].count), (4, 250, 34, 20));
        assert_eq!(rows[0].prices, [500, 350, 250]);
    }

    #[test]
    fn item_tables_parse_and_expose_the_usage_class() {
        let vfs = fake_vfs();
        let rows = parse_item_table(vfs_entry(&vfs, "c/csv/item_18.dat").expect("item_18"));
        assert_eq!(rows.len(), 49);
        assert_eq!(UsageClass::from_byte(rows[2].class), UsageClass::Service);
        assert_eq!(UsageClass::from_byte(rows[6].class), UsageClass::RandomBox);
        assert_eq!(UsageClass::from_byte(rows[12].class), UsageClass::Consumed);
        assert_eq!(UsageClass::from_byte(rows[34].class), UsageClass::Usable);
    }

    /// The whole point of the recovered mapping: no configuration needed.
    ///
    /// Row id 9 rather than the elixir, because the elixir is one of the two
    /// listings that are deliberately *not* granted out of `item_18.dat` —
    /// see [`the_elixir_is_remapped_to_the_battle_item_table`].
    #[test]
    fn the_catalogue_alone_yields_a_grant() {
        let vfs = fake_vfs();
        let rows = parse_catalog(vfs_entry(&vfs, CATALOG_PATH).expect("catalogue entry"));
        let t = GrantTable::build(&[], &vfs, None);

        let r = t.records(9, None);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, CASH_ITEM_KIND, "an ordinary cash item is item_18.dat");
        assert_eq!(r[0].index, 12, "index is the catalogue's `a`");
        assert_eq!(r[0].count, 1, "count is the catalogue's quantity");
        assert_eq!(r[0].name, rows[1].name);
    }

    /// 엘릭서 is granted out of `item_11.dat` row 8, not as its own
    /// `item_18.dat` listing.
    ///
    /// As kind 18 the bag can only offer it popup 4 or 5, and 단축키등록 —
    /// the quickslot entry the user asked for — is in popup 3, which only kind
    /// 11 reaches. The listing is also inert when used: the kind-18 use path
    /// sends every row above 13 to 0x10BEE8, which prints
    /// `을(를) 사용했습니다.` and deletes the item without healing. See
    /// [`BATTLE_USE_KIND`].
    #[test]
    fn the_elixir_is_remapped_to_the_battle_item_table() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        let r = t.records(4, None);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].kind, r[0].index), (BATTLE_USE_KIND, ELIXIR_ROW));
        assert_eq!(r[0].count, 20, "the catalogue's quantity survives the remap");
        assert_eq!(r[0].name, b"realelixir".to_vec(), "the name comes from the target table");

        // …and what actually goes out on the wire, in the shape
        // `NETWORK::readNetItem` (0xC85E4) reads it. Kind 11 is above that
        // function's `cmp #0xa; ble`, so this is the *short* record — ten
        // bytes and the name, with no 58-byte equipment tail. A tail here
        // would be read as the start of the next record.
        let wire = crate::localserver::net_item(r[0].count, r[0].kind, r[0].index, &r[0].name);
        assert_eq!(&wire[..4], &20i32.to_be_bytes(), "+0x00 is the repeat count");
        assert_eq!((wire[4], wire[5]), (11, 8), "+0x04 kind, +0x05 index");
        assert_eq!(
            wire.len(),
            10 + r[0].name.len(),
            "kind > 10 ends the record at the name: {wire:02x?}"
        );
    }

    /// The four 유물함 grant one armour piece each, in their own tier band.
    ///
    /// They were refused outright until `/c/csv/tips.dat` tip 24 was read —
    /// "유물함에서는 제한 레벨이 낮아진 아이템을 얻을 수 있습니다" — which says
    /// the contents are equipment. See [`RELIC_BANDS`].
    #[test]
    fn a_relic_box_grants_one_armour_piece_in_its_band() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        for (id, (lo, hi)) in [(18, RELIC_BANDS[0].1), (19, RELIC_BANDS[1].1)]
            .into_iter()
            .chain([(20, RELIC_BANDS[2].1), (21, RELIC_BANDS[3].1)])
        {
            let r = t.records(id, None);
            assert_eq!(r.len(), 1, "id {id}: a relic chest is one item");
            assert!(
                ARMOUR_KINDS.contains(&r[0].kind),
                "id {id}: kind {} is not one of the class-agnostic armour tables",
                r[0].kind
            );
            assert!(
                (lo..=hi).contains(&r[0].index),
                "id {id}: index {} outside {lo}..={hi}",
                r[0].index
            );
            assert_eq!(r[0].count, 1);
            assert!(
                WEARABLE_GROUPS.contains(&r[0].wearer_group()),
                "id {id}: group {} is worn by nobody",
                r[0].wearer_group()
            );
        }
    }

    /// The bands must not all collapse onto one slot or one row: a chest that
    /// always gave the same helm would pass the test above and be wrong.
    #[test]
    fn repeated_relic_purchases_vary_the_slot_and_the_row() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        let (lo, hi) = RELIC_BANDS[0].1;
        let mut kinds = std::collections::BTreeSet::new();
        let mut rows = std::collections::BTreeSet::new();
        for _ in 0..44 {
            let r = t.records(18, None);
            kinds.insert(r[0].kind);
            rows.insert(r[0].index);
        }
        assert_eq!(
            kinds.len(),
            ARMOUR_KINDS.len(),
            "all four slots should come up over 44 draws, saw {kinds:?}"
        );
        assert_eq!(
            rows.len(),
            usize::from(hi - lo) + 1,
            "every row of the band should come up over 44 draws, saw {rows:?}"
        );
    }

    /// A weapon's table is its class (§39.3), and `IsEquipPossible` compares
    /// the wearer byte against the hero's own class — so a box may hand a
    /// weapon only to the class whose table it came from, the shield only to
    /// a knight, and a session with no known class must stay armour-only.
    #[test]
    fn a_relic_box_rolls_weapons_only_for_the_matching_class() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        let kinds_over = |class: Option<u8>| -> std::collections::BTreeSet<u8> {
            (0..200).map(|_| t.records(18, class)[0].kind).collect()
        };

        let gunner = kinds_over(Some(2));
        assert!(gunner.contains(&2), "a gunslinger should see 권총 rows, saw {gunner:?}");
        assert!(
            gunner.iter().all(|k| *k == 2 || ARMOUR_KINDS.contains(k)),
            "a gunslinger must never draw another class's weapon or the shield: {gunner:?}"
        );

        let knight = kinds_over(Some(KNIGHT_CLASS));
        assert!(knight.contains(&3), "a knight should see 장창 rows, saw {knight:?}");
        assert!(knight.contains(&SHIELD_KIND), "a knight should see 방패 rows, saw {knight:?}");

        let unknown = kinds_over(None);
        assert!(
            unknown.iter().all(|k| ARMOUR_KINDS.contains(k)),
            "no class means §39's armour-only box: {unknown:?}"
        );
    }

    /// The weapon pool is picked by the armour band's *level span*, not by
    /// row number — nothing proved the weapon tables are row-aligned with the
    /// armour tiers — and rows past [`RELIC_ROW_MAX`] stay out even when
    /// their level falls in the span, because 81..85 are the 공허 weapons.
    /// `fake_vfs` plants exactly that trap: its void stand-ins carry level
    /// 45, inside the cheapest band's 40..=50.
    #[test]
    fn relic_weapons_stay_inside_the_level_span_and_off_the_void_rows() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        let (lo, hi) = RELIC_BANDS[0].1; // rows 20..=30 -> fake levels 40..=50
        let mut weapon_rows = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let r = t.records(18, Some(0));
            if r[0].kind == 0 {
                weapon_rows.insert(r[0].index);
            }
        }
        assert!(!weapon_rows.is_empty(), "the band has in-span weapon rows to draw");
        for i in &weapon_rows {
            assert!(*i <= RELIC_ROW_MAX, "row {i} is a 공허 slot and must never be drawn");
            assert!(
                (lo..=hi).contains(i),
                "fake weapon levels are 20+row, so in-span rows are exactly {lo}..={hi}, drew {i}"
            );
        }
    }

    /// The bug a player reported as "a 유물함 helmet takes my head off", as a
    /// test: a drawn piece must reach the wire carrying the table row's own
    /// appearance words. `ItemInfo+0x150` is the `%03d` in
    /// `/c/sp/imgN/%03d.mgr` (`HERO::SetLoadImgPath`, 0x8DE58) and every
    /// equipment table's range of it stops exactly at its sprite directory's
    /// last file, so a value one step past the row's is a file the archive does
    /// not have and the guest draws nothing where the helmet should be.
    ///
    /// The three words that are not stats (`+0x150`, `+0x152`, `+0x15a`) and
    /// the two that are (`+0x156`, `+0x158`) are all checked the same way,
    /// because after §45 the host modifies none of them.
    #[test]
    fn relic_draws_carry_the_table_rows_appearance_verbatim() {
        let vfs = fake_vfs();
        let t = GrantTable::build(&[], &vfs, None);
        let tables: Vec<(u8, Vec<ItemRow>)> = ARMOUR_KINDS
            .iter()
            .map(|&k| {
                let d = vfs_entry(&vfs, &format!("c/csv/item_{k:02}.dat")).expect("armour");
                (k, parse_item_table(d))
            })
            .collect();
        let mut saw_a_non_zero_word = false;
        for _ in 0..44 {
            let r = t.records(18, None);
            let (kind, index) = (r[0].kind, r[0].index);
            let row = tables
                .iter()
                .find(|(k, _)| *k == kind)
                .and_then(|(_, rows)| rows.get(usize::from(index)))
                .expect("the drawn row exists");
            let table_block = row.equip.expect("equipment row");
            let wire_block = r[0].equip.as_deref().expect("relic grants carry the block");
            for &at in &EQUIP_U16_AT {
                let v = u16::from_le_bytes([table_block[at], table_block[at + 1]]);
                let w = u16::from_le_bytes([wire_block[at], wire_block[at + 1]]);
                assert_eq!(
                    v, w,
                    "block halfword at {at}: the row says {v}, the wire says {w}. \
                     +0x150 is a sprite file number, not a stat"
                );
                saw_a_non_zero_word |= v != 0;
            }
            // Everything outside the option pairs is the row's, byte for byte;
            // `localserver` zeroes the level limit later, on the wire.
            for (i, (a, b)) in table_block.iter().zip(wire_block).enumerate() {
                if (OPTION_PAIRS_AT..EQUIP_BLOCK).contains(&i) {
                    continue;
                }
                assert_eq!(a, b, "block byte {i} is the table's");
            }
        }
        assert!(saw_a_non_zero_word, "the fake rows carry non-zero words to compare");
    }

    /// The draw and the roll both step off the same deterministic cursor —
    /// the rule the orb boxes set (`rand` would make a probe replay
    /// unreproducible). Two tables built from the same archive must produce
    /// the same purchases in the same order.
    #[test]
    fn relic_draws_are_deterministic_across_identical_tables() {
        let vfs = fake_vfs();
        let (a, b) = (GrantTable::build(&[], &vfs, None), GrantTable::build(&[], &vfs, None));
        for id in [18, 19, 20, 21] {
            for class in [None, Some(0), Some(KNIGHT_CLASS)] {
                let (ra, rb) = (a.records(id, class), b.records(id, class));
                assert_eq!(ra[0].kind, rb[0].kind, "id {id} class {class:?}");
                assert_eq!(ra[0].index, rb[0].index, "id {id} class {class:?}");
                assert_eq!(ra[0].equip, rb[0].equip, "id {id} class {class:?}");
            }
        }
    }

    /// `GetItemTableInfo` (0xA27D8) multiplies the index by the row stride with
    /// no bounds test, so a band that runs past a short table would fabricate
    /// an item out of the bytes after it. All four tables have to cover the
    /// band or the row is refused.
    #[test]
    fn a_relic_band_past_a_short_armour_table_is_refused() {
        let full = fake_vfs();
        let keep = |p: &'static str| (p, vfs_entry(&full, p).expect(p).to_vec());
        // Everything except item_08 (장화), which the top band needs.
        let mut entries = vec![
            keep(CATALOG_PATH),
            keep("c/csv/item_18.dat"),
            keep("c/csv/item_12.dat"),
            keep("c/csv/item_10.dat"),
            keep("c/csv/item_11.dat"),
        ];
        for k in [5u8, 6, 7] {
            let path: &'static str = Box::leak(format!("c/csv/item_{k:02}.dat").into_boxed_str());
            entries.push((path, vfs_entry(&full, path).expect("armour").to_vec()));
        }
        let t = GrantTable::build(&[], &vfs_of(&entries), None);
        for id in [18, 19, 20, 21] {
            assert!(t.records(id, None).is_empty(), "id {id} must not grant a fabricated piece");
            assert!(
                t.rejected.iter().any(|(r, why)| *r == id && why.contains("relic band")),
                "id {id} should say why"
            );
        }
        // …and the rest of the table is unaffected.
        assert!(!t.records(9, None).is_empty());
    }

    /// The wire has to carry the row's stats, not just its wearer group.
    ///
    /// `NewBagNetItem` fills the object from the item table and then overwrites
    /// `ItemInfo+0x150..+0x164` from the record, so a zero-filled tail is a
    /// helm with no defence — the failure this test exists to keep out. The
    /// five stat words are little-endian in the file and big-endian on the
    /// wire, which is the part a memcpy gets wrong.
    #[test]
    fn a_granted_equipment_record_carries_the_rows_stats_byte_swapped() {
        // A row whose block is entirely distinguishable, so a swap in the
        // wrong place shows up.
        let mut body = vec![0u8, 0, 4];
        body.extend_from_slice(b"helm");
        let block: Vec<u8> = (0x40u8..0x40 + EQUIP_BLOCK as u8).collect();
        body.extend_from_slice(&block);
        let rows = parse_item_table(&item_table(&[body]));
        let src = rows[0].equip.expect("a long enough record has a block");
        assert_eq!(&src[..], &block[..], "the block is the record's last 21 bytes");

        let wire = crate::localserver::net_item_granted(1, 5, 0, b"helm", Some(&src), 0);
        // head: [i32 count][u8 kind][u8 index][i32 namelen][name]
        let tail = &wire[4 + 1 + 1 + 4 + 4..];
        assert_eq!(tail.len(), 58, "kind <= 10 is the long record");
        let on_wire = &tail[24..24 + EQUIP_BLOCK];

        for (i, want) in block.iter().enumerate() {
            let expect = if i == EQUIP_LEVEL_AT {
                // The one field deliberately not passed through.
                0
            } else if let Some(&at) = EQUIP_U16_AT.iter().find(|&&a| a == i || a + 1 == i) {
                // Little-endian in the file, big-endian on the wire.
                block[if i == at { at + 1 } else { at }]
            } else {
                *want
            };
            assert_eq!(on_wire[i], expect, "block byte {i}");
        }
        assert_eq!(on_wire[EQUIP_GROUP_AT], block[EQUIP_GROUP_AT], "the group is a plain byte");
    }

    /// Group 7 is worn by nobody — `EquipItemInfo::IsEquipPossible` refuses it
    /// for all four classes, and `ItemTable::MakeSocket` (0xA123C) will not
    /// socket it either. No shipped band contains one; this is what keeps that
    /// true if a band ever moves.
    /// The real requirement, and the one the user reported broken: a granted
    /// piece must be **socket-expandable**, i.e. its wire record must carry at
    /// least one 0xFF slot byte. `BagItem::SetItemFilter(31)` admits an item
    /// only on that byte (0xA180C..0xA1844) — not on the count, not the grade.
    #[test]
    fn every_relic_draw_can_be_socket_expanded() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        for id in 18..=21 {
            for _ in 0..64 {
                let r = t.records(id, Some(0));
                assert_eq!(r.len(), 1, "id {id}: a relic chest is one item");
                let g = &r[0];
                assert!(g.sockets <= SOCKET_SLOTS, "id {id}: {} sockets", g.sockets);
                let equip = g.equip.as_deref().expect("a relic draw is equipment");
                let wire = crate::localserver::net_item_granted(
                    g.count, g.kind, g.index, &g.name, Some(equip), g.sockets,
                );
                let tail = &wire[4 + 1 + 1 + 4 + g.name.len()..];
                let nine = &tail[24 + EQUIP_BLOCK..24 + EQUIP_BLOCK + 9];
                // A piece already at the game's own five-socket limit
                // ("아이템 소켓은 5개 제한입니다", item_18 row 12) has no free
                // slot and is correctly absent from the picker. Every other
                // piece must be in it.
                assert_eq!(
                    nine[4..].contains(&0xff),
                    g.sockets < SOCKET_SLOTS,
                    "id {id}: kind {} grade {} with {} socket(s): slots {:02x?}",
                    g.kind,
                    equip[EQUIP_GRADE_AT],
                    g.sockets,
                    &nine[4..]
                );
            }
        }
    }

    /// The guest refuses to socket a legendary outright (`MakeSocket` 0xA1258),
    /// so a granted one must arrive with none — and must still be expandable,
    /// which the test above covers.
    #[test]
    fn a_legendary_is_granted_with_no_socket() {
        for salt in 0..2_000u64 {
            assert_eq!(roll_sockets(5, 5, 4, salt), 0, "grade 4 must never roll a socket");
        }
    }

    /// `MakeSocket` and `MakeSocketItemCompose` both refuse a kind above 9
    /// (0xA1238 / 0xA1188), and group 7 is refused at 0xA1244.
    #[test]
    fn a_grant_the_guest_would_never_socket_carries_none() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        for id in [46, 4] {
            for g in t.records(id, Some(0)) {
                assert!(g.kind > 9, "id {id}: expected a non-socketable kind");
                assert_eq!(g.sockets, 0, "id {id}: kind {} must carry no socket", g.kind);
            }
        }
        assert_eq!(roll_sockets(10, 5, 0, 7), 0, "an accessory never rolls a socket");
        assert_eq!(roll_sockets(18, 5, 0, 7), 0, "a cash item never rolls a socket");
        assert_eq!(roll_sockets(5, 7, 0, 7), 0, "wearer group 7 never rolls a socket");
    }

    /// The roll must reproduce `MakeSocket`'s own two columns. Checked as a
    /// distribution, because a constant would satisfy a range assertion.
    #[test]
    fn the_socket_roll_follows_make_sockets_curve() {
        let n = 200_000u64;
        for (grade, want) in [
            (0u8, [0.50, 0.20, 0.15, 0.10, 0.04, 0.01]),
            (3u8, [0.70, 0.20, 0.10, 0.0, 0.0, 0.0]),
        ] {
            let mut hist = [0u32; 6];
            for salt in 0..n {
                hist[usize::from(roll_sockets(5, 5, grade, salt))] += 1;
            }
            for (k, w) in want.iter().enumerate() {
                let got = f64::from(hist[k]) / n as f64;
                assert!(
                    (got - w).abs() < 0.01,
                    "grade {grade}, {k} socket(s): {got:.4} is not within 1pp of {w:.2}"
                );
            }
        }
    }

    /// The nine bytes at `_NET_ITEM_+0x4d` land on `ItemInfo+0x165..+0x16D`
    /// (`writeNetItem`, 0xC8A70..0xC8B30) and `NewBagNetItem` stores every one
    /// of them (0xA589C, 0xA58A4..0xA58C4). Pinned byte by byte, including the
    /// 0x00 / 0xFF distinction the socket-expansion picker depends on.
    #[test]
    fn granted_socket_bytes_match_what_make_socket_would_have_left() {
        let mut body = vec![0u8, 0, 4];
        body.extend_from_slice(b"helm");
        body.extend_from_slice(&(0x40u8..0x40 + EQUIP_BLOCK as u8).collect::<Vec<u8>>());
        let rows = parse_item_table(&item_table(&[body]));
        let src = rows[0].equip.expect("a long enough record has a block");

        for n in 0..=SOCKET_SLOTS {
            let wire = crate::localserver::net_item_granted(1, 5, 0, b"helm", Some(&src), n);
            let tail = &wire[4 + 1 + 1 + 4 + 4..];
            assert_eq!(tail.len(), 58, "kind <= 10 is the long record");
            let nine = &tail[24 + EQUIP_BLOCK..24 + EQUIP_BLOCK + 9];

            // +0x165 refine level, +0x166 bonus, +0x167 cracked: a new piece.
            assert_eq!(&nine[..3], &[0, 0, 0], "n={n}: refine state must be clear");
            assert_eq!(nine[3], n, "n={n}: +0x168 is the socket count");
            for i in 0..usize::from(SOCKET_SLOTS) {
                let want = if (i as u8) < n { 0x00 } else { 0xff };
                assert_eq!(
                    nine[4 + i], want,
                    "n={n}: slot {i} must be {want:#04x} (0 = empty socket, 0xff = absent)"
                );
            }
            assert_eq!(&tail[24 + EQUIP_BLOCK + 9..], &[0, 0, 0, 0], "the two trailing u16");
        }
    }

    /// A record with no equipment block is not equipment, and its nine bytes
    /// must stay zero — that is what `net_item` sends and what
    /// `tests/localserver_ab.rs` pins against the Python server.
    #[test]
    fn a_non_equipment_record_keeps_the_deployed_servers_zeros() {
        let wire = crate::localserver::net_item(3, 11, 8, b"elixir");
        // kind 11 > 10, so the record ends at the name: nothing to check but
        // that it stayed short.
        assert_eq!(wire.len(), 4 + 1 + 1 + 4 + 6, "kind > 10 is the short record");
        let wire = crate::localserver::net_item_granted(1, 5, 0, b"helm", None, 4);
        let tail = &wire[4 + 1 + 1 + 4 + 4..];
        let nine = &tail[24 + EQUIP_BLOCK..24 + EQUIP_BLOCK + 9];
        assert_eq!(nine, &[0u8; 9], "no equip block means no socket bytes");
    }

    #[test]
    fn an_unwearable_row_never_enters_a_relic_pool() {
        let full = fake_vfs();
        let keep = |p: &'static str| (p, vfs_entry(&full, p).expect(p).to_vec());
        let (lo, hi) = RELIC_BANDS[0].1;
        let mut entries = vec![
            keep(CATALOG_PATH),
            keep("c/csv/item_18.dat"),
            keep("c/csv/item_12.dat"),
            keep("c/csv/item_10.dat"),
            keep("c/csv/item_11.dat"),
        ];
        // Every armour row in the cheapest band is group 7 except the first.
        for k in ARMOUR_KINDS {
            let rows: Vec<Vec<u8>> = (0..81u8)
                .map(|i| {
                    let g = if (lo..=hi).contains(&i) && i != lo { 7 } else { 5 };
                    equip_row(format!("a{k}-{i}").as_bytes(), g, 20u8.saturating_add(i))
                })
                .collect();
            let path: &'static str = Box::leak(format!("c/csv/item_{k:02}.dat").into_boxed_str());
            entries.push((path, item_table(&rows)));
        }
        let t = GrantTable::build(&[], &vfs_of(&entries), None);
        for _ in 0..20 {
            let r = t.records(18, None);
            assert_eq!(r[0].index, lo, "only the one wearable row may be drawn");
            assert_eq!(r[0].wearer_group(), 5);
        }
    }

    /// The byte that decides whether a granted equipment item can ever be
    /// worn. Zero is refused by every class
    /// (`EquipItemInfo::IsEquipPossible`, 0xA81AC), and it was zero for the
    /// 공허의 목걸이 from the day that remap landed.
    #[test]
    fn granted_equipment_carries_a_wearer_group_and_other_kinds_do_not() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        // 공허의 목걸이 -> item_10 row 17, kind 10: equipment.
        let neck = t.records(46, None);
        assert_eq!(neck[0].kind, ACCESSORY_KIND);
        // `item_10` row 17 is group 5 in the shipped archive. Sending 0 — what
        // every grant sent until now — is refused by every class, so the remap
        // was handing over a necklace nobody could wear.
        assert_eq!(neck[0].wearer_group(), 5);
        // 엘릭서 -> kind 11, and every kind above 10 ends at the name, so the
        // field is not on the wire at all and must stay zero.
        let elixir = t.records(4, None);
        assert_eq!(elixir[0].kind, BATTLE_USE_KIND);
        assert_eq!(elixir[0].wearer_group(), 0);
        // An ordinary kind-18 cash item, likewise.
        let cash = t.records(9, None);
        assert_eq!(cash[0].kind, CASH_ITEM_KIND);
        assert_eq!(cash[0].wearer_group(), 0);
    }

    /// Both remaps aim at a row of another table, and `GetItemTableInfo` would
    /// fabricate an item rather than fail if that row were not there. An
    /// archive without the target table must refuse the row, not grant a
    /// made-up one.
    #[test]
    fn a_remap_whose_target_table_is_absent_is_refused() {
        // The same archive with `item_11.dat` and `item_10.dat` left out.
        let full = fake_vfs();
        let keep = |p: &'static str| (p, vfs_entry(&full, p).expect(p).to_vec());
        let vfs = vfs_of(&[keep(CATALOG_PATH), keep("c/csv/item_18.dat"), keep("c/csv/item_12.dat")]);

        let t = GrantTable::build(&[], &vfs, None);
        for id in [4, 46] {
            assert!(t.records(id, None).is_empty(), "id {id} must not grant a fabricated row");
            assert!(
                t.rejected.iter().any(|(r, why)| *r == id && why.contains("remap target")),
                "id {id} should say why it was refused"
            );
        }
        // …and the ordinary rows are untouched by the refusal.
        assert!(!t.records(9, None).is_empty(), "an item_18 row still grants");
    }

    /// A class-1 listing is a box the operator's server opened. Granting the
    /// listing itself hands the player a token `StateInGameMenu::KeyBagMenu`
    /// gives **no menu at all** — the bug the user hit after buying 하이퍼오브.
    #[test]
    fn a_random_box_grants_real_items_not_the_listing() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        let r = t.records(6, None);
        assert_eq!(r.len(), 5, "오브원석 is five rolls, not one item with a count of five");
        for rec in &r {
            assert_eq!(rec.kind, ORB_KIND, "a real orb, not the kind-18 listing");
            assert!(
                (ORB_PLAIN.0..=ORB_PLAIN.1).contains(&rec.index),
                "index {} is outside the plain-orb rows",
                rec.index
            );
            assert_eq!(rec.count, 1);
        }
    }

    /// Rolling has to happen per purchase. Doing it once at build time would
    /// make every 오브원석 anyone ever buys the same five orbs.
    #[test]
    fn successive_purchases_of_a_box_differ() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        let first: Vec<u8> = t.records(6, None).iter().map(|r| r.index).collect();
        let second: Vec<u8> = t.records(6, None).iter().map(|r| r.index).collect();
        assert_ne!(first, second, "the cursor must move between purchases");
        // …and stay inside the band however many times it is asked.
        for _ in 0..40 {
            for rec in t.records(6, None) {
                assert!((ORB_PLAIN.0..=ORB_PLAIN.1).contains(&rec.index));
            }
        }
    }

    /// 공허의 목걸이 is class 2, so it *does* get a bag menu — but as kind 18
    /// index 43 it could never be worn: the guest's own remap
    /// (`ProcPacket_Shop+0x388`) covers indices 44..48 only.
    #[test]
    fn the_void_necklace_is_remapped_to_the_accessory_table() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        let r = t.records(46, None);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].kind, r[0].index), (ACCESSORY_KIND, VOID_NECKLACE_ROW));
        assert_eq!(r[0].name, b"realnecklace".to_vec(), "the name comes from the target table");
    }

    /// A class-0 row is an account service the shop never delivered into a
    /// bag. `KeyBagMenu` would give it no menu either, so refusing is the
    /// honest answer rather than granting a dead token.
    #[test]
    fn an_account_service_row_is_refused() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        assert!(t.records(2, None).is_empty());
        assert!(t.rejected.iter().any(|(id, _)| *id == 2));
    }

    /// `GetItemTableInfo` does not bounds-check `index`; it multiplies and
    /// reads. An `a` past the end of `item_18.dat` would fabricate an item
    /// out of neighbouring memory, so the host refuses the row instead.
    #[test]
    fn an_index_past_the_item_table_is_refused_not_granted() {
        let vfs = fake_vfs();
        let mut broken = vfs.clone();
        // The second catalogue row's `a` (12) sits right after its 5-byte name.
        let at = broken
            .windows(5)
            .position(|w| w == b"item9")
            .expect("the fake catalogue's second row")
            + 5;
        assert_eq!(broken[at], 12);
        broken[at] = 200;

        let t = GrantTable::build(&[], &broken, None);
        assert!(t.records(9, None).is_empty(), "out-of-range rows must not grant");
        assert!(t.rejected.iter().any(|(id, _)| *id == 9));
        assert!(!t.records(6, None).is_empty(), "the good row is still there");
    }

    /// Without `item_18.dat` there is no usage class, and a grant derived
    /// without one is exactly the token that produces no bag menu. Refusing
    /// is the safe direction.
    #[test]
    fn a_missing_item_table_refuses_every_row() {
        let cat = {
            let full = fake_vfs();
            vfs_entry(&full, CATALOG_PATH).expect("catalogue").to_vec()
        };
        let t = GrantTable::build(&[], &vfs_of(&[(CATALOG_PATH, cat)]), None);
        assert!(t.is_empty());
        assert_eq!(t.rejected.len(), 9);
    }

    #[test]
    fn an_unmapped_id_grants_nothing() {
        let t = GrantTable::build(&[], &fake_vfs(), None);
        assert!(t.records(77, None).is_empty());
        assert!(GrantTable::default().records(4, None).is_empty());
        assert!(GrantTable::default().is_empty());
    }

    /// A `--grant` entry replaces the derived one rather than adding to it —
    /// two records for one purchase would grant the item twice — and it lifts
    /// a refusal, which is what makes it usable as a diagnostic.
    #[test]
    fn an_override_replaces_the_derived_row_and_lifts_a_refusal() {
        let vfs = fake_vfs();
        let t = GrantTable::build(&[ShopGrant { id: 4, kind: 11, index: 3, count: Some(1) }], &vfs, None);
        let r = t.records(4, None);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].kind, r[0].index, r[0].count), (11, 3, 1));

        let t = GrantTable::build(&[ShopGrant { id: 2, kind: 18, index: 2, count: None }], &vfs, None);
        assert!(!t.records(2, None).is_empty(), "an override overrides a refusal too");
        assert!(!t.rejected.iter().any(|(id, _)| *id == 2));
    }

    /// With no catalogue at all an override still has to produce a legal
    /// record: `count` reaches `_NET_ITEM_+0x00`, where zero or less ends the
    /// list and grants nothing.
    #[test]
    fn count_without_a_catalogue_defaults_to_one() {
        let t = GrantTable::build(&[ShopGrant { id: 7, kind: 18, index: 2, count: None }], &[], None);
        assert_eq!(t.records(7, None)[0].count, 1);
    }
}
