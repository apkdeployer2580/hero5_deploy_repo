//! Deliberate probability changes, shipped by handing the guest a rewritten
//! copy of its own data file.
//!
//! # Why this needs no hook and no patch
//!
//! `StaticUtil::LoadRes` (0x144E80) — the one door every `/c/...` resource
//! comes through — asks the **host filesystem first**:
//!
//! ```text
//!   0x144EC0  bl MC_knlSprintk        ; the template is a bare "%s" (0x156F28),
//!                                     ; so the path arrives verbatim
//!   0x144ED0  bl MC_fsFileAttribute
//!   0x144ED4  cmp r0, #0
//!   0x144EDC  bne 0x144EEC            ; failed -> fall through to the archive
//!   0x144EE0  ldr r6, [sp, #0x108]    ; the size it reported
//!   0x144EE8  bne 0x144F40            ; non-zero -> READ IT FROM THE HOST FS
//!   0x144EEC  bl CUSTOM_knlGetResourceID   ; only now the VFS
//! ```
//!
//! and the host's own `stat`/`open` re-root a guest path under
//! [`Config::storage`](crate::Config::storage) ([`crate::Vm::host_path`]). So
//! putting a file where that lands makes the guest load *that* instead of the
//! archive's copy — **zero guest hooks, zero entries in [`crate::patch`], and
//! the guest's own parser does the reading.**
//!
//! **Where it lands is not the bare path.** `MC_fsFileAttribute` (0x14C0B8)
//! runs the path through `MIDASKernelManager::getFullFilePath` (0x151E40)
//! first, which with mode 1 — the mode `LoadRes` passes (0x144ECC `mov r2,#1`)
//! — returns `root + '/' + path`. So the file belongs at
//! `<storage>/data/data/co.kr.eamobile.HeroesLore5/files/c/csv/probability.dat`
//! ([`crate::INTERNAL_STORAGE_ROOT`]), which is the same tree the guest's own
//! saves live in. Written to `<storage>/c/csv/...` it is simply never stat'd,
//! and the only symptom is that the rates silently stay vanilla.
//!
//! That is the whole mechanism. The alternative — hooking
//! `RefineItem::ApplyOrbCombine` and rewriting its comparison — would put a
//! behaviour change inside the interpreter for something the game is perfectly
//! willing to read out of a file.
//!
//! # Why the bytes are computed and not checked in
//!
//! The override is derived from the archive's own copy at boot: read the
//! original, rewrite the fields that are changing, write the result out. A
//! pre-built 614-byte blob in the tree would be unreviewable and would rot
//! silently if the archive ever differed. This way the diff is the code below,
//! and a wrong offset fails loudly in [`patch_probability`] rather than
//! shipping a table nobody can read.

use std::path::Path;

/// The guest path, and therefore also the path under
/// [`Config::storage`](crate::Config::storage). No leading `/`: that is how
/// the VFS keys it (`tempLocalGetExactFilename`, 0x387D0, skips a leading
/// separator), and [`crate::Vm::host_path`] treats both the same.
pub const PROBABILITY_PATH: &str = "c/csv/probability.dat";

/// Orb-combine success thresholds in per mille, indexed by **how many copies
/// of that same orb are already socketed in the item** — that index is what
/// `ApplyOrbCombine` counts at 0xA1F34 before choosing the row.
///
/// The guest's shipped values are 400 / 200 / 50 / 10 / 5 (= 40 / 20 / 5 / 1 /
/// 0.5%). These are the requested 50 / 45 / 40 / 35 / 30%.
///
/// The 달성의부적 doubling is untouched and keeps working: it is a runtime
/// multiplier on whatever this threshold is (`mul r2, ip, r2` at 0xA1F9C, with
/// `ip` = 2 only when the consumed `kind 18` row is 9 — 0xA1EF4), so the first
/// row simply reaches 1000 and `Rand(0,999)` can no longer beat it. 1000 is
/// representable: the threshold is read with `ldrsh` (0xA1F98) and 1000 is far
/// inside a signed halfword.
pub const ORB_SUCCESS: [u16; 5] = [500, 450, 400, 350, 300];

/// `probability.dat` rows that `LoadProbablityTable` sends to block B — the
/// orb-combine block at `ItemTable+0xB4` (ANALYSIS.md §41.1).
const ORB_ROWS: std::ops::Range<usize> = 6..11;

/// How many of a row's five values the orb block keeps. The loader copies
/// three (`r6` walks `r4+2 .. r4+8`, 0xA0D34..0xA0D4C), and the walk in
/// `ApplyOrbCombine` reads them as `[success, wipe, 1000]`.
const ORB_KEPT: usize = 3;

