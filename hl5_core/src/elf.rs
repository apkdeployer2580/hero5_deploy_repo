//! A minimal ELF32 ARM shared-object loader.
//!
//! `libHeroesLore5.so` is nearly as simple as a `.so` gets: `PT_LOAD` segments,
//! `DT_SYMBOLIC`, and four relocation types across the whole image (1540
//! `R_ARM_RELATIVE`, 75 `R_ARM_JUMP_SLOT`, 2 `R_ARM_ABS32`, 1
//! `R_ARM_GLOB_DAT`). There is no `PT_TLS` and no `DT_INIT`/`DT_FINI`.
//!
//! Two things here that the sibling chronosSword loader did not need:
//!
//! * **`.init_array` / `.fini_array`** — one entry each,
//!   `_GLOBAL__I_._code_Heroslore5.cpp` and `_GLOBAL__D_._code_Heroslore5.cpp`.
//!   This is a C++ guest and its static constructors must run before the entry
//!   point. The array *contents* are themselves `R_ARM_RELATIVE` targets, so
//!   they are only correct after relocation — this module records where the
//!   array is and leaves reading it to the caller (see `Vm::run_init_array`).
//!
//! * **`R_ARM_ABS32`** — both of them land in `.text`, which is what the
//!   `DT_TEXTREL` flag is announcing. They are the literal-pool words holding
//!   `__cxa_call_unexpected` and `__gnu_Unwind_Find_exidx` inside the unwinder.
//!   `DT_TEXTREL` costs us nothing: guest memory is one flat byte array with no
//!   page protection, so a relocation into the text segment is an ordinary
//!   store. It matters only to a loader that would have had to `mprotect`.

use crate::mem::GuestMem;
use std::collections::HashMap;

pub const R_ARM_ABS32: u32 = 2;
pub const R_ARM_GLOB_DAT: u32 = 21;
pub const R_ARM_JUMP_SLOT: u32 = 22;
pub const R_ARM_RELATIVE: u32 = 23;

pub struct Elf {
    /// Guest address the image was placed at.
    pub base: u32,
    /// Every defined symbol, by name, already rebased. Built from `.symtab`
    /// when present (this binary is not stripped, which is what makes the
    /// runtime debuggable) and from `.dynsym` otherwise.
    pub symbols: HashMap<String, u32>,
    /// `st_size` for the symbols that declare one, by name. Kept because "the
    /// next symbol along" is *not* a safe stand-in for a function's end. A hook
    /// asking "is this return address inside function F" has to ask the size.
    pub sizes: HashMap<String, u32>,
    /// Undefined symbols this image imports, in `.dynsym` index order.
    /// Relocations name their symbol by that index, so the order is load-bearing.
    pub imports: Vec<String>,
    /// `PT_ARM_EXIDX` (address, byte length), needed by
    /// `__gnu_Unwind_Find_exidx`.
    pub exidx: (u32, u32),
    /// `.init_array` / `.fini_array` as (guest address, entry count). The
    /// entries hold function pointers *after* relocation.
    pub init_array: (u32, u32),
    pub fini_array: (u32, u32),
    pub image_end: u32,
}

