//! 세이브 내보내기 / 불러오기 / 되돌리기 — the file that carries a save from
//! one phone to another, and everything that touches the disk on its behalf
//! (ANALYSIS.md §70). The menu side is [`crate::savexfer`].
//!
//! # What a save is, on disk
//!
//! The guest keeps every character in **one** MIDAS container, `kickass`
//! (`ff 56 30 00`, then 20-byte `[name 16][size 4]` entries — `M.sav`,
//! `Heroslore5`, `ET/EX/OP.sav`, and `SL_n/H_n/B_n.sav` per character), plus a
//! 63-byte `LOCAL.sav`, both under [`crate::INTERNAL_STORAGE_ROOT`] inside the
//! host storage. The host keeps four more files of the player's things next to
//! its switches: the 공용창고 (two files and the staged half of a two-file
//! commit — [`crate::warehouse`]), and the avatars ([`crate::avatar`]). The
//! 패드키 mapping travels too, because it names pad keycodes and buttons, not
//! anything about the screen; 배치변경 does not, because it is positions on
//! *this* display.
//!
//! The container is DES-encrypted with a key that is a constant in the binary
//! (`onStartApp` hands `MX_desInit` a `.rodata` string), so it is not bound to
//! the phone: the bytes open anywhere this build runs. Nothing here decrypts
//! anything — a bundle carries files byte for byte, which is also why
//! restoring one needs no guest and no key.
//!
//! # The bundle
//!
//! An uncompressed zip, so a support request can be opened with any tool:
//! `manifest.txt` first (`hl5 save bundle 1`, the export time, which groups are
//! authoritative, how many characters), then the members of [`MEMBERS`] that
//! existed. Reading is strict on purpose — only what [`zip`] writes is
//! accepted: stored entries, no encryption, no data descriptors, every CRC
//! checked, every name on the list, none twice.
//!
//! # Applying one: at the next start, never live
//!
//! The guest mirrors the open save in its heap and writes the whole container
//! back on every save, so bytes swapped under a running game are overwritten
//! seconds later. So the menu only *stages* a bundle ([`stage`]) and closes the
//! game, and [`apply_pending`] swaps the files at the next start — called by
//! the shell before it reads any of them, and before the guest exists:
//!
//! 1. the staged bundle is parsed again (it is integrity, not policy, here);
//! 2. the files it is about to replace are copied into the vault — **the
//!    pre-image**; if that copy cannot be made, nothing is applied;
//! 3. the stage is renamed to `.applying` — the commit point;
//! 4. every member is written (temp + fsync + rename) or, when the bundle does
//!    not have it, removed;
//! 5. `.applying` is removed and the vault rotated.
//!
//! A start that finds `.applying` was interrupted in step 4 and finishes it —
//! once: the file is renamed `.rollforward` first, and a start that finds
//! *that* gives up rather than crash on every launch. The vault still holds
//! the save from before.
//!
//! # The vault
//!
//! `hl5_save_vault/<14-digit unix ms>.h5save`, the same format, one per apply,
//! newest [`VAULT_KEEP`] kept. 세이브 되돌리기 stages the newest — the save as
//! it was just before the last import or restore — so pressing it twice
//! swaps back. Applies are rare (a player action each), so a flat window is
//! enough here; the game's own saves never touch the vault.

use std::path::{Path, PathBuf};

/// The suffix the shell suggests and the vault uses.
pub const EXT: &str = "h5save";
/// A bundle's first manifest line. The number is the format, not the build.
const MAGIC_LINE: &str = "hl5 save bundle 1";
const MANIFEST: &str = "manifest.txt";

/// A bundle the menu accepted, waiting for the next start.
pub const PENDING: &str = "hl5_save_import.h5save";
/// [`PENDING`] once the apply has committed — see the module docs.
const APPLYING: &str = "hl5_save_import.applying";
/// [`APPLYING`] while a start is finishing it.
const ROLLFORWARD: &str = "hl5_save_import.rollforward";
/// The export the shell copies to the player's chosen document.
pub const EXPORT_OUT: &str = "hl5_export.h5save";
/// Where the shell copies the document the player picked.
pub const IMPORT_IN: &str = "hl5_import_in.h5save";

pub const VAULT_DIR: &str = "hl5_save_vault";
/// Generations the vault keeps. Five applies back is far past "undo the one I
/// just did", and each is tens of kilobytes.
pub const VAULT_KEEP: usize = 5;

/// One member's ceiling. A three-character container is ~50 KB; ten-page
/// warehouses are a few tens. A megabyte is not a save.
const MEMBER_MAX: usize = 1 << 20;
/// A whole bundle's — and what the shell is asked to copy at most.
pub const BUNDLE_MAX: usize = 4 << 20;

