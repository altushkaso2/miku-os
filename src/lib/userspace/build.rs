use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let stub_c = out_dir.join("miku_stub.c");
    let stub_so = out_dir.join("libmiku.so");

    std::fs::write(&stub_c, r#"
void miku_println(const char *s) {}
void miku_print(const char *s) {}
long miku_write(unsigned long fd, const char *buf, unsigned long len) { return 0; }
void miku_exit(long code) { for(;;); }
void miku_itoa(long val, char *buf) {}
long miku_atoi(const char *s) { return 0; }
void miku_utoa(unsigned long val, char *buf) {}
unsigned long miku_strlen(const char *s) { return 0; }
int miku_strcmp(const char *a, const char *b) { return 0; }
int miku_strncmp(const char *a, const char *b, unsigned long n) { return 0; }
char *miku_strcpy(char *d, const char *s) { return d; }
char *miku_strncpy(char *d, const char *s, unsigned long n) { return d; }
char *miku_strcat(char *d, const char *s) { return d; }
char *miku_strncat(char *d, const char *s, unsigned long n) { return d; }
const char *miku_strchr(const char *s, int c) { return 0; }
const char *miku_strrchr(const char *s, int c) { return 0; }
const char *miku_strstr(const char *h, const char *n) { return 0; }
char *miku_strdup(const char *s) { return 0; }
int miku_toupper(int c) { return c; }
int miku_tolower(int c) { return c; }
int miku_isdigit(int c) { return 0; }
int miku_isalpha(int c) { return 0; }
int miku_isalnum(int c) { return 0; }
int miku_isspace(int c) { return 0; }
char *miku_strtok(char *s, const char *d) { return 0; }
const char *miku_strpbrk(const char *s, const char *a) { return 0; }
unsigned long miku_strspn(const char *s, const char *a) { return 0; }
unsigned long miku_strcspn(const char *s, const char *r) { return 0; }
long miku_strtol(const char *s, const char **e, int b) { return 0; }
unsigned long miku_strtoul(const char *s, const char **e, int b) { return 0; }
unsigned long miku_strlcpy(char *d, const char *s, unsigned long n) { return 0; }
unsigned long miku_strlcat(char *d, const char *s, unsigned long n) { return 0; }
void *miku_memset(void *d, int v, unsigned long n) { return d; }
void *miku_memcpy(void *d, const void *s, unsigned long n) { return d; }
void *miku_memmove(void *d, const void *s, unsigned long n) { return d; }
int miku_memcmp(const void *a, const void *b, unsigned long n) { return 0; }
void miku_bzero(void *d, unsigned long n) {}
long miku_abs(long x) { return x; }
long miku_min(long a, long b) { return a; }
long miku_max(long a, long b) { return a; }
long miku_clamp(long v, long lo, long hi) { return v; }
void miku_swap(unsigned long *a, unsigned long *b) {}
void miku_srand(unsigned long s) {}
unsigned long miku_rand(void) { return 0; }
unsigned long miku_rand_range(unsigned long lo, unsigned long hi) { return lo; }
void miku_assert_fail(const char *e, const char *f, int l) {}
void miku_panic(const char *m) { for(;;); }
void *miku_malloc(unsigned long s) { return 0; }
void miku_free(void *p) {}
void *miku_realloc(void *p, unsigned long s) { return 0; }
void *miku_calloc(unsigned long c, unsigned long s) { return 0; }
unsigned long miku_getpid(void) { return 0; }
unsigned long miku_uptime(void) { return 0; }
unsigned long miku_uptime_ms(void) { return 0; }
void miku_sleep(unsigned long t) {}
void miku_sleep_ms(unsigned long t) {}
void miku_print_int(long v) {}
void miku_print_hex(unsigned long v) {}
int miku_putchar(int c) { return c; }
int miku_getchar(void) { return -1; }
int miku_puts(const char *s) { return 0; }
long miku_open_cstr(const char *p) { return -1; }
long miku_open(const char *p, unsigned long l) { return -1; }
long miku_close(long fd) { return 0; }
long miku_fsize(long fd) { return 0; }
long miku_read(unsigned long fd, void *b, unsigned long l) { return 0; }
long miku_seek(long fd, unsigned long o) { return 0; }
void *miku_read_file(const char *p, unsigned long *s) { return 0; }
int miku_readline(char *b, unsigned long m) { return -1; }
char *miku_getline(void) { return 0; }
unsigned long miku_brk(unsigned long a) { return 0; }
int miku_snprintf(char *b, unsigned long m, const char *f, ...) { return 0; }
int miku_printf(const char *f, ...) { return 0; }
void *miku_mmap(unsigned long a, unsigned long l, unsigned long p) { return 0; }
long miku_munmap(void *a, unsigned long l) { return 0; }
long miku_mprotect(unsigned long a, unsigned long l, unsigned long p) { return 0; }
char *miku_getcwd(char *b, unsigned long s) { return 0; }
long miku_set_tls(unsigned long a) { return 0; }
unsigned long miku_get_tls(void) { return 0; }
long miku_map_lib(const char *n, unsigned long l) { return 0; }
"#).unwrap();

    let status = Command::new("gcc")
        .args(["-shared", "-nostdlib", "-fPIC",
               "-Wl,-soname,libmiku.so",
               "-o", stub_so.to_str().unwrap(),
               stub_c.to_str().unwrap()])
        .status()
        .expect("gcc failed");
    assert!(status.success(), "Failed to build libmiku stub");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=dylib=miku");
}
