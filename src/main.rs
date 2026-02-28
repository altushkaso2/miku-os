#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    static_mut_refs,
    mismatched_lifetime_syntaxes,
    unused_assignments,
    unused_mut
)]
#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

use core::panic::PanicInfo;

mod allocator;
mod ata;
pub mod boot;
mod color;
mod commands;
mod console;
mod font;
mod gdt;
mod interrupts;
mod limine;
mod miku_extfs;
pub mod mkfs;
mod net;
mod pmm;
mod power;
mod process;
mod ring3;
mod scheduler;
mod syscall;
pub mod serial;
mod shell;
pub mod stdin;
pub mod timing;
mod vmm;
mod vfs;

#[no_mangle]
unsafe extern "C" fn _start() -> ! {
    kernel_main();
}

fn kernel_main() -> ! {
    serial_println!("[kern] MikuOS starting");

    if !limine::BASE_REVISION.is_supported() {
        serial_println!("[kern] fatal: limine base revision not supported!");
        loop { x86_64::instructions::hlt(); }
    }

    gdt::init();
    syscall::init();
    interrupts::init_idt();
    interrupts::init_pics();
    interrupts::init_pit_1000hz();
    allocator::init();
    scheduler::reinit_scheduler();

    init_framebuffer();

    if let Some(hhdm) = limine::HHDM_REQUEST.get_response() {
        net::set_hhdm_offset(hhdm.offset());
    }
    if let Some(kaddr) = limine::KERNEL_ADDR_REQUEST.get_response() {
        net::set_kernel_address(kaddr.virtual_base(), kaddr.physical_base());
    }
    if let Some(mmap) = limine::MEMMAP_REQUEST.get_response() {
        for entry in mmap.entries() {
            if entry.entry_type == ::limine::memory_map::EntryType::USABLE {
                pmm::add_region(entry.base, entry.length);
            }
        }
    }

    boot_step!("Physical memory manager", Ok(()));
    boot_step!("Virtual file system",     vfs::core::init_vfs());
    boot_step!("Network subsystem",       net::init());

    scheduler::SCHEDULER.lock().init_main_thread();
    scheduler::init_workers(4);
    boot_step!("Scheduler (4 workers)",   Ok(()));

    x86_64::instructions::interrupts::enable();
    boot_step!("Interrupts",              Ok(()));

    timing::calibrate();
    boot_step!("Timer calibration",       Ok(()));

    boot::mark_done();

    scheduler::spawn_named(shell::kbd_thread,   "kbd",   2);
    scheduler::spawn_named(shell::shell_thread, "shell", 2);

    console::clear_screen();
    shell::init();

    loop {
        x86_64::instructions::interrupts::enable_and_hlt();
        interrupts::check_and_schedule();
    }
}

fn init_framebuffer() {
    let response = match limine::FRAMEBUFFER_REQUEST.get_response() {
        Some(r) => r,
        None => {
            serial_println!("[kern] warn: no framebuffer from limine");
            return;
        }
    };
    let fb = match response.framebuffers().next() {
        Some(fb) => fb,
        None => {
            serial_println!("[kern] warn: no framebuffers available");
            return;
        }
    };

    let width           = fb.width()  as usize;
    let height          = fb.height() as usize;
    let pitch           = fb.pitch()  as usize;
    let bpp             = fb.bpp()    as usize;
    let bytes_per_pixel = bpp / 8;

    if bytes_per_pixel == 0 || pitch == 0 || width == 0 || height == 0 {
        serial_println!("[kern] warn: invalid framebuffer params");
        return;
    }

    let fb_ptr = fb.addr();
    if fb_ptr.is_null() {
        serial_println!("[kern] warn: framebuffer address is null!");
        return;
    }

    let buffer = unsafe { core::slice::from_raw_parts_mut(fb_ptr, pitch * height) };
    let config = console::FrameBufferConfig {
        width,
        height,
        stride: pitch / bytes_per_pixel,
        bytes_per_pixel,
        is_bgr: true,
    };
    *console::WRITER.lock() = Some(console::Console::new_limine(buffer, config));
    serial_println!("[kern] framebuffer initialized");
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    x86_64::instructions::interrupts::disable();
    serial_println!("[panic] {}", info);
    crate::cprintln!(255, 50, 50, "kernel panic: {}", info);
    loop { x86_64::instructions::hlt(); }
}
