//! 가방·창고 10페이지 — the 싱글가방 and the 공용창고 from five pages of
//! sixteen to ten.
//!
//! ANALYSIS.md §60 is the survey; §65 is this module. The short version:
//!
//! # Why a capacity is not one number here
//!
//! The 2011 server decided how big the warehouse was — `4/6`'s record count is
//! `NetStorageItem::SetSize`'s argument (`ProcPacket_Inventory+0x640`) — and
//! every page count on the screen is `size / 16`. So "send 160 records" sounds
//! like the whole job. It is not, because the object the records land in has
//! room for 80: `StateNetMenu`'s constructor allocates it with `new(0x1E8)` and
//! lays three arrays end to end,
//!
//! ```text
//!   +0x000 i32 money   +0x004 i16 size   +0x006 i16 count
//!   +0x008 ItemInfo*[80]   +0x148 i8 stack[80]   +0x198 i8 state[80]   (0x1E8)
//! ```
//!
//! and `NewNetStorageItem` writes slot `n` at `this[n+2]`, `this+0x148+n` and
//! `this+0x198+n` without checking `n`. Slot 80's pointer is stack counts 0..3.
//!
//! # What changes
//!
//! The object is allocated at `0x3C8` and the two byte arrays move behind a
//! 160-entry pointer array: `+0x008 ptr[160]`, `+0x288 stack[160]`,
//! `+0x328 state[160]`. Every instruction that knows the old layout carries it
//! as an **immediate** — a `mov r0, #0x1e8`, an `ldrb r0, [r1, #0x148]`, a
//! `cmp r4, #0x50` — so the rewrite is a table of single instruction words,
//! each replaced by the same instruction with a different immediate
//! ([`STORAGE`]). [`apply`] reads every word first and writes none of them if
//! one is not what the table says: a different binary gets an error naming the
//! word, never half a layout.
//!
//! The table was built from an immediate census of every `StateNetMenu`,
//! `NETWORK` and `NetStorageItem` function (`tools_re/immscan.py`) and checked
//! against the interpreter: `--memwatch storage` records every PC that loads or
//! stores inside the object (`crate::memwatch`), and a PC that touches a moved
//! array but is not in the table is a hole.
//!
//! # The bag is the same job, larger
//!
//! `BagItem` lives inside `HERO` at `+0x17C0` and holds, end to end, 96
//! pointers, 96 stack counts at `+0x188`, 96 filter-list pointers at `+0x1E8`
//! and the filter length at `+0x368` — with the next `HERO` field right after.
//! The pointer array stays put and grows to 172; the stack counts move behind
//! it (`+0x2B8`) and the filter list leaves for the end of a `HERO` grown from
//! `0x2018/0x2020` to `0x22D0` (`HERO+0x2020`). That is [`BAG`]: 186 words —
//! every stack-count and filter access (found by data flow,
//! `tools_re/bagscan.py` → `bagplan.py`), every bound of 96, the sixteen
//! `HERO` allocations, and `StateInGameMenu::ChangeHeroClass`, which copies the
//! bag into arrays in its own stack frame and gets a bigger frame. The bag
//! shows 160 slots (ten pages); 160..171 are the overflow the cash-shop grant
//! has always been allowed into.
//!
//! The save keeps its format and grows an 80-byte tail with the stack counts
//! of slots 96..171 (see "the bag's save tail" below). The filtered lists the
//! refine/orb/equip screens page through can now pass 127 entries, and their
//! cursors are signed bytes — [`crate::tenlists`] widens them.
//!
//! This is the second place the host writes guest code (the first is
//! [`crate::portrait`]). Same rules: before the first guest instruction, only
//! behind its switch ([`crate::Config::ten_pages`]), all or nothing.
//!
//! # The tabs
//!
//! Four screens — the warehouse, the warehouse's view of the bag, the in-game
//! bag and the NPC sale screen — draw one chest per page, 24 px apart, from
//! `size / 16`; ten would run off the panel. The player asked for the game's
//! own chests in two columns (1–5 left, 6–10 right), with the item grid moved
//! right to make room. The panel is 176 px wide and the column used 13..36 of
//! it before the grid at 44, so the grid moves [`GRID_SHIFT`] = 12 px and the
//! chests sit at 4 and 28 (an open chest is 24 px wide, a closed one 21).
//!
//! * **draw** — the tab loop's three blits (`DrawImage` for the open chest and
//!   the closed ones, `DrawImageBlending` for the closed ones while the cursor
//!   is in the grid) keep their image and frame; a hook moves the blit's `x`
//!   and `y` to the chest's column and row and lets the guest draw it.
//! * **the grid** — every grid `x` the three draw functions compute is an
//!   immediate added to the panel origin (`add r2, r2, #0x2c` …), so [`GRID`]
//!   rewrites 23 of them the same way the layouts above are rewritten. The
//!   three touch handlers hit-test the cells from a 16-entry `.rodata` table
//!   they copy onto their stack; a hook just after the copy moves that copy.
//! * **touch** — `TouchStorageUI`, `TouchBagMenu` and `TouchItemSale` each
//!   hit-test five fixed rects from `.rodata` (`cmp sl, #5`). A press anywhere
//!   in the chest columns is answered here with exactly what that guest loop
//!   does on a tab hit, and never reaches the old rects.
//! * **keys** — up and down already walk `size / 16` tabs in order (1 → 10).
//!   Left and right in the tab column now cross between the two columns;
//!   right from a tab with nothing to its right enters the grid, as before.

use arm32_cpu::{reg, Mode};

use crate::patch::PatchFn;
use crate::{Vm, RETURN_MAGIC};

// ---- the object -------------------------------------------------------------

/// `NetStorageItem` — the 공용창고's contents.
pub const STORAGE_C1: &str = "_ZN14NetStorageItemC1Ev";
pub const STORAGE_C2: &str = "_ZN14NetStorageItemC2Ev";
/// `sizeof(NetStorageItem)` as shipped: money, size, count, 80 pointers, 80
/// stack counts, 80 states.
pub const STORAGE_SIZE_OLD: u32 = 0x1e8;
/// The same with 160 of each: `8 + 160*4 + 160 + 160`.
pub const STORAGE_SIZE_NEW: u32 = 0x3c8;
/// Slots the warehouse offers with the rewrite on — ten pages of sixteen.
pub const STORAGE_SLOTS: i32 = 160;

/// `BagItem` — embedded in `HERO` at `+0x17c0` (`HERO::GetBagItemPtr`).
pub const BAG_C1: &str = "_ZN7BagItemC1Ev";
pub const BAG_C2: &str = "_ZN7BagItemC2Ev";
pub const HERO_BAG: u32 = 0x17c0;
/// `sizeof(BagItem)`: the next `HERO` field is `+0x1B2C`.
pub const BAG_SIZE: u32 = 0x36c;
/// Where the filter array goes when it leaves the object: the end of a
/// `HERO` grown from `0x2020` to `0x22D0`.
pub const BAG_FILTER_NEW: u32 = 0x860;
pub const BAG_FILTER_LEN_NEW: u32 = 172 * 4;

// ---- the rewrite ------------------------------------------------------------

/// One instruction whose immediate changes, and nothing else about it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Imm {
    pub sym: &'static str,
    pub at: u32,
    /// The whole shipped word, checked before anything is written.
    pub was: u32,
    /// Its immediate as shipped — redundant with `was`, and checked against
    /// it, so the table states what it believes the instruction is.
    pub old: u32,
    pub new: u32,
}

const fn imm(sym: &'static str, at: u32, was: u32, old: u32, new: u32) -> Imm {
    Imm { sym, at, was, old, new }
}

const NET_MENU_C1A: &str = "_ZN12StateNetMenuC1Eia";
const NET_MENU_C2A: &str = "_ZN12StateNetMenuC2Eia";
const NET_MENU_C1: &str = "_ZN12StateNetMenuC1Ev";
const NET_MENU_C2: &str = "_ZN12StateNetMenuC2Ev";
const GET_BUNCH: &str = "_ZN14NetStorageItem13GetBunchCountEs";
const GET_STATE: &str = "_ZN14NetStorageItem12GetItemStateEs";
const DELETE_POS: &str = "_ZN14NetStorageItem28DeleteNetStorageItemPosIndexEs";
const DELETE_ALL: &str = "_ZN14NetStorageItem9DeleteAllEv";
const NEW_ITEM: &str = "_ZN14NetStorageItem17NewNetStorageItemEP10_NET_ITEM_ai";
const DRAW_STORAGE_UI: &str = "_ZN12StateNetMenu13DrawStorageUIEaahhh";

/// The warehouse layout, every word that knows it. `tools_re/words.py` prints
/// these from the binary; `tools_re/immscan.py` found them.
pub(crate) const STORAGE: &[Imm] = &[
    // `new(sizeof(NetStorageItem))` in all four `StateNetMenu` constructors.
    imm(NET_MENU_C1A, 0xec, 0xe3a0_0f7a, 0x1e8, 0x3c8),
    imm(NET_MENU_C2A, 0xec, 0xe3a0_0f7a, 0x1e8, 0x3c8),
    imm(NET_MENU_C1, 0xd4, 0xe3a0_0f7a, 0x1e8, 0x3c8),
    imm(NET_MENU_C2, 0xd4, 0xe3a0_0f7a, 0x1e8, 0x3c8),
    // The constructor's two memsets: the pointers, then the stack counts at
    // their new place. (The state bytes were never cleared; they still are
    // not — `NewNetStorageItem` writes one for every slot the listing sends.)
    imm(STORAGE_C1, 0x20, 0xe3a0_2d05, 0x140, 0x280),
    imm(STORAGE_C1, 0x30, 0xe284_0f52, 0x148, 0x288),
    imm(STORAGE_C1, 0x34, 0xe3a0_2050, 0x50, 0xa0),
    imm(STORAGE_C2, 0x20, 0xe3a0_2d05, 0x140, 0x280),
    imm(STORAGE_C2, 0x30, 0xe284_0f52, 0x148, 0x288),
    imm(STORAGE_C2, 0x34, 0xe3a0_2050, 0x50, 0xa0),
    // The two byte getters.
    imm(GET_BUNCH, 0x4, 0xe5d1_0148, 0x148, 0x288),
    imm(GET_STATE, 0x4, 0xe5d1_0198, 0x198, 0x328),
    // Removing one slot, and the loop that removes all of them.
    imm(DELETE_POS, 0xb0, 0xe5c7_3148, 0x148, 0x288),
    imm(DELETE_ALL, 0x20, 0xe354_0050, 0x50, 0xa0),
    // `NewNetStorageItem`'s three exits each store a stack count and a state.
    imm(NEW_ITEM, 0xbc, 0xe5c7_0148, 0x148, 0x288),
    imm(NEW_ITEM, 0xc0, 0xe5c7_0198, 0x198, 0x328),
    imm(NEW_ITEM, 0x13c, 0xe5c7_8198, 0x198, 0x328),
    imm(NEW_ITEM, 0x140, 0xe5c7_2148, 0x148, 0x288),
    imm(NEW_ITEM, 0x2cc, 0xe5c7_8198, 0x198, 0x328),
    imm(NEW_ITEM, 0x2d4, 0xe5c7_2148, 0x148, 0x288),
    // The grid's pointer to the stack counts it prints under each item.
    imm(DRAW_STORAGE_UI, 0x4c, 0x1283_3f52, 0x148, 0x288),
];

