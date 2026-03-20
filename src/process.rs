use alloc::boxed::Box;
use alloc::vec;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicU8, Ordering};
use x86_64::structures::paging::PageTableFlags;
use crate::vmm::AddressSpace;

pub const DEFAULT_STACK_SIZE:  usize = 512 * 1024;
pub const USER_STACK_VIRT_TOP: u64   = 0x0000_7FFF_FFFF_0000;
pub const USER_STACK_PAGES:    usize = DEFAULT_STACK_SIZE / 4096;
pub const CPU_ALL:             u64   = u64::MAX;
pub const FRAME_SIZE:          u64   = 0xA0;

pub const STATE_READY:   u8 = 0;
pub const STATE_RUNNING: u8 = 1;
pub const STATE_SLEEPING: u8 = 2;
pub const STATE_BLOCKED:  u8 = 3;
pub const STATE_DEAD:     u8 = 4;

static NEXT_PID: AtomicU64 = AtomicU64::new(1);

pub fn pid_range() -> u64 {
    NEXT_PID.load(Ordering::Relaxed)
}

pub struct Process {
    pub pid:             u64,
    pub name:            &'static str,
    pub is_idle:         bool,
    pub priority:        AtomicU8,
    pub cpu_mask:        u64,
    pub cr3:             u64,
    pub wall_start_tick: u64,

    pub rsp:              AtomicU64,
    pub state:            AtomicU8,
    pub sleep_until:      AtomicU64,
    pub blocked_cause:    AtomicPtr<u8>,
    pub vruntime:         AtomicU64,
    pub cpu_time:         AtomicU64,
    pub window_cpu_ticks: AtomicU64,
    pub window_start:     AtomicU64,
    pub last_run_tick:    AtomicU64,
    pub preempt_count:    AtomicU64,
    pub sleep_count:      AtomicU64,
    pub switch_in_count:  AtomicU64,

    pub rq_next: AtomicPtr<Process>,
    pub on_rq:   AtomicBool,

    pub stack:           Box<[u8]>,
    pub user_stack_phys: Option<u64>,
    pub brk:             AtomicU64,
}

impl Process {
    pub fn alloc_raw(name: &'static str, priority: u8, cr3: u64) -> Box<Self> {
        let tick  = crate::interrupts::get_tick();
        let stack = vec![0u8; DEFAULT_STACK_SIZE].into_boxed_slice();
        Box::new(Self {
            pid:             NEXT_PID.fetch_add(1, Ordering::SeqCst),
            name,
            is_idle:         false,
            priority:        AtomicU8::new(priority.clamp(1, 20)),
            cpu_mask:        CPU_ALL,
            cr3,
            wall_start_tick: tick,
            rsp:              AtomicU64::new(0),
            state:            AtomicU8::new(STATE_READY),
            sleep_until:      AtomicU64::new(0),
            blocked_cause:    AtomicPtr::new(null_mut()),
            vruntime:         AtomicU64::new(0),
            cpu_time:         AtomicU64::new(0),
            window_cpu_ticks: AtomicU64::new(0),
            window_start:     AtomicU64::new(tick),
            last_run_tick:    AtomicU64::new(tick),
            preempt_count:    AtomicU64::new(0),
            sleep_count:      AtomicU64::new(0),
            switch_in_count:  AtomicU64::new(0),
            rq_next:          AtomicPtr::new(null_mut()),
            on_rq:            AtomicBool::new(false),
            stack,
            user_stack_phys:  None,
            brk:              AtomicU64::new(0),
        })
    }

    pub fn new_idle(cr3: u64, tick: u64) -> Box<Self> {
        let stack = vec![0u8; DEFAULT_STACK_SIZE].into_boxed_slice();
        Box::new(Self {
            pid:             0,
            name:            "idle",
            is_idle:         true,
            priority:        AtomicU8::new(20),
            cpu_mask:        CPU_ALL,
            cr3,
            wall_start_tick: tick,
            rsp:              AtomicU64::new(0),
            state:            AtomicU8::new(STATE_RUNNING),
            sleep_until:      AtomicU64::new(0),
            blocked_cause:    AtomicPtr::new(null_mut()),
            vruntime:         AtomicU64::new(0),
            cpu_time:         AtomicU64::new(0),
            window_cpu_ticks: AtomicU64::new(0),
            window_start:     AtomicU64::new(tick),
            last_run_tick:    AtomicU64::new(tick),
            preempt_count:    AtomicU64::new(0),
            sleep_count:      AtomicU64::new(0),
            switch_in_count:  AtomicU64::new(0),
            rq_next:          AtomicPtr::new(null_mut()),
            on_rq:            AtomicBool::new(false),
            stack,
            user_stack_phys:  None,
            brk:              AtomicU64::new(0),
        })
    }

