//! Host `startDes` — the DES driver behind every save, every save-slot read and
//! every encrypted resource.
//!
//! This is the "save button freezes the game" stall, whole. Pressing 저장
//! (`Battle::KeyPressed` −10 → `HERO::SaveAll`) runs
//! `SaveEncryptFile → MX_desEncrypt → startDes` over every save file in one
//! frame, and the shipped DES costs ~3,500 guest instructions per byte:
//!
//! ```text
//!   ingame.txt, frame 3718 (the save tap), --profile-frames 3718..3718
//!     51.7 M guest instructions in one frame   (a gameplay frame is ~0.3 M)
//!     87.6% desRound  5.4% startDes  2.9% binary2char  1.5% swap32bit
//!      1.4% char2binary                         = 98.8% of the frame
//! ```
//!
//! At the ~34 M instructions a second a Galaxy S9 retires, that is a second and
//! a half with the screen frozen. The same code decrypts every save slot the
//! slot-select screen lists and every resource `LoadResDecrypt` opens.
//!
//! It costs that because the shipped implementation works on **bit arrays** —
//! one bit per 32-bit word — so a block is 64 words in, 64 out, with 16 rounds
//! of 48-word expansion, XOR and permutation in between, all of it `ldr`/`str`
//! through the interpreter. It is the same midas-layer DES the sibling
//! chronosSword ported (`cs_core/src/des.rs`), and the same findings hold.
//!
//! # Why this is not "just DES"
//!
//! It encrypts the player's save, so a port that differs by one bit destroys
//! saves irreversibly. What makes a textbook DES the wrong answer, all checked
//! against *this* image:
//!
//! 1. **`MAT_SBOX` is corrupt.** S1 row 3 column 10 (`MAT_SBOX[0x3a]`) is `2`
//!    where FIPS 46-3 has `3`; that row is not a permutation of 0..15 — the
//!    only one of the 32 that is not. And in this build both wrappers
//!    **store `2` there again before every call** (`strb r4,[MAT_SBOX,#0x3a]`
//!    at 0x39624 / 0x3956C), so the value is live state, not just a table.
//! 2. **The final swap is omitted**: the output is `IP⁻¹(L16 ‖ R16)`. The
//!    scheme inverts itself — its decrypt undoes its encrypt — but it is not
//!    FIPS DES.
//! 3. **The block count truncates**: the wrappers pass `len / 8` (C division,
//!    0x39680..0x3968C), so a length that is not a multiple of eight leaves its
//!    tail in cleartext. Hooking the driver, below the wrappers, inherits that.
//!
//! So every table is read out of guest memory by symbol name on every call,
//! never hardcoded, and the algorithm is transcribed from this binary's
//! instruction stream. `desRound`, `char2binary`, `binary2char` and
//! `swap32bit` are instruction-for-instruction the chronosSword ones (only
//! PLT and GOT offsets differ, and those resolve to the same four tables);
//! `startDes` is not — it is twelve bytes longer and **returns `blocks * 8`**
//! on every path (`[sp,#0x20]`, set at 0x39328 and at 0x39268), which the
//! host reproduces. On top of that the host **proves itself against the
//! guest's own `startDes`** before it is trusted with a byte — see
//! [`Vm::des_verify`].
//!
//! # Why `startDes` is the place
//!
//! Its only callers are `MX_desEncrypt` (0x395F8) and `MX_desDecrypt`
//! (0x39540), and theirs are `SaveEncryptFile`, `LoadDecryptFile` and
//! `LoadResDecrypt` — nothing reaches `desRound` any other way.
//!
//! The direction-dependent state lives in the wrappers: each reverses
//! `KEY4REAL`'s sixteen 48-byte round keys in place when the `KEY4ENCRYPT`
//! flag says the schedule is in the other order (the loops at 0x3963C and
//! 0x39584), **before** calling `startDes`, and `desRound` always walks rounds
//! 0..15 forward. Serving the shared driver therefore inherits whatever order
//! the guest just established and needs no model of the flag at all. It also
//! makes the self-check side-effect-free: `startDes` only *reads* `KEY4REAL`
//! and writes its own stack and the buffer it was handed.

