use crate::elf::*;
use crate::syscall;
use crate::util;
use crate::symtab;

const MAX_LIBS:   usize = 16;
const MAX_NEEDED: usize = 32;
const MAX_PATH:   usize = 256;

struct LoadedLib {
    soname: [u8; 64],
    active: bool,
}

static mut LIBS:       [LoadedLib; MAX_LIBS] = {
    const E: LoadedLib = LoadedLib { soname: [0u8; 64], active: false };
    [E; MAX_LIBS]
};
static mut LIBS_COUNT: usize = 0;

fn is_loaded(name: &[u8]) -> bool {
    unsafe {
        for i in 0..LIBS_COUNT {
            let lib = &LIBS[i];
            if !lib.active { continue; }
            let slen = lib.soname.iter().position(|&b| b == 0).unwrap_or(64);
            if util::bytes_eq(&lib.soname[..slen], name) { return true; }
        }
    }
    false
}

fn register_lib(name: &[u8]) {
    unsafe {
        if LIBS_COUNT >= MAX_LIBS { return; }
        let lib = &mut LIBS[LIBS_COUNT];
        let copy_len = name.len().min(63);
        lib.soname[..copy_len].copy_from_slice(&name[..copy_len]);
        lib.soname[copy_len] = 0;
        lib.active = true;
        LIBS_COUNT += 1;
    }
}

pub struct DynInfo {
    pub rela_va:        u64,
    pub rela_sz:        u64,
    pub jmprel_va:      u64,
    pub jmprel_sz:      u64,
    pub strtab_va:      u64,
    pub symtab_va:      u64,
    pub syment:         u64,
    pub init_fn:        u64,
    pub init_array_va:  u64,
    pub init_array_sz:  u64,
    pub needed:         [[u8; 64]; MAX_NEEDED],
    pub needed_count:   usize,
}

impl DynInfo {
    fn zero() -> Self {
        Self {
            rela_va:       0,
            rela_sz:       0,
            jmprel_va:     0,
            jmprel_sz:     0,
            strtab_va:     0,
            symtab_va:     0,
            syment:        24,
            init_fn:       0,
            init_array_va: 0,
            init_array_sz: 0,
            needed:        [[0u8; 64]; MAX_NEEDED],
            needed_count:  0,
        }
    }
}

unsafe fn read_u16(p: *const u8) -> u16 {
    core::ptr::read_unaligned(p as *const u16)
}
unsafe fn read_u32(p: *const u8) -> u32 {
    core::ptr::read_unaligned(p as *const u32)
}
unsafe fn read_u64(p: *const u8) -> u64 {
    core::ptr::read_unaligned(p as *const u64)
}
unsafe fn read_i64(p: *const u8) -> i64 {
    core::ptr::read_unaligned(p as *const i64)
}

pub fn parse_dynamic(base: u64, phdrs_va: u64, phnum: u16, phent: u16) -> DynInfo {
    let mut di = DynInfo::zero();

    for i in 0..phnum as u64 {
        let ph_ptr  = (phdrs_va + i * phent as u64) as *const u8;
        let p_type  = unsafe { read_u32(ph_ptr) };
        if p_type != PT_DYNAMIC { continue; }

        let p_vaddr  = unsafe { read_u64(ph_ptr.add(16)) };
        let p_filesz = unsafe { read_u64(ph_ptr.add(32)) };
        let dyn_base = base + p_vaddr;
        let count    = p_filesz / 16;

        for j in 0..count {
            let d   = (dyn_base + j * 16) as *const u8;
            let tag = unsafe { read_i64(d) };
            let val = unsafe { read_u64(d.add(8)) };
            match tag {
                DT_STRTAB       => di.strtab_va      = base + val,
                DT_SYMTAB       => di.symtab_va      = base + val,
                DT_SYMENT       => di.syment         = val,
                DT_RELA         => di.rela_va        = base + val,
                DT_RELASZ       => di.rela_sz        = val,
                DT_JMPREL       => di.jmprel_va      = base + val,
                DT_PLTRELSZ     => di.jmprel_sz      = val,
                DT_INIT         => di.init_fn        = base + val,
                DT_INIT_ARRAY   => di.init_array_va  = base + val,
                DT_INIT_ARRAYSZ => di.init_array_sz  = val,
                DT_NULL         => break,
                _               => {}
            }
        }

        if di.strtab_va != 0 {
            for j in 0..count {
                let d   = (dyn_base + j * 16) as *const u8;
                let tag = unsafe { read_i64(d) };
                let val = unsafe { read_u64(d.add(8)) };
                match tag {
                    DT_NEEDED => {
                        if di.needed_count < MAX_NEEDED {
                            let name_ptr   = (di.strtab_va + val) as *const u8;
                            let name_bytes = util::cstr_to_bytes(name_ptr);
                            let copy_len   = name_bytes.len().min(63);
                            di.needed[di.needed_count][..copy_len]
                                .copy_from_slice(&name_bytes[..copy_len]);
                            di.needed_count += 1;
                        }
                    }
                    DT_NULL => break,
                    _       => {}
                }
            }
        }
        break;
    }

    di
}

