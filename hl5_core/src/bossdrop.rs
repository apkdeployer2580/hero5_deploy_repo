//! 정체모를 괴수: the drop the story scene eats, and the accessory it now adds.
//!
//! # The bug
//!
//! Killing that boss starts a story scene at once — no looting time, unlike
//! every other boss — and whatever it dropped is gone before the player can
//! walk onto it. Two independent reasons, and the fix has to survive both
//! because the binary alone cannot say which one fires here:
//!
//! * **The floor is wiped.** `EventProc::Scene_Init` is the only caller of
//!   `MapItem::DeleteAll` (0x72820), and `StateGame::LoadProc` is the only
//!   caller of `Scene_Init`. A scene that loads a map therefore deletes every
//!   item lying on the ground.
//! * **The drop may never happen at all.** `Monster::SetDropItem` is reached
//!   from exactly one place — `Monster::EndProc+0x12c` (0xC21BC) — and only
//!   after `GetDecreaseDieFrame` has counted down to 0 with the monster in die
//!   motion. A map load inside that window destroys the monster mid-animation.
//!
//! # The fix is the guest's own auto-pickup
//!
//! `MapItem::NewDropItem` already knows how to hand an item straight to the
//! player instead of putting it on the floor. Right after `MakeSocket` it asks
//! `HERO::GetAutoPickupItem` (0x89D48 — the `int64` buff timer at `HERO+0x310`
//! / `+0x314`), and on a positive value takes the branch at 0xA77F0:
//! `HERO::GetBagItemPtr` → `BagItem::NewBagItem` → `Battle::SetItemPickupUi`.
//! The 64-bit test is the same split-compare shape as the 성장의 서 timer
//! (`bgt` on the high word, then `bne`/`beq` on the low one), so a positive low
//! word with a zero high word is a live buff.
//!
//! So this module does two small things and nothing else:
//!
//! 1. do that boss's drop at **death** — [`SET_DIE_MOTION`], the one place a
//!    monster's HP reaching zero is observed — instead of at the end of the
//!    death animation, and
//! 2. hold the auto-pickup answer positive for the length of that one call.
//!
//! **Which items drop, at what grade, with which sockets and options is not
//! touched.** The guest's own `Monster::SetDropItem` runs, with the monster's
//! own ladder, through the guest's own `NewDropItem`. The only difference is
//! which of that function's two existing endings it takes.
//!
//! Doing it early has to be paid for once: after the drop this zeroes the
//! monster's seven thresholds (`Monster+0x254..+0x26C`) and its roll count
//! (`+0x275`), so the natural `SetDropItem` later in `EndProc` — if the scene
//! lets it run — rolls nothing. That is not an invented state: the shipped
//! table already contains rows shaped exactly like it (`enemy_0.dat` row 8),
//! and `setEnemyData` refills both from the file on the next spawn.
//!
//! It cannot fire twice for one death. `SetDieMotion` ends in
//! `CHAR::SetMotion(5)` (0xC086C), and both of `EndProc`'s calls to it —
//! 0xC24C4 and 0xC2844 — sit behind a `GetMotion() != 5` test (0xC2130 /
//! 0xC24F0). Once the monster is in die motion nothing calls it again.
//!
//! # Which monster
//!
//! By name. `Monster::setEnemyData` memcpy's the record's name into
//! `Monster+0x218` (0xC1B9C), so the check costs one string compare and needs
//! no row index. Drop group will not do: group 55 in `enemy_2.dat` is shared
//! with 크리스 and 강화병.
//!
//! # The dead twin
//!
//! Each of `enemy_0/1/2.dat` holds **two** rows for this boss: a marked one
//! (`'정체모를 괴수{`, four rolls, a full ladder) and a plain one (all seven
//! thresholds zero, zero rolls) — the same doubling the shipped table uses for
//! 아일린 and for the gatherable 박스 / 광물 / 풀. Which one the map spawns is
//! not decidable from the binary. If the dying monster carries the dead twin's
//! parameters, [`Row`] — read out of the archive at boot, per difficulty — is
//! written into it first, so the kill pays either way. A monster that already
//! has a live ladder is left exactly as the table made it.
//!
//! # The accessory
//!
//! On top of that, a flat **1% per kill** of one of the four 전설 rows of
//! `item_10.dat` ([`ACCESSORY_ROWS`]). It is an independent roll, so nothing
//! is taken from the weapon bands, and it goes through the same
//! `NewDropItem` with the same auto-pickup window. Not `patch.rs`: this is a
//! deliberate behaviour change, not a port.

