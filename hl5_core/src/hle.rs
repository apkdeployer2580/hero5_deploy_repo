//! High-level emulation of everything outside the guest binary.
//!
//! `libHeroesLore5.so` imports exactly 77 symbols — 50 libc, 19 GLES 1.x, 4
//! libstdc++/C++-ABI, 3 libm, 1 liblog. Each gets a one-word trampoline in the
//! HLE window and the loader points its `R_ARM_JUMP_SLOT` (or, for the two
//! unwinder literals, its `R_ARM_ABS32`) there, so a guest call through the PLT
//! lands on an address the run loop recognises *before* it fetches an
//! instruction from it. The guest binary is never modified.
//!
//! **An import with no implementation is an error naming itself.** Returning 0
//! and carrying on is the single worst thing this layer can do: the symptom
//! arrives hundreds of thousands of instructions later as a black screen with
//! no cause attached.
//!
//! Bring-up status: all 76 trampoline-bound imports are served. libc, libm,
//! threads and the printf engine live here; the file/socket seam is
//! `files.rs`, the GLES capture state machine is `gl.rs`. JNI and the
//! `CletEntry` callbacks are the remaining unimplemented seam — they abort by
//! name, which is exactly the signal the next increment works from.

use crate::fmt::{self, VaSource};
use crate::mem::{HLE_BASE, RETURN_MAGIC};
use crate::Vm;
use arm32_cpu::{reg, Memory, Mode};

// ---- trampoline address space -------------------------------------------

pub const TRAMP_IMPORT: usize = 0;
pub const TRAMP_JNIENV: usize = 256;
pub const TRAMP_JAVAVM: usize = 512;
pub const TRAMP_COUNT: usize = 528;

/// Guest word holding `__stack_chk_guard`. It is a data import, not a function,
/// so it must resolve to storage rather than to a trampoline — hand the guest a
/// trampoline and it reads that address as its stack canary.
pub const STACK_GUARD_ADDR: u32 = HLE_BASE + 0x8000;

/// The word `__errno()` hands out. bionic's errno is a per-thread `int*`; this
/// guest is single-threaded, so one word of HLE-window storage is the whole
/// implementation. Every failing file/socket import writes it.
pub const ERRNO_ADDR: u32 = HLE_BASE + 0x8010;

/// Static buffer `strerror` returns. 96 bytes — the longest message written
/// into it is under 40.
pub const STRERROR_BUF: u32 = HLE_BASE + 0x8020;

pub fn tramp_addr(idx: usize) -> u32 {
    HLE_BASE + (TRAMP_IMPORT + idx) as u32 * 4
}

pub fn tramp_index(pc: u32) -> Option<usize> {
    if pc < HLE_BASE {
        return None;
    }
    let off = (pc - HLE_BASE) / 4;
    ((pc - HLE_BASE) % 4 == 0 && (off as usize) < TRAMP_COUNT).then_some(off as usize)
}

/// Imports that are objects rather than functions.
pub fn data_import_addr() -> impl Fn(&str) -> Option<u32> {
    |name: &str| match name {
        "__stack_chk_guard" => Some(STACK_GUARD_ADDR),
        _ => None,
    }
}

