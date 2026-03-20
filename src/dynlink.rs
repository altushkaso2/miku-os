extern crate alloc;
use alloc::vec::Vec;

use crate::elf::*;
use crate::vmm::AddressSpace;
use crate::grub;

#[derive(Debug, Clone, Copy)]
pub enum RelocError {
    BadReloc,
    UndefinedSymbol,
    WriteError,
    UnknownType(u32),
}

pub struct DynInfo {
    pub rela_vaddr:      u64,
    pub rela_size:       u64,
    pub rela_ent:        u64,
    pub rel_vaddr:       u64,
    pub rel_size:        u64,
    pub rel_ent:         u64,
    pub plt_rela_vaddr:  u64,
    pub plt_rela_size:   u64,
    pub plt_is_rela:     bool,
    pub symtab_vaddr:    u64,
    pub strtab_vaddr:    u64,
    pub strtab_size:     u64,
    pub init_vaddr:      u64,
    pub fini_vaddr:      u64,
    pub gnu_hash_vaddr:  u64,
    pub flags_1:         u64,
    pub needed:          [u32; 32],
    pub needed_count:    usize,
}

impl DynInfo {
    pub fn empty() -> Self {
        Self {
            rela_vaddr: 0, rela_size: 0, rela_ent: 24,
            rel_vaddr: 0, rel_size: 0, rel_ent: 16,
            plt_rela_vaddr: 0, plt_rela_size: 0, plt_is_rela: true,
            symtab_vaddr: 0, strtab_vaddr: 0, strtab_size: 0,
            init_vaddr: 0, fini_vaddr: 0, gnu_hash_vaddr: 0,
            flags_1: 0,
            needed: [0u32; 32],
            needed_count: 0,
        }
    }
}

pub fn parse_dynamic(data: &[u8], info: &ElfInfo) -> DynInfo {
    let mut d = DynInfo::empty();
    let bias = info.load_bias;

    for i in 0..info.phdr_count {
        let ph = &info.phdrs[i];
        if ph.p_type != PT_DYNAMIC { continue; }

        let off = ph.p_offset as usize;
        let end = (off + ph.p_filesz as usize).min(data.len());
        let bytes = &data[off..end];
        let entry_size = core::mem::size_of::<Elf64Dyn>();
        let count = bytes.len() / entry_size;

        for j in 0..count {
            let dyn_entry = unsafe {
                core::ptr::read_unaligned(
                    bytes.as_ptr().add(j * entry_size) as *const Elf64Dyn
                )
            };
            match dyn_entry.d_tag {
                DT_RELA      => d.rela_vaddr    = dyn_entry.d_val.wrapping_sub(bias),
                DT_RELASZ    => d.rela_size      = dyn_entry.d_val,
                DT_RELAENT   => d.rela_ent       = dyn_entry.d_val,
                DT_REL       => d.rel_vaddr      = dyn_entry.d_val.wrapping_sub(bias),
                DT_RELSZ     => d.rel_size        = dyn_entry.d_val,
                DT_RELENT    => d.rel_ent         = dyn_entry.d_val,
                DT_JMPREL    => d.plt_rela_vaddr  = dyn_entry.d_val.wrapping_sub(bias),
                DT_PLTRELSZ  => d.plt_rela_size   = dyn_entry.d_val,
                DT_PLTREL    => d.plt_is_rela     = dyn_entry.d_val == DT_RELA as u64,
                DT_SYMTAB    => d.symtab_vaddr    = dyn_entry.d_val.wrapping_sub(bias),
                DT_STRTAB    => d.strtab_vaddr    = dyn_entry.d_val.wrapping_sub(bias),
                DT_STRSZ     => d.strtab_size     = dyn_entry.d_val,
                DT_INIT      => d.init_vaddr      = dyn_entry.d_val,
                DT_FINI      => d.fini_vaddr      = dyn_entry.d_val,
                DT_GNU_HASH  => d.gnu_hash_vaddr  = dyn_entry.d_val,
                DT_FLAGS_1   => d.flags_1         = dyn_entry.d_val,
                DT_NEEDED    => {
                    if d.needed_count < 32 {
                        d.needed[d.needed_count] = dyn_entry.d_val as u32;
                        d.needed_count += 1;
                    }
                }
                DT_NULL => break,
                _ => {}
            }
        }
        break;
    }

    d
}