use crate::mem::RETURN_MAGIC;
use crate::patch::PatchFn;
use crate::Vm;
use arm32_cpu::{reg, Mode};

/// EUC-KR `정체모를 괴수`. The stored name may carry the table's own markers
/// around it (`'` … `{`), so the test is containment, not equality.
pub const TARGET_NAME: &[u8] = &[
    0xc1, 0xa4, 0xc3, 0xbc, 0xb8, 0xf0, 0xb8, 0xa6, 0x20, 0xb1, 0xab, 0xbc, 0xf6,
];

/// `item_10.dat` is 장신구. The shipped drop table already has 38 kind-10
/// slots, so the drop and pickup paths for it are the game's own.
pub const ACCESSORY_KIND: u8 = 10;

/// The four 전설 rows of `item_10.dat`: 기사의징표 / 스폰의사슬 /
/// 홀리즈이어링 / 카오스이어링. 공허의 목걸이 (17) is the cash weapon's
/// companion and 신비조사회링 (16) is a level-99 노멀 oddity; both stay out.
pub const ACCESSORY_ROWS: [u8; 4] = [4, 9, 14, 15];

/// Per **kill**, not per roll: one `Rand(0,99) == 0`.
pub const ACCESSORY_PERCENT: u32 = 1;

/// Seven thresholds, `Monster+0x254..+0x26C`, compared against
/// `Rand(0, 0xffff)` at 0xBCB18..0xBCE24.
const RUNGS: usize = 7;

mod monster {
    /// `setEnemyData` memcpy destination (0xC1B94), NUL-terminated.
    pub const NAME: u32 = 0x218;
    /// `BATTLER` current HP — `SetDieMotion` returns at once while it is > 0
    /// (0xC0688), and `EndProc` re-tests it before dropping (0xC21AC).
    pub const HP: u32 = 0xf0;
    /// First of [`super::RUNGS`] drop thresholds.
    pub const T0: u32 = 0x254;
    /// `droptable.dat` group.
    pub const GROUP: u32 = 0x273;
    /// How many times `SetDropItem` rolls (0xBC9D4).
    pub const ROLLS: u32 = 0x275;
}

/// `HERO::GetAutoPickupItem` is three instructions: `r1 = [this+0x314]`,
/// `r0 = [this+0x310]`, `bx lr`.
mod hero {
    pub const AUTO_PICKUP_LO: u32 = 0x310;
    pub const AUTO_PICKUP_HI: u32 = 0x314;
    /// `/c/csv/enemy_%d.dat` selector (`setEnemyData+0x80`).
    pub const DIFFICULTY: u32 = 0x277;
}

/// `gv + 0x1448` = `MapItem*`, `+ 0x1474` = `HERO*`. Both read straight out of
/// the functions that use them (`SetDropItem` 0xBCBD0, `setEnemyData` 0xC1B08).
const GV_MAP_ITEM: u32 = 0x1448;
const GV_HERO: u32 = 0x1474;

const SET_DIE_MOTION: &str = "_ZN7Monster12SetDieMotionEv";
const SET_DROP_ITEM: &str = "_ZN7Monster11SetDropItemEv";
const GET_AUTO_PICKUP: &str = "_ZN4HERO17GetAutoPickupItemEv";
const NEW_DROP_ITEM: &str = "_ZN7MapItem11NewDropItemEiiaaaaaaaaa";
const RAND: &str = "_ZN10StaticUtil4RandEii";
const GET_X: &str = "_ZN6OBJECT4GetXEv";
const GET_Y: &str = "_ZN6OBJECT4GetYEv";

/// `/c/csv/enemy_%d.dat`, the three difficulty variants.
const ENEMY_PATHS: [&str; 3] = crate::rates::ENEMY_PATHS;

/// The drop parameters of the **marked** row, per difficulty. Read once at
/// boot out of the archive so the dead twin can be handed real numbers rather
/// than numbers this host invented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub thresholds: [u32; RUNGS],
    pub group: u8,
    pub rolls: u8,
}

/// Boot-parsed rows plus the one bit the auto-pickup hook reads.
#[derive(Default)]
pub struct BossDrop {
    rows: [Option<Row>; 3],
    /// Non-zero only inside this module's own `SetDropItem` / `NewDropItem`
    /// calls. Nothing else in the process ever sets it, so
    /// [`report_auto_pickup`] is a faithful re-read of the two words at every
    /// other moment.
    armed: u32,
}

