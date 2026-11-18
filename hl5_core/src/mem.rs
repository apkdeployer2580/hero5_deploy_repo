//! The guest's 32-bit address space.
//!
//! Everything the guest can touch lives inside one contiguous host allocation
//! mapped at [`GUEST_BASE`]. A guest address translates to a host offset with a
//! single subtract, which matters: the interpreter issues a `Memory` call for
//! every load, every store, and every instruction fetch, so this is the hottest
//! code in the process after the decoder itself.
//!
//! Layout (all constants are guest addresses):
//!
//! ```text
//!   0x0100_0000  image      libHeroesLore5.so LOAD segments + .bss
//!   0x0180_0000  hle        one word per imported symbol — see hle::tramp_addr
//!   0x0181_0000  jni        synthetic JNIEnv, its function table, fake handles
//!   0x0190_0000  heap       guest malloc arena
//!   0x0d90_0000  stack      guest stack, grows down from the top
//!   0x0db0_0000  (end)
//! ```
//!
//! The image needs 0x176b68 bytes (`.bss` ends at 0x1652a8 + 0x118c0, then the
//! loader's `.prgend`), so the 8 MB between `IMAGE_BASE` and `HLE_BASE` is a
//! comfortable fit for a 1.5 MB image.
//!
//! Guest address 0 is deliberately outside the window so a null dereference
//! faults here instead of silently reading the ELF header.

use arm32_cpu::Memory;

pub const GUEST_BASE: u32 = 0x0100_0000;

pub const IMAGE_BASE: u32 = 0x0100_0000;
pub const HLE_BASE: u32 = 0x0180_0000;
pub const HLE_SIZE: u32 = 0x0001_0000;
pub const JNI_BASE: u32 = 0x0181_0000;
pub const JNI_SIZE: u32 = 0x0007_0000;
pub const HEAP_BASE: u32 = 0x0190_0000;
pub const HEAP_SIZE: u32 = 0x0c00_0000; // 192 MB
pub const STACK_BASE: u32 = HEAP_BASE + HEAP_SIZE;
pub const STACK_SIZE: u32 = 0x0020_0000; // 2 MB
pub const GUEST_END: u32 = STACK_BASE + STACK_SIZE;

/// Address the runtime pushes as LR when it calls into guest code. Execution
/// stops when PC reaches it. Chosen outside the mapped window so a stray branch
/// to it can never be mistaken for real code.
pub const RETURN_MAGIC: u32 = 0xffff_0000;

pub struct GuestMem {
    ram: Box<[u8]>,
    /// Set when a load or store fell outside the window. The interpreter's
    /// `Memory` trait is infallible, so faults are reported out of band and
    /// checked by the run loop after each step.
    pub fault: Option<Fault>,
}

#[derive(Debug, Clone, Copy)]
pub struct Fault {
    pub addr: u32,
    pub write: bool,
    pub width: u8,
}

impl GuestMem {
    pub fn new() -> Self {
        let len = (GUEST_END - GUEST_BASE) as usize;
        // `vec![0u8; len]` goes through `alloc_zeroed`, so the pages are
        // committed lazily by the OS rather than touched now.
        Self { ram: vec![0u8; len].into_boxed_slice(), fault: None }
    }

    /// Record an out-of-range access. Kept `#[cold]` and out of line so the
    /// fast paths stay a load and a branch.
    #[cold]
    #[inline(never)]
    fn fault(&mut self, addr: u32, width: u8, write: bool) {
        if self.fault.is_none() {
            self.fault = Some(Fault { addr, write, width });
        }
    }

    pub fn take_fault(&mut self) -> Option<Fault> {
        self.fault.take()
    }

    /// Borrow a guest range as host bytes. Returns `None` if any of it is
    /// outside the window. This one does *not* set `fault` — callers are host
    /// code that wants to handle the failure itself.
    pub fn slice(&self, addr: u32, len: u32) -> Option<&[u8]> {
        if addr < GUEST_BASE {
            return None;
        }
        let off = (addr - GUEST_BASE) as usize;
        self.ram.get(off..off.checked_add(len as usize)?)
    }

    pub fn slice_mut(&mut self, addr: u32, len: u32) -> Option<&mut [u8]> {
        if addr < GUEST_BASE {
            return None;
        }
        let off = (addr - GUEST_BASE) as usize;
        let end = off.checked_add(len as usize)?;
        self.ram.get_mut(off..end)
    }

    pub fn write_bytes(&mut self, addr: u32, data: &[u8]) {
        if let Some(dst) = self.slice_mut(addr, data.len() as u32) {
            dst.copy_from_slice(data);
        } else {
            self.fault = Some(Fault { addr, write: true, width: 0 });
        }
    }

