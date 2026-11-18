//! Extended 2026-10-11: 33 additional table-only rows, including nine
//! reassigned from unused groups via LOST_ROUTES. The history below describes
//! the original twenty-row restoration.
//!
//! The equipment rows the shipped tables assign to a monster that can never
//! reach them — reopened as a host extra roll.
//!
//! # What was wrong
//!
//! `Monster::SetDropItem` (0xBC910) rolls `Rand(0,65535)` down the monster's
//! own seven-step ladder (`Monster+0x254..+0x26C`) and picks a *band*:
//!
//! ```text
//!   [T3,T4)  droptable body[5],body[6]   영웅 구간
//!   [T4,T5)  droptable body[7],body[8]   전설 구간
//!   [T5,T6)  droptable body[9],body[10]  세트 구간
//!   >= T6    nothing
//! ```
//!
//! An ordinary unnamed mob ships with `T3 == T4 == T5 == T6`: bands 4, 5 and
//! 6 are **zero width**. Its drop group still has `(kind, index)` pairs
//! sitting in those slots — they are behind a door that never opens. A census
//! of every local acquisition path (drop, NPC shop, smith, 조합서 `item_15`,
//! quest rewards, relic box, cash shop, starting gear) found **20 equipment
//! rows reachable by none of them**, and every one of the 20 is one of these
//! stranded slots. Two groups are exact sets — group 2's four band-4 slots
//! are precisely 스톰브링거 / 데몬브레이커 / 속사의피마펭 / 오웬스피어, and
//! group 13's are the three 사티라스 pieces plus 리트무쇠갑옷 — which is what
//! content that was designed and then switched off looks like.
//!
//! # Why an extra roll and not a wider band
//!
//! Widening the band means shadowing `enemy_<N>.dat`, and that is a **drop
//! rate change**. On 2026-09-07 the legendary-band boost was withdrawn by the
//! user and [`crate::rates::restore_shipped_drop_bands`] now deletes any such
//! shadow on every boot, so the shipped rates are the game's own on both
//! content lines. This module leaves the ladder untouched and takes the shape
//! `legendaries::extra_endgame_rolls` takes for the hell bosses: after the
//! guest's own `SetDropItem` has run, roll again.
//!
//! # The rate is the monster's own number
//!
//! The width of the reopened band is that monster's **band 3** width
//! (`T3 - T2`), read off the monster itself. No new constant is invented and
//! it scales with difficulty the way the shipped ladders do — 655 at 노말,
//! 754 at 하드, 852 at 헬, i.e. ~1% of 65536. With the guest's own
//! `Rand(0,3)` slot pick that is **~0.25% per kill** for a one-roll mob.
//!
//! # The slot pick is the guest's
//!
//! `SetDropItem` chooses one of the group's four records with `Rand(0,3)` and
//! drops whatever that slot holds. This does the same, and then drops **only
//! if the chosen slot is one of the 20** — so a group where three of the four
//! slots are already obtainable (or are `0xFF` padding, as the accessory
//! bands mostly are) yields a drop one time in four, exactly as the original
//! band would have. Nothing already reachable gets a rate change.
//!
//! # The record's option pair travels with the drop
//!
//! A band is not only a `(kind, index)`. `Monster::SetDropItem`'s band-4 arm
//! (0xBCB54) and band-6 arm (0xBCE30) also hand `NewDropItem` the record's
//! **second option pair** (`body[3]`, `body[4]`) as slot 2, with the value
//! scaled by the monster's own percent (`Monster+0x274`: `v * (100 + pct) /
//! 100`, 0xBCB68..0xBCB80). Band 5 hands nothing — every band-5 row names
//! all three slots itself. That pair is the whole reason a 영웅 row can be
//! dropped at all: the shipped 영웅 rows reserve slot 2 with `0xFF`, and the
//! screen draws three option lines for 영웅, so a band-4 drop made with
//! every override `0xFF` reads `+랜덤옵션` on its third line. That is what
//! the first version of this walk did, and all fifteen band-4 rows here are
//! 영웅 (§54). Now the walk passes the same pair the guest would have, read
//! from the guest's own loaded table.
//!
//! # Both content lines
//!
//! There is no `vanilla` gate here, deliberately. `BRANCHES.md`: the branches
//! decide *which items exist*, never *how often anything drops*. These 20 are
//! shipped rows, present in both.

use crate::Vm;