/// Rewrite the orb-combine success thresholds in a `probability.dat` image.
///
/// Returns `None` — rather than a half-rewritten file — if the image does not
/// parse exactly, because the fallback (the archive's own copy) is correct and
/// a mangled override would be a silent gameplay bug.
///
/// # What gets written, and the one field that moves with it
///
/// Element 0 is the success threshold. Element 1 is **not** a second grade of
/// success — it ends the band that *punishes* you. `ApplyOrbCombine`'s walk
/// hands `[T0, T1)` to 0xA21DC, which rolls a **third** `Rand(0,999)`
/// (0xA21E8) and splits that band in half at 500 (0xA21EC `cmp r0, #0x1f4`):
/// below it the **item is destroyed** (0xA2240), above it the item survives but
/// **every orb already socketed is wiped**. Only `[T1, 1000)` is the harmless
/// failure that costs nothing but the orb being inserted (0xA1FC4).
///
/// So raising success eats into the destroy/wipe band, and that is not a side
/// effect to apologise for — at four matching orbs the shipped table destroys
/// or wipes on **80%** of attempts, and moving `T0` from 5 to 300 takes that to
/// 50.5%.
///
/// If a new `T0` passed `T1` the ladder would read backwards, so `T1` is lifted
/// to `max(T0, T1)`. That is not a second policy change: the walk is
/// sequential, so a `T1` below `T0` already yields an *empty* punish band (a
/// roll that failed `< T0` also fails `< T1`). Lifting it makes the table say
/// what the code already does and leaves a monotone ladder for the next reader.
pub fn patch_probability(src: &[u8]) -> Option<Vec<u8>> {
    let count = usize::from(u16::from_le_bytes([*src.first()?, *src.get(1)?]));
    let mut out = src.to_vec();
    let mut pos = 2usize;
    for row in 0..count {
        let reclen = usize::from(u16::from_le_bytes([*out.get(pos)?, *out.get(pos + 1)?]));
        let body = pos + 2;
        if body.checked_add(reclen)? > out.len() {
            return None;
        }
        if ORB_ROWS.contains(&row) {
            // Refuse a row too short to hold the fields we are about to move:
            // the archive's rows are all reclen 10 (five u16).
            if reclen < ORB_KEPT * 2 {
                return None;
            }
            let want = ORB_SUCCESS[row - ORB_ROWS.start];
            let at = |i: usize| body + i * 2;
            let read = |o: &[u8], i: usize| u16::from_le_bytes([o[at(i)], o[at(i) + 1]]);
            let wipe = read(&out, 1).max(want);
            out[at(0)..at(0) + 2].copy_from_slice(&want.to_le_bytes());
            out[at(1)..at(1) + 2].copy_from_slice(&wipe.to_le_bytes());
        }
        pos = body + reclen;
    }
    // The parse must land exactly on the last byte, the same closure check
    // every other table parse in this tree makes.
    if pos != out.len() {
        return None;
    }
    Some(out)
}

/// The three difficulty variants of the monster table. `Monster::setEnemyData`
/// (0xC1A94) picks one with `MC_knlSprintk("/c/csv/enemy_%d.dat", ...)`
/// (0x158D38), and only 0, 1 and 2 exist in the archive.
pub const ENEMY_PATHS: [&str; 3] =
    ["c/csv/enemy_0.dat", "c/csv/enemy_1.dat", "c/csv/enemy_2.dat"];

/// Offset of the seven drop thresholds inside one `enemy_<N>.dat` record,
/// measured from the byte after the name.
///
/// `setEnemyData` reads them with `ByteToInt32` at `r6+0x26 .. r6+0x3e`
/// (0xC1D74..0xC1DD4), and `r6` has by then walked past the record's
/// `[u16 reclen][u8 namelen][name]` plus one further byte — it sits at
/// `record_start + 4 + namelen`, which is 39 past the end of the name. That is
/// arithmetic, not a guess, and it is confirmed independently: 39 is the only
/// offset in the record that yields a monotone ladder within 65536 for all 166
/// rows of all three files.
pub(crate) const LADDER_AFTER_NAME: usize = 39;