    pub fn read_u32(&self, addr: u32) -> u32 {
        self.slice(addr, 4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("slice(_, 4)")))
            .unwrap_or(0)
    }

    pub fn write_u32(&mut self, addr: u32, v: u32) {
        self.write_bytes(addr, &v.to_le_bytes());
    }

    /// A byte read. Indexed images are the guest's usual sprite format, so a
    /// ported blitter reads one of these per pixel.
    pub fn read_u8(&self, addr: u32) -> u8 {
        self.slice(addr, 1).map(|b| b[0]).unwrap_or(0)
    }

    /// A 16-bit read. The blitters this guest spends its time in are all
    /// RGB565, so half-words are what a ported one moves.
    pub fn read_u16(&self, addr: u32) -> u16 {
        self.slice(addr, 2)
            .map(|b| u16::from_le_bytes(b.try_into().expect("slice(_, 2)")))
            .unwrap_or(0)
    }

    pub fn write_u16(&mut self, addr: u32, v: u16) {
        self.write_bytes(addr, &v.to_le_bytes());
    }

    /// Read a NUL-terminated guest string as **raw bytes**.
    ///
    /// This is the one to use for anything the guest will see again. Its text is
    /// EUC-KR (the same midas layer as the sibling project, where this was
    /// established), so decoding it as UTF-8 and re-encoding would replace every
    /// high byte with U+FFFD and silently rewrite the string.
    pub fn cstr_bytes(&self, addr: u32) -> Vec<u8> {
        const MAX: u32 = 8192;
        if addr < GUEST_BASE {
            return Vec::new();
        }
        let avail = MAX.min(GUEST_END.saturating_sub(addr));
        let Some(bytes) = self.slice(addr, avail) else {
            return Vec::new();
        };
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        bytes[..end].to_vec()
    }

    /// Read a NUL-terminated guest string, lossily decoded to UTF-8. Only for
    /// values the host itself consumes — file paths, asset names, log lines —
    /// all of which are ASCII. Never for something that goes back to the guest:
    /// use [`Self::cstr_bytes`].
    pub fn cstr(&self, addr: u32) -> String {
        String::from_utf8_lossy(&self.cstr_bytes(addr)).into_owned()
    }
}

impl Default for GuestMem {
    fn default() -> Self {
        Self::new()
    }
}

impl Memory for GuestMem {
    // ── Why these are written as slice `get` + `try_into` ────────────────────
    //
    // The obvious form, `u32::from_le_bytes([self.ram[o], self.ram[o+1], ...])`,
    // is four separately bounds-checked byte loads that the backend then has to
    // prove it can merge. It does not always manage that, and the interpreter
    // issues one of these per guest instruction fetch plus one per guest load or
    // store — measured on the sibling project, this shape is worth roughly a
    // third of the interpreter's throughput over the byte-indexed one.
    //
    // `get(o..o + N)` is a single bounds check and `try_into` on a fixed-size
    // slice lowers to one (possibly unaligned) load. The out-of-range arm folds
    // to a cold branch. `wrapping_sub` means an address below `GUEST_BASE`
    // becomes a huge offset and fails the same check, so there is no separate
    // lower-bound test.

    #[inline(always)]
    fn r8(&mut self, addr: u32) -> u8 {
        let off = addr.wrapping_sub(GUEST_BASE) as usize;
        match self.ram.get(off) {
            Some(&b) => b,
            None => {
                self.fault(addr, 1, false);
                0
            }
        }
    }

    #[inline(always)]
    fn r16(&mut self, addr: u32) -> u16 {
        let off = addr.wrapping_sub(GUEST_BASE) as usize;
        match self.ram.get(off..off + 2) {
            Some(b) => u16::from_le_bytes(b.try_into().unwrap()),
            None => {
                self.fault(addr, 2, false);
                0
            }
        }
    }

    #[inline(always)]
    fn r32(&mut self, addr: u32) -> u32 {
        let off = addr.wrapping_sub(GUEST_BASE) as usize;
        match self.ram.get(off..off + 4) {
            Some(b) => u32::from_le_bytes(b.try_into().unwrap()),
            None => {
                self.fault(addr, 4, false);
                0
            }
        }
    }

    #[inline(always)]
    fn w8(&mut self, addr: u32, val: u8) {
        let off = addr.wrapping_sub(GUEST_BASE) as usize;
        match self.ram.get_mut(off) {
            Some(b) => *b = val,
            None => self.fault(addr, 1, true),
        }
    }

    #[inline(always)]
    fn w16(&mut self, addr: u32, val: u16) {
        let off = addr.wrapping_sub(GUEST_BASE) as usize;
        match self.ram.get_mut(off..off + 2) {
            Some(b) => b.copy_from_slice(&val.to_le_bytes()),
            None => self.fault(addr, 2, true),
        }
    }

    #[inline(always)]
    fn w32(&mut self, addr: u32, val: u32) {
        let off = addr.wrapping_sub(GUEST_BASE) as usize;
        match self.ram.get_mut(off..off + 4) {
            Some(b) => b.copy_from_slice(&val.to_le_bytes()),
            None => self.fault(addr, 4, true),
        }
    }
}
