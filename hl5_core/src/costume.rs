//! Costume sheets for the wardrobe ([`crate::avatar`]): our overlays, drawn
//! on the game's own sheet at boot.
//!
//! A costume is designed as a grid of characters on top of a layer the game
//! already ships — a hat on the bare head (`img0/000`) — in `costumes/sets/`,
//! and `costumes/build.py` transcribes those designs into [`COSTUMES`]
//! (`costume_sets.rs`). **The game's pixels never enter this repository**
//! (`payload/` is licensed data and lives outside it): the composite is made
//! here, from the archive the player already has, and written where `LoadRes`
//! looks first — `<storage>/data/data/co.kr.eamobile.HeroesLore5/files/c/sp/
//! img<N>/<sheet>.mgr` ([`crate::INTERNAL_STORAGE_ROOT`]).
//!
//! # The format, as `tools_re/mgr.py` decoded it from the guest
//!
//! `.mgr` (`GbmMerger::OpenGbmMerger` 0x85800): `[u32 count]` then `count` x
//! `[u32 size][one GBM]`. GBM (`GbmImage::LoadImageResID` 0x85370):
//! `[u8 type][u8 ncolors][u16 w][u16 h][ncolors x u16 RGB565][pixels]`, the low
//! nibble of `type` the bits per pixel (4 or 8; 4 bpp rows are `ceil(w/2)`
//! bytes, even x in the high nibble). Transparency is the **colour** `0xF81F`,
//! not an index. 427 of 427 shipped sheets round-trip byte for byte through
//! that reading.
//!
//! # What a sheet must keep
//!
//! The animation (`.cif`) places each image **by its centre** (`w/2, h/2`
//! rounded down) and does no bounds check on the image index. So a costume
//! keeps every index the base sheet has (a short sheet would read the heap),
//! and the canvas of a frame that grows is grown by the same amount on both
//! sides, so the centre does not move.
//!
//! # The rig: more drawings, the same animation
//!
//! The hero's cif names ONE head and ONE torso image for a whole motion —
//! the idle bob and the walk only move them by a pixel — and the game shows
//! one cif frame per tick (`<CLASS>::EndProc` -> `CHAR::IncreaseSpriteFrame`,
//! end test `CHAR::IsEndFrame`), so more frames would mean longer motions,
//! later hits and a slower stride. A rigged costume (`costumes/rig.py`)
//! instead APPENDS pose images to its sheet — hair that lags the bob, a hem
//! that swings with the stride — and [`rig_cif`] rewrites the image index of
//! the head and torso parts of the four everyday motions (stand and walk,
//! with and without a weapon) to name them. Frame counts, offsets, effects
//! and every other layer are the archive's bytes. `SPRITE_IMAGE::LoadSprImg`
//! sizes each layer from the sheet's own count (`GetMergedGbmCount`), so a
//! longer sheet loads whole; the variant is only ever paired with rigged
//! sheets ([`crate::avatar`] swaps it in and out with them).

use std::path::Path;

pub use crate::costume_sets::{COSTUMES, RIG_BODY_COUNT, RIG_HEAD_COUNT, RIG_REMAPS};

/// See-through, in every blitter (`onStartApp` sets `SKIP_COLOR` to this in
/// 565 mode).
pub const TRANSPARENT: u16 = 0xF81F;

/// [`Costume::classes`] for a costume the spirit 렌 wears, not the hero: bit
/// 4, above the four hero classes, so no hero wardrobe row ever matches it.
/// Her sheets are `img0/042` (head) and `img1/025` (everything else) — see
/// `spirit_avatar`.
pub const SPIRIT: u8 = 0b1_0000;

/// The two archive sheets 렌 is drawn from (`Battle::LoadSpiritRes`, 0x4908C).
pub const SPIRIT_HEAD_BASE: &str = "c/sp/img0/042.mgr";
pub const SPIRIT_BODY_BASE: &str = "c/sp/img1/025.mgr";
/// Her two light-tails in the body sheet (drawn by the cif at 59% opacity).
/// An outfit may clear them — a snowman suit has none — so these two may be
/// empty images where every other drawn frame must show something.
pub const SPIRIT_TAILS: [usize; 2] = [29, 30];

