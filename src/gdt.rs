use core::cell::UnsafeCell;
use lazy_static::lazy_static;
use x86_64::registers::model_specific::KernelGsBase;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

#[repr(align(16))]
struct Stack8K([u8; 8192]);

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static mut DOUBLE_FAULT_STACK:   Stack8K = Stack8K([0; 8192]);
static mut KERNEL_SYSCALL_STACK: Stack8K = Stack8K([0; 8192]);

#[repr(C)]
pub struct PerCpu {
    pub kernel_rsp: u64,
    pub user_rsp:   u64,
}

static mut PER_CPU: PerCpu = PerCpu { kernel_rsp: 0, user_rsp: 0 };

struct TssCell(UnsafeCell<TaskStateSegment>);

unsafe impl Sync for TssCell {}

static TSS_CELL: TssCell = TssCell(UnsafeCell::new(TaskStateSegment::new()));

fn tss_ptr() -> *mut TaskStateSegment {
    TSS_CELL.0.get()
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        unsafe {
            let tss = &*tss_ptr();
            let mut gdt = GlobalDescriptorTable::new();
            let kernel_code = gdt.add_entry(Descriptor::kernel_code_segment());
            let kernel_data = gdt.add_entry(Descriptor::kernel_data_segment());
            let user_data   = gdt.add_entry(Descriptor::user_data_segment());
            let user_code   = gdt.add_entry(Descriptor::user_code_segment());
            let tss_sel     = gdt.add_entry(Descriptor::tss_segment(tss));
            (gdt, Selectors { kernel_code, kernel_data, user_code, user_data, tss: tss_sel })
        }
    };
}

pub struct Selectors {
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub user_code:   SegmentSelector,
    pub user_data:   SegmentSelector,
    pub tss:         SegmentSelector,
}

pub fn kernel_code_selector() -> SegmentSelector { GDT.1.kernel_code }
pub fn kernel_data_selector() -> SegmentSelector { GDT.1.kernel_data }
pub fn user_code_selector()   -> SegmentSelector { GDT.1.user_code   }
pub fn user_data_selector()   -> SegmentSelector { GDT.1.user_data   }

pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
    use x86_64::instructions::tables::load_tss;

    unsafe {
        let tss = &mut *tss_ptr();

        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            let start = VirtAddr::from_ptr(&DOUBLE_FAULT_STACK.0);
            start + 8192u64
        };
        tss.privilege_stack_table[0] = {
            let start = VirtAddr::from_ptr(&KERNEL_SYSCALL_STACK.0);
            start + 8192u64
        };
    }

    GDT.0.load();

    unsafe {
        CS::set_reg(GDT.1.kernel_code);
        DS::set_reg(GDT.1.kernel_data);
        SS::set_reg(GDT.1.kernel_data);
        ES::set_reg(GDT.1.kernel_data);
        load_tss(GDT.1.tss);

        let kernel_rsp = (*tss_ptr()).privilege_stack_table[0].as_u64();
        PER_CPU.kernel_rsp = kernel_rsp;
        KernelGsBase::write(VirtAddr::new(core::ptr::addr_of!(PER_CPU) as u64));
    }

    crate::serial_println!(
        "[gdt] loaded: kernel_cs={:#x} user_cs={:#x} user_ds={:#x}",
        GDT.1.kernel_code.0,
        GDT.1.user_code.0,
        GDT.1.user_data.0,
    );
}

pub fn set_kernel_stack(stack_top: u64) {
    unsafe {
        (*tss_ptr()).privilege_stack_table[0] = VirtAddr::new(stack_top);
        PER_CPU.kernel_rsp = stack_top;
    }
}