/// Every import `Vm::dispatch` below actually serves.
///
/// **Kept in step with the match by hand**, which is a real cost, and it buys
/// the one question that matters during bring-up: `hl5_probe --todo` subtracts
/// this from the guest's imports and prints what is left. A stale entry here
/// would hide work rather than invent it — the match is still the only thing
/// that decides what runs — so the failure mode is a missing line in a report,
/// not a silently wrong guest.
///
/// Some of these (`realloc`, `memcmp`, `strchr`, `strrchr`,
/// `__cxa_pure_virtual`, `fopen`, `fwrite`, `ftell`) are *not* in this guest's
/// import list at all. They cost nothing and they are the obvious neighbours
/// of what is imported, so they stay: the next guest in this family will want
/// them.
///
/// The three `__cxa_*` exception entries count as implemented deliberately:
/// their behaviour is a named diagnostic ("a C++ throw is in flight"), because
/// serving them any other way would need a real ARM EHABI unwinder and nothing
/// observed so far throws. If a throw ever happens, the error says so by name
/// instead of the stack silently unwinding into nonsense.
pub const IMPLEMENTED: &[&str] = &[
    // allocator / mem* / str*
    "malloc",
    "calloc",
    "realloc",
    "free",
    "memcpy",
    "memmove",
    "memset",
    "memcmp",
    "strlen",
    "strcpy",
    "strncpy",
    "strcat",
    "strcmp",
    "strncmp",
    "strchr",
    "strrchr",
    "strstr",
    "strdup",
    // runtime support
    "__gnu_Unwind_Find_exidx",
    "__stack_chk_fail",
    "__android_log_print",
    "abort",
    "__cxa_pure_virtual",
    "__cxa_begin_cleanup",
    "__cxa_call_unexpected",
    "__cxa_type_match",
    // threads — the guest is single-threaded, its one mutex needs no lock
    "pthread_mutex_lock",
    "pthread_mutex_unlock",
    // libc
    "__errno",
    "ftime",
    "qsort",
    "strerror",
    "vsnprintf",
    // libm (soft-float: doubles travel in register pairs)
    "ceil",
    "floor",
    "pow",
    // files + stdio (files.rs)
    "open",
    "close",
    "read",
    "write",
    "lseek",
    "stat",
    "access",
    "mkdir",
    "rmdir",
    "unlink",
    "rename",
    "chdir",
    "dup",
    "fcntl",
    "fdopen",
    "fopen",
    "fread",
    "fwrite",
    "fseek",
    "ftell",
    "fclose",
    // sockets (files.rs) — emulate "the servers are gone"
    "socket",
    "connect",
    "select",
    "recv",
    "send",
    "setsockopt",
    "getsockopt",
    "shutdown",
    "inet_addr",
    // GLES capture (gl.rs)
    "glGenTextures",
    "glDeleteTextures",
    "glBindTexture",
    "glTexImage2D",
    "glTexSubImage2D",
    "glDrawArrays",
    "glVertexPointer",
    "glTexCoordPointer",
    "glEnableClientState",
    "glDisableClientState",
    "glEnable",
    "glDisable",
    "glMatrixMode",
    "glLoadIdentity",
    "glOrthof",
    "glViewport",
    "glColor4f",
    "glTexParameterf",
    "glCullFace",
];

/// A rough bucket for an import name, so the remaining work reads as groups
/// rather than as one flat list of 55 symbols.
pub fn group(name: &str) -> &'static str {
    if name.starts_with("gl") {
        "GLES"
    } else if name.starts_with("__cxa") || name.starts_with("_Unwind") || name.starts_with("_Z") {
        "C++ ABI"
    } else if name.starts_with("pthread") {
        "threads"
    } else if matches!(
        name,
        "socket" | "connect" | "send" | "recv" | "select" | "setsockopt" | "getsockopt"
            | "gethostbyname" | "shutdown" | "bind" | "listen" | "accept" | "inet_addr"
            | "htons" | "htonl" | "ntohs" | "ntohl"
    ) {
        "sockets"
    } else if matches!(
        name,
        "open" | "close" | "read" | "write" | "lseek" | "stat" | "fstat" | "access" | "mkdir"
            | "rmdir" | "unlink" | "chdir" | "fopen" | "fclose" | "fread" | "fwrite" | "fseek"
            | "ftell" | "fdopen" | "fcntl" | "dup" | "opendir" | "readdir" | "closedir"
            | "remove" | "rename"
    ) {
        "files"
    } else if matches!(name, "ceil" | "floor" | "pow" | "sqrt" | "sin" | "cos" | "atan2" | "fabs")
    {
        "libm"
    } else {
        "libc"
    }
}

impl Vm {
    // ---- calling convention helpers --------------------------------------

    /// Argument `n` of the call currently being dispatched. AAPCS: r0-r3, then
    /// the stack. 64-bit arguments occupy two consecutive slots aligned to an
    /// even one — the caller's problem, not this helper's.
    pub(crate) fn arg(&mut self, n: usize) -> u32 {
        if n < 4 {
            self.cpu.reg_get(Mode::User, n as u8)
        } else {
            let sp = self.cpu.reg_get(Mode::User, reg::SP);
            self.mem.r32(sp + ((n - 4) * 4) as u32)
        }
    }

