//! Guest heap.
//!
//! Block metadata is kept host-side rather than in headers inside the arena.
//! That costs a hash lookup on `free`, but it means a guest buffer overrun
//! corrupts only guest data instead of the allocator's own bookkeeping — which
//! is the difference between a visible glitch and an unexplainable hang, and
//! this guest is 1.17 MB of code we cannot patch.
//!
//! Best-fit with full coalescing: `free_by_addr` answers "who is my neighbour"
//! for merging, `free_by_size` answers "smallest block that fits" for
//! allocation. Both are ordered maps, so every operation is O(log n).

use std::collections::{BTreeMap, BTreeSet, HashMap};

const ALIGN: u32 = 8;

pub struct Heap {
    size: u32,
    free_by_addr: BTreeMap<u32, u32>,
    free_by_size: BTreeSet<(u32, u32)>,
    live: HashMap<u32, u32>,
    pub live_bytes: u32,
    pub peak_bytes: u32,
}

impl Heap {
    pub fn new(base: u32, size: u32) -> Self {
        let mut h = Self {
            size,
            free_by_addr: BTreeMap::new(),
            free_by_size: BTreeSet::new(),
            live: HashMap::new(),
            live_bytes: 0,
            peak_bytes: 0,
        };
        h.insert_free(base, size);
        h
    }

    fn insert_free(&mut self, addr: u32, size: u32) {
        self.free_by_addr.insert(addr, size);
        self.free_by_size.insert((size, addr));
    }

    fn remove_free(&mut self, addr: u32, size: u32) {
        self.free_by_addr.remove(&addr);
        self.free_by_size.remove(&(size, addr));
    }

    pub fn total(&self) -> u32 {
        self.size
    }

    pub fn free_bytes(&self) -> u32 {
        self.size - self.live_bytes
    }

    pub fn alloc(&mut self, want: u32) -> Option<u32> {
        let need = want.max(1).div_ceil(ALIGN) * ALIGN;
        let (bsize, baddr) = *self.free_by_size.range((need, 0)..).next()?;
        self.remove_free(baddr, bsize);
        if bsize > need {
            self.insert_free(baddr + need, bsize - need);
        }
        self.live.insert(baddr, need);
        self.live_bytes += need;
        self.peak_bytes = self.peak_bytes.max(self.live_bytes);
        Some(baddr)
    }

    /// Live allocations, largest first. Diagnostic: pairing a block's size with
    /// how much of it is non-zero is how you tell "the game decoded an image
    /// into this" from "the game allocated a buffer and nothing filled it".
    pub fn live_blocks(&self) -> Vec<(u32, u32)> {
        let mut v: Vec<(u32, u32)> = self.live.iter().map(|(&a, &s)| (a, s)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    }

    /// Size of a live block, for `realloc` and for bounds-checking a `free`.
    pub fn size_of(&self, addr: u32) -> Option<u32> {
        self.live.get(&addr).copied()
    }

    pub fn free(&mut self, addr: u32) -> bool {
        let Some(size) = self.live.remove(&addr) else {
            return false;
        };
        self.live_bytes -= size;

        let mut start = addr;
        let mut end = addr + size;

        // Merge with the block immediately before, if it ends where we start.
        if let Some((&paddr, &psize)) = self.free_by_addr.range(..start).next_back() {
            if paddr + psize == start {
                self.remove_free(paddr, psize);
                start = paddr;
            }
        }
        // ...and the one immediately after.
        if let Some((&naddr, &nsize)) = self.free_by_addr.range(end..).next() {
            if naddr == end {
                self.remove_free(naddr, nsize);
                end = naddr + nsize;
            }
        }
        self.insert_free(start, end - start);
        true
    }
}
