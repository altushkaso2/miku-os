#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    static_mut_refs,
    mismatched_lifetime_syntaxes
)]

extern crate alloc;

use core::panic::PanicInfo;

mod allocator;
mod ata;
mod color;
mod commands;
mod console;
mod font;
mod gdt;
mod interrupts;
mod limine;
mod miku_extfs;
mod power;
pub mod serial;
mod shell;
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
    serial_println!("[kern] limine protocol ok");

    gdt::init();

    interrupts::init_idt();

    interrupts::init_pics();

    allocator::init();

    init_framebuffer();

    vfs::core::init_vfs();
    serial_println!("[kern] vfs ok");

    print_info!("Welcome to Miku OS");

    shell::init();
    serial_println!("[kern] shell ready");

    x86_64::instructions::interrupts::enable();
    serial_println!("[kern] interrupts enabled — keyboard active");

    loop {
        x86_64::instructions::interrupts::disable();
        shell::process_pending();
        x86_64::instructions::interrupts::enable();
        x86_64::instructions::hlt();
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

    let width = fb.width() as usize;
    let height = fb.height() as usize;
    let pitch = fb.pitch() as usize;
    let bpp = fb.bpp() as usize;
    let bytes_per_pixel = bpp / 8;

    if bytes_per_pixel == 0 || pitch == 0 || width == 0 || height == 0 {
        serial_println!("[kern] warn: invalid framebuffer params");
        return;
    }

    let stride = pitch / bytes_per_pixel;
    let fb_size = pitch * height;

    serial_println!(
        "[kern] fb: {}x{} bpp={} pitch={} stride={} size={}",
        width, height, bpp, pitch, stride, fb_size
    );

    let fb_ptr = fb.addr();
    if fb_ptr.is_null() {
        serial_println!("[kern] WARN: framebuffer address is null!");
        return;
    }

    let buffer = unsafe {
        core::slice::from_raw_parts_mut(fb_ptr, fb_size)
    };

    let config = console::FrameBufferConfig {
        width,
        height,
        stride,
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
    loop {
        x86_64::instructions::hlt();
    }
}