/// `(source group, band, slot, kind, index)` for the restored equipment.
///
/// Derived from the shipped `droptable.dat` — record `group * 4 + slot`, with
/// `band` naming the pair inside it (4 → `body[5..6]`, 5 → `body[7..8]`).
/// Fifty-three distinct rows over sixty slots. [`verify`] checks each slot
/// against the archive at boot. Routed groups retain the source option pair.
///
/// Group 37 has no monster that rolls at all; the entry is kept because the
/// slot really is there, and it simply never fires.
pub const LOST_SLOTS: [[u8; 5]; 60] = [
    [1, 4, 1, 6, 17],   // 다크코트
    [1, 4, 3, 8, 17],   // 다크워커
    [2, 4, 0, 0, 17],   // 스톰브링거
    [2, 4, 1, 1, 17],   // 데몬브레이커
    [2, 4, 2, 2, 17],   // 속사의피마펭
    [2, 4, 3, 3, 17],   // 오웬스피어
    [4, 4, 0, 3, 18],   // 소울랜스
    [4, 4, 1, 6, 18],   // 빙해의갑옷
    [4, 4, 3, 8, 18],   // 빙해의장화
    [13, 4, 0, 6, 37],  // 사티라스코트
    [13, 4, 1, 7, 37],  // 사티라스핸드
    [13, 4, 2, 8, 37],  // 사티라스부츠
    [13, 4, 3, 6, 38],  // 리트무쇠갑옷
    [25, 5, 0, 10, 6],  // 쟈칼의발톱
    [26, 5, 0, 10, 6],  // 쟈칼의발톱
    [27, 5, 0, 10, 7],  // 공작의깃털
    [29, 5, 0, 10, 7],  // 공작의깃털
    [30, 5, 0, 10, 11], // 시바스아뮬렛
    [32, 5, 0, 10, 12], // 기간틱아뮬렛
    [33, 5, 2, 10, 13], // 배리어이어링
    [35, 4, 3, 6, 73],  // 무극위상코트
    [36, 4, 0, 8, 73],  // 무극위상장화
    [36, 5, 1, 10, 11], // 시바스아뮬렛
    [37, 5, 0, 10, 12], // 기간틱아뮬렛
    [38, 5, 0, 10, 13], // 배리어이어링
    [9, 4, 2, 0, 23], // 알비스
    [58, 4, 2, 0, 69], // 앤서러
    [58, 5, 0, 0, 75], // 기미르:서리검
    [9, 4, 3, 1, 23], // 지옥불꽃단검
    [58, 4, 3, 1, 69], // 진마도진혼검
    [58, 5, 1, 1, 75], // 알스비드:비원
    [22, 4, 0, 2, 42], // 유니콘혼
    [58, 5, 2, 2, 75], // 페이탈:구원자
    [22, 4, 1, 3, 42], // 섀도우스피어
    [58, 5, 3, 3, 75], // 블린카:분쇄
    [12, 4, 0, 5, 21], // 블러디아이
    [12, 4, 1, 5, 22], // 바하무트헬름
    [18, 4, 2, 5, 40], // 바니시헬름
    [28, 4, 0, 5, 67], // 헤이드기어
    [34, 4, 0, 5, 69], // 아시크서클릿
    [36, 4, 1, 5, 74], // 몰라크투구
    [28, 4, 1, 6, 67], // 헤이드코트
    [36, 4, 2, 6, 74], // 몰라크갑옷
    [9, 4, 0, 7, 22], // 바하무트핸드
    [28, 4, 2, 7, 67], // 헤이드장갑
    [9, 4, 1, 8, 22], // 바하무트워커
    [28, 4, 3, 8, 67], // 헤이드그리브
    [36, 4, 3, 8, 74], // 몰라크신발
    [12, 4, 2, 9, 21], // 임팩트실드
    [12, 4, 3, 9, 22], // 아크프로텍터
    [20, 4, 0, 9, 39], // 라이트닝실드
    [20, 4, 1, 9, 40], // 가이아스실드
    [25, 4, 0, 9, 63], // 카오스섀도우
    [25, 4, 1, 9, 64], // 오르도디펜더
    [39, 4, 2, 9, 73], // 가디언즈실드
    [39, 4, 3, 9, 74], // 데크알브실드
    [18, 5, 0, 10, 5], // 독사의이빨
    [23, 5, 0, 10, 5], // 독사의이빨
    [28, 5, 0, 10, 10], // 샌드웜의비늘
    [35, 5, 1, 10, 10], // 샌드웜의비늘
];

/// How many rows [`LOST_SLOTS`] is supposed to reach. The census number, and
/// a unit test holds the table to it.
pub const LOST_ROW_COUNT: usize = 53;

