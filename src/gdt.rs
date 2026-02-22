use lazy_static::lazy_static;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

#[repr(align(16))]
struct Stack8K([u8; 8192]);

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static mut DOUBLE_FAULT_STACK: Stack8K = Stack8K([0; 8192]);
static mut KERNEL_SYSCALL_STACK: Stack8K = Stack8K([0; 8192]);

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(unsafe { &DOUBLE_FAULT_STACK.0 });
            stack_start + 8192u64
        };
        tss.privilege_stack_table[0] = {
            let stack_start = VirtAddr::from_ptr(unsafe { &KERNEL_SYSCALL_STACK.0 });
            stack_start + 8192u64
        };
        tss
    };
}

pub struct Selectors {
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub user_code:   SegmentSelector,
    pub user_data:   SegmentSelector,
    pub tss:         SegmentSelector,
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let kernel_code = gdt.add_entry(Descriptor::kernel_code_segment());
        let kernel_data = gdt.add_entry(Descriptor::kernel_data_segment());
        let user_data   = gdt.add_entry(Descriptor::user_data_segment());
        let user_code   = gdt.add_entry(Descriptor::user_code_segment());
        let tss         = gdt.add_entry(Descriptor::tss_segment(&TSS));
        (gdt, Selectors { kernel_code, kernel_data, user_code, user_data, tss })
    };
}

pub fn kernel_code_selector() -> SegmentSelector { GDT.1.kernel_code }
pub fn kernel_data_selector() -> SegmentSelector { GDT.1.kernel_data }
pub fn user_code_selector()   -> SegmentSelector { GDT.1.user_code }
pub fn user_data_selector()   -> SegmentSelector { GDT.1.user_data }

pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.kernel_code);
        DS::set_reg(GDT.1.kernel_data);
        SS::set_reg(GDT.1.kernel_data);
        ES::set_reg(GDT.1.kernel_data);
        load_tss(GDT.1.tss);
    }

    crate::serial_println!(
        "[gdt] loaded: kernel_cs={:#x} user_cs={:#x} user_ds={:#x}",
        GDT.1.kernel_code.0,
        GDT.1.user_code.0,
        GDT.1.user_data.0,
    );
}
