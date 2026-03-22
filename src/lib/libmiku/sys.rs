use core::arch::asm;

pub const SYS_EXIT:     u64 = 0;
pub const SYS_WRITE:    u64 = 1;
pub const SYS_READ:     u64 = 2;
pub const SYS_MMAP:     u64 = 3;
pub const SYS_MUNMAP:   u64 = 4;
pub const SYS_MPROTECT: u64 = 5;
pub const SYS_BRK:      u64 = 6;
pub const SYS_GETPID:   u64 = 7;
pub const SYS_GETCWD:   u64 = 8;
pub const SYS_SET_TLS:  u64 = 9;
pub const SYS_GET_TLS:  u64 = 10;
pub const SYS_OPEN:     u64 = 11;
pub const SYS_CLOSE:    u64 = 12;
pub const SYS_SEEK:     u64 = 13;
pub const SYS_FSIZE:    u64 = 14;
pub const SYS_MAP_LIB:  u64 = 15;
pub const SYS_SLEEP:    u64 = 16;
pub const SYS_UPTIME:   u64 = 17;

#[inline(always)]
pub unsafe fn sc0(nr: u64) -> i64 {
    let r: i64;
    asm!("syscall", in("rax") nr, lateout("rax") r, out("rcx") _, out("r11") _, options(nostack));
    r
}

#[inline(always)]
pub unsafe fn sc1(nr: u64, a1: u64) -> i64 {
    let r: i64;
    asm!("syscall", in("rax") nr, in("rdi") a1, lateout("rax") r, out("rcx") _, out("r11") _, options(nostack));
    r
}

#[inline(always)]
pub unsafe fn sc2(nr: u64, a1: u64, a2: u64) -> i64 {
    let r: i64;
    asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2, lateout("rax") r, out("rcx") _, out("r11") _, options(nostack));
    r
}

#[inline(always)]
pub unsafe fn sc3(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let r: i64;
    asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2, in("rdx") a3, lateout("rax") r, out("rcx") _, out("r11") _, options(nostack));
    r
}

#[inline(always)]
pub unsafe fn sc4(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
    let r: i64;
    asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2, in("rdx") a3, in("r10") a4, lateout("rax") r, out("rcx") _, out("r11") _, options(nostack));
    r
}

#[inline(always)]
pub fn zbuf<const N: usize>() -> [u8; N] {
    unsafe {
        let mut buf = core::mem::MaybeUninit::<[u8; N]>::uninit();
        let p = buf.as_mut_ptr() as *mut u8;
        let mut i = 0;
        while i < N {
            core::ptr::write_volatile(p.add(i), 0);
            i += 1;
        }
        buf.assume_init()
    }
}