/// Take back an override this shell wrote in an earlier build.
///
/// **Not writing a file is not the same as undoing it.** `LoadRes` prefers the
/// host filesystem forever (see the module header), so a device that once ran
/// a build which widened the 전설 band keeps reading that widened copy on every
/// later boot. The shipped rate comes back only when the file is deleted.
///
/// `Ok(false)` — nothing was there — is the normal case.
pub(crate) fn remove_override(storage: &Path, path: &str) -> std::io::Result<bool> {
    let dst = storage
        .join(crate::INTERNAL_STORAGE_ROOT.trim_start_matches('/'))
        .join(path);
    match std::fs::remove_file(&dst) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// Put every monster's drop ladder back to the one the game shipped with.
///
/// Operator decision, 2026-09-07: **drop rates are the game's own on both
/// content lines.** An earlier build widened the 전설 band (`[T4, T5)`) to
/// 5% per roll, and the two APKs then disagreed in the field — one install had
/// that override on disk and the other did not, which read as "vanilla drops
/// less than main" when the content line has nothing to do with drop rates.
/// Rather than make the widening reliable, it is withdrawn: the rate is the
/// game's on every install, and the only thing the content line decides is
/// *which* row the hell-boss extra roll can land on
/// (`legendaries::extra_endgame_rolls`).
///
/// Runs on every boot, whatever [`crate::Config::boost_rates`] says, because
/// the file it removes was written by a build that is already out there. Orb
/// combine and 조합 are unaffected — those are different files.
pub fn restore_shipped_drop_bands(storage: &Path) -> Vec<String> {
    let mut logs = Vec::new();
    for path in ENEMY_PATHS {
        match remove_override(storage, path) {
            Ok(true) => logs.push(format!(
                "rates: removed the widened {path} an earlier build left — 전설 band is the shipped one again"
            )),
            Ok(false) => {}
            Err(e) => logs.push(format!("rates: cannot remove the {path} override: {e}")),
        }
    }
    logs
}

/// Write one override to the place the guest will actually stat it. See the
/// module header for why the internal storage root is part of that path.
pub(crate) fn write_override(storage: &Path, path: &str, bytes: &[u8]) -> std::io::Result<std::path::PathBuf> {
    let dst = storage
        .join(crate::INTERNAL_STORAGE_ROOT.trim_start_matches('/'))
        .join(path);
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&dst, bytes)?;
    Ok(dst)
}

/// `/c/csv/smith_<N>.dat`, the 조합 recipe tables — one per difficulty, the
/// same 0/1/2 selector `enemy_<N>.dat` uses (`HERO+0x277`).
pub const SMITH_PATHS: [&str; 3] =
    ["c/csv/smith_0.dat", "c/csv/smith_1.dat", "c/csv/smith_2.dat"];

/// What every real 조합 recipe's success chance becomes. Shipped is 75.
///
/// The roll is `StaticUtil::Rand(0,99)` and the compare is
/// `RefineItem::ApplyNormalMix+0xb0`: `roll >= pct` fails. `Rand` is inclusive,
/// so this is a true percent — 100 always succeeds and 0 never does.
///
/// **Never above 100.** The byte is read with `ldrb` and then sign-extended
/// (`lsl #0x18` / `asr #0x18` at 0xa7dbc/0xa7dc0), so 128..255 arrive negative
/// and `roll >= pct` is always true: the recipe becomes impossible.
///
/// Raising it is also the only way to stop a failure eating the materials —
/// they are deleted *before* the roll (loop 0xa7d58..0xa7da8, roll at
/// 0xa7db4) and there is no refund path to hook.
pub const COMPOSE_SUCCESS_PCT: u8 = 95;

/// The 13-byte fixed tail every `smith_<N>.dat` record ends with. The success
/// percent is its last byte, and the result kind is at +10 — read off
/// `HERO::LoadMixSmithTableInfo` (0x8b958) and then checked by re-parsing all
/// three shipped files, whose walks land exactly on EOF.
const SMITH_TAIL: usize = 13;
/// Offset of the result-kind byte within that tail. `0xFF` marks a `NONE`
/// padding row.
const SMITH_TAIL_KIND: usize = 10;
/// Offset of the result **index** within that tail.
///
/// `LoadMixSmithTableInfo` writes it to `MixSmithTableInfo+0x127`
/// (`strb` at 0x8BB60, the byte after the kind at +0x126), and
/// `ApplyNormalMix` (0xA7DF8) hands that pair to `GetItemTableInfo`.
const SMITH_TAIL_INDEX: usize = 11;
/// Offset of the success percent within that tail.
const SMITH_TAIL_PCT: usize = 12;
/// Third material in a 3-ingredient tail: kind at +7, index at +8, count at +9.
const SMITH_TAIL_MAT3_KIND: usize = 7;
const SMITH_TAIL_MAT3_INDEX: usize = 8;

/// `tail[0] == 4` means three materials plus the result pair — the shape of
/// the unique-weapon 전설 crafts (알렉스의 검 / 루퍼스의 단검 / 88mm포신 /
/// 에블린의 창 as the third ingredient).
const SMITH_THREE_MATS: u8 = 4;

/// Shipped result index of the Hell **second pair** (알케인 / 펜리르 /
/// 데스크림존 / 궁니르). It is the Normal second pair's index — 미노 / 녹스 /
/// 시엘라 / 세라딘, `item_00..03` row 44, level 43. The Hell rows are 80.
const SHIPPED_HELL_PAIR_B_INDEX: u8 = 44;
/// Where those four recipes should point: `item_00..03` row 80, level 97.
const HELL_PAIR_B_INDEX: u8 = 80;

/// `item_13.dat` indices of the four unique weapons the hidden-blacksmith
/// quest line consumes. Pair-B identification uses these rather than the
/// EUC-KR name, so a renamed row still matches and a coincidental
/// `index == 44` craft does not.
const UNIQUE_WEAPON_INDICES: [u8; 4] = [0x4e, 0x51, 0x54, 0x55];

