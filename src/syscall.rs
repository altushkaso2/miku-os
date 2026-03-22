extern crate alloc;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, Star, SFMask};
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;
use crate::gdt;
use crate::mmap;
use crate::vmm::AddressSpace;

const PAGE_SIZE: u64 = 4096;
const USER_MAX: u64 = 0x0000_7FFF_FFFF_FFFF;

struct OpenFile {
    data: Vec<u8>,
    offset: usize,
}

struct ProcessFds {
    files: BTreeMap<u64, OpenFile>,
    next_fd: u64,
}

impl ProcessFds {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            next_fd: 3,
        }
    }
}

static FD_TABLE: Mutex<BTreeMap<u64, ProcessFds>> = Mutex::new(BTreeMap::new());

fn with_fds<F: FnOnce(&mut ProcessFds) -> R, R>(pid: u64, f: F) -> R {
    let mut table = FD_TABLE.lock();
    let pfds = table.entry(pid).or_insert_with(ProcessFds::new);
    f(pfds)
}

pub fn init() {
    unsafe {
        Efer::update(|f| *f |= EferFlags::SYSTEM_CALL_EXTENSIONS | EferFlags::NO_EXECUTE_ENABLE);
    }
    Star::write(
        gdt::GDT.1.user_code,
        gdt::user_data_selector(),
        gdt::kernel_code_selector(),
        gdt::kernel_data_selector(),
    ).unwrap();
    LStar::write(VirtAddr::new(syscall_handler as *const () as u64));
    SFMask::write(RFlags::INTERRUPT_FLAG);
    crate::serial_println!("[syscall] MikuOS native table ready");
}

#[unsafe(naked)]
unsafe extern "C" fn syscall_handler() {
    core::arch::naked_asm!(
        "swapgs",
        "mov gs:[8], rsp",
        "mov rsp, gs:[0]",
        "push rcx",
        "push r11",
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "push r10",
        "push r9",
        "push r8",
        "mov r8,  r10",
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",
        "call {handler}",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "pop r11",
        "pop rcx",
        "mov rsp, gs:[8]",
        "swapgs",
        "sysretq",
        handler = sym dispatch,
    );
}

fn current_cr3() -> u64 {
    let (frame, _) = x86_64::registers::control::Cr3::read();
    frame.start_address().as_u64()
}

fn current_pid() -> u64 {
    crate::scheduler::current_pid()
}

fn user_ptr_mapped(cr3: u64, ptr: u64, len: u64) -> bool {
    if ptr == 0 || len == 0 {
        return false;
    }
    if ptr > USER_MAX {
        return false;
    }

    let end = match ptr.checked_add(len) {
        Some(e) if e <= USER_MAX + 1 => e,
        _ => return false,
    };

    let aspace = AddressSpace::from_raw(cr3);
    let start_page = ptr & !0xFFF;
    let end_page = (end + 0xFFF) & !0xFFF;
    let mut va = start_page;
    let mut ok = true;

    while va < end_page {
        if aspace.virt_to_phys(va).is_none() {
            ok = false;
            break;
        }
        va += PAGE_SIZE;
    }

    let _ = aspace.into_raw();
    ok
}