/// The first four bytes of every `kickass` the guest has written (249 of 249
/// in the probe's storages, every size from options-only to three characters).
const CONTAINER_MAGIC: [u8; 4] = [0xff, 0x56, 0x30, 0x00];
/// `[name 16][size 4]`.
const CONTAINER_ENTRY: usize = 20;

/// Which members a bundle speaks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    /// The characters and their things: always authoritative.
    Save,
    /// The 패드키 mapping: authoritative only when the bundle says so, so a
    /// phone that never mapped a pad does not wipe the mapping of the one it
    /// is imported into.
    Pad,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dir {
    /// The guest's own directory, [`crate::INTERNAL_STORAGE_ROOT`] inside the
    /// host storage.
    Guest,
    /// The host storage itself, beside the switch files.
    Storage,
}

#[derive(Debug)]
pub struct Member {
    pub name: &'static str,
    dir: Dir,
    pub group: Group,
}

/// Everything a bundle can carry. The name is both the file's name and the zip
/// entry's.
pub const MEMBERS: [Member; 8] = [
    Member { name: "kickass", dir: Dir::Guest, group: Group::Save },
    Member { name: "LOCAL.sav", dir: Dir::Guest, group: Group::Save },
    Member { name: "hl5_warehouse.txt", dir: Dir::Storage, group: Group::Save },
    Member { name: "hl5_warehouse_upper.txt", dir: Dir::Storage, group: Group::Save },
    // The staged half of an interrupted two-file commit: carried as it is, so
    // the receiving warehouse loader finishes or forgets it exactly as the
    // sending one would have.
    Member { name: "hl5_warehouse_upper.new", dir: Dir::Storage, group: Group::Save },
    Member { name: "hl5_avatar.txt", dir: Dir::Storage, group: Group::Save },
    Member { name: "hl5_gamepad.txt", dir: Dir::Storage, group: Group::Pad },
    Member { name: "hl5_padmode.txt", dir: Dir::Storage, group: Group::Pad },
];
const KICKASS: usize = 0;

/// Where a member lives under `storage`.
pub fn member_path(storage: &Path, m: &Member) -> PathBuf {
    match m.dir {
        Dir::Guest => storage.join(crate::INTERNAL_STORAGE_ROOT.trim_start_matches('/')).join(m.name),
        Dir::Storage => storage.join(m.name),
    }
}

/// A bundle, read.
#[derive(Debug)]
pub struct Bundle {
    /// `YYYY-MM-DD HH:MM` on the phone that wrote it, filtered to those
    /// characters — it goes into a guest popup.
    pub exported: String,
    /// Whether the 패드키 members are authoritative.
    pub pad: bool,
    /// `(index into MEMBERS, bytes)`.
    pub files: Vec<(usize, Vec<u8>)>,
    /// `SL_n.sav` entries in its `kickass`.
    pub characters: usize,
}

/// Why a file is not something to apply.
#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    /// Not a bundle, or a damaged one; the reason is for the log.
    NotBundle(String),
    /// A sound bundle whose container holds no character. Policy, not
    /// integrity — the menu refuses it, a restore of the vault does not.
    NoCharacter,
}

// ---- the container ----------------------------------------------------------

/// How many characters a `kickass` holds, or `None` if it is not one.
///
/// Only the shape the guest writes is checked — the magic, then entries whose
/// names are printable up to their NUL — and the entry table ends at the first
/// name that starts with NUL, as it does in every file the guest has written.
pub fn characters_in(kickass: &[u8]) -> Option<usize> {
    if kickass.len() < CONTAINER_MAGIC.len() + CONTAINER_ENTRY || kickass[..4] != CONTAINER_MAGIC {
        return None;
    }
    let (mut off, mut entries, mut chars) = (CONTAINER_MAGIC.len(), 0usize, 0usize);
    while off + CONTAINER_ENTRY <= kickass.len() && kickass[off] != 0 {
        let name = &kickass[off..off + 16];
        let len = name.iter().position(|&c| c == 0).unwrap_or(16);
        if !name[..len].iter().all(|c| c.is_ascii_graphic()) {
            return None;
        }
        if name.starts_with(b"SL_") {
            chars += 1;
        }
        entries += 1;
        // The guest's own table has eight entries per one character and
        // fourteen for three; a hundred is not a table.
        if entries > 100 {
            return None;
        }
        off += CONTAINER_ENTRY;
    }
    (entries > 0).then_some(chars)
}

// ---- the zip ----------------------------------------------------------------

/// CRC-32 (IEEE), the one zip uses.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// 1980-01-01, the earliest DOS date: the entries carry no time of their own
/// (the manifest does), and a fixed stamp keeps two exports of the same files
/// byte-identical outside the manifest.
const DOS_DATE: u16 = (1 << 5) | 1;

