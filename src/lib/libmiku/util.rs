#[no_mangle]
pub extern "C" fn miku_abs(x: i64) -> i64 {
    if x < 0 { -x } else { x }
}

#[no_mangle]
pub extern "C" fn miku_min(a: i64, b: i64) -> i64 {
    if a < b { a } else { b }
}

#[no_mangle]
pub extern "C" fn miku_max(a: i64, b: i64) -> i64 {
    if a > b { a } else { b }
}

#[no_mangle]
pub extern "C" fn miku_clamp(val: i64, lo: i64, hi: i64) -> i64 {
    if val < lo { lo } else if val > hi { hi } else { val }
}

#[no_mangle]
pub extern "C" fn miku_swap(a: *mut u64, b: *mut u64) {
    if a.is_null() || b.is_null() { return; }
    unsafe { let tmp = *a; *a = *b; *b = tmp; }
}

static mut RAND_STATE: u64 = 12345;

#[no_mangle]
pub extern "C" fn miku_srand(seed: u64) {
    unsafe { RAND_STATE = if seed == 0 { 1 } else { seed }; }
}

#[no_mangle]
pub extern "C" fn miku_rand() -> u64 {
    unsafe {
        RAND_STATE ^= RAND_STATE << 13;
        RAND_STATE ^= RAND_STATE >> 7;
        RAND_STATE ^= RAND_STATE << 17;
        RAND_STATE
    }
}

#[no_mangle]
pub extern "C" fn miku_rand_range(lo: u64, hi: u64) -> u64 {
    if hi <= lo { return lo; }
    lo + miku_rand() % (hi - lo)
}

#[no_mangle]
pub extern "C" fn miku_assert_fail(expr: *const u8, file: *const u8, line: i32) {
    crate::io::miku_print(b"ASSERT FAILED: ".as_ptr());
    if !expr.is_null() { crate::io::miku_print(expr); }
    if !file.is_null() {
        crate::io::miku_print(b" at ".as_ptr());
        crate::io::miku_print(file);
        crate::io::miku_print(b":".as_ptr());
        crate::num::miku_print_int(line as i64);
    }
    crate::io::miku_write(1, b"\n".as_ptr(), 1);
    crate::proc::miku_exit(134);
}

#[no_mangle]
pub extern "C" fn miku_panic(msg: *const u8) -> ! {
    crate::io::miku_print(b"PANIC: ".as_ptr());
    if !msg.is_null() { crate::io::miku_println(msg); }
    else { crate::io::miku_println(b"(no message)".as_ptr()); }
    crate::proc::miku_exit(134);
}