use crate::mem::RETURN_MAGIC;
use crate::Vm;
use arm32_cpu::{reg, Mode};

/// `startDes(mode, blocks, buf)`: `MX_desDecrypt` passes 0 (`mov r0,#0` at
/// 0x395D8), `MX_desEncrypt` passes 1 (0x39690), and `startDes` only ever
/// tests it against zero (`cmp r0,#0 / beq` at 0x393C0).
const DECRYPT: u32 = 0;

/// The tables, read from guest memory on every call.
///
/// The permutations are in `.data`; `KEY4REAL` is in `.bss` and means nothing
/// until `MX_desInit` (from `onStartApp+0x28`) has built it. Re-read every call
/// rather than cached: the wrappers reverse `KEY4REAL` in place, so its order
/// is call-history state, and they rewrite `MAT_SBOX[0x3a]`. A host that
/// snapshotted either at boot would be serving a different function. The cost
/// is under 1.5 KB against millions of interpreter steps.
struct Tables {
    /// Initial permutation, 64 entries, **1-based**.
    ip: [u8; 64],
    /// Inverse initial permutation, 64 entries, 1-based.
    ip_inv: [u8; 64],
    /// E expansion, 48 entries, 1-based, indexing the 32-word right half.
    exp: [u8; 48],
    /// P permutation, 32 entries, 1-based, indexing the 32 S-box output bits.
    permu: [u8; 32],
    /// Eight 4x16 S-boxes, row-major. **Not** the FIPS tables — see the module
    /// comment.
    sbox: [u8; 512],
    /// Sixteen 48-byte round keys, one bit per byte, in whatever order the
    /// calling wrapper last left them.
    key: [u8; 768],
}

impl Tables {
    /// `None` — and the caller declines — if a symbol is missing or a value is
    /// one the transcription below could not reproduce.
    ///
    /// The guest indexes its stack arrays with these bytes and never checks
    /// them, so an index outside `1..=n` would read a neighbouring local; and it
    /// XORs key bytes into words that then feed the S-box index arithmetic, so a
    /// key byte other than 0/1 would index outside the box. Neither happens
    /// with the shipped tables (`MX_desInit` stores `char2binary` output, which
    /// is 0/1 by construction). Refusing instead of guessing keeps the host
    /// exact by construction rather than by assumption.
    fn read(vm: &Vm) -> Option<Self> {
        let mut t = Tables {
            ip: [0; 64],
            ip_inv: [0; 64],
            exp: [0; 48],
            permu: [0; 32],
            sbox: [0; 512],
            key: [0; 768],
        };
        let slurp = |name: &str, out: &mut [u8]| -> Option<()> {
            let addr = vm.symbol(name)?;
            out.copy_from_slice(vm.mem.slice(addr, out.len() as u32)?);
            Some(())
        };
        slurp("MAT_IP", &mut t.ip)?;
        slurp("MAT_IP_1", &mut t.ip_inv)?;
        slurp("MAT_EXP", &mut t.exp)?;
        slurp("MAT_PERMU", &mut t.permu)?;
        slurp("MAT_SBOX", &mut t.sbox)?;
        slurp("KEY4REAL", &mut t.key)?;
        let in_range = |tbl: &[u8], n: u8| tbl.iter().all(|&v| (1..=n).contains(&v));
        let ok = in_range(&t.ip, 64)
            && in_range(&t.ip_inv, 64)
            && in_range(&t.exp, 32)
            && in_range(&t.permu, 32)
            && t.key.iter().all(|&b| b <= 1);
        ok.then_some(t)
    }
}

/// One 64-bit block as the guest holds it: 64 words, one bit per word.
type Bits = [u8; 64];