    pub fn stack_top(&self) -> u64 {
        (self.stack.as_ptr() as u64 + self.stack.len() as u64) & !0xF
    }

    pub fn stack_used_bytes(&self) -> usize {
        let top = self.stack_top();
        let rsp = self.rsp.load(Ordering::Relaxed);
        if rsp == 0 || rsp > top { return 0; }
        (top - rsp) as usize
    }

    pub fn cpu_percent_window(&self, now: u64) -> u32 {
        let ws  = self.window_start.load(Ordering::Relaxed);
        let wct = self.window_cpu_ticks.load(Ordering::Relaxed);
        ((wct * 1000) / now.saturating_sub(ws).max(1)).min(1000) as u32
    }

    pub fn uptime_ticks(&self, now: u64) -> u64 {
        now.saturating_sub(self.wall_start_tick)
    }

    pub fn state_name(&self) -> &'static str {
        match self.state.load(Ordering::Relaxed) {
            STATE_READY | STATE_SLEEPING => "S",
            STATE_RUNNING                => "R",
            STATE_BLOCKED                => "B",
            STATE_DEAD                   => "X",
            _                            => "?",
        }
    }

    pub fn new_kernel(entry: fn() -> !) -> Box<Self> {
        Self::new_kernel_named(entry, "kthread", 10)
    }

    pub fn new_kernel_named(entry: fn() -> !, name: &'static str, priority: u8) -> Box<Self> {
        let cr3 = crate::vmm::kernel_cr3();
        let mut p = Self::alloc_raw(name, priority, cr3);
        let top = p.stack_top();
        p.rsp.store(build_kernel_frame(top, entry as u64), Ordering::Relaxed);
        p
    }

    pub fn new_user(entry: u64, aspace: AddressSpace) -> Box<Self> {
        let cr3 = aspace.into_raw();
        let mut p = Self::alloc_raw("user", 10, cr3);
        let top = p.stack_top();
        p.rsp.store(build_kernel_frame(top, entry), Ordering::Relaxed);
        p
    }

    pub fn new_user_ring3(entry: u64, mut aspace: AddressSpace) -> Option<Box<Self>> {
        let stack_size      = (USER_STACK_PAGES * 4096) as u64;
        let stack_virt_base = USER_STACK_VIRT_TOP - stack_size;
        let stack_phys      = crate::pmm::alloc_frames(USER_STACK_PAGES)?;
        let flags           = PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

        if !aspace.map_range(stack_virt_base, stack_phys, stack_size, flags) {
            crate::pmm::free_frames(stack_phys, USER_STACK_PAGES);
            return None;
        }

        let user_rsp = (USER_STACK_VIRT_TOP - 8) & !0xF;
        let cr3      = aspace.into_raw();

        let mut p = Self::alloc_raw("user-r3", 10, cr3);
        let top = p.stack_top();
        p.rsp.store(build_user_frame(top, entry, user_rsp), Ordering::Relaxed);
        p.user_stack_phys = Some(stack_phys);
        Some(p)
    }

    pub fn new_elf(entry: u64, user_rsp: u64, aspace: AddressSpace) -> Option<Box<Self>> {
        let cr3 = aspace.into_raw();
        let mut p = Self::alloc_raw("user-elf", 10, cr3);
        let top = p.stack_top();
        p.rsp.store(build_user_frame(top, entry, user_rsp), Ordering::Relaxed);
        Some(p)
    }

    pub fn cleanup_user_address_space(&mut self) {
        if self.cr3 == 0 || self.cr3 == crate::vmm::kernel_cr3() { return; }
        let mut aspace = AddressSpace::from_raw(self.cr3);
        aspace.free_address_space();
        self.cr3 = 0;
    }
}

fn build_kernel_frame(kernel_stack_top: u64, rip: u64) -> u64 {
    write_frame(kernel_stack_top, rip, 0x08, kernel_stack_top, 0x10)
}

fn build_user_frame(kernel_stack_top: u64, rip: u64, user_rsp: u64) -> u64 {
    write_frame(
        kernel_stack_top, rip,
        crate::gdt::user_code_selector().0 as u64,
        user_rsp,
        crate::gdt::user_data_selector().0 as u64,
    )
}

fn write_frame(kernel_stack_top: u64, rip: u64, cs: u64, iret_rsp: u64, ss: u64) -> u64 {
    let rsp = kernel_stack_top - FRAME_SIZE;
    unsafe {
        let f = rsp as *mut u64;
        for i in 0..15 { f.add(i).write(0); }
        f.add(15).write(rip);
        f.add(16).write(cs);
        f.add(17).write(0x202);
        f.add(18).write(iret_rsp);
        f.add(19).write(ss);
    }
    rsp
}