pub fn export_symbols(base: u64, di: &DynInfo) {
    if di.symtab_va == 0 || di.strtab_va == 0 { return; }

    let sym_end = if di.strtab_va > di.symtab_va {
        di.strtab_va
    } else {
        return;
    };

    let mut sym_va = di.symtab_va;
    while sym_va + di.syment <= sym_end {
        let s = sym_va as *const u8;
        let st_name  = unsafe { read_u32(s) };
        let st_info  = unsafe { *s.add(4) };
        let st_shndx = unsafe { read_u16(s.add(6)) };
        let st_value = unsafe { read_u64(s.add(8)) };

        let bind = st_info >> 4;
        if st_shndx != SHN_UNDEF
            && (bind == STB_GLOBAL || bind == STB_WEAK)
            && st_value != 0
        {
            let name_ptr = (di.strtab_va + st_name as u64) as *const u8;
            symtab::export(name_ptr, base + st_value, bind == STB_WEAK);
        }
        sym_va += di.syment;
    }
}

pub fn apply_relocations(base: u64, rela_va: u64, rela_sz: u64, di: &DynInfo) {
    if rela_va == 0 || rela_sz == 0 { return; }

    let count = rela_sz / 24;
    for i in 0..count {
        let r = (rela_va + i * 24) as *const u8;
        let r_offset = unsafe { read_u64(r) };
        let r_info   = unsafe { read_u64(r.add(8)) };
        let r_addend = unsafe { read_i64(r.add(16)) };

        let rtype  = r_info as u32;
        let rsym   = (r_info >> 32) as u32;
        let target = (base + r_offset) as *mut u64;

        if rtype == R_X86_64_RELATIVE {
            unsafe { target.write_unaligned((base as i64 + r_addend) as u64); }
            continue;
        }

        let mut sym_val = 0u64;
        if rsym != 0 && di.symtab_va != 0 {
            let s = (di.symtab_va + rsym as u64 * di.syment) as *const u8;
            let st_shndx = unsafe { read_u16(s.add(6)) };
            let st_value = unsafe { read_u64(s.add(8)) };
            let st_name  = unsafe { read_u32(s) };

            if st_shndx != SHN_UNDEF {
                sym_val = base + st_value;
            } else {
                let name_ptr = (di.strtab_va + st_name as u64) as *const u8;
                sym_val = symtab::lookup(name_ptr);
                if sym_val == 0 {
                    util::print(b"[ld-miku] unresolved symbol: ");
                    util::println(util::cstr_to_bytes(name_ptr));
                }
            }
        }

        unsafe {
            match rtype {
                R_X86_64_64 =>
                    target.write_unaligned(sym_val.wrapping_add(r_addend as u64)),
                R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT =>
                    target.write_unaligned(sym_val),
                R_X86_64_COPY => {
                    let s = (di.symtab_va + rsym as u64 * di.syment) as *const u8;
                    let st_size = read_u64(s.add(16));
                    if sym_val != 0 && st_size > 0 {
                        util::memcpy(target as *mut u8, sym_val as *const u8, st_size as usize);
                    }
                }
                R_X86_64_NONE => {}
                _ => {}
            }
        }
    }
}

pub fn apply_relro(base: u64, phdrs_va: u64, phnum: u16, phent: u16) {
    for i in 0..phnum as u64 {
        let ph = (phdrs_va + i * phent as u64) as *const u8;
        let p_type  = unsafe { read_u32(ph) };
        let p_vaddr = unsafe { read_u64(ph.add(16)) };
        let p_memsz = unsafe { read_u64(ph.add(40)) };
        if p_type != PT_GNU_RELRO || p_memsz == 0 { continue; }
        let start = util::page_align_down(base + p_vaddr);
        let end   = util::page_align_up(base + p_vaddr + p_memsz);
        syscall::mprotect(start, (end - start) as usize, syscall::PROT_READ);
        break;
    }
}

fn open_lib(soname: &[u8]) -> i64 {
    let prefixes: [&[u8]; 2] = [b"/lib/", b"/usr/lib/"];
    for prefix in &prefixes {
        let mut path = [0u8; MAX_PATH];
        let plen = prefix.len();
        let nlen = soname.len().min(MAX_PATH - plen - 1);
        path[..plen].copy_from_slice(prefix);
        path[plen..plen + nlen].copy_from_slice(&soname[..nlen]);
        let fd = syscall::open(&path[..plen + nlen]);
        if fd >= 0 { return fd; }
    }
    -1
}

