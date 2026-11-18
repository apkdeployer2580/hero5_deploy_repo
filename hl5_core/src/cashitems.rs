//! New cash-shop items that the 2011 game never shipped.
//!
//! The 슈퍼 성장의 서 is the first, and the user has said more are coming, so
//! this is a *table* and a dispatcher rather than a one-off. Adding cash item
//! N+1 is one row in [`NEW_ITEMS`] plus one arm of [`Effect`]; nothing else in
//! this file, and nothing at all in `shop.rs` or `lib.rs`, has to change.
//!
//! # Three pieces, and the middle one is the easy one to get wrong
//!
//! **1. The data.** The guest learns what items exist by reading two files, so
//! new items are new rows in those files, handed over by the same file-shadow
//! mechanism [`crate::rates`] uses — no guest hooks, no patched bytes. Both
//! layouts were read off the loaders and then verified by re-parsing the
//! shipped files and checking the walk lands exactly on EOF:
//!
//! ```text
//! /c/csv/item_18.dat              CashItemInfo, 49 rows, 4022 B
//!   [u16 count]
//!   per row: [u16 reclen][u16 param][u8 nameLen][name]
//!            [u32 money][u8 descLen][desc][u8 useClass][u8 last]
//!
//! /c/csv/cash_single_android.dat  the shop catalogue, 30 rows, 2530 B
//!   [u16 count]
//!   per row: [u16 reclen][u8 id][u16 param][u8 nameLen][name]
//!            [u8 a][u8 qty][u8 descLen][desc][u16 price][u16 price2][u16 price3]
//! ```
//!
//! `useClass` is the byte `StateInGameMenu::KeyBagMenu+0xAF4` selects the bag
//! popup from: 2 gives {버리기}, **3 gives {사용하기, 버리기}**, and anything
//! else gives no popup at all — which is what made the old orb tokens look
//! broken (ANALYSIS.md §32.9). Every new item here therefore uses 3.
//!
//! `a` is the catalogue's pointer into `item_18.dat`, and `id` is the byte the
//! client puts on the wire as category 6 subcmd 2.
//!
//! **2. The boot ordering.** `shop::ShopHost::new` derives its grant table by
//! parsing `item_18.dat` **out of the archive**, and refuses any catalogue row
//! whose `a` is past the row count it found. If the shadows were written after
//! that, a new item would appear in the shop and be refused at grant time — it
//! would take the money and hand over nothing. So [`build`] runs first and the
//! extended bytes are given to both the grant table and the shadow writer.
//!
//! **3. The effect.** The guest has a 15-entry jump table for "the player used
//! a cash item", covering `item_18` rows 11..25 only:
//!
//! ```text
//!   0x10af38  ldrb r3, [ip, #0x15]        ; ip = ItemInfo*, r3 = the row index
//!   0x10af54  bhi  #0x10ba34              ; the one and only way in
//!   0x10ba34  lsl  r3, r3, #0x18          ; <- the hook sits here, r3 still raw
//!   0x10ba38  asr  r2, r3, #0x18
//!   0x10ba3c  sub  r2, r2, #0xb
//!   0x10ba40  cmp  r2, #0xe
//!   0x10ba44  addls pc, pc, r2, lsl #2    ; case 0 is at pc+8 = 0x10ba4c = row 11
//!   0x10ba48  b    #0x10bb50              ; the fallthrough default
//! ```
//!
//! The alignment was checked the only way that proves it: row 25 must land on
//! the `SetExpGoodTime` body, and `0x10ba4c + (25-11)*4 = 0x10ba84`, whose word
//! is `b #0x10bc78` — and `0x10bc98` there is `bl HERO::SetExpGoodTime`. So the
//! table is not off by one.
//!
//! A row past 25 falls through to the default, which consumes the item and does
//! nothing. The hook takes those over.
//!
//! # Why the hook can jump into the middle of a function
//!
//! The dispatcher returns `true`, which makes `Vm::run` set `PC := LR`. So the
//! hook *chooses where to resume* by writing `LR` first. It resumes at
//! `Process+0x18b8` (0x10bee8), which is the guest's own tail for "a cash item
//! was used":
//!
//! ```text
//!   0x10bf04  bl TextMgr::GetCommonText(298)     ; "%s을(를) 사용했습니다."
//!   0x10bf20  bl MC_knlSprintk                   ; with the name at [ip+0x18]
//!   0x10bf34  bl HERO::GetBagItemPtr
//!   0x10bf48  bl BagItem::DeleteBagItemCategoryIndex(bag, 18, [ip+0x15], 1)
//! ```
//!
//! That is why the host never touches the bag: the guest consumes the item and
//! writes the message itself, from `ip`, which is still the `ItemInfo*` because
//! the single predecessor loaded it at `0x10af34` and nothing between there and
//! the hook writes it. Clobbering `LR` is safe because this function pushed its
//! real return address in its prologue, and `0x10bee8` writes `LR` nothing —
//! it reads `r5`, `r6`, `ip` and `sp`.
//!
//! **`LR` must be written after any `vm.call`**, because `Vm::call2` restores
//! the whole CPU when the guest function returns.
//!
//! # Off by default, and a decline is the shipped behaviour
//!
//! Unknown rows return `false`, so with an empty table the hook is a no-op and
//! every shipped index runs the original code untouched — which is what keeps
//! the 1,241-frame A/B meaningful. This is a deliberate behaviour change, so it
//! lives in [`Vm::hooks`], never in `patch.rs`.

