extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec::Vec;
use spin::Mutex;
use x86_64::instructions::interrupts;

use crate::process::{Context, Process, ProcessState};

pub trait Task: Send {
    fn run(self: Box<Self>);
}

impl<F: FnOnce() + Send> Task for F {
    fn run(self: Box<Self>) {
        (*self)()
    }
}

static WORK_QUEUE: Mutex<VecDeque<Box<dyn Task>>> = Mutex::new(VecDeque::new());

pub fn submit_task<F: FnOnce() + Send + 'static>(f: F) {
    WORK_QUEUE.lock().push_back(Box::new(f));
}

fn worker_loop() -> ! {
    x86_64::instructions::interrupts::enable();
    loop {
        let task = interrupts::without_interrupts(|| WORK_QUEUE.lock().pop_front());
        match task {
            Some(t) => t.run(),
            None    => sleep(5),
        }
    }
}

pub fn init_workers(count: usize) {
    for _ in 0..count {
        spawn_named(worker_loop, "worker", 1);
    }
    crate::serial_println!("[sched] {} worker threads started", count);
}

pub struct Scheduler {
    pub procs:        BTreeMap<u64, Box<Process>>,
    pub ready_queue:  BTreeSet<(u64, u64)>,
    pub sleep_queue:  BTreeSet<(u64, u64)>,
    pub current:      Option<u64>,
    pub min_vruntime: u64,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            procs:        BTreeMap::new(),
            ready_queue:  BTreeSet::new(),
            sleep_queue:  BTreeSet::new(),
            current:      None,
            min_vruntime: 0,
        }
    }

    pub fn init_main_thread(&mut self) {
        let cr3 = crate::vmm::kernel_cr3();
        let stack = alloc::vec![0u8; crate::process::DEFAULT_STACK_SIZE].into_boxed_slice();
        let p = Box::new(Process {
            pid:             0,
            name:            "kernel-main",
            state:           ProcessState::Running,
            context:         Context::zero(),
            stack,
            started:         true,
            cr3,
            priority:        2,
            cpu_time:        0,
            vruntime:        0,
            user_stack_phys: None,
        });
        self.procs.insert(0, p);
        self.current = Some(0);
    }

    pub fn add(&mut self, mut p: Box<Process>) {
        let pid = p.pid;
        let name = p.name;
        p.vruntime = self.min_vruntime;
        self.ready_queue.insert((p.vruntime, pid));
        self.procs.insert(pid, p);
        crate::serial_println!("[sched] thread '{}' pid={} added", name, pid);
    }

    pub fn next(&mut self, current_tick: u64) -> Option<*const Context> {
        let dead: Vec<u64> = self.procs.iter()
            .filter(|(_, p)| p.state == ProcessState::Dead)
            .map(|(pid, _)| *pid)
            .collect();

        for pid in dead {
            if let Some(mut p) = self.procs.remove(&pid) {
                self.ready_queue.remove(&(p.vruntime, pid));
                if let Some(phys) = p.user_stack_phys.take() {
                    crate::pmm::free_frames(phys, crate::process::USER_STACK_PAGES);
                }
                if p.cr3 != 0 && p.cr3 != crate::vmm::kernel_cr3() {
                    let mut aspace = crate::vmm::AddressSpace { cr3: p.cr3 };
                    aspace.free_address_space();
                    p.cr3 = 0;
                }
                crate::serial_println!("[sched] thread pid={} collected", pid);
            }
        }

        let to_wake: Vec<(u64, u64)> = self.sleep_queue.iter()
            .take_while(|&&(wake, _)| current_tick >= wake)
            .copied()
            .collect();

        for key in to_wake {
            self.sleep_queue.remove(&key);
            let pid = key.1;
            if let Some(p) = self.procs.get_mut(&pid) {
                p.state = ProcessState::Ready;
                p.vruntime = p.vruntime.max(self.min_vruntime);
                self.ready_queue.insert((p.vruntime, pid));
            }
        }

        let mut do_preempt = false;

        if let Some(curr_pid) = self.current {
            if let Some(p) = self.procs.get_mut(&curr_pid) {
                match p.state {
                    ProcessState::Sleeping(wake) => {
                        self.sleep_queue.insert((wake, curr_pid));
                    }
                    ProcessState::Dead | ProcessState::Ready => {}
                    ProcessState::Running => {
                        p.cpu_time += 1;
                        let weight = p.priority.max(1) as u64;
                        p.vruntime += 10 / weight;
                        if let Some(&(next_v, _)) = self.ready_queue.iter().next() {
                            if p.vruntime > next_v {
                                p.state = ProcessState::Ready;
                                let vr = p.vruntime;
                                let pid = curr_pid;
                                self.ready_queue.insert((vr, pid));
                                do_preempt = true;
                            }
                        }
                    }
                }
            }
        }

        let need_switch = match self.current {
            None => true,
            Some(pid) => match self.procs.get(&pid).map(|p| p.state) {
                Some(ProcessState::Running) => do_preempt,
                _ => true,
            },
        };

        if !need_switch {
            return None;
        }

        if let Some(&(next_v, next_pid)) = self.ready_queue.iter().next() {
            self.ready_queue.remove(&(next_v, next_pid));
            self.min_vruntime = next_v;
            self.current = Some(next_pid);

            let p = self.procs.get_mut(&next_pid).unwrap();
            p.state = ProcessState::Running;
            p.started = true;

            return Some(&p.context as *const _);
        }

        None
    }
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

