//! The 공용창고 (shared warehouse), host side.
//!
//! # Why this file exists
//!
//! The warehouse is the one menu behind 창고접속 that is *storage*: the guest
//! sends an item away and expects to find it again on a later run, possibly on
//! a different character — that is what "공용" means. Everything else that menu
//! offers is a query (거래소, 우편함, 공지), and a query can be declined with a
//! message. Storage cannot: `localserver.rs` refused `4/6` for exactly this
//! reason — "a warehouse you can put things *into* and that forgets them is
//! worse than one that says it is closed". This module is the thing that had to
//! exist before that refusal could be lifted.
//!
//! # What it stores, and what it deliberately does not understand
//!
//! A slot holds the **`_NET_ITEM_` wire bytes verbatim**, not a parsed item.
//!
//! That is a decision, not laziness. `NETWORK::readNetItem` (0xC8578) parses a
//! record whose long form carries three `i64`s, two `i16`s and a run of bytes
//! whose meanings are equipment stats, socket contents and refine state — none
//! of which a warehouse changes. A store that re-encoded them would have to get
//! every width right to hand back the same item, and a width that is wrong by
//! one silently shifts every field after it (ANALYSIS.md §25 paid for that
//! lesson twice). A store that keeps the bytes hands back exactly what it was
//! given, and its correctness does not depend on understanding them.
//!
//! What this module therefore knows about an item is only what the *framing*
//! needs: a record begins with an `i32` count, and a count of zero or less ends
//! it (`readNetItem+0x34`). So an empty slot is four bytes, and a full slot is
//! however many the guest sent.
//!
//! # The file
//!
//! Text, one directive per line, next to the guest's own save under
//! [`crate::Config::storage`]. Text because this is a file a person will end up
//! reading when an item goes missing, and hex because the payload is opaque by
//! construction:
//!
//! ```text
//! # hl5 warehouse v1
//! capacity 16
//! money 0
//! slot 3 state 0 item 0000000112034142
//! ```
//!
//! Slots not named by a `slot` line are empty. Writing is
//! whole-file-then-rename so a crash between the two cannot leave a half-file
//! where a warehouse used to be.
//!
//! # Pages six to ten live in a second file
//!
//! With [`crate::Config::ten_pages`] the guest is offered 160 slots, not 80.
//! Slots 80..159 go to `hl5_warehouse_upper.txt`, never to this file, and this
//! file never says more than `capacity 80`. The reason is the build before
//! this one: it reads `hl5_warehouse.txt`, drops any `slot` line past 80 as
//! "past capacity", and on the next deposit writes the file back without
//! them. One file would make reinstalling an older APK *delete* pages six to
//! ten; two files make it *hide* them until a ten-page build reads both again.
//!
//! # The two files commit as one
//!
//! 아이템정리 moves items *between* the files, so a save that writes them
//! one after the other can stop in between with an item in neither (upper
//! written first) or in both (lower first). So a save has one commit point,
//! the rename of the lower file, and both files carry the `generation` of the
//! save that wrote them:
//!
//! 1. the upper file is written to `hl5_warehouse_upper.new`, not over the
//!    real one — nothing has changed yet;
//! 2. the lower file is renamed into place — **this is the commit**;
//! 3. the staged upper file is renamed over the real one.
//!
//! Loading finishes or forgets what it finds: a staged file whose generation
//! is the lower file's is a save that committed and stopped before step 3, so
//! it becomes the upper file; any other staged file is a save that never
//! committed, and is deleted. Either way the two files describe the same
//! warehouse. An older build ignores the `generation` line and writes the lower
//! file without one, so a staged file next to such a file was never committed
//! by anything this one can name, and is dropped.

use std::path::{Path, PathBuf};

/// The file, under [`crate::Config::storage`]: slots 0..79 and the money.
const FILE: &str = "hl5_warehouse.txt";
/// Slots 80..159 — see the module docs for why they are not in [`FILE`].
const FILE_UPPER: &str = "hl5_warehouse_upper.txt";
/// [`FILE_UPPER`] as a save that has not committed yet wants it — see "the
/// two files commit as one" above.
const FILE_UPPER_NEXT: &str = "hl5_warehouse_upper.new";

/// Slots a warehouse starts with: **five pages**, the most the grid can show.
///
/// `StateNetMenu::DrawStorageUI` lays the grid out sixteen to a page and draws
/// at most five page tabs, so 80 is both the ceiling and the whole thing. It is
/// also the floor, on purpose: a private warehouse has nobody to sell capacity
/// to, and a capacity *below* 16 draws an empty grid however many records
/// arrive — the failure this constant exists to avoid.
pub const DEFAULT_CAPACITY: i32 = 80;

/// The most [`FILE`] ever holds, and the most the shipped grid can page
/// through: five tabs of sixteen.
pub const MAX_CAPACITY: i32 = 80;

/// The most both files together hold: ten pages (`crate::tenpages`).
pub const MAX_TOTAL: i32 = 160;

/// An empty `_NET_ITEM_` on the wire: the leading count, zero, and nothing
/// after it. `readNetItem` returns at `cmp r0, #0; ble` before reading the kind.
pub const EMPTY_ITEM: [u8; 4] = [0, 0, 0, 0];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slot {
    /// The `u8` the listing frame carries ahead of the item and hands to
    /// `NetStorageItem::NewNetStorageItem` as its `char` argument. Stored
    /// rather than derived for the same reason the item bytes are.
    pub state: u8,
    /// `_NET_ITEM_` wire bytes, or empty for a free slot.
    pub item: Vec<u8>,
}

