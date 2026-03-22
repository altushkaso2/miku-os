#![no_std]
#![no_main]
#![allow(dead_code, unused, static_mut_refs)]

pub mod sys;
pub mod proc;
pub mod io;
pub mod mem;
pub mod num;
pub mod string;
pub mod heap;
pub mod file;
pub mod time;
pub mod util;
pub mod fmt;

#[no_mangle]
#[link_section = ".text._libmiku_start"]
pub extern "C" fn _libmiku_start() -> ! { loop {} }

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    io::miku_write(2, b"libmiku: panic\n".as_ptr(), 15);
    proc::miku_exit(127);
}