impl BossDrop {
    /// Parse the marked row out of each `enemy_<N>.dat`. Rows that do not
    /// parse are simply absent — the hook then leaves a dead twin dead.
    pub fn new(vfs: &[u8]) -> (Self, Vec<String>) {
        let mut rows = [None; 3];
        let mut logs = Vec::new();
        for (n, path) in ENEMY_PATHS.iter().enumerate() {
            let Some(src) = crate::shop::vfs_entry(vfs, path) else {
                logs.push(format!("bossdrop: {path} is not in the archive"));
                continue;
            };
            match marked_row(src, TARGET_NAME) {
                Some(row) => {
                    // Printed, not merely stored: the numbers are the whole
                    // claim this module makes about the archive, and a device
                    // log is the only place they can be checked.
                    logs.push(format!(
                        "bossdrop: {path} 정체모를 괴수 = group {} x {} roll(s), bands {:?}",
                        row.group, row.rolls, row.thresholds
                    ));
                    rows[n] = Some(row);
                }
                None => logs.push(format!("bossdrop: no live 정체모를 괴수 row in {path}")),
            }
        }
        (Self { rows, armed: 0 }, logs)
    }

    pub fn row(&self, difficulty: u8) -> Option<Row> {
        self.rows.get(usize::from(difficulty)).copied().flatten()
    }
}

/// Walk `[u16 count]` + `[u16 reclen][u8 namelen][name][…]` and return the
/// parameters of the first record whose name contains `needle` **and** which
/// actually rolls. The offsets are `rates`' own, measured the same way.
fn marked_row(src: &[u8], needle: &[u8]) -> Option<Row> {
    let count = usize::from(u16::from_le_bytes([*src.first()?, *src.get(1)?]));
    let mut pos = 2usize;
    for _ in 0..count {
        let reclen = usize::from(u16::from_le_bytes([*src.get(pos)?, *src.get(pos + 1)?]));
        let body = pos + 2;
        let end = body.checked_add(reclen)?;
        if end > src.len() {
            return None;
        }
        let namelen = usize::from(*src.get(body)?);
        let name = src.get(body + 1..body + 1 + namelen)?;
        let tail = body + 1 + namelen;
        let ladder = tail.checked_add(crate::rates::LADDER_AFTER_NAME)?;
        if ladder.checked_add(RUNGS * 4)? <= end && contains(name, needle) {
            let at = |k: usize| ladder + k * 4;
            let mut thresholds = [0u32; RUNGS];
            for (k, t) in thresholds.iter_mut().enumerate() {
                let b = src.get(at(k)..at(k) + 4)?;
                *t = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            }
            let group = *src.get(tail + GROUP_AFTER_NAME)?;
            let rolls = *src.get(tail + ROLLS_AFTER_NAME)?;
            if rolls > 0 && thresholds.iter().any(|&t| t != 0) {
                return Some(Row { thresholds, group, rolls });
            }
        }
        pos = end;
    }
    None
}

/// Record offsets, measured from the byte after the name, of the two fields
/// `setEnemyData` lands at `Monster+0x273` (drop group) and `+0x275` (roll
/// count). Both were measured the way [`crate::rates::LADDER_AFTER_NAME`] was
/// — by the offset that makes the whole shipped table agree — and they are
/// confirmed independently against [`crate::legendaries::BOSS_GROUPS`]: read
/// this way, `enemy_2.dat` gives 강화 시작형 / 루레인 / 빙의된 루퍼스 /
/// 테오도어 / 알테나 groups 59..62 with four rolls and 붉은눈 the same groups
/// with two, exactly the split that module derived from the drop table.
const GROUP_AFTER_NAME: usize = 70;
const ROLLS_AFTER_NAME: usize = 72;

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Install both hooks, or say why this binary cannot carry them.
pub fn install_hooks(vm: &mut Vm) -> Result<Vec<&'static str>, String> {
    for name in [
        SET_DIE_MOTION,
        SET_DROP_ITEM,
        GET_AUTO_PICKUP,
        NEW_DROP_ITEM,
        RAND,
        GET_X,
        GET_Y,
        crate::warehouse::GV,
    ] {
        if vm.symbol(name).is_none() {
            return Err(format!("bossdrop hook needs `{name}`, not in this binary"));
        }
    }
    let die = vm.symbol(SET_DIE_MOTION).expect("checked above");
    let pickup = vm.symbol(GET_AUTO_PICKUP).expect("checked above");
    let f_die: PatchFn = drop_on_death;
    let f_pickup: PatchFn = report_auto_pickup;
    vm.install_hook(SET_DIE_MOTION, die, f_die)?;
    vm.install_hook(GET_AUTO_PICKUP, pickup, f_pickup)?;
    Ok(vec![SET_DIE_MOTION, GET_AUTO_PICKUP])
}

