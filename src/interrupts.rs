use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

static TICK: AtomicU64 = AtomicU64::new(0);
static NEED_SCHEDULE: AtomicBool = AtomicBool::new(false);

pub static ATA_PRIMARY_IRQ:   AtomicBool = AtomicBool::new(false);
pub static ATA_SECONDARY_IRQ: AtomicBool = AtomicBool::new(false);

pub fn get_tick() -> u64 {
    TICK.load(Ordering::Relaxed)
}

pub fn check_and_schedule() {
    if NEED_SCHEDULE.swap(false, Ordering::AcqRel) {
        crate::scheduler::schedule(get_tick());
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer    = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1,
    AtaIrq14 = PIC_2_OFFSET + 6,
    AtaIrq15 = PIC_2_OFFSET + 7,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 { self as u8 }
    fn as_usize(self) -> usize { usize::from(self.as_u8()) }
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.general_protection_fault.set_handler_fn(gpf_handler);
        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);
        idt[InterruptIndex::AtaIrq14.as_usize()].set_handler_fn(ata_irq14_handler);
        idt[InterruptIndex::AtaIrq15.as_usize()].set_handler_fn(ata_irq15_handler);
        idt
    };
}

pub fn init_idt() {
    crate::serial_println!("[int] loading idt");
    IDT.load();
    crate::serial_println!("[int] idt loaded");
}

pub fn init_pics() {
    unsafe {
        let mut pics = PICS.lock();
        pics.initialize();
        pics.write_masks(0b1111_1000, 0b0011_1111);
    }
    let masks = unsafe { PICS.lock().read_masks() };
    crate::serial_println!(
        "[int] PIC masks: PIC1=0b{:08b} PIC2=0b{:08b}",
        masks[0], masks[1]
    );
}

pub fn init_pit_1000hz() {
    const PIT_FREQUENCY: u32 = 1_193_182;
    const TARGET_HZ: u32 = 1000;
    const DIVISOR: u16 = (PIT_FREQUENCY / TARGET_HZ) as u16;

    unsafe {
        use x86_64::instructions::port::Port;
        Port::<u8>::new(0x43).write(0x36);
        Port::<u8>::new(0x40).write(DIVISOR as u8);
        Port::<u8>::new(0x40).write((DIVISOR >> 8) as u8);
    }
    crate::serial_println!("[pit] 1000 Hz (divisor={})", DIVISOR);
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    crate::vfs::procfs::tick();
    TICK.fetch_add(1, Ordering::Relaxed);
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
    NEED_SCHEDULE.store(true, Ordering::Release);
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    let scancode: u8 = unsafe { Port::<u8>::new(0x60).read() };
    crate::stdin::push(scancode);
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn ata_irq14_handler(_stack_frame: InterruptStackFrame) {
    ATA_PRIMARY_IRQ.store(true, Ordering::Release);
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::AtaIrq14.as_u8());
    }
}

extern "x86-interrupt" fn ata_irq15_handler(_stack_frame: InterruptStackFrame) {
    ATA_SECONDARY_IRQ.store(true, Ordering::Release);
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::AtaIrq15.as_u8());
    }
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    crate::serial_println!("[int] breakpoint\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    crate::serial_println!("[double fault] code={}\n{:#?}", error_code, stack_frame);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: x86_64::structures::idt::PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    let addr = Cr2::read();
    crate::serial_println!(
        "[page fault] addr={:?} code={:?}\n{:#?}", addr, error_code, stack_frame
    );
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn gpf_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    crate::serial_println!("[gpf] code={}\n{:#?}", error_code, stack_frame);
    loop { x86_64::instructions::hlt(); }
}