// ---- the receive buffer -----------------------------------------------------
//
// Every inbound game frame reaches `NETWORK` in one piece: the midas layer
// collects the whole SmartReq answer in its 0x5000-byte queue
// (`MX_smartConnect`, `_midas_netQueueReadBuffer`), `netLibCB` hands it on once
// the frame's own length word says it is complete, and `NETWORK::receiveData`
// (0xC8438) copies it into a fresh `DM_MALLOC(0x2000 + 4)` — `memcpy(buf + 4,
// frame, len)` with **no check of `len`**. Nothing after that checks either:
// `readNet8/32/Buf` index `buf + 4 + readPos` and never compare it with
// `writeLen`.
//
// At five pages the largest listing `4/6` can carry is 80 equipment records,
// about 7 KB, and the 8 KB body was never reached. At ten pages it is 160 —
// up to 14 KB — and from the 91st equipment item on, the copy runs past the
// block into whatever the best-fit allocator put next to it (ANALYSIS.md
// §65.14). The probe faults in `EquipItemInfo::CopyData` the moment 창고관리
// opens; on a phone the outcome depends on the neighbour, from a crash to
// `ItemInfo`s allocated by the listing itself landing on records not yet read.
//
// So the buffer grows to the midas queue's own size: any frame the transport
// below can deliver, `NETWORK` can now hold. The cursors are `i16`
// (`ldrsh`/`asr #16` in every `readNet*`), so 0x5000 is also well inside what
// they can address.

const RECEIVE_DATA: &str = "_ZN7NETWORK11receiveDataEiiPv";
/// `receiveData+0x8` is `mov r0, #0x2000` and `+0xc` is `add r0, r0, #4`: the
/// buffer's body, then its `[u16 writeLen][u16 readPos]` header.
const RECV_SIZE_AT: u32 = 0x8;
const RECV_HEADER_ADD: u32 = 0xe280_0004;
/// The largest game frame the shipped guest can receive.
pub const RECV_BODY_OLD: u32 = 0x2000;
/// The same with ten pages: the midas read queue's size, which bounds every
/// frame before `receiveData` sees it.
pub const RECV_BODY_NEW: u32 = 0x5000;

/// The one word that sizes the receive buffer.
pub(crate) const RECV: &[Imm] =
    &[imm(RECEIVE_DATA, RECV_SIZE_AT, 0xe3a0_0a02, RECV_BODY_OLD, RECV_BODY_NEW)];

/// The largest game frame the guest can take in one piece **right now**, read
/// off the instruction that sizes its buffer rather than off a switch — so the
/// in-process server's guard (`localserver::Session::set_frame_limit`) follows
/// whatever guest memory actually says. An instruction this does not recognise
/// reads as the shipped size, the cautious answer.
pub fn recv_capacity(vm: &Vm) -> u32 {
    let Some(base) = vm.symbol(RECEIVE_DATA) else { return RECV_BODY_OLD };
    let size = vm.mem.read_u32(base + RECV_SIZE_AT);
    let header = vm.mem.read_u32(base + RECV_SIZE_AT + 4);
    match imm_of(size) {
        // `mov r0, #n`, unconditional, then the header's `add r0, r0, #4`.
        Some(n) if size & 0xffff_f000 == 0xe3a0_0000 && header == RECV_HEADER_ADD => n,
        _ => RECV_BODY_OLD,
    }
}