/// One costume, as `costumes/build.py` writes it.
pub struct Costume {
    /// Stable: it is what `hl5_avatar.txt` records.
    pub id: u32,
    /// EUC-KR, NUL-terminated.
    pub name: &'static [u8],
    /// The `_szSprFile` layer it dresses.
    pub layer: u8,
    /// The number it is served at: `/c/sp/img<layer>/<sheet>.mgr`.
    pub sheet: u16,
    /// The archive sheet the overlays are drawn on.
    pub base: &'static str,
    /// Allowed hero classes (bit 0 warrior, 1 rogue, 2 gunner, 3 knight).
    pub classes: u8,
    /// Deliberately transparent weapon skin (combat behaviour is unchanged).
    pub hidden_weapon: bool,
    /// Carries the rig's pose images ([`RIG_HEAD_COUNT`] / [`RIG_BODY_COUNT`]
    /// images) and animates through [`rig_cif`].
    pub rig: bool,
    /// For a weapon: the effect sheet (`img5/<fx>.mgr`) worn with it, its
    /// avatar's colours on the class's own slashes and flames. 0 = none.
    pub fx: u16,
    /// For an effect sheet: the base recoloured, RGB565 -> RGB565, on every
    /// image but those in `keep` (labels, arrows, dust — colour that is
    /// information). Empty for a drawn costume.
    pub recolour: &'static [(u16, u16)],
    pub keep: &'static [u8],
    /// Grid character -> RGB565.
    pub colours: &'static [(u8, u16)],
    pub frames: &'static [Overlay],
}

/// What is drawn over one image of the base sheet. See `costumes/build.py`
/// for the grid rules.
pub struct Overlay {
    pub index: usize,
    /// The base image it is drawn on: `index` itself, or — for an image
    /// appended past the base sheet's end (a rig pose) — the view it poses.
    pub src: usize,
    pub at: (i32, i32),
    pub clear_above: i32,
    pub grid: &'static [&'static str],
}

/// One image, as RGB565 colours per pixel (`TRANSPARENT` for clear).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u16>,
}

/// Decode a `.mgr` into its images. `None` for anything that is not one.
pub fn decode(data: &[u8]) -> Option<Vec<Image>> {
    let rd32 = |p: usize| data.get(p..p + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let count = rd32(0)? as usize;
    if count > 4096 {
        return None;
    }
    let mut pos = 4;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let size = rd32(pos)? as usize;
        pos += 4;
        let g = data.get(pos..pos + size)?;
        pos += size;
        out.push(decode_gbm(g)?);
    }
    Some(out)
}

fn decode_gbm(g: &[u8]) -> Option<Image> {
    let bpp = (*g.first()? & 0x0f) as usize;
    let n = *g.get(1)? as usize;
    let w = u16::from_le_bytes([*g.get(2)?, *g.get(3)?]) as usize;
    let h = u16::from_le_bytes([*g.get(4)?, *g.get(5)?]) as usize;
    let pal: Vec<u16> =
        (0..n).map(|i| g.get(6 + 2 * i..8 + 2 * i).map(|b| u16::from_le_bytes([b[0], b[1]]))).collect::<Option<_>>()?;
    let pix = g.get(6 + 2 * n..)?;
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let idx = match bpp {
                8 => *pix.get(y * w + x)? as usize,
                4 => {
                    let b = *pix.get(y * w.div_ceil(2) + x / 2)?;
                    (if x % 2 == 0 { b >> 4 } else { b & 0x0f }) as usize
                }
                _ => return None,
            };
            px.push(*pal.get(idx)?);
        }
    }
    Some(Image { w, h, px })
}

/// Encode images back into a `.mgr`: 4 bpp when 16 colours do, 8 bpp
/// otherwise, the clear colour first — the shape the shipped tool writes.
pub fn encode(images: &[Image]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    out.extend_from_slice(&(images.len() as u32).to_le_bytes());
    for im in images {
        let g = encode_gbm(im)?;
        out.extend_from_slice(&(g.len() as u32).to_le_bytes());
        out.extend_from_slice(&g);
    }
    Some(out)
}