/// `HERO::GetAutoPickupItem`. Re-reads the two words the original returns, and
/// reports a live buff only while this module is holding the window open.
fn report_auto_pickup(vm: &mut Vm) -> Result<bool, String> {
    let this = vm.arg(0);
    let (lo, hi) = if vm.bossdrop.armed > 0 {
        // A positive low word with a zero high word: `bgt` on the high word
        // fails, `bne` fails, `cmp r0,#0 / beq` fails — the bag branch.
        (1, 0)
    } else {
        (
            vm.mem.read_u32(this + hero::AUTO_PICKUP_LO),
            vm.mem.read_u32(this + hero::AUTO_PICKUP_HI),
        )
    };
    vm.cpu.reg_set(Mode::User, 0, lo);
    vm.cpu.reg_set(Mode::User, 1, hi);
    Ok(true)
}

/// `Monster::SetDieMotion` — run the original, then, for the one boss whose
/// death starts a scene, do its drop now and into the bag.
fn drop_on_death(vm: &mut Vm) -> Result<bool, String> {
    let lr = vm.cpu.reg_get(Mode::User, reg::LR);
    if lr == RETURN_MAGIC {
        return Ok(false);
    }
    let this = vm.arg(0);
    let Some(die) = vm.symbol(SET_DIE_MOTION) else {
        return Ok(false);
    };
    vm.call(die, &[this])?;
    if let Err(e) = grant(vm, this) {
        vm.log.push(format!("bossdrop: skipped: {e}"));
        vm.bossdrop.armed = 0;
    }
    Ok(true)
}

/// The whole behaviour change, for one monster, once.
fn grant(vm: &mut Vm, monster: u32) -> Result<(), String> {
    // The original returned without doing anything: this call was not a death.
    if vm.mem.read_u32(monster + monster::HP) as i32 > 0 {
        return Ok(());
    }
    let name = vm.mem.cstr_bytes(monster + monster::NAME);
    if !contains(&name, TARGET_NAME) {
        return Ok(());
    }
    // Hand the dead twin the marked row's numbers. A monster that already
    // rolls keeps every byte the table gave it.
    if !rolls_anything(vm, monster) {
        let difficulty = difficulty(vm);
        let Some(row) = vm.bossdrop.row(difficulty) else {
            vm.log
                .push("bossdrop: 정체모를 괴수 has no drop ladder and none was parsed".into());
            return Ok(());
        };
        for (k, t) in row.thresholds.iter().enumerate() {
            vm.mem.write_u32(monster + monster::T0 + (k as u32) * 4, *t);
        }
        vm.mem.write_bytes(monster + monster::GROUP, &[row.group]);
        vm.mem.write_bytes(monster + monster::ROLLS, &[row.rolls]);
        vm.log.push(format!(
            "bossdrop: 정체모를 괴수 spawned with the empty row; using difficulty {difficulty} \
             group {} × {} roll(s)",
            row.group, row.rolls
        ));
    }
    let (Some(drop), Some(rand)) = (vm.symbol(SET_DROP_ITEM), vm.symbol(RAND)) else {
        return Ok(());
    };

    vm.bossdrop.armed += 1;
    let result = (|vm: &mut Vm| -> Result<(), String> {
        // The guest's own roll, its own grades, its own sockets. `Vm::call`
        // sets `LR = RETURN_MAGIC`, so the hook `legendaries` owns on this
        // address declines and the original body runs.
        vm.call(drop, &[monster])?;
        if vm.call(rand, &[0, 99])? < ACCESSORY_PERCENT {
            let which = vm.call(rand, &[0, (ACCESSORY_ROWS.len() - 1) as u32])? as usize;
            let index = ACCESSORY_ROWS[which.min(ACCESSORY_ROWS.len() - 1)];
            drop_accessory(vm, monster, index)?;
        }
        Ok(())
    })(vm);
    vm.bossdrop.armed -= 1;
    result?;

    // Whatever `EndProc` does with this monster later, it must not roll again.
    for k in 0..RUNGS as u32 {
        vm.mem.write_u32(monster + monster::T0 + k * 4, 0);
    }
    vm.mem.write_bytes(monster + monster::ROLLS, &[0]);
    Ok(())
}