impl Slot {
    pub fn is_empty(&self) -> bool {
        self.item.is_empty()
    }

    /// The bytes to put on the wire for this slot's item.
    pub fn item_bytes(&self) -> &[u8] {
        if self.item.is_empty() {
            &EMPTY_ITEM
        } else {
            &self.item
        }
    }
}

/// One installation's warehouse.
///
/// Not one per save slot: 공용 means the three characters share it, and the
/// guest never tells the server which slot it is playing, so there is nothing
/// to key on even if we wanted to.
#[derive(Debug)]
pub struct Warehouse {
    path: PathBuf,
    /// Every slot either file holds, at least [`Warehouse::capacity`] long.
    /// Slots past the offered capacity are kept, never offered — see
    /// [`Warehouse::load_offering`].
    slots: Vec<Slot>,
    money: i32,
    /// What the guest is told the warehouse holds: the listing sends exactly
    /// this many records and the guest takes the count as its capacity, so
    /// the two cannot disagree.
    offered: usize,
    /// The last committed save's number; the next save writes one more.
    generation: u64,
}

impl Warehouse {
    /// Load, or start an empty one. A malformed line is skipped with a note
    /// rather than dropping the whole file: losing one slot to a bad line is
    /// bad, losing eighty is worse.
    pub fn load(storage: &Path) -> (Self, Vec<String>) {
        Self::load_offering(storage, DEFAULT_CAPACITY)
    }

    /// Load both files and offer the guest `offered` slots (80, or 160 with
    /// ten pages).
    ///
    /// Whatever the files hold is kept whatever `offered` is: a ten-page
    /// warehouse opened by a five-page run still has its upper pages after
    /// that run saves, and [`Warehouse::compact`] moves them down into any
    /// free slot below 80.
    pub fn load_offering(storage: &Path, offered: i32) -> (Self, Vec<String>) {
        let offered = offered.clamp(1, MAX_TOTAL) as usize;
        let path = storage.join(FILE);
        let mut logs = Vec::new();
        let mut wh = Self {
            path: path.clone(),
            slots: vec![Slot::default(); offered.max(DEFAULT_CAPACITY as usize)],
            money: 0,
            offered,
            generation: 0,
        };
        let mut pending: Vec<(usize, Slot)> = Vec::new();
        let mut lower_gen = None;
        if let Ok(text) = std::fs::read_to_string(&path) {
            lower_gen = wh.parse(&text, FILE, MAX_CAPACITY, &mut pending, &mut logs);
        }
        let (upper_path, next_path) = (storage.join(FILE_UPPER), storage.join(FILE_UPPER_NEXT));
        let mut upper = std::fs::read_to_string(&upper_path).ok();
        if let Ok(next) = std::fs::read_to_string(&next_path) {
            if lower_gen.is_some() && generation_of(&next) == lower_gen {
                // Committed, then stopped before the rename: finish it.
                match std::fs::rename(&next_path, &upper_path) {
                    Ok(()) => logs.push(format!(
                        "warehouse: finished an interrupted save (generation {})",
                        lower_gen.unwrap_or(0)
                    )),
                    Err(e) => logs.push(format!(
                        "warehouse: {} is the committed upper file but could not be renamed ({e}); using it",
                        next_path.display()
                    )),
                }
                upper = Some(next);
            } else {
                // Staged by a save whose lower file never landed.
                let _ = std::fs::remove_file(&next_path);
                logs.push(format!(
                    "warehouse: dropped a save that never committed (staged generation {:?}, lower file {:?})",
                    generation_of(&next),
                    lower_gen
                ));
            }
        }
        let mut upper_gen = None;
        if let Some(text) = upper {
            upper_gen = wh.parse(&text, FILE_UPPER, MAX_TOTAL, &mut pending, &mut logs);
        }
        wh.generation = lower_gen.unwrap_or(0).max(upper_gen.unwrap_or(0));
        // Applied after both files so a `capacity` line anywhere still sizes
        // the vector the slots go into.
        for (i, slot) in pending {
            if i >= MAX_TOTAL as usize {
                logs.push(format!("warehouse: slot {i} is past {MAX_TOTAL}; dropped"));
                continue;
            }
            if i >= wh.slots.len() {
                wh.slots.resize(i + 1, Slot::default());
            }
            wh.slots[i] = slot;
        }
        let hidden = wh.slots[wh.offered.min(wh.slots.len())..].iter().filter(|s| !s.is_empty()).count();
        logs.push(format!(
            "warehouse: {} loaded — {}/{} slots used, {} money{}",
            path.display(),
            wh.used(),
            wh.capacity(),
            wh.money,
            if hidden > 0 {
                format!(", {hidden} more on pages this run does not offer (kept)")
            } else {
                String::new()
            }
        ));
        (wh, logs)
    }