fn put16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// Write an uncompressed zip.
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let (crc, size, n) = (crc32(data), data.len() as u32, name.len() as u16);
        put32(&mut out, 0x0403_4b50);
        put16(&mut out, 20); // version needed
        put16(&mut out, 0); // flags
        put16(&mut out, 0); // stored
        put16(&mut out, 0); // time
        put16(&mut out, DOS_DATE);
        put32(&mut out, crc);
        put32(&mut out, size);
        put32(&mut out, size);
        put16(&mut out, n);
        put16(&mut out, 0); // extra
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        put32(&mut central, 0x0201_4b50);
        put16(&mut central, 20); // made by
        put16(&mut central, 20); // needed
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, DOS_DATE);
        put32(&mut central, crc);
        put32(&mut central, size);
        put32(&mut central, size);
        put16(&mut central, n);
        put16(&mut central, 0); // extra
        put16(&mut central, 0); // comment
        put16(&mut central, 0); // disk
        put16(&mut central, 0); // internal attributes
        put32(&mut central, 0); // external attributes
        put32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let (cd_off, cd_len) = (out.len() as u32, central.len() as u32);
    out.extend_from_slice(&central);
    put32(&mut out, 0x0605_4b50);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, entries.len() as u16);
    put16(&mut out, entries.len() as u16);
    put32(&mut out, cd_len);
    put32(&mut out, cd_off);
    put16(&mut out, 0); // comment
    out
}

fn get16(b: &[u8], i: usize) -> Option<usize> {
    b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize)
}

fn get32(b: &[u8], i: usize) -> Option<usize> {
    b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
}

/// Read a zip [`zip`] could have written. Anything else is an error.
pub fn unzip(b: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let bad = |why: &str| Err(why.to_string());
    if b.len() < 22 {
        return bad("shorter than an end record");
    }
    // The end record is the last 22 bytes plus its comment; we write none but
    // tolerate one, the way every reader does.
    let lo = b.len().saturating_sub(22 + 0xffff);
    let eocd = (lo..=b.len() - 22)
        .rev()
        .find(|&i| get32(b, i) == Some(0x0605_4b50) && get16(b, i + 20).map(|c| i + 22 + c) == Some(b.len()))
        .ok_or_else(|| "no end record".to_string())?;
    let count = get16(b, eocd + 10).unwrap_or(0);
    let cd_len = get32(b, eocd + 12).unwrap_or(usize::MAX);
    let cd_off = get32(b, eocd + 16).unwrap_or(usize::MAX);
    if cd_off.checked_add(cd_len).map_or(true, |end| end > eocd) {
        return bad("central directory out of bounds");
    }
    let mut out = Vec::with_capacity(count);
    let mut p = cd_off;
    for _ in 0..count {
        if get32(b, p) != Some(0x0201_4b50) {
            return bad("broken central directory");
        }
        let field = |o: usize| get16(b, p + o).unwrap_or(usize::MAX);
        let (flags, method) = (field(8), field(10));
        let crc = get32(b, p + 16).unwrap_or(0) as u32;
        let (csize, usize_) = (get32(b, p + 20).unwrap_or(usize::MAX), get32(b, p + 24).unwrap_or(usize::MAX));
        let (n, extra, comment) = (field(28), field(30), field(32));
        let local = get32(b, p + 42).unwrap_or(usize::MAX);
        // Bit 0 is encryption, bit 3 a trailing data descriptor; neither is
        // ours.
        if flags & 0x0009 != 0 || method != 0 || csize != usize_ {
            return bad("an entry that is not stored plainly");
        }
        let name = b.get(p + 46..p + 46 + n).ok_or_else(|| "name out of bounds".to_string())?;
        let name = std::str::from_utf8(name).map_err(|_| "a name that is not UTF-8".to_string())?.to_string();
        p += 46 + n + extra + comment;
        if p > eocd {
            return bad("central directory overruns");
        }
        if get32(b, local) != Some(0x0403_4b50) {
            return bad("broken local header");
        }
        let (ln, lx) = (get16(b, local + 26).unwrap_or(0), get16(b, local + 28).unwrap_or(0));
        if b.get(local + 30..local + 30 + ln) != Some(name.as_bytes()) {
            return bad("local and central names differ");
        }
        let start = local + 30 + ln + lx;
        let data = start
            .checked_add(usize_)
            .filter(|&end| end <= cd_off)
            .and_then(|end| b.get(start..end))
            .ok_or_else(|| "entry data out of bounds".to_string())?;
        if crc32(data) != crc {
            return Err(format!("{name}: CRC mismatch"));
        }
        out.push((name, data.to_vec()));
    }
    Ok(out)
}

// ---- bundles ----------------------------------------------------------------

