//! The four Hell-endgame weapons with `(진)` in the name, as extra boss drops.
//!
//! The user asked for copies of 알케인:혼돈 / 펜리르:야성검 / 데스크림존 /
//! 궁니르:번개 (`item_00..03` row 80) with +20% attack and baked options, and
//! for those copies to come from **monster drops only** — not the smith, not
//! the cash shop, not a 유물함. They also asked to **add** the new items to
//! the bosses' drop table rather than overwrite a slot that already drops
//! 쇼픈 / 스콜 / 오드.
//!
//! # Why this is a new group plus an extra roll, not a fifth slot
//!
//! `/c/csv/droptable.dat` is 252 records, 63 groups × 4 slots.
//! `Monster::SetDropItem` (0xBC910) picks a slot with `Rand(0,3)` and indexes
//! `group * 4 + slot` at 13 bytes each (0xBCA60..0xBCA94). A group cannot grow
//! to five slots without changing that `Rand` and the stride, which would
//! steal probability from the four items already there.
//!
//! So the file change is an **appended group 63** (records 252..255) pointing
//! at the new rows, and the behaviour change is an extra independent band-5
//! walk of that group after the original `SetDropItem` returns. Existing
//! 쇼픈/스콜/오드 slots are untouched. The extra walk uses the monster's own
//! roll count (`Monster+0x275`) and the same `[T4, T5)` legendary band, so a
//! `(진)` lands at the same per-kill rate as one of those existing
//! legendaries — and a lucky kill can drop both.
//!
//! The extra walk only runs for hell bosses that already drop the lv97
//! weapons: group 59..62 and at least four rolls. That is 강화 시작형, 루레인,
//! 악몽의 감시자, 빙의된 루퍼스, 테오도어, 크레이지 머신, 불완전 강림자,
//! 알테나. 붉은눈 (same groups, two rolls) is left alone. Normal and Hard
//! never assign those groups, so they cannot drop a `(진)`.
//!
//! # Why the new rows sit at index 86
//!
//! Rows 81..85 are the 공허 1~5급 cash weapons. [`crate::shop`]'s relic pool
//! refuses anything past [`crate::shop`]`::RELIC_ROW_MAX` = 80, and that fence
//! is left alone: 오래된 유물함's level span would otherwise include a level-97
//! `(진)`. `GetItemTableInfo` does not bounds-check `index` (`mla`), and
//! `LoadItemTable` (0xA38E0) allocates from the file's own `[u16 count]`, so
//! appending one row per table is the same shape [`crate::cashitems`] already
//! uses for `item_18`.
//!
//! # Why the warehouse needs a hook
//!
//! `StateNetMenu::KeyStorage` (0x12D3CC) refuses a bag deposit before any
//! packet when `kind <= 3 && index > 0x50` (`+0xa34`, `ldrsb [sl,#0x15];
//! cmp #0x50; bgt` reject). Index 86 is past that fence, so a `(진)` would
//! show `이용할 수 없는 아이템입니다.` and never reach `4/10`. The host
//! store already keeps the 72 bytes it is given; the guest just never
//! sends them.
//!
//! The hook sits on that compare, skips only [`JIN_INDEX`], and resumes at
//! the next check (`(10, 17)`). Equipped items, kind 14 / 18, 공허 81..85,
//! and the rest of the fence stay refused. Touch only moves the cursor;
//! SELECT and a second tap both arrive here (`KeyPressed` 0x139848).
//! Drop-only, no smith, no cash shop, no 유물함 — those policies do not
//! move. Not `patch.rs`.
//!
//! # What is scaled, and what must not be
//!
//! The 21-byte tail is `ItemInfo+0x150..+0x164`. `+0x150` is the sprite file
//! and `+0x152` is the palette (ANALYSIS.md §45) — those stay. Only
//! `+0x156` / `+0x158` and the three baked option **values** (`+0x160` /
//! `+0x163` / `+0x164`, file bytes 0x10 / 0x12 / 0x14 of the tail) move by
//! 6/5. Grade 4 has no `MakeItemOption` slots, so there is nothing random to
//! roll.
//!
//! # Set armour (groups 64..)
//!
//! No shipped drop-table slot is kind 5..8, so set armour had no monster
//! path. Fourteen appended groups cover every `set_option` row of
//! `item_05..08`. The four hell endgame sets (지크문드 / 폭군쟈칼 / 푸른비룡 /
//! 아체야키) each get their own band-5 walk — same per-kill rate as a `(진)`.
//! The ten lower sets share one walk so a hell boss does not rain 가시나무.
//! A piece that already has a mix book still occupies its slot: `Rand(0,3)`
//! cannot be a three-way split. Relic boxes are left alone. Not `patch.rs`.
//!
//! # Why a drop can show `+랜덤옵션` instead of an option
//!
//! `CommonUi::DrawItemDiscription` draws as many option lines as the
//! **grade** asks for (1 / 2 / 3 from 레어 / 에픽 / 영웅-and-up) and prints
//! `common_text` 287 = `+랜덤옵션` for any of those slots whose kind byte is
//! `<= 0` ([`crate::shop::empty_option_lines`]). Every shipped 영웅 and 세트
//! row reserves slot 2 with `0xFF`, expecting a factory to fill it. The
//! guest's own factory (`ItemTable::NewItemEx` → `MakeItemOption`) does;
//! the drop path does not — `MapItem::NewDropItem` copies the table,
//! overwrites grade/options only where the signed-char args are `>= 0`, then
//! calls `MakeSocket` and never `MakeItemOption`. The shipped game gets away
//! with it because `Monster::SetDropItem` hands band 4 and 6 the drop
//! record's own `(kind, value)` pair as slot 2 (0xBCB54 / 0xBCE30), and
//! every band-5 row names all three slots itself. A host walk that passes
//! `0xFF` for every override — as the set roll did, and as
//! [`crate::lostdrops`] did for a band-4 slot — leaves the reserved byte and
//! the player reads a promise with nothing behind it. Same hole as the
//! 유물함 wire grant (`NewBagNetItem`), a third door.
//!
//! The fill is a hook on `MakeSocket` that fires only from `NewDropItem`'s
//! one call (`+0x48c`), and only when the item would still print the
//! phrase: after the original `MakeSocket` it runs the guest's own
//! `ItemTable::SetItemOption(item, slot)` for each such slot — the exact
//! call `MakeItemOption` makes, band = slot. That is before auto-pickup
//! copies the item into the bag. It is a net, not a policy: the shipped
//! drop table never trips it (a census of all 252 records against
//! `item_00..10` finds no band whose result has an empty displayed slot),
//! so original drops keep their table bytes to the last one; only a host
//! door that forgot the pair is caught, whichever door that is.

use crate::mem::RETURN_MAGIC;
use crate::patch::PatchFn;
use crate::shop::EQUIP_BLOCK;
use crate::Vm;
use arm32_cpu::{reg, Mode};
use std::path::Path;

/// `item_00..03.dat` — one file per class weapon table.
pub const WEAPON_PATHS: [&str; 4] = [
    "c/csv/item_00.dat",
    "c/csv/item_01.dat",
    "c/csv/item_02.dat",
    "c/csv/item_03.dat",
];

/// `/c/csv/droptable.dat`.
pub const DROP_TABLE_PATH: &str = "c/csv/droptable.dat";

/// Shipped row copied into the `(진)` weapon. Level 97, grade 4.
pub const SOURCE_INDEX: usize = 80;

/// Where the copy lands. 81..85 are the 공허 cash weapons; 86 is the first
/// free index past every hardcoded fence.
pub const JIN_INDEX: u8 = 86;

/// Shipped `item_00..03` row count (indices 0..85).
pub const SHIPPED_WEAPON_ROWS: u16 = 86;

/// Shipped `droptable.dat` record count (63 groups × 4).
pub const SHIPPED_DROP_RECORDS: u16 = 252;

