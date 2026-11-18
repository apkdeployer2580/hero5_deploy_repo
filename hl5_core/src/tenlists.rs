//! 가방 10페이지 — the filtered lists, widened past 127.
//!
//! ANALYSIS.md §65.11. The bag holds 160 items (172 with the cash-shop
//! overflow), and seven screens show *every* matching item of the bag as one
//! list: 정련, 오브결합 (equipment, then orbs), 도감, and in the in-game menu
//! 장비, 스킬 (the compose list), 정령 and the bag menu's own pickers. Each is a
//! row of five (four for 스킬) icons paged through by a cursor the screen keeps
//! in **a signed byte** — and the refine/orb screens copy the chosen position
//! into another one, `gv+0x1B3E`, where −1 means "nothing chosen". With 96
//! bag slots no list could pass 96. With 172 one can pass 127, and the 128th
//! position reads as −128: on 정련 with 150 equipment the strip goes blank and
//! the cursor never comes back (measured, §65.7).
//!
//! # Not a window — a wider number
//!
//! The other way to reach item 128 is to show the list in pages of fewer than
//! 128 and turn the page where the guest wraps. It was rejected: a chosen
//! position outlives the page it was chosen on (`ApplyItemRefine` rebuilds
//! the list and reads `filter[gv+0x1B3E]` straight after), so a page turn the
//! guest does not know about makes that position name a *different item* —
//! the item that then gets refined, combined or equipped.
//!
//! So the guest's own arithmetic is widened instead. Every position is read
//! **unsigned** and kept in a full register:
//!
//! * `ldrsb` of a position field → `ldrb`;
//! * `lsl #24` / `asr #24` (the compiler's sign extension of a `signed char`)
//!   → a plain move, and an operand `…, rX, asr #24` → `…, rX`;
//! * `and rX, rY, #0xff` (truncation after `±1`, `±5`) → `add rX, rY, #0`;
//! * `tst rX, #0x80` (the "went below zero" test after `−1`) → `tst rX,
//!   #0x80000000`; a compare against `n << 24` → against `n`;
//! * `gv+0x1B3E`'s "nothing chosen" is now `0xFF`: `cmp rX, #0` → `cmp rX,
//!   #0xff`, and the `lt` that followed → `eq`.
//!
//! 412 words: 145 loads, 164 shifts, 10 shifted operands, 9 shifted
//! compares, 52 masks, 11 sign tests, 21 sentinel compares and branches.
//!
//! A shifted compare is unshifted only when its *left* side is a position
//! whose own `lsl #24` became a move. `TouchEquipMenu` and `TouchSpiritMenu`
//! compare the tapped cell `i << 24` (a loop counter, untouched, reloaded
//! from a stack slot) with `cursor % 5, lsl #24`; the first table dropped the
//! right side's shift there too, so "tap the highlighted cell again" held for
//! the first cell of a row only and **equipping by touch stopped working**
//! (2026-10-16, `widen.py` took the empty set for `S24`). Those two words keep
//! their shipped bits — `cursor % 5` is 0..4 whatever the cursor is.
//!
//! Every intermediate value the guest computes from a position stays within
//! `[−8, 176]`, so for a list of 127 or fewer — every list a shipped bag could
//! hold — each of these instructions yields **the same bits it always did**:
//! an `asr #24` of a value in `[−128, 127]` shifted left by 24 is that value,
//! an `and #0xff` followed by a signed read of a value in that range is that
//! value. Past 127 they yield the position instead of its wrapped twin. The
//! stores were always `strb`; a position up to 255 survives them.
//!
//! # Where the table comes from
//!
//! `tools_re/widen.py`. Per screen, every byte load of a `this` field is a
//! seed; the fields that reach an index into the filter array, or meet the
//! filter count in a compare, are that screen's positions (refine/orb `+0x11`,
//! 도감 `+0x14`, 장비 `+0xf`, 스킬 `+0x12`, 정령 and the bag menu `+0x10`), plus
//! `gv+0x1B3E` and the count's low byte. A forward taint analysis follows them
//! through registers and stack slots and prints every instruction whose
//! meaning depends on the sign, with the replacement word. Four checks keep
//! it honest:
//!
//! * **Two seed finders.** `bagscan.py`'s pointer analysis, and a second one
//!   inside the taint pass that follows `this` copies, `GetBagItemPtr`'s
//!   return and constants through stack slots. The first used to forget `sp`
//!   at every `stmib sp, {…}` (stack arguments) — `DrawRefine` then read
//!   `gv+0x1B3E` through a spilled offset nobody saw, and a chosen position of
//!   141 showed a refine cost of 0 where 126 showed 300. Both finders now
//!   agree; a syntactic sweep for byte loads at the field offsets finds
//!   nothing else.
//! * **Whole chains.** An `lsl #24` becomes a move only if *every* reader of
//!   the shifted value is a shape that is rewritten to match; one reader of
//!   another shape keeps the chain as it was. That keeps exactly one chain:
//!   `DrawSpiritMenu`'s spirit-skill page makes `x << 16` as
//!   `(x << 24) asr 8` from a skill index that never passes 8.
//! * **A *must* pass.** Six rewrites are reached on some path by another
//!   value (`TouchSkillMenu`'s scroll arithmetic; the other value is the
//!   skill list's own length) — read by hand.
//! * **The interpreter.** `--memwatch lists` records every PC that reads one
//!   of these fields; `tools_re/listcheck.py` compares them with the seeds.
//!   Across the five acceptance scenes and the six list scenarios below: 95
//!   reading PCs, every one on a list screen a seed.
//!
//! Calls that receive a position as a `signed char` (`CommonUi::DrawScroll`'s
//! row numbers, `GetItemTableInfo`'s index in the skill-table page, the
//! hot-key belt) were read by hand: each gets a row number or a slot of a list
//! that cannot pass 43. `tools_re/checkwiden.py` decoded all 412 new words back.
//!
//! # What it was checked with
//!
//! * The five acceptance scenes, ten pages on, table applied vs not: **1,241
//!   frames byte-identical** — the "same bits up to 127" claim, measured.
//! * 정련, 144 entries: right through 126..143 and around to 0, left from 0
//!   to 143; choosing 141 shows the same screen pixel for pixel as choosing
//!   126 (the same item); refining 141 raises bag slot 157 and eats one stone.
//! * 오브결합, 153 entries: choosing 129 and combining puts orbs into bag
//!   slot 130 (`03 000000` → `03 010000` → `03 010100`); the fourth try's
//!   destruction removes exactly that item and resets the choice to 0xFF.
//! * 장비, 151 entries: the highlighted entry is the one equipped — 140
//!   (bag slot 143). (Two earlier runs opened the list at 1, so 140 presses
//!   landed on 141 and equipped that; read once as an off-by-one, it was not.)
//!   Those runs used keys. By touch — highlight a cell, tap it again — the
//!   second cell of the weapon list equips (`equip [0, …]` → `[4, …]`, the
//!   same as with ten pages off); it did not before the fix above.
//! * 소켓확장 (the bag menu's picker), 152 entries: expanding 140 grows bag
//!   slot 142 (`00 ff…` → `01 00ff…`).
//! * Not reached by a replay: the 스킬 compose list (needs a learned skill)
//!   and the 정령 equipment lists (need spirit gear) — static only.
//!
//! The fields are reset when a menu is entered (`StateInNpcMenu::SetState`
//! clears `+0xe..+0x12`, `StateInGameMenu::SetState` `+0xd..+0x13`), so a
//! position of 150 left in one screen never reaches another screen that uses
//! the same byte for something small.

use crate::Vm;

/// A whole instruction word replaced — unlike [`crate::tenpages::Imm`], the
/// opcode or condition changes, not only an immediate.
pub(crate) struct Word {
    pub sym: &'static str,
    pub at: u32,
    pub was: u32,
    pub new: u32,
}

