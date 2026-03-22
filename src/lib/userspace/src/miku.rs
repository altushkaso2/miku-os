#![allow(dead_code)]

#[link(name = "miku")]
extern "C" {
    pub fn miku_exit(code: i64) -> !;
    pub fn miku_write(fd: u64, buf: *const u8, len: usize) -> i64;
    pub fn miku_read(fd: u64, buf: *mut u8, len: usize) -> i64;
    pub fn miku_print(s: *const u8);
    pub fn miku_println(s: *const u8);
    pub fn miku_puts(s: *const u8) -> i32;
    pub fn miku_putchar(c: i32) -> i32;
    pub fn miku_getchar() -> i32;
    pub fn miku_print_int(val: i64);
    pub fn miku_print_hex(val: u64);
    pub fn miku_printf(fmt: *const u8, ...) -> i32;
    pub fn miku_snprintf(buf: *mut u8, max: usize, fmt: *const u8, ...) -> i32;

    pub fn miku_strlen(s: *const u8) -> usize;
    pub fn miku_strcmp(a: *const u8, b: *const u8) -> i32;
    pub fn miku_strncmp(a: *const u8, b: *const u8, n: usize) -> i32;
    pub fn miku_strcpy(dst: *mut u8, src: *const u8) -> *mut u8;
    pub fn miku_strncpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8;
    pub fn miku_strcat(dst: *mut u8, src: *const u8) -> *mut u8;
    pub fn miku_strncat(dst: *mut u8, src: *const u8, n: usize) -> *mut u8;
    pub fn miku_strchr(s: *const u8, c: i32) -> *const u8;
    pub fn miku_strrchr(s: *const u8, c: i32) -> *const u8;
    pub fn miku_strstr(h: *const u8, n: *const u8) -> *const u8;
    pub fn miku_strdup(s: *const u8) -> *mut u8;
    pub fn miku_toupper(c: i32) -> i32;
    pub fn miku_tolower(c: i32) -> i32;
    pub fn miku_isdigit(c: i32) -> i32;
    pub fn miku_isalpha(c: i32) -> i32;
    pub fn miku_isalnum(c: i32) -> i32;
    pub fn miku_isspace(c: i32) -> i32;
    pub fn miku_strtok(s: *mut u8, delim: *const u8) -> *mut u8;
    pub fn miku_strpbrk(s: *const u8, accept: *const u8) -> *const u8;
    pub fn miku_strspn(s: *const u8, accept: *const u8) -> usize;
    pub fn miku_strcspn(s: *const u8, reject: *const u8) -> usize;
    pub fn miku_strtol(s: *const u8, endptr: *mut *const u8, base: i32) -> i64;
    pub fn miku_strtoul(s: *const u8, endptr: *mut *const u8, base: i32) -> u64;
    pub fn miku_strlcpy(dst: *mut u8, src: *const u8, size: usize) -> usize;
    pub fn miku_strlcat(dst: *mut u8, src: *const u8, size: usize) -> usize;
    pub fn miku_itoa(val: i64, buf: *mut u8);
    pub fn miku_utoa(val: u64, buf: *mut u8);
    pub fn miku_atoi(s: *const u8) -> i64;

    pub fn miku_memset(dst: *mut u8, val: i32, n: usize) -> *mut u8;
    pub fn miku_memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8;
    pub fn miku_memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8;
    pub fn miku_memcmp(a: *const u8, b: *const u8, n: usize) -> i32;
    pub fn miku_bzero(dst: *mut u8, n: usize);

    pub fn miku_abs(x: i64) -> i64;
    pub fn miku_min(a: i64, b: i64) -> i64;
    pub fn miku_max(a: i64, b: i64) -> i64;
    pub fn miku_clamp(val: i64, lo: i64, hi: i64) -> i64;
    pub fn miku_swap(a: *mut u64, b: *mut u64);
    pub fn miku_srand(seed: u64);
    pub fn miku_rand() -> u64;
    pub fn miku_rand_range(lo: u64, hi: u64) -> u64;
    pub fn miku_assert_fail(expr: *const u8, file: *const u8, line: i32);
    pub fn miku_panic(msg: *const u8) -> !;

    pub fn miku_malloc(size: usize) -> *mut u8;
    pub fn miku_free(ptr: *mut u8);
    pub fn miku_realloc(ptr: *mut u8, new_size: usize) -> *mut u8;
    pub fn miku_calloc(count: usize, size: usize) -> *mut u8;

    pub fn miku_open(path: *const u8, len: usize) -> i64;
    pub fn miku_open_cstr(path: *const u8) -> i64;
    pub fn miku_close(fd: i64) -> i64;
    pub fn miku_seek(fd: i64, offset: u64) -> i64;
    pub fn miku_fsize(fd: i64) -> i64;
    pub fn miku_read_file(path: *const u8, out_size: *mut usize) -> *mut u8;
    pub fn miku_readline(buf: *mut u8, max_len: usize) -> i32;
    pub fn miku_getline() -> *mut u8;

    pub fn miku_getpid() -> u64;
    pub fn miku_getcwd(buf: *mut u8, size: usize) -> *mut u8;
    pub fn miku_brk(addr: u64) -> u64;
    pub fn miku_sleep(ticks: u64);
    pub fn miku_sleep_ms(ms: u64);
    pub fn miku_uptime() -> u64;
    pub fn miku_uptime_ms() -> u64;

    pub fn miku_mmap(addr: u64, len: usize, prot: u64) -> *mut u8;
    pub fn miku_munmap(addr: *mut u8, len: usize) -> i64;
    pub fn miku_mprotect(addr: u64, len: usize, prot: u64) -> i64;
    pub fn miku_set_tls(addr: u64) -> i64;
    pub fn miku_get_tls() -> u64;
    pub fn miku_map_lib(name: *const u8, name_len: usize) -> i64;
}

