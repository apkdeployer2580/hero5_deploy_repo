//! Get the player's save off a device we cannot `run-as` into.
//!
//! The release APK is not `debuggable`, so `adb shell run-as` is refused and
//! the save under `<files>/data/data/co.kr.eamobile.HeroesLore5/files/` is
//! unreachable from the host side. That is normally fine — the save is the
//! player's — but it blocks one specific kind of work: reproducing a
//! *location-dependent* bug offline. The frame drop near the shore is that
//! kind. The map effect that causes it is set by the event script when the
//! player walks into that screen, so the probe cannot reach it without
//! standing where the player was standing.
//!
//! Asking the player to replay to that spot in the debuggable bench build is
//! not an option (they declined, and reasonably: nobody knows how many hours
//! of play the water region is behind). So the release build itself carries
//! the save out, and it does it through the one channel that needs no
//! permission and no writable shared directory: **logcat**.
//!
//! `/sdcard/Android/data/<pkg>/` was the obvious alternative and is the worse
//! one — recent Android and Samsung builds gate `adb` reads of that tree, and
//! a failure there looks like "the file is not there" rather than "you may not
//! read it". A base64 line in logcat either arrives or does not.
//!
//! This is diagnostics, not a feature: it is off unless the build asks for it
//! (`HL5_DUMP_SAVE`, i.e. `build_android.ps1 -DumpSave`) or a switch file says
//! so, and it only ever *reads*. Restoring the bytes is the operator's job on
//! the host side, with [`decode`].

use std::path::Path;

/// Where the guest keeps its own files, relative to the host storage root.
/// The guest builds absolute paths under [`crate::INTERNAL_STORAGE_ROOT`], so
/// that prefix is part of the on-disk location — the same trap
/// [`crate::rates`] documents for the data-file shadow.
fn save_dir(storage: &Path) -> std::path::PathBuf {
    storage.join(crate::INTERNAL_STORAGE_ROOT.trim_start_matches('/'))
}

/// Files the guest writes that are worth carrying out. `kickass` is the hero
/// record and `LOCAL.sav` the world state; the rest of that directory is the
/// data-file shadow this host wrote itself, which the operator already has.
const SAVE_FILES: [&str; 2] = ["kickass", "LOCAL.sav"];

/// One logcat line's worth of base64. Short enough that no buffer in the chain
/// wraps it, and prefixed with an index so a truncated capture is detectable
/// rather than silently short.
const CHUNK: usize = 480;

/// A save this large is not a save — refuse rather than flood the log.
const MAX_BYTES: usize = 1024 * 1024;

/// Read the save and render it as log lines.
///
/// Every line is self-describing, because the operator reassembles them with
/// `grep` from a capture that also holds thousands of unrelated lines:
///
/// ```text
/// savedump: kickass 1180 B in 3 chunks
/// savedump: kickass 0/3 <base64>
/// savedump: kickass 1/3 <base64>
/// savedump: kickass 2/3 <base64>
/// savedump: end kickass
/// ```
///
/// A missing file is reported, not skipped: "the save is not there" and "I did
/// not look" have to be distinguishable, which is the same rule
/// [`crate::hle`] applies to unimplemented imports.
pub fn dump(storage: &Path) -> Vec<String> {
    let dir = save_dir(storage);
    let mut out = vec![format!("savedump: reading {}", dir.display())];
    for name in SAVE_FILES {
        let path = dir.join(name);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                out.push(format!("savedump: {name} unreadable: {e}"));
                continue;
            }
        };
        if bytes.len() > MAX_BYTES {
            out.push(format!(
                "savedump: {name} is {} B, over the {MAX_BYTES} B cap — skipped",
                bytes.len()
            ));
            continue;
        }
        let b64 = encode(&bytes);
        let chunks = b64.len().div_ceil(CHUNK).max(1);
        out.push(format!(
            "savedump: {name} {} B in {chunks} chunks",
            bytes.len()
        ));
        for (i, chunk) in b64.as_bytes().chunks(CHUNK).enumerate() {
            // `chunks` splits on byte boundaries, and base64 is ASCII, so this
            // is always valid UTF-8.
            out.push(format!(
                "savedump: {name} {i}/{chunks} {}",
                std::str::from_utf8(chunk).unwrap_or("")
            ));
        }
        out.push(format!("savedump: end {name}"));
    }
    out
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding. Written out rather than pulled in: this crate
/// has no base64 dependency and one diagnostic switch is not a reason to add
/// one to the shipped binary.
pub fn encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let b = [group[0], *group.get(1).unwrap_or(&0), *group.get(2).unwrap_or(&0)];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        s.push(ALPHABET[(n >> 18) as usize & 63] as char);
        s.push(ALPHABET[(n >> 12) as usize & 63] as char);
        s.push(if group.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if group.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    s
}