const fn wd(sym: &'static str, at: u32, was: u32, new: u32) -> Word {
    Word { sym, at, was, new }
}

const KEY_REFINE: &str = "_ZN14StateInNpcMenu9KeyRefineEih";
const TOUCH_REFINE: &str = "_ZN14StateInNpcMenu11TouchRefineEii";
const DRAW_REFINE: &str = "_ZN14StateInNpcMenu10DrawRefineEh";
const KEY_ORB: &str = "_ZN14StateInNpcMenu13KeyOrbCombineEi";
const TOUCH_ORB: &str = "_ZN14StateInNpcMenu15TouchOrbCombineEii";
const DRAW_ORB: &str = "_ZN14StateInNpcMenu14DrawOrbCombineEv";
const KEY_COLLECTION: &str = "_ZN14StateInNpcMenu13KeyCollectionEi";
const TOUCH_COLLECTION: &str = "_ZN14StateInNpcMenu15TouchCollectionEii";
const DRAW_COLLECTION: &str = "_ZN14StateInNpcMenu14DrawCollectionEv";
const NPC_PROCESS: &str = "_ZN14StateInNpcMenu7ProcessEv";
const KEY_EQUIP: &str = "_ZN15StateInGameMenu12KeyEquipMenuEi";
const TOUCH_EQUIP: &str = "_ZN15StateInGameMenu14TouchEquipMenuEii";
const DRAW_EQUIP: &str = "_ZN15StateInGameMenu13DrawEquipMenuEv";
const KEY_SKILL: &str = "_ZN15StateInGameMenu12KeySkillMenuEi";
const TOUCH_SKILL: &str = "_ZN15StateInGameMenu14TouchSkillMenuEii";
const DRAW_SKILL: &str = "_ZN15StateInGameMenu13DrawSkillMenuEv";
const KEY_SPIRIT: &str = "_ZN15StateInGameMenu13KeySpiritMenuEi";
const TOUCH_SPIRIT: &str = "_ZN15StateInGameMenu15TouchSpiritMenuEii";
const DRAW_SPIRIT: &str = "_ZN15StateInGameMenu14DrawSpiritMenuEv";
const KEY_BAG: &str = "_ZN15StateInGameMenu10KeyBagMenuEi";
const TOUCH_BAG: &str = "_ZN15StateInGameMenu12TouchBagMenuEii";
const DRAW_BAG: &str = "_ZN15StateInGameMenu11DrawBagMenuEv";
const GAME_PROCESS: &str = "_ZN15StateInGameMenu7ProcessEv";
const APPLY_REFINE: &str = "_ZN10RefineItem15ApplyItemRefineEP13EquipItemInfosaihPP8ItemInfo";
const APPLY_ORB: &str = "_ZN10RefineItem15ApplyOrbCombineEP13EquipItemInfosaaPP8ItemInfo";