    /// Return from an HLE call: r0 = `v`, PC = LR.
    pub(crate) fn ret(&mut self, v: u32) {
        let lr = self.cpu.reg_get(Mode::User, reg::LR);
        self.cpu.reg_set(Mode::User, 0, v);
        self.cpu.reg_set(Mode::User, reg::PC, lr);
    }

    /// A `double` argument starting at AAPCS slot `lo` (soft-float: the value
    /// travels as two words, low word first). Callers pass an even slot —
    /// the ABI aligns doubleword arguments to even register pairs.
    pub(crate) fn arg_f64(&mut self, lo: usize) -> f64 {
        let l = self.arg(lo) as u64;
        let h = self.arg(lo + 1) as u64;
        f64::from_bits(l | (h << 32))
    }

    /// Return a `double`: r0 = low word, r1 = high word, PC = LR.
    pub(crate) fn ret_f64(&mut self, v: f64) {
        self.ret_pair(v.to_bits());
    }

    /// Return a 64-bit value (a `jlong`): r0 = low word, r1 = high word.
    pub(crate) fn ret_pair(&mut self, bits: u64) {
        let lr = self.cpu.reg_get(Mode::User, reg::LR);
        self.cpu.reg_set(Mode::User, 0, bits as u32);
        self.cpu.reg_set(Mode::User, 1, (bits >> 32) as u32);
        self.cpu.reg_set(Mode::User, reg::PC, lr);
    }

    /// Set the guest's errno (the word behind [`ERRNO_ADDR`]).
    pub(crate) fn set_errno(&mut self, e: u32) {
        self.mem.write_u32(ERRNO_ADDR, e);
    }

    // ---- dispatch ---------------------------------------------------------