/// Keep only what a `YYYY-MM-DD HH:MM` stamp is made of, so a hand-edited
/// manifest cannot put arbitrary bytes into a guest popup.
fn clean_stamp(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit() || matches!(c, '-' | ':' | ' ')).take(16).collect()
}

/// Assemble a bundle from member files.
fn build(files: &[(usize, Vec<u8>)], exported: &str, line: &str, pad: bool) -> Vec<u8> {
    let characters = files.iter().find(|(i, _)| *i == KICKASS).and_then(|(_, b)| characters_in(b)).unwrap_or(0);
    let manifest = format!(
        "{MAGIC_LINE}\nexported {}\ngroups save{}\ncharacters {characters}\nline {line}\n",
        clean_stamp(exported),
        if pad { " pad" } else { "" }
    );
    let mut entries: Vec<(&str, &[u8])> = vec![(MANIFEST, manifest.as_bytes())];
    entries.extend(files.iter().map(|(i, b)| (MEMBERS[*i].name, b.as_slice())));
    zip(&entries)
}

/// The members present under `storage`, in [`MEMBERS`] order.
fn present(storage: &Path) -> Vec<(usize, Vec<u8>)> {
    MEMBERS
        .iter()
        .enumerate()
        .filter_map(|(i, m)| std::fs::read(member_path(storage, m)).ok().map(|b| (i, b)))
        .collect()
}

/// What [`snapshot`] made.
pub struct Snapshot {
    pub bytes: Vec<u8>,
    pub characters: usize,
}

/// A bundle of what is on disk now, for 세이브 내보내기. `None` when there is
/// no save at all. `line` is `main` or `vanilla`, for whoever reads the
/// manifest; nothing checks it.
pub fn snapshot(storage: &Path, exported: &str, line: &str) -> Option<Snapshot> {
    let files = present(storage);
    if !files.iter().any(|(i, _)| *i == KICKASS) {
        return None;
    }
    let pad = files.iter().any(|(i, _)| MEMBERS[*i].group == Group::Pad);
    let characters = files.iter().find(|(i, _)| *i == KICKASS).and_then(|(_, b)| characters_in(b)).unwrap_or(0);
    Some(Snapshot { bytes: build(&files, exported, line, pad), characters })
}

/// Read a bundle. Integrity only — see [`Reject::NoCharacter`] for the one
/// policy, which callers apply.
pub fn parse(bytes: &[u8]) -> Result<Bundle, Reject> {
    let no = |why: String| Reject::NotBundle(why);
    if bytes.len() > BUNDLE_MAX {
        return Err(no(format!("{} bytes, over the {BUNDLE_MAX} B ceiling", bytes.len())));
    }
    let entries = unzip(bytes).map_err(no)?;
    let manifest = entries
        .iter()
        .find(|(n, _)| n == MANIFEST)
        .ok_or_else(|| no("no manifest".to_string()))?;
    let manifest = std::str::from_utf8(&manifest.1).map_err(|_| no("manifest is not text".to_string()))?;
    let mut lines = manifest.lines();
    if lines.next().map(str::trim) != Some(MAGIC_LINE) {
        return Err(no("not an hl5 save bundle (or a newer format)".to_string()));
    }
    let (mut exported, mut pad) = (String::new(), false);
    for l in lines {
        match l.split_once(' ') {
            Some(("exported", v)) => exported = clean_stamp(v.trim()),
            Some(("groups", v)) => pad = v.split_whitespace().any(|g| g == "pad"),
            _ => {}
        }
    }
    let mut files: Vec<(usize, Vec<u8>)> = Vec::new();
    for (name, data) in entries {
        if name == MANIFEST {
            continue;
        }
        let i = MEMBERS
            .iter()
            .position(|m| m.name == name)
            .ok_or_else(|| no(format!("unexpected entry {name:?}")))?;
        if files.iter().any(|(j, _)| *j == i) {
            return Err(no(format!("{name} twice")));
        }
        if data.len() > MEMBER_MAX {
            return Err(no(format!("{name} is {} bytes", data.len())));
        }
        files.push((i, data));
    }
    let characters = match files.iter().find(|(i, _)| *i == KICKASS) {
        Some((_, b)) => characters_in(b).ok_or_else(|| no("kickass is not a save container".to_string()))?,
        None => 0,
    };
    Ok(Bundle { exported, pad, files, characters })
}

/// The import policy: a bundle with no character would only replace every
/// character on this phone with nothing.
pub fn parse_import(bytes: &[u8]) -> Result<Bundle, Reject> {
    let b = parse(bytes)?;
    if b.characters == 0 {
        return Err(Reject::NoCharacter);
    }
    Ok(b)
}

// ---- files --------------------------------------------------------------------