/// `char2binary` (0x35818): for each byte, bits 7 down to 0 into consecutive
/// words — `dst[i*8 + (7-b)] = (src[i] >> b) & 1`. MSB first.
fn to_bits(block: &[u8]) -> Bits {
    let mut out = [0u8; 64];
    for (i, &byte) in block.iter().enumerate().take(8) {
        for b in 0..8 {
            out[i * 8 + (7 - b)] = (byte >> b) & 1;
        }
    }
    out
}

/// `binary2char` (0x35864): accumulates `1 << (7 - i)` and treats **any**
/// non-zero word as a set bit (`cmp r2,#0 / beq`). Every word here is already
/// 0 or 1, but the guest's rule is reproduced rather than assumed.
fn from_bits(bits: &[u8]) -> u8 {
    let mut out = 0u8;
    for (i, &bit) in bits.iter().enumerate().take(8) {
        if bit != 0 {
            out |= 1 << (7 - i);
        }
    }
    out
}

/// `desRound(a, b)` (0x35580). All sixteen rounds, in place on the two
/// 32-word halves. There is no direction argument: the subkeys are walked
/// forward (`[sp]` += 0x30 up to 0x300) and the caller has already put them in
/// the right order.
fn rounds(a: &mut [u8; 32], b: &mut [u8; 32], t: &Tables) {
    let mut expanded = [0u8; 48];
    let mut sboxout = [0u8; 32];
    let mut permout = [0u8; 32];
    for round in 0..16 {
        // 0x35630: the old B is copied aside first; it becomes the new A.
        let old_b = *b;
        // 0x3564C / 0x3567C: E(B), then XOR with the round's 48 key bytes.
        for i in 0..48 {
            expanded[i] = b[t.exp[i] as usize - 1] ^ t.key[round * 48 + i];
        }
        // 0x356B4..0x35728: eight 6-word groups. The index is
        // `sbox + 16*(2*x0 + x5 + 4n) + 2*(4*x1 + 2*x2 + x3) + x4` — i.e. row
        // `2*b1 + b6`, column `b2 b3 b4 b5` of box n — and the nibble unpacks
        // MSB first into four consecutive words.
        for n in 0..8 {
            let x = &expanded[n * 6..n * 6 + 6];
            let row = 2 * x[0] as usize + x[5] as usize;
            let col = 8 * x[1] as usize + 4 * x[2] as usize + 2 * x[3] as usize + x[4] as usize;
            let v = t.sbox[64 * n + 16 * row + col];
            for bit in 0..4 {
                sboxout[n * 4 + bit] = (v >> (3 - bit)) & 1;
            }
        }
        // 0x35740: P, 1-based.
        for i in 0..32 {
            permout[i] = sboxout[t.permu[i] as usize - 1];
        }
        // 0x35768: B = A ^ P(...), then 0x35788: A = old B.
        for i in 0..32 {
            b[i] = a[i] ^ permout[i];
        }
        *a = old_b;
    }
}

/// One block, as `startDes`'s loop body (0x39334..0x394C4) does it.
fn block(input: &[u8], mode: u32, t: &Tables) -> [u8; 8] {
    let bits = to_bits(input);
    // IP, 1-based (0x3939C: `ldr r2, [sp + 0x30 + 4*MAT_IP[i]]` against the
    // bit array at sp+0x34).
    let mut x = [0u8; 64];
    for i in 0..64 {
        x[i] = bits[t.ip[i] as usize - 1];
    }

    let encrypt = mode != DECRYPT;
    let (mut a, mut b) = ([0u8; 32], [0u8; 32]);
    if encrypt {
        // 0x393D4: A = X[0..32], B = X[32..64].
        a.copy_from_slice(&x[..32]);
        b.copy_from_slice(&x[32..]);
    } else {
        // 0x394F8: crossed — B = X[0..32], A = X[32..64]. This is what makes
        // the shipped scheme invert itself despite the missing final swap.
        b.copy_from_slice(&x[..32]);
        a.copy_from_slice(&x[32..]);
    }
    rounds(&mut a, &mut b, t);
    if encrypt {
        // 0x3940C `swap32bit(A, B, tmp)` — a real three-pass swap. Decrypt
        // branches straight to the tail past it (0x39520).
        std::mem::swap(&mut a, &mut b);
    }

    // 0x39410: the tail always emits B ‖ A, then IP⁻¹, 1-based (0x3944C).
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&b);
    out[32..].copy_from_slice(&a);
    let mut y = [0u8; 64];
    for i in 0..64 {
        y[i] = out[t.ip_inv[i] as usize - 1];
    }
    // 0x39494: eight `binary2char` calls, eight words apart.
    let mut bytes = [0u8; 8];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = from_bits(&y[i * 8..i * 8 + 8]);
    }
    bytes
}

