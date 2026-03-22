const ALLOC_ALIGN: usize = 16;
const BLOCK_HDR: usize = 32;
const SLAB_SIZE: usize = 131072;
const LARGE_THRESHOLD: usize = 32768;
const PROT_RW: u64 = 1 | 2;

#[repr(C)]
struct Block {
    size: usize,
    flags: usize,
    prev_free: *mut Block,
    next_free: *mut Block,
}

const FLAG_USED: usize = 1;
const FLAG_MMAP: usize = 2;

impl Block {
    fn data_size(&self) -> usize { self.size - BLOCK_HDR }
    fn data_ptr(&self) -> *mut u8 { (self as *const Block as *mut u8).wrapping_add(BLOCK_HDR) }
    fn is_free(&self) -> bool { self.flags & FLAG_USED == 0 }
    fn is_mmap(&self) -> bool { self.flags & FLAG_MMAP != 0 }
}

static mut FREE_HEAD: *mut Block = core::ptr::null_mut();
static mut SLAB_PTR: *mut u8 = core::ptr::null_mut();
static mut SLAB_LEFT: usize = 0;

fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

unsafe fn alloc_from_slab(total: usize) -> *mut Block {
    if SLAB_LEFT < total {
        let map_size = if SLAB_SIZE > total { SLAB_SIZE } else { align_up(total, 4096) };
        let p = crate::proc::miku_mmap(0, map_size, PROT_RW);
        if p.is_null() { return core::ptr::null_mut(); }
        SLAB_PTR = p;
        SLAB_LEFT = map_size;
    }
    let block = SLAB_PTR as *mut Block;
    (*block).size = total;
    (*block).flags = FLAG_USED;
    (*block).prev_free = core::ptr::null_mut();
    (*block).next_free = core::ptr::null_mut();
    SLAB_PTR = SLAB_PTR.add(total);
    SLAB_LEFT -= total;
    block
}

unsafe fn alloc_large(total: usize) -> *mut Block {
    let map_size = align_up(total, 4096);
    let p = crate::proc::miku_mmap(0, map_size, PROT_RW);
    if p.is_null() { return core::ptr::null_mut(); }
    let block = p as *mut Block;
    (*block).size = map_size;
    (*block).flags = FLAG_USED | FLAG_MMAP;
    (*block).prev_free = core::ptr::null_mut();
    (*block).next_free = core::ptr::null_mut();
    block
}

unsafe fn free_list_remove(block: *mut Block) {
    let prev = (*block).prev_free;
    let next = (*block).next_free;
    if !prev.is_null() { (*prev).next_free = next; }
    else { FREE_HEAD = next; }
    if !next.is_null() { (*next).prev_free = prev; }
    (*block).prev_free = core::ptr::null_mut();
    (*block).next_free = core::ptr::null_mut();
}

unsafe fn free_list_insert(block: *mut Block) {
    (*block).flags &= !FLAG_USED;
    (*block).prev_free = core::ptr::null_mut();
    (*block).next_free = FREE_HEAD;
    if !FREE_HEAD.is_null() { (*FREE_HEAD).prev_free = block; }
    FREE_HEAD = block;
}

unsafe fn find_free(needed: usize) -> *mut Block {
    let mut cur = FREE_HEAD;
    while !cur.is_null() {
        if (*cur).size >= needed { free_list_remove(cur); return cur; }
        cur = (*cur).next_free;
    }
    core::ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn miku_malloc(size: usize) -> *mut u8 {
    if size == 0 { return core::ptr::null_mut(); }
    let total = align_up(size + BLOCK_HDR, ALLOC_ALIGN);
    unsafe {
        let block = find_free(total);
        if !block.is_null() { (*block).flags |= FLAG_USED; return (*block).data_ptr(); }
        let block = if size >= LARGE_THRESHOLD { alloc_large(total) } else { alloc_from_slab(total) };
        if block.is_null() { return core::ptr::null_mut(); }
        (*block).data_ptr()
    }
}

#[no_mangle]
pub extern "C" fn miku_free(ptr: *mut u8) {
    if ptr.is_null() { return; }
    unsafe {
        let block = ptr.sub(BLOCK_HDR) as *mut Block;
        if (*block).is_free() { return; }
        if (*block).is_mmap() {
            let sz = (*block).size;
            crate::proc::miku_munmap(block as *mut u8, sz);
            return;
        }
        free_list_insert(block);
    }
}

#[no_mangle]
pub extern "C" fn miku_realloc(ptr: *mut u8, new_size: usize) -> *mut u8 {
    if ptr.is_null() { return miku_malloc(new_size); }
    if new_size == 0 { miku_free(ptr); return core::ptr::null_mut(); }
    unsafe {
        let block = ptr.sub(BLOCK_HDR) as *mut Block;
        let old_data = (*block).data_size();
        if old_data >= new_size { return ptr; }
        let new_ptr = miku_malloc(new_size);
        if new_ptr.is_null() { return core::ptr::null_mut(); }
        let copy_len = if old_data < new_size { old_data } else { new_size };
        crate::mem::miku_memcpy(new_ptr, ptr, copy_len);
        miku_free(ptr);
        new_ptr
    }
}

#[no_mangle]
pub extern "C" fn miku_calloc(count: usize, size: usize) -> *mut u8 {
    let total = count.saturating_mul(size);
    if total == 0 { return core::ptr::null_mut(); }
    let p = miku_malloc(total);
    if !p.is_null() { crate::mem::miku_memset(p, 0, total); }
    p
}