/// Temp + fsync + rename, so an interrupted write leaves the previous file.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".xfer");
    let tmp = PathBuf::from(tmp);
    let err = |e: std::io::Error| format!("{}: {e}", tmp.display());
    let mut f = std::fs::File::create(&tmp).map_err(err)?;
    f.write_all(bytes).map_err(err)?;
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

/// Write the members a bundle speaks for: its own files, and the removal of
/// the ones it does not have.
fn apply(storage: &Path, b: &Bundle) -> Result<(usize, usize), String> {
    let (mut written, mut removed) = (0, 0);
    for (i, m) in MEMBERS.iter().enumerate() {
        if m.group == Group::Pad && !b.pad {
            continue;
        }
        let path = member_path(storage, m);
        match b.files.iter().find(|(j, _)| *j == i) {
            Some((_, data)) => {
                write_atomically(&path, data)?;
                written += 1;
            }
            None if path.exists() => {
                std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                removed += 1;
            }
            None => {}
        }
    }
    Ok((written, removed))
}

/// Accept a bundle for the next start.
pub fn stage(storage: &Path, bytes: &[u8]) -> Result<(), String> {
    write_atomically(&storage.join(PENDING), bytes)
}

// ---- the vault ------------------------------------------------------------------

/// A generation's time, if `name` is one. Everything else in the directory is
/// ignored, never deleted.
pub fn parse_gen(name: &str) -> Option<u64> {
    let stem = name.strip_suffix(".h5save")?;
    (stem.len() == 14 && stem.bytes().all(|c| c.is_ascii_digit())).then(|| stem.parse().ok()).flatten()
}

/// The vault's generations, oldest first.
pub fn vault_list(storage: &Path) -> Vec<(u64, PathBuf)> {
    let dir = storage.join(VAULT_DIR);
    let mut v: Vec<(u64, PathBuf)> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| parse_gen(&e.file_name().to_string_lossy()).map(|ms| (ms, e.path())))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn newest_vault(storage: &Path) -> Option<(u64, PathBuf)> {
    vault_list(storage).pop()
}

/// Copy what is on disk now into the vault. `Ok(None)` when there is nothing
/// to copy, or when it is already the newest generation.
fn vault_current(storage: &Path, now_ms: u64) -> Result<Option<PathBuf>, String> {
    let files = present(storage);
    if files.is_empty() {
        return Ok(None);
    }
    let newest = newest_vault(storage);
    if let Some((_, path)) = &newest {
        if let Ok(b) = std::fs::read(path).map_err(|_| ()).and_then(|b| parse(&b).map_err(|_| ())) {
            if b.files == files {
                return Ok(None);
            }
        }
    }
    // Monotonic even if the clock went back, so the newest by name is the
    // newest by fact.
    let ms = newest.map_or(now_ms, |(t, _)| now_ms.max(t + 1));
    let path = storage.join(VAULT_DIR).join(format!("{ms:014}.{EXT}"));
    write_atomically(&path, &build(&files, "", "vault", true))?;
    Ok(Some(path))
}

/// Remove all but the newest [`VAULT_KEEP`] generations.
fn vault_rotate(storage: &Path) -> Vec<String> {
    let list = vault_list(storage);
    let extra = list.len().saturating_sub(VAULT_KEEP);
    list.into_iter()
        .take(extra)
        .map(|(_, p)| match std::fs::remove_file(&p) {
            Ok(()) => format!("savexfer: vault rotated out {}", p.display()),
            Err(e) => format!("savexfer: cannot rotate out {}: {e}", p.display()),
        })
        .collect()
}

// ---- the start ------------------------------------------------------------------