struct Sec {
    name: String,
    sh_type: u32,
    addr: u32,
    offset: u32,
    size: u32,
    link: u32,
    entsize: u32,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn cstr_at(b: &[u8], o: usize) -> String {
    let end = b[o..].iter().position(|&c| c == 0).map(|e| o + e).unwrap_or(b.len());
    String::from_utf8_lossy(&b[o..end]).into_owned()
}

/// Load `data` into `mem` at `base` and apply its relocations.
///
/// `resolve` maps an imported symbol name to the guest address that should be
/// written into its slot. Returning `None` is not "skip": the name is collected
/// and the load fails with the list. A slot left at zero shows up much later as
/// a branch to address 0, a hundred thousand instructions from the cause.
pub fn load(
    data: &[u8],
    base: u32,
    mem: &mut GuestMem,
    mut resolve: impl FnMut(&str) -> Option<u32>,
) -> Result<Elf, String> {
    if data.len() < 52 || &data[0..4] != b"\x7fELF" {
        return Err("not an ELF".into());
    }
    if data[4] != 1 || data[5] != 1 {
        return Err("not 32-bit little-endian".into());
    }
    if u16le(data, 18) != 40 {
        return Err("not EM_ARM".into());
    }

    let phoff = u32le(data, 28) as usize;
    let shoff = u32le(data, 32) as usize;
    let phentsize = u16le(data, 42) as usize;
    let phnum = u16le(data, 44) as usize;
    let shentsize = u16le(data, 46) as usize;
    let shnum = u16le(data, 48) as usize;
    let shstrndx = u16le(data, 50) as usize;

    // ---- segments -------------------------------------------------------
    let mut image_end = base;
    let mut exidx = (0u32, 0u32);
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        let p_type = u32le(data, p);
        let p_offset = u32le(data, p + 4) as usize;
        let p_vaddr = u32le(data, p + 8);
        let p_filesz = u32le(data, p + 16) as usize;
        let p_memsz = u32le(data, p + 20);

        match p_type {
            1 => {
                // PT_LOAD. `.bss` is the memsz beyond filesz; guest RAM starts
                // zeroed, so only the file-backed part needs copying.
                let end = p_offset.checked_add(p_filesz).ok_or("PT_LOAD overflows")?;
                let src = data.get(p_offset..end).ok_or("PT_LOAD past end of file")?;
                mem.write_bytes(base + p_vaddr, src);
                image_end = image_end.max(base + p_vaddr + p_memsz);
            }
            0x7000_0001 => exidx = (base + p_vaddr, p_memsz), // PT_ARM_EXIDX
            _ => {}
        }
    }

    // ---- sections -------------------------------------------------------
    let mut secs: Vec<Sec> = Vec::with_capacity(shnum);
    for i in 0..shnum {
        // Elf32_Shdr: name 0, type 4, flags 8, addr 12, offset 16, size 20,
        // link 24, info 28, addralign 32, entsize 36.
        let s = shoff + i * shentsize;
        secs.push(Sec {
            name: String::new(),
            sh_type: u32le(data, s + 4),
            addr: u32le(data, s + 12),
            offset: u32le(data, s + 16),
            size: u32le(data, s + 20),
            link: u32le(data, s + 24),
            entsize: u32le(data, s + 36),
        });
    }
    let shstr = secs[shstrndx].offset as usize;
    for i in 0..shnum {
        let s = shoff + i * shentsize;
        secs[i].name = cstr_at(data, shstr + u32le(data, s) as usize);
    }
    let find = |n: &str| secs.iter().position(|s| s.name == n);

    // The C++ static-constructor and destructor arrays. Their *contents* are
    // relocated below, so only the location is useful here.
    let array_of = |n: &str| -> (u32, u32) {
        find(n).map(|i| (base + secs[i].addr, secs[i].size / 4)).unwrap_or((0, 0))
    };
    let init_array = array_of(".init_array");
    let fini_array = array_of(".fini_array");

    // ---- symbols --------------------------------------------------------
    let mut symbols = HashMap::new();
    let mut sizes = HashMap::new();
    let mut imports = Vec::new();

    let read_symtab = |sec: &Sec,
                       secs: &[Sec],
                       out: &mut HashMap<String, u32>,
                       sizes: &mut HashMap<String, u32>| {
        let strtab = secs[sec.link as usize].offset as usize;
        let n = (sec.size / sec.entsize.max(16)) as usize;
        for i in 0..n {
            let e = sec.offset as usize + i * 16;
            let name = cstr_at(data, strtab + u32le(data, e) as usize);
            let value = u32le(data, e + 4);
            let size = u32le(data, e + 8);
            let shndx = u16le(data, e + 14);
            if name.is_empty() || shndx == 0 || out.contains_key(&name) {
                continue;
            }
            out.insert(name.clone(), base + value);
            // First definition wins for both maps, so they stay in step: a size
            // must belong to the address stored beside it.
            if size != 0 {
                sizes.insert(name, size);
            }
        }
    };