/// One 전설 accessory, through the same door the weapons went through.
fn drop_accessory(vm: &mut Vm, monster: u32, index: u8) -> Result<(), String> {
    if crate::legendaries::table_rows(vm, ACCESSORY_KIND).unwrap_or(0) <= u32::from(index) {
        return Ok(());
    }
    let (Some(new_drop), Some(get_x), Some(get_y), Some(gv)) = (
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
    // Every override char is 0xff: `NewDropItem` reads them with `ldrsb` and
    // only applies a value `>= 0`, so the table's own grade and options stand.
    let ff = 0xffu32;
    vm.call(
        new_drop,
        &[map, x, y, u32::from(ACCESSORY_KIND), u32::from(index), ff, ff, ff, ff, ff, ff, ff],
    )?;
    vm.log
        .push(format!("bossdrop: 정체모를 괴수 also dropped 장신구 row {index}"));
    Ok(())
}

/// Would `SetDropItem` produce anything for this monster as it stands?
fn rolls_anything(vm: &Vm, monster: u32) -> bool {
    if vm.mem.read_u8(monster + monster::ROLLS) == 0 {
        return false;
    }
    (0..RUNGS as u32).any(|k| vm.mem.read_u32(monster + monster::T0 + k * 4) != 0)
}

/// `HERO+0x277`, the same byte `setEnemyData` puts in `enemy_%d.dat`.
fn difficulty(vm: &Vm) -> u8 {
    let Some(gv) = vm.symbol(crate::warehouse::GV) else {
        return 0;
    };
    let hero = vm.mem.read_u32(gv + GV_HERO);
    if hero == 0 {
        return 0;
    }
    vm.mem.read_u8(hero + hero::DIFFICULTY)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `[u16 reclen][u8 namelen][name][39 bytes][7×u32][…]` — the shape
    /// `rates`' own tests build, with the two bytes this module reads.
    fn record(name: &[u8], thresholds: [u32; RUNGS], group: u8, rolls: u8) -> Vec<u8> {
        let mut body = vec![name.len() as u8];
        body.extend_from_slice(name);
        let tail = body.len();
        body.extend(std::iter::repeat(0u8).take(crate::rates::LADDER_AFTER_NAME));
        for t in thresholds {
            body.extend_from_slice(&t.to_le_bytes());
        }
        // Pad out to the two fields, then set them.
        while body.len() <= tail + ROLLS_AFTER_NAME {
            body.push(0);
        }
        body[tail + GROUP_AFTER_NAME] = group;
        body[tail + ROLLS_AFTER_NAME] = rolls;
        let mut rec = (body.len() as u16).to_le_bytes().to_vec();
        rec.extend_from_slice(&body);
        rec
    }

    fn table(recs: &[Vec<u8>]) -> Vec<u8> {
        let mut out = (recs.len() as u16).to_le_bytes().to_vec();
        for r in recs {
            out.extend_from_slice(r);
        }
        out
    }

    /// The shipped `enemy_0.dat` numbers for the marked row.
    const LIVE: [u32; RUNGS] = [655, 655, 655, 33423, 39976, 40632, 41287];
    const DEAD: [u32; RUNGS] = [0; RUNGS];

    #[test]
    fn the_marked_row_wins_over_the_dead_twin() {
        let mut marked = b"'".to_vec();
        marked.extend_from_slice(TARGET_NAME);
        marked.push(b'{');
        // Dead twin first, so a parse that took the first name match would fail.
        let img = table(&[
            record(TARGET_NAME, DEAD, 44, 0),
            record(&marked, LIVE, 44, 4),
        ]);
        let row = marked_row(&img, TARGET_NAME).expect("the live row is found");
        assert_eq!(row, Row { thresholds: LIVE, group: 44, rolls: 4 });
    }

    #[test]
    fn a_table_without_the_boss_yields_nothing() {
        let img = table(&[record(b"goblin", LIVE, 0, 1)]);
        assert!(marked_row(&img, TARGET_NAME).is_none());
    }

    #[test]
    fn only_the_dead_twin_is_not_a_row() {
        let img = table(&[record(TARGET_NAME, DEAD, 44, 0)]);
        assert!(marked_row(&img, TARGET_NAME).is_none());
    }

    #[test]
    fn a_short_image_is_refused_rather_than_guessed() {
        let mut img = table(&[record(TARGET_NAME, LIVE, 44, 4)]);
        img.truncate(img.len() - 1);
        assert!(marked_row(&img, TARGET_NAME).is_none());
    }

    #[test]
    fn accessory_rows_are_the_four_legendary_ones() {
        assert_eq!(ACCESSORY_ROWS, [4, 9, 14, 15]);
        assert_eq!(ACCESSORY_KIND, 10);
    }
}
