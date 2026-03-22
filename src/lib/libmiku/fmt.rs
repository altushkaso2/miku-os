use crate::sys::zbuf;

core::arch::global_asm!(
    ".global miku_printf",
    "miku_printf:",
    "push rbp",
    "mov rbp, rsp",
    "and rsp, -16",
    "sub rsp, 48",
    "mov [rsp], rsi",
    "mov [rsp+8], rdx",
    "mov [rsp+16], rcx",
    "mov [rsp+24], r8",
    "mov [rsp+32], r9",
    "mov rsi, rsp",
    "call miku_printf_impl",
    "mov rsp, rbp",
    "pop rbp",
    "ret",
);

core::arch::global_asm!(
    ".global miku_snprintf",
    "miku_snprintf:",
    "push rbp",
    "mov rbp, rsp",
    "and rsp, -16",
    "sub rsp, 48",
    "mov [rsp], rcx",
    "mov [rsp+8], r8",
    "mov [rsp+16], r9",
    "mov rcx, rsp",
    "call miku_snprintf_impl",
    "mov rsp, rbp",
    "pop rbp",
    "ret",
);

unsafe fn read_arg(args: *const u64, idx: usize) -> u64 {
    *args.add(idx)
}

static mut FMT_BUF: [u8; 24] = [0u8; 24];

unsafe fn fmt_int_to_buf(val: i64) -> usize {
    crate::num::miku_itoa(val, FMT_BUF.as_mut_ptr());
    crate::string::miku_strlen(FMT_BUF.as_ptr())
}

unsafe fn fmt_uint_to_buf(val: u64) -> usize {
    crate::num::miku_utoa(val, FMT_BUF.as_mut_ptr());
    crate::string::miku_strlen(FMT_BUF.as_ptr())
}

unsafe fn fmt_hex_to_buf(val: u64) -> usize {
    let mut n = val;
    if n == 0 { FMT_BUF[0] = b'0'; FMT_BUF[1] = 0; return 1; }
    let mut digits = 0usize;
    let mut v = n;
    while v > 0 { digits += 1; v >>= 4; }
    let mut pos = digits;
    FMT_BUF[pos] = 0;
    while n > 0 {
        pos -= 1;
        let d = (n & 0xF) as u8;
        FMT_BUF[pos] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
        n >>= 4;
    }
    digits
}

unsafe fn fmt_write_int(val: i64) -> i32 {
    let len = fmt_int_to_buf(val);
    crate::io::miku_write(1, FMT_BUF.as_ptr(), len);
    len as i32
}

unsafe fn fmt_write_uint(val: u64) -> i32 {
    let len = fmt_uint_to_buf(val);
    crate::io::miku_write(1, FMT_BUF.as_ptr(), len);
    len as i32
}

unsafe fn fmt_write_hex(val: u64) -> i32 {
    let len = fmt_hex_to_buf(val);
    crate::io::miku_write(1, FMT_BUF.as_ptr(), len);
    len as i32
}

unsafe fn fmt_write_ptr(val: u64) -> i32 {
    crate::io::miku_write(1, b"0x".as_ptr(), 2);
    let mut n = val;
    let mut pos = 0usize;
    while pos < 16 {
        let shift = (60 - pos * 4) as u32;
        let d = ((n >> shift) & 0xF) as u8;
        FMT_BUF[pos] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
        pos += 1;
    }
    crate::io::miku_write(1, FMT_BUF.as_ptr(), 16);
    18
}

#[no_mangle]
pub unsafe extern "C" fn miku_printf_impl(fmt: *const u8, args: *const u64) -> i32 {
    if fmt.is_null() { return -1; }
    let mut written: i32 = 0;
    let mut i = 0usize;
    let mut ai = 0usize;
    loop {
        let c = *fmt.add(i);
        if c == 0 { break; }
        if c == b'%' {
            i += 1;
            let spec = *fmt.add(i);
            match spec {
                b's' => {
                    let s = read_arg(args, ai) as *const u8; ai += 1;
                    if !s.is_null() {
                        let len = crate::string::miku_strlen(s);
                        crate::io::miku_write(1, s, len);
                        written += len as i32;
                    }
                }
                b'd' | b'i' => { let v = read_arg(args, ai) as i32 as i64; ai += 1; written += fmt_write_int(v); }
                b'u' => { let v = read_arg(args, ai) as u32 as u64; ai += 1; written += fmt_write_uint(v); }
                b'x' | b'X' => { let v = read_arg(args, ai) as u32 as u64; ai += 1; written += fmt_write_hex(v); }
                b'c' => { let ch = read_arg(args, ai) as u8; ai += 1; crate::io::miku_write(1, &ch as *const u8, 1); written += 1; }
                b'p' => { let v = read_arg(args, ai); ai += 1; written += fmt_write_ptr(v); }
                b'%' => { crate::io::miku_write(1, b"%".as_ptr(), 1); written += 1; }
                0 => break,
                _ => { crate::io::miku_write(1, b"%".as_ptr(), 1); crate::io::miku_write(1, &spec as *const u8, 1); written += 2; }
            }
        } else {
            crate::io::miku_write(1, fmt.add(i), 1);
            written += 1;
        }
        i += 1;
    }
    written
}

#[no_mangle]
pub unsafe extern "C" fn miku_snprintf_impl(buf: *mut u8, max: usize, fmt: *const u8, args: *const u64) -> i32 {
    if buf.is_null() || max == 0 || fmt.is_null() { return 0; }
    let limit = max - 1;
    let mut out = 0usize;
    let mut i = 0usize;
    let mut ai = 0usize;

    macro_rules! emit { ($b:expr) => { if out < limit { *buf.add(out) = $b; out += 1; } }; }
    macro_rules! emit_str { ($p:expr, $len:expr) => { let mut _k = 0usize; while _k < $len { emit!(*($p).add(_k)); _k += 1; } }; }

    loop {
        let c = *fmt.add(i);
        if c == 0 { break; }
        if c == b'%' {
            i += 1;
            let spec = *fmt.add(i);
            match spec {
                b's' => { let s = read_arg(args, ai) as *const u8; ai += 1; if !s.is_null() { let mut j = 0usize; while *s.add(j) != 0 { emit!(*s.add(j)); j += 1; } } }
                b'd' | b'i' => { let v = read_arg(args, ai) as i32 as i64; ai += 1; let len = fmt_int_to_buf(v); emit_str!(FMT_BUF.as_ptr(), len); }
                b'u' => { let v = read_arg(args, ai) as u32 as u64; ai += 1; let len = fmt_uint_to_buf(v); emit_str!(FMT_BUF.as_ptr(), len); }
                b'x' | b'X' => { let v = read_arg(args, ai) as u32 as u64; ai += 1; let len = fmt_hex_to_buf(v); emit_str!(FMT_BUF.as_ptr(), len); }
                b'c' => { let ch = read_arg(args, ai) as u8; ai += 1; emit!(ch); }
                b'%' => { emit!(b'%'); }
                0 => break,
                _ => { emit!(b'%'); emit!(spec); }
            }
        } else { emit!(c); }
        i += 1;
    }
    *buf.add(out) = 0;
    out as i32
}