/// Host wall-clock milliseconds, for the vault's names and the export stamp.
/// Not the guest's clock: that one is virtual in the probe and scaled by
/// `Config::speed`.
pub fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Apply a staged bundle, if there is one. The shell calls this before it
/// reads any file of the player's, and the probe at the same point; with
/// nothing staged it only sweeps two stale transfer files.
pub fn apply_pending(storage: &Path, now_ms: u64) -> Vec<String> {
    let mut log = Vec::new();
    for stale in [EXPORT_OUT, IMPORT_IN] {
        if std::fs::remove_file(storage.join(stale)).is_ok() {
            log.push(format!("savexfer: removed a stale {stale}"));
        }
    }
    let roll = storage.join(ROLLFORWARD);
    if roll.exists() {
        let _ = std::fs::remove_file(&roll);
        log.push(
            "savexfer: a roll-forward stopped part-way last start; not trying again — \
             the save from before is in the vault (세이브 되돌리기)"
                .to_string(),
        );
    }
    let applying = storage.join(APPLYING);
    if applying.exists() {
        match std::fs::rename(&applying, &roll) {
            Ok(()) => {
                let r = std::fs::read(&roll)
                    .map_err(|e| e.to_string())
                    .and_then(|b| parse(&b).map_err(|e| format!("{e:?}")))
                    .and_then(|b| apply(storage, &b));
                log.push(match r {
                    Ok((w, d)) => format!("savexfer: finished an interrupted import ({w} written, {d} removed)"),
                    Err(e) => format!("savexfer: could not finish an interrupted import: {e}"),
                });
                let _ = std::fs::remove_file(&roll);
            }
            Err(e) => {
                let _ = std::fs::remove_file(&applying);
                log.push(format!("savexfer: dropped an interrupted import ({e})"));
            }
        }
    }

    let pending = storage.join(PENDING);
    let bytes = match std::fs::read(&pending) {
        Ok(b) => b,
        Err(_) => return log,
    };
    let bundle = match parse(&bytes) {
        Ok(b) => b,
        Err(e) => {
            let _ = std::fs::remove_file(&pending);
            log.push(format!("savexfer: the staged save is unusable ({e:?}); nothing changed"));
            return log;
        }
    };
    match vault_current(storage, now_ms) {
        Ok(Some(p)) => log.push(format!("savexfer: the save it replaces is in {}", p.display())),
        Ok(None) => log.push("savexfer: nothing new to keep in the vault".to_string()),
        Err(e) => {
            // No destructive write without its pre-image.
            let _ = std::fs::remove_file(&pending);
            log.push(format!("savexfer: cannot keep the current save ({e}); the import was NOT applied"));
            return log;
        }
    }
    if let Err(e) = std::fs::rename(&pending, &applying) {
        let _ = std::fs::remove_file(&pending);
        log.push(format!("savexfer: cannot commit the import ({e}); nothing changed"));
        return log;
    }
    log.push(match apply(storage, &bundle) {
        Ok((w, d)) => format!(
            "savexfer: applied {} — {} character(s), {w} file(s) written, {d} removed{}",
            if bundle.exported.is_empty() {
                "a vault generation".to_string()
            } else {
                format!("the save exported {:?}", bundle.exported)
            },
            bundle.characters,
            if bundle.pad { ", pad mapping included" } else { "" }
        ),
        Err(e) => format!("savexfer: the import stopped part-way: {e} (the save from before is in the vault)"),
    });
    let _ = std::fs::remove_file(&applying);
    log.extend(vault_rotate(storage));
    log
}

// ---- time -------------------------------------------------------------------------