/// Whether the host implementation has been proven against the guest's own
/// code, per direction, for this process.
#[derive(Default)]
pub struct Des {
    checked: [bool; 2],
    trusted: [bool; 2],
    /// Calls the host served, for the probe's report.
    pub served: u64,
}

/// Bytes the self-check round-trips through both implementations.
///
/// Sixteen blocks, because one deviation is only caught probabilistically.
/// Everything else — a bit-order flip, a 1-based slip in any of the five
/// permutations, the missing final swap, the crossed decrypt entry, a wrong
/// subkey order — changes essentially every block. But the corrupt S1 entry
/// fires on exactly one of that box's 64 inputs, so 128 bytes miss it only
/// ~1.8% of the time where 64 would miss it ~13%. The host reads the table out
/// of guest memory and cannot hold the wrong value; this is insurance against
/// a future edit that hardcodes it, at ~450 K guest instructions per
/// direction, once per process.
const PROBE: usize = 128;

/// Room below the caller's `sp` for the probe, with the guest's own frame for
/// `startDes` going below that — see the scratch rule in `patch.rs`.
const PROBE_SCRATCH: u32 = PROBE as u32 + 0x40;

/// The `startDes` entry in `patch::PATCHES`.
pub(crate) fn start_des(vm: &mut Vm) -> Result<bool, String> {
    // The self-check calls the guest's own `startDes` through `Vm::call`, which
    // enters with this LR. Without this the host would answer its own reference
    // call and the check would prove nothing.
    if vm.cpu.reg_get(Mode::User, reg::LR) == RETURN_MAGIC {
        return Ok(false);
    }
    let mode = vm.arg(0);
    let blocks = vm.arg(1) as i32;
    let buf = vm.arg(2);
    // `cmp r1,#0 / ble` at 0x39244: nothing is written and `blocks << 3` is
    // returned. Trivial for the interpreter; leave it the shipped path.
    if blocks <= 0 || buf == 0 {
        return Ok(false);
    }
    // `blocks << 3` is also the loop's signed length (0x39314) — past this it
    // wraps, and the shipped code would do something no real save asks for.
    if blocks > i32::MAX / 8 {
        return Ok(false);
    }
    let dir = usize::from(mode != DECRYPT);
    if !vm.des_verify(mode, dir) {
        return Ok(false);
    }
    let Some(t) = Tables::read(vm) else { return Ok(false) };
    let len = blocks as u32 * 8;
    let Some(src) = vm.mem.slice(buf, len) else { return Ok(false) };

    // The guest copies each block into a 9-byte stack buffer, transforms it and
    // `memcpy`s it back over the input before moving on (0x39378, 0x394C4).
    // One write of the whole span is the same bytes, because no block reads
    // another block.
    let mut out = Vec::with_capacity(len as usize);
    for chunk in src.chunks_exact(8) {
        out.extend_from_slice(&block(chunk, mode, &t));
    }
    vm.mem.write_bytes(buf, &out);

    // The return value (`ldr r0,[sp,#0x20]` at 0x39524). Both wrappers drop it,
    // but it is what the function returns.
    vm.cpu.reg_set(Mode::User, 0, len);
    vm.des.served += 1;
    Ok(true)
}