/// New group. `Monster+0x273` is a byte, so 63 is in range.
pub const JIN_GROUP: u8 = 63;

/// First appended set group. [`SET_ROWS`]`[i]` lives at `SET_GROUP + i`.
pub const SET_GROUP: u8 = 64;

/// Shipped 지크문드 row — first of [`HELL_SET_ROWS`].
pub const SET_INDEX: u8 = 80;

/// 투구 / 갑옷 / 장갑 / 장화. Slot `i` of a set group is `SET_KINDS[i]`.
pub const SET_KINDS: [u8; 4] = [5, 6, 7, 8];

/// Hell endgame sets, each with its own extra walk. 지크문드, 폭군쟈칼,
/// 푸른비룡, 아체야키 — `item_05..08` rows 80..77.
pub const HELL_SET_ROWS: [u8; 4] = [80, 79, 78, 77];

/// Lower sets. They share one extra walk. 드래곤 … 가시나무.
pub const LOWER_SET_ROWS: [u8; 10] = [76, 75, 46, 45, 44, 43, 26, 25, 24, 23];

/// [`HELL_SET_ROWS`] then [`LOWER_SET_ROWS`]. Group `SET_GROUP + i`.
pub const SET_ROWS: [u8; 14] = [80, 79, 78, 77, 76, 75, 46, 45, 44, 43, 26, 25, 24, 23];

/// Hell groups whose four slots are the lv97 legendaries (쇼픈 line through
/// 알테나). Confirmed by walking `enemy_2.dat` against `droptable.dat`.
pub const BOSS_GROUPS: std::ops::RangeInclusive<i8> = 59..=62;

/// Named hell bosses use 4 rolls; 붉은눈 shares a group but only has 2.
const BOSS_ROLLS: u8 = 4;

/// EUC-KR `(진)`. Goes **in front** of the shipped name
/// (`(진)데스크림존`, not `데스크림존(진)`).
pub const JIN_PREFIX: &[u8] = &[0x28, 0xc1, 0xf8, 0x29];

/// `LoadItemTable` zeroes five words at `ItemInfo+0x18` then `memcpy`s the
/// name there. Longer than 20 bytes walks into the next field.
const NAME_CAP: usize = 20;

/// One record in `droptable.dat`, including the `[u16 reclen]`.
const DROP_REC: usize = 13;

/// Template group whose four slots are already kind 0..3 of a lv97
/// legendary. Group 61 = 오드 / 그랑드 / 레딘 / 히엘로 (index 78).
const TEMPLATE_GROUP: usize = 61;

/// `+20%`, rounded to nearest.
fn scale_u16(n: u16) -> u16 {
    ((u32::from(n) * 6 + 2) / 5).min(u32::from(u16::MAX)) as u16
}

fn scale_u8(n: u8) -> u8 {
    ((u32::from(n) * 6 + 2) / 5).min(255) as u8
}

/// Walk a `[u16 count]` + `[u16 reclen][body]` table. `None` unless the walk
/// lands on EOF — the same closure check every other table parse here uses.
fn table_end(b: &[u8]) -> Option<(u16, usize)> {
    let count = u16::from_le_bytes([*b.first()?, *b.get(1)?]);
    let mut pos = 2usize;
    for _ in 0..count {
        let reclen = usize::from(u16::from_le_bytes([*b.get(pos)?, *b.get(pos + 1)?]));
        pos = pos.checked_add(2)?.checked_add(reclen)?;
        if pos > b.len() {
            return None;
        }
    }
    if pos != b.len() {
        return None;
    }
    Some((count, pos))
}

/// Body of record `index`, without the reclen halfword.
pub(crate) fn record_body(b: &[u8], index: usize) -> Option<&[u8]> {
    let count = usize::from(u16::from_le_bytes([*b.first()?, *b.get(1)?]));
    if index >= count {
        return None;
    }
    let mut pos = 2usize;
    for i in 0..count {
        let reclen = usize::from(u16::from_le_bytes([*b.get(pos)?, *b.get(pos + 1)?]));
        let start = pos + 2;
        let end = start.checked_add(reclen)?;
        if end > b.len() {
            return None;
        }
        if i == index {
            return b.get(start..end);
        }
        pos = end;
    }
    None
}

/// Copy row [`SOURCE_INDEX`], prefix `(진)`, scale the two stats and the
/// three option values. Returns the new image, or `None` if the file is not
/// the shipped shape.
pub fn append_jin_weapon(src: &[u8]) -> Option<Vec<u8>> {
    let (count, _) = table_end(src)?;
    if count != SHIPPED_WEAPON_ROWS {
        return None;
    }
    let body = record_body(src, SOURCE_INDEX)?;
    let nlen = usize::from(*body.get(2)?);
    let name = body.get(3..3 + nlen)?;
    if name.len() + JIN_PREFIX.len() > NAME_CAP {
        return None;
    }
    let tail = body.get(3 + nlen..)?;
    if tail.len() < EQUIP_BLOCK {
        return None;
    }
    let mut scaled = tail.to_vec();
    let eq = scaled.len() - EQUIP_BLOCK;
    let stat = |o: &mut [u8], at: usize| {
        let v = u16::from_le_bytes([o[at], o[at + 1]]);
        o[at..at + 2].copy_from_slice(&scale_u16(v).to_le_bytes());
    };
    // `ItemInfo+0x156` / `+0x158` — file offsets 6 and 8 of the 21-byte tail.
    // Sprite (0) and palette (2) stay; see the module comment.
    stat(&mut scaled[eq..], 6);
    stat(&mut scaled[eq..], 8);
    // Three baked option values. Types at 0x0f / 0x11 / 0x13 are left alone.
    for at in [0x10, 0x12, 0x14] {
        scaled[eq + at] = scale_u8(scaled[eq + at]);
    }

    let mut rec = Vec::with_capacity(2 + nlen + JIN_PREFIX.len() + scaled.len());
    rec.extend_from_slice(&body[..2]);
    rec.push((nlen + JIN_PREFIX.len()) as u8);
    rec.extend_from_slice(JIN_PREFIX);
    rec.extend_from_slice(name);
    rec.extend_from_slice(&scaled);

    let mut out = Vec::with_capacity(src.len() + 2 + rec.len());
    out.extend_from_slice(&(count + 1).to_le_bytes());
    out.extend_from_slice(&src[2..]);
    out.extend_from_slice(&(rec.len() as u16).to_le_bytes());
    out.extend_from_slice(&rec);
    // The walk that accepted `src` must still accept the result, and the new
    // last row is the only thing that grew.
    let (n, end) = table_end(&out)?;
    if n != count + 1 || end != out.len() {
        return None;
    }
    Some(out)
}

/// Append group [`JIN_GROUP`]: four slots, kinds 0..3.
///
/// `jin` picks the row those slots point at — [`JIN_INDEX`] for `main`,
/// [`SOURCE_INDEX`] for `vanilla` — so the file says the same thing
/// [`extra_endgame_rolls`] does on that line. Nothing actually reads this
/// group either way (no shipped `enemy_<N>.dat` holds [`JIN_GROUP`], and the
/// extra walk calls `NewDropItem` itself); it is appended so [`SET_GROUP`]
/// keeps its number, and kept honest so the table does not misdescribe the
/// build it shipped with.
///
/// The other ten payload bytes are copied from group [`TEMPLATE_GROUP`] so a
/// field this host does not decode (the per-slot extras `SetDropItem` still
/// reads) stays a real drop-table value rather than a guess.
pub fn append_jin_drop_group(src: &[u8], jin: bool) -> Option<Vec<u8>> {
    let count = u16::from_le_bytes([*src.first()?, *src.get(1)?]);
    if count != SHIPPED_DROP_RECORDS || src.len() != 2 + usize::from(count) * DROP_REC {
        return None;
    }
    let mut out = src.to_vec();
    out[0..2].copy_from_slice(&(count + 4).to_le_bytes());
    for slot in 0..4 {
        let i = TEMPLATE_GROUP * 4 + slot;
        let at = 2 + i * DROP_REC;
        let mut rec = src.get(at..at + DROP_REC)?.to_vec();
        if rec[0..2] != 11u16.to_le_bytes() {
            return None;
        }
        rec[2] = JIN_GROUP;
        rec[9] = slot as u8;
        rec[10] = if jin { JIN_INDEX } else { SOURCE_INDEX as u8 };
        out.extend_from_slice(&rec);
    }
    if out.len() != 2 + usize::from(count + 4) * DROP_REC {
        return None;
    }
    Some(out)
}