pub fn exit(code: i64) -> ! {
    unsafe { miku_exit(code) }
}

pub fn print(s: &str) {
    unsafe { miku_write(1, s.as_ptr(), s.len()); }
}

pub fn println(s: &str) {
    print(s);
    print("\n");
}

pub fn print_int(val: i64) {
    unsafe { miku_print_int(val); }
}

pub fn print_hex(val: u64) {
    unsafe { miku_print_hex(val); }
}

pub fn putchar(c: u8) {
    unsafe { miku_putchar(c as i32); }
}

pub fn getchar() -> Option<u8> {
    let r = unsafe { miku_getchar() };
    if r < 0 { None } else { Some(r as u8) }
}

pub fn sleep(ticks: u64) {
    unsafe { miku_sleep(ticks); }
}

pub fn sleep_ms(ms: u64) {
    unsafe { miku_sleep_ms(ms); }
}

pub fn uptime() -> u64 {
    unsafe { miku_uptime() }
}

pub fn uptime_ms() -> u64 {
    unsafe { miku_uptime_ms() }
}

pub fn getpid() -> u64 {
    unsafe { miku_getpid() }
}

pub fn brk(addr: u64) -> u64 {
    unsafe { miku_brk(addr) }
}

pub fn abs(x: i64) -> i64 {
    unsafe { miku_abs(x) }
}

pub fn min(a: i64, b: i64) -> i64 {
    unsafe { miku_min(a, b) }
}

pub fn max(a: i64, b: i64) -> i64 {
    unsafe { miku_max(a, b) }
}

pub fn clamp(val: i64, lo: i64, hi: i64) -> i64 {
    unsafe { miku_clamp(val, lo, hi) }
}

pub fn rand() -> u64 {
    unsafe { miku_rand() }
}

pub fn srand(seed: u64) {
    unsafe { miku_srand(seed); }
}

pub fn rand_range(lo: u64, hi: u64) -> u64 {
    unsafe { miku_rand_range(lo, hi) }
}

pub fn strlen(s: &[u8]) -> usize {
    unsafe { miku_strlen(s.as_ptr()) }
}

pub fn streq(a: &[u8], b: &[u8]) -> bool {
    unsafe { miku_strcmp(a.as_ptr(), b.as_ptr()) == 0 }
}

pub unsafe fn malloc(size: usize) -> *mut u8 {
    miku_malloc(size)
}

pub unsafe fn free(ptr: *mut u8) {
    miku_free(ptr)
}

pub unsafe fn realloc(ptr: *mut u8, new_size: usize) -> *mut u8 {
    miku_realloc(ptr, new_size)
}

pub unsafe fn calloc(count: usize, size: usize) -> *mut u8 {
    miku_calloc(count, size)
}

pub fn open(path: &str) -> Result<i64, i64> {
    let fd = unsafe { miku_open(path.as_ptr(), path.len()) };
    if fd < 0 { Err(fd) } else { Ok(fd) }
}

pub fn close(fd: i64) {
    unsafe { miku_close(fd); }
}

pub fn fsize(fd: i64) -> i64 {
    unsafe { miku_fsize(fd) }
}

pub fn seek(fd: i64, offset: u64) {
    unsafe { miku_seek(fd, offset); }
}

pub fn read(fd: i64, buf: &mut [u8]) -> i64 {
    unsafe { miku_read(fd as u64, buf.as_mut_ptr(), buf.len()) }
}

pub fn read_file(path: &str) -> Option<(*mut u8, usize)> {
    let mut size: usize = 0;
    let mut p = [0u8; 256];
    let len = path.len().min(255);
    p[..len].copy_from_slice(&path.as_bytes()[..len]);
    p[len] = 0;
    let ptr = unsafe { miku_read_file(p.as_ptr(), &mut size as *mut usize) };
    if ptr.is_null() || size == 0 { None } else { Some((ptr, size)) }
}

pub fn write(fd: u64, data: &[u8]) -> i64 {
    unsafe { miku_write(fd, data.as_ptr(), data.len()) }
}

#[macro_export]
macro_rules! cstr {
    ($s:expr) => { concat!($s, "\0").as_ptr() }
}

core::arch::global_asm!(
    ".global _start",
    "_start:",
    "and rsp, -16",
    "call _start_main",
    "ud2",
);
