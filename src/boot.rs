use core::sync::atomic::{AtomicBool, Ordering};

pub type InitResult = Result<(), &'static str>;

static BOOT_DONE: AtomicBool = AtomicBool::new(false);

pub fn is_done() -> bool {
    BOOT_DONE.load(Ordering::Acquire)
}

pub fn mark_done() {
    BOOT_DONE.store(true, Ordering::Release);
}

pub fn record(name: &str, result: InitResult) {
    match result {
        Ok(()) => {
            crate::cprint!(100, 100, 100, "[");
            crate::cprint!(100, 220, 150, "  ok  ");
            crate::cprint!(100, 100, 100, "] ");
            crate::cprintln!(100, 170, 255, "{}", name);
            crate::serial_println!("[boot] ok  {}", name);
        }
        Err(reason) => {
            crate::cprint!(100, 100, 100, "[");
            crate::cprint!(255, 70, 70, " fail ");
            crate::cprint!(100, 100, 100, "] ");
            crate::cprint!(100, 170, 255, "{}", name);
            crate::cprintln!(120, 120, 120, ": {}", reason);
            crate::serial_println!("[boot] fail {} : {}", name, reason);
        }
    }
}

#[macro_export]
macro_rules! boot_step {
    ($name:expr, $expr:expr) => {
        $crate::boot::record($name, $expr)
    };
}