/// Tail offsets of every record, or `None` if the image is not the loader's
/// layout. Same walk [`patch_smith`] and [`fix_hell_legendary_results`] share,
/// so a file that one refuses the other refuses.
fn smith_tail_offsets(src: &[u8]) -> Option<Vec<usize>> {
    let count = usize::from(u16::from_le_bytes([*src.first()?, *src.get(1)?]));
    let mut pos = 2usize;
    let mut tails = Vec::with_capacity(count);
    for _ in 0..count {
        let reclen = usize::from(u16::from_le_bytes([*src.get(pos)?, *src.get(pos + 1)?]));
        let body = pos + 2;
        let end = body.checked_add(reclen)?;
        if end > src.len() {
            return None;
        }
        let namelen = usize::from(*src.get(body + 2)?);
        let after_name = body.checked_add(3)?.checked_add(namelen)?;
        let desclen = usize::from(*src.get(after_name + 4)?);
        let tail = after_name.checked_add(5)?.checked_add(desclen)?;
        if tail.checked_add(SMITH_TAIL)? != end {
            return None;
        }
        tails.push(tail);
        pos = end;
    }
    if pos != src.len() {
        return None;
    }
    Some(tails)
}

/// Lift every real recipe's success percent to [`COMPOSE_SUCCESS_PCT`].
///
/// One byte per record and no length change, so `reclen` stays valid and the
/// parse-lands-on-EOF self-check still holds afterwards. Returns the new image
/// and how many rows were lifted, or `None` if the layout is not what the
/// loader says — in which case the caller leaves the shipped file alone.
///
/// Rows deliberately left untouched: the `NONE` padding rows, and anything
/// already at or above the target (it is never lowered).
///
/// The padding test is the result-kind byte being `0xFF`, and it is not a
/// guess — it was cross-checked against the name being literally `NONE` over
/// all three shipped files: 27 / 15 / 15 rows, **zero disagreements**. That
/// check mattered, because `smith_0.dat` has 79 rows at 75% but only 60 real
/// ones — nineteen of its padding rows carry a 75 that would otherwise look
/// like a recipe.
pub fn patch_smith(src: &[u8]) -> Option<(Vec<u8>, usize)> {
    let tails = smith_tail_offsets(src)?;
    let mut out = src.to_vec();
    let mut lifted = 0usize;
    for tail in tails {
        let pct = out[tail + SMITH_TAIL_PCT];
        let kind = out[tail + SMITH_TAIL_KIND];
        if pct != 0 && kind != 0xff && pct < COMPOSE_SUCCESS_PCT {
            out[tail + SMITH_TAIL_PCT] = COMPOSE_SUCCESS_PCT;
            lifted += 1;
        }
    }
    Some((out, lifted))
}

/// Point Hell's unique-weapon **second pair** at the Hell items, not the
/// Normal ones. ANALYSIS.md §47.
///
/// `smith_2.dat` rows 84–87 are named 알케인:혼돈 / 펜리르:야성검 /
/// 데스크림존 / 궁니르:번개 and consume Hell-tier materials, but the result
/// index is the shipped Normal pair-B value 44. `ApplyNormalMix` then builds
/// 미노:창공의검 / 녹스:밤의검 / 시엘라:매의눈 / 세라딘:척살 (level 43)
/// instead of the level-97 rows at index 80.
///
/// The first pair on the same file (이시르 / 프레키 / 바이드 / 에테르, index
/// 79) is already correct and is not touched — its index is 79, not 44.
///
/// **Only call this on `smith_2`.** The same predicate matches Normal
/// `smith_0` pair B, where 44 is the right answer.
///
/// Identification is the tail shape, not the name: three materials, result
/// kind 0..3, result index 44, third ingredient one of the four unique
/// weapons. A renamed row still matches; a coincidental index-44 craft does
/// not. Idempotent: an already-80 row is left alone.
pub fn fix_hell_legendary_results(src: &[u8]) -> Option<(Vec<u8>, usize)> {
    let tails = smith_tail_offsets(src)?;
    let mut out = src.to_vec();
    let mut fixed = 0usize;
    for tail in tails {
        let kind = out[tail + SMITH_TAIL_KIND];
        let index = out[tail + SMITH_TAIL_INDEX];
        let mats = out[tail];
        let mat3_kind = out[tail + SMITH_TAIL_MAT3_KIND];
        let mat3_index = out[tail + SMITH_TAIL_MAT3_INDEX];
        if mats == SMITH_THREE_MATS
            && kind <= 3
            && index == SHIPPED_HELL_PAIR_B_INDEX
            && mat3_kind == 13
            && UNIQUE_WEAPON_INDICES.contains(&mat3_index)
        {
            out[tail + SMITH_TAIL_INDEX] = HELL_PAIR_B_INDEX;
            fixed += 1;
        }
    }
    Some((out, fixed))
}