    /// Read one file into `self` and `pending`; returns its `generation`.
    fn parse(
        &mut self,
        text: &str,
        file: &str,
        max: i32,
        pending: &mut Vec<(usize, Slot)>,
        logs: &mut Vec<String>,
    ) -> Option<u64> {
        let mut generation = None;
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.as_slice() {
                // Clamped up to `DEFAULT_CAPACITY`, never down. A file written
                // by an older build says `capacity 16`; growing it appends
                // empty slots and costs nothing, while honouring a smaller
                // number would hide pages a player may already have filled.
                // Shrinking is the one operation that could strand an item, so
                // it is not offered here at all.
                ["capacity", v] => match v.parse::<i32>() {
                    Ok(c) if (1..=max).contains(&c) => {
                        let want = (c.max(DEFAULT_CAPACITY) as usize).max(self.slots.len());
                        self.slots.resize(want, Slot::default());
                    }
                    _ => logs.push(format!("warehouse: {file} line {}: bad capacity {v:?}", n + 1)),
                },
                ["money", v] => match v.parse::<i32>() {
                    Ok(m) if m >= 0 => self.money = m,
                    _ => logs.push(format!("warehouse: {file} line {}: bad money {v:?}", n + 1)),
                },
                ["generation", v] => match v.parse::<u64>() {
                    Ok(g) => generation = Some(g),
                    _ => logs.push(format!("warehouse: {file} line {}: bad generation {v:?}", n + 1)),
                },
                ["slot", i, "state", s, "item", hex] => {
                    match (i.parse::<usize>(), s.parse::<u8>(), unhex(hex)) {
                        (Ok(i), Ok(state), Some(item)) => pending.push((i, Slot { state, item })),
                        _ => logs.push(format!("warehouse: {file} line {}: bad slot record", n + 1)),
                    }
                }
                _ => logs.push(format!("warehouse: {file} line {}: unrecognised {line:?}", n + 1)),
            }
        }
        generation
    }

    /// Write the files as one save — see "the two files commit as one" in the
    /// module docs. `Err` means nothing committed: the files still describe
    /// the warehouse as the last successful save left it.
    pub fn save(&mut self) -> Result<(), String> {
        let generation = self.generation + 1;
        let lower = self.slots.len().min(MAX_CAPACITY as usize);
        let mut out = String::from("# hl5 warehouse v1 — see hl5_core/src/warehouse.rs\n");
        out.push_str(&format!("capacity {lower}\n"));
        out.push_str(&format!("money {}\n", self.money));
        out.push_str(&format!("generation {generation}\n"));
        for (i, s) in self.slots[..lower].iter().enumerate() {
            if !s.is_empty() {
                out.push_str(&format!("slot {} state {} item {}\n", i, s.state, tohex(&s.item)));
            }
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let next = self.path.with_file_name(FILE_UPPER_NEXT);
        let staged = self.slots.len() > lower;
        if staged {
            let mut up = String::from(
                "# hl5 warehouse pages 6-10 (slots 80..159) — see hl5_core/src/warehouse.rs\n",
            );
            up.push_str(&format!("capacity {}\n", self.slots.len()));
            up.push_str(&format!("generation {generation}\n"));
            for (i, s) in self.slots.iter().enumerate().skip(lower) {
                if !s.is_empty() {
                    up.push_str(&format!("slot {} state {} item {}\n", i, s.state, tohex(&s.item)));
                }
            }
            write_atomically(&next, &up)?;
        }
        if let Err(e) = write_atomically(&self.path, &out) {
            if staged {
                // Not needed for safety — a load drops it — but tidy.
                let _ = std::fs::remove_file(&next);
            }
            return Err(e);
        }
        // Committed. If the rename below fails, the next load finishes it.
        self.generation = generation;
        if staged {
            let _ = std::fs::rename(&next, self.path.with_file_name(FILE_UPPER));
        }
        Ok(())
    }

    /// What the guest is offered — the listing's record count.
    pub fn capacity(&self) -> i32 {
        self.offered as i32
    }

    pub fn money(&self) -> i32 {
        self.money
    }

    pub fn set_money(&mut self, v: i32) {
        self.money = v.max(0);
    }

    /// The offered slots, in order — what the listing sends.
    pub fn slots(&self) -> &[Slot] {
        &self.slots[..self.offered]
    }

    /// Items in the offered slots.
    pub fn used(&self) -> usize {
        self.slots().iter().filter(|s| !s.is_empty()).count()
    }

    /// Put an item in the lowest free slot. `None` when the warehouse is full —
    /// the caller must then decline the deposit, because the guest deletes the
    /// bag item as soon as the reply says the deposit succeeded
    /// (`ProcPacket_Inventory+0xf8` calls `BagItem::DeleteBagItemCategoryIndex`
    /// on `result == 0`). Answering "yes" with nowhere to put it destroys the
    /// item, which is the exact failure this whole module exists to prevent.
    pub fn deposit(&mut self, state: u8, item: Vec<u8>) -> Option<usize> {
        let i = self.slots[..self.offered].iter().position(Slot::is_empty)?;
        self.slots[i] = Slot { state, item };
        Some(i)
    }

    /// Take an item out. `None` for an index that is out of range or already
    /// empty — the caller declines, and nothing is created out of nothing.
    pub fn withdraw(&mut self, index: usize) -> Option<Slot> {
        if index >= self.offered {
            return None;
        }
        let s = self.slots.get_mut(index)?;
        if s.is_empty() {
            return None;
        }
        Some(std::mem::take(s))
    }

    /// Take `want` of a stacked item out, leaving the rest.
    ///
    /// The only field of the record this module reads: its leading `i32` is
    /// the stack count, in both directions of the wire
    /// (`readNetItem+0x28` reads it first; `writeNetItem+0x14` writes the
    /// caller's count first). Rewriting exactly that word is what lets a
    /// player take five of a stack of twenty without this module knowing what
    /// the other fifteen are.
    ///
    /// Returns the bytes to send back, or `None` for an empty or out-of-range
    /// slot. `want <= 0`, or more than there is, takes the whole stack: the
    /// guest computes the count itself and the alternative — silently
    /// returning less than the player asked for — loses items.
    pub fn withdraw_count(&mut self, index: usize, want: i32) -> Option<Vec<u8>> {
        let held = self.stack_count(index)?;
        if want <= 0 || want >= held {
            return self.withdraw(index).map(|s| s.item);
        }
        let slot = self.slots.get_mut(index)?;
        let mut taken = slot.item.clone();
        write_count(&mut taken, want);
        write_count(&mut slot.item, held - want);
        Some(taken)
    }

    /// The stack count of the record in `index`, or `None` if the slot is
    /// empty, out of range, or holds fewer than the four bytes a count needs.
    pub fn stack_count(&self, index: usize) -> Option<i32> {
        if index >= self.offered {
            return None;
        }
        let s = self.slots.get(index)?;
        if s.is_empty() || s.item.len() < 4 {
            return None;
        }
        Some(i32::from_be_bytes(s.item[..4].try_into().expect("4")))
    }

    /// 아이템정리: move every item to the front, order preserved.
    ///
    /// The guest asks for this with `4/9` and then re-requests the listing, so
    /// the whole feature is "renumber the slots and say yes". It runs over
    /// **every** slot the files hold, offered or not, so a five-page run
    /// tidying a ten-page warehouse pulls the upper pages down into what it
    /// can show rather than stranding them.
    pub fn compact(&mut self) -> usize {
        let mut items: Vec<Slot> =
            self.slots.iter_mut().filter(|s| !s.is_empty()).map(std::mem::take).collect();
        let moved = items.len();
        items.resize(self.slots.len(), Slot::default());
        self.slots = items;
        moved
    }

    /// Move `amount` into the warehouse (negative takes it out).
    ///
    /// `false` means the host declined and the caller must answer `result != 0`
    /// — the guest only adjusts the carried money on a success reply
    /// (`ProcPacket_Inventory+0x548`), so a decline leaves both sides agreeing.
    pub fn move_money(&mut self, amount: i32) -> bool {
        match self.money.checked_add(amount) {
            Some(v) if v >= 0 => {
                self.money = v;
                true
            }
            _ => false,
        }
    }

    /// Grow the warehouse (창고 확장). Clamped to [`MAX_CAPACITY`]; returns the
    /// new capacity.
    pub fn expand(&mut self, by: i32) -> i32 {
        let ceiling = (self.slots.len() as i32).max(MAX_CAPACITY);
        let want = (self.capacity() + by.max(0)).clamp(1, ceiling) as usize;
        self.offered = self.offered.max(want);
        self.capacity()
    }
}