/// Inverse of [`encode`], for the host tool that reassembles a logcat capture.
/// Rejects anything that is not well-formed rather than returning a truncated
/// buffer — a half-decoded save that still "works" would be the worst outcome.
pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    let mut rev = [255u8; 256];
    for (i, &c) in ALPHABET.iter().enumerate() {
        rev[c as usize] = i as u8;
    }
    let body: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if body.len() % 4 != 0 {
        return Err(format!("base64 length {} is not a multiple of 4", body.len()));
    }
    let mut out = Vec::with_capacity(body.len() / 4 * 3);
    for quad in body.chunks(4) {
        let pad = quad.iter().filter(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && quad[3] != b'=') {
            return Err("misplaced base64 padding".to_string());
        }
        let mut n = 0u32;
        for &c in quad {
            let v = if c == b'=' { 0 } else { rev[c as usize] };
            if v == 255 {
                return Err(format!("byte {c:#x} is not base64"));
            }
            n = n << 6 | u32::from(v);
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_every_length_class() {
        // The three residues mod 3 are the whole space of padding cases, and
        // the empty input is the fourth thing that has ever broken a hand
        // written base64.
        for n in 0..64usize {
            let src: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            let round = decode(&encode(&src)).expect("decodes");
            assert_eq!(round, src, "length {n}");
        }
    }

    #[test]
    fn base64_matches_known_vectors() {
        // RFC 4648 section 10, so a rewrite of `encode` cannot quietly invent
        // its own alphabet or padding.
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn decode_rejects_malformed_input() {
        assert!(decode("Zg=").is_err(), "short quad");
        assert!(decode("Zg=x").is_err(), "padding before data");
        assert!(decode("Zg!=").is_err(), "byte outside the alphabet");
    }

    #[test]
    fn dump_reports_a_missing_save_instead_of_staying_quiet() {
        let dir = std::env::temp_dir().join("hl5_savedump_missing");
        let _ = std::fs::remove_dir_all(&dir);
        let lines = dump(&dir);
        assert!(lines.iter().any(|l| l.contains("kickass unreadable")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("LOCAL.sav unreadable")), "{lines:?}");
    }

    #[test]
    fn dump_chunks_a_save_and_the_chunks_reassemble() {
        let dir = std::env::temp_dir().join("hl5_savedump_ok");
        let files = dir.join(crate::INTERNAL_STORAGE_ROOT.trim_start_matches('/'));
        std::fs::create_dir_all(&files).expect("mkdir");
        // Longer than one chunk, so the reassembly path is the one under test.
        let payload: Vec<u8> = (0..2000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(files.join("kickass"), &payload).expect("write");
        let lines = dump(&dir);
        let joined: String = lines
            .iter()
            .filter_map(|l| l.strip_prefix("savedump: kickass "))
            .filter_map(|rest| rest.split_once(' '))
            .filter(|(head, _)| head.contains('/'))
            .map(|(_, b64)| b64)
            .collect();
        assert_eq!(decode(&joined).expect("decodes"), payload);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