unsafe fn uread64(aspace: &AddressSpace, uva: u64) -> Option<u64> {
    let hhdm = grub::hhdm();
    let phys = aspace.virt_to_phys(uva & !0xFFF)?;
    let off  = uva & 0xFFF;
    if off + 8 > 4096 {
        return None;
    }
    let ptr = (phys + hhdm + off) as *const u64;
    Some(ptr.read_unaligned())
}

unsafe fn uwrite64(aspace: &AddressSpace, uva: u64, val: u64) -> bool {
    let hhdm = grub::hhdm();
    let phys = match aspace.virt_to_phys(uva & !0xFFF) {
        Some(p) => p,
        None => return false,
    };
    let off = uva & 0xFFF;
    if off + 8 > 4096 {
        return false;
    }
    let ptr = (phys + hhdm + off) as *mut u64;
    ptr.write_unaligned(val);
    true
}

fn sym_value(
    data:          &[u8],
    sym_idx:       u32,
    load_bias:     u64,
    dyn_info:      &DynInfo,
) -> Option<u64> {
    let sym_size = core::mem::size_of::<Elf64Sym>();
    if dyn_info.symtab_vaddr == 0 { return None; }

    let sym_off  = dyn_info.symtab_vaddr as usize + sym_idx as usize * sym_size;
    if sym_off + sym_size > data.len() { return None; }

    let sym = unsafe {
        core::ptr::read_unaligned(data.as_ptr().add(sym_off) as *const Elf64Sym)
    };

    if sym.st_value == 0 { return None; }
    Some(sym.st_value + load_bias)
}

pub fn apply_rela_section(
    data:      &[u8],
    rela_off:  usize,
    rela_size: u64,
    load_bias: u64,
    aspace:    &AddressSpace,
    dyn_info:  &DynInfo,
) -> Result<(), RelocError> {
    let entry_size = core::mem::size_of::<Elf64Rela>();
    let count = rela_size as usize / entry_size;

    for i in 0..count {
        let off = rela_off + i * entry_size;
        if off + entry_size > data.len() { break; }

        let rela = unsafe {
            core::ptr::read_unaligned(data.as_ptr().add(off) as *const Elf64Rela)
        };

        let target_uva = rela.r_offset + load_bias;
        let rtype      = rela.rtype();
        let sym_idx    = rela.sym();
        let addend     = rela.r_addend;

        match rtype {
            R_X86_64_NONE => {}

            R_X86_64_RELATIVE => {
                let val = (load_bias as i64 + addend) as u64;
                if !unsafe { uwrite64(aspace, target_uva, val) } {
                    crate::serial_println!(
                        "[dynlink] R_RELATIVE write fail at {:#x}", target_uva
                    );
                    return Err(RelocError::WriteError);
                }
            }

            R_X86_64_64 => {
                let base = sym_value(data, sym_idx, load_bias, dyn_info)
                    .unwrap_or(0);
                let val  = (base as i64 + addend) as u64;
                if !unsafe { uwrite64(aspace, target_uva, val) } {
                    return Err(RelocError::WriteError);
                }
            }

            R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
                if let Some(sv) = sym_value(data, sym_idx, load_bias, dyn_info) {
                    if !unsafe { uwrite64(aspace, target_uva, sv) } {
                        return Err(RelocError::WriteError);
                    }
                }
            }

            R_X86_64_IRELATIVE => {
                crate::serial_println!(
                    "[dynlink] R_IRELATIVE at {:#x} (skipped - handled by binary)", target_uva
                );
            }

            R_X86_64_COPY => {}

            other => {
                crate::serial_println!("[dynlink] unknown reloc type {}", other);
            }
        }
    }

    Ok(())
}

