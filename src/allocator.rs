use linked_list_allocator::LockedHeap;

pub const HEAP_SIZE: usize = 16 * 1024 * 1024;

#[repr(align(4096))]
struct HeapMemory([u8; HEAP_SIZE]);

static mut HEAP_MEMORY: HeapMemory = HeapMemory([0; HEAP_SIZE]);

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init() {
    unsafe {
        let heap_start = HEAP_MEMORY.0.as_mut_ptr();
        ALLOCATOR.lock().init(heap_start, HEAP_SIZE);
    }
    crate::serial_println!("[heap] {} KB initialized", HEAP_SIZE / 1024);
}

pub fn used() -> usize {
    ALLOCATOR.lock().used()
}

pub fn free() -> usize {
    ALLOCATOR.lock().free()
}