fn encode_gbm(im: &Image) -> Option<Vec<u8>> {
    let mut pal: Vec<u16> = Vec::new();
    if im.px.contains(&TRANSPARENT) {
        pal.push(TRANSPARENT);
    }
    for c in &im.px {
        if !pal.contains(c) {
            pal.push(*c);
        }
    }
    if pal.len() > 255 || im.w > 0xffff || im.h > 0xffff {
        return None;
    }
    let four = pal.len() <= 16;
    let clear = u8::from(pal.first() == Some(&TRANSPARENT));
    let mut g = vec![(clear << 4) | if four { 4 } else { 8 }, pal.len() as u8];
    g.extend_from_slice(&(im.w as u16).to_le_bytes());
    g.extend_from_slice(&(im.h as u16).to_le_bytes());
    for c in &pal {
        g.extend_from_slice(&c.to_le_bytes());
    }
    let index = |c: u16| pal.iter().position(|p| *p == c).unwrap_or(0) as u8;
    for y in 0..im.h {
        let row = &im.px[y * im.w..(y + 1) * im.w];
        if four {
            for pair in row.chunks(2) {
                let hi = index(pair[0]);
                let lo = pair.get(1).map_or(0, |c| index(*c));
                g.push((hi << 4) | lo);
            }
        } else {
            g.extend(row.iter().map(|c| index(*c)));
        }
    }
    Some(g)
}

/// Draw one overlay on one image, growing the canvas symmetrically.
pub fn compose(base: &Image, o: &Overlay, colours: &[(u8, u16)]) -> Option<Image> {
    let gw = o.grid.iter().map(|r| r.len()).max().unwrap_or(0) as i32;
    let gh = o.grid.len() as i32;
    let (bw, bh) = (base.w as i32, base.h as i32);
    let (ox, oy) = o.at;
    let padx = (-ox.min(0)).max((ox + gw - bw).max(0));
    let pady = (-oy.min(0)).max((oy + gh - bh).max(0));
    let (w, h) = ((bw + 2 * padx) as usize, (bh + 2 * pady) as usize);
    let mut px = vec![TRANSPARENT; w * h];
    for y in 0..bh {
        if y < o.clear_above {
            continue;
        }
        for x in 0..bw {
            let c = base.px[(y * bw + x) as usize];
            px[((y + pady) as usize) * w + (x + padx) as usize] = c;
        }
    }
    for (gy, row) in o.grid.iter().enumerate() {
        for (gx, ch) in row.bytes().enumerate() {
            let at = ((oy + gy as i32 + pady) as usize) * w + (ox + gx as i32 + padx) as usize;
            match ch {
                b'.' => {}
                b'-' => px[at] = TRANSPARENT,
                _ => px[at] = colours.iter().find(|(k, _)| *k == ch)?.1,
            }
        }
    }
    Some(Image { w, h, px })
}

/// The finished sheet for one costume, from the archive's base sheet. An
/// overlay past the base's end appends an image; every appended index must
/// be drawn (a gap would be an image the cif names and the sheet lacks).
pub fn build(vfs: &[u8], c: &Costume) -> Option<Vec<u8>> {
    let base = decode(crate::shop::vfs_entry(vfs, c.base)?)?;
    if !c.recolour.is_empty() {
        let mut images = base;
        for (i, im) in images.iter_mut().enumerate() {
            if c.keep.contains(&(i as u8)) {
                continue;
            }
            for p in im.px.iter_mut() {
                if let Ok(k) = c.recolour.binary_search_by_key(p, |(from, _)| *from) {
                    *p = c.recolour[k].1;
                }
            }
        }
        return encode(&images);
    }
    let n = c.frames.iter().map(|o| o.index + 1).max().unwrap_or(0).max(base.len());
    let mut images: Vec<Option<Image>> = base.iter().cloned().map(Some).collect();
    images.resize(n, None);
    for o in c.frames {
        images[o.index] = Some(compose(base.get(o.src)?, o, c.colours)?);
    }
    encode(&images.into_iter().collect::<Option<Vec<_>>>()?)
}

// ---- the rig's cif variant --------------------------------------------------

/// The cif `<CLASS>::LoadSprite` loads for class `class` (0 warrior .. 3
/// knight), as the guest spells it.
pub fn hero_cif(class: u8) -> String {
    format!("/c/sp/cif/{:03}.cif", class as u32 + 1)
}