/// (source group, source band, target group). Unused endgame table groups
/// retain their slots and options; occupied hell groups supply the monsters.
pub const LOST_ROUTES: [[u8; 3]; 4] = [
    [34, 4, 35],
    [39, 4, 38],
    [58, 4, 59],
    [58, 5, 59],
];

fn target_group(source: u8, band: u8) -> u8 {
    LOST_ROUTES.iter().find(|r| r[0] == source && r[1] == band)
        .map_or(source, |r| r[2])
}

fn roll_width(source: u8, band: u8, group: u8, thresholds: &[u32; 7]) -> u32 {
    if target_group(source, band) != group {
        return 0;
    }
    let routed = source != group;
    if !routed && thresholds[band as usize] > thresholds[band as usize - 1] {
        return 0;
    }
    let rate_band = if routed && band == 5 { 5 } else { 3 };
    thresholds[rate_band].saturating_sub(thresholds[rate_band - 1])
}

/// One `droptable.dat` record: `[u16 reclen][11 bytes]`.
const DROP_REC: usize = 13;

/// Where a band's `(kind, index)` pair sits inside a record body.
///
/// Band 4 → `body[5..6]`, band 5 → `body[7..8]`, band 6 → `body[9..10]`, i.e.
/// `2 * band - 3`. The same walk `tools_re/codex/drops.py` uses.
const fn band_off(band: u8) -> usize {
    (2 * band as usize).saturating_sub(3)
}

/// The monster's seven thresholds, `Monster+0x254..+0x26C`.
const T_BASE: u32 = 0x254;
/// `Monster+0x273`, the drop group (`enemy_<N>.dat` name + 70).
const GROUP: u32 = 0x273;
/// `Monster+0x275`, how many times this monster rolls (`enemy_<N>.dat` + 72).
const ROLLS: u32 = 0x275;
/// `Monster+0x274`, signed percent applied to a record's option value
/// (`enemy_<N>.dat` + 71). `SetDropItem` reads it with `ldrb` then
/// sign-extends (0xBCB54 / 0xBCB60).
const OPTION_PCT: u32 = 0x274;

/// Where a record body keeps the option pair bands 4 and 6 carry: `body[3]`
/// is the kind, `body[4]` the value (`SetDropItem` `+7` / `+8` from the
/// record's reclen, 0xBCAB0 / 0xBCAC8).
const PAIR_B_OFF: usize = 3;

/// The value the guest sends with a record's option pair: `v * (100 + pct)`
/// then a signed divide by 100 (`smull` by 0x51EB851F, `asr #5`, minus the
/// sign — truncation toward zero, which is what `/` does on `i32`), then
/// `and #0xff` (0xBCB90) before `NewDropItem` reads it back with `ldrsb`.
pub fn scaled_option_value(value: i8, pct: i8) -> u8 {
    (i32::from(value) * (100 + i32::from(pct)) / 100) as u8
}

/// The slot-2 override a band hands `NewDropItem`: bands 4 and 6 pass the
/// record's second pair, band 5 passes `(0xFF, 0xFF)` and keeps the row's own.
fn slot2_override(band: u8, pair_b: (u8, u8), pct: i8) -> (u32, u32) {
    match band {
        4 | 6 => (
            u32::from(pair_b.0),
            u32::from(scaled_option_value(pair_b.1 as i8, pct)),
        ),
        _ => (0xff, 0xff),
    }
}

/// Check every [`LOST_SLOTS`] entry against the shipped drop table.
///
/// Returns the entries that still describe the table, plus a line per entry
/// that does not. A row that moved is dropped rather than rolled: a stale
/// entry would hand the player whatever now sits in that slot, and doing that
/// silently is the failure this project keeps refusing (`GrantTable::build`,
/// `rates::smith_tail_offsets`).
pub fn verify(droptable: &[u8]) -> (Vec<[u8; 5]>, Vec<String>) {
    let mut ok = Vec::new();
    let mut bad = Vec::new();
    let Some(count) = droptable.get(..2).map(|b| u16::from_le_bytes([b[0], b[1]])) else {
        return (ok, vec!["lostdrops: droptable too short for its own count".into()]);
    };
    for e in LOST_SLOTS {
        let [group, band, slot, kind, index] = e;
        let rec = usize::from(group) * 4 + usize::from(slot);
        if rec >= usize::from(count) {
            bad.push(format!(
                "lostdrops: group {group} slot {slot} is record {rec}, past the table's {count}"
            ));
            continue;
        }
        let at = 2 + rec * DROP_REC + 2;
        let off = band_off(band);
        match (droptable.get(at + off), droptable.get(at + off + 1)) {
            (Some(&k), Some(&i)) if k == kind && i == index => ok.push(e),
            (Some(&k), Some(&i)) => bad.push(format!(
                "lostdrops: group {group} band {band} slot {slot} holds ({k},{i}), \
                 expected ({kind},{index}) — not rolled"
            )),
            _ => bad.push(format!(
                "lostdrops: group {group} slot {slot} runs off the end of the table"
            )),
        }
    }
    (ok, bad)
}

