use lazy_static::lazy_static;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

#[repr(align(16))]
struct DoubleFaultStack([u8; 8192]);

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static mut DOUBLE_FAULT_STACK: DoubleFaultStack = DoubleFaultStack([0; 8192]);

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(unsafe { &DOUBLE_FAULT_STACK.0 });
            stack_start + 8192u64
        };
        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
        let data_selector = gdt.add_entry(Descriptor::kernel_data_segment());
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
        (
            gdt,
            Selectors {
                code_selector,
                data_selector,
                tss_selector,
            },
        )
    };
}

struct Selectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.data_selector);
        SS::set_reg(GDT.1.data_selector); 
        ES::set_reg(GDT.1.data_selector); 
        load_tss(GDT.1.tss_selector);
    }

    crate::serial_println!(
        "[gdt] gdt+tss loaded, cs={:#x} ds=ss=es={:#x}",
        GDT.1.code_selector.0,
        GDT.1.data_selector.0
    );
}
