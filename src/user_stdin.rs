use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

const READ_BUF_SIZE: usize = 1024;
const LINE_BUF_SIZE: usize = 256;

static FOREGROUND_PID: AtomicU64 = AtomicU64::new(0);

struct ReadRing {
    buf:  [u8; READ_BUF_SIZE],
    head: usize,
    tail: usize,
}

impl ReadRing {
    const fn new() -> Self {
        Self { buf: [0; READ_BUF_SIZE], head: 0, tail: 0 }
    }

    fn push(&mut self, byte: u8) {
        let next = (self.tail + 1) % READ_BUF_SIZE;
        if next == self.head { return; }
        self.buf[self.tail] = byte;
        self.tail = next;
    }

    fn pop(&mut self) -> Option<u8> {
        if self.head == self.tail { return None; }
        let b = self.buf[self.head];
        self.head = (self.head + 1) % READ_BUF_SIZE;
        Some(b)
    }

    fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
    }
}

struct LineBuf {
    buf: [u8; LINE_BUF_SIZE],
    len: usize,
}

impl LineBuf {
    const fn new() -> Self {
        Self { buf: [0; LINE_BUF_SIZE], len: 0 }
    }

    fn push(&mut self, b: u8) -> bool {
        if self.len >= LINE_BUF_SIZE { return false; }
        self.buf[self.len] = b;
        self.len += 1;
        true
    }

    fn pop(&mut self) -> bool {
        if self.len == 0 { return false; }
        self.len -= 1;
        true
    }

    fn clear(&mut self) {
        self.len = 0;
    }
}

struct UserStdinInner {
    read_ring: ReadRing,
    line_buf:  LineBuf,
}

impl UserStdinInner {
    const fn new() -> Self {
        Self {
            read_ring: ReadRing::new(),
            line_buf:  LineBuf::new(),
        }
    }
}

static INNER: Mutex<UserStdinInner> = Mutex::new(UserStdinInner::new());

pub fn set_foreground(pid: u64) {
    {
        let mut inner = INNER.lock();
        inner.read_ring.clear();
        inner.line_buf.clear();
    }
    FOREGROUND_PID.store(pid, Ordering::Release);
}

pub fn clear_foreground() {
    FOREGROUND_PID.store(0, Ordering::Release);
}

pub fn foreground_pid() -> u64 {
    FOREGROUND_PID.load(Ordering::Acquire)
}

pub fn is_foreground_active() -> bool {
    foreground_pid() != 0
}

pub fn feed_char(c: char) {
    if !is_foreground_active() { return; }

    match c {
        '\n' => {
            crate::print!("\n");
            let mut inner = INNER.lock();
            for i in 0..inner.line_buf.len {
                let b = inner.line_buf.buf[i];
                inner.read_ring.push(b);
            }
            inner.read_ring.push(b'\n');
            inner.line_buf.clear();
        }
        '\u{0008}' | '\u{007F}' => {
            let mut inner = INNER.lock();
            if inner.line_buf.pop() {
                crate::print!("\u{0008} \u{0008}");
            }
        }
        '\u{0003}' => {
            let pid = foreground_pid();
            if pid != 0 {
                crate::print!("^C\n");
                crate::scheduler::kill(pid);
            }
        }
        c if c >= ' ' && (c as u32) < 127 => {
            let mut inner = INNER.lock();
            if inner.line_buf.push(c as u8) {
                drop(inner);
                crate::print!("{}", c);
            }
        }
        _ => {}
    }
}

pub fn read(buf_ptr: u64, len: u64) -> u64 {
    if buf_ptr == 0 || len == 0 || buf_ptr < 0x1000 || buf_ptr > 0x0000_7FFF_FFFF_FFFF {
        return u64::MAX;
    }

    let max_read = (len as usize).min(READ_BUF_SIZE);

    loop {
        if foreground_pid() == 0 {
            return 0;
        }

        {
            let mut inner = INNER.lock();
            if !inner.read_ring.is_empty() {
                let mut count = 0usize;
                while count < max_read {
                    match inner.read_ring.pop() {
                        Some(b) => {
                            unsafe { *((buf_ptr + count as u64) as *mut u8) = b; }
                            count += 1;
                        }
                        None => break,
                    }
                }
                return count as u64;
            }
        }

        crate::scheduler::sleep(1);
    }
}