/// The bag layout, every word that knows it — 186 of them. Generated by
/// `tools_re/bagplan.py` (the stack-count and filter accesses
/// `tools_re/bagscan.py` traces by data flow) plus the bounds, sizes and
/// frames read by hand (ANALYSIS.md §65.3); `tools_re/words.py` prints any of
/// them back from the binary. The comment is the shipped instruction and why
/// it moves.
pub(crate) const BAG: &[Imm] = &[
    imm("_ZN8CommonUi8LoadHeroEa", 0x6c, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN8CommonUi8LoadHeroEa", 0xe4, 0xe2800020, 0x20, 0x2d0), // add r0, r0, #0x20 — HERO alloc
    imm("_ZN8CommonUi8LoadHeroEa", 0x104, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN8CommonUi8LoadHeroEa", 0x124, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN4HERO14GetBagDataSizeEv", 0x50, 0xe2810095, 0x95, 0xe5), // add r0, r1, #0x95 — +80 tail
    imm("_ZN4HERO12LoadHeroDataEa", 0x88, 0xe3540060, 0x60, 0xac), // cmp r4, #0x60 — clear loop
    imm("_ZN4HERO11LoadBagDataEa", 0x88, 0xe3540060, 0x60, 0xac), // cmp r4, #0x60 — clear loop
    imm("_ZN4HERO11LoadBagDataEa", 0x3b0, 0xe2822008, 0x8, 0x138), // add r2, r2, #8 — stack
    imm("_ZN4HERO11SaveBagDataEa", 0x25c, 0xe2822008, 0x8, 0x138), // add r2, r2, #8 — stack
    imm("_ZN4HERO11SaveBagDataEa", 0x3e0, 0xe28b2021, 0x21, 0x71), // add r2, fp, #0x21 — +80 tail
    imm("_ZN7BagItem6IsFullEaas", 0x60, 0xe5d7c188, 0x188, 0x2b8), // ldrb ip, [r7, #0x188] — stack
    imm("_ZN7BagItem26GetBagItemPtrCategoryIndexEaa", 0x24, 0xe3550060, 0x60, 0xac), // cmp r5, #0x60 — loop
    imm("_ZN7BagItem26GetBagItemPtrCategoryIndexEaa", 0x68, 0xe5d76188, 0x188, 0x2b8), // ldrb r6, [r7, #0x188] — stack
    imm("_ZN7BagItem26GetBagItemPtrCategoryIndexEaa", 0x84, 0xe3550060, 0x60, 0xac), // cmp r5, #0x60 — loop
    imm("_ZN7BagItem25GetBagItemTotalBunchCountEaa", 0x18, 0xe3540060, 0x60, 0xac), // cmp r4, #0x60 — loop
    imm("_ZN7BagItem25GetBagItemTotalBunchCountEaa", 0x4c, 0x05d5c188, 0x188, 0x2b8), // ldrbeq ip, [r5, #0x188] — stack
    imm("_ZN7BagItem25GetBagItemTotalBunchCountEaa", 0x5c, 0xe3540060, 0x60, 0xac), // cmp r4, #0x60 — loop
    imm("_ZN7BagItemC1Ev", 0x4, 0xe3a03030, 0x30, 0xa0), // mov r3, #0x30 — size 160
    imm("_ZN7BagItemC1Ev", 0xc, 0xe3a06d06, 0x180, 0x2b0), // mov r6, #0x180 — ptr/filter memset
    imm("_ZN7BagItemC1Ev", 0x34, 0xe3a02060, 0x60, 0xac), // mov r2, #0x60 — stack memset
    imm("_ZN7BagItemC1Ev", 0x38, 0xe2840f62, 0x188, 0x2b8), // add r0, r4, #0x188 — stack addr
    imm("_ZN7BagItemC1Ev", 0x48, 0xe2840f7a, 0x1e8, 0x860), // add r0, r4, #0x1e8 — filter addr
    imm("_ZN7BagItemC2Ev", 0x4, 0xe3a03030, 0x30, 0xa0), // mov r3, #0x30 — size 160
    imm("_ZN7BagItemC2Ev", 0xc, 0xe3a06d06, 0x180, 0x2b0), // mov r6, #0x180 — ptr/filter memset
    imm("_ZN7BagItemC2Ev", 0x34, 0xe3a02060, 0x60, 0xac), // mov r2, #0x60 — stack memset
    imm("_ZN7BagItemC2Ev", 0x38, 0xe2840f62, 0x188, 0x2b8), // add r0, r4, #0x188 — stack addr
    imm("_ZN7BagItemC2Ev", 0x48, 0xe2840f7a, 0x1e8, 0x860), // add r0, r4, #0x1e8 — filter addr
    imm("_ZN7BagItem13SetItemFilterEaa", 0x2c, 0xe59391e8, 0x1e8, 0x860), // ldr sb, [r3, #0x1e8] — filter
    imm("_ZN7BagItem13SetItemFilterEaa", 0x64, 0xe58321e8, 0x1e8, 0x860), // str r2, [r3, #0x1e8] — filter
    imm("_ZN7BagItem13SetItemFilterEaa", 0x68, 0xe3540060, 0x60, 0xac), // cmp r4, #0x60 — loop
    imm("_ZN7BagItem13SetItemFilterEaa", 0x12c, 0xe289207a, 0x7a, 0x218), // add r2, sb, #0x7a — filter
    imm("_ZN7BagItem13SetItemFilterEi", 0x28, 0xe59b81e8, 0x1e8, 0x860), // ldr r8, [fp, #0x1e8] — filter
    imm("_ZN7BagItem13SetItemFilterEi", 0x58, 0xe58b31e8, 0x1e8, 0x860), // str r3, [fp, #0x1e8] — filter
    imm("_ZN7BagItem13SetItemFilterEi", 0x5c, 0xe3540060, 0x60, 0xac), // cmp r4, #0x60 — loop
    imm("_ZN7BagItem13SetItemFilterEi", 0x1a4, 0xe288307a, 0x7a, 0x218), // add r3, r8, #0x7a — filter
    imm("_ZN7BagItem21DeleteBagItemPosIndexEsh", 0x78, 0xd5c32188, 0x188, 0x2b8), // strble r2, [r3, #0x188] — stack
    imm("_ZN7BagItem21DeleteBagItemPosIndexEsh", 0x138, 0xe357005e, 0x5e, 0xaa), // cmp r7, #0x5e — shift guard
    imm("_ZN7BagItem21DeleteBagItemPosIndexEsh", 0x14c, 0xe289cf62, 0x188, 0x2b8), // add ip, sb, #0x188 — stack
    imm("_ZN7BagItem21DeleteBagItemPosIndexEsh", 0x1c0, 0xe356005f, 0x5f, 0xab), // cmp r6, #0x5f — shift end
    imm("_ZN7BagItem21DeleteBagItemPosIndexEsh", 0x1d0, 0xe287b05e, 0x5e, 0xaa), // add fp, r7, #0x5e — last slot
    imm("_ZN7BagItem21DeleteBagItemPosIndexEsh", 0x1e8, 0xe5c13188, 0x188, 0x2b8), // strb r3, [r1, #0x188] — stack
    imm("_ZN7BagItem21DeleteBagItemPosIndexEsh", 0x224, 0xe5d32188, 0x188, 0x2b8), // ldrb r2, [r3, #0x188] — stack
    imm("_ZN7BagItem21DeleteBagItemPosIndexEsh", 0x238, 0xe5c32188, 0x188, 0x2b8), // strb r2, [r3, #0x188] — stack
    imm("_ZN7BagItem26DeleteBagItemCategoryIndexEaah", 0x38, 0xe3550060, 0x60, 0xac), // cmp r5, #0x60 — loop
    imm("_ZN7BagItem26DeleteBagItemCategoryIndexEaah", 0x60, 0xe5d74188, 0x188, 0x2b8), // ldrb r4, [r7, #0x188] — stack
    imm("_ZN7BagItem26DeleteBagItemCategoryIndexEaah", 0x7c, 0xe3550060, 0x60, 0xac), // cmp r5, #0x60 — loop
    imm("_ZN10RefineItem15ApplyOrbCombineEP13EquipItemInfosaaPP8ItemInfo", 0x224, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN10RefineItem15ApplyOrbCombineEP13EquipItemInfosaaPP8ItemInfo", 0x240, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN7BagItemD1Ev", 0x20, 0xe59641e8, 0x1e8, 0x860), // ldr r4, [r6, #0x1e8] — filter
    imm("_ZN7BagItemD1Ev", 0x4c, 0xe586a1e8, 0x1e8, 0x860), // str sl, [r6, #0x1e8] — filter
    imm("_ZN7BagItemD1Ev", 0x50, 0xe3570060, 0x60, 0xac), // cmp r7, #0x60 — filter loop
    imm("_ZN7BagItemD1Ev", 0x78, 0xe3540060, 0x60, 0xac), // cmp r4, #0x60 — delete loop
    imm("_ZN7BagItemD2Ev", 0x20, 0xe59641e8, 0x1e8, 0x860), // ldr r4, [r6, #0x1e8] — filter
    imm("_ZN7BagItemD2Ev", 0x4c, 0xe586a1e8, 0x1e8, 0x860), // str sl, [r6, #0x1e8] — filter
    imm("_ZN7BagItemD2Ev", 0x50, 0xe3570060, 0x60, 0xac), // cmp r7, #0x60 — filter loop
    imm("_ZN7BagItemD2Ev", 0x78, 0xe3540060, 0x60, 0xac), // cmp r4, #0x60 — delete loop
    imm("_ZN7BagItem11SortBagItemEv", 0x16c, 0xe5d32188, 0x188, 0x2b8), // ldrb r2, [r3, #0x188] — stack
    imm("_ZN7BagItem11SortBagItemEv", 0x170, 0xe5d1c188, 0x188, 0x2b8), // ldrb ip, [r1, #0x188] — stack
    imm("_ZN7BagItem11SortBagItemEv", 0x17c, 0xe5c3c188, 0x188, 0x2b8), // strb ip, [r3, #0x188] — stack
    imm("_ZN7BagItem11SortBagItemEv", 0x190, 0xe5c32188, 0x188, 0x2b8), // strb r2, [r3, #0x188] — stack
    imm("_ZN10RefineItem15ApplyItemRefineEP13EquipItemInfosaihPP8ItemInfo", 0x188, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN10RefineItem15ApplyItemRefineEP13EquipItemInfosaihPP8ItemInfo", 0x1a4, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN7BagItem26DeleteBagItemBunchPosIndexEsh", 0x118, 0xe358005e, 0x5e, 0xaa), // cmp r8, #0x5e — shift guard
    imm("_ZN7BagItem26DeleteBagItemBunchPosIndexEsh", 0x12c, 0xe289cf62, 0x188, 0x2b8), // add ip, sb, #0x188 — stack
    imm("_ZN7BagItem26DeleteBagItemBunchPosIndexEsh", 0x1a0, 0xe356005f, 0x5f, 0xab), // cmp r6, #0x5f — shift end
    imm("_ZN7BagItem26DeleteBagItemBunchPosIndexEsh", 0x1b0, 0xe288b05e, 0x5e, 0xaa), // add fp, r8, #0x5e — last slot
    imm("_ZN7BagItem26DeleteBagItemBunchPosIndexEsh", 0x1c8, 0xe5c13188, 0x188, 0x2b8), // strb r3, [r1, #0x188] — stack
    imm("_ZN7BagItem18DeleteQuestItemAllEv", 0x1c, 0xe3550606, 0x600000, 0xac0000), // cmp r5, #0x600000 — loop
    imm("_ZN7BagItem18DeleteQuestItemAllEv", 0x60, 0xe3550606, 0x600000, 0xac0000), // cmp r5, #0x600000 — loop
    imm("_ZN7BagItem13NewBagNetItemEP10_NET_ITEM_h", 0x14, 0x13a08060, 0x60, 0xac), // movne r8, #0x60 — overflow scan
    imm("_ZN7BagItem13NewBagNetItemEP10_NET_ITEM_h", 0x78, 0xe5d01188, 0x188, 0x2b8), // ldrb r1, [r0, #0x188] — stack
    imm("_ZN7BagItem13NewBagNetItemEP10_NET_ITEM_h", 0x98, 0xe5c21188, 0x188, 0x2b8), // strb r1, [r2, #0x188] — stack
    imm("_ZN7BagItem13NewBagNetItemEP10_NET_ITEM_h", 0x2cc, 0xe5c71188, 0x188, 0x2b8), // strb r1, [r7, #0x188] — stack
    imm("_ZN7BagItem13NewBagNetItemEP10_NET_ITEM_h", 0x3c0, 0xe5c72188, 0x188, 0x2b8), // strb r2, [r7, #0x188] — stack
    imm("_ZN7BagItem10NewBagItemEP8ItemInfoh", 0x14, 0x13a08060, 0x60, 0xac), // movne r8, #0x60 — overflow scan
    imm("_ZN7BagItem10NewBagItemEP8ItemInfoh", 0x78, 0xe5d01188, 0x188, 0x2b8), // ldrb r1, [r0, #0x188] — stack
    imm("_ZN7BagItem10NewBagItemEP8ItemInfoh", 0x98, 0xe5c21188, 0x188, 0x2b8), // strb r1, [r2, #0x188] — stack
    imm("_ZN7BagItem10NewBagItemEP8ItemInfoh", 0x16c, 0xe5c72188, 0x188, 0x2b8), // strb r2, [r7, #0x188] — stack
    imm("_ZN7BagItem10NewBagItemEP8ItemInfoh", 0x254, 0xe5c72188, 0x188, 0x2b8), // strb r2, [r7, #0x188] — stack
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0x1c, 0xe24dd004, 0x4, 0x390), // sub sp, sp, #4 — frame
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0x244, 0xe28dce3f, 0x3f0, 0x740), // add ip, sp, #0x3f0 — stack copy
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0x248, 0xe28d0e27, 0x270, 0x480), // add r0, sp, #0x270 — ptr copy
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0x3ac, 0xe5d03188, 0x188, 0x2b8), // ldrb r3, [r0, #0x188] — stack
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0x4d0, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0x6d4, 0xe28d8e3f, 0x3f0, 0x740), // add r8, sp, #0x3f0 — stack copy
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0x6d8, 0xe28dce27, 0x270, 0x480), // add ip, sp, #0x270 — ptr copy
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0x958, 0xe5c03188, 0x188, 0x2b8), // strb r3, [r0, #0x188] — stack
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0xa7c, 0xe28d6e27, 0x270, 0x480), // add r6, sp, #0x270 — ptr copy cleanup
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0xad0, 0xe28dd064, 0x64, 0x3f0), // add sp, sp, #0x64 — frame
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0xd0c, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0xd44, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN15StateInGameMenu15ChangeHeroClassEa", 0xd7c, 0xe2800020, 0x20, 0x2d0), // add r0, r0, #0x20 — HERO alloc
    imm("_ZN15StateInGameMenu14DrawSpiritMenuEv", 0x4a0, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu14DrawSpiritMenuEv", 0x4e0, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu14DrawSpiritMenuEv", 0x540, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu14DrawSpiritMenuEv", 0x998, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu14DrawSpiritMenuEv", 0x9cc, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu14DrawSpiritMenuEv", 0x1ad8, 0xe283107a, 0x7a, 0x218), // add r1, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu14DrawSpiritMenuEv", 0x1b58, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawSkillMenuEv", 0xea8, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawSkillMenuEv", 0xee4, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawSkillMenuEv", 0xf24, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawSkillMenuEv", 0xf48, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawSkillMenuEv", 0x2174, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawSkillMenuEv", 0x21f4, 0xe281107a, 0x7a, 0x218), // add r1, r1, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawEquipMenuEv", 0xfec, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawEquipMenuEv", 0x1030, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawEquipMenuEv", 0x109c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawEquipMenuEv", 0x1828, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawEquipMenuEv", 0x185c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawEquipMenuEv", 0x1ce8, 0xe283107a, 0x7a, 0x218), // add r1, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13DrawEquipMenuEv", 0x1d6c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu11DrawBagMenuEv", 0x1fc, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu11DrawBagMenuEv", 0x240, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu11DrawBagMenuEv", 0x2a0, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu11DrawBagMenuEv", 0xbb0, 0xe5d37188, 0x188, 0x2b8), // ldrb r7, [r3, #0x188] — stack
    imm("_ZN15StateInGameMenu13KeySpiritMenuEi", 0x29c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu13KeySpiritMenuEi", 0x2e8, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu12KeySkillMenuEi", 0x98c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu12KeySkillMenuEi", 0x9d8, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN15StateInGameMenu12KeySkillMenuEi", 0x17fc, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu12KeySkillMenuEi", 0x1838, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN15StateInGameMenu12KeyEquipMenuEi", 0x1f4, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu12KeyEquipMenuEi", 0x250, 0xe28c207a, 0x7a, 0x218), // add r2, ip, #0x7a — filter
    imm("_ZN15StateInGameMenu10KeyBagMenuEi", 0x5b4, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu7ProcessEv", 0x1478, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu7ProcessEv", 0x170c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN15StateInGameMenu7ProcessEv", 0x1784, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu13KeyCollectionEi", 0x640, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu13KeyCollectionEi", 0x678, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawCollectionEv", 0x8b4, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawCollectionEv", 0x8f0, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawCollectionEv", 0x9c8, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawCollectionEv", 0x9f8, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu13KeyOrbCombineEi", 0x1f4, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu13KeyOrbCombineEi", 0x270, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu13KeyOrbCombineEi", 0x53c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu13KeyOrbCombineEi", 0x55c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu13KeyOrbCombineEi", 0x598, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu13KeyOrbCombineEi", 0x5f8, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu13KeyOrbCombineEi", 0x62c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu9KeyRefineEih", 0x24c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu9KeyRefineEih", 0x290, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu9KeyRefineEih", 0x478, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0x270, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0x290, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0x40c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0x448, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0x490, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0x4e0, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0x520, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0x59c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0x5c0, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0xc5c, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0xc6c, 0xe5d33188, 0x188, 0x2b8), // ldrb r3, [r3, #0x188] — stack
    imm("_ZN14StateInNpcMenu14DrawOrbCombineEv", 0xf74, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu10DrawRefineEh", 0x224, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu10DrawRefineEh", 0x244, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu10DrawRefineEh", 0x854, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu10DrawRefineEh", 0x89c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu10DrawRefineEh", 0xb74, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu10DrawRefineEh", 0xbb8, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu10DrawRefineEh", 0xc08, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu10DrawRefineEh", 0xc74, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu12DrawItemSaleEv", 0x6e4, 0xe5d39188, 0x188, 0x2b8), // ldrb sb, [r3, #0x188] — stack
    imm("_ZN14StateInNpcMenu7ProcessEv", 0x200, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0x730, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0x750, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0x7f4, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0x814, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0x844, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0xa30, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0xa4c, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0xba0, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0xbc4, 0xe282207a, 0x7a, 0x218), // add r2, r2, #0x7a — filter
    imm("_ZN14StateInNpcMenu7ProcessEv", 0xbf4, 0xe283307a, 0x7a, 0x218), // add r3, r3, #0x7a — filter
    imm("_ZN13StateMainMenu7NewGameEaa", 0x50, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN13StateMainMenu7NewGameEaa", 0x1a8, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN13StateMainMenu7NewGameEaa", 0x1d8, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN13StateMainMenu7NewGameEaa", 0x208, 0xe2800020, 0x20, 0x2d0), // add r0, r0, #0x20 — HERO alloc
    imm("_ZN12StateNetMenu13DrawStorageUIEaahhh", 0x840, 0xe2800f62, 0x188, 0x2b8), // add r0, r0, #0x188 — stack
    imm("_ZN12StateNetMenu7ProcessEv", 0xd70, 0xe5d33188, 0x188, 0x2b8), // ldrb r3, [r3, #0x188] — stack
    imm("_ZN10StatePopup7ProcessEv", 0x41c, 0xe2800020, 0x20, 0x2d0), // add r0, r0, #0x20 — HERO alloc
    imm("_ZN10StatePopup7ProcessEv", 0x44c, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN10StatePopup7ProcessEv", 0x47c, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
    imm("_ZN10StatePopup7ProcessEv", 0x4ac, 0xe2800018, 0x18, 0x2d0), // add r0, r0, #0x18 — HERO alloc
];

/// The immediate an instruction carries, for the three encodings this table
/// uses: data-processing with a rotated 8-bit immediate (`mov`/`add`/`cmp`),
/// word/byte load-store with a 12-bit offset (`ldrb`/`strb`), and the
/// halfword/signed-byte form with a split 8-bit offset (`ldrsb`/`ldrh`/…).
pub(crate) fn imm_of(w: u32) -> Option<u32> {
    match (w >> 25) & 7 {
        // Data processing, immediate operand.
        0b001 => {
            let rot = (w >> 8) & 0xf;
            Some((w & 0xff).rotate_right(2 * rot))
        }
        // Single data transfer with an immediate offset (I = 0).
        0b010 => Some(w & 0xfff),
        // Extra load/store (H/SB/SH/D) with an immediate offset: bit 22 set,
        // bits 7 and 4 set.
        0b000 if w & (1 << 22) != 0 && w & 0x90 == 0x90 => Some(((w >> 4) & 0xf0) | (w & 0xf)),
        _ => None,
    }
}

/// The same instruction with immediate `v`, or `None` if `v` does not encode
/// in that form.
pub(crate) fn with_imm(w: u32, v: u32) -> Option<u32> {
    match (w >> 25) & 7 {
        0b001 => (0..16).find_map(|rot| {
            let imm8 = v.rotate_left(2 * rot);
            (imm8 <= 0xff).then_some((w & !0xfff) | (rot << 8) | imm8)
        }),
        0b010 => (v <= 0xfff).then_some((w & !0xfff) | v),
        0b000 if w & (1 << 22) != 0 && w & 0x90 == 0x90 => {
            (v <= 0xff).then_some((w & !0xf0f) | ((v & 0xf0) << 4) | (v & 0xf))
        }
        _ => None,
    }
}

/// Write `table` into guest memory. **All or nothing**: every word is located,
/// compared with the table and re-encoded before the first one is written.
pub(crate) fn apply(vm: &mut Vm, table: &[Imm], what: &str) -> Result<usize, String> {
    let mut plan = Vec::with_capacity(table.len());
    for e in table {
        let base = vm.symbol(e.sym).ok_or_else(|| format!("{what}: `{}` is not in this binary", e.sym))?;
        let addr = base + e.at;
        let got = vm.mem.read_u32(addr);
        if got != e.was {
            return Err(format!(
                "{what}: {}+{:#x} holds {got:#010x}, expected {:#010x} — not the binary this \
                 layout was read from; nothing written",
                e.sym, e.at, e.was
            ));
        }
        if imm_of(got) != Some(e.old) {
            return Err(format!("{what}: {}+{:#x} does not carry {:#x}; nothing written", e.sym, e.at, e.old));
        }
        let now = with_imm(got, e.new)
            .ok_or_else(|| format!("{what}: {:#x} does not encode at {}+{:#x}", e.new, e.sym, e.at))?;
        plan.push((addr, now));
    }
    for (addr, now) in &plan {
        vm.mem.write_u32(*addr, *now);
    }
    Ok(plan.len())
}

// ---- the bag's save tail -------------------------------------------------------
//
// `HERO::SaveBagData` writes, in order: `[i32 money][i16 size][i16 count]`, one
// record per item, **96 stack-count bytes** (a fixed `cmp r3, #0x60` loop, not
// `count`), and 53 bytes of equipment slots and hot keys
// (`HERO::GetBagDataSize` = `8 + records + 0x95`). `HERO::LoadBagData` reads it
// back the same way and never looks at the length — so bytes after the 53 are
// ignored by every build, this one included.
//
// The stack counts of slots 96..171 go there: an 80-byte tail, `BG10` and 76
// counts. `GetBagDataSize` and the length `SaveBagData` hands
// `StaticUtil::SaveEncryptFile` each grow by 80 (two words in [`BAG`]), and two
// observation hooks fill and read the tail — inside the guest's own buffer, so
// the guest's own DES and MD5 cover it. A save from an older build has no tail
// and loads with those counts at zero, which is what they are: it cannot hold
// more than 96 items.
//
// **The other direction is not safe.** An older build reading a save that
// holds more than 96 items writes past its 96-slot arrays. The tail cannot
// prevent that; nothing in this build can.

const SAVE_BAG: &str = "_ZN4HERO11SaveBagDataEa";
const LOAD_BAG: &str = "_ZN4HERO11LoadBagDataEa";
/// `SaveBagData+0x3e4` (0x8F8D8) is `bl StaticUtil::SaveEncryptFile` with
/// `r1` = the buffer, `r2` = its length and `r8` = the `HERO`.
const SAVE_TAIL_AT: u32 = 0x3e4;
/// `LoadBagData+0x510` (0x8F05C) is `mov r0, r5` before the buffer is freed:
/// everything is parsed, `r5` is the buffer, `fp` is `buffer + end - 0x21`
/// (the last byte read is `[fp, #0x20]`, 0x8F044), `sb` is the `HERO`, and
/// `[sp+0x24]` is what `LoadDecryptFile` returned — the file's size.
const LOAD_TAIL_AT: u32 = 0x510;
pub const TAIL_MAGIC: [u8; 4] = *b"BG10";
pub const TAIL_LEN: u32 = 0x50;
/// Slots the arrays hold, and how many stack counts the shipped format saves.
pub const BAG_SLOTS: u32 = 172;
pub const SAVED_STACKS: u32 = 96;
/// Slots the bag shows: ten pages of sixteen.
pub const BAG_PAGES_SIZE: u16 = 160;
/// `HERO + 0x17c0 + 0x2b8`: the moved stack-count array.
const HERO_STACKS: u32 = HERO_BAG + 0x2b8;
const HERO_BAG_SIZE: u32 = HERO_BAG + 4;

/// Install the two save-tail observers. Both decline: the guest's own
/// instruction runs after them.
pub fn install_bag_hooks(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    let save = vm.symbol(SAVE_BAG).ok_or("ten pages: SaveBagData is not in this binary")?;
    let load = vm.symbol(LOAD_BAG).ok_or("ten pages: LoadBagData is not in this binary")?;
    vm.install_hook(SAVE_BAG, save + SAVE_TAIL_AT, save_tail_hook)?;
    vm.install_hook(LOAD_BAG, load + LOAD_TAIL_AT, load_tail_hook)?;
    Ok(vec![SAVE_BAG, LOAD_BAG])
}

fn save_tail_hook(vm: &mut Vm) -> Result<bool, String> {
    let buf = vm.cpu.reg_get(Mode::User, 1);
    let len = vm.cpu.reg_get(Mode::User, 2);
    let hero = vm.cpu.reg_get(Mode::User, 8);
    if len < TAIL_LEN || buf == 0 || hero == 0 {
        return Ok(false);
    }
    let at = buf + len - TAIL_LEN;
    let n = (BAG_SLOTS - SAVED_STACKS) as usize;
    let counts = vm.mem.slice(hero + HERO_STACKS + SAVED_STACKS, n as u32).map(<[u8]>::to_vec);
    let Some(counts) = counts else { return Ok(false) };
    vm.mem.write_bytes(at, &TAIL_MAGIC);
    vm.mem.write_bytes(at + 4, &counts);
    vm.pages10.tails_saved += 1;
    Ok(false)
}

fn load_tail_hook(vm: &mut Vm) -> Result<bool, String> {
    let buf = vm.cpu.reg_get(Mode::User, 5);
    let fp = vm.cpu.reg_get(Mode::User, 11);
    let hero = vm.cpu.reg_get(Mode::User, 9);
    let sp = vm.cpu.reg_get(Mode::User, reg::SP);
    if buf == 0 || hero == 0 || fp < buf {
        return Ok(false);
    }
    let end = fp - buf + 0x21;
    let data = (vm.mem.read_u32(sp + 0x24) as i32).saturating_sub(0x10);
    let n = (BAG_SLOTS - SAVED_STACKS) as usize;
    let mut counts = vec![0u8; n];
    if data >= 0 && end + TAIL_LEN <= data as u32 {
        if let Some(t) = vm.mem.slice(buf + end, TAIL_LEN) {
            if t[..4] == TAIL_MAGIC {
                counts.copy_from_slice(&t[4..4 + n]);
                vm.pages10.tails_loaded += 1;
            }
        }
    }
    vm.mem.write_bytes(hero + HERO_STACKS + SAVED_STACKS, &counts);
    // Ten pages for every bag, whatever size the save carries (48..80): the
    // constructor already says 160 and this is the one place a save replaces
    // it. The 창고 확장 item and its purchase both refuse above 0x4F, so
    // nothing grows it past the arrays.
    vm.mem.write_u16(hero + HERO_BAG_SIZE, BAG_PAGES_SIZE);
    Ok(false)
}

// ---- the tabs ---------------------------------------------------------------

const DRAW_IMAGE_BLENDING: &str = "_ZN7Graphic17DrawImageBlendingEiiiP8GbmImagehhhi";
const TOUCH_STORAGE_UI: &str = "_ZN12StateNetMenu14TouchStorageUIEiiPaS0_hhaa";
const KEY_STORAGE: &str = "_ZN12StateNetMenu10KeyStorageEi";
const SET_SUB_STATE: &str = "_ZN12StateNetMenu11SetSubStateEa";
const TOUCH_AUTO_INFO: &str = "_ZN8CommonUi13TouchAutoInfoEii";
const NUM_KEY_TO_NAVI: &str = "_ZN10StaticUtil15NumKeyToNaviKeyEi";
const GV: &str = "gv";
const GV_COMMON_UI: u32 = 0x15fc;
const GV_HERO: u32 = 0x1474;
/// The byte `TouchStorageUI` clears on a tab change (`gv + 0x198B`, 0x12E2CC).
const GV_TOUCH_LATCH: u32 = 0x198b;

/// The tab loop's three blits, by return address inside `DrawStorageUI`
/// (0x131E7C): the open chest for the current page (0x1327B0), a closed chest
/// (0x1323F4), and a closed chest dimmed while the cursor is in the grid
/// (`DrawImageBlending`, 0x132374).
const TAB_RET_CURRENT: u32 = 0x1327b4 - 0x131e7c;
const TAB_RET_OTHER: u32 = 0x1323f8 - 0x131e7c;
const TAB_RET_DIM: u32 = 0x132378 - 0x131e7c;
/// The loop draws tab `i` at `y = origin.y + 0x3D + 24 i` (0x132344, 0x132380).
const TAB_Y0: i32 = 0x3d;
const TAB_STEP: i32 = 0x18;

/// `StateNetMenu` fields. The panel origin every rect is relative to
/// (`[this+0x18]`, `[this+0x1c]`), the sub-state (`GetSubState` is
/// `ldrsb [this, #0xd]`), the page the key handler moves (`[this+0xe]`), and
/// the warehouse object.
const NM_ORIGIN_X: u32 = 0x18;
const NM_ORIGIN_Y: u32 = 0x1c;
const NM_SUB_STATE: u32 = 0xd;
const NM_PAGE: u32 = 0xe;
const NM_STORAGE: u32 = 0x24;

/// Every tab column the game draws blits chest `i` at `x0 + 13` — the open
/// chest covers `x .. x+24`, a closed one `x .. x+21`, both ending one row
/// above `y` — and puts the grid's first cell box at `x0 + 44`, 26 px wide,
/// 29 px apart (`.rodata` 0x15A0A8 and its two twins).
const TAB_X0: i32 = 13;
const CHEST_W: i32 = 24;
pub const GRID_X0: i32 = 44;

/// How far right the grid moves. Two chest columns need 4 + 24 + 24 of the
/// panel's 176 px; 12 leaves 4 px between the second column and the grid and
/// ends the grid 7 px from the panel's right edge, as the chests start 4 px
/// from its left.
pub const GRID_SHIFT: u32 = 12;

/// The new chest columns: where each blits, relative to the panel origin.
/// Rows keep the guest's 24 px pitch.
pub const TAB_COL_X: [i32; 2] = [4, 4 + CHEST_W];
/// A tab's touch cell is the chest's whole 24 × 24 pitch, from 2 px above the
/// open chest's top (`y0 + 0x3d - 22`) — the guest's own rects were 21 × 21
/// from `y0 + 40` (`.rodata` 0x15A838).
pub const TAB_W: i32 = CHEST_W;
pub const TAB_H: i32 = 24;
pub const TAB_ROW_Y: i32 = 38;
pub const TAB_ROW_STEP: i32 = 24;
pub const TABS_PER_COL: i32 = 5;
/// The whole column a press is answered in, so the old rects never see one —
/// everything left of the moved grid, above the warehouse bag view's extra
/// cell (`y0 + 0xb5`) and the bag's `0.아이템정리` box (`y0 + 0xa2`).
const COLUMN: (i32, i32, i32, i32) =
    (0, TAB_ROW_Y, GRID_X0 + GRID_SHIFT as i32, TAB_ROW_Y + TABS_PER_COL * TAB_ROW_STEP);

/// Tab `i`'s touch cell, relative to the panel origin.
pub fn tab_rect(i: i32) -> (i32, i32, i32, i32) {
    let (col, row) = (i / TABS_PER_COL, i % TABS_PER_COL);
    (TAB_COL_X[col as usize], TAB_ROW_Y + row * TAB_ROW_STEP, TAB_W, TAB_H)
}

/// Which tab a point (relative to the panel origin) is on, if any.
pub fn tab_at(x: i32, y: i32) -> Option<i32> {
    (0..2 * TABS_PER_COL).find(|&i| {
        let (rx, ry, w, h) = tab_rect(i);
        x >= rx && x < rx + w && y >= ry && y < ry + h
    })
}

/// Where left or right takes the cursor in the tab column, or `None` to let
/// the guest have the key (right with nothing to the right enters the grid).
pub fn tab_step(page: i32, pages: i32, navi: i32) -> Option<i32> {
    if page < 0 || page >= pages {
        return None;
    }
    match navi {
        KEY_RIGHT if page < TABS_PER_COL && page + TABS_PER_COL < pages => Some(page + TABS_PER_COL),
        KEY_LEFT if page >= TABS_PER_COL => Some(page - TABS_PER_COL),
        _ => None,
    }
}

const KEY_LEFT: i32 = -3;
const KEY_RIGHT: i32 = -4;

/// Counters the probe prints so "the hook never ran" is visible.
#[derive(Default)]
pub struct TenPages {
    /// Chest blits moved into the two columns.
    pub chests: u64,
    /// Touch-handler cell tables moved with the grid.
    pub cell_tables: u64,
    pub taps: u64,
    pub keys: u64,
    /// Bag saves that carried the stack-count tail out, and loads that found
    /// one.
    pub tails_saved: u64,
    pub tails_loaded: u64,
}

// The three bag screens draw the same tab column the warehouse does — the
// same chest sprites at `x0 + 13`, `y0 + 0x3d + 24 i`, the same 21-px touch
// rects from a five-entry `.rodata` table — and differ only in where their
// fields live. Each is a row here.

/// One screen with a bag-or-warehouse tab column.
struct Screen {
    /// The function whose tab loop blits the chests, and the return
    /// addresses of its `DrawImage` (current and other chest share one in the
    /// bag screens) and `DrawImageBlending` (other chests, cursor in the grid).
    draw: &'static str,
    ret_image: u32,
    ret_blend: u32,
    /// The register the loop keeps the tab index in.
    index_reg: u8,
    /// `this` fields: the page byte the tabs select and the grid cell a page
    /// change resets.
    page_at: u32,
    cell_at: u32,
    touch: &'static str,
    key: &'static str,
    get_sub: &'static str,
    set_sub: &'static str,
}

/// `StateInGameMenu` — the in-game 가방 (DrawBagMenu: blits at 0xFB024 and
/// 0xFB100; TouchBagMenu's tab loop 0x1031F8; KeyBagMenu's 0x1093BC).
const BAG_MENU: Screen = Screen {
    draw: "_ZN15StateInGameMenu11DrawBagMenuEv",
    ret_image: 0xfb028 - 0xfa9b8,
    ret_blend: 0xfb104 - 0xfa9b8,
    index_reg: 7,
    page_at: 0xe,
    cell_at: 0xf,
    touch: "_ZN15StateInGameMenu12TouchBagMenuEii",
    key: "_ZN15StateInGameMenu10KeyBagMenuEi",
    get_sub: "_ZN15StateInGameMenu11GetSubStateEv",
    set_sub: "_ZN15StateInGameMenu11SetSubStateEa",
};

/// `StateInNpcMenu` — selling to an NPC (DrawItemSale: blits at 0x116F74 and
/// 0x117050; TouchItemSale's tab loop 0x111C78; KeyItemSale's 0x112048).
const SALE_MENU: Screen = Screen {
    draw: "_ZN14StateInNpcMenu12DrawItemSaleEv",
    ret_image: 0x116f78 - 0x116c88,
    ret_blend: 0x117054 - 0x116c88,
    index_reg: 4,
    page_at: 0x10,
    cell_at: 0x11,
    touch: "_ZN14StateInNpcMenu13TouchItemSaleEii",
    key: "_ZN14StateInNpcMenu11KeyItemSaleEi",
    get_sub: "_ZN14StateInNpcMenu11GetSubStateEv",
    set_sub: "_ZN14StateInNpcMenu11SetSubStateEa",
};

const GET_HALF_WIDTH: &str = "_ZN7Graphic12GetHalfWidthEv";
const GET_HALF_HEIGHT: &str = "_ZN7Graphic13GetHalfHeightEv";
const LCD_G: &str = "_lcdG";

const DRAW_BAG_MENU: &str = BAG_MENU.draw;
const DRAW_ITEM_SALE: &str = SALE_MENU.draw;

/// Every grid `x` the three draw functions compute, moved [`GRID_SHIFT`] to
/// the right. Each is the panel origin plus an immediate — the bag screens
/// subtract theirs from the half width (`x0 = halfW - 0x58`), the warehouse
/// adds its to `[this + 0x18]` — so the cells, their icons, stack counts,
/// `e` and hot-key marks, the cursor box, the `#` hint and the item-name
/// tooltip all follow from 23 words. Nothing else in these functions uses
/// those immediates: the header, the tabs, the `0.아이템정리` box and the
/// auto item-info box are placed from other ones, or from the screen edges.
/// `tools_re/words.py` prints any of them back from the binary.
pub(crate) const GRID: &[Imm] = &[
    // DrawBagMenu (0xFA9B8): the icon column (halfW − 0x1f) and the cell box
    // column (halfW − 0x2b) the loop steps by 29; then the `#` hint and the
    // name tooltip, left- and right-aligned, from x0.
    imm(DRAW_BAG_MENU, 0x780, 0xe243_301f, 0x1f, 0x1f - GRID_SHIFT), // sub r3, r3, #0x1f
    imm(DRAW_BAG_MENU, 0x790, 0xe240_002b, 0x2b, 0x2b - GRID_SHIFT), // sub r0, r0, #0x2b
    imm(DRAW_BAG_MENU, 0xf74, 0xe283_202c, 0x2c, 0x2c + GRID_SHIFT), // add r2, r3, #0x2c — `#`
    imm(DRAW_BAG_MENU, 0xfdc, 0xe280_002f, 0x2f, 0x2f + GRID_SHIFT), // add r0, r0, #0x2f — name
    imm(DRAW_BAG_MENU, 0x1114, 0xe281_b02f, 0x2f, 0x2f + GRID_SHIFT), // add fp, r1, #0x2f — name, right
    // DrawItemSale (0x116C88): the same five.
    imm(DRAW_ITEM_SALE, 0x3fc, 0xe243_301f, 0x1f, 0x1f - GRID_SHIFT),
    imm(DRAW_ITEM_SALE, 0x40c, 0xe240_002b, 0x2b, 0x2b - GRID_SHIFT),
    imm(DRAW_ITEM_SALE, 0x9dc, 0xe283_202c, 0x2c, 0x2c + GRID_SHIFT),
    imm(DRAW_ITEM_SALE, 0xa48, 0xe28e_e02f, 0x2f, 0x2f + GRID_SHIFT), // add lr, lr, #0x2f
    imm(DRAW_ITEM_SALE, 0xba4, 0xe281_a02f, 0x2f, 0x2f + GRID_SHIFT), // add sl, r1, #0x2f
    // DrawStorageUI (0x131E7C), tooltip pass: the `#` hint, the name box and
    // the name, then the right-aligned box and name.
    imm(DRAW_STORAGE_UI, 0x1b0, 0xe282_202c, 0x2c, 0x2c + GRID_SHIFT), // add r2, r2, #0x2c
    imm(DRAW_STORAGE_UI, 0x214, 0xe28c_c02c, 0x2c, 0x2c + GRID_SHIFT), // add ip, ip, #0x2c
    imm(DRAW_STORAGE_UI, 0x294, 0xe282_202f, 0x2f, 0x2f + GRID_SHIFT), // add r2, r2, #0x2f
    imm(DRAW_STORAGE_UI, 0xccc, 0xe282_2046, 0x46, 0x46 + GRID_SHIFT), // add r2, r2, #0x46
    imm(DRAW_STORAGE_UI, 0xd54, 0xe282_2049, 0x49, 0x49 + GRID_SHIFT), // add r2, r2, #0x49
    // …grid pass: cell box, icon, stack count, the icon again for the bag
    // view, the item-state overlay, the cursor box, the `e` mark and the
    // hot-key mark.
    imm(DRAW_STORAGE_UI, 0x620, 0xe282_202c, 0x2c, 0x2c + GRID_SHIFT),
    imm(DRAW_STORAGE_UI, 0x6fc, 0xe282_2039, 0x39, 0x39 + GRID_SHIFT),
    imm(DRAW_STORAGE_UI, 0x778, 0xe282_2045, 0x45, 0x45 + GRID_SHIFT),
    imm(DRAW_STORAGE_UI, 0x9ac, 0xe282_2039, 0x39, 0x39 + GRID_SHIFT),
    imm(DRAW_STORAGE_UI, 0xa30, 0xe282_2039, 0x39, 0x39 + GRID_SHIFT),
    imm(DRAW_STORAGE_UI, 0xa94, 0xe282_202c, 0x2c, 0x2c + GRID_SHIFT),
    imm(DRAW_STORAGE_UI, 0xbc0, 0xe282_202d, 0x2d, 0x2d + GRID_SHIFT),
    imm(DRAW_STORAGE_UI, 0xc38, 0xe282_202d, 0x2d, 0x2d + GRID_SHIFT),
];

/// A touch handler's cell `x` table. Each copies its own 16-entry `.rodata`
/// table onto its stack in its prologue and hit-tests the cells from the copy;
/// `at` is the first instruction after the copy is complete and `sp` where the
/// copy sits.
struct CellTable {
    sym: &'static str,
    at: u32,
    sp: u32,
}

const CELL_TABLES: [CellTable; 3] = [
    // TouchBagMenu (0x102EE4): `.rodata` 0x15A0A8 to sp+0x60 by 0x102F3C.
    CellTable { sym: BAG_MENU.touch, at: 0x103118 - 0x102ee4, sp: 0x60 },
    // TouchItemSale (0x111B7C): 0x15A4DC to sp+0x40 by 0x111C10.
    CellTable { sym: SALE_MENU.touch, at: 0x111c4c - 0x111b7c, sp: 0x40 },
    // TouchStorageUI (0x12DE94): 0x15A844 to sp+0x68 by 0x12DF5C.
    CellTable { sym: TOUCH_STORAGE_UI, at: 0x12df88 - 0x12de94, sp: 0x68 },
];

/// The shipped cell `x` table: four columns, four rows.
fn shipped_cell_x(k: u32) -> i32 {
    GRID_X0 + 29 * (k % 4) as i32
}

/// Install the tab hooks. `DrawImage` is not installed here — it is shared
/// with the warehouse's coin hook and goes through `Vm`'s dispatcher, which
/// calls [`draw_image_arm`].
pub fn install_hooks(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    let mut need = vec![
        DRAW_STORAGE_UI,
        DRAW_IMAGE_BLENDING,
        TOUCH_STORAGE_UI,
        KEY_STORAGE,
        SET_SUB_STATE,
        TOUCH_AUTO_INFO,
        NUM_KEY_TO_NAVI,
        GET_HALF_WIDTH,
        GET_HALF_HEIGHT,
        LCD_G,
        GV,
    ];
    for sc in [&BAG_MENU, &SALE_MENU] {
        need.extend([sc.draw, sc.touch, sc.key, sc.get_sub, sc.set_sub]);
    }
    for s in need {
        if vm.symbol(s).is_none() {
            return Err(format!("ten pages: `{s}` is not in this binary"));
        }
    }
    // `DrawImageBlending` is not installed here either: the 배치변경 capture
    // passes want it too, so `Vm`'s dispatcher calls [`dim_tab_arm`].
    let hooks: [(&'static str, u32, PatchFn); 9] = [
        (TOUCH_STORAGE_UI, 0, touch_hook),
        (KEY_STORAGE, 0, key_hook),
        (BAG_MENU.touch, 0, bag_touch_hook),
        (BAG_MENU.key, 0, bag_key_hook),
        (SALE_MENU.touch, 0, sale_touch_hook),
        (SALE_MENU.key, 0, sale_key_hook),
        (CELL_TABLES[0].sym, CELL_TABLES[0].at, bag_cells_hook),
        (CELL_TABLES[1].sym, CELL_TABLES[1].at, sale_cells_hook),
        (CELL_TABLES[2].sym, CELL_TABLES[2].at, storage_cells_hook),
    ];
    let mut names = Vec::new();
    for (s, off, f) in hooks {
        let at = vm.symbol(s).expect("checked above");
        vm.install_hook(s, at + off, f)?;
        if off == 0 {
            names.push(s);
        }
    }
    Ok(names)
}

fn lr(vm: &Vm) -> u32 {
    vm.cpu.reg_get(Mode::User, reg::LR)
}

/// The `DrawImage` arm: the chests of all four tab columns.
pub fn draw_image_arm(vm: &mut Vm) -> Result<bool, String> {
    let lr = lr(vm);
    if lr == RETURN_MAGIC {
        return Ok(false);
    }
    if vm.symbol(DRAW_STORAGE_UI).is_some_and(|f| lr == f + TAB_RET_CURRENT || lr == f + TAB_RET_OTHER) {
        return storage_chest(vm);
    }
    for sc in [&BAG_MENU, &SALE_MENU] {
        if vm.symbol(sc.draw).is_some_and(|f| lr == f + sc.ret_image) {
            return screen_chest(vm, sc);
        }
    }
    Ok(false)
}

pub(crate) fn dim_tab_arm(vm: &mut Vm) -> Result<bool, String> {
    let lr = lr(vm);
    if vm.symbol(DRAW_STORAGE_UI).is_some_and(|f| lr == f + TAB_RET_DIM) {
        return storage_chest(vm);
    }
    for sc in [&BAG_MENU, &SALE_MENU] {
        if vm.symbol(sc.draw).is_some_and(|f| lr == f + sc.ret_blend) {
            return screen_chest(vm, sc);
        }
    }
    Ok(false)
}

/// The warehouse's blits: the blit's `y` says which tab it is, and `r4` is
/// `DrawStorageUI`'s `this` for the whole function (`mov r4, r0` in its
/// prologue), which gives the panel origin.
fn storage_chest(vm: &mut Vm) -> Result<bool, String> {
    let this = vm.cpu.reg_get(Mode::User, 4);
    let y = vm.arg(3) as i32;
    let oy = vm.mem.read_u32(this + NM_ORIGIN_Y) as i32;
    let d = y - oy - TAB_Y0;
    if d < 0 || d % TAB_STEP != 0 || d / TAB_STEP >= 2 * TABS_PER_COL {
        // Not a tab position this layout knows — leave the guest's blit.
        return Ok(false);
    }
    move_chest(vm, d / TAB_STEP)
}

/// A bag screen's blits: the loop counter is the tab.
fn screen_chest(vm: &mut Vm, sc: &Screen) -> Result<bool, String> {
    let i = vm.cpu.reg_get(Mode::User, sc.index_reg) as i32;
    if !(0..2 * TABS_PER_COL).contains(&i) {
        return Ok(false);
    }
    move_chest(vm, i)
}

/// Move chest `i`'s blit from the guest's one column to its place in two, and
/// let the guest draw it — same image, same frame, same dimming. Every tab
/// column the game draws puts chest `i` at `(x0 + 13, y0 + 0x3d + 24 i)`, so
/// the blit's own `x` and `y` give the column's origin.
fn move_chest(vm: &mut Vm, i: i32) -> Result<bool, String> {
    let (col, row) = (i / TABS_PER_COL, i % TABS_PER_COL);
    let x = vm.arg(2) as i32 - TAB_X0 + TAB_COL_X[col as usize];
    let y = vm.arg(3) as i32 - TAB_STEP * (i - row);
    vm.cpu.reg_set(Mode::User, 2, x as u32);
    vm.cpu.reg_set(Mode::User, 3, y as u32);
    vm.pages10.chests += 1;
    Ok(false)
}

/// Move a touch handler's stack copy of its cell table with the grid. Only a
/// copy that holds exactly the shipped values is moved — anything else means
/// the hook is not where the table says, and the guest keeps its own.
fn shift_cell_table(vm: &mut Vm, t: &CellTable) -> Result<bool, String> {
    let at = vm.cpu.reg_get(Mode::User, reg::SP) + t.sp;
    if (0..16).all(|k| vm.mem.read_u16(at + 2 * k) as i16 as i32 == shipped_cell_x(k)) {
        for k in 0..16 {
            vm.mem.write_u16(at + 2 * k, (shipped_cell_x(k) + GRID_SHIFT as i32) as u16);
        }
        vm.pages10.cell_tables += 1;
    }
    Ok(false)
}

fn bag_cells_hook(vm: &mut Vm) -> Result<bool, String> {
    shift_cell_table(vm, &CELL_TABLES[0])
}

fn sale_cells_hook(vm: &mut Vm) -> Result<bool, String> {
    shift_cell_table(vm, &CELL_TABLES[1])
}

fn storage_cells_hook(vm: &mut Vm) -> Result<bool, String> {
    shift_cell_table(vm, &CELL_TABLES[2])
}

/// Pages in the warehouse (`storage`) or the bag — `(size < 0 ? size + 15 :
/// size) >> 4`, the guest's own rounding (`TouchStorageUI+0xf8`).
fn pages(vm: &Vm, this: u32, storage: bool) -> i32 {
    if !storage {
        return bag_pages(vm);
    }
    let obj = vm.mem.read_u32(this + NM_STORAGE);
    if obj == 0 {
        return 0;
    }
    let size = vm.mem.read_u16(obj + 4) as i16 as i32;
    (if size < 0 { size + 15 } else { size }) >> 4
}

fn bag_pages(vm: &Vm) -> i32 {
    let Some(gv) = vm.symbol(GV) else { return 0 };
    let hero = vm.mem.read_u32(gv + GV_HERO);
    if hero == 0 {
        return 0;
    }
    let size = vm.mem.read_u16(hero + HERO_BAG + 4) as i16 as i32;
    (if size < 0 { size + 15 } else { size }) >> 4
}

/// Offer a press to an open item-info box, as every one of these touch
/// handlers does first while the grid has the cursor. True if it took it.
fn auto_info_took(vm: &mut Vm, x: i32, y: i32) -> Result<bool, String> {
    let gv = vm.symbol(GV).ok_or("ten pages: gv vanished")?;
    let common_ui = vm.mem.read_u32(gv + GV_COMMON_UI);
    let f = vm.symbol(TOUCH_AUTO_INFO).ok_or("ten pages: TouchAutoInfo vanished")?;
    Ok(common_ui != 0 && vm.call(f, &[common_ui, x as u32, y as u32])? != 0)
}

/// Which tab a press at `(x, y)` relative to the column origin is on:
/// `None` outside the column (the guest's), `Some(None)` inside it but on no
/// tab (swallowed, so the old rects never see it), `Some(Some(i))` on tab `i`.
fn column_press(rx: i32, ry: i32) -> Option<Option<i32>> {
    let (c0, r0, c1, r1) = COLUMN;
    if rx < c0 || rx >= c1 || ry < r0 || ry >= r1 {
        return None;
    }
    Some(tab_at(rx, ry))
}

/// `TouchStorageUI(this, x, y, char* page, char* cell, u8 grid, u8 storage,
/// u8, i8 sub_state)`. A press inside the tab column is answered here.
fn touch_hook(vm: &mut Vm) -> Result<bool, String> {
    if lr(vm) == RETURN_MAGIC {
        return Ok(false);
    }
    let this = vm.arg(0);
    let (x, y) = (vm.arg(1) as i32, vm.arg(2) as i32);
    let page_p = vm.arg(3);
    let cell_p = vm.arg(4);
    let grid = vm.arg(5) & 0xff;
    let storage = vm.arg(6) & 0xff != 0;
    let sub_state = vm.arg(8) as u8 as i8 as i32;
    let ox = vm.mem.read_u32(this + NM_ORIGIN_X) as i32;
    let oy = vm.mem.read_u32(this + NM_ORIGIN_Y) as i32;
    let Some(hit) = column_press(x - ox, y - oy) else { return Ok(false) };
    // The guest offers the press to an open item-info box first when the
    // cursor is in the grid (0x12DF98 → 0x12E214), and stops if it took it.
    if grid != 0 && auto_info_took(vm, x, y)? {
        return Ok(true);
    }
    let Some(i) = hit else { return Ok(true) };
    if i >= pages(vm, this, storage) {
        return Ok(true);
    }
    let gv = vm.symbol(GV).ok_or("ten pages: gv vanished")?;
    let set_sub = vm.symbol(SET_SUB_STATE).ok_or("ten pages: SetSubState vanished")?;
    let current = vm.mem.read_u8(page_p) as i8 as i32;
    // What the guest's tab loop does on a hit (0x12E2A0 / 0x12E304).
    if i != current {
        vm.mem.write_bytes(page_p, &[i as u8]);
        vm.call(set_sub, &[this, sub_state as u32])?;
        vm.mem.write_bytes(cell_p, &[0]);
        vm.mem.write_bytes(gv + GV_TOUCH_LATCH, &[0]);
    } else if grid == 0 {
        vm.call(set_sub, &[this, sub_state as u32])?;
        vm.mem.write_bytes(cell_p, &[0]);
    }
    vm.pages10.taps += 1;
    Ok(true)
}

/// The bag screens' panel origin, `(halfW - 0x58, halfH - 0x66)` — what
/// `TouchBagMenu` (0x1031EC) and `TouchItemSale` (0x111C50) compute.
fn menu_origin(vm: &mut Vm) -> Result<(i32, i32), String> {
    let g = vm.symbol(LCD_G).ok_or("ten pages: _lcdG vanished")?;
    let g = vm.mem.read_u32(g);
    let hw = vm.call(vm.symbol(GET_HALF_WIDTH).ok_or("ten pages: GetHalfWidth vanished")?, &[g])? as i32;
    let hh = vm.call(vm.symbol(GET_HALF_HEIGHT).ok_or("ten pages: GetHalfHeight vanished")?, &[g])? as i32;
    Ok((hw - 0x58, hh - 0x66))
}

/// `TouchBagMenu(this, x, y)` / `TouchItemSale(this, x, y)`: a press in the
/// tab column, answered the way their tab loops answer one.
///
/// * The bag tests the column only with the cursor in the tabs (sub-state 0)
///   or the grid (1); the sale screen tests it in every sub-state. Both offer
///   the press to the item-info box first in sub-state 1.
/// * On the current page: in sub-state 0 the cursor enters the grid.
/// * On another page that exists: select it, cursor into the grid at cell 0,
///   and clear `gv + 0x198B`.
/// * The sale screen also clears `gv + 0x198B` on any tab press (0x111D3C).
fn screen_touch(vm: &mut Vm, sc: &Screen, any_sub_state: bool, clear_latch_always: bool) -> Result<bool, String> {
    if lr(vm) == RETURN_MAGIC {
        return Ok(false);
    }
    let this = vm.arg(0);
    let (x, y) = (vm.arg(1) as i32, vm.arg(2) as i32);
    let get_sub = vm.symbol(sc.get_sub).ok_or("ten pages: GetSubState vanished")?;
    let sub = vm.call(get_sub, &[this])? as u8 as i8 as i32;
    if !any_sub_state && sub != 0 && sub != 1 {
        return Ok(false);
    }
    let (ox, oy) = menu_origin(vm)?;
    let Some(hit) = column_press(x - ox, y - oy) else { return Ok(false) };
    if sub == 1 && auto_info_took(vm, x, y)? {
        return Ok(true);
    }
    let Some(i) = hit else { return Ok(true) };
    let gv = vm.symbol(GV).ok_or("ten pages: gv vanished")?;
    let set_sub = vm.symbol(sc.set_sub).ok_or("ten pages: SetSubState vanished")?;
    let page = vm.mem.read_u8(this + sc.page_at) as i8 as i32;
    if i == page {
        if sub == 0 {
            vm.call(set_sub, &[this, 1])?;
            vm.mem.write_bytes(this + sc.cell_at, &[0]);
        }
    } else if i < bag_pages(vm) {
        vm.mem.write_bytes(this + sc.page_at, &[i as u8]);
        vm.call(set_sub, &[this, 1])?;
        vm.mem.write_bytes(this + sc.cell_at, &[0]);
        vm.mem.write_bytes(gv + GV_TOUCH_LATCH, &[0]);
    }
    if clear_latch_always {
        vm.mem.write_bytes(gv + GV_TOUCH_LATCH, &[0]);
    }
    vm.pages10.taps += 1;
    Ok(true)
}

fn bag_touch_hook(vm: &mut Vm) -> Result<bool, String> {
    screen_touch(vm, &BAG_MENU, false, false)
}

fn sale_touch_hook(vm: &mut Vm) -> Result<bool, String> {
    screen_touch(vm, &SALE_MENU, true, true)
}

/// Left and right in a tab column: across to the other column, or decline
/// (right with nothing to the right enters the grid, as before).
fn column_key(vm: &mut Vm, key: u32, page_at: u32, this: u32, pages: i32) -> Result<bool, String> {
    let f = vm.symbol(NUM_KEY_TO_NAVI).ok_or("ten pages: NumKeyToNaviKey vanished")?;
    let navi = vm.call(f, &[key])? as i32;
    if navi != KEY_LEFT && navi != KEY_RIGHT {
        return Ok(false);
    }
    let page = vm.mem.read_u8(this + page_at) as i8 as i32;
    let Some(to) = tab_step(page, pages, navi) else { return Ok(false) };
    vm.mem.write_bytes(this + page_at, &[to as u8]);
    vm.pages10.keys += 1;
    Ok(true)
}

/// `KeyStorage(this, key)`: sub-state 0 is the warehouse's tab column, 2 the
/// bag view's; 1 and 3 are their grids.
fn key_hook(vm: &mut Vm) -> Result<bool, String> {
    if lr(vm) == RETURN_MAGIC {
        return Ok(false);
    }
    let this = vm.arg(0);
    let key = vm.arg(1);
    let sub_state = vm.mem.read_u8(this + NM_SUB_STATE) as i8 as i32;
    if sub_state != 0 && sub_state != 2 {
        return Ok(false);
    }
    let n = pages(vm, this, sub_state == 0);
    column_key(vm, key, NM_PAGE, this, n)
}

/// `KeyBagMenu` / `KeyItemSale`: sub-state 0 is the tab column in both.
fn screen_key(vm: &mut Vm, sc: &Screen) -> Result<bool, String> {
    if lr(vm) == RETURN_MAGIC {
        return Ok(false);
    }
    let this = vm.arg(0);
    let key = vm.arg(1);
    let get_sub = vm.symbol(sc.get_sub).ok_or("ten pages: GetSubState vanished")?;
    if vm.call(get_sub, &[this])? as u8 as i8 != 0 {
        return Ok(false);
    }
    let n = bag_pages(vm);
    column_key(vm, key, sc.page_at, this, n)
}

fn bag_key_hook(vm: &mut Vm) -> Result<bool, String> {
    screen_key(vm, &BAG_MENU)
}

fn sale_key_hook(vm: &mut Vm) -> Result<bool, String> {
    screen_key(vm, &SALE_MENU)
}

/// Judge a `--memwatch storage,bag` run against the ten-page layout: every
/// access that could only come from an instruction still using the shipped
/// offsets. Empty means the run found no hole in [`STORAGE`] or [`BAG`].
///
/// Width tells most of it apart — the pointer arrays are words, the
/// stack-count and state arrays bytes. The one case it cannot: a word access
/// in the back half of the bag's pointer array (`+0x1e8..+0x2b8`) is either a
/// pointer past slot 120 or a filter access that kept `+0x1e8`. A filter walk
/// starts at its first entry, so a PC whose lowest touch is exactly `+0x1e8`
/// is reported as a suspect to read — that is how `DrawEquipMenu`'s seven
/// were found.
pub fn check_accesses(w: &crate::memwatch::MemWatch) -> Vec<String> {
    let mut out = Vec::new();
    for ((kind, pc), h) in &w.hits {
        let width = |want: u8| h.widths & !want != 0;
        let (lo, hi) = (h.lo, h.hi);
        let bad = match *kind {
            "storage" => {
                (lo < 0x288 && hi >= 0x8 && lo >= 0x8 && width(4) && hi < 0x288)
                    || (lo >= 0x288 && width(1))
                    || (lo < 0x288 && hi >= 0x288)
            }
            "bag" => {
                let in_ptr = lo >= 0x8 && hi < 0x2b8;
                let in_stack = lo >= 0x2b8 && hi < 0x364;
                (in_ptr && width(4))
                    || (in_stack && width(1))
                    || (lo < 0x2b8 && hi >= 0x2b8 && lo >= 0x8)
                    || (lo >= 0x364 && hi < 0x368)
            }
            _ => false,
        };
        if bad {
            out.push(format!("VIOLATION {kind} pc {pc:#x} [{lo:#x}..{hi:#x}] widths {:#b}", h.widths));
        } else if *kind == "bag" && lo == 0x1e8 && hi < 0x2b8 && h.widths == 4 {
            // A filter access that kept `+0x1e8` starts at the filter's first
            // entry; a pointer accessor that reaches this far started lower.
            out.push(format!("suspect   {kind} pc {pc:#x} [{lo:#x}..{hi:#x}] (an old filter access?)"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_word_carries_its_old_immediate_and_encodes_the_new() {
        for e in STORAGE.iter().chain(RECV).chain(BAG).chain(GRID) {
            assert_eq!(imm_of(e.was), Some(e.old), "{}+{:#x}", e.sym, e.at);
            let now = with_imm(e.was, e.new).unwrap_or_else(|| panic!("{}+{:#x}", e.sym, e.at));
            assert_eq!(imm_of(now), Some(e.new));
            // Only the immediate field may differ: condition, opcode and
            // registers are the shipped instruction's.
            let field = if (e.was >> 25) & 7 == 0b000 { 0xf0f } else { 0xfff };
            assert_eq!(now & !field, e.was & !field, "{}+{:#x}", e.sym, e.at);
        }
    }

    #[test]
    fn no_word_is_rewritten_twice() {
        let mut seen = std::collections::HashSet::new();
        for e in STORAGE.iter().chain(RECV).chain(BAG).chain(GRID) {
            assert!(seen.insert((e.sym, e.at)), "{}+{:#x} twice", e.sym, e.at);
        }
        assert_eq!(BAG.len(), 186);
        assert_eq!(GRID.len(), 23);
    }

    /// Every grid word moves its `x` right by exactly the shift: an `add` of
    /// the origin grows, a `sub` from the half width shrinks.
    #[test]
    fn every_grid_word_moves_x_right_by_the_shift() {
        const SUB: u32 = 0b0010;
        const ADD: u32 = 0b0100;
        for e in GRID {
            let at = format!("{}+{:#x}", e.sym, e.at);
            assert_eq!((e.was >> 25) & 7, 0b001, "{at} is not a data-processing immediate");
            match (e.was >> 21) & 0xf {
                SUB => assert_eq!(e.old - e.new, GRID_SHIFT, "{at}"),
                ADD => assert_eq!(e.new - e.old, GRID_SHIFT, "{at}"),
                op => panic!("{at}: opcode {op:#06b} is neither add nor sub"),
            }
        }
    }

    #[test]
    fn the_bag_layout_fits_the_object_and_the_grown_hero() {
        // ptr[172] from +8, then stack[172], ending before the filter count
        // at +0x368 that stays where it is.
        assert_eq!(8 + BAG_SLOTS * 4, 0x2b8);
        assert!(0x2b8 + BAG_SLOTS <= 0x368);
        // The filter array at the end of a HERO grown to 0x22d0, past the
        // largest subclass (WARRIOR, 0x2020).
        assert_eq!(HERO_BAG + BAG_FILTER_NEW, 0x2020);
        assert_eq!(HERO_BAG + BAG_FILTER_NEW + BAG_FILTER_LEN_NEW, 0x22d0);
        assert_eq!(TAIL_LEN as usize, TAIL_MAGIC.len() + (BAG_SLOTS - SAVED_STACKS) as usize);
    }

    #[test]
    fn the_encoder_round_trips_the_three_forms() {
        // mov r0, #0x1e8 → #0x3c8; ldrb r0, [r1, #0x148]; ldrsb r1, [r0, #0x7d]
        assert_eq!(with_imm(0xe3a0_0f7a, 0x3c8), Some(0xe3a0_0ff2));
        assert_eq!(with_imm(0xe5d1_0148, 0x288), Some(0xe5d1_0288));
        assert_eq!(imm_of(0xe1d0_17dd), Some(0x7d));
        assert_eq!(with_imm(0xe1d0_17dd, 0x5a), Some(0xe1d0_15da));
        // 0x101 has no 8-bit rotation.
        assert_eq!(with_imm(0xe3a0_0f7a, 0x101), None);
    }

    #[test]
    fn the_new_layout_fits_its_allocation() {
        // ptr[160] from +8, then stack[160], then state[160], ending where
        // the allocation does.
        assert_eq!(8 + 160 * 4, 0x288);
        assert_eq!(0x288 + 160, 0x328);
        assert_eq!(0x328 + 160, STORAGE_SIZE_NEW);
        assert_eq!(STORAGE_SLOTS, 160);
    }

    /// The largest `4/6` listing ten pages can produce fits the grown receive
    /// buffer — and five pages of the same never reached the shipped one,
    /// which is why the overrun only arrived with the tenth page.
    #[test]
    fn the_largest_listing_fits_the_receive_buffer() {
        // Per slot: `[i32 slot][u8 state]`, then an equipment `_NET_ITEM_`:
        // `[i32 count][i8 kind][i8 index][u32 len]`, the name, 58 bytes. The
        // name lands in `_NET_ITEM_+0x06..+0x20`, so 26 bytes at most (the
        // longest in the tables is 13, `(진)` makes 17). The frame adds its
        // 20-byte header and `[i32 result][u32 0][i32 count]`.
        let listing = |slots: u32| 20 + 12 + slots * (5 + 10 + 26 + 58);
        let ten = listing(STORAGE_SLOTS as u32);
        assert!(ten <= RECV_BODY_NEW, "{ten} B");
        assert!(ten < 0x8000, "every readNet* cursor is an i16");
        assert!(ten > RECV_BODY_OLD, "the case this exists for");
        assert!(listing(80) <= RECV_BODY_OLD, "five pages always fitted");
        // `mov r0, #0x2000` → `mov r0, #0x5000`.
        assert_eq!(with_imm(RECV[0].was, RECV_BODY_NEW), Some(0xe3a0_0a05));
    }

    #[test]
    fn ten_chests_sit_in_two_columns_left_of_the_moved_grid() {
        const PANEL_W: i32 = 176;
        const GRID_SPAN: i32 = 3 * 29 + 26;
        let grid = GRID_X0 + GRID_SHIFT as i32;
        // The open chest, the widest, in the right column stays off the grid,
        // and the moved grid stays inside the panel.
        assert!(TAB_COL_X[0] > 0);
        assert!(TAB_COL_X[1] >= TAB_COL_X[0] + CHEST_W, "the columns overlap");
        assert!(TAB_COL_X[1] + CHEST_W < grid, "a chest reaches the grid");
        assert!(grid + GRID_SPAN <= PANEL_W, "the grid leaves the panel");
        // Every press left of the grid is the column's; the first cell is not.
        assert_eq!(COLUMN.2, grid);
        for i in 0..10 {
            let (x, y, w, h) = tab_rect(i);
            assert!(x >= COLUMN.0 && x + w <= COLUMN.2, "tab {i} leaves the column");
            assert!(y >= COLUMN.1 && y + h <= COLUMN.3);
            // Its own centre finds it, and it covers the chest the guest
            // draws there: the open one ends one row above the blit's `y`
            // (y0 + 0x3d + 24 row) and starts 22 rows up.
            assert_eq!(tab_at(x + w / 2, y + h / 2), Some(i));
            let blit_y = TAB_Y0 + TAB_STEP * (i % TABS_PER_COL);
            assert!(y <= blit_y - 22 && blit_y <= y + h);
        }
        assert_eq!(tab_rect(0).0, tab_rect(4).0);
        assert_eq!(tab_rect(5).1, tab_rect(0).1, "6 sits beside 1");
        assert_eq!(tab_at(2, 50), None, "the margin left of the chests is no tab");
    }

    #[test]
    fn the_cell_tables_are_the_grid_the_draw_words_move() {
        // The shipped table is the cell boxes `DrawBagMenu` draws at
        // halfW − 0x2c (x0 + 44), 29 apart; the hook adds the same shift the
        // draw words add.
        let xs: Vec<i32> = (0..16).map(shipped_cell_x).collect();
        assert_eq!(&xs[..4], &[44, 73, 102, 131]);
        assert_eq!(&xs[12..], &[44, 73, 102, 131]);
        assert_eq!(0x58 - (0x2b + 1), GRID_X0);
    }

    #[test]
    fn left_and_right_cross_columns_and_right_then_enters_the_grid() {
        assert_eq!(tab_step(0, 10, KEY_RIGHT), Some(5));
        assert_eq!(tab_step(5, 10, KEY_RIGHT), None, "right column → grid");
        assert_eq!(tab_step(5, 10, KEY_LEFT), Some(0));
        assert_eq!(tab_step(0, 10, KEY_LEFT), None);
        // Seven pages: 3 has no neighbour (8 does not exist), 1 has 6.
        assert_eq!(tab_step(3, 7, KEY_RIGHT), None);
        assert_eq!(tab_step(1, 7, KEY_RIGHT), Some(6));
        // Five or fewer: one column, the guest's behaviour unchanged.
        assert_eq!(tab_step(2, 5, KEY_RIGHT), None);
        // The bag view's extra cell under the tabs (page == pages) is the
        // guest's.
        assert_eq!(tab_step(10, 10, KEY_LEFT), None);
    }
}