pub fn load_library(soname: &[u8]) {
    let name_trimmed = {
        let end = soname.iter().position(|&b| b == 0).unwrap_or(soname.len());
        &soname[..end]
    };
    if name_trimmed.is_empty() || is_loaded(name_trimmed) { return; }

    let base = syscall::map_lib(name_trimmed);
    if base > 0 && (base as i64) > 0 {
        register_lib(name_trimmed);

        let eh_ptr      = base as *const u8;
        let e_phoff     = unsafe { read_u64(eh_ptr.add(32)) };
        let e_phnum     = unsafe { read_u16(eh_ptr.add(56)) };
        let e_phentsize = unsafe { read_u16(eh_ptr.add(54)) };
        let phdrs_va    = base as u64 + e_phoff;

        let di = parse_dynamic(base as u64, phdrs_va, e_phnum, e_phentsize);

        for i in 0..di.needed_count {
            load_library(&di.needed[i]);
        }

        apply_relocations(base as u64, di.rela_va,   di.rela_sz,   &di);
        export_symbols(base as u64, &di);
        apply_relocations(base as u64, di.jmprel_va, di.jmprel_sz, &di);
        apply_relro(base as u64, phdrs_va, e_phnum, e_phentsize);

        util::print(b"[ld-miku] shared: ");
        util::print(name_trimmed);
        util::print(b" @ ");
        util::print_hex(base as u64);
        util::println(b"");
        return;
    }

    let fd = open_lib(name_trimmed);
    if fd < 0 {
        util::print(b"[ld-miku] not found: ");
        util::println(name_trimmed);
        return;
    }

    let fsize = syscall::fsize(fd);
    if fsize <= 0 || fsize > 64 * 1024 * 1024 {
        syscall::close(fd);
        return;
    }

    let buf = syscall::mmap(0, fsize as usize, syscall::PROT_READ | syscall::PROT_WRITE);
    if buf.is_null() {
        syscall::close(fd);
        util::println(b"[ld-miku] mmap failed for lib");
        return;
    }

    syscall::seek(fd, 0);
    let mut done = 0usize;
    while done < fsize as usize {
        let n = syscall::read(fd as u64, unsafe { buf.add(done) }, fsize as usize - done);
        if n <= 0 { break; }
        done += n as usize;
    }
    syscall::close(fd);

    let e_phoff     = unsafe { read_u64(buf.add(32)) } as usize;
    let e_phnum     = unsafe { read_u16(buf.add(56)) };
    let e_phentsize = unsafe { read_u16(buf.add(54)) };

    let mut phdr_copy = [0u8; 64 * 16];
    let phdr_bytes = (e_phnum as usize) * (e_phentsize as usize);
    let copy_len = phdr_bytes.min(phdr_copy.len());
    if e_phoff + copy_len <= fsize as usize {
        util::memcpy(phdr_copy.as_mut_ptr(), unsafe { buf.add(e_phoff) }, copy_len);
    }

    let base = map_elf_segments(buf, fsize as usize);
    syscall::munmap(buf, fsize as usize);

    if base == 0 {
        util::print(b"[ld-miku] failed to map: ");
        util::println(name_trimmed);
        return;
    }

    register_lib(name_trimmed);

    let phdrs_va = phdr_copy.as_ptr() as u64;

    let di = parse_dynamic(base, phdrs_va, e_phnum, e_phentsize);

    for i in 0..di.needed_count {
        load_library(&di.needed[i]);
    }

    apply_relocations(base, di.rela_va,    di.rela_sz,    &di);
    export_symbols(base, &di);
    apply_relocations(base, di.jmprel_va,  di.jmprel_sz,  &di);
    apply_relro(base, phdrs_va, e_phnum, e_phentsize);

    util::print(b"[ld-miku] loaded: ");
    util::print(name_trimmed);
    util::print(b" @ ");
    util::print_hex(base);
    util::println(b"");
}