/// `YYYY-MM-DD HH:MM` for unix milliseconds at a UTC offset in minutes.
pub fn local_stamp(ms: u64, offset_min: i32) -> String {
    let secs = (ms / 1000) as i64 + offset_min as i64 * 60;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hl5_savebundle_{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A container shaped like the guest's: magic, a table of `chars`
    /// characters, the zeros that end it (the guest reserves 800 bytes of
    /// table), then a body.
    fn container(chars: usize, fill: u8) -> Vec<u8> {
        let mut v = CONTAINER_MAGIC.to_vec();
        let mut names = vec!["M.sav".to_string(), "Heroslore5".to_string(), "OP.sav".to_string()];
        for n in 0..chars {
            names.push(format!("SL_{n}.sav"));
            names.push(format!("H_{n}.sav"));
        }
        for n in names {
            let mut e = [0u8; 20];
            e[..n.len()].copy_from_slice(n.as_bytes());
            e[17] = 1;
            v.extend_from_slice(&e);
        }
        v.resize(804, 0);
        v.extend_from_slice(&[fill; 200]);
        v
    }

    fn write(storage: &Path, name: &str, bytes: &[u8]) {
        let m = MEMBERS.iter().find(|m| m.name == name).unwrap();
        let p = member_path(storage, m);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn read(storage: &Path, name: &str) -> Option<Vec<u8>> {
        let m = MEMBERS.iter().find(|m| m.name == name).unwrap();
        std::fs::read(member_path(storage, m)).ok()
    }

    #[test]
    fn crc32_matches_the_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn zip_round_trips_and_rejects_damage() {
        let z = zip(&[("a.txt", b"hello"), ("b", &[0u8, 1, 2, 255])]);
        let e = unzip(&z).unwrap();
        assert_eq!(e, vec![("a.txt".to_string(), b"hello".to_vec()), ("b".to_string(), vec![0, 1, 2, 255])]);
        // One flipped byte in an entry's data is a CRC mismatch.
        let mut bad = z.clone();
        bad[30 + 5] ^= 1;
        assert!(unzip(&bad).unwrap_err().contains("CRC"));
        // Truncation loses the end record.
        assert!(unzip(&z[..z.len() - 1]).is_err());
        assert!(unzip(b"").is_err());
        assert!(unzip(&[0u8; 100]).is_err());
    }

    #[test]
    fn the_container_check_counts_characters_and_refuses_strangers() {
        assert_eq!(characters_in(&container(1, 7)), Some(1));
        assert_eq!(characters_in(&container(3, 7)), Some(3));
        assert_eq!(characters_in(&container(0, 7)), Some(0), "options only");
        let mut wrong = container(1, 7);
        wrong[0] = 0;
        assert_eq!(characters_in(&wrong), None);
        let mut garbled = container(1, 7);
        garbled[4] = 0x01;
        assert_eq!(characters_in(&garbled), None);
        assert_eq!(characters_in(b"PK\x03\x04"), None);
    }

    #[test]
    fn a_snapshot_parses_back_to_the_same_files() {
        let s = dir("snapshot");
        assert!(snapshot(&s, "2026-10-25 14:30", "main").is_none(), "no kickass, no save");
        write(&s, "kickass", &container(2, 9));
        write(&s, "LOCAL.sav", b"local");
        write(&s, "hl5_warehouse.txt", b"# hl5 warehouse v1\n");
        let snap = snapshot(&s, "2026-10-25 14:30", "main").unwrap();
        assert_eq!(snap.characters, 2);
        let b = parse_import(&snap.bytes).unwrap();
        assert_eq!(b.exported, "2026-10-25 14:30");
        assert_eq!(b.characters, 2);
        assert!(!b.pad, "no pad file, so the pad group is not claimed");
        assert_eq!(b.files.len(), 3);
        write(&s, "hl5_gamepad.txt", b"96=OK\n");
        assert!(parse(&snapshot(&s, "", "main").unwrap().bytes).unwrap().pad);
    }

    #[test]
    fn parse_refuses_what_it_did_not_write() {
        let manifest = format!("{MAGIC_LINE}\n");
        let k = container(1, 1);
        // An entry not on the list.
        let z = zip(&[(MANIFEST, manifest.as_bytes()), ("../evil", b"x")]);
        assert!(matches!(parse(&z), Err(Reject::NotBundle(_))));
        // A member twice.
        let z = zip(&[(MANIFEST, manifest.as_bytes()), ("kickass", &k), ("kickass", &k)]);
        assert!(matches!(parse(&z), Err(Reject::NotBundle(_))));
        // No manifest, or a different format.
        assert!(matches!(parse(&zip(&[("kickass", &k)])), Err(Reject::NotBundle(_))));
        let z = zip(&[(MANIFEST, b"hl5 save bundle 2\n"), ("kickass", &k)]);
        assert!(matches!(parse(&z), Err(Reject::NotBundle(_))));
        // A kickass that is not a container.
        let z = zip(&[(MANIFEST, manifest.as_bytes()), ("kickass", b"not a save")]);
        assert!(matches!(parse(&z), Err(Reject::NotBundle(_))));
        // A sound bundle with no character: integrity passes, policy refuses.
        let z = zip(&[(MANIFEST, manifest.as_bytes()), ("kickass", &container(0, 1))]);
        assert!(parse(&z).is_ok());
        assert_eq!(parse_import(&z).unwrap_err(), Reject::NoCharacter);
        // A stamp cannot smuggle bytes into a popup.
        let m = format!("{MAGIC_LINE}\nexported 2026-10-25 14:30^\x01;evil\n");
        let z = zip(&[(MANIFEST, m.as_bytes()), ("kickass", &k)]);
        assert_eq!(parse(&z).unwrap().exported, "2026-10-25 14:30");
    }

    /// The whole import: the old save into the vault, the new files in, the
    /// ones the bundle does not have removed — and the 패드키 files left alone
    /// when the bundle does not speak for them.
    #[test]
    fn a_staged_import_is_applied_at_the_next_start_with_its_pre_image_kept() {
        let src = dir("import_src");
        write(&src, "kickass", &container(2, 2));
        write(&src, "LOCAL.sav", b"src-local");
        let bundle = snapshot(&src, "2026-10-25 14:30", "main").unwrap().bytes;

        let dst = dir("import_dst");
        write(&dst, "kickass", &container(1, 1));
        write(&dst, "LOCAL.sav", b"dst-local");
        write(&dst, "hl5_warehouse.txt", b"dst warehouse");
        write(&dst, "hl5_gamepad.txt", b"97=OK\n");
        stage(&dst, &bundle).unwrap();
        let log = apply_pending(&dst, 1_000);
        assert!(log.iter().any(|l| l.contains("applied")), "{log:?}");

        assert_eq!(read(&dst, "kickass"), Some(container(2, 2)));
        assert_eq!(read(&dst, "LOCAL.sav").as_deref(), Some(&b"src-local"[..]));
        assert_eq!(read(&dst, "hl5_warehouse.txt"), None, "the source had no warehouse");
        assert_eq!(read(&dst, "hl5_gamepad.txt").as_deref(), Some(&b"97=OK\n"[..]), "pad untouched");
        assert!(!dst.join(PENDING).exists() && !dst.join(APPLYING).exists());

        // The pre-image is the newest generation and restores exactly.
        let (ms, gen) = newest_vault(&dst).unwrap();
        assert_eq!(ms, 1_000);
        let before = std::fs::read(&gen).unwrap();
        let b = parse(&before).unwrap();
        assert!(b.pad, "a vault generation always speaks for the pad group");
        stage(&dst, &before).unwrap();
        apply_pending(&dst, 2_000);
        assert_eq!(read(&dst, "kickass"), Some(container(1, 1)));
        assert_eq!(read(&dst, "hl5_warehouse.txt").as_deref(), Some(&b"dst warehouse"[..]));
        // And the restore kept what it replaced: pressing it again swaps back.
        let (ms2, _) = newest_vault(&dst).unwrap();
        assert_eq!(ms2, 2_000);
    }

    #[test]
    fn an_unusable_stage_changes_nothing_and_is_not_retried() {
        let s = dir("bad_stage");
        write(&s, "kickass", &container(1, 5));
        std::fs::write(s.join(PENDING), b"garbage").unwrap();
        let log = apply_pending(&s, 1);
        assert!(log.iter().any(|l| l.contains("unusable")), "{log:?}");
        assert_eq!(read(&s, "kickass"), Some(container(1, 5)));
        assert!(!s.join(PENDING).exists(), "consumed, so it cannot loop");
        assert!(vault_list(&s).is_empty());
    }

    /// A start that died while writing finishes the job once; one that died
    /// finishing it gives up rather than try on every launch.
    #[test]
    fn an_interrupted_apply_rolls_forward_once() {
        let src = dir("roll_src");
        write(&src, "kickass", &container(3, 3));
        let bundle = snapshot(&src, "", "main").unwrap().bytes;
        let s = dir("roll_dst");
        write(&s, "kickass", &container(1, 1));
        std::fs::write(s.join(APPLYING), &bundle).unwrap();
        let log = apply_pending(&s, 1);
        assert!(log.iter().any(|l| l.contains("finished an interrupted")), "{log:?}");
        assert_eq!(read(&s, "kickass"), Some(container(3, 3)));
        assert!(!s.join(APPLYING).exists() && !s.join(ROLLFORWARD).exists());

        std::fs::write(s.join(ROLLFORWARD), &bundle).unwrap();
        let log = apply_pending(&s, 2);
        assert!(log.iter().any(|l| l.contains("not trying again")), "{log:?}");
        assert!(!s.join(ROLLFORWARD).exists());
    }

    #[test]
    fn the_vault_keeps_the_newest_five_and_ignores_strangers() {
        let s = dir("vault");
        for n in 0..8u8 {
            write(&s, "kickass", &container(1, n));
            vault_current(&s, 100 + n as u64).unwrap();
            vault_rotate(&s);
        }
        std::fs::write(s.join(VAULT_DIR).join("notes.txt"), b"mine").unwrap();
        let list = vault_list(&s);
        assert_eq!(list.iter().map(|(ms, _)| *ms).collect::<Vec<_>>(), vec![103, 104, 105, 106, 107]);
        assert!(s.join(VAULT_DIR).join("notes.txt").exists(), "never deleted, never listed");
        // The same files twice is one generation.
        assert_eq!(vault_current(&s, 999).unwrap(), None);
        // A clock that went backwards still sorts after the newest.
        write(&s, "kickass", &container(1, 99));
        let p = vault_current(&s, 5).unwrap().unwrap();
        assert_eq!(parse_gen(&p.file_name().unwrap().to_string_lossy()), Some(108));
    }

    #[test]
    fn generation_names_parse_strictly() {
        assert_eq!(parse_gen("01761400000000.h5save"), Some(1_761_400_000_000));
        assert_eq!(parse_gen("1761400000000.h5save"), None, "13 digits");
        assert_eq!(parse_gen("0176140000000x.h5save"), None);
        assert_eq!(parse_gen("01761400000000.zip"), None);
    }

    #[test]
    fn stamps_are_local_civil_time() {
        // 2026-10-25 05:30 UTC.
        let ms = 1_792_906_200_000;
        assert_eq!(local_stamp(ms, 0), "2026-10-25 05:30");
        assert_eq!(local_stamp(ms, 540), "2026-10-25 14:30");
        assert_eq!(local_stamp(ms, -360), "2026-10-24 23:30");
        assert_eq!(local_stamp(0, 0), "1970-01-01 00:00");
        assert_eq!(local_stamp(951_782_400_000, 0), "2000-02-29 00:00");
    }
}