pub fn reinit_scheduler() {
    unsafe {
        core::ptr::write(
            core::ptr::addr_of!(SCHEDULER) as *mut Mutex<Scheduler>,
            Mutex::new(Scheduler::new()),
        );
        core::ptr::write(
            core::ptr::addr_of!(WORK_QUEUE) as *mut Mutex<VecDeque<Box<dyn Task>>>,
            Mutex::new(VecDeque::new()),
        );
    }
}

pub fn spawn(entry: fn() -> !) -> u64 {
    spawn_named(entry, "kthread", 2)
}

pub fn spawn_named(entry: fn() -> !, name: &'static str, priority: u8) -> u64 {
    let p = Process::new_kernel_named(entry, name, priority);
    let pid = p.pid;
    interrupts::without_interrupts(|| {
        SCHEDULER.lock().add(p);
    });
    pid
}

pub fn current_pid() -> u64 {
    interrupts::without_interrupts(|| {
        SCHEDULER.lock().current.unwrap_or(0)
    })
}

pub fn kill(pid: u64) {
    interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if let Some(p) = sched.procs.get_mut(&pid) {
            p.state = ProcessState::Dead;
            crate::serial_println!("[sched] kill pid={}", pid);
        }
    });
}

pub fn yield_now() {
    interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if let Some(curr) = sched.current {
            if let Some(p) = sched.procs.get_mut(&curr) {
                if p.state == ProcessState::Running {
                    p.state = ProcessState::Ready;
                    let vr = p.vruntime;
                    sched.ready_queue.insert((vr, curr));
                }
            }
        }
    });

    let tick = crate::interrupts::get_tick();
    schedule(tick);
}

pub fn sleep(ticks: u64) {
    let wakeup = crate::interrupts::get_tick() + ticks;

    interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if let Some(curr) = sched.current {
            if let Some(p) = sched.procs.get_mut(&curr) {
                p.state = ProcessState::Sleeping(wakeup);
            }
        }
    });

    let tick = crate::interrupts::get_tick();
    schedule(tick);
}

pub fn schedule(current_tick: u64) {
    let were_enabled = x86_64::instructions::interrupts::are_enabled();
    x86_64::instructions::interrupts::disable();

    let result = {
        let mut sched = SCHEDULER.lock();

        let old_pid = sched.current;
        let old_ctx = old_pid
            .and_then(|pid| sched.procs.get_mut(&pid))
            .map(|p| &mut p.context as *mut Context);
        let old_cr3 = old_pid
            .and_then(|pid| sched.procs.get(&pid))
            .map(|p| p.cr3);

        let new = sched.next(current_tick);
        if new.is_none() || sched.current == old_pid {
            if were_enabled {
                x86_64::instructions::interrupts::enable();
            }
            return;
        }

        let new_pid  = sched.current;
        let new_cr3  = new_pid.and_then(|pid| sched.procs.get(&pid)).map(|p| p.cr3);
        let new_rsp0 = new_pid.and_then(|pid| sched.procs.get(&pid)).map(|p| p.stack_top());

        Some((old_ctx, old_cr3, new, new_cr3, new_rsp0))
    };

    let (old_ctx, old_cr3, new_ctx, new_cr3, new_rsp0) = match result {
        Some(v) => v,
        None => {
            if were_enabled {
                x86_64::instructions::interrupts::enable();
            }
            return;
        }
    };

    if let Some(rsp0) = new_rsp0 {
        crate::gdt::set_kernel_stack(rsp0);
    }

    if let (Some(old_cr3), Some(new_cr3)) = (old_cr3, new_cr3) {
        if old_cr3 != new_cr3 {
            unsafe {
                core::arch::asm!(
                    "mov cr3, {}",
                    in(reg) new_cr3,
                    options(nostack, preserves_flags)
                );
            }
        }
    }

    match (old_ctx, new_ctx) {
        (Some(old), Some(new)) if old != new as *mut Context => {
            unsafe { switch_context(old, new) }
        }
        (None, Some(new)) => {
            unsafe { jump_to_context(new) }
        }
        _ => {
            if were_enabled {
                x86_64::instructions::interrupts::enable();
            }
        }
    }
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
        "or qword ptr [rdi + 0x40], 0x200",
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