/// Append one group per [`SET_ROWS`] onto a table that already has [`JIN_GROUP`].
///
/// Kinds 5..8, each set's armour-table row. Same template-copy as
/// [`append_jin_drop_group`], so the unread extra bytes stay real values.
pub fn append_set_drop_group(src: &[u8]) -> Option<Vec<u8>> {
    let count = u16::from_le_bytes([*src.first()?, *src.get(1)?]);
    if count != SHIPPED_DROP_RECORDS + 4 || src.len() != 2 + usize::from(count) * DROP_REC {
        return None;
    }
    let added = 4 * SET_ROWS.len();
    let mut out = src.to_vec();
    out[0..2].copy_from_slice(&(count + added as u16).to_le_bytes());
    for (g, &index) in SET_ROWS.iter().enumerate() {
        for slot in 0..4 {
            let i = TEMPLATE_GROUP * 4 + slot;
            let at = 2 + i * DROP_REC;
            let mut rec = src.get(at..at + DROP_REC)?.to_vec();
            if rec[0..2] != 11u16.to_le_bytes() {
                return None;
            }
            rec[2] = SET_GROUP + g as u8;
            rec[9] = SET_KINDS[slot];
            rec[10] = index;
            out.extend_from_slice(&rec);
        }
    }
    if out.len() != 2 + (usize::from(count) + added) * DROP_REC {
        return None;
    }
    Some(out)
}

/// Materialise the four weapon tables and the new drop group.
///
/// Failures are reported and skipped, never fatal: without the files the
/// guest reads the archive, the hook sees 86 rows and declines, and nothing
/// new drops.
/// `jin_drops = false` is the `vanilla` line (`BRANCHES.md`): **the four
/// `(진)` rows are still written**, so a copy already in someone's bag,
/// equipment or warehouse loads with its real name and stats — only the way
/// to *get a new one* goes away.
///
/// Keeping the rows is not a nicety, it is the safe choice. A save holds two
/// bytes per item, `kind` and `index` (`HERO::SaveBagData` 0x8F4F4), and
/// `HERO::LoadBagData` hands them to `ItemTable::GetItemTableInfo` (0xA2728),
/// which **does not bounds-check the index** — it is one `mla` by the 0x178
/// record stride (0xA27A0). An 86-row table plus a saved index 86 is a read
/// past the end of the array, i.e. an item built out of whatever heap follows.
/// It does not fault and it does not go away: `SaveBagData` writes the same
/// two bytes back. So the rows stay and the acquisition stops instead.
///
/// What changes: group [`JIN_GROUP`] and [`extra_endgame_rolls`] point at
/// [`SOURCE_INDEX`] instead of [`JIN_INDEX`]. **The roll itself does not
/// move** — same bosses, same `[T4, T5)` band, same count — so both lines
/// drop at exactly the same rate and differ only in which weapon lands.
/// Group 63 is appended either way because [`SET_GROUP`] is 64 and the set
/// groups have to keep their numbers; no monster is ever assigned group 63
/// (`Monster+0x273` never holds it in any shipped `enemy_<N>.dat`).
///
/// The other doors were already shut for both lines: the smith and the cash
/// shop never list row 86, and `shop::RELIC_ROW_MAX` is 80.
pub fn install(vfs: &[u8], storage: &Path, jin_drops: bool) -> (Vec<String>, bool) {
    let mut logs = Vec::new();
    let mut ok = true;
    for path in WEAPON_PATHS {
        let Some(src) = crate::shop::vfs_entry(vfs, path) else {
            logs.push(format!("legendaries: {path} is not in the archive — (진) not added"));
            ok = false;
            continue;
        };
        let Some(patched) = append_jin_weapon(src) else {
            logs.push(format!(
                "legendaries: {path} ({} B) did not parse as {SHIPPED_WEAPON_ROWS} rows — (진) not added",
                src.len()
            ));
            ok = false;
            continue;
        };
        match crate::rates::write_override(storage, path, &patched) {
            Ok(dst) => logs.push(format!(
                "legendaries: {} B -> {} (row {JIN_INDEX} = row {SOURCE_INDEX} + (진), +20%)",
                patched.len(),
                dst.display()
            )),
            Err(e) => {
                logs.push(format!("legendaries: cannot write {path}: {e}"));
                ok = false;
            }
        }
    }
    let Some(src) = crate::shop::vfs_entry(vfs, DROP_TABLE_PATH) else {
        logs.push(format!(
            "legendaries: {DROP_TABLE_PATH} is not in the archive — drop group {JIN_GROUP} not added"
        ));
        return (logs, false);
    };
    let Some(with_jin) = append_jin_drop_group(src, jin_drops) else {
        logs.push(format!(
            "legendaries: {DROP_TABLE_PATH} ({} B) did not parse as {SHIPPED_DROP_RECORDS} records — drop group {JIN_GROUP} not added",
            src.len()
        ));
        return (logs, false);
    };
    let Some(patched) = append_set_drop_group(&with_jin) else {
        logs.push(format!(
            "legendaries: {DROP_TABLE_PATH} after group {JIN_GROUP} did not accept group {SET_GROUP}"
        ));
        return (logs, false);
    };
    match crate::rates::write_override(storage, DROP_TABLE_PATH, &patched) {
        Ok(dst) => logs.push(format!(
            "legendaries: {} B -> {} (group {JIN_GROUP} = {line}; groups {SET_GROUP}..{} = {} sets)",
            patched.len(),
            dst.display(),
            SET_GROUP + SET_ROWS.len() as u8 - 1,
            SET_ROWS.len(),
            line = if jin_drops { "(진)" } else { "placeholder, vanilla" }
        )),
        Err(e) => {
            logs.push(format!("legendaries: cannot write {DROP_TABLE_PATH}: {e}"));
            ok = false;
        }
    }
    (logs, ok)
}

const SET_DROP_ITEM: &str = "_ZN7Monster11SetDropItemEv";
pub(crate) const NEW_DROP_ITEM: &str = "_ZN7MapItem11NewDropItemEiiaaaaaaaaa";
const MAKE_SOCKET: &str = "_ZN9ItemTable10MakeSocketEP8ItemInfo";
/// `(ItemTable*, ItemInfo*, signed char slot)` — rolls one option pair from
/// `option.dat` band `slot` into `+0x15f+slot` / `+0x162+slot`. What
/// `MakeItemOption` tail-calls for the grade's slot; called here per slot.
const SET_ITEM_OPTION: &str = "_ZN9ItemTable13SetItemOptionEP8ItemInfoa";
pub(crate) const RAND: &str = "_ZN10StaticUtil4RandEii";
pub(crate) const GET_X: &str = "_ZN6OBJECT4GetXEv";
pub(crate) const GET_Y: &str = "_ZN6OBJECT4GetYEv";
const KEY_STORAGE: &str = "_ZN12StateNetMenu10KeyStorageEi";
const LOAD_BAG: &str = "_ZN4HERO11LoadBagDataEa";
const SAVE_BAG: &str = "_ZN4HERO11SaveBagDataEa";
const GET_BAG: &str = "_ZN4HERO13GetBagItemPtrEv";
const BAG_SIZE: &str = "_ZN7BagItem7GetSizeEv";
const BAG_AT: &str = "_ZN7BagItem13GetBagItemPtrEs";
const NEW_ITEM: &str = "_ZN9ItemTable9NewItemExEaa";
const NEW_BAG: &str = "_ZN7BagItem10NewBagItemEP8ItemInfoh";

