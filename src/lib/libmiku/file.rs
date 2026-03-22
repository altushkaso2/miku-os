use crate::sys::*;

#[no_mangle]
pub extern "C" fn miku_open(path: *const u8, path_len: usize) -> i64 {
    unsafe { sc2(SYS_OPEN, path as u64, path_len as u64) }
}

#[no_mangle]
pub extern "C" fn miku_open_cstr(path: *const u8) -> i64 {
    if path.is_null() { return -22; }
    let len = crate::string::miku_strlen(path);
    miku_open(path, len)
}

#[no_mangle]
pub extern "C" fn miku_close(fd: i64) -> i64 {
    unsafe { sc1(SYS_CLOSE, fd as u64) }
}

#[no_mangle]
pub extern "C" fn miku_seek(fd: i64, offset: u64) -> i64 {
    unsafe { sc2(SYS_SEEK, fd as u64, offset) }
}

#[no_mangle]
pub extern "C" fn miku_fsize(fd: i64) -> i64 {
    unsafe { sc1(SYS_FSIZE, fd as u64) }
}

#[no_mangle]
pub extern "C" fn miku_read_file(path: *const u8, out_size: *mut usize) -> *mut u8 {
    if path.is_null() { return core::ptr::null_mut(); }
    let fd = miku_open_cstr(path);
    if fd < 0 { return core::ptr::null_mut(); }
    let size = miku_fsize(fd);
    if size <= 0 { miku_close(fd); return core::ptr::null_mut(); }
    let buf = crate::heap::miku_malloc(size as usize + 1);
    if buf.is_null() { miku_close(fd); return core::ptr::null_mut(); }
    miku_seek(fd, 0);
    let mut done = 0usize;
    while done < size as usize {
        let n = crate::io::miku_read(fd as u64, unsafe { buf.add(done) }, size as usize - done);
        if n <= 0 { break; }
        done += n as usize;
    }
    miku_close(fd);
    unsafe { *buf.add(done) = 0; }
    if !out_size.is_null() { unsafe { *out_size = done; } }
    buf
}