impl Vm {
    /// Prove both directions now, rather than at the first real call.
    ///
    /// Called at the end of `boot`, which matters for the *encrypt* direction:
    /// its first real call is the player's first save, and although a failed
    /// check only means the interpreter keeps doing the work, it is better that
    /// a disagreement is a boot log line than something first learned at the
    /// save button. It also moves the check's ~30 ms off the frame the player
    /// is waiting on.
    ///
    /// A no-op unless the `startDes` port is installed — `--no-patch` runs must
    /// stay free of host guest-calls.
    pub fn des_selftest(&mut self) {
        let Some(f) = self.symbol("startDes") else { return };
        if !self.patches.contains_key(&f) {
            return;
        }
        for mode in [DECRYPT, 1] {
            self.des_verify(mode, usize::from(mode != DECRYPT));
        }
        // Only an unready key schedule leaves a direction unchecked, and then
        // the first real call checks it instead.
        if !self.des.checked.iter().all(|&c| c) {
            self.log.push("des: MX_desInit has not run; checking at first use".into());
        }
    }

    /// Prove the host DES against the guest's own `startDes`, once per
    /// direction, and refuse to serve that direction ever again if they
    /// differ.
    ///
    /// This is not belt-and-braces. The host is about to rewrite the bytes of
    /// the player's save file, and the only authority on what those bytes
    /// should be is the code being replaced — so it is asked, with the real key
    /// schedule, in the real process.
    ///
    /// Nothing the game depends on is disturbed: `startDes` reads `KEY4REAL`
    /// and writes only its own stack and the buffer it was handed, which here
    /// is scratch below the current `sp`. The `KEY4ENCRYPT` flag and the
    /// `MAT_SBOX[0x3a]` store live in the wrappers, which are not called.
    fn des_verify(&mut self, mode: u32, dir: usize) -> bool {
        if self.des.checked[dir] {
            return self.des.trusted[dir];
        }
        // `KEY4ENCRYPT` ships as 0xff in `.data` and `MX_desInit` sets it to 1:
        // until then the schedule is zeros and a check would compare two
        // results of a key nobody uses. Wait — do not mark it checked.
        match self.symbol("KEY4ENCRYPT").map(|a| self.mem.read_u8(a)) {
            Some(0xff) | None => return false,
            Some(_) => {}
        }
        self.des.checked[dir] = true;
        let Some(func) = self.symbol("startDes") else { return false };
        let Some(t) = Tables::read(self) else {
            self.log.push("des: a table is missing or out of range; DES stays in the guest".into());
            return false;
        };

        // Deterministic, and deliberately not all-distinct: repeated bytes
        // catch a host that indexes by position where the guest indexes by
        // value. Counted in `usize` and cast per byte, so a probe of 256 would
        // not truncate to an empty range and pass vacuously.
        let probe: Vec<u8> =
            (0..PROBE).map(|i| (i as u8).wrapping_mul(37).wrapping_add(11)).collect();
        let mut want = Vec::with_capacity(PROBE);
        for chunk in probe.chunks_exact(8) {
            want.extend_from_slice(&block(chunk, mode, &t));
        }

        let sp0 = self.cpu.reg_get(Mode::User, reg::SP);
        let scratch = (sp0 - PROBE_SCRATCH) & !7;
        self.mem.write_bytes(scratch, &probe);
        self.cpu.reg_set(Mode::User, reg::SP, scratch);
        let ran = self.call(func, &[mode, (PROBE / 8) as u32, scratch]);
        self.cpu.reg_set(Mode::User, reg::SP, sp0);
        let what = if mode == DECRYPT { "decrypt" } else { "encrypt" };
        // This can run inside a real save; a reference call that faults must
        // not take the frame down with it. Untrusted, and the guest keeps the
        // job — it would meet the same fault on its own, or not at all.
        let ret = match ran {
            Ok(r) => r,
            Err(e) => {
                self.des.trusted[dir] = false;
                self.log.push(format!("des: the guest's own {what} faulted in the self-check: {e}"));
                return false;
            }
        };
        let got = self.mem.slice(scratch, PROBE as u32).map(<[u8]>::to_vec).unwrap_or_default();

        self.des.trusted[dir] = got == want && ret == PROBE as u32;
        if self.des.trusted[dir] {
            self.log.push(format!("des: host {what} verified against the guest ({PROBE} bytes)"));
        } else {
            // Loud, because the alternative to noticing is a corrupted save.
            // Name the first block that differs: the corrupt S1 entry fires
            // for one 6-bit input only, so most blocks agree even when wrong.
            let at = got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(0) / 8 * 8;
            let show = |v: &[u8]| v.get(at..at + 8).map(|s| format!("{s:02x?}")).unwrap_or_default();
            self.log.push(format!(
                "des: host {what} DISAGREES with the guest; DES stays in the interpreter. \
                 block {} of {}: guest {} host {}, returned {ret} (want {PROBE})",
                at / 8,
                PROBE / 8,
                show(&got),
                show(&want),
            ));
        }
        self.des.trusted[dir]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table shapes the transcription relies on, with the shipped
    /// permutations. A structural check only — the real proof is the
    /// in-process comparison against the guest's own `startDes`.
    fn identity_tables() -> Tables {
        let mut t = Tables {
            ip: [0; 64],
            ip_inv: [0; 64],
            exp: [0; 48],
            permu: [0; 32],
            sbox: [0; 512],
            key: [0; 768],
        };
        for i in 0..64 {
            t.ip[i] = i as u8 + 1;
            t.ip_inv[i] = i as u8 + 1;
        }
        for i in 0..48 {
            t.exp[i] = (i % 32) as u8 + 1;
        }
        for i in 0..32 {
            t.permu[i] = i as u8 + 1;
        }
        for (i, v) in t.sbox.iter_mut().enumerate() {
            *v = (i % 16) as u8;
        }
        for (i, k) in t.key.iter_mut().enumerate() {
            *k = ((i * 7) % 3 == 0) as u8;
        }
        t
    }

    #[test]
    fn bits_round_trip_msb_first() {
        let bytes = [0x80, 0x01, 0xa5, 0x5a, 0xff, 0x00, 0x13, 0xc8];
        let bits = to_bits(&bytes);
        assert_eq!(bits[0], 1, "bit 7 of byte 0 comes first");
        assert_eq!(bits[15], 1, "bit 0 of byte 1 comes last in its octet");
        for i in 0..8 {
            assert_eq!(from_bits(&bits[i * 8..i * 8 + 8]), bytes[i]);
        }
    }

    #[test]
    fn binary2char_counts_any_nonzero_word_as_set() {
        assert_eq!(from_bits(&[2, 0, 0, 0, 0, 0, 0, 7]), 0x81);
    }

    /// The self-inverse property, with the key order the wrappers establish:
    /// `MX_desDecrypt` reverses the sixteen subkeys (the loop at 0x39584)
    /// before it calls `startDes`, so decryption only undoes encryption with
    /// the schedule reversed — the crossed entry and the missing final swap
    /// are what make that sufficient.
    #[test]
    fn decrypt_inverts_encrypt_despite_the_missing_swap() {
        let enc = identity_tables();
        let mut dec = identity_tables();
        for r in 0..16 {
            dec.key[r * 48..r * 48 + 48].copy_from_slice(&enc.key[(15 - r) * 48..(15 - r) * 48 + 48]);
        }
        for seed in 0u8..32 {
            let plain: Vec<u8> =
                (0..8u8).map(|i| seed.wrapping_mul(29).wrapping_add(i.wrapping_mul(71))).collect();
            let c = block(&plain, 1, &enc);
            assert_ne!(c.as_slice(), plain.as_slice(), "seed {seed}: encryption did nothing");
            assert_eq!(block(&c, DECRYPT, &dec).as_slice(), plain.as_slice(), "seed {seed}");
        }
    }
}
