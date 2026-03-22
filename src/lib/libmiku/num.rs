#[no_mangle]
pub extern "C" fn miku_itoa(val: i64, buf: *mut u8) {
    if buf.is_null() { return; }
    let mut pos = 0usize;
    let mut num: u64;
    if val < 0 {
        unsafe { *buf = b'-'; }
        pos = 1;
        num = (-(val + 1)) as u64 + 1;
    } else {
        num = val as u64;
    }
    let start = pos;
    if num == 0 {
        unsafe { *buf.add(pos) = b'0'; }
        pos += 1;
    } else {
        while num > 0 {
            unsafe { *buf.add(pos) = b'0' + (num % 10) as u8; }
            pos += 1;
            num /= 10;
        }
        let mut l = start;
        let mut r = pos - 1;
        while l < r {
            unsafe {
                let tmp = *buf.add(l);
                *buf.add(l) = *buf.add(r);
                *buf.add(r) = tmp;
            }
            l += 1;
            r -= 1;
        }
    }
    unsafe { *buf.add(pos) = 0; }
}

#[no_mangle]
pub extern "C" fn miku_utoa(val: u64, buf: *mut u8) {
    if buf.is_null() { return; }
    let mut num = val;
    let mut pos = 0usize;
    if num == 0 {
        unsafe { *buf = b'0'; *buf.add(1) = 0; }
        return;
    }
    while num > 0 {
        unsafe { *buf.add(pos) = b'0' + (num % 10) as u8; }
        pos += 1;
        num /= 10;
    }
    let mut l = 0usize;
    let mut r = pos - 1;
    while l < r {
        unsafe {
            let tmp = *buf.add(l);
            *buf.add(l) = *buf.add(r);
            *buf.add(r) = tmp;
        }
        l += 1;
        r -= 1;
    }
    unsafe { *buf.add(pos) = 0; }
}

#[no_mangle]
pub extern "C" fn miku_atoi(s: *const u8) -> i64 {
    if s.is_null() { return 0; }
    let mut i = 0usize;
    unsafe {
        while *s.add(i) == b' ' || *s.add(i) == b'\t' { i += 1; }
    }
    let neg = unsafe { *s.add(i) } == b'-';
    if neg || unsafe { *s.add(i) } == b'+' { i += 1; }
    let mut result: i64 = 0;
    unsafe {
        while *s.add(i) >= b'0' && *s.add(i) <= b'9' {
            result = result.wrapping_mul(10).wrapping_add((*s.add(i) - b'0') as i64);
            i += 1;
        }
    }
    if neg { -result } else { result }
}

#[no_mangle]
pub extern "C" fn miku_print_int(val: i64) {
    let mut buf = core::mem::MaybeUninit::<[u8; 24]>::uninit();
    let ptr = buf.as_mut_ptr() as *mut u8;
    miku_itoa(val, ptr);
    crate::io::miku_print(ptr);
}

#[no_mangle]
pub extern "C" fn miku_print_hex(val: u64) {
    let mut buf = core::mem::MaybeUninit::<[u8; 19]>::uninit();
    let p = buf.as_mut_ptr() as *mut u8;
    unsafe {
        *p = b'0';
        *p.add(1) = b'x';
        let mut n = val;
        let mut i = 17usize;
        loop {
            let d = (n & 0xF) as u8;
            *p.add(i) = if d < 10 { b'0' + d } else { b'a' + d - 10 };
            n >>= 4;
            if i == 2 { break; }
            i -= 1;
        }
        *p.add(18) = 0;
        crate::io::miku_print(p);
    }
}
