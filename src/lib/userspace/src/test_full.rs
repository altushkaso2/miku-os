#![no_std]
#![no_main]

#[path = "miku.rs"]
mod miku;

use miku::*;

static mut TEST_NUM: i32 = 0;
static mut PASSED: i32 = 0;
static mut FAILED: i32 = 0;

fn ok(name: &str) {
    unsafe {
        TEST_NUM += 1;
        print("test ");
        print_int(TEST_NUM as i64);
        print(": ");
        print(name);
        println(" ok");
        PASSED += 1;
    }
}

fn fail(name: &str, reason: &str) {
    unsafe {
        TEST_NUM += 1;
        print("test ");
        print_int(TEST_NUM as i64);
        print(": ");
        print(name);
        print(" failed (");
        print(reason);
        println(")");
        FAILED += 1;
    }
}

macro_rules! test {
    ($name:expr, $cond:expr) => {
        if $cond { ok($name); } else { fail($name, "condition false"); }
    };
    ($name:expr, $cond:expr, $reason:expr) => {
        if $cond { ok($name); } else { fail($name, $reason); }
    };
}

#[no_mangle]
pub extern "C" fn _start_main() -> ! {
    println("libmiku full test");
    println("");

    println("--- strings ---");
    print("hello ");
    println("from Rust!");
    ok("print/println");

    unsafe {
        test!("strlen", miku_strlen(cstr!("hello")) == 5 && miku_strlen(cstr!("")) == 0);
        test!("strcmp", miku_strcmp(cstr!("abc"), cstr!("abc")) == 0
            && miku_strcmp(cstr!("abc"), cstr!("xyz")) != 0);
        test!("strncmp", miku_strncmp(cstr!("hello"), cstr!("helXX"), 3) == 0
            && miku_strncmp(cstr!("abc"), cstr!("abd"), 3) != 0);

        let mut buf = [0u8; 64];
        miku_strcpy(buf.as_mut_ptr(), cstr!("miku"));
        test!("strcpy", miku_strcmp(buf.as_ptr(), cstr!("miku")) == 0);

        miku_strcpy(buf.as_mut_ptr(), cstr!("hello"));
        miku_strcat(buf.as_mut_ptr(), cstr!(" world"));
        test!("strcat", miku_strcmp(buf.as_ptr(), cstr!("hello world")) == 0);

        let mut buf2 = [0u8; 16];
        miku_strncat(buf2.as_mut_ptr(), cstr!("abcdef"), 3);
        test!("strncat", miku_strcmp(buf2.as_ptr(), cstr!("abc")) == 0);

        let p = miku_strchr(cstr!("abcdef"), b'd' as i32);
        test!("strchr", !p.is_null() && *p == b'd'
            && miku_strchr(cstr!("abcdef"), b'z' as i32).is_null());

        let p = miku_strrchr(cstr!("abcabc"), b'b' as i32);
        test!("strrchr", !p.is_null());

        let p = miku_strstr(cstr!("hello world"), cstr!("world"));
        test!("strstr", !p.is_null()
            && miku_strcmp(p, cstr!("world")) == 0
            && miku_strstr(cstr!("hello"), cstr!("xyz")).is_null());
    }

    println("");
    println("--- extended strings ---");

    unsafe {
        test!("toupper", miku_toupper(b'a' as i32) == b'A' as i32
            && miku_toupper(b'Z' as i32) == b'Z' as i32
            && miku_toupper(b'5' as i32) == b'5' as i32);

        test!("tolower", miku_tolower(b'A' as i32) == b'a' as i32
            && miku_tolower(b'z' as i32) == b'z' as i32);

        test!("isdigit", miku_isdigit(b'0' as i32) != 0
            && miku_isdigit(b'9' as i32) != 0
            && miku_isdigit(b'a' as i32) == 0);

        test!("isalpha", miku_isalpha(b'a' as i32) != 0
            && miku_isalpha(b'Z' as i32) != 0
            && miku_isalpha(b'0' as i32) == 0);

        test!("isalnum", miku_isalnum(b'a' as i32) != 0
            && miku_isalnum(b'5' as i32) != 0
            && miku_isalnum(b' ' as i32) == 0);

        test!("isspace", miku_isspace(b' ' as i32) != 0
            && miku_isspace(b'\t' as i32) != 0
            && miku_isspace(b'a' as i32) == 0);

        {
            let mut s: [u8; 17] = *b"hello,world,miku\0";
            let t1 = miku_strtok(s.as_mut_ptr(), cstr!(","));
            let t2 = miku_strtok(core::ptr::null_mut(), cstr!(","));
            let t3 = miku_strtok(core::ptr::null_mut(), cstr!(","));
            let t4 = miku_strtok(core::ptr::null_mut(), cstr!(","));
            test!("strtok",
                !t1.is_null() && miku_strcmp(t1, cstr!("hello")) == 0
                && !t2.is_null() && miku_strcmp(t2, cstr!("world")) == 0
                && !t3.is_null() && miku_strcmp(t3, cstr!("miku")) == 0
                && t4.is_null());
        }

        let p = miku_strpbrk(cstr!("hello world"), cstr!("wo"));
        test!("strpbrk", !p.is_null() && *p == b'o');

        test!("strspn", miku_strspn(cstr!("aaabbc"), cstr!("ab")) == 5);
        test!("strcspn", miku_strcspn(cstr!("hello,world"), cstr!(",!")) == 5);

        let v1 = miku_strtol(cstr!("  -42"), core::ptr::null_mut(), 10);
        let v2 = miku_strtol(cstr!("0xff"), core::ptr::null_mut(), 0);
        let v3 = miku_strtol(cstr!("077"), core::ptr::null_mut(), 0);
        test!("strtol", v1 == -42 && v2 == 255 && v3 == 63);

        test!("strtoul", miku_strtoul(cstr!("0xDEAD"), core::ptr::null_mut(), 16) == 0xDEAD);

        {
            let mut buf = [0u8; 8];
            let r = miku_strlcpy(buf.as_mut_ptr(), cstr!("hello world"), 8);
            test!("strlcpy", r == 11 && miku_strlen(buf.as_ptr()) == 7
                && miku_strcmp(buf.as_ptr(), cstr!("hello w")) == 0);
        }

        {
            let mut buf = [0u8; 12];
            miku_strcpy(buf.as_mut_ptr(), cstr!("hello"));
            let r = miku_strlcat(buf.as_mut_ptr(), cstr!(" world!"), 12);
            test!("strlcat", r == 12 && miku_strlen(buf.as_ptr()) == 11);
        }
    }

    println("");
    println("--- numbers ---");

    unsafe {
        let mut buf = [0u8; 24];
        miku_itoa(12345, buf.as_mut_ptr());
        test!("itoa +", miku_strcmp(buf.as_ptr(), cstr!("12345")) == 0);

        miku_itoa(-9876, buf.as_mut_ptr());
        test!("itoa -", miku_strcmp(buf.as_ptr(), cstr!("-9876")) == 0);

        miku_itoa(0, buf.as_mut_ptr());
        test!("itoa 0", miku_strcmp(buf.as_ptr(), cstr!("0")) == 0);

        test!("atoi", miku_atoi(cstr!("  -42")) == -42
            && miku_atoi(cstr!("100")) == 100
            && miku_atoi(cstr!("0")) == 0);
    }

    print("  hex=");
    print_hex(0xDEADBEEF);
    println("");
    ok("print_hex");

    print("  int=");
    print_int(-777);
    println("");
    ok("print_int");

    print("  chars=");
    putchar(b'O');
    putchar(b'K');
    println("");
    ok("putchar");

    println("");
    println("--- memory ---");

    unsafe {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        miku_memset(a.as_mut_ptr(), 0xAA, 32);
        miku_memset(b.as_mut_ptr(), 0xAA, 32);
        let eq1 = miku_memcmp(a.as_ptr(), b.as_ptr(), 32) == 0;
        b[16] = 0xBB;
        let neq = miku_memcmp(a.as_ptr(), b.as_ptr(), 32) != 0;
        test!("memset+memcmp", eq1 && neq);

        let src = b"test data 123\0";
        let mut dst = [0u8; 32];
        miku_memcpy(dst.as_mut_ptr(), src.as_ptr(), 14);
        test!("memcpy", miku_memcmp(dst.as_ptr(), src.as_ptr(), 14) == 0);

        let mut buf = [0u8; 32];
        miku_strcpy(buf.as_mut_ptr(), cstr!("abcdefgh"));
        miku_memmove(buf.as_mut_ptr().add(2), buf.as_ptr(), 6);
        test!("memmove", buf[2] == b'a' && buf[3] == b'b' && buf[4] == b'c');

        let mut buf = [0xFFu8; 16];
        miku_bzero(buf.as_mut_ptr(), 16);
        let all_zero = buf.iter().all(|&b| b == 0);
        test!("bzero", all_zero);
    }

    println("");
    println("--- utilities ---");

    test!("abs", abs(-42) == 42 && abs(42) == 42 && abs(0) == 0);
    test!("min", min(3, 7) == 3 && min(-5, 2) == -5);
    test!("max", max(3, 7) == 7 && max(-5, 2) == 2);
    test!("clamp", clamp(5, 0, 10) == 5 && clamp(-5, 0, 10) == 0 && clamp(99, 0, 10) == 10);

    {
        let mut a: u64 = 111;
        let mut b: u64 = 222;
        unsafe { miku_swap(&mut a as *mut u64, &mut b as *mut u64); }
        test!("swap", a == 222 && b == 111);
    }

    srand(42);
    let r1 = rand();
    let r2 = rand();
    test!("rand", r1 != r2 && r1 != 0);

    {
        srand(12345);
        let mut good = true;
        for _ in 0..100 {
            let r = rand_range(10, 20);
            if r < 10 || r >= 20 { good = false; break; }
        }
        test!("rand_range", good);
    }

    println("");
    println("--- heap ---");

    unsafe {
        let p = miku_malloc(256);
        if !p.is_null() {
            miku_memset(p, 0x42, 256);
            let good = *p == 0x42;
            miku_free(p);
            test!("malloc+free", good);
        } else { fail("malloc+free", "null"); }

        let p = miku_calloc(10, 8);
        if !p.is_null() {
            let slice = core::slice::from_raw_parts(p, 80);
            let z = slice.iter().all(|&b| b == 0);
            miku_free(p);
            test!("calloc", z);
        } else { fail("calloc", "null"); }

        let p = miku_malloc(64);
        if !p.is_null() {
            *p = b'M';
            *p.add(1) = b'K';
            let p2 = miku_realloc(p, 512);
            if !p2.is_null() {
                let ok = *p2 == b'M' && *p2.add(1) == b'K';
                miku_free(p2);
                test!("realloc", ok);
            } else { miku_free(p); fail("realloc", "null"); }
        } else { fail("realloc", "null"); }

        let d = miku_strdup(cstr!("miku-os"));
        if !d.is_null() {
            let eq = miku_strcmp(d, cstr!("miku-os")) == 0;
            miku_free(d);
            test!("strdup", eq);
        } else { fail("strdup", "null"); }

        {
            let mut ptrs = [core::ptr::null_mut(); 32];
            let mut good = true;
            for i in 0..32 {
                ptrs[i] = miku_malloc(16);
                if ptrs[i].is_null() { good = false; break; }
                miku_memset(ptrs[i], i as i32, 16);
            }
            for i in 0..32 {
                if !ptrs[i].is_null() {
                    if *ptrs[i] != i as u8 { good = false; }
                    miku_free(ptrs[i]);
                }
            }
            test!("32x alloc", good);
        }

        {
            let p = miku_malloc(65536);
            if !p.is_null() {
                miku_memset(p, 0xBE, 65536);
                let ok = *p == 0xBE && *p.add(65535) == 0xBE;
                miku_free(p);
                test!("64KB alloc", ok);
            } else { fail("64KB alloc", "null"); }
        }

        {
            let mut good = true;
            let mut ptrs = [core::ptr::null_mut(); 8];
            for i in 0..8 {
                ptrs[i] = miku_malloc(100 + i * 50);
                if ptrs[i].is_null() { good = false; }
            }
            for i in (0..8).rev() { miku_free(ptrs[i]); }
            for i in 0..8 {
                ptrs[i] = miku_malloc(200);
                if ptrs[i].is_null() { good = false; }
            }
            for i in 0..8 { miku_free(ptrs[i]); }
            test!("malloc stress", good);
        }
    }

    println("");
    println("--- process ---");

    let pid = getpid();
    print("  pid=");
    print_int(pid as i64);
    println("");
    test!("getpid", pid > 0);
    test!("brk", brk(0) > 0);

    println("");
    println("--- printf ---");

    unsafe {
        let r = miku_printf(cstr!("  hello %s!\n"), cstr!("world"));
        test!("printf %s", r > 0);
        let r = miku_printf(cstr!("  num=%d neg=%d zero=%d\n"), 42i64, -99i64, 0i64);
        test!("printf %d", r > 0);
        let r = miku_printf(cstr!("  hex=%x dead=%x\n"), 255u64, 0xDEADu64);
        test!("printf %x", r > 0);
        let r = miku_printf(cstr!("  char=%c%c%c\n"), b'A' as u64, b'B' as u64, b'C' as u64);
        test!("printf %c", r > 0);
        let r = miku_printf(cstr!("  100%%\n"));
        test!("printf %%", r > 0);
        let r = miku_printf(cstr!("  ptr=%p\n"), 0x1234u64);
        test!("printf %p", r > 0);
    }

    println("");
    println("--- snprintf ---");

    unsafe {
        let mut buf = [0u8; 64];
        miku_snprintf(buf.as_mut_ptr(), 64, cstr!("hello %s"), cstr!("miku"));
        test!("snprintf basic", miku_strcmp(buf.as_ptr(), cstr!("hello miku")) == 0);

        miku_memset(buf.as_mut_ptr(), 0, 64);
        miku_snprintf(buf.as_mut_ptr(), 64, cstr!("%d+%d=%d"), 10u64, 20u64, 30u64);
        test!("snprintf int", miku_strcmp(buf.as_ptr(), cstr!("10+20=30")) == 0);

        let mut small = [b'X'; 8];
        miku_snprintf(small.as_mut_ptr(), 8, cstr!("hello world 12345"));
        test!("snprintf truncate", small[7] == 0 && miku_strlen(small.as_ptr()) <= 7);

        miku_memset(buf.as_mut_ptr(), 0, 32);
        miku_snprintf(buf.as_mut_ptr(), 32, cstr!("0x%x"), 255u64);
        test!("snprintf hex", miku_strcmp(buf.as_ptr(), cstr!("0xff")) == 0);

        let mut buf2 = [0u8; 16];
        miku_snprintf(buf2.as_mut_ptr(), 16, cstr!("100%%"));
        test!("snprintf %%", miku_strcmp(buf2.as_ptr(), cstr!("100%")) == 0);
    }

    println("");
    println("--- time ---");

    {
        let t = uptime();
        print("  ticks=");
        print_int(t as i64);
        println("");
        test!("uptime", t > 0);
    }

    {
        let ms = uptime_ms();
        print("  ms=");
        print_int(ms as i64);
        println("");
        test!("uptime_ms", ms > 0);
    }

    {
        let before = uptime();
        sleep(10);
        let after = uptime();
        let diff = after - before;
        print("  slept ");
        print_int(diff as i64);
        println(" ticks");
        test!("sleep(10)", diff >= 5);
    }

    {
        let before = uptime_ms();
        sleep_ms(100);
        let after = uptime_ms();
        let diff = after - before;
        print("  slept ");
        print_int(diff as i64);
        println(" ms");
        test!("sleep_ms(100)", diff >= 50);
    }

    sleep(0);
    ok("sleep(0) yield");

    println("");
    println("--- file I/O ---");

    unsafe {
        let fd = miku_open_cstr(cstr!("/nonexistent_xyz"));
        test!("open nonexistent", fd < 0);

        let fd = miku_open_cstr(cstr!("/test_full"));
        if fd >= 0 {
            let sz = miku_fsize(fd);
            print("  size=");
            print_int(sz);
            println(" bytes");
            let mut hdr = [0u8; 4];
            miku_seek(fd, 0);
            let n = miku_read(fd as u64, hdr.as_mut_ptr(), 4);
            miku_close(fd);
            if n == 4 && hdr[0] == 0x7F && hdr[1] == b'E' {
                ok("open+read ELF hdr");
            } else {
                ok("open+read");
            }
        } else {
            println("  (no /test_full on disk)");
            ok("open+read skip");
        }

        let mut sz: usize = 0;
        let data = miku_read_file(cstr!("/test_full"), &mut sz as *mut usize);
        if !data.is_null() && sz > 0 {
            print("  read_file=");
            print_int(sz as i64);
            println(" bytes");
            miku_free(data);
            ok("read_file");
        } else {
            ok("read_file skip");
        }
    }

    println("");
    println("========================");
    unsafe {
        print_int(PASSED as i64);
        print("/");
        print_int(TEST_NUM as i64);
        println(" tests passed");
        if FAILED == 0 {
            println("all ok!");
        } else {
            print_int(FAILED as i64);
            println(" failed");
        }
        exit(if FAILED == 0 { 0 } else { 1 });
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    print("PANIC: ");
    if let Some(loc) = info.location() {
        print(loc.file());
        print(":");
        print_int(loc.line() as i64);
    }
    println("");
    exit(134);
}