/// Generated by `python tools_re/widen.py payload/libHeroesLore5.so --rust`.
pub(crate) const WIDEN: &[Word] = &[
    // _ZN10RefineItem15ApplyOrbCombineEP13EquipItemInfosaaPP8ItemInfo
    wd(APPLY_ORB, 0x220, 0xe198_30da, 0xe7d8_300a), // ldrsb -> ldrb gv+0x1b3e
    wd(APPLY_ORB, 0x23c, 0xe198_30da, 0xe7d8_300a), // ldrsb -> ldrb gv+0x1b3e
    // _ZN10RefineItem15ApplyItemRefineEP13EquipItemInfosaihPP8ItemInfo
    wd(APPLY_REFINE, 0x184, 0xe19a_30d9, 0xe7da_3009), // ldrsb -> ldrb gv+0x1b3e
    wd(APPLY_REFINE, 0x1a0, 0xe19a_30d9, 0xe7da_3009), // ldrsb -> ldrb gv+0x1b3e
    // _ZN15StateInGameMenu14DrawSpiritMenuEv
    wd(DRAW_SPIRIT, 0x3e8, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x404, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x408, 0xe155_0c42, 0xe155_0002), // opsx: operand asr/lsr #24 -> plain
    wd(DRAW_SPIRIT, 0x47c, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x490, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x494, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x4bc, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x4d0, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x4d4, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x534, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x7fc, 0xe1d6_81d0, 0xe5d6_8010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x994, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x9c0, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0xe48, 0xe1d0_31d0, 0xe5d0_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x1068, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x1070, 0xe1a0_1c41, 0xe1a0_1001), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x184c, 0xe1a0_cc0c, 0xe1a0_c00c), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x1854, 0xe1a0_3c4c, 0xe1a0_300c), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x18c4, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x18cc, 0xe1a0_1c41, 0xe1a0_1001), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x18f4, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x18fc, 0xe1a0_1c41, 0xe1a0_1001), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x191c, 0xe1a0_bc0b, 0xe1a0_b00b), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x1934, 0xe1a0_bc4b, 0xe1a0_b00b), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x19b4, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x19b8, 0xe1a0_6c06, 0xe1a0_6006), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x19bc, 0xe1a0_1c41, 0xe1a0_1001), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x19c8, 0xe1a0_6c46, 0xe1a0_6006), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x1ad0, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x1ad4, 0xe1a0_3c42, 0xe1a0_3002), // sx: asr #24 -> mov
    wd(DRAW_SPIRIT, 0x1b28, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x1b2c, 0xe353_0402, 0xe353_0002), // cmpS: #0x2000000 -> #0x2
    wd(DRAW_SPIRIT, 0x1b4c, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x2530, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x2a28, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_SPIRIT, 0x2a50, 0xe1a0_2c03, 0xe1a0_2003), // sx: lsl #24 -> mov
    wd(DRAW_SPIRIT, 0x2a58, 0xe1a0_2c42, 0xe1a0_2002), // sx: asr #24 -> mov
    // _ZN15StateInGameMenu13DrawSkillMenuEv
    wd(DRAW_SKILL, 0xe00, 0xe1d6_31d2, 0xe5d6_3012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0xe18, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_SKILL, 0xe1c, 0xe155_0c43, 0xe155_0003), // opsx: operand asr/lsr #24 -> plain
    wd(DRAW_SKILL, 0xe88, 0xe1d6_31d2, 0xe5d6_3012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0xe98, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_SKILL, 0xe9c, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_SKILL, 0xec4, 0xe1d6_31d2, 0xe5d6_3012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0xed4, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_SKILL, 0xed8, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_SKILL, 0xf20, 0xe1d6_31d2, 0xe5d6_3012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0xf3c, 0xe1d6_31d2, 0xe5d6_3012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0x1014, 0xe1d6_61d2, 0xe5d6_6012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0x1510, 0xe1d6_01d2, 0xe5d6_0012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0x159c, 0xe1d6_31d2, 0xe5d6_3012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0x1734, 0xe207_70ff, 0xe287_7000), // and255: and #0xff -> add #0
    wd(DRAW_SKILL, 0x1824, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_SKILL, 0x1828, 0xe152_0c43, 0xe152_0003), // opsx: operand asr/lsr #24 -> plain
    wd(DRAW_SKILL, 0x1840, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_SKILL, 0x1ef0, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_SKILL, 0x1ef8, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_SKILL, 0x214c, 0xe1d6_31d2, 0xe5d6_3012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0x2164, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_SKILL, 0x2168, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_SKILL, 0x21d0, 0xe1d6_01d2, 0xe5d6_0012), // ldrsb -> ldrb THIS+0x12
    wd(DRAW_SKILL, 0x24d4, 0xe1d6_21d2, 0xe5d6_2012), // ldrsb -> ldrb THIS+0x12
    // _ZN15StateInGameMenu13DrawEquipMenuEv
    wd(DRAW_EQUIP, 0xf2c, 0xe1d5_30df, 0xe5d5_300f), // ldrsb -> ldrb THIS+0xf
    wd(DRAW_EQUIP, 0xf48, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(DRAW_EQUIP, 0xf4c, 0xe156_0c42, 0xe156_0002), // opsx: operand asr/lsr #24 -> plain
    wd(DRAW_EQUIP, 0xfc8, 0xe1d5_30df, 0xe5d5_300f), // ldrsb -> ldrb THIS+0xf
    wd(DRAW_EQUIP, 0xfdc, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_EQUIP, 0xfe0, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_EQUIP, 0x1004, 0xe1d5_30df, 0xe5d5_300f), // ldrsb -> ldrb THIS+0xf
    wd(DRAW_EQUIP, 0x1020, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_EQUIP, 0x1024, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_EQUIP, 0x1088, 0xe1d5_30df, 0xe5d5_300f), // ldrsb -> ldrb THIS+0xf
    wd(DRAW_EQUIP, 0x16dc, 0xe1d5_70df, 0xe5d5_700f), // ldrsb -> ldrb THIS+0xf
    wd(DRAW_EQUIP, 0x1824, 0xe1d5_30df, 0xe5d5_300f), // ldrsb -> ldrb THIS+0xf
    wd(DRAW_EQUIP, 0x1850, 0xe1d5_30df, 0xe5d5_300f), // ldrsb -> ldrb THIS+0xf
    wd(DRAW_EQUIP, 0x1ce0, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(DRAW_EQUIP, 0x1ce4, 0xe1a0_3c42, 0xe1a0_3002), // sx: asr #24 -> mov
    wd(DRAW_EQUIP, 0x1d38, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_EQUIP, 0x1d3c, 0xe353_0402, 0xe353_0002), // cmpS: #0x2000000 -> #0x2
    wd(DRAW_EQUIP, 0x1d60, 0xe1d5_30df, 0xe5d5_300f), // ldrsb -> ldrb THIS+0xf
    wd(DRAW_EQUIP, 0x1f58, 0xe1d5_30df, 0xe5d5_300f), // ldrsb -> ldrb THIS+0xf
    wd(DRAW_EQUIP, 0x1f80, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(DRAW_EQUIP, 0x1f88, 0xe1a0_2c42, 0xe1a0_2002), // sx: asr #24 -> mov
    wd(DRAW_EQUIP, 0x2018, 0xe1d5_30df, 0xe5d5_300f), // ldrsb -> ldrb THIS+0xf
    // _ZN15StateInGameMenu11DrawBagMenuEv
    wd(DRAW_BAG, 0x158, 0xe1d5_31d0, 0xe5d5_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_BAG, 0x170, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_BAG, 0x174, 0xe157_0c43, 0xe157_0003), // opsx: operand asr/lsr #24 -> plain
    wd(DRAW_BAG, 0x1dc, 0xe1d5_31d0, 0xe5d5_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_BAG, 0x1ec, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_BAG, 0x1f0, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_BAG, 0x214, 0xe1d5_31d0, 0xe5d5_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_BAG, 0x230, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_BAG, 0x234, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_BAG, 0x294, 0xe1d5_31d0, 0xe5d5_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_BAG, 0x370, 0xe1d5_31d0, 0xe5d5_3010), // ldrsb -> ldrb THIS+0x10
    wd(DRAW_BAG, 0xa44, 0xe1d5_e1d0, 0xe5d5_e010), // ldrsb -> ldrb THIS+0x10
    // _ZN15StateInGameMenu15TouchSpiritMenuEii
    wd(TOUCH_SPIRIT, 0x2e0, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(TOUCH_SPIRIT, 0x2e8, 0xe1a0_0c41, 0xe1a0_0001), // sx: asr #24 -> mov
    wd(TOUCH_SPIRIT, 0x320, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(TOUCH_SPIRIT, 0x5f8, 0xe201_10ff, 0xe281_1000), // and255: and #0xff -> add #0
    wd(TOUCH_SPIRIT, 0x5fc, 0xe1a0_0c01, 0xe1a0_0001), // sx: lsl #24 -> mov
    wd(TOUCH_SPIRIT, 0x604, 0xe1b0_1c40, 0xe1b0_1000), // sx: asrs #24 -> mov
    wd(TOUCH_SPIRIT, 0x620, 0xe201_30ff, 0xe281_3000), // and255: and #0xff -> add #0
    wd(TOUCH_SPIRIT, 0x870, 0xe1a0_ec0e, 0xe1a0_e00e), // sx: lsl #24 -> mov
    wd(TOUCH_SPIRIT, 0x878, 0xe1a0_0c4e, 0xe1a0_000e), // sx: asr #24 -> mov
    wd(TOUCH_SPIRIT, 0x8dc, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(TOUCH_SPIRIT, 0x8e4, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(TOUCH_SPIRIT, 0x8fc, 0xe1d6_01d0, 0xe5d6_0010), // ldrsb -> ldrb THIS+0x10
    wd(TOUCH_SPIRIT, 0xa5c, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(TOUCH_SPIRIT, 0xa60, 0xe1a0_0c03, 0xe1a0_0003), // sx: lsl #24 -> mov
    wd(TOUCH_SPIRIT, 0xa68, 0xe1a0_0c40, 0xe1a0_0000), // sx: asr #24 -> mov
    // _ZN15StateInGameMenu14TouchSkillMenuEii
    wd(TOUCH_SKILL, 0x914, 0xe201_10ff, 0xe281_1000), // and255: and #0xff -> add #0
    wd(TOUCH_SKILL, 0x918, 0xe1a0_2c01, 0xe1a0_2001), // sx: lsl #24 -> mov
    wd(TOUCH_SKILL, 0x920, 0xe1b0_1c42, 0xe1b0_1002), // sx: asrs #24 -> mov
    wd(TOUCH_SKILL, 0x93c, 0xe202_30ff, 0xe282_3000), // and255: and #0xff -> add #0
    wd(TOUCH_SKILL, 0x9a8, 0xe1d5_21d2, 0xe5d5_2012), // ldrsb -> ldrb THIS+0x12
    wd(TOUCH_SKILL, 0xaac, 0xe202_20ff, 0xe282_2000), // and255: and #0xff -> add #0 (may)
    wd(TOUCH_SKILL, 0xab0, 0x4203_30ff, 0x4283_3000), // and255: and #0xff -> add #0 (may)
    wd(TOUCH_SKILL, 0xabc, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0 (may)
    wd(TOUCH_SKILL, 0xac0, 0xe1a0_1c03, 0xe1a0_1003), // sx: lsl #24 -> mov (may)
    wd(TOUCH_SKILL, 0xac4, 0xe351_0407, 0xe351_0007), // cmpS: #0x7000000 -> #0x7 (may)
    wd(TOUCH_SKILL, 0xad0, 0xc203_30ff, 0xc283_3000), // and255: and #0xff -> add #0 (may)
    wd(TOUCH_SKILL, 0xcf0, 0xe1a0_0c00, 0xe1a0_0000), // sx: lsl #24 -> mov
    wd(TOUCH_SKILL, 0xcf4, 0xe1a0_2c40, 0xe1a0_2000), // sx: asr #24 -> mov
    wd(TOUCH_SKILL, 0xd0c, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(TOUCH_SKILL, 0xd10, 0xe152_0c01, 0xe152_0001), // cmpS: operand lsl #24 -> plain
    wd(TOUCH_SKILL, 0xd48, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(TOUCH_SKILL, 0xd50, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(TOUCH_SKILL, 0xd68, 0xe1d5_31d2, 0xe5d5_3012), // ldrsb -> ldrb THIS+0x12
    wd(TOUCH_SKILL, 0xe68, 0xe202_20ff, 0xe282_2000), // and255: and #0xff -> add #0
    wd(TOUCH_SKILL, 0xe70, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(TOUCH_SKILL, 0xe74, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(TOUCH_SKILL, 0xe80, 0x1203_30ff, 0x1283_3000), // and255: and #0xff -> add #0
    wd(TOUCH_SKILL, 0xef8, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(TOUCH_SKILL, 0xefc, 0xe1a0_1c42, 0xe1a0_1002), // sx: asr #24 -> mov
    wd(TOUCH_SKILL, 0xf20, 0xe1d5_21d2, 0xe5d5_2012), // ldrsb -> ldrb THIS+0x12
    wd(TOUCH_SKILL, 0x1094, 0xe202_20ff, 0xe282_2000), // and255: and #0xff -> add #0
    wd(TOUCH_SKILL, 0x1098, 0xe1a0_0c02, 0xe1a0_0002), // sx: lsl #24 -> mov
    wd(TOUCH_SKILL, 0x10a0, 0xe1a0_0c40, 0xe1a0_0000), // sx: asr #24 -> mov
    // _ZN15StateInGameMenu14TouchEquipMenuEii
    wd(TOUCH_EQUIP, 0x2fc, 0xe1a0_ec0e, 0xe1a0_e00e), // sx: lsl #24 -> mov
    wd(TOUCH_EQUIP, 0x304, 0xe1a0_0c4e, 0xe1a0_000e), // sx: asr #24 -> mov
    wd(TOUCH_EQUIP, 0x368, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(TOUCH_EQUIP, 0x370, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(TOUCH_EQUIP, 0x388, 0xe1d6_00df, 0xe5d6_000f), // ldrsb -> ldrb THIS+0xf
    wd(TOUCH_EQUIP, 0x464, 0xe201_10ff, 0xe281_1000), // and255: and #0xff -> add #0
    wd(TOUCH_EQUIP, 0x468, 0xe1a0_0c01, 0xe1a0_0001), // sx: lsl #24 -> mov
    wd(TOUCH_EQUIP, 0x470, 0xe1b0_1c40, 0xe1b0_1000), // sx: asrs #24 -> mov
    wd(TOUCH_EQUIP, 0x48c, 0xe201_30ff, 0xe281_3000), // and255: and #0xff -> add #0
    wd(TOUCH_EQUIP, 0x550, 0xe1a0_cc0c, 0xe1a0_c00c), // sx: lsl #24 -> mov
    wd(TOUCH_EQUIP, 0x558, 0xe1a0_ec4c, 0xe1a0_e00c), // sx: asr #24 -> mov
    wd(TOUCH_EQUIP, 0x57c, 0xe1d6_30df, 0xe5d6_300f), // ldrsb -> ldrb THIS+0xf
    wd(TOUCH_EQUIP, 0x610, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(TOUCH_EQUIP, 0x614, 0xe1a0_0c03, 0xe1a0_0003), // sx: lsl #24 -> mov
    wd(TOUCH_EQUIP, 0x61c, 0xe1a0_0c40, 0xe1a0_0000), // sx: asr #24 -> mov
    // _ZN15StateInGameMenu12TouchBagMenuEii
    wd(TOUCH_BAG, 0x6a8, 0xe1d3_31d0, 0xe5d3_3010), // ldrsb -> ldrb THIS+0x10
    wd(TOUCH_BAG, 0x730, 0xe1d3_01d0, 0xe5d3_0010), // ldrsb -> ldrb THIS+0x10
    wd(TOUCH_BAG, 0x844, 0xe201_10ff, 0xe281_1000), // and255: and #0xff -> add #0
    wd(TOUCH_BAG, 0x848, 0xe1a0_2c01, 0xe1a0_2001), // sx: lsl #24 -> mov
    wd(TOUCH_BAG, 0x850, 0xe1b0_0c42, 0xe1b0_0002), // sx: asrs #24 -> mov
    wd(TOUCH_BAG, 0xae4, 0xe1a0_0c00, 0xe1a0_0000), // sx: lsl #24 -> mov
    wd(TOUCH_BAG, 0xae8, 0xe1a0_0c40, 0xe1a0_0000), // sx: asr #24 -> mov
    wd(TOUCH_BAG, 0xb18, 0xe1de_31d0, 0xe5de_3010), // ldrsb -> ldrb THIS+0x10
    wd(TOUCH_BAG, 0xba8, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(TOUCH_BAG, 0xbac, 0xe1a0_0c03, 0xe1a0_0003), // sx: lsl #24 -> mov
    wd(TOUCH_BAG, 0xbb4, 0xe1a0_0c40, 0xe1a0_0000), // sx: asr #24 -> mov
    // _ZN15StateInGameMenu13KeySpiritMenuEi
    wd(KEY_SPIRIT, 0x298, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(KEY_SPIRIT, 0x2e0, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    wd(KEY_SPIRIT, 0x5d0, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_SPIRIT, 0x5d4, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_SPIRIT, 0x690, 0xe1a0_2c03, 0xe1a0_2003), // sx: lsl #24 -> mov
    wd(KEY_SPIRIT, 0x694, 0xe080_2c42, 0xe080_2002), // opsx: operand asr/lsr #24 -> plain
    wd(KEY_SPIRIT, 0x6a8, 0xe1a0_1c03, 0xe1a0_1003), // sx: lsl #24 -> mov
    wd(KEY_SPIRIT, 0x6b0, 0xe1a0_1c41, 0xe1a0_1001), // sx: asr #24 -> mov
    wd(KEY_SPIRIT, 0x710, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_SPIRIT, 0x738, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_SPIRIT, 0x754, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_SPIRIT, 0x758, 0xe1a0_2c03, 0xe1a0_2003), // sx: lsl #24 -> mov
    wd(KEY_SPIRIT, 0x75c, 0xe352_040f, 0xe352_000f), // cmpS: #0xf000000 -> #0xf
    wd(KEY_SPIRIT, 0x79c, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_SPIRIT, 0x7a0, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_SPIRIT, 0x7dc, 0xe20a_a0ff, 0xe28a_a000), // and255: and #0xff -> add #0
    wd(KEY_SPIRIT, 0x8dc, 0xe1d6_31d0, 0xe5d6_3010), // ldrsb -> ldrb THIS+0x10
    // _ZN15StateInGameMenu12KeySkillMenuEi
    wd(KEY_SKILL, 0x8b8, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_SKILL, 0x8bc, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_SKILL, 0x948, 0xe1d5_31d2, 0xe5d5_3012), // ldrsb -> ldrb THIS+0x12
    wd(KEY_SKILL, 0x988, 0xe1d5_31d2, 0xe5d5_3012), // ldrsb -> ldrb THIS+0x12
    wd(KEY_SKILL, 0x9c4, 0xe1d5_21d2, 0xe5d5_2012), // ldrsb -> ldrb THIS+0x12
    wd(KEY_SKILL, 0x12e0, 0xe202_20ff, 0xe282_2000), // and255: and #0xff -> add #0
    wd(KEY_SKILL, 0x12f4, 0xe312_0080, 0xe312_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_SKILL, 0x1328, 0xe202_20ff, 0xe282_2000), // and255: and #0xff -> add #0
    wd(KEY_SKILL, 0x1340, 0xe1a0_ec02, 0xe1a0_e002), // sx: lsl #24 -> mov
    wd(KEY_SKILL, 0x1348, 0xe15c_0c4e, 0xe15c_000e), // opsx: operand asr/lsr #24 -> plain
    wd(KEY_SKILL, 0x137c, 0xe202_20ff, 0xe282_2000), // and255: and #0xff -> add #0
    wd(KEY_SKILL, 0x13b8, 0xe202_20ff, 0xe282_2000), // and255: and #0xff -> add #0
    wd(KEY_SKILL, 0x13f0, 0xe1d5_31d2, 0xe5d5_3012), // ldrsb -> ldrb THIS+0x12
    wd(KEY_SKILL, 0x17f4, 0xe1d5_31d2, 0xe5d5_3012), // ldrsb -> ldrb THIS+0x12
    wd(KEY_SKILL, 0x182c, 0xe1d5_21d2, 0xe5d5_2012), // ldrsb -> ldrb THIS+0x12
    wd(KEY_SKILL, 0x19e0, 0xe208_80ff, 0xe288_8000), // and255: and #0xff -> add #0
    // _ZN15StateInGameMenu12KeyEquipMenuEi
    wd(KEY_EQUIP, 0x1f0, 0xe1d7_30df, 0xe5d7_300f), // ldrsb -> ldrb THIS+0xf
    wd(KEY_EQUIP, 0x240, 0xe1d7_c0df, 0xe5d7_c00f), // ldrsb -> ldrb THIS+0xf
    wd(KEY_EQUIP, 0x350, 0xe1d7_30df, 0xe5d7_300f), // ldrsb -> ldrb THIS+0xf
    wd(KEY_EQUIP, 0x394, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_EQUIP, 0x398, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_EQUIP, 0x3d4, 0xe20a_a0ff, 0xe28a_a000), // and255: and #0xff -> add #0
    // _ZN15StateInGameMenu10KeyBagMenuEi
    wd(KEY_BAG, 0xcc, 0xe1a0_1c09, 0xe1a0_1009), // sx: lsl #24 -> mov
    wd(KEY_BAG, 0xd4, 0xe1a0_1c41, 0xe1a0_1001), // sx: asr #24 -> mov
    wd(KEY_BAG, 0xf8, 0xe1a0_1c0b, 0xe1a0_100b), // sx: lsl #24 -> mov
    wd(KEY_BAG, 0x100, 0xe1a0_1c41, 0xe1a0_1001), // sx: asr #24 -> mov
    wd(KEY_BAG, 0x18c, 0xe1a0_1c0a, 0xe1a0_100a), // sx: lsl #24 -> mov
    wd(KEY_BAG, 0x1a0, 0xe1a0_1c41, 0xe1a0_1001), // sx: asr #24 -> mov
    wd(KEY_BAG, 0x4b0, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_BAG, 0x4b4, 0xe1a0_2c03, 0xe1a0_2003), // sx: lsl #24 -> mov
    wd(KEY_BAG, 0x4b8, 0xe352_0301, 0xe352_0004), // cmpS: #0x4000000 -> #0x4
    wd(KEY_BAG, 0x580, 0xe1d5_31d0, 0xe5d5_3010), // ldrsb -> ldrb THIS+0x10
    wd(KEY_BAG, 0x5b0, 0xe1d5_31d0, 0xe5d5_3010), // ldrsb -> ldrb THIS+0x10
    wd(KEY_BAG, 0x62c, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_BAG, 0x630, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_BAG, 0x66c, 0xe208_80ff, 0xe288_8000), // and255: and #0xff -> add #0
    wd(KEY_BAG, 0xb9c, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_BAG, 0xba0, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_BAG, 0xbe4, 0xe1a0_1c08, 0xe1a0_1008), // sx: lsl #24 -> mov
    wd(KEY_BAG, 0xbf0, 0xe1a0_1c41, 0xe1a0_1001), // sx: asr #24 -> mov
    // _ZN15StateInGameMenu7ProcessEv
    wd(GAME_PROCESS, 0xb08, 0xe1d4_31d0, 0xe5d4_3010), // ldrsb -> ldrb THIS+0x10
    wd(GAME_PROCESS, 0xcc8, 0xe1d4_01d0, 0xe5d4_0010), // ldrsb -> ldrb THIS+0x10
    wd(GAME_PROCESS, 0xd2c, 0xe1d4_21d0, 0xe5d4_2010), // ldrsb -> ldrb THIS+0x10
    wd(GAME_PROCESS, 0xd50, 0xe1a0_3c02, 0xe1a0_3002), // sx: lsl #24 -> mov
    wd(GAME_PROCESS, 0xd54, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(GAME_PROCESS, 0x1364, 0xe1d4_01d0, 0xe5d4_0010), // ldrsb -> ldrb THIS+0x10
    wd(GAME_PROCESS, 0x1470, 0xe1d4_31d0, 0xe5d4_3010), // ldrsb -> ldrb THIS+0x10
    wd(GAME_PROCESS, 0x1700, 0xe1d4_31d0, 0xe5d4_3010), // ldrsb -> ldrb THIS+0x10
    wd(GAME_PROCESS, 0x1778, 0xe1d4_21d0, 0xe5d4_2010), // ldrsb -> ldrb THIS+0x10
    // _ZN14StateInNpcMenu15TouchCollectionEii
    wd(TOUCH_COLLECTION, 0x8d8, 0xe1a0_0c00, 0xe1a0_0000), // sx: lsl #24 -> mov
    wd(TOUCH_COLLECTION, 0x8dc, 0xe1a0_1c40, 0xe1a0_1000), // sx: asr #24 -> mov
    wd(TOUCH_COLLECTION, 0x8f4, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(TOUCH_COLLECTION, 0x8f8, 0xe151_0c02, 0xe151_0002), // cmpS: operand lsl #24 -> plain
    wd(TOUCH_COLLECTION, 0x92c, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(TOUCH_COLLECTION, 0x934, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(TOUCH_COLLECTION, 0x948, 0xe1d6_31d4, 0xe5d6_3014), // ldrsb -> ldrb THIS+0x14
    wd(TOUCH_COLLECTION, 0xa64, 0xe201_10ff, 0xe281_1000), // and255: and #0xff -> add #0
    wd(TOUCH_COLLECTION, 0xa68, 0xe1a0_2c01, 0xe1a0_2001), // sx: lsl #24 -> mov
    wd(TOUCH_COLLECTION, 0xa70, 0xe1b0_1c42, 0xe1b0_1002), // sx: asrs #24 -> mov
    wd(TOUCH_COLLECTION, 0xa8c, 0xe202_30ff, 0xe282_3000), // and255: and #0xff -> add #0
    wd(TOUCH_COLLECTION, 0xbd0, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(TOUCH_COLLECTION, 0xbd4, 0xe1a0_1c42, 0xe1a0_1002), // sx: asr #24 -> mov
    wd(TOUCH_COLLECTION, 0xbfc, 0xe1d6_21d4, 0xe5d6_2014), // ldrsb -> ldrb THIS+0x14
    wd(TOUCH_COLLECTION, 0xdd8, 0xe202_20ff, 0xe282_2000), // and255: and #0xff -> add #0
    wd(TOUCH_COLLECTION, 0xddc, 0xe1a0_0c02, 0xe1a0_0002), // sx: lsl #24 -> mov
    wd(TOUCH_COLLECTION, 0xde4, 0xe1a0_0c40, 0xe1a0_0000), // sx: asr #24 -> mov
    // _ZN14StateInNpcMenu15TouchOrbCombineEii
    wd(TOUCH_ORB, 0x364, 0xe1a0_ec0e, 0xe1a0_e00e), // sx: lsl #24 -> mov
    wd(TOUCH_ORB, 0x36c, 0xe1a0_ec4e, 0xe1a0_e00e), // sx: asr #24 -> mov
    wd(TOUCH_ORB, 0x398, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(TOUCH_ORB, 0x39c, 0xe151_0c00, 0xe151_0000), // cmpS: operand lsl #24 -> plain
    wd(TOUCH_ORB, 0x3d4, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(TOUCH_ORB, 0x3dc, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(TOUCH_ORB, 0x3f4, 0xe1d7_31d1, 0xe5d7_3011), // ldrsb -> ldrb THIS+0x11
    wd(TOUCH_ORB, 0x514, 0xe201_10ff, 0xe281_1000), // and255: and #0xff -> add #0
    wd(TOUCH_ORB, 0x518, 0xe1a0_0c01, 0xe1a0_0001), // sx: lsl #24 -> mov
    wd(TOUCH_ORB, 0x520, 0xe1b0_1c40, 0xe1b0_1000), // sx: asrs #24 -> mov
    wd(TOUCH_ORB, 0x53c, 0xe201_30ff, 0xe281_3000), // and255: and #0xff -> add #0
    wd(TOUCH_ORB, 0x5a4, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(TOUCH_ORB, 0x5ac, 0xe1a0_ec41, 0xe1a0_e001), // sx: asr #24 -> mov
    wd(TOUCH_ORB, 0x5e4, 0xe1d7_11d1, 0xe5d7_1011), // ldrsb -> ldrb THIS+0x11
    wd(TOUCH_ORB, 0x6bc, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(TOUCH_ORB, 0x6c0, 0xe1a0_0c03, 0xe1a0_0003), // sx: lsl #24 -> mov
    wd(TOUCH_ORB, 0x6c8, 0xe1a0_0c40, 0xe1a0_0000), // sx: asr #24 -> mov
    // _ZN14StateInNpcMenu11TouchRefineEii
    wd(TOUCH_REFINE, 0x31c, 0xe1a0_ec0e, 0xe1a0_e00e), // sx: lsl #24 -> mov
    wd(TOUCH_REFINE, 0x324, 0xe1a0_ec4e, 0xe1a0_e00e), // sx: asr #24 -> mov
    wd(TOUCH_REFINE, 0x350, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(TOUCH_REFINE, 0x354, 0xe151_0c00, 0xe151_0000), // cmpS: operand lsl #24 -> plain
    wd(TOUCH_REFINE, 0x38c, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(TOUCH_REFINE, 0x394, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(TOUCH_REFINE, 0x3ac, 0xe1d6_31d1, 0xe5d6_3011), // ldrsb -> ldrb THIS+0x11
    wd(TOUCH_REFINE, 0x4cc, 0xe201_10ff, 0xe281_1000), // and255: and #0xff -> add #0
    wd(TOUCH_REFINE, 0x4d0, 0xe1a0_0c01, 0xe1a0_0001), // sx: lsl #24 -> mov
    wd(TOUCH_REFINE, 0x4d8, 0xe1b0_1c40, 0xe1b0_1000), // sx: asrs #24 -> mov
    wd(TOUCH_REFINE, 0x4f4, 0xe201_30ff, 0xe281_3000), // and255: and #0xff -> add #0
    wd(TOUCH_REFINE, 0x584, 0xe1a0_1c01, 0xe1a0_1001), // sx: lsl #24 -> mov
    wd(TOUCH_REFINE, 0x58c, 0xe1a0_ec41, 0xe1a0_e001), // sx: asr #24 -> mov
    wd(TOUCH_REFINE, 0x5c4, 0xe1d6_11d1, 0xe5d6_1011), // ldrsb -> ldrb THIS+0x11
    wd(TOUCH_REFINE, 0x694, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(TOUCH_REFINE, 0x698, 0xe1a0_0c03, 0xe1a0_0003), // sx: lsl #24 -> mov
    wd(TOUCH_REFINE, 0x6a0, 0xe1a0_0c40, 0xe1a0_0000), // sx: asr #24 -> mov
    // _ZN14StateInNpcMenu13KeyCollectionEi
    wd(KEY_COLLECTION, 0x588, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_COLLECTION, 0x58c, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_COLLECTION, 0x610, 0xe1d7_31d4, 0xe5d7_3014), // ldrsb -> ldrb THIS+0x14
    wd(KEY_COLLECTION, 0x63c, 0xe1d7_31d4, 0xe5d7_3014), // ldrsb -> ldrb THIS+0x14
    wd(KEY_COLLECTION, 0x674, 0xe1d7_31d4, 0xe5d7_3014), // ldrsb -> ldrb THIS+0x14
    wd(KEY_COLLECTION, 0x920, 0xe20a_a0ff, 0xe28a_a000), // and255: and #0xff -> add #0
    // _ZN14StateInNpcMenu14DrawCollectionEv
    wd(DRAW_COLLECTION, 0x80c, 0xe1d7_31d4, 0xe5d7_3014), // ldrsb -> ldrb THIS+0x14
    wd(DRAW_COLLECTION, 0x824, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_COLLECTION, 0x828, 0xe155_0c43, 0xe155_0003), // opsx: operand asr/lsr #24 -> plain
    wd(DRAW_COLLECTION, 0x894, 0xe1d7_31d4, 0xe5d7_3014), // ldrsb -> ldrb THIS+0x14
    wd(DRAW_COLLECTION, 0x8a4, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_COLLECTION, 0x8a8, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_COLLECTION, 0x8d0, 0xe1d7_31d4, 0xe5d7_3014), // ldrsb -> ldrb THIS+0x14
    wd(DRAW_COLLECTION, 0x8e0, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_COLLECTION, 0x8e4, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_COLLECTION, 0x9c4, 0xe1d7_31d4, 0xe5d7_3014), // ldrsb -> ldrb THIS+0x14
    wd(DRAW_COLLECTION, 0x9ec, 0xe1d7_31d4, 0xe5d7_3014), // ldrsb -> ldrb THIS+0x14
    wd(DRAW_COLLECTION, 0xaac, 0xe1d7_31d4, 0xe5d7_3014), // ldrsb -> ldrb THIS+0x14
    // _ZN14StateInNpcMenu13KeyOrbCombineEi
    wd(KEY_ORB, 0x130, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_ORB, 0x134, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_ORB, 0x1c0, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_ORB, 0x1f0, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_ORB, 0x268, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_ORB, 0x388, 0xe192_30d3, 0xe7d2_3003), // ldrsb -> ldrb gv+0x1b3e
    wd(KEY_ORB, 0x38c, 0xe353_0000, 0xe353_00ff), // sentinel: cmp #0 -> #0xff
    wd(KEY_ORB, 0x390, 0xba00_00c9, 0x0a00_00c9), // sentinel: cond 0xb -> 0x0
    wd(KEY_ORB, 0x41c, 0xe199_30d3, 0xe7d9_3003), // ldrsb -> ldrb gv+0x1b3e
    wd(KEY_ORB, 0x420, 0xe353_0000, 0xe353_00ff), // sentinel: cmp #0 -> #0xff
    wd(KEY_ORB, 0x424, 0xba00_00a4, 0x0a00_00a4), // sentinel: cond 0xb -> 0x0
    wd(KEY_ORB, 0x484, 0xe192_30d3, 0xe7d2_3003), // ldrsb -> ldrb gv+0x1b3e
    wd(KEY_ORB, 0x488, 0xe353_0000, 0xe353_00ff), // sentinel: cmp #0 -> #0xff
    wd(KEY_ORB, 0x48c, 0xba00_008a, 0x0a00_008a), // sentinel: cond 0xb -> 0x0
    wd(KEY_ORB, 0x538, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_ORB, 0x554, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_ORB, 0x590, 0xe198_30d9, 0xe7d8_3009), // ldrsb -> ldrb gv+0x1b3e
    wd(KEY_ORB, 0x5f0, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_ORB, 0x624, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_ORB, 0x678, 0xe208_80ff, 0xe288_8000), // and255: and #0xff -> add #0
    // _ZN14StateInNpcMenu9KeyRefineEih
    wd(KEY_REFINE, 0x188, 0xe203_30ff, 0xe283_3000), // and255: and #0xff -> add #0
    wd(KEY_REFINE, 0x18c, 0xe313_0080, 0xe313_0102), // tst80: tst #0x80 -> #0x80000000
    wd(KEY_REFINE, 0x218, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_REFINE, 0x248, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_REFINE, 0x28c, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(KEY_REFINE, 0x418, 0xe19b_30d9, 0xe7db_3009), // ldrsb -> ldrb gv+0x1b3e
    wd(KEY_REFINE, 0x41c, 0xe353_0000, 0xe353_00ff), // sentinel: cmp #0 -> #0xff
    wd(KEY_REFINE, 0x420, 0xb3a0_1070, 0x03a0_1070), // sentinel: cond 0xb -> 0x0
    wd(KEY_REFINE, 0x424, 0xba00_004e, 0x0a00_004e), // sentinel: cond 0xb -> 0x0
    wd(KEY_REFINE, 0x470, 0xe19b_30d9, 0xe7db_3009), // ldrsb -> ldrb gv+0x1b3e
    wd(KEY_REFINE, 0x500, 0xe208_80ff, 0xe288_8000), // and255: and #0xff -> add #0
    // _ZN14StateInNpcMenu14DrawOrbCombineEv
    wd(DRAW_ORB, 0x250, 0xe196_30d8, 0xe7d6_3008), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_ORB, 0x254, 0xe353_0000, 0xe353_00ff), // sentinel: cmp #0 -> #0xff
    wd(DRAW_ORB, 0x258, 0xba00_012f, 0x0a00_012f), // sentinel: cond 0xb -> 0x0
    wd(DRAW_ORB, 0x26c, 0xe196_30d8, 0xe7d6_3008), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_ORB, 0x288, 0xe196_30d8, 0xe7d6_3008), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_ORB, 0x360, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0x378, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_ORB, 0x37c, 0xe156_0c43, 0xe156_0003), // opsx: operand asr/lsr #24 -> plain
    wd(DRAW_ORB, 0x3ec, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0x3fc, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_ORB, 0x400, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_ORB, 0x428, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0x438, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_ORB, 0x43c, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_ORB, 0x46c, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0x480, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_ORB, 0x484, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_ORB, 0x4bc, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0x4d0, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_ORB, 0x4d4, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_ORB, 0x4f8, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0x510, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_ORB, 0x514, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_ORB, 0x598, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0x5b4, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0x68c, 0xe1d5_51d1, 0xe5d5_5011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0x938, 0xe192_30d3, 0xe7d2_3003), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_ORB, 0xc38, 0xe1d5_21d1, 0xe5d5_2011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0xc4c, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(DRAW_ORB, 0xc50, 0xe1a0_2c42, 0xe1a0_2002), // sx: asr #24 -> mov
    wd(DRAW_ORB, 0xf48, 0xe1d5_c1d1, 0xe5d5_c011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_ORB, 0xf60, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(DRAW_ORB, 0xf68, 0xe1a0_2c42, 0xe1a0_2002), // sx: asr #24 -> mov
    // _ZN14StateInNpcMenu10DrawRefineEh
    wd(DRAW_REFINE, 0x1fc, 0xe196_30d9, 0xe7d6_3009), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_REFINE, 0x200, 0xe353_0000, 0xe353_00ff), // sentinel: cmp #0 -> #0xff
    wd(DRAW_REFINE, 0x204, 0xba00_0020, 0x0a00_0020), // sentinel: cond 0xb -> 0x0
    wd(DRAW_REFINE, 0x21c, 0xe196_20d9, 0xe7d6_2009), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_REFINE, 0x23c, 0xe196_30d9, 0xe7d6_3009), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_REFINE, 0x4bc, 0xe193_20d2, 0xe7d3_2002), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_REFINE, 0x4c0, 0xe352_0000, 0xe352_00ff), // sentinel: cmp #0 -> #0xff
    wd(DRAW_REFINE, 0x4c4, 0xba00_0315, 0x0a00_0315), // sentinel: cond 0xb -> 0x0
    wd(DRAW_REFINE, 0x640, 0xe192_30d3, 0xe7d2_3003), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_REFINE, 0x644, 0xe353_0000, 0xe353_00ff), // sentinel: cmp #0 -> #0xff
    wd(DRAW_REFINE, 0x648, 0xba00_030a, 0x0a00_030a), // sentinel: cond 0xb -> 0x0
    wd(DRAW_REFINE, 0x838, 0xe196_30dc, 0xe7d6_300c), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_REFINE, 0x83c, 0xe353_0000, 0xe353_00ff), // sentinel: cmp #0 -> #0xff
    wd(DRAW_REFINE, 0x840, 0xba00_0229, 0x0a00_0229), // sentinel: cond 0xb -> 0x0
    wd(DRAW_REFINE, 0x850, 0xe196_30dc, 0xe7d6_300c), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_REFINE, 0x898, 0xe196_30dc, 0xe7d6_300c), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_REFINE, 0xa04, 0xe192_30d3, 0xe7d2_3003), // ldrsb -> ldrb gv+0x1b3e
    wd(DRAW_REFINE, 0xa08, 0xe353_0000, 0xe353_00ff), // sentinel: cmp #0 -> #0xff
    wd(DRAW_REFINE, 0xa0c, 0xba00_0014, 0x0a00_0014), // sentinel: cond 0xb -> 0x0
    wd(DRAW_REFINE, 0xacc, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_REFINE, 0xae8, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_REFINE, 0xaec, 0xe156_0c43, 0xe156_0003), // opsx: operand asr/lsr #24 -> plain
    wd(DRAW_REFINE, 0xb50, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_REFINE, 0xb64, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_REFINE, 0xb68, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_REFINE, 0xb90, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_REFINE, 0xba8, 0xe1a0_3c03, 0xe1a0_3003), // sx: lsl #24 -> mov
    wd(DRAW_REFINE, 0xbac, 0xe1a0_3c43, 0xe1a0_3003), // sx: asr #24 -> mov
    wd(DRAW_REFINE, 0xbe0, 0xe1d5_21d1, 0xe5d5_2011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_REFINE, 0xbf8, 0xe1a0_2c02, 0xe1a0_2002), // sx: lsl #24 -> mov
    wd(DRAW_REFINE, 0xbfc, 0xe1a0_2c42, 0xe1a0_2002), // sx: asr #24 -> mov
    wd(DRAW_REFINE, 0xc68, 0xe1d5_31d1, 0xe5d5_3011), // ldrsb -> ldrb THIS+0x11
    wd(DRAW_REFINE, 0xd54, 0xe1d5_51d1, 0xe5d5_5011), // ldrsb -> ldrb THIS+0x11
    // _ZN14StateInNpcMenu7ProcessEv
    wd(NPC_PROCESS, 0x1f8, 0xe1d5_31d4, 0xe5d5_3014), // ldrsb -> ldrb THIS+0x14
    wd(NPC_PROCESS, 0x72c, 0xe19a_30db, 0xe7da_300b), // ldrsb -> ldrb gv+0x1b3e
    wd(NPC_PROCESS, 0x744, 0xe19a_20db, 0xe7da_200b), // ldrsb -> ldrb gv+0x1b3e
    wd(NPC_PROCESS, 0x7ec, 0xe199_20d3, 0xe7d9_2003), // ldrsb -> ldrb gv+0x1b3e
    wd(NPC_PROCESS, 0x810, 0xe199_20d3, 0xe7d9_2003), // ldrsb -> ldrb gv+0x1b3e
    wd(NPC_PROCESS, 0x840, 0xe199_30d3, 0xe7d9_3003), // ldrsb -> ldrb gv+0x1b3e
    wd(NPC_PROCESS, 0xa2c, 0xe196_30d7, 0xe7d6_3007), // ldrsb -> ldrb gv+0x1b3e
    wd(NPC_PROCESS, 0xa44, 0xe196_30d7, 0xe7d6_3007), // ldrsb -> ldrb gv+0x1b3e
    wd(NPC_PROCESS, 0xb98, 0xe199_20d3, 0xe7d9_2003), // ldrsb -> ldrb gv+0x1b3e
    wd(NPC_PROCESS, 0xbc0, 0xe199_20d3, 0xe7d9_2003), // ldrsb -> ldrb gv+0x1b3e
    wd(NPC_PROCESS, 0xbf0, 0xe199_30d3, 0xe7d9_3003), // ldrsb -> ldrb gv+0x1b3e
];

/// Write [`WIDEN`] into guest memory. **All or nothing**, like
/// [`crate::tenpages::apply`]: every word is located and compared before the
/// first is written.
pub(crate) fn apply(vm: &mut Vm, table: &[Word], what: &str) -> Result<usize, String> {
    let mut plan = Vec::with_capacity(table.len());
    for e in table {
        let base = vm.symbol(e.sym).ok_or_else(|| format!("{what}: `{}` is not in this binary", e.sym))?;
        let addr = base + e.at;
        let got = vm.mem.read_u32(addr);
        if got != e.was {
            return Err(format!(
                "{what}: {}+{:#x} holds {got:#010x}, expected {:#010x} — not the binary these \
                 lists were read from; nothing written",
                e.sym, e.at, e.was
            ));
        }
        plan.push((addr, e.new));
    }
    for (addr, now) in &plan {
        vm.mem.write_u32(*addr, *now);
    }
    Ok(plan.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenpages::{BAG, GRID, STORAGE};

    #[test]
    fn no_word_is_rewritten_twice_or_by_two_tables() {
        let mut seen = std::collections::HashSet::new();
        for e in WIDEN {
            assert!(seen.insert((e.sym, e.at)), "{}+{:#x} twice", e.sym, e.at);
            assert_ne!(e.was, e.new, "{}+{:#x} is not a change", e.sym, e.at);
        }
        for e in STORAGE.iter().chain(BAG).chain(GRID) {
            assert!(!seen.contains(&(e.sym, e.at)), "{}+{:#x} is in two tables", e.sym, e.at);
        }
    }

    /// Each rewrite is one of the six shapes the module docs list, and changes
    /// nothing else in the word — registers, addressing and condition stay.
    #[test]
    fn every_rewrite_is_one_of_the_documented_shapes() {
        for e in WIDEN {
            let (w, n) = (e.was, e.new);
            let at = format!("{}+{:#x}: {w:08x} -> {n:08x}", e.sym, e.at);
            let ldrsb_imm = w & 0x0e5000f0 == 0x005000d0; // ldrsb rT, [rN, #imm]
            let ldrsb_reg = w & 0x0e500ff0 == 0x001000d0; // ldrsb rT, [rN, rM]
            let dp_reg = w & 0x0e000010 == 0x00000000; // data processing, shift by immediate
            if ldrsb_imm {
                let imm = ((w >> 4) & 0xf0) | (w & 0xf);
                let want = (w & 0xf1af_f000) | 0x0450_0000 | imm; // P U W Rn Rt kept
                assert_eq!(n, want & !(1 << 21) | (w & (1 << 21)), "{at} (ldrsb imm)");
            } else if ldrsb_reg {
                let want = (w & 0xf1af_f000) | 0x0650_0000 | (w & 0xf);
                assert_eq!(n, want, "{at} (ldrsb reg)");
            } else if dp_reg && (w >> 5) & 0x7f != 0 && n == w & !0xfe0 {
                // shift cleared: lsl/asr/lsr #24 -> mov, operand ..., asr #24 -> plain
                assert_eq!((w >> 7) & 0x1f, 24, "{at}: only #24 shifts are undone");
            } else if w & 0x0ff0_0fff == 0x0200_00ff && n == (w & !0x01e0_0fff) | 0x0080_0000 {
                // and rD, rN, #0xff -> add rD, rN, #0
            } else if w & 0x0ff0_ffff == 0x0310_0080 && n == (w & !0xfff) | 0x102 {
                // tst rN, #0x80 -> tst rN, #0x80000000
            } else if w & 0x0ff0_f000 == 0x0350_0000 && w & 0xfff != 0 && n & !0xfff == w & !0xfff {
                // cmp rN, #k<<24 -> cmp rN, #k
                let v = (w & 0xff).rotate_right(2 * ((w >> 8) & 0xf));
                assert!(v & 0x00ff_ffff == 0 && n & 0xfff == v >> 24, "{at}: not k<<24 -> k");
            } else if w & 0x0ff0_ffff == 0x0350_0000 && n == w | 0xff {
                // sentinel: cmp rN, #0 -> cmp rN, #0xff
            } else if w & 0x0fff_ffff == n & 0x0fff_ffff {
                // sentinel: condition lt -> eq / ge -> ne, nothing else
                let (c, d) = (w >> 28, n >> 28);
                assert!(matches!((c, d), (0xb, 0x0) | (0xa, 0x1)), "{at}: condition {c:#x} -> {d:#x}");
            } else {
                panic!("{at}: not a documented shape");
            }
        }
    }

    /// A compare loses its `<< 24` only together with the value it compares:
    /// the word just before it must be an `lsl rN, rX, #24` made a move, with
    /// `rN` the compare's left register. The two this would have caught
    /// (`TouchEquipMenu+0x32c`, `TouchSpiritMenu+0x8a0`) compared a loop
    /// counter's `i << 24` from a stack slot and broke equipping by touch.
    #[test]
    fn a_compare_is_unshifted_only_with_its_left_side() {
        let lsl24_to_mov = |w: u32, n: u32| {
            w & 0x0fef_0ff0 == 0x01a0_0c00 && n == w & !0xfe0 // mov rD, rM, lsl #24
        };
        for e in WIDEN {
            let (w, n) = (e.was, e.new);
            let cmp_reg = w & 0x0ff0_0ff0 == 0x0150_0c00 && n == w & !0xf80; // cmp rN, rM, lsl #24
            let cmp_imm = w & 0x0ff0_f000 == 0x0350_0000 && w & 0xf00 != 0 && n & !0xfff == w & !0xfff
                && n & 0xf00 == 0; // cmp rN, #k<<24 -> #k
            if !(cmp_reg || cmp_imm) {
                continue;
            }
            let rn = (w >> 16) & 0xf;
            let ok = WIDEN.iter().any(|p| {
                p.sym == e.sym && p.at + 4 == e.at && lsl24_to_mov(p.was, p.new) && (p.was >> 12) & 0xf == rn
            });
            assert!(ok, "{}+{:#x}: {w:08x} -> {n:08x} unshifts a compare whose left side keeps its shift", e.sym, e.at);
        }
    }

    #[test]
    fn the_sentinel_compares_come_with_their_branches() {
        // Every `cmp rN, #0 -> #0xff` is followed in the table by at least one
        // condition flip at a later word of the same function.
        for (k, e) in WIDEN.iter().enumerate() {
            if e.was & 0x0ff0_ffff == 0x0350_0000 && e.new == e.was | 0xff {
                assert!(
                    WIDEN[k + 1..].iter().any(|f| f.sym == e.sym
                        && f.at > e.at
                        && f.at <= e.at + 8
                        && f.was & 0x0fff_ffff == f.new & 0x0fff_ffff),
                    "{}+{:#x}: a sentinel compare with no flipped branch",
                    e.sym,
                    e.at
                );
            }
        }
    }
}