/// Whole-file-then-rename, so an interrupted save leaves the previous file.
/// The data is flushed before the rename and, where the platform allows, the
/// directory after it: the rename is the warehouse's commit point, and a
/// rename that reaches the disk ahead of the bytes it names is an empty file.
fn write_atomically(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    let err = |e: std::io::Error| format!("{}: {e}", tmp.display());
    let mut f = std::fs::File::create(&tmp).map_err(err)?;
    f.write_all(text.as_bytes()).map_err(err)?;
    f.sync_all().map_err(err)?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

/// The `generation` a file names, if it names one.
fn generation_of(text: &str) -> Option<u64> {
    text.lines().find_map(|l| match l.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["generation", v] => v.parse().ok(),
        _ => None,
    })
}

/// Overwrite the record's leading stack count in place.
fn write_count(item: &mut [u8], n: i32) {
    if item.len() >= 4 {
        item[..4].copy_from_slice(&n.to_be_bytes());
    }
}

fn tohex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

// ---- the guest side: hiding the one control that cannot work offline --------
//
// The 싱글가방 pane has a coin pile at its bottom-left corner, and pressing it
// opens 거래용 화폐 환전 — move game money from the purse into the warehouse's
// own currency. `4/2` implements it and works, so this is not a missing
// feature. It is a **one-way door**:
//
// * the guest has no path that moves 거래용 화폐 back out. Every category-4
//   money frame is `4/2` (purse -> warehouse) plus `4/7` (report the balance);
//   there is no fourth arm and no UI that asks for one.
// * what 거래용 화폐 is *for* is the 거래소, and the 거래소 needs somebody on
//   the other side. Offline it stays shut (§32.7).
//
// So offline the button can only take money and never give it back, and the
// thing it buys is unreachable. A control like that is worse than a missing
// one: it looks like a feature and costs the player their gold. Two hooks
// remove it — the icon and the dialog — and nothing else about the screen
// changes.
//
// Only when the warehouse is the in-process one. With a real server configured
// the exchange means what it always meant, and the icon comes back.

use crate::patch::PatchFn;
use crate::Vm;
use arm32_cpu::{reg, Mode};

/// `Graphic::DrawImage(fb, x, y, GbmImage*, frame)` — the general sprite blit,
/// hooked only to decline one call.
const DRAW_IMAGE: &str = "_ZN7Graphic9DrawImageEiiiP8GbmImagei";

/// The pane chrome, and the coin icon's return address inside it (0x132DA0
/// against 0x132C70). The call is
/// `DrawImage(GetFB(), origin.x + 0xd, origin.y + 0xb5, gv+0x16a0, 12)` — the
/// bottom-left corner of the panel, which is where the icon is on screen.
const DRAW_STORAGE: &str = "_ZN12StateNetMenu11DrawStorageEv";
const COIN_RET: u32 = 0x132da0 - 0x132c70;

