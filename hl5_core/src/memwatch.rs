//! Who touches this object? — every guest load and store that lands in a
//! watched heap object, by the PC that issued it. **Probe only.**
//!
//! # Why this exists
//!
//! Growing the 공용창고 and the 가방 from five pages to ten (ANALYSIS.md §60)
//! means moving arrays inside two objects whose methods are tiny — a getter
//! is `add r1, r0, r1; ldrb r0, [r1, #0x148]` — and the game is one C++
//! translation unit (`_GLOBAL__I_._code_Heroslore5.cpp`), so GCC was free to
//! inline those getters anywhere. A list of call sites is therefore a lower
//! bound on the code that knows the layout, and an immediate census
//! (`#0x148` anywhere) drowns in `ItemInfo`, whose fields sit at the same
//! numbers. The one source that cannot miss an access it exercised is the
//! interpreter itself: it is already doing every load and store.
//!
//! So this module wraps [`GuestMem`] for the duration of one instruction,
//! and for each access that falls inside a registered region it records the
//! PC, the lowest and highest offset touched, and whether it read or wrote.
//! The report is then checked against the layout rewrite's own table: a PC
//! that touches a moved array and is not in the table is a hole in the static
//! census, found by running rather than by reading.
//!
//! # How regions come and go
//!
//! A region opens when the guest **enters a constructor** (`r0` = the new
//! object) and closes when `operator delete` is handed the allocation it
//! belongs to. Constructor entry rather than allocation, because the heap
//! hands out recycled addresses and only the constructor says what is about
//! to live there. A region left open after its object was freed some other
//! way can only cause *extra* hits — the report is to be read, so a false
//! positive costs a minute and a false negative is the bug this exists to
//! catch.
//!
//! # Cost
//!
//! None unless armed. [`crate::Vm::run`] takes one `Option` test per
//! instruction (next to the profiler's, which is already there) and only
//! steps through [`Watched`] when a watch is set — which only the probe does.

use std::collections::{BTreeMap, HashMap};

use arm32_cpu::Memory;

use crate::mem::GuestMem;

/// What a constructor entry opens: a range relative to `r0`.
#[derive(Clone, Copy, Debug)]
pub struct Opener {
    /// Report label, and the key regions are grouped under.
    pub kind: &'static str,
    /// Start of the region, relative to `r0` at the constructor's entry.
    pub offset: u32,
    pub len: u32,
    /// How far back from `r0` the **allocation** starts — what
    /// `operator delete` will later be handed. Zero for an object that is its
    /// own allocation; `0x17c0` for the `BagItem` embedded in a `HERO`.
    pub owner_back: u32,
}

#[derive(Clone, Copy, Debug)]
struct Region {
    lo: u32,
    hi: u32,
    owner: u32,
    kind: &'static str,
}

/// One PC's dealings with one kind of region.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hit {
    /// Lowest and highest byte offset touched (inclusive of the access
    /// width), relative to the region's start.
    pub lo: u32,
    pub hi: u32,
    pub reads: u64,
    pub writes: u64,
    /// Bit `w` set when an access of `w` bytes was seen (1, 2, 4).
    pub widths: u8,
}

#[derive(Default)]
pub struct MemWatch {
    openers: HashMap<u32, Vec<Opener>>,
    /// `operator delete`'s entry — where regions close.
    deleter: Option<u32>,
    regions: Vec<Region>,
    pub hits: BTreeMap<(&'static str, u32), Hit>,
    /// How many regions each kind has opened over the run. Zero means the
    /// replay never built the object, which is a statement about the replay.
    pub opened: BTreeMap<&'static str, u32>,
}

impl MemWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a region every time the guest enters `ctor`.
    pub fn open_at(&mut self, ctor: u32, o: Opener) {
        self.openers.entry(ctor).or_default().push(o);
        self.opened.entry(o.kind).or_insert(0);
    }

    pub fn close_at(&mut self, deleter: u32) {
        self.deleter = Some(deleter);
    }

    /// A region that is always open: a global, not a heap object.
    pub fn open_fixed(&mut self, lo: u32, len: u32, kind: &'static str) {
        self.regions.push(Region { lo, hi: lo + len, owner: u32::MAX, kind });
        *self.opened.entry(kind).or_insert(0) += 1;
    }