pub fn apply_rel_section(
    data:     &[u8],
    rel_off:  usize,
    rel_size: u64,
    load_bias: u64,
    aspace:   &AddressSpace,
    dyn_info: &DynInfo,
) -> Result<(), RelocError> {
    let entry_size = core::mem::size_of::<Elf64Rel>();
    let count = rel_size as usize / entry_size;

    for i in 0..count {
        let off = rel_off + i * entry_size;
        if off + entry_size > data.len() { break; }

        let rel = unsafe {
            core::ptr::read_unaligned(data.as_ptr().add(off) as *const Elf64Rel)
        };

        let target_uva = rel.r_offset + load_bias;
        let rtype      = rel.rtype();
        let sym_idx    = rel.sym();

        match rtype {
            R_X86_64_NONE     => {}
            R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
                if let Some(sv) = sym_value(data, sym_idx, load_bias, dyn_info) {
                    unsafe { uwrite64(aspace, target_uva, sv); }
                }
            }
            R_X86_64_RELATIVE => {
                let addend = unsafe { uread64(aspace, target_uva) }.unwrap_or(0);
                let val    = load_bias.wrapping_add(addend);
                unsafe { uwrite64(aspace, target_uva, val); }
            }
            _ => {}
        }
    }

    Ok(())
}

pub fn apply_all_relocations(
    data:      &[u8],
    load_bias: u64,
    aspace:    &AddressSpace,
    dyn_info:  &DynInfo,
) -> Result<(), RelocError> {
    if dyn_info.rela_size > 0 && dyn_info.rela_vaddr < data.len() as u64 {
        apply_rela_section(
            data,
            dyn_info.rela_vaddr as usize,
            dyn_info.rela_size,
            load_bias,
            aspace,
            dyn_info,
        )?;
    }

    if dyn_info.rel_size > 0 && dyn_info.rel_vaddr < data.len() as u64 {
        apply_rel_section(
            data,
            dyn_info.rel_vaddr as usize,
            dyn_info.rel_size,
            load_bias,
            aspace,
            dyn_info,
        )?;
    }

    if dyn_info.plt_rela_size > 0 && dyn_info.plt_rela_vaddr < data.len() as u64 {
        if dyn_info.plt_is_rela {
            apply_rela_section(
                data,
                dyn_info.plt_rela_vaddr as usize,
                dyn_info.plt_rela_size,
                load_bias,
                aspace,
                dyn_info,
            )?;
        } else {
            apply_rel_section(
                data,
                dyn_info.plt_rela_vaddr as usize,
                dyn_info.plt_rela_size,
                load_bias,
                aspace,
                dyn_info,
            )?;
        }
    }

    Ok(())
}

pub fn get_needed_names<'a>(data: &'a [u8], dyn_info: &DynInfo) -> [Option<&'a str>; 32] {
    let mut result = [None; 32];
    if dyn_info.strtab_vaddr == 0 || dyn_info.strtab_size == 0 { return result; }

    let strtab_off  = dyn_info.strtab_vaddr as usize;
    let strtab_end  = (strtab_off + dyn_info.strtab_size as usize).min(data.len());
    if strtab_off >= data.len() { return result; }
    let strtab = &data[strtab_off..strtab_end];

    for i in 0..dyn_info.needed_count {
        let str_off = dyn_info.needed[i] as usize;
        if str_off >= strtab.len() { continue; }
        let nul = strtab[str_off..].iter().position(|&b| b == 0)
            .unwrap_or(strtab.len() - str_off);
        result[i] = core::str::from_utf8(&strtab[str_off..str_off + nul]).ok();
    }

    result
}
