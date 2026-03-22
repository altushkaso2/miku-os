#[no_mangle]
pub extern "C" fn miku_memset(dst: *mut u8, val: i32, n: usize) -> *mut u8 {
    if dst.is_null() || n == 0 { return dst; }
    let b = val as u8;
    let mut i = 0usize;
    let align_off = (8 - (dst as usize & 7)) & 7;
    let head = align_off.min(n);
    while i < head {
        unsafe { *dst.add(i) = b; }
        i += 1;
    }
    if i + 8 <= n {
        let fill: u64 = (b as u64) * 0x0101_0101_0101_0101;
        let p = unsafe { dst.add(i) as *mut u64 };
        let chunks = (n - i) / 8;
        for j in 0..chunks {
            unsafe { p.add(j).write(fill); }
        }
        i += chunks * 8;
    }
    while i < n {
        unsafe { *dst.add(i) = b; }
        i += 1;
    }
    dst
}

#[no_mangle]
pub extern "C" fn miku_memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dst.is_null() || src.is_null() || n == 0 { return dst; }
    let mut i = 0usize;
    if (dst as usize & 7) == (src as usize & 7) {
        let align_off = (8 - (dst as usize & 7)) & 7;
        let head = align_off.min(n);
        while i < head {
            unsafe { *dst.add(i) = *src.add(i); }
            i += 1;
        }
        if i + 8 <= n {
            let dp = unsafe { dst.add(i) as *mut u64 };
            let sp = unsafe { src.add(i) as *const u64 };
            let chunks = (n - i) / 8;
            for j in 0..chunks {
                unsafe { dp.add(j).write(sp.add(j).read()); }
            }
            i += chunks * 8;
        }
    }
    while i < n {
        unsafe { *dst.add(i) = *src.add(i); }
        i += 1;
    }
    dst
}

#[no_mangle]
pub extern "C" fn miku_memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dst.is_null() || src.is_null() || n == 0 { return dst; }
    if (dst as usize) < (src as usize) || (dst as usize) >= (src as usize) + n {
        return miku_memcpy(dst, src, n);
    }
    let mut i = n;
    while i > 0 {
        i -= 1;
        unsafe { *dst.add(i) = *src.add(i); }
    }
    dst
}

#[no_mangle]
pub extern "C" fn miku_memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let ca = unsafe { *a.add(i) };
        let cb = unsafe { *b.add(i) };
        if ca != cb { return ca as i32 - cb as i32; }
    }
    0
}

#[no_mangle]
pub extern "C" fn miku_bzero(dst: *mut u8, n: usize) {
    miku_memset(dst, 0, n);
}