    /// Called by the run loop **before** the instruction at `pc` executes.
    #[inline]
    pub fn at_pc(&mut self, pc: u32, r0: u32) {
        if let Some(list) = self.openers.get(&pc) {
            for o in list.clone() {
                let lo = r0.wrapping_add(o.offset);
                let hi = lo.wrapping_add(o.len);
                // A recycled address: whatever was registered there before is
                // gone, and keeping both would double-count every access.
                self.regions.retain(|r| r.hi <= lo || r.lo >= hi);
                self.regions.push(Region { lo, hi, owner: r0.wrapping_sub(o.owner_back), kind: o.kind });
                *self.opened.entry(o.kind).or_insert(0) += 1;
            }
        }
        if Some(pc) == self.deleter && r0 != 0 {
            self.regions.retain(|r| r.owner != r0);
        }
    }

    #[inline]
    fn note(&mut self, pc: u32, addr: u32, width: u32, write: bool) {
        for r in &self.regions {
            if addr >= r.lo && addr < r.hi {
                let off = addr - r.lo;
                let h = self.hits.entry((r.kind, pc)).or_insert(Hit {
                    lo: u32::MAX,
                    ..Hit::default()
                });
                h.lo = h.lo.min(off);
                h.hi = h.hi.max(off + width - 1);
                if write {
                    h.writes += 1;
                } else {
                    h.reads += 1;
                }
                h.widths |= width as u8;
            }
        }
    }
}

/// [`GuestMem`] seen through a watch, for one instruction.
pub struct Watched<'a> {
    pub mem: &'a mut GuestMem,
    pub w: &'a mut MemWatch,
    pub pc: u32,
}

impl Memory for Watched<'_> {
    fn r8(&mut self, addr: u32) -> u8 {
        self.w.note(self.pc, addr, 1, false);
        self.mem.r8(addr)
    }
    fn r16(&mut self, addr: u32) -> u16 {
        self.w.note(self.pc, addr, 2, false);
        self.mem.r16(addr)
    }
    fn r32(&mut self, addr: u32) -> u32 {
        // Instruction fetch comes through here too; code is never inside a
        // heap region, so it never matches.
        self.w.note(self.pc, addr, 4, false);
        self.mem.r32(addr)
    }
    fn w8(&mut self, addr: u32, val: u8) {
        self.w.note(self.pc, addr, 1, true);
        self.mem.w8(addr, val)
    }
    fn w16(&mut self, addr: u32, val: u16) {
        self.w.note(self.pc, addr, 2, true);
        self.mem.w16(addr, val)
    }
    fn w32(&mut self, addr: u32, val: u32) {
        self.w.note(self.pc, addr, 4, true);
        self.mem.w32(addr, val)
    }
}

/// `operator delete(void*)` — where every region closes.
const DELETE: &str = "_ZdlPv";

