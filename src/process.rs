use alloc::boxed::Box;
use alloc::vec;
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::paging::PageTableFlags;
use crate::vmm::AddressSpace;

static NEXT_PID: AtomicU64 = AtomicU64::new(1);

pub const DEFAULT_STACK_SIZE:  usize = 512 * 1024;
pub const USER_STACK_VIRT_TOP: u64   = 0x0000_7FFF_FFFF_0000;
pub const USER_STACK_PAGES:    usize = DEFAULT_STACK_SIZE / 4096;
pub const CPU_ALL:             u64   = u64::MAX;

pub const FRAME_SIZE: u64 = 0xA0;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    pub rsp: u64,
}

impl Context {
    pub const fn zero() -> Self { Self { rsp: 0 } }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessState {
    Running,
    Ready,
    Sleeping(u64),
    Blocked(&'static str),
    Dead,
}

impl ProcessState {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Running        => "R",
            Self::Ready          => "S",
            Self::Sleeping(_)    => "S",
            Self::Blocked(cause) => cause,
            Self::Dead           => "X",
        }
    }
}

pub struct Process {
    pub pid:              u64,
    pub name:             &'static str,
    pub state:            ProcessState,
    pub context:          Context,
    pub stack:            Box<[u8]>,
    pub started:          bool,
    pub cr3:              u64,
    pub is_idle:          bool,

    pub priority:         u8,
    pub cpu_mask:         u64,

    pub cpu_time:         u64,
    pub vruntime:         u64,

    pub wall_start_tick:  u64,
    pub last_run_tick:    u64,
    pub sleep_count:      u64,
    pub preempt_count:    u64,
    pub switch_in_count:  u64,

    pub window_cpu_ticks: u64,
    pub window_start:     u64,

    pub user_stack_phys:  Option<u64>,
}

impl Process {
    fn alloc(name: &'static str, priority: u8, cr3: u64) -> Box<Self> {
        let tick = crate::interrupts::get_tick();
        let stack = vec![0u8; DEFAULT_STACK_SIZE].into_boxed_slice();
        Box::new(Self {
            pid:              NEXT_PID.fetch_add(1, Ordering::SeqCst),
            name,
            state:            ProcessState::Ready,
            context:          Context::zero(),
            stack,
            started:          false,
            cr3,
            is_idle:          false,
            priority:         priority.clamp(1, 20),
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
        })
    }

    pub fn stack_top(&self) -> u64 {
        let top = self.stack.as_ptr() as u64 + self.stack.len() as u64;
        top & !0xF
    }

    pub fn stack_used_bytes(&self) -> usize {
        let top = self.stack_top();
        let rsp = self.context.rsp;
        if rsp == 0 || rsp > top {
            return 0;
        }
        (top - rsp) as usize
    }

    pub fn new_kernel(entry: fn() -> !) -> Box<Self> {
        Self::new_kernel_named(entry, "kthread", 10)
    }

    pub fn new_kernel_named(entry: fn() -> !, name: &'static str, priority: u8) -> Box<Self> {
        let cr3 = crate::vmm::kernel_cr3();
        let mut p = Self::alloc(name, priority, cr3);
        let top = p.stack_top();
        p.context.rsp = build_kernel_frame(top, entry as u64);
        p
    }

    pub fn new_user(entry: u64, aspace: AddressSpace) -> Box<Self> {
        let mut p = Self::alloc("user", 10, aspace.cr3);
        let top = p.stack_top();
        p.context.rsp = build_kernel_frame(top, entry);
        core::mem::forget(aspace);
        p
    }

    pub fn new_user_ring3(entry: u64, mut aspace: AddressSpace) -> Option<Box<Self>> {
        let stack_size = (USER_STACK_PAGES * 4096) as u64;
        let stack_virt_base = USER_STACK_VIRT_TOP - stack_size;
        let stack_phys = crate::pmm::alloc_frames(USER_STACK_PAGES)?;
        let flags = PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        if !aspace.map_range(stack_virt_base, stack_phys, stack_size, flags) {
            crate::pmm::free_frames(stack_phys, USER_STACK_PAGES);
            return None;
        }
        let user_rsp = (USER_STACK_VIRT_TOP - 8) & !0xF;
        let cr3 = aspace.cr3;
        core::mem::forget(aspace);
        let mut p = Self::alloc("user-r3", 10, cr3);
        let top = p.stack_top();
        p.context.rsp = build_user_frame(top, entry, user_rsp);
        p.user_stack_phys = Some(stack_phys);
        Some(p)
    }

    pub fn cpu_percent_window(&self, now: u64) -> u32 {
        let window = now.saturating_sub(self.window_start).max(1);
        ((self.window_cpu_ticks * 1000) / window).min(1000) as u32
    }

    pub fn uptime_ticks(&self, now: u64) -> u64 {
        now.saturating_sub(self.wall_start_tick)
    }
}

fn build_kernel_frame(kernel_stack_top: u64, rip: u64) -> u64 {
    write_frame(kernel_stack_top, rip, 0x08, kernel_stack_top, 0x10)
}

fn build_user_frame(kernel_stack_top: u64, rip: u64, user_rsp: u64) -> u64 {
    write_frame(
        kernel_stack_top,
        rip,
        crate::gdt::user_code_selector().0 as u64,
        user_rsp,
        crate::gdt::user_data_selector().0 as u64,
    )
}

fn write_frame(kernel_stack_top: u64, rip: u64, cs: u64, iret_rsp: u64, ss: u64) -> u64 {
    let rsp = kernel_stack_top - FRAME_SIZE;
    unsafe {
        let f = rsp as *mut u64;
        f.add(0).write(0);
        f.add(1).write(0);
        f.add(2).write(0);
        f.add(3).write(0);
        f.add(4).write(0);
        f.add(5).write(0);
        f.add(6).write(0);
        f.add(7).write(0);
        f.add(8).write(0);
        f.add(9).write(0);
        f.add(10).write(0);
        f.add(11).write(0);
        f.add(12).write(0);
        f.add(13).write(0);
        f.add(14).write(0);
        f.add(15).write(rip);
        f.add(16).write(cs);
        f.add(17).write(0x202);
        f.add(18).write(iret_rsp);
        f.add(19).write(ss);
    }
    rsp
}