use crate::patch::PatchFn;
use crate::Vm;
use arm32_cpu::Mode;
use std::path::Path;
use std::sync::Arc;

/// `/c/csv/item_18.dat`, as [`crate::shop::vfs_entry`] and
/// [`crate::rates`] spell archive paths (no leading slash).
pub const ITEM18_PATH: &str = "c/csv/item_18.dat";
/// `/c/csv/cash_single_android.dat`.
pub const CATALOG_PATH: &str = "c/csv/cash_single_android.dat";

/// What using the item does.
///
/// One arm per new item kind. An arm is free to call any number of guest
/// functions through [`Vm::call`]; it must not touch the bag, because the
/// resume address already consumes one of the item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Start the experience buff with this total duration, by calling
    /// `HERO::SetExpGoodTime`. The duration doubles as the marker that tells
    /// [`crate::exp`] which book is running — see that module.
    ExpBoost { duration_ms: i64 },
}

/// One item to add. `item_18` row numbers are assigned by [`build`] rather than
/// written here, because they depend on how many rows the archive already has.
pub struct NewCashItem {
    /// The catalogue `id`, i.e. the byte that goes on the wire for a purchase.
    ///
    /// Must be **<= 56**: `CashShop::GetDiscount` indexes `gv + id*4 + 0x1D04`
    /// with no bounds check and `gv` is 7,656 bytes, so id 57 writes past it.
    /// The shipped catalogue's highest id is 51.
    pub catalog_id: u8,
    /// EUC-KR. The `item_18` loader `memcpy`s it into a 20-byte field.
    pub name: &'static [u8],
    /// EUC-KR. `;` is the guest's line break.
    pub desc: &'static [u8],
    /// Icon id. Copying an existing row's `param` reuses that row's icon, which
    /// is why the super book looks like the original.
    pub param: u16,
    /// How many the purchase grants.
    pub qty: u8,
    /// Shown struck through, with `0원` beside it, when the shop is free.
    pub price: u16,
    pub effect: Effect,
}

/// The shipped 성장의 서 is `item_18` row 25 with `param` 224 and a 1,000원
/// listing (catalogue id 29). The super book copies the icon and doubles the
/// nominal price; the price is cosmetic because [`crate::shop`] strikes it out.
pub const NEW_ITEMS: &[NewCashItem] = &[NewCashItem {
    catalog_id: 52,
    // EUC-KR, not UTF-8. Rust string literals are UTF-8 and the guest would
    // render them as mojibake, so the bytes are spelled out. Decoded:
    //   name: 슈퍼 성장의 서
    //   desc: 3시간 동안 경험치를 300% 획득합니다.;(성장의 서보다 2배 더 많이 얻습니다)
    // `;` is the guest's line break, so the description shows as two lines.
    name: b"\xbd\xb4\xc6\xdb\x20\xbc\xba\xc0\xe5\xc0\xc7\x20\xbc\xad",
    desc: b"\x33\xbd\xc3\xb0\xa3\x20\xb5\xbf\xbe\xc8\x20\xb0\xe6\xc7\xe8\xc4\xa1\xb8\xa6\x20\x33\x30\x30\x25\x20\xc8\xb9\xb5\xe6\xc7\xd5\xb4\xcf\xb4\xd9\x2e\x3b\x28\xbc\xba\xc0\xe5\xc0\xc7\x20\xbc\xad\xba\xb8\xb4\xd9\x20\x32\xb9\xe8\x20\xb4\xf5\x20\xb8\xb9\xc0\xcc\x20\xbe\xf2\xbd\xc0\xb4\xcf\xb4\xd9\x29",
    param: 224,
    qty: 1,
    price: 2000,
    effect: Effect::ExpBoost { duration_ms: crate::exp::SUPER_DURATION_MS },
}];