    /// Serve one trampoline hit. `id` indexes the same space `tramp_index`
    /// returns: imports below [`TRAMP_JNIENV`], JNI slots above it.
    pub(crate) fn dispatch(&mut self, id: usize) -> Result<(), String> {
        if id >= TRAMP_JNIENV {
            return self.dispatch_jni(id);
        }

        let Some(name) = self.imports.get(id).cloned() else {
            return Err(format!("trampoline {id} has no import bound to it"));
        };

        self.hle_calls += 1;
        if self.trace {
            let lr = self.cpu.reg_get(Mode::User, reg::LR);
            let a: Vec<u32> = (0..4).map(|i| self.arg(i)).collect();
            let from = self.describe(lr);
            self.log.push(format!(
                "{name}({:#x}, {:#x}, {:#x}, {:#x}) <- {from}",
                a[0], a[1], a[2], a[3]
            ));
        }

        match name.as_str() {
            // ---- allocator ------------------------------------------------
            "malloc" => {
                let n = self.arg(0);
                let p = self.heap.alloc(n).unwrap_or(0);
                self.ret(p);
            }
            "calloc" => {
                let count = self.arg(0);
                let each = self.arg(1);
                let n = count.saturating_mul(each);
                let p = self.heap.alloc(n).unwrap_or(0);
                if p != 0 {
                    if let Some(d) = self.mem.slice_mut(p, n) {
                        d.fill(0);
                    }
                }
                self.ret(p);
            }
            "realloc" => {
                let old = self.arg(0);
                let n = self.arg(1);
                if old == 0 {
                    let p = self.heap.alloc(n).unwrap_or(0);
                    self.ret(p);
                } else if n == 0 {
                    self.heap.free(old);
                    self.ret(0);
                } else {
                    let keep = self.heap.size_of(old).unwrap_or(0).min(n);
                    let p = self.heap.alloc(n).unwrap_or(0);
                    if p != 0 {
                        let src = self.mem.slice(old, keep).map(<[u8]>::to_vec).unwrap_or_default();
                        self.mem.write_bytes(p, &src);
                        self.heap.free(old);
                    }
                    self.ret(p);
                }
            }
            "free" => {
                let p = self.arg(0);
                if p != 0 {
                    self.heap.free(p);
                }
                self.ret(0);
            }

            // ---- mem* -----------------------------------------------------
            "memcpy" | "memmove" => {
                let d = self.arg(0);
                let s = self.arg(1);
                let n = self.arg(2);
                let Some(src) = self.mem.slice(s, n).map(<[u8]>::to_vec) else {
                    return Err(bad_range(&name, s, n));
                };
                self.mem.write_bytes(d, &src);
                self.ret(d);
            }
            "memset" => {
                let d = self.arg(0);
                let c = self.arg(1) as u8;
                let n = self.arg(2);
                let Some(b) = self.mem.slice_mut(d, n) else {
                    return Err(bad_range(&name, d, n));
                };
                b.fill(c);
                self.ret(d);
            }
            "memcmp" => {
                let a = self.arg(0);
                let b = self.arg(1);
                let n = self.arg(2);
                let (Some(x), Some(y)) = (self.mem.slice(a, n), self.mem.slice(b, n)) else {
                    return Err(bad_range(&name, a, n));
                };
                let r = cmp_bytes(x, y);
                self.ret(r as u32);
            }

            // ---- str* -----------------------------------------------------
            "strlen" => {
                let p = self.arg(0);
                let n = self.mem.cstr_bytes(p).len() as u32;
                self.ret(n);
            }
            "strcpy" => {
                let d = self.arg(0);
                let s = self.arg(1);
                let mut b = self.mem.cstr_bytes(s);
                b.push(0);
                self.mem.write_bytes(d, &b);
                self.ret(d);
            }
            "strncpy" => {
                let d = self.arg(0);
                let s = self.arg(1);
                let n = self.arg(2) as usize;
                let src = self.mem.cstr_bytes(s);
                // C pads the destination with NULs out to `n` and does *not*
                // terminate when the source is longer.
                let mut b = vec![0u8; n];
                let take = src.len().min(n);
                b[..take].copy_from_slice(&src[..take]);
                self.mem.write_bytes(d, &b);
                self.ret(d);
            }
            "strcat" => {
                let d = self.arg(0);
                let s = self.arg(1);
                let at = d + self.mem.cstr_bytes(d).len() as u32;
                let mut b = self.mem.cstr_bytes(s);
                b.push(0);
                self.mem.write_bytes(at, &b);
                self.ret(d);
            }
            "strcmp" => {
                let pa = self.arg(0);
                let pb = self.arg(1);
                let a = self.mem.cstr_bytes(pa);
                let b = self.mem.cstr_bytes(pb);
                let r = cmp_bytes(&a, &b);
                self.ret(r as u32);
            }
            "strncmp" => {
                let pa = self.arg(0);
                let pb = self.arg(1);
                let n = self.arg(2) as usize;
                let a = self.mem.cstr_bytes(pa);
                let b = self.mem.cstr_bytes(pb);
                let r = cmp_bytes(&a[..a.len().min(n)], &b[..b.len().min(n)]);
                self.ret(r as u32);
            }
            "strchr" | "strrchr" => {
                let p = self.arg(0);
                let c = self.arg(1) as u8;
                let b = self.mem.cstr_bytes(p);
                let pos = if name == "strchr" {
                    b.iter().position(|&x| x == c)
                } else {
                    b.iter().rposition(|&x| x == c)
                };
                // The terminating NUL counts as part of the string for both.
                let r = match pos {
                    Some(i) => p + i as u32,
                    None if c == 0 => p + b.len() as u32,
                    None => 0,
                };
                self.ret(r);
            }
            "strstr" => {
                let h = self.arg(0);
                let n = self.arg(1);
                let hay = self.mem.cstr_bytes(h);
                let ned = self.mem.cstr_bytes(n);
                let r = if ned.is_empty() {
                    h
                } else if ned.len() > hay.len() {
                    0
                } else {
                    match hay.windows(ned.len()).position(|w| w == ned) {
                        Some(i) => h + i as u32,
                        None => 0,
                    }
                };
                self.ret(r);
            }

            // ---- runtime support -----------------------------------------
            "__gnu_Unwind_Find_exidx" => {
                // (pc, &count) -> exidx base. The unwinder asks which table
                // covers a PC; there is exactly one image, so the answer is
                // always ours. Entries are 8 bytes each.
                let out = self.arg(1);
                let (base, len) = self.elf.exidx;
                self.mem.write_u32(out, len / 8);
                self.ret(base);
            }
            "__stack_chk_fail" => {
                let lr = self.cpu.reg_get(Mode::User, reg::LR);
                let from = self.describe(lr);
                return Err(format!("guest stack canary died in {from}"));
            }
            "__android_log_print" => {
                // (prio, tag, fmt, ...) — render through the printf engine so
                // the log shows what the guest meant, not just which line.
                let a1 = self.arg(1);
                let tag = self.mem.cstr(a1);
                let fp = self.arg(2);
                let f = self.mem.cstr_bytes(fp);
                let mut va = VaSource::Call { idx: 3 };
                let out = fmt::format(self, &f, &mut va);
                self.log.push(format!("[guest:{tag}] {}", String::from_utf8_lossy(&out)));
                self.ret(1);
            }
            "abort" | "__cxa_pure_virtual" => {
                let lr = self.cpu.reg_get(Mode::User, reg::LR);
                let from = self.describe(lr);
                return Err(format!("guest called {name} from {from}"));
            }
            "__cxa_begin_cleanup" | "__cxa_call_unexpected" | "__cxa_type_match" => {
                // Reached only while a real C++ throw is unwinding. There is
                // no host unwinder; dying by name here is the implementation —
                // it keeps the first exception ever thrown from turning into
                // an unexplainable corrupted-stack crash later.
                let lr = self.cpu.reg_get(Mode::User, reg::LR);
                let from = self.describe(lr);
                return Err(format!(
                    "guest reached C++ exception machinery ({name}) from {from} — \
                     a throw is in flight and the EHABI unwinder is not implemented"
                ));
            }

            // ---- threads ---------------------------------------------------
            // Measured single-threaded guest (one Java-side GL thread drives
            // everything); its mutexes guard nothing. Success, not a stub.
            "pthread_mutex_lock" | "pthread_mutex_unlock" => self.ret(0),

            // ---- libc ------------------------------------------------------
            "__errno" => self.ret(ERRNO_ADDR),
            "strdup" => {
                let s = self.arg(0);
                let mut b = self.mem.cstr_bytes(s);
                b.push(0);
                let p = self.heap.alloc(b.len() as u32).unwrap_or(0);
                if p != 0 {
                    self.mem.write_bytes(p, &b);
                }
                self.ret(p);
            }
            "strerror" => {
                let e = self.arg(0);
                let msg: &[u8] = match e {
                    0 => b"Success",
                    2 => b"No such file or directory",
                    5 => b"I/O error",
                    9 => b"Bad file number",
                    13 => b"Permission denied",
                    17 => b"File exists",
                    22 => b"Invalid argument",
                    24 => b"Too many open files",
                    107 => b"Transport endpoint is not connected",
                    110 => b"Connection timed out",
                    111 => b"Connection refused",
                    115 => b"Operation now in progress",
                    _ => b"Unknown error",
                };
                let mut b = msg.to_vec();
                b.push(0);
                self.mem.write_bytes(STRERROR_BUF, &b);
                self.ret(STRERROR_BUF);
            }
            "ftime" => {
                // struct timeb { time_t time; u16 millitm; i16 timezone, dstflag; }
                let tp = self.arg(0);
                let ms = self.now_ms();
                self.mem.write_u32(tp, (ms / 1000) as u32);
                let milli = (ms % 1000) as u16;
                self.mem.write_bytes(tp + 4, &milli.to_le_bytes());
                self.mem.write_u32(tp + 6, 0); // timezone + dstflag
                self.ret(0);
            }
            "vsnprintf" => {
                let (buf, size, fp, ap) = (self.arg(0), self.arg(1), self.arg(2), self.arg(3));
                let f = self.mem.cstr_bytes(fp);
                let mut va = VaSource::List { ptr: ap };
                let out = fmt::format(self, &f, &mut va);
                if size > 0 {
                    let take = out.len().min(size as usize - 1);
                    self.mem.write_bytes(buf, &out[..take]);
                    self.mem.write_bytes(buf + take as u32, &[0]);
                }
                // C99: the length the output *would* have had.
                self.ret(out.len() as u32);
            }
            "qsort" => {
                // Re-enters the guest: the comparator is guest code, called
                // through Vm::call while the array is still in its original
                // order, so the pointers it receives are real element
                // addresses. The permutation is written back at the end.
                let base = self.arg(0);
                let n = self.arg(1) as usize;
                let sz = self.arg(2) as usize;
                let cmp = self.arg(3);
                if n > 1 && sz > 0 {
                    let total = (n * sz) as u32;
                    let Some(orig) = self.mem.slice(base, total).map(<[u8]>::to_vec) else {
                        return Err(bad_range(&name, base, total));
                    };
                    // Bottom-up merge sort on indices — iterative, stable, and
                    // every comparison can propagate a guest fault as an error.
                    let mut idx: Vec<usize> = (0..n).collect();
                    let mut tmp = idx.clone();
                    let mut width = 1;
                    while width < n {
                        let mut lo = 0;
                        while lo < n {
                            let mid = (lo + width).min(n);
                            let hi = (lo + 2 * width).min(n);
                            let (mut i, mut j, mut k) = (lo, mid, lo);
                            while i < mid && j < hi {
                                let a = base + (idx[i] * sz) as u32;
                                let b = base + (idx[j] * sz) as u32;
                                let r = self
                                    .call(cmp, &[a, b])
                                    .map_err(|e| format!("qsort comparator: {e}"))?
                                    as i32;
                                if r <= 0 {
                                    tmp[k] = idx[i];
                                    i += 1;
                                } else {
                                    tmp[k] = idx[j];
                                    j += 1;
                                }
                                k += 1;
                            }
                            while i < mid {
                                tmp[k] = idx[i];
                                i += 1;
                                k += 1;
                            }
                            while j < hi {
                                tmp[k] = idx[j];
                                j += 1;
                                k += 1;
                            }
                            lo = hi;
                        }
                        std::mem::swap(&mut idx, &mut tmp);
                        width *= 2;
                    }
                    let mut out = Vec::with_capacity(total as usize);
                    for &e in &idx {
                        out.extend_from_slice(&orig[e * sz..(e + 1) * sz]);
                    }
                    self.mem.write_bytes(base, &out);
                }
                self.ret(0);
            }

            // ---- libm ------------------------------------------------------
            "ceil" => {
                let v = self.arg_f64(0).ceil();
                self.ret_f64(v);
            }
            "floor" => {
                let v = self.arg_f64(0).floor();
                self.ret_f64(v);
            }
            "pow" => {
                let x = self.arg_f64(0);
                let y = self.arg_f64(2);
                self.ret_f64(x.powf(y));
            }

            // ---- files, sockets, GLES — their own modules ------------------
            _ => {
                if self.dispatch_files(&name)? {
                    return Ok(());
                }
                if self.dispatch_sockets(&name)? {
                    return Ok(());
                }
                if self.dispatch_gl(&name)? {
                    return Ok(());
                }
                let lr = self.cpu.reg_get(Mode::User, reg::LR);
                let from = self.describe(lr);
                return Err(format!(
                    "unimplemented import `{name}` (trampoline {id}), called from {from}"
                ));
            }
        }
        Ok(())
    }
}

fn bad_range(who: &str, addr: u32, len: u32) -> String {
    format!("{who}: guest range {addr:#010x}..+{len} is outside the mapped window")
}

/// C `memcmp`/`strcmp` ordering as an i32.
fn cmp_bytes(a: &[u8], b: &[u8]) -> i32 {
    for (x, y) in a.iter().zip(b.iter()) {
        if x != y {
            return *x as i32 - *y as i32;
        }
    }
    a.len() as i32 - b.len() as i32
}

const _: () = assert!((TRAMP_COUNT as u32) * 4 < STACK_GUARD_ADDR - HLE_BASE);
const _: () = assert!(RETURN_MAGIC > crate::mem::GUEST_END);
