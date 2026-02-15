#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![allow(dead_code, unused_imports, unused_variables, static_mut_refs, mismatched_lifetime_syntaxes)]

extern crate alloc;

use core::panic::PanicInfo;
use bootloader_api::{entry_point, BootloaderConfig};

mod console;
mod color;
mod interrupts;
mod gdt;
mod shell;
mod commands;
mod vfs;
mod ata;
mod font;
mod miku_extfs;
mod allocator;
mod power;
pub mod serial;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.kernel_stack_size = 512 * 1024;
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();
        *console::WRITER.lock() = Some(console::Console::new(buffer, info));
    }

    serial_println!("[kern] framebuffer ok");

    gdt::init();
    allocator::init();

    vfs::core::init_vfs();
    serial_println!("[kern] vfs ok");

    print_info!("Welcome to Miku OS");

    interrupts::init_idt();
    unsafe { interrupts::PICS.lock().initialize() };
    x86_64::instructions::interrupts::enable();

    serial_println!("[kern] interrupts ok");
    shell::init();

    loop {
        shell::process_pending();
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("[panic] {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
