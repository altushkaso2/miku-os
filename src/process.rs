extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::paging::PageTableFlags;
use crate::vmm::AddressSpace;

static NEXT_PID: AtomicU64 = AtomicU64::new(1);
pub const DEFAULT_STACK_SIZE:      usize = 512 * 1024;
pub const USER_STACK_VIRT_TOP:     u64   = 0x0000_7FFF_FFFF_0000;
pub const USER_STACK_PAGES:        usize = DEFAULT_STACK_SIZE / 4096;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessState {
    Running,
    Ready,
    Sleeping(u64),
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
            rbx: 0, rbp: 0,
            rip: 0, rsp: 0,
            rflags: 0x202,
        }
    }
}

pub struct Process {
    pub pid:             u64,
    pub name:            &'static str,
    pub state:           ProcessState,
    pub context:         Context,
    pub stack:           Box<[u8]>,
    pub started:         bool,
    pub cr3:             u64,
    pub priority:        u8,
    pub cpu_time:        u64,
    pub vruntime:        u64,
    pub user_stack_phys: Option<u64>,
}

impl Process {
    fn alloc(name: &'static str, priority: u8, cr3: u64) -> Box<Self> {
        let stack = vec![0u8; DEFAULT_STACK_SIZE].into_boxed_slice();
        Box::new(Self {
            pid:             NEXT_PID.fetch_add(1, Ordering::SeqCst),
            name,
            state:           ProcessState::Ready,
            context:         Context::zero(),
            stack,
            started:         false,
            cr3,
            priority,
            cpu_time:        0,
            vruntime:        0,
            user_stack_phys: None,
        })
    }

    pub fn stack_top(&self) -> u64 {
        let top = self.stack.as_ptr() as u64 + self.stack.len() as u64;
        top & !0xF
    }

    pub fn new_kernel(entry: fn() -> !) -> Box<Self> {
        Self::new_kernel_named(entry, "kthread", 2)
    }

    pub fn new_kernel_named(entry: fn() -> !, name: &'static str, priority: u8) -> Box<Self> {
        let cr3 = crate::vmm::kernel_cr3();
        let mut p = Self::alloc(name, priority, cr3);
        p.context.rip = entry as u64;
        p.context.rsp = p.stack_top() - 8;
        p
    }

    pub fn new_user(entry: u64, aspace: AddressSpace) -> Box<Self> {
        let mut p = Self::alloc("user", 2, aspace.cr3);
        p.context.rip = entry;
        p.context.rsp = p.stack_top() - 8;
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

        let mut p = Self::alloc("user-r3", 2, cr3);
        p.context.rip = entry;
        p.context.rsp = user_rsp;
        p.user_stack_phys = Some(stack_phys);

        Some(p)
    }
}
