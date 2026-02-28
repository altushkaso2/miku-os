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

    LStar::write(VirtAddr::new(syscall_handler as *const () as u64));

    SFMask::write(RFlags::INTERRUPT_FLAG);

    crate::serial_println!(
        "[syscall] syscall/sysret ready, handler={:#x}",
        syscall_handler as *const () as u64
    );
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

        "mov rsp, gs:[8]",
        "swapgs",
        "sysretq",
        handler = sym dispatch,
    );
}

extern "C" fn dispatch(nr: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    match nr {
        0 => sys_write(a1, a2, a3),
        1 => sys_read(a1, a2, a3),
        2 => sys_exit(),
        3 => sys_sleep(a1),
        4 => sys_getpid(),
        _ => u64::MAX,
    }
}

fn sys_write(fd: u64, buf_ptr: u64, len: u64) -> u64 {
    if fd != 1 && fd != 2 {
        return u64::MAX;
    }
    if buf_ptr == 0 || len == 0 || len > 4096 {
        return u64::MAX;
    }
    if buf_ptr < 0x1000 || buf_ptr > 0x0000_7FFF_FFFF_FFFF {
        return u64::MAX;
    }
    let slice = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, len as usize) };
    if let Ok(s) = core::str::from_utf8(slice) {
        crate::print!("{}", s);
    }
    len
}

fn sys_read(_fd: u64, _buf: u64, _len: u64) -> u64 {
    0
}

fn sys_exit() -> u64 {
    {
        let mut sched = crate::scheduler::SCHEDULER.lock();
        if let Some(curr) = sched.current {
            if let Some(p) = sched.procs.get_mut(&curr) {
                p.state = crate::process::ProcessState::Dead;
            }
        }
    }
    crate::scheduler::schedule(crate::interrupts::get_tick());
    0
}

fn sys_sleep(ticks: u64) -> u64 {
    crate::scheduler::sleep(ticks);
    0
}

fn sys_getpid() -> u64 {
    crate::scheduler::current_pid()
}