/// Materialise every override under `storage`, so `LoadRes` finds them.
///
/// Returns one log line per file for the boot log — the asymmetry §39.6 fixed
/// applies here too: a change this quiet must say what it did, or the first
/// question ("did the new rate actually take?") has no answer.
///
/// Failures are reported and skipped, never fatal: without the file the guest
/// reads the archive and plays at the original rates.
pub fn install(vfs: &[u8], storage: &Path) -> Vec<String> {
    let mut logs = Vec::new();
    let Some(src) = crate::shop::vfs_entry(vfs, PROBABILITY_PATH) else {
        logs.push(format!("rates: {PROBABILITY_PATH} is not in the archive — rates unchanged"));
        return logs;
    };
    let Some(patched) = patch_probability(src) else {
        logs.push(format!(
            "rates: {PROBABILITY_PATH} ({} B) did not parse — rates unchanged",
            src.len()
        ));
        return logs;
    };
    // Not `storage.join(PROBABILITY_PATH)`: the guest never stats a bare
    // `/c/...` path. `MIDASKernelManager::getFullFilePath` (0x151E40) prefixes
    // the internal storage root first, so the file has to sit where that
    // prefixed path lands. See [`crate::INTERNAL_STORAGE_ROOT`].
    let dst = storage
        .join(crate::INTERNAL_STORAGE_ROOT.trim_start_matches('/'))
        .join(PROBABILITY_PATH);
    if let Some(parent) = dst.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            logs.push(format!("rates: cannot create {}: {e}", parent.display()));
            return logs;
        }
    }
    match std::fs::write(&dst, &patched) {
        Ok(()) => logs.push(format!(
            "rates: {} B -> {} (orb success {:?} per mille)",
            patched.len(),
            dst.display(),
            ORB_SUCCESS
        )),
        Err(e) => logs.push(format!("rates: cannot write {}: {e}", dst.display())),
    }

    // 조합 success. Unlike the orb table above this is not in
    // `probability.dat` at all: it is one byte per recipe, and all three
    // difficulty files carry their own copy of the same recipe list.
    for (i, path) in SMITH_PATHS.iter().enumerate() {
        write_smith(vfs, storage, path, true, i == 2, &mut logs);
    }
    logs
}

/// The Hell pair-B index fix, without the success-rate lift.
///
/// This is a shipped-data typo, not a rate policy, so it must run even when
/// [`crate::Config::boost_rates`] is off. [`install`] applies the same remap
/// on `smith_2` after lifting percents; calling both would let the second
/// write wipe the first.
pub fn install_hell_legendaries(vfs: &[u8], storage: &Path) -> Vec<String> {
    let mut logs = Vec::new();
    write_smith(vfs, storage, SMITH_PATHS[2], false, true, &mut logs);
    logs
}

fn write_smith(
    vfs: &[u8],
    storage: &Path,
    path: &str,
    lift_pct: bool,
    fix_legend: bool,
    logs: &mut Vec<String>,
) {
    let Some(src) = crate::shop::vfs_entry(vfs, path) else {
        logs.push(format!("rates: {path} is not in the archive - 조합 unchanged"));
        return;
    };
    let mut img = src.to_vec();
    let mut lifted = 0usize;
    let mut fixed = 0usize;
    if lift_pct {
        let Some((patched, n)) = patch_smith(&img) else {
            logs.push(format!(
                "rates: {path} ({} B) did not parse - 조합 unchanged",
                src.len()
            ));
            return;
        };
        img = patched;
        lifted = n;
    }
    if fix_legend {
        let Some((patched, n)) = fix_hell_legendary_results(&img) else {
            logs.push(format!(
                "rates: {path} ({} B) did not parse - Hell legendaries unchanged",
                img.len()
            ));
            return;
        };
        img = patched;
        fixed = n;
    }
    match write_override(storage, path, &img) {
        Ok(dst) => {
            let mut note = format!("rates: {} B -> {}", img.len(), dst.display());
            if lift_pct {
                note.push_str(&format!(" ({lifted} 조합 recipe(s) now {COMPOSE_SUCCESS_PCT}%"));
                if fix_legend {
                    note.push_str(&format!(
                        ", {fixed} Hell legendary result(s) remapped {SHIPPED_HELL_PAIR_B_INDEX}→{HELL_PAIR_B_INDEX})"
                    ));
                } else {
                    note.push(')');
                }
            } else if fix_legend {
                note.push_str(&format!(
                    " ({fixed} Hell legendary result(s) remapped {SHIPPED_HELL_PAIR_B_INDEX}→{HELL_PAIR_B_INDEX})"
                ));
            }
            logs.push(note);
        }
        Err(e) => logs.push(format!("rates: {path}: {e}")),
    }
}

#[cfg(test)]
mod tests {

