//! Guest-facing printf.
//!
//! Two byte-level rules matter more than any formatting nicety. The engine
//! works on **bytes**, never `String` — guest text is EUC-KR, and `%s` must
//! pass it through untouched. And every conversion consumes its argument from
//! a [`VaSource`], which knows the two ways variadic arguments reach an
//! import: still sitting in registers/stack at the call boundary
//! (`__android_log_print(prio, tag, fmt, ...)`), or already spilled to guest
//! memory behind a `va_list` (`vsnprintf(buf, n, fmt, ap)`).
//!
//! Coverage is the printf subset a 2010 feature-phone port actually emits:
//! `%d %i %u %x %X %o %c %s %p %f` with flags, width, precision and `l`/`ll`.
//! `%e/%g` fall back to fixed-point — this output feeds logs, not parsers.

use crate::Vm;

pub enum VaSource {
    /// A direct variadic call: the next argument is AAPCS slot `idx`
    /// (r0-r3, then the stack — [`Vm::arg`]'s numbering).
    Call { idx: usize },
    /// A `va_list`: the arguments were spilled to guest memory at `ptr`.
    List { ptr: u32 },
}

impl VaSource {
    /// One 32-bit variadic argument (an `int`, `jboolean`, or `jobject`
    /// handle). Public so the JNI callback layer can walk a method's args.
    pub fn next_word(&mut self, vm: &mut Vm) -> u32 {
        self.next_u32(vm)
    }

    /// One 32-bit argument read as an object handle. Same width as
    /// [`Self::next_word`]; named for intent at the call site.
    pub fn next_obj(&mut self, vm: &mut Vm) -> u32 {
        self.next_u32(vm)
    }

    /// One 64-bit variadic argument (a `jlong`).
    pub fn next_long(&mut self, vm: &mut Vm) -> u64 {
        self.next_u64(vm)
    }

    fn next_u32(&mut self, vm: &mut Vm) -> u32 {
        match self {
            VaSource::Call { idx } => {
                let v = vm.arg(*idx);
                *idx += 1;
                v
            }
            VaSource::List { ptr } => {
                let v = vm.mem.read_u32(*ptr);
                *ptr += 4;
                v
            }
        }
    }

    fn next_u64(&mut self, vm: &mut Vm) -> u64 {
        match self {
            VaSource::Call { idx } => {
                // AAPCS: a doubleword takes an even/odd register pair or an
                // 8-aligned stack slot. Slot 4 is the first stack word and the
                // stack is 8-aligned at the call boundary, so rounding the slot
                // index up to even covers both halves of the sequence.
                if *idx % 2 == 1 {
                    *idx += 1;
                }
                let lo = vm.arg(*idx) as u64;
                let hi = vm.arg(*idx + 1) as u64;
                *idx += 2;
                lo | (hi << 32)
            }
            VaSource::List { ptr } => {
                *ptr = (*ptr + 7) & !7;
                let lo = vm.mem.read_u32(*ptr) as u64;
                let hi = vm.mem.read_u32(*ptr + 4) as u64;
                *ptr += 8;
                lo | (hi << 32)
            }
        }
    }
}