    if let Some(i) = find(".symtab") {
        read_symtab(&secs[i], &secs, &mut symbols, &mut sizes);
    }
    let dynsym_idx = find(".dynsym").ok_or("no .dynsym")?;
    read_symtab(&secs[dynsym_idx], &secs, &mut symbols, &mut sizes);

    // The import list must stay in `.dynsym` index order: relocations name
    // their symbol by that index. Defined symbols get an empty string so the
    // indices still line up.
    {
        let sec = &secs[dynsym_idx];
        let strtab = secs[sec.link as usize].offset as usize;
        let n = (sec.size / 16) as usize;
        for i in 0..n {
            let e = sec.offset as usize + i * 16;
            let name = cstr_at(data, strtab + u32le(data, e) as usize);
            let shndx = u16le(data, e + 14);
            imports.push(if shndx == 0 { name } else { String::new() });
        }
    }

    // ---- relocations ----------------------------------------------------
    let mut unresolved: Vec<String> = Vec::new();
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for sec in secs.iter().filter(|s| s.sh_type == 9 /* SHT_REL */) {
        let n = (sec.size / 8) as usize;
        for i in 0..n {
            let e = sec.offset as usize + i * 8;
            let r_offset = u32le(data, e);
            let r_info = u32le(data, e + 4);
            let sym = (r_info >> 8) as usize;
            let kind = r_info & 0xff;
            let target = base + r_offset;
            *counts.entry(kind).or_insert(0) += 1;

            match kind {
                R_ARM_RELATIVE => {
                    // B(S) + A, where A is the addend already stored at the
                    // target. This is what rebases `.init_array`'s contents.
                    let addend = mem.read_u32(target);
                    mem.write_u32(target, base.wrapping_add(addend));
                }
                R_ARM_GLOB_DAT | R_ARM_JUMP_SLOT | R_ARM_ABS32 => {
                    // GLOB_DAT and JUMP_SLOT are `S` — the addend a REL entry
                    // carries is meaningless for them (every JUMP_SLOT here has
                    // the PLT base sitting in its slot, the lazy-binding stub
                    // address, which must not be added). ABS32 is `S + A`.
                    let addend =
                        if kind == R_ARM_ABS32 { mem.read_u32(target) } else { 0 };
                    let name = imports.get(sym).cloned().unwrap_or_default();
                    let value = if name.is_empty() {
                        // Defined in this image: DT_SYMBOLIC binds it here.
                        Some(symbol_value(data, &secs, dynsym_idx, sym, base))
                    } else {
                        // A symbol can be both imported here and defined here;
                        // ask the host first, then fall back to our own table.
                        resolve(&name).or_else(|| symbols.get(&name).copied())
                    };
                    match value {
                        Some(v) => mem.write_u32(target, v.wrapping_add(addend)),
                        None => {
                            mem.write_u32(target, 0);
                            if !unresolved.contains(&name) {
                                unresolved.push(name);
                            }
                        }
                    }
                }
                other => {
                    return Err(format!(
                        "unsupported relocation type {other} at {target:#010x} in {}",
                        sec.name
                    ))
                }
            }
        }
    }

    if !unresolved.is_empty() {
        return Err(format!("unresolved imports: {}", unresolved.join(", ")));
    }

    Ok(Elf { base, symbols, sizes, imports, exidx, init_array, fini_array, image_end })
}

fn symbol_value(data: &[u8], secs: &[Sec], dynsym: usize, idx: usize, base: u32) -> u32 {
    let sec = &secs[dynsym];
    let e = sec.offset as usize + idx * 16;
    base + u32le(data, e + 4)
}