/// The class a hero cif path is for, if it is one.
pub fn hero_cif_class(path: &[u8]) -> Option<u8> {
    (0..4u8).find(|c| hero_cif(*c).as_bytes() == path)
}

/// Where the variant for `class` with a rigged head and/or body is served:
/// `/c/sp/cif/9<class+1><1 head | 2 body>.cif` — the archive's cifs are
/// 000..124 and named ones, so nothing it ships is shadowed. `None` when
/// neither layer is rigged (the archive's own file is the answer).
pub fn rig_cif_path(class: u8, head: bool, body: bool) -> Option<String> {
    let v = u32::from(head) | u32::from(body) << 1;
    (v != 0 && class < 4).then(|| format!("/c/sp/cif/{}.cif", 900 + (class as u32 + 1) * 10 + v))
}

/// Byte offsets of every part of every frame, keyed (anim, dir, frame),
/// walking a hero cif (12-bit coordinates) exactly as
/// `SPRITE_DATA::LoadSprData` (0xDBC64) does: a part is 5 bytes, 10 when
/// `b0 & 0x80` carries the effect tail; an empty slot (`b0 == 0x7F`) is 5
/// bytes and still counts. `None` unless the walk ends exactly at the end.
fn part_offsets(raw: &[u8]) -> Option<std::collections::HashMap<(u8, u8, u8), Vec<usize>>> {
    let at = |i: usize| raw.get(i).copied();
    let s8 = |v: u8| v as i8 as i32;
    let mut r = 1 + s8(at(0)?).max(0) as usize;
    let anims = at(r)?;
    r += 1;
    let mut out = std::collections::HashMap::new();
    for a in 0..anims {
        let dirs = s8(at(r)?).max(0);
        r += 1;
        for d in 0..dirs as u8 {
            let frames = at(r)?;
            r += 1;
            for f in 0..frames {
                let parts = s8(at(r)?).max(0);
                r += 1;
                let mut offs = Vec::with_capacity(parts as usize);
                for _ in 0..parts {
                    offs.push(r);
                    r += if at(r)? & 0x80 != 0 { 10 } else { 5 };
                }
                out.insert((a, d, f), offs);
            }
        }
    }
    (r == raw.len()).then_some(out)
}

/// The archive's hero cif with the rig's remaps applied to the head (layer
/// 0) and/or body (layer 1) parts: in each listed frame, the first part of
/// that layer naming `from` names `to` instead. Nothing else changes.
pub fn rig_cif(raw: &[u8], head: bool, body: bool) -> Option<Vec<u8>> {
    let offs = part_offsets(raw)?;
    let mut out = raw.to_vec();
    for &(a, d, f, layer, from, to) in RIG_REMAPS {
        if (layer == 0 && !head) || (layer == 1 && !body) {
            continue;
        }
        let Some(parts) = offs.get(&(a, d, f)) else { continue };
        if let Some(&o) = parts
            .iter()
            .find(|&&o| out[o] != 0x7f && out[o] & 0x1f == layer && out[o + 1] == from)
        {
            out[o + 1] = to;
        }
    }
    Some(out)
}

/// Write the rig's cif variants for the four classes. `true` when all twelve
/// are served — only then may a rigged costume animate through them.
fn install_rig_cifs(vfs: &[u8], storage: &Path, logs: &mut Vec<String>) -> bool {
    let root = storage.join(crate::INTERNAL_STORAGE_ROOT.trim_start_matches('/'));
    let mut served = 0;
    for class in 0..4u8 {
        let Some(raw) = crate::shop::vfs_entry(vfs, hero_cif(class).trim_start_matches('/')) else {
            logs.push(format!("costume: {} is not in the archive; no rig", hero_cif(class)));
            return false;
        };
        for (head, body) in [(true, false), (false, true), (true, true)] {
            let path = rig_cif_path(class, head, body).expect("a rigged layer");
            let Some(cif) = rig_cif(raw, head, body) else {
                logs.push(format!("costume: {} did not walk; no rig", hero_cif(class)));
                return false;
            };
            let dst = root.join(path.trim_start_matches('/'));
            if std::fs::read(&dst).ok().as_deref() != Some(&cif[..]) {
                if let Some(parent) = dst.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::write(&dst, &cif) {
                    logs.push(format!("costume: cannot write {}: {e}; no rig", dst.display()));
                    return false;
                }
            }
            served += 1;
        }
    }
    logs.push(format!("costume: {served} rig cif variant(s) served ({} remaps)", RIG_REMAPS.len()));
    true
}