/// The guest's global block, and the two fields the refund reads out of it —
/// both taken from the guest's own use of them in `ProcPacket_Inventory`'s
/// deposit arm (0xCADA4..0xCADC0), which is the code that saves the bag after a
/// deposit on this very screen.
pub(crate) const GV: &str = "gv";
pub(crate) const GV_HERO: u32 = 0x1474;
const GV_SAVE_SLOT: u32 = 0x1970;

/// `HERO::GetBagItemPtr()` — `add r0, r0, #0x17c0; bx lr`. Called rather than
/// open-coded so the offset stays the guest's business.
const GET_BAG: &str = "_ZN4HERO13GetBagItemPtrEv";

/// `BagItem::IncreaseMoney(int)` — the guest's own credit path. The refund goes
/// through this rather than writing the purse directly, which is the rule a
/// behaviour change has to keep if the alignment A/B is to stay possible
/// (ANALYSIS.md §16.10).
const INCREASE_MONEY: &str = "_ZN7BagItem13IncreaseMoneyEi";

/// `HERO::SaveBagData(char)` — same call the deposit path makes, with the same
/// two arguments.
const SAVE_BAG: &str = "_ZN4HERO11SaveBagDataEa";

/// Every popup this screen raises goes through here, and the id is the first
/// real argument (r1 — r0 is `this`).
const SET_POPUP_STATE: &str = "_ZN12StateNetMenu13SetPopupStateEaPciiiiiP8ItemInfo";

/// 거래용 화폐 환전. Identified from the call that raises it
/// (`TouchStorageUI+0x50c`): it seeds the dialog with `BagItem::GetMoney()` as
/// the maximum, which is what an exchange dialog needs and no other popup on
/// this screen asks for.
///
/// **Exactly two sites in `.text` raise it**, and both are this screen: the
/// icon's tap at 0x12E3B4 (`TouchStorageUI`) and a key path at 0x12D798
/// (`KeyStorage`). Every `SetPopupState` call site was swept for
/// `mov r1, #0x1e` to establish that, which is why keying on the id is safe —
/// it cannot swallow a dialog some other screen wanted.
const POPUP_EXCHANGE: u32 = 0x1e;