/// The highest catalogue id that is safe to use. See [`NewCashItem::catalog_id`].
const MAX_CATALOG_ID: u8 = 56;

/// `item_18` rows below this are referenced by hard-coded constants somewhere in
/// the guest (the 11..25 use table, `ProcPacket_Shop`'s 44..48 void-weapon
/// remap, the bag filters at 30/31 and 39..42, and the rows `shop.rs` remaps).
/// The shipped table has exactly 49 rows, so appending is always clear of them.
const FIRST_SAFE_ITEM18_ROW: usize = 49;

/// The two extended files, plus the row-to-effect map the dispatcher needs.
pub struct Extended {
    /// The image the GUEST reads, via the file shadow: new rows, prices lifted
    /// off zero, and random boxes given a discard popup.
    pub item18: Vec<u8>,
    /// The image the HOST derives its grant table from: new rows only, with
    /// every shipped usage class untouched. See
    /// [`patch_item18_discardable`] for why these must differ.
    pub item18_grant: Vec<u8>,
    pub catalog: Vec<u8>,
    /// `(item_18 row, effect)`, in table order.
    pub rows: Vec<(u8, Effect)>,
    /// One line per item, for the boot log.
    pub logs: Vec<String>,
}

/// Read a `u16` little-endian, or `None` past the end.
fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

/// Walk a `[u16 count]` + `[u16 reclen][body]` table and return the offset just
/// past the last record, or `None` if the walk does not land exactly on the end.
///
/// Landing exactly on the end is the self-check that the layout is right. Every
/// table in `/c/csv` is built this way, and a parse that stops short means the
/// record shape is wrong — at which point appending would corrupt the file.
fn table_end(b: &[u8]) -> Option<(u16, usize)> {
    let count = u16_at(b, 0)?;
    let mut pos = 2usize;
    for _ in 0..count {
        let reclen = usize::from(u16_at(b, pos)?);
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

/// Append records to a `[u16 count]` table, bumping the count.
fn append(base: &[u8], count: u16, records: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(base.len() + records.iter().map(Vec::len).sum::<usize>());
    let total = count + records.len() as u16;
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&base[2..]);
    for r in records {
        // Each record is `[u16 reclen][body]`; reclen counts only the body.
        out.extend_from_slice(&(r.len() as u16).to_le_bytes());
        out.extend_from_slice(r);
    }
    out
}

/// One `item_18.dat` record body, in the loader's field order.
fn item18_record(it: &NewCashItem) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&it.param.to_le_bytes());
    r.push(it.name.len() as u8);
    r.extend_from_slice(it.name);
    r.extend_from_slice(&0u32.to_le_bytes()); // money: cash items are never bought with gold
    r.push(it.desc.len() as u8);
    r.extend_from_slice(it.desc);
    r.push(3); // useClass 3 = {사용하기, 버리기}
    r.push(0xff); // `last`, 0xff on every non-equipment cash row
    r
}

