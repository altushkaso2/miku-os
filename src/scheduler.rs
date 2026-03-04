extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec::Vec;
use spin::Mutex;
use x86_64::instructions::interrupts;

use crate::process::{Context, Process, ProcessState, CPU_ALL};

const CPU_WINDOW_TICKS: u64 = 250;

static PRIO_WEIGHT: [u64; 20] = [
    88761, 71755, 56483, 46273, 36291,
    29154, 23254, 18705, 14949, 11916,
     9548,  7620,  6100,  4904,  3906,
     3121,  2501,  1991,  1586,  1277,
];

fn weight(priority: u8) -> u64 {
    PRIO_WEIGHT[priority.clamp(1, 20) as usize - 1]
}

const TICK_SCALE: u64 = 1_000_000;

pub trait Task: Send {
    fn run(self: Box<Self>);
}

impl<F: FnOnce() + Send> Task for F {
    fn run(self: Box<Self>) { (*self)() }
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
        spawn_named(worker_loop, "worker", 10);
    }
    crate::serial_println!("[sched] {} worker threads started", count);
}

#[derive(Debug, Clone)]
pub struct ThreadStat {
    pub pid:            u64,
    pub name:           &'static str,
    pub state:          &'static str,
    pub priority:       u8,
    pub cpu_mask:       u64,
    pub cpu_time:       u64,
    pub vruntime:       u64,
    pub preempt_count:  u64,
    pub sleep_count:    u64,
    pub switch_in:      u64,
    pub cpu_pct_x10:    u32,
    pub uptime_ticks:   u64,
    pub is_idle:        bool,
    pub stack_alloc_kb: usize,
    pub stack_used_kb:  usize,
}