/// One-shot operator grant. `HL5_INJECT_JIN_SLOT` is the guest slot index
/// (`0` / `1` / `2`). Unset means the hook is not installed. The marker
/// file plus a bag scan keep a second load from duplicating the four.
fn inject_target_slot() -> Option<i8> {
    option_env!("HL5_INJECT_JIN_SLOT")?.trim().parse().ok()
}

const INJECT_MARKER: &str = "hl5_jin_slot_injected.txt";

/// `ldrsb r3, [sl, #0x15]` at 0x12DE00 — about to `cmp #0x50; bgt` reject.
/// Confirmed against the shipping `.so` (`e1da31d5` at that word).
const INDEX_FENCE_OFF: u32 = 0xa34;

/// The packed `(kind, index) == (10, 17)` check, immediately after the fence
/// (0x12DE0C). A `(진)` fails that compare and falls into the deposit dialog.
const AFTER_INDEX_FENCE_OFF: u32 = 0xa40;

/// `sl` holds the `ItemInfo*` for the highlighted bag cell. The fence reads
/// `+0x15` from it; the hook reads the same two bytes.
const SL: u8 = 10;

/// `NewDropItem` `bl MakeSocket` at `+0x488`; the return address the hook
/// keys on. Confirmed against the shipping `.so` (`ebffe5cc` at `0xa7aec`).
const NEW_DROP_MAKE_SOCKET_LR: u32 = 0x48c;

mod iteminfo {
    pub const KIND: u32 = 0x14;
    pub const INDEX: u32 = 0x15;
    pub const GRADE: u32 = 0x15c;
    /// The three option kind bytes, slots 0..3 (`SetItemOption` 0xa106c
    /// `strb [sl,#0x15f]` with `sl = item + slot`). Values sit at `+0x162..`.
    pub const OPT_KINDS: u32 = 0x15f;
}

/// `gv + 0x1444` = `ItemTable*`, `+ 0x1448` = `MapItem*`. Both taken from
/// `SetDropItem` itself (0xBCA8C / 0xBCBD0).
pub(crate) const GV_ITEM_TABLE: u32 = 0x1444;
pub(crate) const GV_MAP_ITEM: u32 = 0x1448;

/// `ItemTable + 0x214` = the `droptable.dat` bytes as `LoadItemDropTable`
/// (0xA0B54) got them from `LoadRes` — `[u16 count]` still in front, so
/// record `i`'s reclen is at `13 * i` and its body at `13 * i + 2`.
/// `SetDropItem` indexes exactly that (0xBCA94 `mul` by 13, then `+5..+14`).
pub(crate) const ITEM_TABLE_DROP_BUF: u32 = 0x214;

/// `LoadItemTable` stores kind `k`'s array at `ItemTable + (k + 10) * 4`,
/// and the `[u32 count]` lives four bytes before the first `EquipItemInfo`.
const KIND_PTR_BASE: u32 = 10;

mod monster {
    pub const T4: u32 = 0x264;
    pub const T5: u32 = 0x268;
    pub const GROUP: u32 = 0x273;
    pub const ROLLS: u32 = 0x275;
}

/// Install the extra-roll hook, the set-option fill, and the warehouse
/// fence skip, or report that this binary cannot carry them.
pub fn install_hooks(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for name in [
        SET_DROP_ITEM,
        NEW_DROP_ITEM,
        MAKE_SOCKET,
        SET_ITEM_OPTION,
        RAND,
        GET_X,
        GET_Y,
        KEY_STORAGE,
        crate::warehouse::GV,
    ] {
        if vm.symbol(name).is_none() {
            return Err(format!("legendaries hook needs `{name}`, not in this binary"));
        }
    }
    let drop = vm.symbol(SET_DROP_ITEM).expect("checked above");
    let sock = vm.symbol(MAKE_SOCKET).expect("checked above");
    let storage = vm.symbol(KEY_STORAGE).expect("checked above");
    let f_drop: PatchFn = extra_jin_on_boss;
    let f_sock: PatchFn = fill_empty_options_on_drop;
    let f_wh: PatchFn = allow_jin_in_warehouse;
    vm.install_hook(SET_DROP_ITEM, drop, f_drop)?;
    vm.install_hook(MAKE_SOCKET, sock, f_sock)?;
    vm.install_hook(KEY_STORAGE, storage + INDEX_FENCE_OFF, f_wh)?;
    let mut names = vec![SET_DROP_ITEM, MAKE_SOCKET, KEY_STORAGE];
    if let Some(slot) = inject_target_slot() {
        for name in [LOAD_BAG, SAVE_BAG, GET_BAG, BAG_SIZE, BAG_AT, NEW_ITEM, NEW_BAG] {
            if vm.symbol(name).is_none() {
                return Err(format!("legendaries inject needs `{name}`, not in this binary"));
            }
        }
        let load = vm.symbol(LOAD_BAG).expect("checked above");
        let f_inj: PatchFn = inject_jin_on_load;
        vm.install_hook(LOAD_BAG, load, f_inj)?;
        names.push(LOAD_BAG);
        vm.log.push(format!(
            "legendaries: will grant (진)×4 on LoadBagData slot {slot} (once)"
        ));
    }
    Ok(names)
}

/// After the guest has rebuilt slot N's bag from the save, put the four
/// `(진)` weapons in if this load is the operator slot and they are not
/// already there. `SaveBagData` is the guest's own writer, so the DES
/// wrapping stays the guest's business.
fn inject_jin_on_load(vm: &mut Vm) -> Result<bool, String> {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if lr == RETURN_MAGIC {
        return Ok(false);
    }
    let Some(want) = inject_target_slot() else {
        return Ok(false);
    };
    let hero = vm.arg(0);
    let slot = vm.arg(1) as i8;
    let Some(load) = vm.symbol(LOAD_BAG) else {
        return Ok(false);
    };
    vm.call(load, &[hero, slot as u32])?;
    if slot != want {
        return Ok(true);
    }
    let marker = vm.cfg.storage.join(INJECT_MARKER);
    if marker.exists() {
        return Ok(true);
    }
    match grant_four_jin(vm, hero) {
        Ok(n) if n > 0 => {
            if let Some(save) = vm.symbol(SAVE_BAG) {
                vm.call(save, &[hero, slot as u32])?;
            }
            if let Err(e) = std::fs::write(&marker, format!("slot {slot}\n")) {
                vm.log.push(format!("legendaries: inject marker not written: {e}"));
            }
            vm.log.push(format!(
                "legendaries: injected {n} (진) into save slot {slot} and saved"
            ));
        }
        Ok(_) => {
            vm.log.push(format!(
                "legendaries: slot {slot} already had every (진); not saving"
            ));
            let _ = std::fs::write(&marker, format!("slot {slot} already\n"));
        }
        Err(e) => vm.log.push(format!("legendaries: inject skipped: {e}")),
    }
    Ok(true)
}