/// The path, under the guest's storage root, a costume is served from.
pub fn shadow_path(storage: &Path, c: &Costume) -> std::path::PathBuf {
    storage
        .join(crate::INTERNAL_STORAGE_ROOT.trim_start_matches('/'))
        .join(format!("c/sp/img{}/{:03}.mgr", c.layer, c.sheet))
}

/// Build every costume and write the ones whose shadow is missing or stale.
/// Returns the costumes that are servable — only those may join the wardrobe,
/// because a look whose sheet is not there is a layer the guest skips (§45:
/// a missing helmet sheet took the hero's head with it) — and whether the
/// rig's cif variants are served (without them a rigged costume still dresses,
/// through its first images, and simply does not animate).
pub fn install(vfs: &[u8], storage: &Path) -> (Vec<&'static Costume>, bool, Vec<String>) {
    let mut ok = Vec::new();
    let mut logs = Vec::new();
    let rig = COSTUMES.iter().any(|c| c.rig) && install_rig_cifs(vfs, storage, &mut logs);
    for c in COSTUMES {
        let Some(sheet) = build(vfs, c) else {
            logs.push(format!("costume: {} — base {} did not compose; left out", c.id, c.base));
            continue;
        };
        let dst = shadow_path(storage, c);
        if std::fs::read(&dst).ok().as_deref() != Some(&sheet[..]) {
            if let Some(parent) = dst.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&dst, &sheet) {
                logs.push(format!("costume: cannot write {}: {e}; left out", dst.display()));
                continue;
            }
        }
        ok.push(c);
    }
    logs.push(format!("costume: {} of {} costume sheet(s) served", ok.len(), COSTUMES.len()));
    (ok, rig, logs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> Option<Vec<u8>> {
        std::fs::read("../payload/data.vfs.mp3").ok()
    }

    /// Decoding and re-encoding the base sheet changes no pixel — the
    /// composite starts from exactly what the game draws.
    #[test]
    fn the_base_sheet_round_trips_pixel_for_pixel() {
        let Some(vfs) = shipped() else { return };
        for c in COSTUMES {
            let raw = crate::shop::vfs_entry(&vfs, c.base).expect("base sheet");
            let a = decode(raw).expect("decodes");
            let b = decode(&encode(&a).expect("encodes")).expect("re-decodes");
            assert_eq!(a, b, "{}", c.base);
        }
    }

    /// Every costume keeps every image index its base has, grows each frame
    /// it touches symmetrically (the .cif places images by their centre), and
    /// fits the format.
    #[test]
    fn every_costume_keeps_the_centre_and_the_index_count() {
        let Some(vfs) = shipped() else { return };
        for c in COSTUMES {
            let base = decode(crate::shop::vfs_entry(&vfs, c.base).unwrap()).unwrap();
            let sheet = decode(&build(&vfs, c).expect("builds")).expect("decodes");
            let want = match (c.rig, c.layer) {
                (true, 0) => RIG_HEAD_COUNT,
                (true, 1) => RIG_BODY_COUNT,
                (true, l) => panic!("{}: layer {l} has no rig", c.id),
                (false, _) => base.len(),
            };
            assert_eq!(sheet.len(), want, "{}: image count", c.id);
            for o in c.frames {
                assert!(o.src < base.len(), "{} frame {}: src {}", c.id, o.index, o.src);
                assert!(o.src == o.index || o.index >= base.len(), "{} frame {}: src", c.id, o.index);
                let (b, s) = (&base[o.src], &sheet[o.index]);
                assert_eq!((s.w - b.w) % 2, 0, "{} frame {}: width grows unevenly", c.id, o.index);
                assert_eq!((s.h - b.h) % 2, 0, "{} frame {}: height grows unevenly", c.id, o.index);
                if c.hidden_weapon {
                    assert_eq!(c.layer, 4);
                    assert!(s.px.iter().all(|p| *p == TRANSPARENT));
                } else if c.classes == SPIRIT && c.layer == 1 && SPIRIT_TAILS.contains(&o.index) {
                    // A tail cleared on purpose (see SPIRIT_TAILS).
                } else if b.w * b.h > 1 {
                    // A 1x1 base is a clear slot (the hair-back piece of a
                    // costume without one); everything else draws something.
                    assert!(s.px.iter().any(|p| *p != TRANSPARENT), "{} frame {}", c.id, o.index);
                }
            }
            assert!(c.sheet >= 256, "{}: below 256 a monster table could name it", c.id);
            assert_eq!(c.name.last(), Some(&0));
        }
    }

    #[test]
    fn ids_and_sheets_are_unique() {
        let mut ids = std::collections::HashSet::new();
        let mut sheets = std::collections::HashSet::new();
        for c in COSTUMES {
            assert!(ids.insert(c.id), "id {}", c.id);
            assert!(sheets.insert((c.layer, c.sheet)), "sheet {}/{}", c.layer, c.sheet);
        }
    }

    /// 렌's costumes are drawn on her own two sheets and nothing else, keep
    /// their image count (her wardrobe swaps them in place with
    /// `ChangeSprImg`, which reloads into the array the old sheet sized), and
    /// come as pairs — a head and a body under one number — because an
    /// outfit is worn whole.
    #[test]
    fn spirit_costumes_are_head_and_body_pairs_on_her_sheets() {
        let mut heads = std::collections::BTreeSet::new();
        let mut bodies = std::collections::BTreeSet::new();
        for c in COSTUMES.iter().filter(|c| c.classes == SPIRIT) {
            assert!(!c.rig && c.fx == 0 && c.recolour.is_empty(), "{}", c.id);
            assert!(c.frames.iter().all(|o| o.src == o.index), "{}: no appended images", c.id);
            match c.layer {
                0 => {
                    assert_eq!(c.base, SPIRIT_HEAD_BASE, "{}", c.id);
                    heads.insert(c.sheet);
                }
                1 => {
                    assert_eq!(c.base, SPIRIT_BODY_BASE, "{}", c.id);
                    bodies.insert(c.sheet);
                }
                l => panic!("{}: 렌 has no layer {l}", c.id),
            }
        }
        assert_eq!(heads, bodies, "every outfit needs both halves");
    }

    #[test]
    fn weapon_costumes_have_explicit_compatible_classes() {
        for c in COSTUMES {
            if c.classes == SPIRIT {
                continue;
            }
            assert!(c.classes > 0 && c.classes & !0b1111 == 0);
            if c.layer == 4 {
                if c.hidden_weapon {
                    assert_eq!(c.base, "c/sp/img4/016.mgr");
                    assert_eq!(c.classes, 0b1111);
                    continue;
                }
                // Each class draws its own weapon with its own pieces: a
                // sword sheet in a spear animation lands its pieces where
                // the spear's go (v2 gave the knight the sword).
                let (want, fx_class) = match c.base {
                    "c/sp/img4/000.mgr" => (0b0001, 0),
                    "c/sp/img4/016.mgr" => (0b0010, 1),
                    "c/sp/img4/044.mgr" => (0b0100, 2),
                    "c/sp/img4/050.mgr" => (0b1000, 3),
                    other => panic!("Add a compatibility check for {other}"),
                };
                assert_eq!(c.classes, want, "{}", c.id);
                if c.fx != 0 {
                    let fx = COSTUMES.iter().find(|f| f.layer == 5 && f.sheet == c.fx).expect("its effect sheet");
                    assert_eq!(fx.base, format!("c/sp/img5/{fx_class:03}.mgr"), "{}", c.id);
                    assert_eq!(fx.classes, want);
                }
            }
            if c.layer == 6 {
                assert_eq!(c.classes, 0b1000);
            }
            if c.layer == 5 {
                assert!(!c.recolour.is_empty() && c.frames.is_empty(), "{}", c.id);
                assert!(c.recolour.windows(2).all(|w| w[0].0 < w[1].0), "{}: sorted for the search", c.id);
            }
        }
    }

    /// An effect sheet changes only colours, only off the kept images, and
    /// never into the clear colour.
    #[test]
    fn effect_sheets_recolour_and_keep_the_labels() {
        let Some(vfs) = shipped() else { return };
        for c in COSTUMES.iter().filter(|c| c.layer == 5) {
            let base = decode(crate::shop::vfs_entry(&vfs, c.base).unwrap()).unwrap();
            let sheet = decode(&build(&vfs, c).unwrap()).unwrap();
            assert_eq!(base.len(), sheet.len());
            let mut changed = 0;
            for (i, (a, b)) in base.iter().zip(&sheet).enumerate() {
                assert_eq!((a.w, a.h), (b.w, b.h));
                if c.keep.contains(&(i as u8)) {
                    assert_eq!(a, b, "{} image {i} is kept", c.id);
                }
                for (p, q) in a.px.iter().zip(&b.px) {
                    assert_eq!(*p == TRANSPARENT, *q == TRANSPARENT);
                    changed += usize::from(p != q);
                }
            }
            assert!(changed > 0, "{}", c.id);
        }
    }

    /// The variant walk covers each hero cif exactly, the variant has the
    /// archive's length and differs only in image-index bytes of head and
    /// body parts, every remap lands (head and torso of every listed frame),
    /// and every index it names exists in a rigged sheet.
    #[test]
    fn the_rig_cif_changes_only_head_and_body_indices() {
        let Some(vfs) = shipped() else { return };
        for class in 0..4u8 {
            let raw = crate::shop::vfs_entry(&vfs, hero_cif(class).trim_start_matches('/')).expect("hero cif");
            let offs = part_offsets(raw).expect("walks to the end");
            let both = rig_cif(raw, true, true).unwrap();
            assert_eq!(both.len(), raw.len());
            let index_bytes: std::collections::HashSet<usize> =
                offs.values().flatten().filter(|&&o| raw[o] != 0x7f).map(|o| o + 1).collect();
            let mut changed = 0;
            for (i, (a, b)) in raw.iter().zip(&both).enumerate() {
                if a != b {
                    assert!(index_bytes.contains(&i), "class {class}: byte {i} is not an image index");
                    let layer = raw[i - 1] & 0x1f;
                    assert!(layer <= 1, "class {class}: layer {layer} changed");
                    let limit = if layer == 0 { RIG_HEAD_COUNT } else { RIG_BODY_COUNT };
                    assert!((*b as usize) < limit, "class {class}: index {b} past the sheet");
                    changed += 1;
                }
            }
            // Head and torso of every remapped frame (the hair-back slot only
            // where the class draws it).
            let main = RIG_REMAPS.iter().filter(|r| !(r.3 == 0 && r.4 == 2)).count();
            assert!(changed >= main, "class {class}: {changed} of {main} remaps landed");
            let head = rig_cif(raw, true, false).unwrap();
            let body = rig_cif(raw, false, true).unwrap();
            for i in 0..raw.len() {
                if head[i] != raw[i] {
                    assert_eq!(raw[i - 1] & 0x1f, 0);
                }
                if body[i] != raw[i] {
                    assert_eq!(raw[i - 1] & 0x1f, 1);
                }
            }
        }
    }

    #[test]
    fn rig_cif_paths_are_new_names() {
        let Some(vfs) = shipped() else { return };
        for class in 0..4u8 {
            assert_eq!(hero_cif_class(hero_cif(class).as_bytes()), Some(class));
            assert_eq!(rig_cif_path(class, false, false), None);
            for (h, b) in [(true, false), (false, true), (true, true)] {
                let p = rig_cif_path(class, h, b).unwrap();
                assert!(crate::shop::vfs_entry(&vfs, p.trim_start_matches('/')).is_none(), "{p} shadows the archive");
            }
        }
        assert_eq!(rig_cif_path(0, true, true).as_deref(), Some("/c/sp/cif/913.cif"));
        assert_eq!(hero_cif_class(b"/c/sp/cif/005.cif"), None);
    }

    #[test]
    fn four_bit_rows_pack_high_nibble_first() {
        let im = Image { w: 3, h: 1, px: vec![TRANSPARENT, 0x1234, 0x1234] };
        let g = encode_gbm(&im).unwrap();
        assert_eq!(g[0], 0x14, "4 bpp with a clear colour");
        assert_eq!(&g[6..10], &[0x1f, 0xf8, 0x34, 0x12]);
        assert_eq!(&g[10..], &[0x01, 0x10]);
        assert_eq!(decode_gbm(&g).unwrap(), im);
    }
}