/// Arm a watch on the objects named in `kinds` (comma-separated `storage`,
/// `bag`), sized for the layout this run uses: `ten` is whether the ten-page
/// rewrite ([`crate::tenpages`]) is on. Returns one line per region kind for
/// the probe to print.
pub fn arm(vm: &mut crate::Vm, kinds: &str, ten: bool) -> Result<Vec<String>, String> {
    use crate::tenpages as tp;
    let mut w = MemWatch::new();
    let sym = |vm: &crate::Vm, s: &str| vm.symbol(s).ok_or_else(|| format!("memwatch: `{s}` is not in this binary"));
    w.close_at(sym(vm, DELETE)?);
    let mut lines = Vec::new();
    for k in kinds.split(',').map(str::trim).filter(|k| !k.is_empty()) {
        match k {
            "storage" => {
                let len = if ten { tp::STORAGE_SIZE_NEW } else { tp::STORAGE_SIZE_OLD };
                for c in [tp::STORAGE_C1, tp::STORAGE_C2] {
                    let at = sym(vm, c)?;
                    w.open_at(at, Opener { kind: "storage", offset: 0, len, owner_back: 0 });
                }
                lines.push(format!("storage: NetStorageItem, {len:#x} bytes from `this`"));
            }
            "bag" => {
                for c in [tp::BAG_C1, tp::BAG_C2] {
                    let at = sym(vm, c)?;
                    w.open_at(
                        at,
                        Opener { kind: "bag", offset: 0, len: tp::BAG_SIZE, owner_back: tp::HERO_BAG },
                    );
                    if ten {
                        w.open_at(
                            at,
                            Opener {
                                kind: "bagfilter",
                                offset: tp::BAG_FILTER_NEW,
                                len: tp::BAG_FILTER_LEN_NEW,
                                owner_back: tp::HERO_BAG,
                            },
                        );
                    }
                }
                lines.push(format!("bag: BagItem, {:#x} bytes at HERO+{:#x}", tp::BAG_SIZE, tp::HERO_BAG));
                if ten {
                    lines.push(format!(
                        "bagfilter: the moved filter array, {:#x} bytes at bag+{:#x}",
                        tp::BAG_FILTER_LEN_NEW,
                        tp::BAG_FILTER_NEW
                    ));
                }
            }
            // The list positions `crate::tenlists` widens: every PC that reads
            // one should be a seed of `tools_re/widen.py` (§65.11).
            "lists" => {
                const NPC: [&str; 6] = [
                    "_ZN14StateInNpcMenuC1Ei",
                    "_ZN14StateInNpcMenuC2Ei",
                    "_ZN14StateInNpcMenuC1Ev",
                    "_ZN14StateInNpcMenuC2Ev",
                    "_ZN14StateInNpcMenuC1Eias",
                    "_ZN14StateInNpcMenuC2Eias",
                ];
                const GAME: [&str; 4] = [
                    "_ZN15StateInGameMenuC1Eia",
                    "_ZN15StateInGameMenuC2Eia",
                    "_ZN15StateInGameMenuC1Ev",
                    "_ZN15StateInGameMenuC2Ev",
                ];
                for c in NPC {
                    let at = sym(vm, c)?;
                    w.open_at(at, Opener { kind: "npc+11", offset: 0x11, len: 1, owner_back: 0 });
                    w.open_at(at, Opener { kind: "npc+14", offset: 0x14, len: 1, owner_back: 0 });
                }
                for c in GAME {
                    let at = sym(vm, c)?;
                    w.open_at(at, Opener { kind: "game+f", offset: 0xf, len: 1, owner_back: 0 });
                    w.open_at(at, Opener { kind: "game+10", offset: 0x10, len: 1, owner_back: 0 });
                    w.open_at(at, Opener { kind: "game+12", offset: 0x12, len: 1, owner_back: 0 });
                }
                let gv = sym(vm, "gv")?;
                w.open_fixed(gv + 0x1b3e, 1, "gv+1b3e");
                lines.push("lists: StateInNpcMenu +0x11/+0x14, StateInGameMenu +0xf/+0x10/+0x12, gv+0x1B3E".into());
            }
            other => return Err(format!("--memwatch {other}: expected `storage`, `bag` or `lists`")),
        }
    }
    vm.memwatch = Some(Box::new(w));
    Ok(lines)
}

/// The report: one line per (kind, PC), in address order.
pub fn report(vm: &crate::Vm) -> Vec<String> {
    let Some(w) = vm.memwatch.as_deref() else { return Vec::new() };
    let mut out = Vec::new();
    for (k, n) in &w.opened {
        out.push(format!("{k}: {n} object(s) constructed"));
    }
    if vm.cfg.ten_pages {
        let bad = crate::tenpages::check_accesses(w);
        out.push(format!("ten-page layout check: {} finding(s)", bad.len()));
        for b in bad {
            out.push(format!("  {b}"));
        }
    }
    for ((kind, pc), h) in &w.hits {
        let off = pc.wrapping_sub(vm.elf.base);
        out.push(format!(
            "{kind:9} {off:#08x} {:<52} [{:#05x}..{:#05x}] r{} w{} {}",
            vm.describe(*pc),
            h.lo,
            h.hi,
            h.reads,
            h.writes,
            match h.widths {
                1 => "b",
                2 => "h",
                4 => "w",
                _ => "mixed",
            }
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_records_its_accessors_and_closes_on_delete() {
        let mut w = MemWatch::new();
        w.open_at(0x100, Opener { kind: "obj", offset: 8, len: 0x10, owner_back: 0 });
        w.close_at(0x200);
        w.at_pc(0x100, 0x1000); // ctor entered with this = 0x1000
        w.note(0x300, 0x1008, 4, true);
        w.note(0x300, 0x1014, 1, false);
        w.note(0x304, 0x1018, 4, false); // past the region
        w.note(0x308, 0x1004, 4, false); // before it
        let h = w.hits[&("obj", 0x300)];
        assert_eq!((h.lo, h.hi, h.reads, h.writes, h.widths), (0, 0xc, 1, 1, 5));
        assert_eq!(w.hits.len(), 1);
        w.at_pc(0x200, 0x1000); // operator delete(this)
        w.note(0x30c, 0x1008, 4, true);
        assert_eq!(w.hits.len(), 1, "a closed region records nothing");
        assert_eq!(w.opened["obj"], 1);
    }
}