fn grant_four_jin(vm: &mut Vm, hero: u32) -> Result<u32, String> {
    let (Some(gv), Some(get_bag), Some(size_of), Some(at), Some(new_item), Some(new_bag)) = (
        vm.symbol(crate::warehouse::GV),
        vm.symbol(GET_BAG),
        vm.symbol(BAG_SIZE),
        vm.symbol(BAG_AT),
        vm.symbol(NEW_ITEM),
        vm.symbol(NEW_BAG),
    ) else {
        return Err("inject symbols missing".into());
    };
    let table = vm.mem.read_u32(gv + GV_ITEM_TABLE);
    if table == 0 {
        return Err("ItemTable not loaded".into());
    }
    let bag = vm.call(get_bag, &[hero])?;
    if bag == 0 {
        return Err("no BagItem".into());
    }
    let mut added = 0u32;
    for kind in 0u32..4 {
        if bag_has_jin(vm, bag, size_of, at, kind as u8)? {
            vm.log.push(format!("legendaries: bag already has (진) kind={kind}"));
            continue;
        }
        if table_rows(vm, kind as u8).unwrap_or(0) <= u32::from(JIN_INDEX) {
            return Err(format!("item_{kind:02} has no row {JIN_INDEX}"));
        }
        let item = vm.call(new_item, &[table, kind, u32::from(JIN_INDEX)])?;
        if item == 0 {
            return Err(format!("NewItemEx({kind},{JIN_INDEX}) returned 0"));
        }
        // flag 1 = scan 96 slots, same as the shop grant path.
        let ok = vm.call(new_bag, &[bag, item, 1])?;
        if ok == 0 {
            return Err(format!("NewBagItem refused kind={kind} (bag full?)"));
        }
        added += 1;
        vm.log.push(format!(
            "legendaries: granted (진) kind={kind} index={JIN_INDEX}"
        ));
    }
    Ok(added)
}