/// Render `fmt` with arguments from `va`. Returns the full output — callers
/// that have a size limit (vsnprintf) truncate on write, because the C return
/// value is the *untruncated* length.
pub fn format(vm: &mut Vm, fmt: &[u8], va: &mut VaSource) -> Vec<u8> {
    let mut out = Vec::with_capacity(fmt.len() + 32);
    let mut i = 0usize;

    while i < fmt.len() {
        if fmt[i] != b'%' {
            out.push(fmt[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i >= fmt.len() {
            out.push(b'%');
            break;
        }
        if fmt[i] == b'%' {
            out.push(b'%');
            i += 1;
            continue;
        }

        // flags
        let (mut left, mut zero, mut plus, mut space, mut alt) = (false, false, false, false, false);
        while i < fmt.len() {
            match fmt[i] {
                b'-' => left = true,
                b'0' => zero = true,
                b'+' => plus = true,
                b' ' => space = true,
                b'#' => alt = true,
                _ => break,
            }
            i += 1;
        }

        // width
        let mut width = 0usize;
        if i < fmt.len() && fmt[i] == b'*' {
            let w = va.next_u32(vm) as i32;
            if w < 0 {
                left = true;
                width = w.unsigned_abs() as usize;
            } else {
                width = w as usize;
            }
            i += 1;
        } else {
            while i < fmt.len() && fmt[i].is_ascii_digit() {
                width = width * 10 + (fmt[i] - b'0') as usize;
                i += 1;
            }
        }

        // precision
        let mut prec: Option<usize> = None;
        if i < fmt.len() && fmt[i] == b'.' {
            i += 1;
            if i < fmt.len() && fmt[i] == b'*' {
                prec = Some((va.next_u32(vm) as i32).max(0) as usize);
                i += 1;
            } else {
                let mut p = 0usize;
                while i < fmt.len() && fmt[i].is_ascii_digit() {
                    p = p * 10 + (fmt[i] - b'0') as usize;
                    i += 1;
                }
                prec = Some(p);
            }
        }

        // length modifiers — only `ll` changes how much is consumed (ints are
        // 32-bit here, `long` included: this is ILP32).
        let mut longs = 0u8;
        while i < fmt.len() {
            match fmt[i] {
                b'l' => {
                    longs += 1;
                    i += 1;
                }
                b'h' | b'z' | b'L' | b'q' | b'j' | b't' => i += 1,
                _ => break,
            }
        }
        if i >= fmt.len() {
            break;
        }
        let conv = fmt[i];
        i += 1;

        match conv {
            b'd' | b'i' => {
                let v: i64 = if longs >= 2 {
                    va.next_u64(vm) as i64
                } else {
                    va.next_u32(vm) as i32 as i64
                };
                let sign = if v < 0 {
                    Some(b'-')
                } else if plus {
                    Some(b'+')
                } else if space {
                    Some(b' ')
                } else {
                    None
                };
                push_num(&mut out, v.unsigned_abs(), 10, false, sign, b"", width, prec, left, zero);
            }
            b'u' | b'x' | b'X' | b'o' => {
                let v: u64 = if longs >= 2 { va.next_u64(vm) } else { va.next_u32(vm) as u64 };
                let (base, upper) = match conv {
                    b'x' => (16, false),
                    b'X' => (16, true),
                    b'o' => (8, false),
                    _ => (10, false),
                };
                let prefix: &[u8] = if alt && v != 0 {
                    match conv {
                        b'x' => b"0x",
                        b'X' => b"0X",
                        b'o' => b"0",
                        _ => b"",
                    }
                } else {
                    b""
                };
                push_num(&mut out, v, base, upper, None, prefix, width, prec, left, zero);
            }
            b'p' => {
                let v = va.next_u32(vm) as u64;
                push_num(&mut out, v, 16, false, None, b"0x", width, Some(8), left, false);
            }
            b'c' => {
                let ch = va.next_u32(vm) as u8;
                pad_bytes(&mut out, &[ch], width, left, false);
            }
            b's' => {
                let p = va.next_u32(vm);
                let mut s = if p == 0 { b"(null)".to_vec() } else { vm.mem.cstr_bytes(p) };
                if let Some(pc) = prec {
                    s.truncate(pc);
                }
                pad_bytes(&mut out, &s, width, left, false);
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                let v = f64::from_bits(va.next_u64(vm));
                let pc = prec.unwrap_or(6);
                let body = std::format!("{v:.pc$}");
                pad_bytes(&mut out, body.as_bytes(), width, left, zero);
            }
            b'n' => {
                let p = va.next_u32(vm);
                vm.mem.write_u32(p, out.len() as u32);
            }
            other => {
                out.push(b'%');
                out.push(other);
            }
        }
    }
    out
}

fn digits(mut v: u64, base: u64, upper: bool) -> Vec<u8> {
    const LO: &[u8; 16] = b"0123456789abcdef";
    const HI: &[u8; 16] = b"0123456789ABCDEF";
    let tab = if upper { HI } else { LO };
    let mut d = Vec::with_capacity(20);
    loop {
        d.push(tab[(v % base) as usize]);
        v /= base;
        if v == 0 {
            break;
        }
    }
    d.reverse();
    d
}

#[allow(clippy::too_many_arguments)]
fn push_num(
    out: &mut Vec<u8>,
    v: u64,
    base: u64,
    upper: bool,
    sign: Option<u8>,
    prefix: &[u8],
    width: usize,
    prec: Option<usize>,
    left: bool,
    zero: bool,
) {
    let mut body = digits(v, base, upper);
    if let Some(pc) = prec {
        while body.len() < pc {
            body.insert(0, b'0');
        }
    }
    // C: an explicit precision turns the `0` flag off.
    let zero = zero && prec.is_none() && !left;

    let head_len = sign.map_or(0, |_| 1) + prefix.len();
    let content = head_len + body.len();
    if zero && width > content {
        // zeros go between the sign/prefix and the digits
        if let Some(s) = sign {
            out.push(s);
        }
        out.extend_from_slice(prefix);
        for _ in 0..width - content {
            out.push(b'0');
        }
        out.extend_from_slice(&body);
        return;
    }
    let mut full = Vec::with_capacity(content);
    if let Some(s) = sign {
        full.push(s);
    }
    full.extend_from_slice(prefix);
    full.extend_from_slice(&body);
    pad_bytes(out, &full, width, left, false);
}

fn pad_bytes(out: &mut Vec<u8>, s: &[u8], width: usize, left: bool, zero: bool) {
    let pad = width.saturating_sub(s.len());
    if left {
        out.extend_from_slice(s);
        for _ in 0..pad {
            out.push(b' ');
        }
    } else {
        let fill = if zero { b'0' } else { b' ' };
        for _ in 0..pad {
            out.push(fill);
        }
        out.extend_from_slice(s);
    }
}