    /// Build one `smith_<N>.dat` record: `[u16 reclen][u16 id][u8 nameLen]
    /// [name][u32 money][u8 descLen][desc][13-byte tail]`.
    fn smith_rec(id: u16, name: &[u8], desc: &[u8], kind: u8, pct: u8) -> Vec<u8> {
        smith_rec_tail(id, name, desc, [0u8; 13], kind, 0, pct)
    }

    /// A 3-material unique-weapon 전설 craft: `tail[0] = 4`, third ingredient
    /// kind 13 / `weapon`, result (`kind`, `index`).
    fn smith_legend_rec(
        name: &[u8],
        result_kind: u8,
        result_index: u8,
        weapon: u8,
        pct: u8,
    ) -> Vec<u8> {
        let mut tail = [0u8; 13];
        tail[0] = super::SMITH_THREE_MATS;
        tail[super::SMITH_TAIL_MAT3_KIND] = 13;
        tail[super::SMITH_TAIL_MAT3_INDEX] = weapon;
        smith_rec_tail(212, name, b"mats", tail, result_kind, result_index, pct)
    }

    fn smith_rec_tail(
        id: u16,
        name: &[u8],
        desc: &[u8],
        mut tail: [u8; 13],
        kind: u8,
        index: u8,
        pct: u8,
    ) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&id.to_le_bytes());
        body.push(name.len() as u8);
        body.extend_from_slice(name);
        body.extend_from_slice(&0u32.to_le_bytes());
        body.push(desc.len() as u8);
        body.extend_from_slice(desc);
        tail[super::SMITH_TAIL_KIND] = kind;
        tail[super::SMITH_TAIL_INDEX] = index;
        tail[super::SMITH_TAIL_PCT] = pct;
        body.extend_from_slice(&tail);
        let mut out = (body.len() as u16).to_le_bytes().to_vec();
        out.extend_from_slice(&body);
        out
    }

    fn smith_file(recs: &[Vec<u8>]) -> Vec<u8> {
        let mut v = (recs.len() as u16).to_le_bytes().to_vec();
        for r in recs {
            v.extend_from_slice(r);
        }
        v
    }

    #[test]
    fn patch_smith_lifts_only_the_real_recipes_and_keeps_the_length() {
        let src = smith_file(&[
            smith_rec(204, b"AA", b"desc one", 0x05, 75),  // real -> lifted
            smith_rec(212, b"BB", b"d", 0x0d, 100),        // already certain
            smith_rec(0, b"NONE", b"", 0xff, 0),           // padding
            smith_rec(205, b"CC", b"xy", 0x06, 75),        // real -> lifted
        ]);
        let (out, lifted) = patch_smith(&src).expect("parses");
        assert_eq!(lifted, 2, "only the two 75% rows");
        assert_eq!(out.len(), src.len(), "length must not change");
        // Byte-level: exactly two bytes differ, and both are a percent byte.
        let diff: Vec<usize> =
            (0..src.len()).filter(|&i| src[i] != out[i]).collect();
        assert_eq!(diff.len(), 2, "changed {diff:?}");
        for i in diff {
            assert_eq!(src[i], 75);
            assert_eq!(out[i], COMPOSE_SUCCESS_PCT);
        }
    }

    #[test]
    fn patch_smith_never_writes_an_impossible_percent() {
        // Above 100 the guest sign-extends the byte and the recipe becomes
        // unreachable, so the constant itself is the thing under test.
        assert!(COMPOSE_SUCCESS_PCT <= 100, "a percent above 100 goes negative");
        assert!(COMPOSE_SUCCESS_PCT > 0);
    }

    #[test]
    fn patch_smith_refuses_a_table_whose_tail_does_not_close_the_record() {
        // A record one byte short of its 13-byte tail means the layout is not
        // what we think; writing into it would corrupt the file.
        let mut src = smith_file(&[smith_rec(204, b"AA", b"d", 0x05, 75)]);
        let n = src.len();
        src[2] = (n - 4 - 1) as u8; // shrink reclen by one
        assert!(patch_smith(&src).is_none());
        // Trailing slop must be refused too.
        let mut slop = smith_file(&[smith_rec(204, b"AA", b"d", 0x05, 75)]);
        slop.push(0);
        assert!(patch_smith(&slop).is_none());
    }

    #[test]
    fn hell_pair_b_is_remapped_to_the_hell_row_and_nothing_else_moves() {
        // Four pair-B crafts (the shipped bug), one pair-A (already 79), one
        // ordinary recipe whose result happens to be 44.
        let src = smith_file(&[
            smith_legend_rec(b"al", 0, super::SHIPPED_HELL_PAIR_B_INDEX, 0x51, 75),
            smith_legend_rec(b"pe", 1, super::SHIPPED_HELL_PAIR_B_INDEX, 0x4e, 75),
            smith_legend_rec(b"de", 2, super::SHIPPED_HELL_PAIR_B_INDEX, 0x55, 75),
            smith_legend_rec(b"gu", 3, super::SHIPPED_HELL_PAIR_B_INDEX, 0x54, 75),
            smith_legend_rec(b"is", 0, 79, 0x51, 75),
            smith_rec(204, b"hat", b"d", 5, 75),
        ]);
        let (out, n) = fix_hell_legendary_results(&src).expect("parses");
        assert_eq!(n, 4);
        assert_eq!(out.len(), src.len());
        let diff: Vec<usize> = (0..src.len()).filter(|&i| src[i] != out[i]).collect();
        assert_eq!(diff.len(), 4, "changed {diff:?}");
        for i in diff {
            assert_eq!(src[i], super::SHIPPED_HELL_PAIR_B_INDEX);
            assert_eq!(out[i], super::HELL_PAIR_B_INDEX);
        }
        let (again, n2) = fix_hell_legendary_results(&out).expect("idempotent");
        assert_eq!(n2, 0);
        assert_eq!(again, out);
    }

    #[test]
    fn a_coincidental_index_44_without_the_unique_weapon_is_left_alone() {
        let mut tail = [0u8; 13];
        tail[0] = super::SMITH_THREE_MATS;
        tail[super::SMITH_TAIL_MAT3_KIND] = 13;
        tail[super::SMITH_TAIL_MAT3_INDEX] = 26; // 하급마력 금속, not a unique weapon
        let src = smith_file(&[smith_rec_tail(
            199,
            b"xx",
            b"d",
            tail,
            0,
            super::SHIPPED_HELL_PAIR_B_INDEX,
            75,
        )]);
        let (out, n) = fix_hell_legendary_results(&src).expect("parses");
        assert_eq!(n, 0);
        assert_eq!(out, src);
    }

    /// Against the real archive, if it is present: exactly four rows move,
    /// each from 44 to 80, and the file keeps its size.
    #[test]
    fn the_shipped_hell_table_has_four_pair_b_rows_at_index_44() {
        let Ok(vfs) = std::fs::read("../payload/data.vfs.mp3") else {
            return;
        };
        let Some(src) = crate::shop::vfs_entry(&vfs, SMITH_PATHS[2]) else {
            return;
        };
        let (out, n) = fix_hell_legendary_results(src).expect("the shipped table must parse");
        assert_eq!(n, 4, "smith_2 pair B is four recipes");
        assert_eq!(out.len(), src.len());
        let (again, n2) = fix_hell_legendary_results(&out).expect("idempotent");
        assert_eq!(n2, 0);
        assert_eq!(again, out);
        // Normal pair B is also four rows at 44 — that is why this function
        // must never be pointed at `smith_0`. Hard is already 72, so it
        // would be a no-op, but we still do not write that file.
        let Some(normal) = crate::shop::vfs_entry(&vfs, SMITH_PATHS[0]) else {
            return;
        };
        let (_, n0) = fix_hell_legendary_results(normal).expect("parses");
        assert_eq!(n0, 4, "smith_0 pair B is the correct 44; do not remap it");
        let Some(hard) = crate::shop::vfs_entry(&vfs, SMITH_PATHS[1]) else {
            return;
        };
        let (_, n1) = fix_hell_legendary_results(hard).expect("parses");
        assert_eq!(n1, 0, "smith_1 pair B is already 72");
    }

    use super::*;

    /// Build a `probability.dat`-shaped image: `[u16 count]` then `count` ×
    /// (`[u16 reclen=10]` + five little-endian u16).
    fn image(rows: &[[u16; 5]]) -> Vec<u8> {
        let mut v = (rows.len() as u16).to_le_bytes().to_vec();
        for r in rows {
            v.extend_from_slice(&10u16.to_le_bytes());
            for x in r {
                v.extend_from_slice(&x.to_le_bytes());
            }
        }
        v
    }

    fn row(img: &[u8], row: usize) -> [u16; 5] {
        let mut out = [0u16; 5];
        let body = 2 + row * 12 + 2;
        for (i, o) in out.iter_mut().enumerate() {
            *o = u16::from_le_bytes([img[body + i * 2], img[body + i * 2 + 1]]);
        }
        out
    }

    /// The archive's real rows 6..10, so the test pins the actual shipped
    /// transition and not a synthetic one.
    const SHIPPED: [[u16; 5]; 5] = [
        [400, 500, 1000, 1000, 1000],
        [200, 400, 1000, 1000, 1000],
        [50, 450, 1000, 1000, 1000],
        [10, 610, 1000, 1000, 1000],
        [5, 805, 1000, 1000, 1000],
    ];

    fn shipped_image() -> Vec<u8> {
        let mut rows = vec![[0u16, 0, 0, 0, 0]; 6];
        rows.extend_from_slice(&SHIPPED);
        rows.push([1u16, 2, 3, 4, 5]); // a row after the block, to catch overrun
        image(&rows)
    }

    #[test]
    fn the_orb_rows_get_the_requested_success_rates() {
        let out = patch_probability(&shipped_image()).expect("the shipped shape parses");
        for (i, want) in ORB_SUCCESS.iter().enumerate() {
            assert_eq!(row(&out, 6 + i)[0], *want, "row {} success threshold", 6 + i);
        }
    }

    /// Only element 0 and (where it must move) element 1 change. Nothing
    /// outside rows 6..10 is touched at all.
    #[test]
    fn nothing_outside_the_orb_block_moves() {
        let src = shipped_image();
        let out = patch_probability(&src).expect("parses");
        assert_eq!(out.len(), src.len(), "the override is the same size");
        for r in (0..6).chain(std::iter::once(11)) {
            assert_eq!(row(&out, r), row(&src, r), "row {r} must be untouched");
        }
        for i in 0..5 {
            let (a, b) = (row(&src, 6 + i), row(&out, 6 + i));
            assert_eq!(a[2..], b[2..], "row {}: elements 2..5 must be untouched", 6 + i);
        }
    }

    /// The wipe band must never read backwards — and row 7 is the one that
    /// forces it (success 450 over a shipped wipe threshold of 400).
    #[test]
    fn the_wipe_threshold_is_lifted_only_where_it_would_invert() {
        let out = patch_probability(&shipped_image()).expect("parses");
        for i in 0..5 {
            let r = row(&out, 6 + i);
            assert!(r[0] <= r[1], "row {}: success {} passed wipe {}", 6 + i, r[0], r[1]);
            assert!(r[1] <= r[2], "row {}: wipe {} passed {}", 6 + i, r[1], r[2]);
        }
        assert_eq!(row(&out, 7)[1], 450, "row 7's wipe threshold rises to meet success");
        // The other four already had room, so they must be byte-identical.
        for i in [0usize, 2, 3, 4] {
            assert_eq!(
                row(&out, 6 + i)[1],
                SHIPPED[i][1],
                "row {} had room; its wipe threshold must not move",
                6 + i
            );
        }
    }

    /// The doubling amulet is a runtime multiplier, so the shipped first row
    /// reaches exactly 1000 with it — full success, and still inside a signed
    /// halfword (the guest loads the threshold with `ldrsh`, 0xA1F98).
    #[test]
    fn the_doubling_amulet_still_fits_in_a_signed_halfword() {
        for t in ORB_SUCCESS {
            let doubled = u32::from(t) * 2;
            assert!(doubled <= 0x7fff, "doubled threshold {doubled} overflows ldrsh");
        }
        assert_eq!(u32::from(ORB_SUCCESS[0]) * 2, 1000, "the first row reaches certainty");
    }

    /// Removing an override is what actually restores the shipped rate: the
    /// guest prefers the host filesystem, so merely not writing the file would
    /// leave an earlier build's widened ladder in place forever.
    #[test]
    fn removing_an_override_is_what_restores_the_shipped_rate() {
        let root = std::env::temp_dir().join(format!("hl5_rates_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dst = root
            .join(crate::INTERNAL_STORAGE_ROOT.trim_start_matches('/'))
            .join(ENEMY_PATHS[0]);
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        std::fs::write(&dst, b"widened").unwrap();
        assert!(dst.exists());

        let logs = restore_shipped_drop_bands(&root);
        assert!(!dst.exists(), "the override must be gone");
        assert_eq!(logs.len(), 1, "and it must say so: {logs:?}");

        // Idempotent: a clean install says nothing at all.
        assert!(restore_shipped_drop_bands(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_malformed_image_is_refused_rather_than_half_written() {
        // A count that promises more rows than the bytes hold.
        let mut bad = shipped_image();
        bad[0] = 99;
        assert!(patch_probability(&bad).is_none(), "a short image must be refused");
        // Trailing junk: the parse must not land on the last byte.
        let mut trailing = shipped_image();
        trailing.push(0);
        assert!(patch_probability(&trailing).is_none(), "trailing bytes must be refused");
        assert!(patch_probability(&[]).is_none(), "an empty image must be refused");
        assert!(patch_probability(&[3, 0]).is_none(), "a header with no rows must be refused");
    }

    /// A row too short to hold the three values the orb block keeps is refused
    /// rather than written past.
    #[test]
    fn a_short_orb_row_is_refused() {
        let mut rows = vec![[0u16; 5]; 6];
        rows.extend_from_slice(&SHIPPED);
        let mut img = image(&rows);
        // Shrink row 6 to four bytes (two u16) and fix up the tail.
        let at = 2 + 6 * 12;
        img[at..at + 2].copy_from_slice(&4u16.to_le_bytes());
        img.drain(at + 2 + 4..at + 2 + 10);
        assert!(patch_probability(&img).is_none(), "a two-value orb row must be refused");
    }
}