fn bag_has_jin(
    vm: &mut Vm,
    bag: u32,
    size_of: u32,
    at: u32,
    kind: u8,
) -> Result<bool, String> {
    let n = vm.call(size_of, &[bag])? as i32;
    for i in 0..n.max(0) {
        let item = vm.call(at, &[bag, i as u32])?;
        if item == 0 {
            continue;
        }
        if vm.mem.read_u8(item + iteminfo::KIND) == kind
            && vm.mem.read_u8(item + iteminfo::INDEX) == JIN_INDEX
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Guest `KeyStorage` is about to refuse `kind <= 3 && index > 80`. Skip
/// that compare for a `(진)` and resume at the next check. Everything else
/// — equipped, kind 14/18, 공허 81..85, `(10, 17)` — declines so the
/// original `bgt` still fires.
fn allow_jin_in_warehouse(vm: &mut Vm) -> Result<bool, String> {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if lr == RETURN_MAGIC {
        return Ok(false);
    }
    let item = vm.cpu.reg_get(Mode::User, SL);
    if item == 0 {
        return Ok(false);
    }
    let kind = vm.mem.read_u8(item + iteminfo::KIND);
    let index = vm.mem.read_u8(item + iteminfo::INDEX);
    if !warehouse_accepts_jin(kind, index) {
        return Ok(false);
    }
    let Some(base) = vm.symbol(KEY_STORAGE) else {
        return Ok(false);
    };
    vm.cpu.reg_set(Mode::User, reg::LR, base + AFTER_INDEX_FENCE_OFF);
    vm.log.push(format!(
        "legendaries: warehouse accepted (진) kind={kind} index={index}"
    ));
    Ok(true)
}

/// The one pair the `index > 0x50` fence was opened for. 공허 cash
/// weapons (81..85) and every other kind stay on the guest's refuse list.
fn warehouse_accepts_jin(kind: u8, index: u8) -> bool {
    kind <= 3 && index == JIN_INDEX
}

/// `Monster::SetDropItem` — run the original, then the extra endgame-weapon
/// and set-armour walks for the hell bosses that already drop the lv97 line.
fn extra_jin_on_boss(vm: &mut Vm) -> Result<bool, String> {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if lr == RETURN_MAGIC {
        return Ok(false);
    }
    let this = vm.arg(0);
    let Some(drop) = vm.symbol(SET_DROP_ITEM) else {
        return Ok(false);
    };
    vm.call(drop, &[this])?;
    if let Err(e) = extra_endgame_rolls(vm, this) {
        vm.log.push(format!("legendaries: extra roll skipped: {e}"));
    }
    if let Err(e) = extra_set_rolls(vm, this) {
        vm.log.push(format!("legendaries: set roll skipped: {e}"));
    }
    // The stranded slots of ordinary mobs. Same shape, different door: those
    // bands are zero-width in the shipped ladder, so the guest's own
    // `SetDropItem` above can never have paid them. See [`crate::lostdrops`].
    let slots = std::mem::take(&mut vm.lostdrops);
    let r = crate::lostdrops::extra_lost_rolls(vm, this, &slots);
    vm.lostdrops = slots;
    if let Err(e) = r {
        vm.log.push(format!("lostdrops: roll skipped: {e}"));
    }
    Ok(true)
}

/// `NewDropItem` has just asked `MakeSocket` to roll sockets. If the item
/// would still print `+랜덤옵션` on any of the lines its grade draws, run the
/// guest's own `SetItemOption` for each such slot after the original
/// `MakeSocket` — the call `NewItemEx` makes through `MakeItemOption` and
/// this path never did. See the module comment. Declines every other caller
/// (`NewItemEx`, mix) so those keep their own `MakeItemOption`, and declines
/// every drop that already names its lines — which is every shipped one.
fn fill_empty_options_on_drop(vm: &mut Vm) -> Result<bool, String> {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if lr == RETURN_MAGIC {
        return Ok(false);
    }
    let Some(new_drop) = vm.symbol(NEW_DROP_ITEM) else {
        return Ok(false);
    };
    if lr != new_drop + NEW_DROP_MAKE_SOCKET_LR {
        return Ok(false);
    }
    let table = vm.arg(0);
    let item = vm.arg(1);
    if item == 0 {
        return Ok(false);
    }
    let empty = drop_empty_option_lines(vm, item);
    if empty.is_empty() {
        return Ok(false);
    }
    let (Some(make_sock), Some(set_opt)) = (vm.symbol(MAKE_SOCKET), vm.symbol(SET_ITEM_OPTION))
    else {
        return Ok(false);
    };
    vm.call(make_sock, &[table, item])?;
    for &slot in &empty {
        vm.call(set_opt, &[table, item, u32::from(slot)])?;
    }
    vm.log.push(format!(
        "legendaries: drop kind={} index={} grade={} rolled option slot(s) {:?} \
         that would have read +랜덤옵션",
        vm.mem.read_u8(item + iteminfo::KIND),
        vm.mem.read_u8(item + iteminfo::INDEX) as i8,
        vm.mem.read_u8(item + iteminfo::GRADE) as i8,
        empty,
    ));
    Ok(true)
}

/// The option slots of a live `ItemInfo` that `DrawItemDiscription` would
/// print `+랜덤옵션` for — equipment only (`kind <= 10`, the `MakeItemOption`
/// gate at 0xa10f8; nothing past that has option lines).
fn drop_empty_option_lines(vm: &Vm, item: u32) -> Vec<u8> {
    if vm.mem.read_u8(item + iteminfo::KIND) > 10 {
        return Vec::new();
    }
    let kinds = [0, 1, 2].map(|s| vm.mem.read_u8(item + iteminfo::OPT_KINDS + s) as i8);
    crate::shop::empty_option_lines(vm.mem.read_u8(item + iteminfo::GRADE), kinds)
}

fn extra_endgame_rolls(vm: &mut Vm, monster: u32) -> Result<(), String> {
    // Both content lines take this walk: same bosses, same `[T4, T5)` band,
    // same number of rolls. **Only the row differs.** `main` lands on the
    // `(진)` copy; `vanilla` lands on the shipped row that copy was made
    // from, which that line can already reach through the smith
    // (`rates::fix_hell_legendary_results` points Hell pair B at row 80).
    // That is the whole split — the same drop rate, a different item — and it
    // is why no monster's drop rate can tell the two APKs apart.
    //
    // The row-count guard below asks for [`JIN_INDEX`] on both lines, even
    // though `vanilla` never drops it. That is deliberate: if the weapon
    // shadow failed to land, the extra roll has to disappear from *both*
    // lines together, or the failure would itself become a rate difference.
    let row = if vm.cfg.vanilla { SOURCE_INDEX as u8 } else { JIN_INDEX };
    let group = vm.mem.read_u8(monster + monster::GROUP) as i8;
    let rolls = vm.mem.read_u8(monster + monster::ROLLS);
    if !BOSS_GROUPS.contains(&group) || rolls < BOSS_ROLLS {
        return Ok(());
    }
    if table_rows(vm, 0).unwrap_or(0) <= u32::from(JIN_INDEX) {
        return Ok(());
    }
    let t4 = vm.mem.read_u32(monster + monster::T4);
    let t5 = vm.mem.read_u32(monster + monster::T5);
    if t5 <= t4 {
        return Ok(());
    }
    let (Some(rand), Some(new_drop), Some(get_x), Some(get_y), Some(gv)) = (
        vm.symbol(RAND),
        vm.symbol(NEW_DROP_ITEM),
        vm.symbol(GET_X),
        vm.symbol(GET_Y),
        vm.symbol(crate::warehouse::GV),
    ) else {
        return Ok(());
    };
    let map = vm.mem.read_u32(gv + GV_MAP_ITEM);
    if map == 0 {
        return Ok(());
    }
    let x = vm.call(get_x, &[monster])?;
    let y = vm.call(get_y, &[monster])?;
    for _ in 0..rolls {
        let roll = vm.call(rand, &[0, 0xffff])?;
        if roll < t4 || roll >= t5 {
            continue;
        }
        let slot = vm.call(rand, &[0, 3])? as u8;
        // Band 5 keeps the table's own grade: every override char is 0xff,
        // which `NewDropItem` reads with `ldrsb` and then `cmp >= 0`, so a
        // negative value leaves `ItemInfo+0x15c` as `GetItemTableInfo` filled
        // it (4). r3 is kind; the first stack char is index.
        let kind = u32::from(slot);
        let index = u32::from(row);
        let ff = 0xffu32;
        vm.call(new_drop, &[map, x, y, kind, index, ff, ff, ff, ff, ff, ff, ff])?;
    }
    Ok(())
}

/// Same bosses and `[T4, T5)` as [`extra_endgame_rolls`]. Each hell endgame set
/// gets its own walk; the lower ten share one. Declines unless every armour
/// table has the highest set row.
fn extra_set_rolls(vm: &mut Vm, monster: u32) -> Result<(), String> {
    let group = vm.mem.read_u8(monster + monster::GROUP) as i8;
    let rolls = vm.mem.read_u8(monster + monster::ROLLS);
    if !BOSS_GROUPS.contains(&group) || rolls < BOSS_ROLLS {
        return Ok(());
    }
    let need = u32::from(*HELL_SET_ROWS.iter().max().expect("non-empty"));
    for kind in SET_KINDS {
        if table_rows(vm, kind).unwrap_or(0) <= need {
            return Ok(());
        }
    }
    let t4 = vm.mem.read_u32(monster + monster::T4);
    let t5 = vm.mem.read_u32(monster + monster::T5);
    if t5 <= t4 {
        return Ok(());
    }
    let (Some(rand), Some(new_drop), Some(get_x), Some(get_y), Some(gv)) = (
        vm.symbol(RAND),
        vm.symbol(NEW_DROP_ITEM),
        vm.symbol(GET_X),
        vm.symbol(GET_Y),
        vm.symbol(crate::warehouse::GV),
    ) else {
        return Ok(());
    };
    let map = vm.mem.read_u32(gv + GV_MAP_ITEM);
    if map == 0 {
        return Ok(());
    }
    let x = vm.call(get_x, &[monster])?;
    let y = vm.call(get_y, &[monster])?;
    let drop = |vm: &mut Vm, index: u8, slot: usize| -> Result<(), String> {
        let kind = u32::from(SET_KINDS[slot]);
        let ff = 0xffu32;
        vm.call(
            new_drop,
            &[map, x, y, kind, u32::from(index), ff, ff, ff, ff, ff, ff, ff],
        )?;
        Ok(())
    };
    for &index in &HELL_SET_ROWS {
        for _ in 0..rolls {
            let roll = vm.call(rand, &[0, 0xffff])?;
            if roll < t4 || roll >= t5 {
                continue;
            }
            let slot = vm.call(rand, &[0, 3])? as usize;
            drop(vm, index, slot)?;
        }
    }
    let last = (LOWER_SET_ROWS.len() - 1) as u32;
    for _ in 0..rolls {
        let roll = vm.call(rand, &[0, 0xffff])?;
        if roll < t4 || roll >= t5 {
            continue;
        }
        let which = vm.call(rand, &[0, last])? as usize;
        let slot = vm.call(rand, &[0, 3])? as usize;
        drop(vm, LOWER_SET_ROWS[which], slot)?;
    }
    Ok(())
}

pub(crate) fn table_rows(vm: &Vm, kind: u8) -> Option<u32> {
    let gv = vm.symbol(crate::warehouse::GV)?;
    let table = vm.mem.read_u32(gv + GV_ITEM_TABLE);
    if table == 0 {
        return None;
    }
    let ptr = vm.mem.read_u32(table + (u32::from(kind) + KIND_PTR_BASE) * 4);
    if ptr < 4 {
        return None;
    }
    Some(vm.mem.read_u32(ptr - 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One equipment record body: `[u16 id][u8 nlen][name][u32 price]
    /// [u8 dlen][desc][21-byte tail]`.
    fn weapon_body(name: &[u8], stat_a: u16, stat_b: u16, opts: [(u8, u8); 3]) -> Vec<u8> {
        let mut eq = [0u8; EQUIP_BLOCK];
        eq[0..2].copy_from_slice(&15u16.to_le_bytes());
        eq[2..4].copy_from_slice(&335u16.to_le_bytes());
        eq[6..8].copy_from_slice(&stat_a.to_le_bytes());
        eq[8..10].copy_from_slice(&stat_b.to_le_bytes());
        eq[0x0c] = 4;
        eq[0x0d] = 97;
        eq[0x0f] = opts[0].0;
        eq[0x10] = opts[0].1;
        eq[0x11] = opts[1].0;
        eq[0x12] = opts[1].1;
        eq[0x13] = opts[2].0;
        eq[0x14] = opts[2].1;
        let mut b = Vec::new();
        b.extend_from_slice(&200u16.to_le_bytes());
        b.push(name.len() as u8);
        b.extend_from_slice(name);
        b.extend_from_slice(&1u32.to_le_bytes());
        b.push(1);
        b.push(b'?');
        b.extend_from_slice(&eq);
        b
    }

    fn table(bodies: &[Vec<u8>]) -> Vec<u8> {
        let mut v = (bodies.len() as u16).to_le_bytes().to_vec();
        for b in bodies {
            v.extend_from_slice(&(b.len() as u16).to_le_bytes());
            v.extend_from_slice(b);
        }
        v
    }

    #[test]
    fn append_copies_row_80_and_lands_on_eof() {
        let mut bodies: Vec<Vec<u8>> = (0..SHIPPED_WEAPON_ROWS)
            .map(|i| weapon_body(format!("w{i:02}").as_bytes(), 100, 200, [(1, 10), (2, 20), (3, 30)]))
            .collect();
        bodies[SOURCE_INDEX] =
            weapon_body(b"source", 150, 708, [(14, 15), (28, 12), (45, 37)]);
        let src = table(&bodies);
        let out = append_jin_weapon(&src).expect("appends");
        let (n, end) = table_end(&out).expect("walks");
        assert_eq!(n, SHIPPED_WEAPON_ROWS + 1);
        assert_eq!(end, out.len());
        let body = record_body(&out, usize::from(JIN_INDEX)).expect("row 86");
        let nlen = usize::from(body[2]);
        let name = &body[3..3 + nlen];
        assert!(name.starts_with(JIN_PREFIX));
        assert!(name.ends_with(b"source"));
        let eq = &body[body.len() - EQUIP_BLOCK..];
        assert_eq!(u16::from_le_bytes([eq[0], eq[1]]), 15, "sprite stays");
        assert_eq!(u16::from_le_bytes([eq[2], eq[3]]), 335, "palette stays");
        assert_eq!(u16::from_le_bytes([eq[6], eq[7]]), 180);
        assert_eq!(u16::from_le_bytes([eq[8], eq[9]]), 850);
        assert_eq!(eq[0x0c], 4);
        assert_eq!(eq[0x0d], 97);
        assert_eq!(eq[0x0f], 14);
        assert_eq!(eq[0x10], 18);
        assert_eq!(eq[0x11], 28);
        assert_eq!(eq[0x12], 14);
        assert_eq!(eq[0x13], 45);
        assert_eq!(eq[0x14], 44);
        assert!(append_jin_weapon(&out).is_none(), "refuse a table that is already extended");
    }

    #[test]
    fn a_short_or_sloppy_table_is_refused() {
        let one = table(&[weapon_body(b"x", 1, 1, [(0, 0), (0, 0), (0, 0)])]);
        assert!(append_jin_weapon(&one).is_none());
        let mut slop = table(
            &(0..SHIPPED_WEAPON_ROWS)
                .map(|i| weapon_body(&[i as u8], 1, 1, [(0, 0), (0, 0), (0, 0)]))
                .collect::<Vec<_>>(),
        );
        slop.push(0);
        assert!(append_jin_weapon(&slop).is_none());
    }

    fn drop_rec(group: u8, kind: u8, index: u8) -> [u8; DROP_REC] {
        let mut r = [0u8; DROP_REC];
        r[0..2].copy_from_slice(&11u16.to_le_bytes());
        r[2] = group;
        r[9] = kind;
        r[10] = index;
        r
    }

    fn drop_table(groups: u16) -> Vec<u8> {
        let mut v = (groups * 4).to_le_bytes().to_vec();
        for g in 0..groups {
            for s in 0..4u8 {
                v.extend_from_slice(&drop_rec(g as u8, s, 70 + g as u8));
            }
        }
        v
    }

    #[test]
    fn set_row_tables_are_hell_then_lower() {
        assert_eq!(&SET_ROWS[..4], &HELL_SET_ROWS);
        assert_eq!(&SET_ROWS[4..], &LOWER_SET_ROWS);
    }

    #[test]
    fn drop_group_63_is_appended_not_overwritten() {
        let src = drop_table(63);
        assert_eq!(src.len(), 2 + usize::from(SHIPPED_DROP_RECORDS) * DROP_REC);
        let out = append_jin_drop_group(&src, true).expect("appends");
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), SHIPPED_DROP_RECORDS + 4);
        assert_eq!(out.len(), 2 + usize::from(SHIPPED_DROP_RECORDS + 4) * DROP_REC);
        // Existing group 61 (오드 line stand-in) is byte-identical.
        let old = 2 + TEMPLATE_GROUP * 4 * DROP_REC;
        assert_eq!(
            &out[old..old + 4 * DROP_REC],
            &src[old..old + 4 * DROP_REC],
            "template group must not move"
        );
        for slot in 0..4 {
            let rec = &out[2 + (252 + slot) * DROP_REC..2 + (253 + slot) * DROP_REC];
            assert_eq!(rec[2], JIN_GROUP);
            assert_eq!(rec[9], slot as u8);
            assert_eq!(rec[10], JIN_INDEX);
        }
        assert!(append_jin_drop_group(&out, true).is_none());
        assert!(append_set_drop_group(&src).is_none(), "set groups need the jin group in front");
        let with_set = append_set_drop_group(&out).expect("appends set");
        let extra = 4 + 4 * SET_ROWS.len();
        assert_eq!(u16::from_le_bytes([with_set[0], with_set[1]]), SHIPPED_DROP_RECORDS + extra as u16);
        assert_eq!(&with_set[2..out.len()], &out[2..], "jin records stay put");
        for (g, &index) in SET_ROWS.iter().enumerate() {
            for slot in 0..4 {
                let i = 256 + g * 4 + slot;
                let rec = &with_set[2 + i * DROP_REC..2 + (i + 1) * DROP_REC];
                assert_eq!(rec[2], SET_GROUP + g as u8, "group {g} slot {slot}");
                assert_eq!(rec[9], SET_KINDS[slot]);
                assert_eq!(rec[10], index);
            }
        }
        assert!(append_set_drop_group(&with_set).is_none());
    }

    #[test]
    fn warehouse_fence_opens_only_the_jin_weapon_row() {
        assert!(warehouse_accepts_jin(0, JIN_INDEX));
        assert!(warehouse_accepts_jin(3, JIN_INDEX));
        assert!(!warehouse_accepts_jin(4, JIN_INDEX), "armour is already past kind<=3");
        assert!(!warehouse_accepts_jin(0, 80), "shipped hell legendary already passes");
        assert!(!warehouse_accepts_jin(0, 81), "공허 stays refused");
        assert!(!warehouse_accepts_jin(0, 85), "공허 5급 stays refused");
        assert!(!warehouse_accepts_jin(0, 87));
    }

    /// Against the real archive, if it is present: each weapon table grows by
    /// one row, the name is the shipped row 80 plus `(진)`, and groups 63 plus
    /// one group per set are appended.
    #[test]
    fn the_shipped_tables_grow_by_exactly_one_row_and_one_group() {
        let Ok(vfs) = std::fs::read("../payload/data.vfs.mp3") else {
            return;
        };
        for path in WEAPON_PATHS {
            let src = crate::shop::vfs_entry(&vfs, path).expect(path);
            let (n, _) = table_end(src).expect("walks");
            assert_eq!(n, SHIPPED_WEAPON_ROWS, "{path}");
            let out = append_jin_weapon(src).expect(path);
            let (n2, end) = table_end(&out).expect("walks after append");
            assert_eq!(n2, n + 1);
            assert_eq!(end, out.len());
            let src_name = {
                let b = record_body(src, SOURCE_INDEX).unwrap();
                let nlen = usize::from(b[2]);
                b[3..3 + nlen].to_vec()
            };
            let dst = record_body(&out, usize::from(JIN_INDEX)).unwrap();
            let nlen = usize::from(dst[2]);
            let name = &dst[3..3 + nlen];
            assert!(name.starts_with(JIN_PREFIX), "{path}");
            assert!(name.ends_with(&src_name), "{path}");
            let src_eq = record_body(src, SOURCE_INDEX).unwrap();
            let src_eq = &src_eq[src_eq.len() - EQUIP_BLOCK..];
            let dst_eq = &dst[dst.len() - EQUIP_BLOCK..];
            assert_eq!(&dst_eq[0..4], &src_eq[0..4], "{path} sprite/palette");
            assert_eq!(dst_eq[0x0c], 4, "{path} stays 전설");
            assert_eq!(dst_eq[0x0d], 97, "{path} stays lv97");
        }
        let src = crate::shop::vfs_entry(&vfs, DROP_TABLE_PATH).expect("droptable");
        let out = append_jin_drop_group(src, true).expect("drop group");
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), 256);
        assert_eq!(&out[2..src.len()], &src[2..], "existing 252 records stay put");
        let with_set = append_set_drop_group(&out).expect("set groups");
        assert_eq!(
            u16::from_le_bytes([with_set[0], with_set[1]]),
            256 + 4 * SET_ROWS.len() as u16
        );
        assert_eq!(&with_set[2..out.len()], &out[2..], "jin records stay put");
        for slot in 0..4 {
            let rec = &with_set[2 + (256 + slot) * DROP_REC..2 + (257 + slot) * DROP_REC];
            assert_eq!(rec[9], SET_KINDS[slot], "slot {slot}");
            assert_eq!(rec[10], HELL_SET_ROWS[0], "first set is 지크문드");
        }
        let last = 256 + (SET_ROWS.len() - 1) * 4;
        let rec = &with_set[2 + last * DROP_REC..2 + (last + 1) * DROP_REC];
        assert_eq!(rec[10], *SET_ROWS.last().unwrap(), "last set is 가시나무");
    }

    /// `vanilla` (`BRANCHES.md`): group 63 still exists so the set groups keep
    /// their numbers, and it points at [`SOURCE_INDEX`] — the shipped lv97 row
    /// the `(진)` copy was made from, and the row `extra_endgame_rolls` drops
    /// on that line — never at the `(진)` row itself.
    #[test]
    fn the_vanilla_placeholder_group_points_at_the_shipped_row() {
        let src = drop_table(63);
        let out = append_jin_drop_group(&src, false).expect("appends");
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), SHIPPED_DROP_RECORDS + 4);
        for slot in 0..4 {
            let rec = &out[2 + (252 + slot) * DROP_REC..2 + (253 + slot) * DROP_REC];
            assert_eq!(rec[2], JIN_GROUP, "it is still group 63");
            assert_eq!(rec[9], slot as u8, "slot i is weapon kind i");
            assert_eq!(rec[10], SOURCE_INDEX as u8, "the shipped lv97 row");
            assert_ne!(rec[10], JIN_INDEX, "and never the (진) row");
        }
        // The set groups still land at 64.., which is what the placeholder is for.
        let with_set = append_set_drop_group(&out).expect("appends set");
        let first = 2 + 256 * DROP_REC;
        assert_eq!(with_set[first + 2], SET_GROUP);
    }

    /// The content split is **one byte per slot**. Same group, same kinds,
    /// same payload, same record count — there is no room for the two APKs to
    /// drop at different rates, only for them to drop different weapons.
    #[test]
    fn the_two_content_lines_differ_only_in_the_row_byte() {
        let src = drop_table(63);
        let main = append_jin_drop_group(&src, true).expect("main");
        let vanilla = append_jin_drop_group(&src, false).expect("vanilla");
        assert_eq!(main.len(), vanilla.len(), "same size table");
        let diffs: Vec<usize> = (0..main.len()).filter(|&i| main[i] != vanilla[i]).collect();
        assert_eq!(diffs.len(), 4, "one byte per slot, nothing else");
        for (slot, &at) in diffs.iter().enumerate() {
            assert_eq!(at, 2 + (252 + slot) * DROP_REC + 10, "it is the index byte");
            assert_eq!(main[at], JIN_INDEX);
            assert_eq!(vanilla[at], SOURCE_INDEX as u8);
        }
    }

    #[test]
    fn a_drop_with_a_reserved_displayed_slot_needs_a_roll() {
        use crate::shop::empty_option_lines as empty;
        // The draw predicate is kind `<= 0`. 0xFF is how the table reserves
        // the slot; 0 is the other value that prints `+랜덤옵션`.
        assert_eq!(empty(18, [0x20, 0x21, -1]), [2]);
        assert_eq!(empty(5, [0x20, 0x21, 0]), [2]);
        assert!(empty(18, [0x20, 0x21, 0x20]).is_empty(), "already a named option");
        // 영웅 draws three lines too — the door lostdrops opened (§54).
        assert_eq!(empty(3, [0x20, 0x21, -1]), [2]);
        // 전설 has no `MakeItemOption` slot, but the screen still draws three
        // lines, so a 전설 row with a hole would print the phrase as well.
        assert_eq!(empty(4, [0x20, -1, -1]), [1, 2]);
        assert!(empty(4, [0x20, 0x21, 0x22]).is_empty());
        // 레어 / 에픽 draw one / two lines.
        assert_eq!(empty(1, [-1, -1, -1]), [0]);
        assert_eq!(empty(2, [0x20, -1, -1]), [1]);
        assert!(empty(0, [-1, -1, -1]).is_empty(), "노멀 has no option line");
    }

    /// Against the real archive: no shipped drop, through any of the bands
    /// `Monster::SetDropItem` can take, ends with an empty displayed slot —
    /// so the `MakeSocket` net never fires on an original drop. This is the
    /// census the module comment cites; it is what lets the net be universal
    /// without touching a single original drop.
    #[test]
    fn shipped_drop_bands_never_leave_a_displayed_slot_empty() {
        let Ok(vfs) = std::fs::read("../payload/data.vfs.mp3") else {
            return;
        };
        let tables: Vec<&[u8]> = (0..=10)
            .map(|k| {
                let path = format!("c/csv/item_{k:02}.dat");
                crate::shop::vfs_entry(&vfs, &path).expect(&path)
            })
            .collect();
        let dt = crate::shop::vfs_entry(&vfs, DROP_TABLE_PATH).expect("droptable");
        let count = usize::from(u16::from_le_bytes([dt[0], dt[1]]));
        assert_eq!(count, usize::from(SHIPPED_DROP_RECORDS));
        let mut seen = 0;
        for i in 0..count {
            let body = &dt[2 + i * DROP_REC + 2..2 + (i + 1) * DROP_REC];
            // `(kind, index, slot-2 override)` per band, as 0xBCB54 (band 4),
            // 0xBCD60 (band 5) and 0xBCE30 (band 6) build them: bands 4 and 6
            // hand the record's second pair to slot 2, band 5 hands nothing.
            let pair_b = body[3] as i8;
            for (kind, index, k2) in [
                (body[5], body[6], pair_b),
                (body[7], body[8], -1),
                (body[9], body[10], pair_b),
            ] {
                if kind > 10 || index == 0xff {
                    continue;
                }
                let row = record_body(tables[usize::from(kind)], usize::from(index))
                    .unwrap_or_else(|| panic!("record {i}: kind {kind} index {index}"));
                let eq = &row[row.len() - EQUIP_BLOCK..];
                let mut kinds = crate::shop::option_kinds(eq.try_into().unwrap());
                if k2 >= 0 {
                    kinds[2] = k2;
                }
                let empty = crate::shop::empty_option_lines(eq[crate::shop::EQUIP_GRADE_AT], kinds);
                assert!(empty.is_empty(), "record {i}: kind {kind} index {index} would print +랜덤옵션 on {empty:?}");
                seen += 1;
            }
        }
        assert!(seen > 500, "walked only {seen} band results");
    }

    /// Against the real archive: every set row we drop reserves slot 2 with
    /// `0xFF`. That is why `NewDropItem` alone shows `+랜덤옵션`.
    #[test]
    fn shipped_set_rows_reserve_slot_two() {
        let Ok(vfs) = std::fs::read("../payload/data.vfs.mp3") else {
            return;
        };
        for kind in SET_KINDS {
            let path = format!("c/csv/item_{kind:02}.dat");
            let src = crate::shop::vfs_entry(&vfs, &path).expect(&path);
            for &index in &SET_ROWS {
                let body = record_body(src, usize::from(index)).unwrap_or_else(|| {
                    panic!("{path} row {index} missing");
                });
                assert!(
                    body.len() >= EQUIP_BLOCK,
                    "{path}[{index}] shorter than the equipment tail"
                );
                let eq = &body[body.len() - EQUIP_BLOCK..];
                assert!(
                    eq[crate::shop::EQUIP_GRADE_AT] >= 5,
                    "{path}[{index}] grade {} is not 세트",
                    eq[crate::shop::EQUIP_GRADE_AT]
                );
                assert_eq!(
                    eq[crate::shop::OPTION_PAIRS_AT + 4],
                    0xff,
                    "{path}[{index}] slot 2 must be the reserved 0xFF that prints +랜덤옵션"
                );
                assert_eq!(
                    crate::shop::empty_option_lines(
                        eq[crate::shop::EQUIP_GRADE_AT],
                        crate::shop::option_kinds(eq.try_into().unwrap()),
                    ),
                    [2],
                    "{path}[{index}]: slot 2 is the one line the net rolls"
                );
            }
        }
    }
}