/// Install both hooks. Names are checked first so a binary without them says
/// so instead of silently doing nothing.
pub fn install_hooks(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for name in [
        DRAW_IMAGE, DRAW_STORAGE, SET_POPUP_STATE, GV, GET_BAG, INCREASE_MONEY, SAVE_BAG,
    ] {
        if vm.symbol(name).is_none() {
            return Err(format!("warehouse hook needs `{name}`, not in this binary"));
        }
    }
    // `DRAW_IMAGE` is not installed here: the ten-page tabs want a call site
    // in it too, so `Vm::boot` installs one dispatcher that offers it to
    // [`hide_coin_icon`] first.
    let sites: [(&'static str, PatchFn); 2] = [
        (SET_POPUP_STATE, refuse_exchange_popup),
        (DRAW_STORAGE, refund_trade_currency),
    ];
    let mut installed = Vec::new();
    for (name, f) in sites {
        let addr = vm.symbol(name).expect("checked above");
        vm.install_hook(name, addr, f)?;
        installed.push(name);
    }
    Ok(installed)
}

/// Decline exactly the one `DrawImage` that draws the coin.
///
/// `Graphic::DrawImage` is the sprite blit the whole game uses, so the return
/// address is the entire test: anything else falls through to the guest's own
/// code, which is what `Ok(false)` means.
pub(crate) fn hide_coin_icon(vm: &mut Vm) -> Result<bool, String> {
    let Some(site) = vm.symbol(DRAW_STORAGE) else { return Ok(false) };
    Ok(fired_at(vm, site + COIN_RET))
}

/// Swallow the exchange dialog wherever it is raised from.
///
/// Keyed on the popup id rather than on the caller, because the icon is not the
/// only way in: `KeyStorage+0x3cc` reaches the same dialog without going
/// through the touch handler. Returning true makes the call a no-op, so the
/// screen simply does not change.
fn refuse_exchange_popup(vm: &mut Vm) -> Result<bool, String> {
    Ok(vm.arg(1) == POPUP_EXCHANGE)
}

/// Give any 거래용 화폐 balance back to the purse, once, and never take it again.
///
/// # Why this exists
///
/// The exchange is gone (above), but a balance can already be sitting there —
/// this repository put 1,000 of the bench character's gold in during the very
/// session that removed the button, with one stray tap. Offline there is no
/// guest path that moves it back and nothing to spend it on, so leaving it is
/// leaving the player's money in a hole.
///
/// # Why it is a standing rule and not a migration
///
/// With the button removed and no server, the balance can only be non-zero if
/// something put it there before, so "hand it back the next time 창고관리
/// opens" converges to zero and then does nothing for ever. And it cannot
/// interfere with a real account: these hooks are installed only when the
/// warehouse is the in-process one (`Config::offline_menus`), and a configured
/// server keeps both the exchange and its balance.
///
/// # Why here
///
/// `StateNetMenu::DrawStorage` is the storage screen's own painter, so when it
/// runs the slot's character is loaded and its `BagItem` is real — the guest's
/// deposit arm calls `HERO::GetBagItemPtr` and `HERO::SaveBagData` from this
/// same screen, which is the proof rather than an assumption. Everything is
/// done through guest calls, and the hook always declines, so the frame is
/// drawn by the guest exactly as before.
fn refund_trade_currency(vm: &mut Vm) -> Result<bool, String> {
    let owed = {
        let wh = vm.warehouse.clone();
        let money = wh.lock().expect("warehouse lock").money();
        if money <= 0 {
            return Ok(false);
        }
        money
    };
    let (Some(gv), Some(get_bag), Some(increase), Some(save)) =
        (vm.symbol(GV), vm.symbol(GET_BAG), vm.symbol(INCREASE_MONEY), vm.symbol(SAVE_BAG))
    else {
        return Ok(false);
    };
    let hero = vm.mem.read_u32(gv + GV_HERO);
    if hero == 0 {
        // No character loaded: nothing to credit, and the balance stays put
        // until a screen that has one asks for it.
        return Ok(false);
    }
    let slot = vm.mem.read_u8(gv + GV_SAVE_SLOT) as i8 as i32 as u32;
    let bag = vm.call(get_bag, &[hero])?;
    if bag == 0 {
        return Ok(false);
    }
    vm.call(increase, &[bag, owed as u32])?;
    vm.call(save, &[hero, slot])?;
    {
        let wh = vm.warehouse.clone();
        let mut wh = wh.lock().expect("warehouse lock");
        wh.set_money(0);
        if let Err(e) = wh.save() {
            // The purse already has it and the bag is already saved, so the
            // worst case is the same money offered again next time. Say so
            // rather than pretend the file was written.
            vm.log.push(format!("warehouse: refund written to the bag but not the file: {e}"));
        }
    }
    vm.log.push(format!(
        "warehouse: 거래용 화폐 {owed} returned to the purse (no guest path spends it offline)"
    ));
    Ok(false)
}

/// Whether this hook fired from `want` rather than from `Vm::call` (which
/// leaves `RETURN_MAGIC` in `lr`, and is how a hook that re-enters the guest
/// avoids catching itself).
fn fired_at(vm: &Vm, want: u32) -> bool {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    lr != crate::RETURN_MAGIC && lr == want
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hl5-wh-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("tmpdir");
        d
    }

    #[test]
    fn an_empty_warehouse_is_five_full_pages() {
        let (wh, _) = Warehouse::load(&tmp("empty"));
        assert_eq!(wh.capacity(), DEFAULT_CAPACITY);
        assert_eq!(wh.used(), 0);
        assert_eq!(wh.money(), 0);
        // Every slot must still produce a legal record: `readNetItem` reads the
        // count first and stops on zero, so four bytes is a whole empty item.
        assert!(wh.slots().iter().all(|s| s.item_bytes() == EMPTY_ITEM));
    }

    #[test]
    fn deposit_then_withdraw_returns_the_same_bytes() {
        let (mut wh, _) = Warehouse::load(&tmp("roundtrip"));
        let item = vec![0, 0, 0, 1, 12, 3, 0, 0, 0, 2, b'h', b'i'];
        let i = wh.deposit(7, item.clone()).expect("a free slot");
        assert_eq!(wh.used(), 1);
        let back = wh.withdraw(i).expect("the item is there");
        assert_eq!(back.item, item, "the store must not reinterpret the record");
        assert_eq!(back.state, 7);
        assert_eq!(wh.used(), 0);
        assert!(wh.withdraw(i).is_none(), "an emptied slot yields nothing twice");
    }

    #[test]
    fn a_full_warehouse_refuses_rather_than_overwriting() {
        let (mut wh, _) = Warehouse::load(&tmp("full"));
        for n in 0..DEFAULT_CAPACITY {
            assert!(wh.deposit(0, vec![n as u8]).is_some());
        }
        // The guest deletes the bag item the moment the reply says the deposit
        // worked, so "no room" has to be visible *before* the reply is built.
        assert_eq!(wh.deposit(0, vec![99]), None);
        assert_eq!(wh.used(), DEFAULT_CAPACITY as usize);
    }

    #[test]
    fn it_survives_a_round_trip_through_the_file() {
        let dir = tmp("persist");
        let item = vec![0, 0, 0, 3, 18, 44, 0, 0, 0, 1, b'x'];
        {
            let (mut wh, _) = Warehouse::load(&dir);
            wh.deposit(2, item.clone());
            wh.deposit(0, vec![0, 0, 0, 9]);
            wh.set_money(1234);
            wh.save().expect("save");
        }
        let (wh, logs) = Warehouse::load(&dir);
        assert!(logs.iter().all(|l| !l.contains("bad")), "{logs:?}");
        assert_eq!(wh.capacity(), DEFAULT_CAPACITY);
        assert_eq!(wh.money(), 1234);
        assert_eq!(wh.used(), 2);
        assert_eq!(wh.slots()[0].item, item);
        assert_eq!(wh.slots()[0].state, 2);
    }

    /// A file from before the warehouse held five pages.
    #[test]
    fn an_old_sixteen_slot_file_grows_and_keeps_its_items() {
        let dir = tmp("grow");
        std::fs::write(
            dir.join(FILE),
            "capacity 16
money 7
slot 15 state 0 item 00000001020300000000
",
        )
        .expect("write");
        let (wh, logs) = Warehouse::load(&dir);
        assert!(logs.iter().all(|l| !l.contains("bad")), "{logs:?}");
        assert_eq!(wh.capacity(), DEFAULT_CAPACITY, "an old file gains the new pages");
        assert_eq!(wh.money(), 7);
        assert_eq!(wh.used(), 1);
        assert!(!wh.slots()[15].is_empty(), "and keeps what was in the last old slot");
    }

    /// A record as the wire carries it: count, kind, index, name.
    fn rec(count: i32, kind: u8) -> Vec<u8> {
        let mut v = count.to_be_bytes().to_vec();
        v.extend_from_slice(&[kind, 0, 0, 0, 0, 2, b'h', b'i']);
        v
    }

    #[test]
    fn a_partial_withdrawal_splits_the_stack() {
        let (mut wh, _) = Warehouse::load(&tmp("split"));
        wh.deposit(0, rec(20, 18));
        let taken = wh.withdraw_count(0, 5).expect("five of twenty");
        assert_eq!(i32::from_be_bytes(taken[..4].try_into().unwrap()), 5);
        assert_eq!(&taken[4..], &rec(20, 18)[4..], "only the count may differ");
        assert_eq!(wh.stack_count(0), Some(15));
        // Asking for everything (or more) empties the slot rather than
        // leaving a zero-count record behind, which `readNetItem` would read
        // as "no item" and the grid would draw as occupied-but-blank.
        assert_eq!(wh.withdraw_count(0, 99).map(|v| v.len()), Some(12));
        assert!(wh.slots()[0].is_empty());
        assert_eq!(wh.withdraw_count(0, 1), None);
    }

    #[test]
    fn tidying_moves_items_to_the_front() {
        let (mut wh, _) = Warehouse::load(&tmp("compact"));
        wh.deposit(0, rec(1, 18));
        wh.deposit(0, rec(2, 18));
        wh.deposit(0, rec(3, 18));
        wh.withdraw(1);
        assert!(wh.slots()[1].is_empty());
        assert_eq!(wh.compact(), 2);
        assert_eq!(wh.stack_count(0), Some(1));
        assert_eq!(wh.stack_count(1), Some(3), "the gap closes, order survives");
        assert!(wh.slots()[2].is_empty());
    }

    #[test]
    fn money_never_goes_negative_and_says_so() {
        let (mut wh, _) = Warehouse::load(&tmp("money"));
        assert!(wh.move_money(500));
        assert_eq!(wh.money(), 500);
        assert!(wh.move_money(-200));
        assert_eq!(wh.money(), 300);
        // The guest only debits the player's purse when the reply says yes, so
        // refusing here is what keeps the two sides adding up.
        assert!(!wh.move_money(-1000));
        assert_eq!(wh.money(), 300);
        assert!(!wh.move_money(i32::MAX), "and it does not wrap");
    }

    #[test]
    fn capacity_is_clamped_at_both_ends() {
        let (mut wh, _) = Warehouse::load(&tmp("clamp"));
        assert_eq!(wh.capacity(), MAX_CAPACITY, "five pages is already the ceiling");
        assert_eq!(wh.expand(1000), MAX_CAPACITY);
        assert_eq!(wh.expand(-1000), MAX_CAPACITY, "shrinking would strand items");
    }

    /// Ten pages: slots 80.. go to the upper file, and the lower file stays
    /// one an older build reads without dropping anything.
    #[test]
    fn upper_pages_live_in_their_own_file() {
        let dir = tmp("ten");
        {
            let (mut wh, _) = Warehouse::load_offering(&dir, MAX_TOTAL);
            assert_eq!(wh.capacity(), MAX_TOTAL);
            for n in 0..MAX_TOTAL {
                assert_eq!(wh.deposit(0, rec(n + 1, 18)), Some(n as usize));
            }
            assert_eq!(wh.deposit(0, rec(1, 18)), None, "160 is full");
            wh.save().expect("save");
        }
        let lower = std::fs::read_to_string(dir.join(FILE)).expect("lower");
        assert!(lower.contains("capacity 80\n"), "{lower}");
        assert!(!lower.contains("slot 80 "), "nothing past 79 in the old file");
        let upper = std::fs::read_to_string(dir.join(FILE_UPPER)).expect("upper");
        assert!(upper.contains("capacity 160\n") && upper.contains("slot 159 "), "{upper}");

        // What the previous build sees: the old file alone, every line good.
        let old = tmp("ten-old");
        std::fs::copy(dir.join(FILE), old.join(FILE)).expect("copy");
        let (wh, logs) = Warehouse::load(&old);
        assert!(logs.iter().all(|l| !l.contains("bad") && !l.contains("dropped")), "{logs:?}");
        assert_eq!(wh.used(), 80);

        // A ten-page run reads both back.
        let (wh, _) = Warehouse::load_offering(&dir, MAX_TOTAL);
        assert_eq!(wh.used(), 160);
        assert_eq!(wh.stack_count(159), Some(160));
    }

    /// A five-page run over a ten-page warehouse: the upper pages are not
    /// offered, survive its saves, and come down when it tidies.
    #[test]
    fn a_five_page_run_keeps_and_can_recover_the_upper_pages() {
        let dir = tmp("five-over-ten");
        {
            let (mut wh, _) = Warehouse::load_offering(&dir, MAX_TOTAL);
            for n in 0..90 {
                wh.deposit(0, rec(n + 1, 18));
            }
            wh.save().expect("save");
        }
        let (mut wh, logs) = Warehouse::load(&dir);
        assert_eq!(wh.capacity(), DEFAULT_CAPACITY);
        assert_eq!(wh.slots().len(), 80, "the listing is the offered pages only");
        assert!(logs.iter().any(|l| l.contains("10 more on pages")), "{logs:?}");
        assert_eq!(wh.stack_count(85), None, "not offered, not withdrawable");
        // Its own save keeps them.
        wh.withdraw(0);
        wh.save().expect("save");
        let (back, _) = Warehouse::load_offering(&dir, MAX_TOTAL);
        assert_eq!(back.used(), 89);
        assert_eq!(back.stack_count(89), Some(90));
        // And tidying brings one of them into the free slot.
        let (mut wh, _) = Warehouse::load(&dir);
        assert_eq!(wh.compact(), 89);
        assert_eq!(wh.used(), 80);
        assert_eq!(wh.stack_count(79), Some(81));
    }

    /// Eighty-one items, slot 0 taken out, then 아이템정리 moves slot 80 down
    /// into slot 0 — across the files. The lower file's write fails.
    fn tidy_across_the_files(dir: &Path) -> Warehouse {
        let (mut wh, _) = Warehouse::load_offering(dir, MAX_TOTAL);
        for n in 0..81 {
            wh.deposit(0, rec(n + 1, 18));
        }
        wh.save().expect("save");
        wh.withdraw(0).expect("slot 0");
        wh.save().expect("save");
        assert_eq!(wh.compact(), 80);
        wh
    }

    fn counts(wh: &Warehouse) -> Vec<i32> {
        let mut v: Vec<i32> = (0..wh.slots().len()).filter_map(|i| wh.stack_count(i)).collect();
        v.sort_unstable();
        v
    }

    /// The review's case: the upper file used to be written first, so a lower
    /// write that failed after it lost the item 아이템정리 had moved.
    #[test]
    fn a_failed_lower_write_loses_nothing() {
        let dir = tmp("commit-fail");
        let mut wh = tidy_across_the_files(&dir);
        // `write_atomically` cannot create its temporary file over a directory.
        std::fs::create_dir(dir.join(FILE).with_extension("tmp")).expect("inject");
        assert!(wh.save().is_err(), "the lower write must fail");
        let (back, logs) = Warehouse::load_offering(&dir, MAX_TOTAL);
        assert_eq!(back.used(), 80, "{logs:?}");
        assert_eq!(counts(&back), (2..=81).collect::<Vec<_>>(), "the save before the failure, whole");
        assert!(!back.slots()[80].is_empty(), "slot 80 still in the upper file");

        // The same process saves again once the disk lets it.
        std::fs::remove_dir(dir.join(FILE).with_extension("tmp")).expect("un-inject");
        wh.save().expect("save");
        let (back, _) = Warehouse::load_offering(&dir, MAX_TOTAL);
        assert_eq!(counts(&back), (2..=81).collect::<Vec<_>>());
        assert!(back.slots()[..80].iter().all(|s| !s.is_empty()), "tidied");
        assert!(back.slots()[80].is_empty());
    }

    /// A save that committed (lower renamed) and stopped before renaming the
    /// staged upper file. Reading the old upper file with the new lower one
    /// would hold the moved item twice.
    #[test]
    fn an_interrupted_commit_is_finished_by_the_next_load() {
        let dir = tmp("commit-finish");
        let mut wh = tidy_across_the_files(&dir);
        let before = std::fs::read_to_string(dir.join(FILE_UPPER)).expect("upper");
        wh.save().expect("save");
        let after = std::fs::read_to_string(dir.join(FILE_UPPER)).expect("upper");
        assert_ne!(before, after);
        // Put the machine back at the moment between step 2 and step 3.
        std::fs::write(dir.join(FILE_UPPER_NEXT), &after).expect("stage");
        std::fs::write(dir.join(FILE_UPPER), &before).expect("old upper");

        let (back, logs) = Warehouse::load_offering(&dir, MAX_TOTAL);
        assert!(logs.iter().any(|l| l.contains("finished an interrupted save")), "{logs:?}");
        assert_eq!(counts(&back), (2..=81).collect::<Vec<_>>(), "once each");
        assert!(back.slots()[80].is_empty());
        assert!(!dir.join(FILE_UPPER_NEXT).exists());
        assert_eq!(std::fs::read_to_string(dir.join(FILE_UPPER)).expect("upper"), after);
    }

    /// A staged file next to a lower file an older build wrote: that build
    /// knows nothing of generations, so nothing it wrote committed the stage.
    #[test]
    fn a_stage_next_to_an_older_builds_file_is_dropped() {
        let dir = tmp("commit-old");
        let mut wh = tidy_across_the_files(&dir);
        wh.save().expect("save");
        let lower = std::fs::read_to_string(dir.join(FILE)).expect("lower");
        let old: String = lower.lines().filter(|l| !l.starts_with("generation")).map(|l| format!("{l}\n")).collect();
        std::fs::write(dir.join(FILE), old).expect("older build");
        std::fs::write(dir.join(FILE_UPPER_NEXT), "capacity 160\ngeneration 9\nslot 90 state 0 item 00000001\n")
            .expect("stage");
        let (back, logs) = Warehouse::load_offering(&dir, MAX_TOTAL);
        assert!(logs.iter().any(|l| l.contains("never committed")), "{logs:?}");
        assert!(back.slots()[90].is_empty());
        assert!(!dir.join(FILE_UPPER_NEXT).exists());
        assert_eq!(back.used(), 80);
    }

    #[test]
    fn a_bad_line_costs_one_slot_and_not_the_file() {
        let dir = tmp("badline");
        std::fs::write(
            dir.join(FILE),
            "capacity 16\nmoney 5\nslot zzz state 0 item 00\nslot 1 state 0 item 000000ff\n",
        )
        .expect("write");
        let (wh, logs) = Warehouse::load(&dir);
        assert_eq!(wh.money(), 5);
        assert_eq!(wh.used(), 1, "the readable slot survives the unreadable one");
        assert!(logs.iter().any(|l| l.contains("bad slot record")), "{logs:?}");
    }
}