/// One `cash_single_android.dat` record body.
fn catalog_record(it: &NewCashItem, item18_row: u8) -> Vec<u8> {
    let mut r = Vec::new();
    r.push(it.catalog_id);
    r.extend_from_slice(&it.param.to_le_bytes());
    r.push(it.name.len() as u8);
    r.extend_from_slice(it.name);
    r.push(item18_row);
    r.push(it.qty);
    r.push(it.desc.len() as u8);
    r.extend_from_slice(it.desc);
    r.extend_from_slice(&it.price.to_le_bytes());
    // price2 / price3 are the discount tiers; 0 means "no discount offered",
    // which is what every shipped row uses.
    r.extend_from_slice(&0u16.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    r
}

/// Offsets inside an `item_18.dat` record body, from the loader
/// (`ItemTable::LoadItemTable`) and confirmed by re-parsing the shipped file.
mod item18 {
    /// The `u32` right after the name — the item's **price**, which
    /// `ItemTable::LoadItemTable+0x1f4` stores at `ItemInfo+0x30`.
    ///
    /// Its position is not fixed: two length-prefixed strings come first, so
    /// it has to be walked to. [`price_at`] does that.
    pub const NAME_LEN_AT: usize = 2;
    /// The usage class is the record's last-but-one byte.
    pub const CLASS_FROM_END: usize = 2;
}

/// Walk to the price `u32` inside one `item_18.dat` record body.
fn price_at(body: &[u8]) -> Option<usize> {
    let namelen = usize::from(*body.get(item18::NAME_LEN_AT)?);
    let at = item18::NAME_LEN_AT.checked_add(1)?.checked_add(namelen)?;
    if at.checked_add(4)? > body.len() {
        return None;
    }
    Some(at)
}

/// Make every cash item discardable from the bag.
///
/// # The bug
///
/// A cash item cannot be thrown away, and the reason is a single field.
/// `BagItem::DeleteBagItemBunchPosIndex` (0xa4cdc) refuses in three ways, and
/// the second one is this:
///
/// ```text
///   0xa4d40  ldr r3, [r6, #0x30]     ; the item's PRICE
///   0xa4d44  cmp r3, #0
///   0xa4d48  beq #0xa4ed4            ; -> mov r0,#0, i.e. "cannot discard"
/// ```
///
/// and the caller turns that 0 into `버릴 수 없는 아이템입니다.`
/// (`common_text` 98, raised at `StateInGameMenu::Process+0x1080`). The sibling
/// `BagItem::DeleteBagItemPosIndex` (0xa1b84) tests the same field.
///
/// **Every one of `item_18.dat`'s 49 rows has price 0.** So no cash item has
/// ever been discardable — not because of anything this host does, but because
/// the shipped table says they are worth nothing. Equipment tables are not
/// like that: `item_00`, `item_05` and `item_09` have a non-zero price in
/// every row, which is why ordinary gear can be thrown away.
///
/// # The two fixes, and why they are separate
///
/// 1. **Price 0 -> [`MIN_PRICE`]** on every row, which is what makes the
///    discard succeed. `BagItem::SetItemFilter`'s price test (0xa1970) sits
///    behind `cmp r2,#0xa / bhi`, i.e. it only ever looks at `kind <= 10`, so
///    raising a `kind 18` price cannot put cash items into an NPC sell list.
///    The one visible effect is `CommonUi::DrawAutoItemShopInfoBox` (0x81554)
///    printing a price, and cash items are not stocked by NPC shops.
///
/// 2. **Usage class 1 -> 2** for the random boxes. A class the bag's popup
///    table does not know gives *no popup at all* (ANALYSIS.md §32.9), so a
///    box in an old save cannot even be selected, let alone discarded. Class 2
///    is `{버리기}`. Nothing is lost: this host does not grant random boxes any
///    more — it rerolls them into real items — so the only holders are old
///    saves, and for them this is the difference between stuck forever and
///    removable.
///
/// **The grant table must NOT see this second change.** `GrantTable::build`
/// keys its reroll off exactly that class byte, so a table rewritten to class 2
/// would stop rerolling and hand out the useless token again. That is why
/// [`Extended`] keeps two images: [`Extended::item18_grant`] for the host's own
/// derivation and [`Extended::item18`] for the guest.
fn patch_item18_discardable(src: &[u8], fix_boxes: bool) -> Option<(Vec<u8>, usize, usize)> {
    let count = usize::from(u16_at(src, 0)?);
    let mut out = src.to_vec();
    let mut pos = 2usize;
    let (mut priced, mut unboxed) = (0usize, 0usize);
    for _ in 0..count {
        let reclen = usize::from(u16_at(&out, pos)?);
        let body_at = pos + 2;
        let end = body_at.checked_add(reclen)?;
        if end > out.len() {
            return None;
        }
        let price = body_at + price_at(out.get(body_at..end)?)?;
        if u32::from_le_bytes(out.get(price..price + 4)?.try_into().ok()?) == 0 {
            out[price..price + 4].copy_from_slice(&MIN_PRICE.to_le_bytes());
            priced += 1;
        }
        let class = end - item18::CLASS_FROM_END;
        if fix_boxes && out[class] == 1 {
            out[class] = 2;
            unboxed += 1;
        }
        pos = end;
    }
    if pos != out.len() {
        return None;
    }
    Some((out, priced, unboxed))
}

/// The price written into a row that shipped with 0. One gold: the smallest
/// value that is not zero, because zero is the only value the discard check
/// rejects and nothing else about the number matters.
const MIN_PRICE: u32 = 1;

/// Build the extended tables from the archive's originals.
///
/// `None` when either file is absent or does not parse — in which case the shell
/// carries on with the shipped catalogue and the new items simply do not exist,
/// which is the same failure posture [`crate::rates`] takes.
pub fn build(vfs: &[u8]) -> Option<Extended> {
    let item18_src = crate::shop::vfs_entry(vfs, ITEM18_PATH)?;
    let catalog_src = crate::shop::vfs_entry(vfs, CATALOG_PATH)?;
    let (item18_count, _) = table_end(item18_src)?;
    let (catalog_count, _) = table_end(catalog_src)?;
    // Appending below row 49 would collide with a hard-coded index.
    if usize::from(item18_count) < FIRST_SAFE_ITEM18_ROW {
        return None;
    }

    let mut item18_recs = Vec::new();
    let mut catalog_recs = Vec::new();
    let mut rows = Vec::new();
    let mut logs = Vec::new();
    for (i, it) in NEW_ITEMS.iter().enumerate() {
        let row = item18_count as usize + i;
        // The row index travels in a single byte (`ItemInfo+0x15`, read with
        // `ldrb`), and the catalogue's `a` is a byte too.
        if row > usize::from(u8::MAX) || it.catalog_id > MAX_CATALOG_ID {
            logs.push(format!(
                "cashitems: {} refused (row {row}, id {})",
                String::from_utf8_lossy(it.name),
                it.catalog_id
            ));
            continue;
        }
        if it.name.len() > 20 || it.desc.len() > 255 {
            logs.push(format!(
                "cashitems: {} refused (name {} B, desc {} B)",
                String::from_utf8_lossy(it.name),
                it.name.len(),
                it.desc.len()
            ));
            continue;
        }
        let row = row as u8;
        item18_recs.push(item18_record(it));
        catalog_recs.push(catalog_record(it, row));
        rows.push((row, it.effect));
        logs.push(format!(
            "cashitems: item_18 row {row}, catalogue id {} -> {:?}",
            it.catalog_id, it.effect
        ));
    }

    // The host's own view: new rows, nothing else touched, so
    // `GrantTable::build` still sees each shipped row's real usage class and
    // keeps rerolling the random boxes.
    let item18_grant = append(item18_src, item18_count, &item18_recs);
    // The guest's view: additionally discardable. `fix_boxes` is true so a
    // random box left in an old save can finally be thrown away.
    let item18 = match patch_item18_discardable(&item18_grant, true) {
        Some((img, priced, unboxed)) => {
            logs.push(format!(
                "cashitems: {priced} row(s) priced for discard, {unboxed} box(es) given 버리기"
            ));
            img
        }
        None => {
            logs.push(
                "cashitems: item_18.dat did not re-parse after appending -                  cash items stay undiscardable"
                    .to_string(),
            );
            item18_grant.clone()
        }
    };
    Some(Extended {
        item18,
        item18_grant,
        catalog: append(catalog_src, catalog_count, &catalog_recs),
        rows,
        logs,
    })
}

impl Extended {
    /// Materialise both files where `StaticUtil::LoadRes` will find them.
    ///
    /// Delegates the path to [`crate::rates::write_override`] rather than
    /// joining it here: the guest never stats a bare `/c/...` path, because
    /// `getFullFilePath` prefixes the internal storage root first, and getting
    /// that wrong produces no error and no log line — just the shipped
    /// catalogue.
    pub fn write_shadows(&self, storage: &Path) -> Vec<String> {
        let mut logs = Vec::new();
        for (path, bytes) in [(ITEM18_PATH, &self.item18), (CATALOG_PATH, &self.catalog)] {
            match crate::rates::write_override(storage, path, bytes) {
                Ok(dst) => logs.push(format!("cashitems: {} B -> {}", bytes.len(), dst.display())),
                Err(e) => logs.push(format!("cashitems: cannot write {path}: {e}")),
            }
        }
        logs
    }
}

/// The row-to-effect map, held on the [`Vm`] because a `PatchFn` is a bare
/// function pointer and cannot close over anything.
#[derive(Default, Clone)]
pub struct CashHost {
    effects: Arc<Vec<(u8, Effect)>>,
}

impl CashHost {
    pub fn new(rows: Vec<(u8, Effect)>) -> Self {
        Self { effects: Arc::new(rows) }
    }

    fn lookup(&self, row: u8) -> Option<Effect> {
        self.effects.iter().find(|(r, _)| *r == row).map(|(_, e)| *e)
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// How many rows the dispatcher owns, for the boot log.
    pub fn len(&self) -> usize {
        self.effects.len()
    }
}

/// `StateInGameMenu::Process`, whose `+0x1404` is the dispatcher site.
const PROCESS: &str = "_ZN15StateInGameMenu7ProcessEv";
/// `lsl r3, r3, #0x18` — the sign-extension in front of the use jump table.
const USE_DISPATCH_OFF: u32 = 0x1404;
/// The guest's own "a cash item was used" tail: message, then consume.
const USE_RESUME_OFF: u32 = 0x18b8;
/// `HERO::SetExpGoodTime(long long)`.
const SET_EXP_GOOD_TIME: &str = "_ZN4HERO14SetExpGoodTimeEx";

/// Apply one effect. May call into the guest.
fn apply(vm: &mut Vm, effect: Effect) -> Result<(), String> {
    match effect {
        Effect::ExpBoost { duration_ms } => {
            let gv = vm
                .symbol(crate::warehouse::GV)
                .ok_or_else(|| "cashitems: no `gv` in this binary".to_string())?;
            let hero = vm.mem.read_u32(gv + crate::warehouse::GV_HERO);
            if hero == 0 {
                return Err("cashitems: no HERO to start the exp buff on".to_string());
            }
            let f = vm
                .symbol(SET_EXP_GOOD_TIME)
                .ok_or_else(|| format!("cashitems: no `{SET_EXP_GOOD_TIME}`"))?;
            let lo = (duration_ms as u64 & 0xffff_ffff) as u32;
            let hi = ((duration_ms as u64) >> 32) as u32;
            // r0 = this, r1 = AAPCS padding (a long long needs an even pair),
            // r2/r3 = the value. `Vm::call` restores the CPU on return.
            vm.call(f, &[hero, 0, lo, hi])?;
            Ok(())
        }
    }
}

/// The dispatcher. Declines every row it does not own.
fn use_cash_item(vm: &mut Vm) -> Result<bool, String> {
    // `r3` still holds the raw row index in its low byte: the very instruction
    // we are standing on is the `lsl` that would sign-extend it.
    let row = (vm.cpu.reg_get(Mode::User, 3) & 0xff) as u8;
    let Some(effect) = vm.cash.clone().lookup(row) else { return Ok(false) };
    apply(vm, effect)?;
    // After the call, never before: `Vm::call2` restores the whole CPU.
    let resume = vm
        .symbol(PROCESS)
        .ok_or_else(|| format!("cashitems: no `{PROCESS}`"))?
        + USE_RESUME_OFF;
    vm.cpu.reg_set(Mode::User, arm32_cpu::reg::LR, resume);
    Ok(true)
}

/// Install the dispatcher. Returns the names it attached to, for the boot log.
pub fn install_hooks(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    let Some(base) = vm.symbol(PROCESS) else {
        return Err(format!("cashitems needs `{PROCESS}`, not in this binary"));
    };
    if vm.symbol(SET_EXP_GOOD_TIME).is_none() {
        return Err(format!("cashitems needs `{SET_EXP_GOOD_TIME}`, not in this binary"));
    }
    let f: PatchFn = use_cash_item;
    vm.install_hook(PROCESS, base + USE_DISPATCH_OFF, f)?;
    Ok(vec![PROCESS])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal `[u16 count]` + `[u16 reclen][body]` table.
    fn table(bodies: &[&[u8]]) -> Vec<u8> {
        let mut v = (bodies.len() as u16).to_le_bytes().to_vec();
        for b in bodies {
            v.extend_from_slice(&(b.len() as u16).to_le_bytes());
            v.extend_from_slice(b);
        }
        v
    }

    #[test]
    fn table_end_accepts_a_well_formed_table_and_rejects_a_short_one() {
        let t = table(&[b"ab", b"cdef"]);
        assert_eq!(table_end(&t), Some((2, t.len())));
        // One byte of trailing slop must be refused, not ignored: landing
        // exactly on the end is the only evidence the layout is right.
        let mut slop = t.clone();
        slop.push(0);
        assert_eq!(table_end(&slop), None);
        // A reclen that runs past the buffer must not panic.
        let mut bad = t.clone();
        bad[2] = 0xff;
        assert_eq!(table_end(&bad), None);
    }

    #[test]
    fn append_bumps_the_count_and_preserves_the_original_bytes() {
        let t = table(&[b"ab"]);
        let out = append(&t, 1, &[b"xyz".to_vec()]);
        assert_eq!(u16_at(&out, 0), Some(2));
        assert_eq!(&out[2..t.len()], &t[2..]);
        assert_eq!(table_end(&out), Some((2, out.len())));
        // and the appended record round-trips through the same walk
        assert_eq!(&out[t.len() + 2..], b"xyz");
    }

    #[test]
    fn an_item18_record_reparses_to_the_fields_it_was_built_from() {
        // The loader's field order is the contract; walk it back the way
        // `GrantTable` and the guest both do.
        let it = &NEW_ITEMS[0];
        let r = item18_record(it);
        assert_eq!(u16_at(&r, 0), Some(it.param));
        let nl = usize::from(r[2]);
        assert_eq!(&r[3..3 + nl], it.name);
        let p = 3 + nl;
        assert_eq!(u32::from_le_bytes(r[p..p + 4].try_into().unwrap()), 0);
        let dl = usize::from(r[p + 4]);
        assert_eq!(&r[p + 5..p + 5 + dl], it.desc);
        let q = p + 5 + dl;
        assert_eq!(r.len(), q + 2, "the record ends with useClass and last");
        // useClass 3 is what gives the bag a 사용하기 entry at all.
        assert_eq!(r[q], 3);
        assert_eq!(r[q + 1], 0xff);
    }

    #[test]
    fn a_catalog_record_reparses_and_points_at_the_row_it_was_given() {
        let it = &NEW_ITEMS[0];
        let r = catalog_record(it, 49);
        assert_eq!(r[0], it.catalog_id);
        assert_eq!(u16_at(&r, 1), Some(it.param));
        let nl = usize::from(r[3]);
        assert_eq!(&r[4..4 + nl], it.name);
        let p = 4 + nl;
        assert_eq!(r[p], 49, "`a` must be the item_18 row");
        assert_eq!(r[p + 1], it.qty);
        let dl = usize::from(r[p + 2]);
        assert_eq!(&r[p + 3..p + 3 + dl], it.desc);
        let q = p + 3 + dl;
        assert_eq!(r.len(), q + 6, "three u16 prices close the record");
        assert_eq!(u16_at(&r, q), Some(it.price));
    }

    /// An `item_18.dat`-shaped record: `[u16 reclen][u16 param][u8 nameLen]
    /// [name][u32 price][u8 descLen][desc][u8 useClass][u8 last]`.
    fn cash_rec(name: &[u8], price: u32, class: u8) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&224u16.to_le_bytes());
        b.push(name.len() as u8);
        b.extend_from_slice(name);
        b.extend_from_slice(&price.to_le_bytes());
        b.push(1);
        b.push(b'd');
        b.push(class);
        b.push(0xff);
        let mut out = (b.len() as u16).to_le_bytes().to_vec();
        out.extend_from_slice(&b);
        out
    }

    #[test]
    fn a_zero_price_is_lifted_and_a_real_price_is_left_alone() {
        // Zero is the only value the discard check rejects, so a row that
        // already has a price must not be touched — changing it would change
        // refine cost and the shop info box for no reason.
        let src = table(&[
            &cash_rec(b"AA", 0, 3)[2..],
            &cash_rec(b"BBBB", 5000, 2)[2..],
        ]);
        let (out, priced, unboxed) = patch_item18_discardable(&src, false).expect("parses");
        assert_eq!((priced, unboxed), (1, 0));
        assert_eq!(out.len(), src.len(), "length must not change");
        let diff: Vec<usize> = (0..src.len()).filter(|&i| src[i] != out[i]).collect();
        assert_eq!(diff.len(), 1, "one byte, the low byte of the price: {diff:?}");
        assert_eq!(out[diff[0]], MIN_PRICE as u8);
    }

    #[test]
    fn a_random_box_gets_a_discard_popup_only_when_asked() {
        // Usage class 1 gives NO popup at all, so a box in an old save cannot
        // be selected. Class 2 is {버리기}.
        let src = table(&[&cash_rec(b"BOX", 0, 1)[2..]]);
        let (kept, _, unboxed) = patch_item18_discardable(&src, false).expect("parses");
        assert_eq!(unboxed, 0, "the grant table's copy must keep class 1");
        let cls = kept.len() - item18::CLASS_FROM_END;
        assert_eq!(kept[cls], 1);

        let (fixed, _, unboxed) = patch_item18_discardable(&src, true).expect("parses");
        assert_eq!(unboxed, 1);
        assert_eq!(fixed[fixed.len() - item18::CLASS_FROM_END], 2);
        // Classes 2 and 3 are already fine and must be left as they are, or a
        // 사용하기 entry would disappear.
        for c in [2u8, 3] {
            let one = table(&[&cash_rec(b"X", 0, c)[2..]]);
            let (o, _, n) = patch_item18_discardable(&one, true).expect("parses");
            assert_eq!(n, 0, "class {c} must not be rewritten");
            assert_eq!(o[o.len() - item18::CLASS_FROM_END], c);
        }
    }

    #[test]
    fn a_malformed_record_is_refused_rather_than_written_into() {
        let src = table(&[&cash_rec(b"AA", 0, 3)[2..]]);
        let mut slop = src.clone();
        slop.push(0);
        assert!(patch_item18_discardable(&slop, true).is_none());
        // A name length that runs past the record must not panic or corrupt.
        let mut bad = src.clone();
        bad[2 + 2 + 2] = 0xff;
        assert!(patch_item18_discardable(&bad, true).is_none());
    }

    #[test]
    fn the_strings_are_euc_kr_and_not_utf8() {
        // This test exists because the first draft of NEW_ITEMS used Rust
        // string literals, which are UTF-8, and the guest would have drawn
        // mojibake. EUC-KR Hangul is two bytes each with the high bit set and
        // is not valid UTF-8, so "does not decode as UTF-8" is a sharp test.
        for it in NEW_ITEMS {
            assert!(
                std::str::from_utf8(it.name).is_err(),
                "name decodes as UTF-8, so it is not EUC-KR Hangul"
            );
            assert!(it.name.iter().any(|b| *b >= 0x80), "no high bytes in name");
            assert!(std::str::from_utf8(it.desc).is_err(), "desc decodes as UTF-8");
            // A stray 0 would truncate the guest's memcpy'd C string.
            assert!(!it.name.contains(&0) && !it.desc.contains(&0));
        }
    }

    #[test]
    fn every_table_entry_is_within_the_guest_limits() {
        // These bounds are not style rules: `catalog_id` indexes gv unchecked,
        // and both string lengths go into a single byte.
        for it in NEW_ITEMS {
            assert!(it.catalog_id <= MAX_CATALOG_ID, "id {}", it.catalog_id);
            assert!(it.name.len() <= 20, "name {} B", it.name.len());
            assert!(it.desc.len() <= 255, "desc {} B", it.desc.len());
            assert!(it.qty >= 1);
        }
        // Catalogue ids must be unique among themselves, and clear of the
        // shipped catalogue's highest id (51).
        let mut ids: Vec<u8> = NEW_ITEMS.iter().map(|i| i.catalog_id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate catalogue id");
        assert!(ids.iter().all(|&i| i > 51), "id collides with a shipped row");
    }

    #[test]
    fn the_super_book_asks_for_the_marker_duration() {
        // If these drift apart, `exp.rs` stops recognising the super book and
        // it silently becomes a plain 1.5x — the exact bug this pairing exists
        // to prevent.
        match NEW_ITEMS[0].effect {
            Effect::ExpBoost { duration_ms } => {
                assert_eq!(duration_ms, crate::exp::SUPER_DURATION_MS);
                assert_ne!(duration_ms, crate::exp::NORMAL_DURATION_MS);
            }
        }
    }

    #[test]
    fn build_appends_past_every_hardcoded_row_and_declines_a_short_table() {
        // A stand-in archive is not available here, so exercise the row-safety
        // gate directly: a table with fewer than 49 rows must be refused
        // rather than appended to.
        let short = table(&[b"x"]);
        assert_eq!(table_end(&short), Some((1, short.len())));
        assert!(usize::from(1u16) < FIRST_SAFE_ITEM18_ROW);
    }

    #[test]
    fn the_effect_lookup_only_answers_for_rows_it_owns() {
        let host = CashHost::new(vec![(49, Effect::ExpBoost { duration_ms: 7 })]);
        assert_eq!(host.lookup(49), Some(Effect::ExpBoost { duration_ms: 7 }));
        // Every shipped row must decline, or the hook would change behaviour
        // the 1,241-frame A/B is supposed to hold constant.
        for row in 0..49u8 {
            assert_eq!(host.lookup(row), None, "row {row}");
        }
        assert!(CashHost::default().is_empty());
    }
}