extern "C" fn dispatch(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> u64 {
    match nr {
        0 => sys_exit(a1),
        1 => sys_write(a1, a2, a3),
        2 => sys_read(a1, a2, a3),
        3 => sys_mmap(a1, a2, a3, a4),
        4 => sys_munmap(a1, a2),
        5 => sys_mprotect(a1, a2, a3),
        6 => sys_brk(a1),
        7 => current_pid(),
        8 => sys_getcwd(a1, a2),
        9 => sys_set_tls(a1),
        10 => sys_get_tls(),
        11 => sys_open(a1, a2),
        12 => sys_close(a1),
        13 => sys_seek(a1, a2),
        14 => sys_fsize(a1),
        15 => sys_map_lib(a1, a2),
        16 => sys_sleep(a1),
        17 => sys_uptime(),
        _ => {
            crate::serial_println!("[syscall] unknown nr={}", nr);
            err(ENOSYS)
        }
    }
}

fn err(code: i64) -> u64 { code as u64 }

const ENOENT: i64 = -2;
const EBADF: i64 = -9;
const ENOMEM: i64 = -12;
const EFAULT: i64 = -14;
const EINVAL: i64 = -22;
const ENOSYS: i64 = -38;

fn sys_exit(_code: u64) -> u64 {
    let pid = current_pid();
    FD_TABLE.lock().remove(&pid);
    crate::scheduler::kill(pid);
    crate::scheduler::yield_now();
    0
}

fn sys_write(fd: u64, ptr: u64, len: u64) -> u64 {
    if fd != 1 && fd != 2 {
        return err(EBADF);
    }
    if len == 0 {
        return 0;
    }
    if len > 65536 {
        return err(EINVAL);
    }

    let cr3 = current_cr3();
    if !user_ptr_mapped(cr3, ptr, len) {
        return err(EFAULT);
    }

    let s = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    match core::str::from_utf8(s) {
        Ok(t) => crate::print!("{}", t),
        Err(_) => {
            for &b in s {
                crate::print!("{}", b as char);
            }
        }
    }
    len
}

fn sys_read(fd: u64, buf: u64, len: u64) -> u64 {
    if len == 0 {
        return 0;
    }

    let cr3 = current_cr3();
    if !user_ptr_mapped(cr3, buf, len) {
        return err(EFAULT);
    }

    if fd == 0 {
        return crate::user_stdin::read(buf, len);
    }

    let pid = current_pid();
    with_fds(pid, |pfds| {
        let file = match pfds.files.get_mut(&fd) {
            Some(f) => f,
            None => return err(EBADF),
        };

        let remaining = file.data.len().saturating_sub(file.offset);
        let to_copy = (len as usize).min(remaining);
        if to_copy == 0 {
            return 0;
        }

        unsafe {
            core::ptr::copy_nonoverlapping(
                file.data.as_ptr().add(file.offset),
                buf as *mut u8,
                to_copy,
            );
        }
        file.offset += to_copy;
        to_copy as u64
    })
}

fn sys_open(path_ptr: u64, path_len: u64) -> u64 {
    if path_len == 0 || path_len > 4096 {
        return err(EINVAL);
    }

    let cr3 = current_cr3();
    if !user_ptr_mapped(cr3, path_ptr, path_len) {
        return err(EFAULT);
    }

    let path_bytes = unsafe {
        core::slice::from_raw_parts(path_ptr as *const u8, path_len as usize)
    };

    let path = match core::str::from_utf8(path_bytes) {
        Ok(s) => s,
        Err(_) => return err(EINVAL),
    };

    let path_trimmed = path.trim_end_matches('\0');

    crate::serial_println!("[syscall] open '{}'", path_trimmed);

    let data = match crate::vfs_read::read_file_or_solib(path_trimmed) {
        Some(d) => d,
        None => return err(ENOENT),
    };

    crate::serial_println!("[syscall] open '{}' -> {} bytes", path_trimmed, data.len());

    let pid = current_pid();
    with_fds(pid, |pfds| {
        let fd = pfds.next_fd;
        pfds.next_fd += 1;
        pfds.files.insert(fd, OpenFile { data, offset: 0 });
        fd
    })
}

fn sys_close(fd: u64) -> u64 {
    let pid = current_pid();
    with_fds(pid, |pfds| {
        if pfds.files.remove(&fd).is_some() {
            0
        } else {
            err(EBADF)
        }
    })
}

fn sys_seek(fd: u64, offset: u64) -> u64 {
    let pid = current_pid();
    with_fds(pid, |pfds| {
        let file = match pfds.files.get_mut(&fd) {
            Some(f) => f,
            None => return err(EBADF),
        };
        file.offset = (offset as usize).min(file.data.len());
        file.offset as u64
    })
}

fn sys_fsize(fd: u64) -> u64 {
    let pid = current_pid();
    with_fds(pid, |pfds| {
        match pfds.files.get(&fd) {
            Some(f) => f.data.len() as u64,
            None => err(EBADF),
        }
    })
}

fn sys_mmap(addr: u64, len: u64, prot: u64, flags: u64) -> u64 {
    if len == 0 {
        return err(EINVAL);
    }
    let cr3 = current_cr3();
    let mflags = (flags as u32) | 0x20;
    let result = mmap::sys_mmap(cr3, addr, len, prot as u32, mflags, -1, 0);
    if result < 0 { err(result as i64) } else { result as u64 }
}

fn sys_munmap(addr: u64, len: u64) -> u64 {
    if addr & 0xFFF != 0 {
        return err(EINVAL);
    }
    let cr3 = current_cr3();
    let result = mmap::sys_munmap(cr3, addr, len);
    if result < 0 { err(result as i64) } else { 0 }
}

fn sys_mprotect(addr: u64, len: u64, prot: u64) -> u64 {
    if addr & 0xFFF != 0 {
        return err(EINVAL);
    }
    let cr3 = current_cr3();
    let result = mmap::sys_mprotect(cr3, addr, len, prot as u32);
    if result < 0 { err(result as i64) } else { 0 }
}

fn sys_brk(addr: u64) -> u64 {
    mmap::sys_brk(current_cr3(), addr)
}

fn sys_getcwd(buf: u64, size: u64) -> u64 {
    if size < 2 {
        return err(EINVAL);
    }
    let cr3 = current_cr3();
    if !user_ptr_mapped(cr3, buf, size) {
        return err(EFAULT);
    }
    unsafe {
        let p = buf as *mut u8;
        p.write(b'/');
        p.add(1).write(0);
    }
    buf
}

fn sys_set_tls(addr: u64) -> u64 {
    x86_64::registers::model_specific::FsBase::write(VirtAddr::new(addr));
    crate::serial_println!("[syscall] set_tls={:#x}", addr);
    0
}

fn sys_get_tls() -> u64 {
    x86_64::registers::model_specific::FsBase::read().as_u64()
}

fn sys_map_lib(name_ptr: u64, name_len: u64) -> u64 {
    if name_len == 0 || name_len > 256 {
        return err(EINVAL);
    }
    let cr3 = current_cr3();
    if !user_ptr_mapped(cr3, name_ptr, name_len) {
        return err(EFAULT);
    }

    let name_bytes = unsafe {
        core::slice::from_raw_parts(name_ptr as *const u8, name_len as usize)
    };
    let soname = match core::str::from_utf8(name_bytes) {
        Ok(s) => s.trim_end_matches('\0'),
        Err(_) => return err(EINVAL),
    };

    match crate::solib::map_into_process(soname, cr3) {
        Ok(base) => base,
        Err(e) => e as u64,
    }
}

fn sys_sleep(ticks: u64) -> u64 {
    if ticks == 0 {
        crate::scheduler::yield_now();
        return 0;
    }
    let clamped = ticks.min(100_000);
    crate::scheduler::sleep(clamped);
    0
}

fn sys_uptime() -> u64 {
    crate::interrupts::get_tick()
}
