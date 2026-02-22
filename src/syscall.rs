use x86_64::registers::model_specific::{Efer, EferFlags, LStar, Star, SFMask};
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;

use crate::gdt;

pub fn init() {
    unsafe {
        Efer::update(|f| *f |= EferFlags::SYSTEM_CALL_EXTENSIONS);
    }

    Star::write(
        gdt::user_code_selector(),
        gdt::user_data_selector(),
        gdt::kernel_code_selector(),
        gdt::kernel_data_selector(),
    ).unwrap();

    LStar::write(VirtAddr::new(syscall_handler as *const () as u64));  // <- вот здесь

    SFMask::write(RFlags::INTERRUPT_FLAG);

    crate::serial_println!("[syscall] syscall/sysret ready, handler={:#x}", syscall_handler as *const () as u64);  // <- и здесь
}

#[unsafe(naked)]
unsafe extern "C" fn syscall_handler() {
    core::arch::naked_asm!(
        "swapgs",
        "push rcx",
        "push r11",
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "mov rdi, rax",
        "mov rsi, rbx",
        "mov rdx, rcx",
        "mov rcx, r10",
        "call {handler}",

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "pop r11",
        "pop rcx",
        "swapgs",
        "sysretq",
        handler = sym dispatch,
    );
}

extern "C" fn dispatch(nr: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    match nr {
        0 => sys_write(a1, a2, a3),
        1 => sys_read(a1, a2, a3),
        _ => u64::MAX,
    }
}

fn sys_write(fd: u64, buf_ptr: u64, len: u64) -> u64 {
    if fd != 1 && fd != 2 {
        return u64::MAX;
    }
    let slice = unsafe {
        core::slice::from_raw_parts(buf_ptr as *const u8, len as usize)
    };
    if let Ok(s) = core::str::from_utf8(slice) {
        crate::print!("{}", s);
    }
    len
}

fn sys_read(_fd: u64, _buf: u64, _len: u64) -> u64 {
    0
}
