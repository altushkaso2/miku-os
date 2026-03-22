#[no_mangle]
pub extern "C" fn miku_strlen(s: *const u8) -> usize {
    if s.is_null() { return 0; }
    let mut n = 0usize;
    unsafe { while *s.add(n) != 0 { n += 1; } }
    n
}

#[no_mangle]
pub extern "C" fn miku_strcmp(a: *const u8, b: *const u8) -> i32 {
    if a.is_null() && b.is_null() { return 0; }
    if a.is_null() { return -1; }
    if b.is_null() { return 1; }
    let mut i = 0usize;
    unsafe {
        loop {
            let ca = *a.add(i);
            let cb = *b.add(i);
            if ca != cb { return ca as i32 - cb as i32; }
            if ca == 0 { return 0; }
            i += 1;
        }
    }
}

#[no_mangle]
pub extern "C" fn miku_strncmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    if n == 0 { return 0; }
    let mut i = 0usize;
    unsafe {
        while i < n {
            let ca = if a.is_null() { 0 } else { *a.add(i) };
            let cb = if b.is_null() { 0 } else { *b.add(i) };
            if ca != cb { return ca as i32 - cb as i32; }
            if ca == 0 { return 0; }
            i += 1;
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn miku_strcpy(dst: *mut u8, src: *const u8) -> *mut u8 {
    if dst.is_null() || src.is_null() { return dst; }
    let mut i = 0usize;
    unsafe {
        loop {
            let c = *src.add(i);
            *dst.add(i) = c;
            if c == 0 { break; }
            i += 1;
        }
    }
    dst
}

#[no_mangle]
pub extern "C" fn miku_strncpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dst.is_null() || src.is_null() { return dst; }
    let mut i = 0usize;
    let mut done = false;
    unsafe {
        while i < n {
            if !done {
                let c = *src.add(i);
                *dst.add(i) = c;
                if c == 0 { done = true; }
            } else {
                *dst.add(i) = 0;
            }
            i += 1;
        }
    }
    dst
}

#[no_mangle]
pub extern "C" fn miku_strcat(dst: *mut u8, src: *const u8) -> *mut u8 {
    if dst.is_null() || src.is_null() { return dst; }
    let dlen = miku_strlen(dst);
    let mut i = 0usize;
    unsafe {
        loop {
            let c = *src.add(i);
            *dst.add(dlen + i) = c;
            if c == 0 { break; }
            i += 1;
        }
    }
    dst
}

#[no_mangle]
pub extern "C" fn miku_strncat(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dst.is_null() || src.is_null() { return dst; }
    let dlen = miku_strlen(dst);
    let mut i = 0usize;
    unsafe {
        while i < n {
            let c = *src.add(i);
            if c == 0 { break; }
            *dst.add(dlen + i) = c;
            i += 1;
        }
        *dst.add(dlen + i) = 0;
    }
    dst
}

#[no_mangle]
pub extern "C" fn miku_strchr(s: *const u8, c: i32) -> *const u8 {
    if s.is_null() { return core::ptr::null(); }
    let target = c as u8;
    let mut i = 0usize;
    unsafe {
        loop {
            let ch = *s.add(i);
            if ch == target { return s.add(i); }
            if ch == 0 { return core::ptr::null(); }
            i += 1;
        }
    }
}

#[no_mangle]
pub extern "C" fn miku_strrchr(s: *const u8, c: i32) -> *const u8 {
    if s.is_null() { return core::ptr::null(); }
    let target = c as u8;
    let mut last: *const u8 = core::ptr::null();
    let mut i = 0usize;
    unsafe {
        loop {
            let ch = *s.add(i);
            if ch == target { last = s.add(i); }
            if ch == 0 { return last; }
            i += 1;
        }
    }
}

#[no_mangle]
pub extern "C" fn miku_strstr(haystack: *const u8, needle: *const u8) -> *const u8 {
    if haystack.is_null() || needle.is_null() { return core::ptr::null(); }
    let nlen = miku_strlen(needle);
    if nlen == 0 { return haystack; }
    let hlen = miku_strlen(haystack);
    if nlen > hlen { return core::ptr::null(); }
    for i in 0..=(hlen - nlen) {
        let mut found = true;
        for j in 0..nlen {
            if unsafe { *haystack.add(i + j) != *needle.add(j) } {
                found = false;
                break;
            }
        }
        if found { return unsafe { haystack.add(i) }; }
    }
    core::ptr::null()
}

#[no_mangle]
pub extern "C" fn miku_strdup(s: *const u8) -> *mut u8 {
    if s.is_null() { return core::ptr::null_mut(); }
    let len = miku_strlen(s);
    let p = crate::heap::miku_malloc(len + 1);
    if p.is_null() { return core::ptr::null_mut(); }
    crate::mem::miku_memcpy(p, s, len + 1);
    p
}

#[no_mangle]
pub extern "C" fn miku_toupper(c: i32) -> i32 {
    if c >= b'a' as i32 && c <= b'z' as i32 { c - 32 } else { c }
}

#[no_mangle]
pub extern "C" fn miku_tolower(c: i32) -> i32 {
    if c >= b'A' as i32 && c <= b'Z' as i32 { c + 32 } else { c }
}

#[no_mangle]
pub extern "C" fn miku_isdigit(c: i32) -> i32 {
    if c >= b'0' as i32 && c <= b'9' as i32 { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn miku_isalpha(c: i32) -> i32 {
    if (c >= b'a' as i32 && c <= b'z' as i32) || (c >= b'A' as i32 && c <= b'Z' as i32) { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn miku_isalnum(c: i32) -> i32 {
    if miku_isalpha(c) != 0 || miku_isdigit(c) != 0 { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn miku_isspace(c: i32) -> i32 {
    if c == b' ' as i32 || c == b'\t' as i32 || c == b'\n' as i32
        || c == b'\r' as i32 || c == 0x0B || c == 0x0C { 1 } else { 0 }
}

fn is_delim(c: u8, delim: *const u8) -> bool {
    if delim.is_null() { return false; }
    let mut i = 0usize;
    unsafe {
        while *delim.add(i) != 0 {
            if *delim.add(i) == c { return true; }
            i += 1;
        }
    }
    false
}

static mut STRTOK_POS: *mut u8 = core::ptr::null_mut();

#[no_mangle]
pub extern "C" fn miku_strtok(s: *mut u8, delim: *const u8) -> *mut u8 {
    unsafe {
        let mut p = if !s.is_null() { s } else { STRTOK_POS };
        if p.is_null() { return core::ptr::null_mut(); }
        while *p != 0 && is_delim(*p, delim) { p = p.add(1); }
        if *p == 0 { STRTOK_POS = core::ptr::null_mut(); return core::ptr::null_mut(); }
        let start = p;
        while *p != 0 && !is_delim(*p, delim) { p = p.add(1); }
        if *p != 0 { *p = 0; STRTOK_POS = p.add(1); }
        else { STRTOK_POS = core::ptr::null_mut(); }
        start
    }
}

#[no_mangle]
pub extern "C" fn miku_strpbrk(s: *const u8, accept: *const u8) -> *const u8 {
    if s.is_null() || accept.is_null() { return core::ptr::null(); }
    let mut i = 0usize;
    unsafe {
        while *s.add(i) != 0 {
            if is_delim(*s.add(i), accept) { return s.add(i); }
            i += 1;
        }
    }
    core::ptr::null()
}

#[no_mangle]
pub extern "C" fn miku_strspn(s: *const u8, accept: *const u8) -> usize {
    if s.is_null() || accept.is_null() { return 0; }
    let mut i = 0usize;
    unsafe { while *s.add(i) != 0 && is_delim(*s.add(i), accept) { i += 1; } }
    i
}

#[no_mangle]
pub extern "C" fn miku_strcspn(s: *const u8, reject: *const u8) -> usize {
    if s.is_null() || reject.is_null() { return 0; }
    let mut i = 0usize;
    unsafe { while *s.add(i) != 0 && !is_delim(*s.add(i), reject) { i += 1; } }
    i
}

#[no_mangle]
pub extern "C" fn miku_strtol(s: *const u8, endptr: *mut *const u8, base: i32) -> i64 {
    if s.is_null() { return 0; }
    let mut i = 0usize;
    unsafe { while miku_isspace(*s.add(i) as i32) != 0 { i += 1; } }
    let neg = unsafe { *s.add(i) } == b'-';
    if neg || unsafe { *s.add(i) } == b'+' { i += 1; }
    let mut radix = base as u64;
    if radix == 0 {
        if unsafe { *s.add(i) } == b'0' {
            i += 1;
            if unsafe { *s.add(i) } == b'x' || unsafe { *s.add(i) } == b'X' { i += 1; radix = 16; }
            else { radix = 8; }
        } else { radix = 10; }
    } else if radix == 16 {
        if unsafe { *s.add(i) } == b'0'
            && (unsafe { *s.add(i + 1) } == b'x' || unsafe { *s.add(i + 1) } == b'X') { i += 2; }
    }
    let mut result: i64 = 0;
    unsafe {
        loop {
            let c = *s.add(i);
            let digit = if c >= b'0' && c <= b'9' { (c - b'0') as u64 }
                else if c >= b'a' && c <= b'f' { (c - b'a' + 10) as u64 }
                else if c >= b'A' && c <= b'F' { (c - b'A' + 10) as u64 }
                else { break; };
            if digit >= radix { break; }
            result = result.wrapping_mul(radix as i64).wrapping_add(digit as i64);
            i += 1;
        }
        if !endptr.is_null() { *endptr = s.add(i); }
    }
    if neg { -result } else { result }
}

#[no_mangle]
pub extern "C" fn miku_strtoul(s: *const u8, endptr: *mut *const u8, base: i32) -> u64 {
    miku_strtol(s, endptr, base) as u64
}

#[no_mangle]
pub extern "C" fn miku_strlcpy(dst: *mut u8, src: *const u8, size: usize) -> usize {
    if dst.is_null() || src.is_null() { return 0; }
    let slen = miku_strlen(src);
    if size > 0 {
        let copy = if slen < size { slen } else { size - 1 };
        unsafe {
            let mut i = 0usize;
            while i < copy { *dst.add(i) = *src.add(i); i += 1; }
            *dst.add(copy) = 0;
        }
    }
    slen
}

#[no_mangle]
pub extern "C" fn miku_strlcat(dst: *mut u8, src: *const u8, size: usize) -> usize {
    if dst.is_null() || src.is_null() { return 0; }
    let dlen = miku_strlen(dst);
    let slen = miku_strlen(src);
    if dlen >= size { return size + slen; }
    let avail = size - dlen - 1;
    let copy = if slen < avail { slen } else { avail };
    unsafe {
        let mut i = 0usize;
        while i < copy { *dst.add(dlen + i) = *src.add(i); i += 1; }
        *dst.add(dlen + copy) = 0;
    }
    dlen + slen
}
