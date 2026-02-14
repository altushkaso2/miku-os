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

use bootloader_api::{entry_point, BootloaderConfig};
use core::panic::PanicInfo;

mod ata;
mod color;
mod commands;
mod console;
mod font;
mod fs;
mod gdt;
mod interrupts;
pub mod serial;
mod shell;
mod vfs;

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
