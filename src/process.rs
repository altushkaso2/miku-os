use core::sync::atomic::{AtomicU64, Ordering};

static NEXT_PID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessState {
    Running,
    Ready,
    Dead,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    pub r15:    u64,
    pub r14:    u64,
    pub r13:    u64,
    pub r12:    u64,
    pub rbx:    u64,
    pub rbp:    u64,
    pub rip:    u64,
    pub rsp:    u64,
    pub rflags: u64,
}

impl Context {
    pub const fn zero() -> Self {
        Self {
            r15: 0, r14: 0, r13: 0, r12: 0,
            rbx: 0, rbp: 0, rip: 0, rsp: 0,
            rflags: 0x202,
        }
    }
}

const STACK_SIZE: usize = 16384;

pub struct Process {
    pub pid:     u64,
    pub state:   ProcessState,
    pub context: Context,
    pub stack:   [u8; STACK_SIZE],
    pub started: bool,
}

impl Process {
    pub fn new_kernel(entry: fn() -> !) -> Self {
        let mut p = Self {
            pid:     NEXT_PID.fetch_add(1, Ordering::SeqCst),
            state:   ProcessState::Ready,
            context: Context::zero(),
            stack:   [0u8; STACK_SIZE],
            started: false,
        };
        let stack_top = p.stack.as_ptr() as u64 + STACK_SIZE as u64;
        p.context.rip = entry as u64;
        p.context.rsp = stack_top;
        p
    }
}
