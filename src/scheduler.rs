use crate::process::{Context, Process, ProcessState};
use spin::Mutex;

const MAX_PROCS: usize = 16;

pub struct Scheduler {
    procs:   [Option<Process>; MAX_PROCS],
    current: usize,
    count:   usize,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            procs:   [None, None, None, None, None, None, None, None,
                      None, None, None, None, None, None, None, None],
            current: 0,
            count:   0,
        }
    }

    pub fn add(&mut self, p: Process) -> bool {
        for slot in self.procs.iter_mut() {
            if slot.is_none() {
                *slot = Some(p);
                self.count += 1;
                return true;
            }
        }
        false
    }

    pub fn current_context_mut(&mut self) -> Option<&mut Context> {
        match self.procs[self.current].as_mut() {
            Some(p) if p.started => Some(&mut p.context),
            _ => None,
        }
    }

    pub fn next(&mut self) -> Option<&mut Context> {
        let start = self.current;
        let mut i = (self.current + 1) % MAX_PROCS;
        let mut found = None;

        loop {
            if let Some(ref p) = self.procs[i] {
                if p.state == ProcessState::Ready || p.state == ProcessState::Running {
                    found = Some(i);
                    break;
                }
            }
            i = (i + 1) % MAX_PROCS;
            if i == start {
                break;
            }
        }

        if let Some(next_i) = found {
            if let Some(ref mut prev) = self.procs[self.current] {
                if prev.state == ProcessState::Running {
                    prev.state = ProcessState::Ready;
                }
            }
            self.current = next_i;
            if let Some(ref mut p) = self.procs[next_i] {
                p.state = ProcessState::Running;
                p.started = true;
                return Some(&mut p.context);
            }
        }

        None
    }
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

pub fn spawn(entry: fn() -> !) {
    let p = Process::new_kernel(entry);
    SCHEDULER.lock().add(p);
}

#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(old: *mut Context, new: *const Context) {
    core::arch::naked_asm!(
        "mov [rdi + 0x00], r15",
        "mov [rdi + 0x08], r14",
        "mov [rdi + 0x10], r13",
        "mov [rdi + 0x18], r12",
        "mov [rdi + 0x20], rbx",
        "mov [rdi + 0x28], rbp",
        "lea rax, [rip + 1f]",
        "mov [rdi + 0x30], rax",
        "mov [rdi + 0x38], rsp",
        "pushfq",
        "pop qword ptr [rdi + 0x40]",

        "mov r15, [rsi + 0x00]",
        "mov r14, [rsi + 0x08]",
        "mov r13, [rsi + 0x10]",
        "mov r12, [rsi + 0x18]",
        "mov rbx, [rsi + 0x20]",
        "mov rbp, [rsi + 0x28]",
        "mov rsp, [rsi + 0x38]",
        "push qword ptr [rsi + 0x40]",
        "popfq",
        "jmp qword ptr [rsi + 0x30]",
        "1:",
        "ret",
    );
}

pub fn schedule() {
    let (old_ctx, new_ctx) = {
        let mut sched = SCHEDULER.lock();
        let old = sched.current_context_mut().map(|c| c as *mut Context);
        let new = sched.next().map(|c| c as *const Context);
        (old, new)
    };

    match (old_ctx, new_ctx) {
        (Some(old), Some(new)) if old != new as *mut Context => {
            unsafe { switch_context(old, new) };
        }
        (None, Some(new)) => {
            unsafe { jump_to_context(new) };
        }
        _ => {}
    }
}

#[unsafe(naked)]
unsafe extern "C" fn jump_to_context(ctx: *const Context) {
    core::arch::naked_asm!(
        "mov r15, [rdi + 0x00]",
        "mov r14, [rdi + 0x08]",
        "mov r13, [rdi + 0x10]",
        "mov r12, [rdi + 0x18]",
        "mov rbx, [rdi + 0x20]",
        "mov rbp, [rdi + 0x28]",
        "mov rsp, [rdi + 0x38]",
        "push qword ptr [rdi + 0x40]",
        "popfq",
        "jmp qword ptr [rdi + 0x30]",
    );
}