/// Per-kill probability of one stranded slot.
///
/// `width / 65536` to land in the reopened band, `1/4` for the slot pick,
/// `rolls` independent tries. The published odds table computes the same
/// thing, which is why it lives here and not only in Python.
pub fn per_kill(width: u32, rolls: u8) -> f64 {
    if width == 0 || rolls == 0 {
        return 0.0;
    }
    let p = f64::from(width) / 65536.0 * 0.25;
    1.0 - (1.0 - p).powi(i32::from(rolls))
}

/// Roll the stranded slots of this monster's drop group.
///
/// Called from the `Monster::SetDropItem` hook **after** the guest's own drop
/// has been made, so this only ever adds. Declines quietly whenever anything
/// it needs is missing — a monster with no rolls, a band the shipped ladder
/// already opens, a map that is not up yet.
pub fn extra_lost_rolls(vm: &mut Vm, monster: u32, slots: &[[u8; 5]]) -> Result<(), String> {
    use crate::legendaries::{
        GET_X, GET_Y, GV_ITEM_TABLE, GV_MAP_ITEM, ITEM_TABLE_DROP_BUF, NEW_DROP_ITEM, RAND,
    };

    let group = vm.mem.read_u8(monster + GROUP);
    let rolls = vm.mem.read_u8(monster + ROLLS);
    if rolls == 0 || !slots.iter().any(|e| target_group(e[0], e[1]) == group) {
        return Ok(());
    }
    let t = |vm: &Vm, i: u8| vm.mem.read_u32(monster + T_BASE + u32::from(i) * 4);
    let thresholds = std::array::from_fn(|i| t(vm, i as u8));
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
    // The drop table the guest itself is walking — `ItemTable+0x214` — so the
    // pair read here is the one its own band-4 arm would have read, shadow or
    // archive alike. Without it the walk cannot carry the option and declines.
    let item_table = vm.mem.read_u32(gv + GV_ITEM_TABLE);
    let drop_buf = if item_table == 0 {
        0
    } else {
        vm.mem.read_u32(item_table + ITEM_TABLE_DROP_BUF)
    };
    if drop_buf == 0 {
        return Ok(());
    }
    let pct = vm.mem.read_u8(monster + OPTION_PCT) as i8;
    let x = vm.call(get_x, &[monster])?;
    let y = vm.call(get_y, &[monster])?;

    // One walk per band this group strands, so group 36 — which strands both
    // its 영웅 and its 전설 slot — gets one each. They are separate doors in
    // the shipped design too.
    let mut bands: Vec<(u8, u8)> = slots.iter()
        .filter(|e| target_group(e[0], e[1]) == group)
        .map(|e| (e[0], e[1])).collect();
    bands.sort_unstable();
    bands.dedup();

    for (source, band) in bands {
        let width = roll_width(source, band, group, &thresholds);
        if width == 0 {
            continue;
        }
        for _ in 0..rolls {
            let roll = vm.call(rand, &[0, 0xffff])?;
            if roll >= width {
                continue;
            }
            // The guest's own slot pick. A slot that is not stranded drops
            // nothing, which is what keeps every reachable item at its
            // shipped rate.
            let slot = vm.call(rand, &[0, 3])? as u8;
            let Some(e) = slots.iter().find(|e| e[0] == source && e[1] == band && e[2] == slot)
            else {
                continue;
            };
            // `0xFF` overrides leave `ItemInfo+0x15c` as `GetItemTableInfo`
            // filled it, so the row keeps the table's own grade — the same
            // thing bands 4..6 do. Slot 2 is the one override a band does
            // send: the record's second pair for bands 4 and 6, nothing for 5.
            let rec = drop_buf + 2 + (u32::from(source) * 4 + u32::from(slot)) * DROP_REC as u32 + 2;
            let pair_b = (
                vm.mem.read_u8(rec + PAIR_B_OFF as u32),
                vm.mem.read_u8(rec + PAIR_B_OFF as u32 + 1),
            );
            let (k2, v2) = slot2_override(band, pair_b, pct);
            let ff = 0xffu32;
            vm.call(
                new_drop,
                // r0..r3 then eight stack bytes: index, grade, k0, v0, k1, v1, k2, v2.
                &[map, x, y, u32::from(e[3]), u32::from(e[4]), ff, ff, ff, ff, ff, k2, v2],
            )?;
            vm.log.push(format!(
                "lostdrops: group {group} band {band} dropped kind={} index={} slot 2 = ({k2}, {v2})",
                e[3], e[4]
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn routed_slots_use_target_rates_without_reopening_live_native_bands() {
        let ordinary = [17039, 25899, 27603, 28455, 28455, 28455, 28455];
        assert_eq!(roll_width(36, 4, 36, &ordinary), 852);
        assert_eq!(roll_width(34, 4, 35, &ordinary), 852);
        assert_eq!(roll_width(39, 4, 38, &ordinary), 852);
        assert_eq!(roll_width(34, 4, 34, &ordinary), 0);
        let boss = [655, 655, 655, 33423, 39976, 40632, 41287];
        assert_eq!(roll_width(58, 4, 59, &boss), 32768);
        assert_eq!(roll_width(58, 5, 59, &boss), 656);
        assert_eq!(roll_width(58, 5, 60, &boss), 0);
        assert_eq!(roll_width(59, 5, 59, &boss), 0);
        assert_eq!(roll_width(58, 5, 59, &[0; 7]), 0);
    }

    /// A drop table that agrees with [`LOST_SLOTS`].
    fn table() -> Vec<u8> {
        let max = LOST_SLOTS
            .iter()
            .map(|e| usize::from(e[0]) * 4 + usize::from(e[2]))
            .max()
            .expect("non-empty");
        let count = max + 1;
        let mut out = (count as u16).to_le_bytes().to_vec();
        for rec in 0..count {
            out.extend_from_slice(&11u16.to_le_bytes());
            let mut body = [0xffu8; 11];
            body[0] = (rec / 4) as u8;
            for e in LOST_SLOTS {
                if usize::from(e[0]) * 4 + usize::from(e[2]) == rec {
                    let off = band_off(e[1]);
                    body[off] = e[3];
                    body[off + 1] = e[4];
                }
            }
            out.extend_from_slice(&body);
        }
        out
    }

    /// 0xBCB68..0xBCB90 on the values the shipped records carry (2..14) at
    /// the percents `enemy_<N>.dat` ships (a few to ~30): `v * (100+pct) / 100`,
    /// truncated, one byte.
    #[test]
    fn the_option_value_scales_like_the_guest() {
        assert_eq!(scaled_option_value(2, 8), 2);
        assert_eq!(scaled_option_value(6, 8), 6);
        assert_eq!(scaled_option_value(14, 8), 15);
        assert_eq!(scaled_option_value(10, 0), 10);
        assert_eq!(scaled_option_value(14, 50), 21);
        assert_eq!(scaled_option_value(10, -50), 5);
        assert_eq!(scaled_option_value(-1, 8), 0xff, "0xFF stays a decline");
    }

    /// Band 4 and 6 carry the pair, band 5 does not — 0xBCB54 / 0xBCE30 vs
    /// 0xBCD60.
    #[test]
    fn only_bands_four_and_six_carry_the_records_pair() {
        assert_eq!(slot2_override(4, (35, 2), 8), (35, 2));
        assert_eq!(slot2_override(6, (35, 6), 8), (35, 6));
        assert_eq!(slot2_override(5, (35, 2), 8), (0xff, 0xff));
    }

    /// Against the real archive: every band-4 stranded slot is a 영웅 row with
    /// slot 2 reserved, and its record carries a real pair for it — so the
    /// pair is what keeps the third line from reading `+랜덤옵션`, and the
    /// record can supply it. Band-5 stranded rows name every line themselves.
    #[test]
    fn stranded_band_four_rows_need_the_pair_and_the_record_has_it() {
        let Ok(vfs) = std::fs::read("../payload/data.vfs.mp3") else {
            return;
        };
        let dt = crate::shop::vfs_entry(&vfs, crate::legendaries::DROP_TABLE_PATH).expect("droptable");
        let (ok, bad) = verify(dt);
        assert!(bad.is_empty(), "{bad:?}");
        for [group, band, slot, kind, index] in ok {
            let rec = usize::from(group) * 4 + usize::from(slot);
            let body = &dt[2 + rec * DROP_REC + 2..2 + (rec + 1) * DROP_REC];
            let path = format!("c/csv/item_{kind:02}.dat");
            let table = crate::shop::vfs_entry(&vfs, &path).expect(&path);
            let row = crate::legendaries::record_body(table, usize::from(index)).expect("row");
            let eq: &[u8; crate::shop::EQUIP_BLOCK] =
                row[row.len() - crate::shop::EQUIP_BLOCK..].try_into().unwrap();
            let grade = eq[crate::shop::EQUIP_GRADE_AT];
            let mut kinds = crate::shop::option_kinds(eq);
            let bare = crate::shop::empty_option_lines(grade, kinds);
            let pair_b = (body[PAIR_B_OFF], body[PAIR_B_OFF + 1]);
            let (k2, _) = slot2_override(band, pair_b, 0);
            if k2 != 0xff {
                kinds[2] = k2 as i8;
            }
            let carried = crate::shop::empty_option_lines(grade, kinds);
            assert!(
                carried.is_empty(),
                "group {group} band {band} slot {slot} ({kind},{index}) still prints +랜덤옵션 on {carried:?}"
            );
            if band == 4 {
                assert_eq!(bare, [2], "({kind},{index}) grade {grade}: the pair is what fills the third line");
                assert!(pair_b.0 > 0 && pair_b.0 < 0x80, "record pair kind {}", pair_b.0);
            } else {
                assert!(bare.is_empty(), "({kind},{index}) grade {grade} names its own lines");
            }
        }
    }

    #[test]
    fn the_slot_table_reads_back_out_of_a_matching_drop_table() {
        let (ok, bad) = verify(&table());
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(ok.len(), LOST_SLOTS.len());
    }

    #[test]
    fn a_moved_row_is_refused_and_not_rolled() {
        let mut t = table();
        // Move group 2 band 4 slot 0 — 스톰브링거 — to a different index.
        let at = 2 + (2 * 4) * DROP_REC + 2 + band_off(4);
        t[at + 1] = 99;
        let (ok, bad) = verify(&t);
        assert_eq!(ok.len(), LOST_SLOTS.len() - 1);
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("group 2"), "{bad:?}");
        assert!(!ok.contains(&[2, 4, 0, 0, 17]));
    }

    #[test]
    fn a_short_table_refuses_everything_instead_of_reading_past_it() {
        let (ok, bad) = verify(&2u16.to_le_bytes());
        assert!(ok.is_empty());
        assert_eq!(bad.len(), LOST_SLOTS.len());
    }

    #[test]
    fn restored_rows_cover_the_census() {
        let rows: BTreeSet<(u8, u8)> = LOST_SLOTS.iter().map(|e| (e[3], e[4])).collect();
        assert_eq!(rows.len(), LOST_ROW_COUNT, "the census found 53 rows");
        assert_eq!(LOST_SLOTS.len(), 60);
    }

    #[test]
    fn no_slot_is_claimed_twice() {
        let keys: BTreeSet<(u8, u8, u8)> =
            LOST_SLOTS.iter().map(|e| (e[0], e[1], e[2])).collect();
        assert_eq!(keys.len(), LOST_SLOTS.len(), "a slot can hold only one row");
    }

    #[test]
    fn every_entry_names_a_real_band_and_slot() {
        for e in LOST_SLOTS {
            assert!((4..=6).contains(&e[1]), "band {} out of range", e[1]);
            assert!(e[2] < 4, "slot {} out of range", e[2]);
            assert!(e[3] <= 10, "kind {} is not equipment", e[3]);
        }
    }

    #[test]
    fn band_offsets_match_the_drop_record_layout() {
        assert_eq!(band_off(4), 5);
        assert_eq!(band_off(5), 7);
        assert_eq!(band_off(6), 9);
    }

    /// 655 at 노말 with one roll is the ~0.25% the module comment claims.
    #[test]
    fn the_published_rate_is_a_quarter_of_a_percent() {
        let p = per_kill(655, 1);
        assert!((p - 0.002498626).abs() < 1e-6, "{p}");
        assert_eq!(per_kill(0, 4), 0.0, "a zero-width band drops nothing");
        assert_eq!(per_kill(655, 0), 0.0, "a monster that never rolls drops nothing");
        assert!(per_kill(852, 2) > per_kill(852, 1), "rolls compound");
        assert!(per_kill(852, 1) > per_kill(655, 1), "hell is wider than normal");
    }
}