pub struct Scheduler {
    pub procs:          BTreeMap<u64, Box<Process>>,
    pub ready_queue:    BTreeSet<(u64, u64)>,
    pub sleep_queue:    BTreeSet<(u64, u64)>,
    pub current:        Option<u64>,
    pub min_vruntime:   u64,
    pub current_cpu:    u8,
    pub total_switches: u64,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            procs:          BTreeMap::new(),
            ready_queue:    BTreeSet::new(),
            sleep_queue:    BTreeSet::new(),
            current:        None,
            min_vruntime:   0,
            current_cpu:    0,
            total_switches: 0,
        }
    }

    pub fn init_main_thread(&mut self) {
        let cr3 = crate::vmm::kernel_cr3();
        let tick = crate::interrupts::get_tick();
        let stack = alloc::vec![0u8; crate::process::DEFAULT_STACK_SIZE].into_boxed_slice();
        let p = Box::new(Process {
            pid:              0,
            name:             "idle",
            state:            ProcessState::Running,
            context:          Context::zero(),
            stack,
            started:          true,
            cr3,
            is_idle:          true,
            priority:         20,
            cpu_mask:         CPU_ALL,
            cpu_time:         0,
            vruntime:         0,
            wall_start_tick:  tick,
            last_run_tick:    tick,
            sleep_count:      0,
            preempt_count:    0,
            switch_in_count:  0,
            window_cpu_ticks: 0,
            window_start:     tick,
            user_stack_phys:  None,
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

    fn cpu_bit(&self) -> u64 { 1u64 << self.current_cpu }

    fn next_eligible(&self) -> Option<(u64, u64)> {
        let cpu_bit = self.cpu_bit();

        for &(vr, pid) in self.ready_queue.iter() {
            if let Some(p) = self.procs.get(&pid) {
                if p.cpu_mask & cpu_bit != 0 && !p.is_idle {
                    return Some((vr, pid));
                }
            }
        }

        for &(vr, pid) in self.ready_queue.iter() {
            if let Some(p) = self.procs.get(&pid) {
                if p.cpu_mask & cpu_bit != 0 {
                    return Some((vr, pid));
                }
            }
        }

        None
    }

    pub fn next(&mut self, current_tick: u64) -> Option<u64> {
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
                }
                crate::serial_println!("[sched] pid={} collected", pid);
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

        let mut was_preempted = false;

        if let Some(curr_pid) = self.current {
            let curr_info = self.procs.get(&curr_pid).map(|p| (p.state, p.is_idle));

            if let Some((state, is_idle)) = curr_info {
                match state {
                    ProcessState::Sleeping(wake) => {
                        if let Some(p) = self.procs.get_mut(&curr_pid) {
                            p.sleep_count += 1;
                        }
                        self.sleep_queue.insert((wake, curr_pid));
                    }
                    ProcessState::Blocked(_) | ProcessState::Dead | ProcessState::Ready => {}
                    ProcessState::Running => {
                        if is_idle {
                            let has_ready = self.next_eligible().is_some();
                            if has_ready {
                                if let Some(p) = self.procs.get_mut(&curr_pid) {
                                    p.state = ProcessState::Ready;
                                    let vr = p.vruntime;
                                    self.ready_queue.insert((vr, curr_pid));
                                    was_preempted = true;
                                }
                            }
                        } else {
                            let new_vr = {
                                let p = self.procs.get_mut(&curr_pid).unwrap();
                                p.cpu_time += 1;
                                p.window_cpu_ticks += 1;
                                if current_tick.saturating_sub(p.window_start) >= CPU_WINDOW_TICKS {
                                    p.window_cpu_ticks = 1;
                                    p.window_start = current_tick;
                                }
                                let w = weight(p.priority);
                                p.vruntime += TICK_SCALE / w;
                                p.vruntime
                            };

                            let should_preempt = self
                                .next_eligible()
                                .map(|(next_v, _)| new_vr > next_v)
                                .unwrap_or(false);

                            if should_preempt {
                                if let Some(p) = self.procs.get_mut(&curr_pid) {
                                    p.state = ProcessState::Ready;
                                    let vr = p.vruntime;
                                    p.preempt_count += 1;
                                    self.ready_queue.insert((vr, curr_pid));
                                    was_preempted = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        let need_switch = match self.current {
            None => true,
            Some(pid) => match self.procs.get(&pid).map(|p| p.state) {
                Some(ProcessState::Running) => was_preempted,
                _ => true,
            },
        };

        if !need_switch {
            return None;
        }

        if let Some((next_v, next_pid)) = self.next_eligible() {
            self.ready_queue.remove(&(next_v, next_pid));
            self.min_vruntime = self.min_vruntime.max(next_v);
            self.current = Some(next_pid);
            self.total_switches += 1;

            let p = self.procs.get_mut(&next_pid).unwrap();
            p.state = ProcessState::Running;
            p.started = true;
            p.last_run_tick = current_tick;
            p.switch_in_count += 1;

            return Some(p.context.rsp);
        }

        None
    }

    pub fn thread_stats(&self) -> Vec<ThreadStat> {
        let now = crate::interrupts::get_tick();
        let mut out = Vec::new();
        for (&pid, p) in self.procs.iter() {
            out.push(ThreadStat {
                pid,
                name:          p.name,
                state:         p.state.name(),
                priority:      p.priority,
                cpu_mask:      p.cpu_mask,
                cpu_time:      p.cpu_time,
                vruntime:      p.vruntime,
                preempt_count: p.preempt_count,
                sleep_count:   p.sleep_count,
                switch_in:     p.switch_in_count,
                cpu_pct_x10:   p.cpu_percent_window(now),
                uptime_ticks:  p.uptime_ticks(now),
                is_idle:        p.is_idle,
                stack_alloc_kb: p.stack.len() / 1024,
                stack_used_kb:  p.stack_used_bytes() / 1024,
            });
        }
        out.sort_by_key(|s| s.pid);
        out
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
    spawn_named(entry, "kthread", 10)
}

pub fn spawn_named(entry: fn() -> !, name: &'static str, priority: u8) -> u64 {
    let p = Process::new_kernel_named(entry, name, priority);
    let pid = p.pid;
    interrupts::without_interrupts(|| SCHEDULER.lock().add(p));
    pid
}

pub fn current_pid() -> u64 {
    interrupts::without_interrupts(|| SCHEDULER.lock().current.unwrap_or(0))
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

pub fn wakeup(pid: u64) {
    interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        let info = sched.procs.get(&pid).map(|p| (p.state, p.vruntime));
        if let Some((state, vruntime)) = info {
            match state {
                ProcessState::Sleeping(w) => { sched.sleep_queue.remove(&(w, pid)); }
                ProcessState::Blocked(_)  => {}
                _ => return,
            }
            let min_vr = sched.min_vruntime;
            if let Some(p) = sched.procs.get_mut(&pid) {
                let vr = vruntime.max(min_vr);
                p.vruntime = vr;
                p.state = ProcessState::Ready;
                sched.ready_queue.insert((vr, pid));
            }
        }
    });
}

pub fn set_affinity(pid: u64, mask: u64) {
    interrupts::without_interrupts(|| {
        if let Some(p) = SCHEDULER.lock().procs.get_mut(&pid) {
            p.cpu_mask = if mask == 0 { CPU_ALL } else { mask };
            crate::serial_println!("[sched] pid={} affinity={:#018x}", pid, p.cpu_mask);
        }
    });
}

pub fn set_priority(pid: u64, priority: u8) {
    interrupts::without_interrupts(|| {
        if let Some(p) = SCHEDULER.lock().procs.get_mut(&pid) {
            p.priority = priority.clamp(1, 20);
            crate::serial_println!("[sched] pid={} priority={}", pid, p.priority);
        }
    });
}

#[no_mangle]
pub unsafe extern "C" fn schedule_from_isr(old_rsp: u64) -> u64 {
    let tick = crate::interrupts::get_tick();

    let mut sched = match SCHEDULER.try_lock() {
        Some(s) => s,
        None    => return old_rsp,
    };

    if let Some(curr_pid) = sched.current {
        if let Some(p) = sched.procs.get_mut(&curr_pid) {
            p.context.rsp = old_rsp;
        }
    }

    let old_cr3 = sched.current
        .and_then(|pid| sched.procs.get(&pid))
        .map(|p| p.cr3);

    let new_rsp = match sched.next(tick) {
        Some(rsp) => rsp,
        None      => return old_rsp,
    };

    let new_pid  = sched.current;
    let new_cr3  = new_pid.and_then(|pid| sched.procs.get(&pid)).map(|p| p.cr3);
    let new_rsp0 = new_pid.and_then(|pid| sched.procs.get(&pid)).map(|p| p.stack_top());

    drop(sched);

    if let Some(rsp0) = new_rsp0 {
        crate::gdt::set_kernel_stack(rsp0);
    }

    if let (Some(oc), Some(nc)) = (old_cr3, new_cr3) {
        if oc != nc {
            core::arch::asm!(
                "mov cr3, {}",
                in(reg) nc,
                options(nostack, preserves_flags)
            );
        }
    }

    new_rsp
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
    unsafe { software_context_switch() }
}

pub fn sleep(ticks: u64) {
    let wakeup_tick = crate::interrupts::get_tick() + ticks;
    interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if let Some(curr) = sched.current {
            if let Some(p) = sched.procs.get_mut(&curr) {
                p.state = ProcessState::Sleeping(wakeup_tick);
            }
        }
    });
    unsafe { software_context_switch() }
}

pub fn block_current(cause: &'static str) {
    interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if let Some(curr) = sched.current {
            if let Some(p) = sched.procs.get_mut(&curr) {
                p.state = ProcessState::Blocked(cause);
            }
        }
    });
    unsafe { software_context_switch() }
}

pub fn total_switches() -> u64 {
    interrupts::without_interrupts(|| SCHEDULER.lock().total_switches)
}

pub fn thread_count() -> usize {
    interrupts::without_interrupts(|| SCHEDULER.lock().procs.len())
}

pub fn get_stats() -> Vec<ThreadStat> {
    interrupts::without_interrupts(|| SCHEDULER.lock().thread_stats())
}

#[unsafe(naked)]
unsafe extern "C" fn software_context_switch() {
    core::arch::naked_asm!(
        "cli",
        "mov rax, rsp",
        "push 0x10",
        "push rax",
        "pushfq",
        "or qword ptr [rsp], 0x200",
        "push 0x08",
        "lea rax, [rip + 1f]",
        "push rax",
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push 0",
        "mov rdi, rsp",
        "call {sched}",
        "mov rsp, rax",
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "iretq",
        "1:",
        "ret",
        sched = sym schedule_from_isr,
    )
}