fn map_elf_segments(buf: *const u8, buf_len: usize) -> u64 {
    if buf_len < 64 { return 0; }

    let magic = unsafe { core::slice::from_raw_parts(buf, 4) };
    if magic != ELF_MAGIC { return 0; }

    let e_type     = unsafe { read_u16(buf.add(16)) };
    let e_phoff    = unsafe { read_u64(buf.add(32)) } as usize;
    let e_phnum    = unsafe { read_u16(buf.add(56)) } as usize;
    let e_phentsize = unsafe { read_u16(buf.add(54)) } as usize;

    if e_type != ET_DYN { return 0; }

    let mut lo = u64::MAX;
    let mut hi = 0u64;

    for i in 0..e_phnum {
        let ph = unsafe { buf.add(e_phoff + i * e_phentsize) };
        let p_type  = unsafe { read_u32(ph) };
        let p_vaddr = unsafe { read_u64(ph.add(16)) };
        let p_memsz = unsafe { read_u64(ph.add(40)) };
        if p_type != PT_LOAD || p_memsz == 0 { continue; }
        if p_vaddr < lo { lo = p_vaddr; }
        let end = p_vaddr + p_memsz;
        if end > hi { hi = end; }
    }
    if lo == u64::MAX || hi == 0 { return 0; }

    lo &= !0xFFF;
    hi  = util::page_align_up(hi);
    let map_size = hi - lo;

    let base_ptr = syscall::mmap(0, map_size as usize, syscall::PROT_READ | syscall::PROT_WRITE);
    if base_ptr.is_null() { return 0; }
    let base = base_ptr as u64 - lo;

    for i in 0..e_phnum {
        let ph = unsafe { buf.add(e_phoff + i * e_phentsize) };
        let p_type   = unsafe { read_u32(ph) };
        let p_flags  = unsafe { read_u32(ph.add(4)) };
        let p_offset = unsafe { read_u64(ph.add(8)) } as usize;
        let p_vaddr  = unsafe { read_u64(ph.add(16)) };
        let p_filesz = unsafe { read_u64(ph.add(32)) } as usize;
        let p_memsz  = unsafe { read_u64(ph.add(40)) } as usize;

        if p_type != PT_LOAD || p_memsz == 0 { continue; }

        let dst = (base + p_vaddr) as *mut u8;

        if p_filesz > 0 && p_offset + p_filesz <= buf_len {
            util::memcpy(dst, unsafe { buf.add(p_offset) }, p_filesz);
        }
        if p_memsz > p_filesz {
            util::memset(unsafe { dst.add(p_filesz) }, 0, p_memsz - p_filesz);
        }

        let mut prot = 0u64;
        if p_flags & PF_R != 0 { prot |= syscall::PROT_READ; }
        if p_flags & PF_W != 0 { prot |= syscall::PROT_WRITE; }
        if p_flags & PF_X != 0 { prot |= syscall::PROT_EXEC; }
        let pstart = util::page_align_down(base + p_vaddr);
        let pend   = util::page_align_up(base + p_vaddr + p_memsz as u64);
        syscall::mprotect(pstart, (pend - pstart) as usize, prot);
    }

    base
}

pub fn call_init(di: &DynInfo) {
    if di.init_fn != 0 {
        let f: fn() = unsafe { core::mem::transmute(di.init_fn) };
        f();
    }
    if di.init_array_va != 0 && di.init_array_sz != 0 {
        let count = di.init_array_sz / 8;
        for i in 0..count {
            let fn_ptr_va = (di.init_array_va + i * 8) as *const u64;
            let fn_addr   = unsafe { fn_ptr_va.read_unaligned() };
            if fn_addr != 0 && fn_addr != u64::MAX {
                let f: fn() = unsafe { core::mem::transmute(fn_addr) };
                f();
            }
        }
    }
}

pub fn setup_tls(base: u64, phdrs_va: u64, phnum: u16, phent: u16) -> u64 {
    for i in 0..phnum as u64 {
        let ph = (phdrs_va + i * phent as u64) as *const u8;
        let p_type   = unsafe { read_u32(ph) };
        let p_offset = unsafe { read_u64(ph.add(8)) };
        let p_vaddr  = unsafe { read_u64(ph.add(16)) };
        let p_filesz = unsafe { read_u64(ph.add(32)) };
        let p_memsz  = unsafe { read_u64(ph.add(40)) };
        let p_align  = unsafe { read_u64(ph.add(48)) };

        if p_type != PT_TLS || p_memsz == 0 { continue; }
        let _ = p_offset;

        let align   = p_align.max(8) as usize;
        let memsz   = p_memsz as usize;
        let filesz  = p_filesz as usize;
        let tcb_off = (memsz + align - 1) & !(align - 1);
        let total   = util::page_align_up((tcb_off + 8) as u64) as usize;

        let tls_mem = syscall::mmap(0, total, syscall::PROT_READ | syscall::PROT_WRITE);
        if tls_mem.is_null() { return 0; }

        util::memset(tls_mem, 0, total);
        if filesz > 0 {
            util::memcpy(tls_mem, (base + p_vaddr) as *const u8, filesz);
        }

        let tcb = unsafe { tls_mem.add(tcb_off) } as u64;
        unsafe { (tcb as *mut u64).write(tcb); }

        syscall::set_tls(tcb);
        return tcb;
    }
    0
}
