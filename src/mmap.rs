extern crate alloc;
use alloc::collections::BTreeMap;
use spin::Mutex;
use x86_64::structures::paging::PageTableFlags;
use crate::vmm::AddressSpace;
use crate::pmm;

const PAGE_SIZE: u64   = 4096;
const MAX_VMAS:  usize = 64;
const MMAP_BASE:  u64  = 0x0000_0001_0000_0000;
const MMAP_LIMIT: u64  = 0x0000_7F00_0000_0000;
const BRK_BASE:   u64  = 0x0000_0060_0000_0000;

pub const PROT_WRITE: u32 = 2;
pub const PROT_EXEC:  u32 = 4;

#[derive(Copy, Clone)]
pub struct Vma {
    pub start:  u64,
    pub end:    u64,
    pub prot:   u32,
    pub active: bool,
}
impl Vma {
    const fn empty() -> Self { Self { start: 0, end: 0, prot: 0, active: false } }
}

pub struct VmaMap {
    vmas:      [Vma; MAX_VMAS],
    count:     usize,
    mmap_bump: u64,
    pub brk:   u64,
}
impl VmaMap {
    pub fn new() -> Self {
        Self { vmas: [Vma::empty(); MAX_VMAS], count: 0, mmap_bump: MMAP_BASE, brk: BRK_BASE }
    }
    pub fn set_brk_base(&mut self, a: u64) {
        self.brk = (a + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    }
    fn insert(&mut self, v: Vma) -> bool {
        for s in self.vmas.iter_mut() {
            if !s.active { *s = v; s.active = true; self.count += 1; return true; }
        }
        false
    }
    fn remove(&mut self, start: u64) {
        for s in self.vmas.iter_mut() {
            if s.active && s.start == start { s.active = false; self.count -= 1; return; }
        }
    }
    fn overlaps(&self, s: u64, e: u64) -> bool {
        self.vmas.iter().any(|v| v.active && v.start < e && v.end > s)
    }
    fn find_free(&mut self, size: u64) -> Option<u64> {
        let mut c = self.mmap_bump;
        while c + size <= MMAP_LIMIT {
            if !self.overlaps(c, c + size) { self.mmap_bump = c + size; return Some(c); }
            c += PAGE_SIZE;
        }
        None
    }
}

static VMA_MAP: Mutex<BTreeMap<u64, VmaMap>> = Mutex::new(BTreeMap::new());

fn with_vma<F: FnOnce(&mut VmaMap) -> R, R>(cr3: u64, f: F) -> R {
    let mut map = VMA_MAP.lock();
    f(map.entry(cr3).or_insert_with(VmaMap::new))
}

pub fn vma_set_brk(cr3: u64, brk_base: u64) {
    with_vma(cr3, |m| m.set_brk_base(brk_base));
}

pub fn kernel_find_free(cr3: u64, size: u64) -> Option<u64> {
    with_vma(cr3, |m| m.find_free(size))
}

pub fn kernel_register_vma(cr3: u64, start: u64, end: u64, prot: u32) {
    with_vma(cr3, |m| {
        m.insert(Vma { start, end, prot, active: true });
    });
}

pub fn vma_cleanup(cr3: u64) { VMA_MAP.lock().remove(&cr3); }

fn prot_to_flags(prot: u32) -> PageTableFlags {
    let mut f = PageTableFlags::USER_ACCESSIBLE;
    if prot & PROT_WRITE != 0 { f |= PageTableFlags::WRITABLE; }
    if prot & PROT_EXEC  == 0 { f |= PageTableFlags::NO_EXECUTE; }
    f
}

pub fn sys_mmap(cr3: u64, addr: u64, length: u64, prot: u32, flags: u32, _fd: i64, _off: u64) -> i64 {
    if length == 0 { return -22; }
    let size = (length + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let base = if flags & 0x10 != 0 {
        if addr == 0 || addr & 0xFFF != 0 { return -22; }
        addr
    } else {
        match with_vma(cr3, |m| m.find_free(size)) { Some(a) => a, None => return -12 }
    };
    let hhdm  = crate::grub::hhdm();
    let pt    = prot_to_flags(prot);
    let aspace = AddressSpace::from_raw(cr3);
    let pages  = (size / PAGE_SIZE) as usize;
    let mut ok = true;
    let mut mapped = 0usize;
    for i in 0..pages {
        match pmm::alloc_frame() {
            Some(phys) => {
                unsafe { core::ptr::write_bytes((phys + hhdm) as *mut u8, 0, 4096); }
                if !aspace.map_page(base + i as u64 * PAGE_SIZE, phys, pt) {
                    pmm::free_frame(phys); ok = false; break;
                }
                mapped += 1;
            }
            None => { ok = false; break; }
        }
    }
    let _ = aspace.into_raw();
    if !ok {
        let c = AddressSpace::from_raw(cr3);
        for i in 0..mapped { c.unmap_page(base + i as u64 * PAGE_SIZE); }
        let _ = c.into_raw();
        return -12;
    }
    with_vma(cr3, |m| m.insert(Vma { start: base, end: base + size, prot, active: true }));
    crate::serial_println!("[mmap] {:#x}+{:#x} prot={}", base, size, prot);
    base as i64
}

pub fn sys_munmap(cr3: u64, addr: u64, length: u64) -> i64 {
    if addr & 0xFFF != 0 { return -22; }
    let size = (length + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let a = AddressSpace::from_raw(cr3);
    let mut p = addr;
    while p < addr + size { a.unmap_page(p); p += PAGE_SIZE; }
    let _ = a.into_raw();
    with_vma(cr3, |m| m.remove(addr));
    0
}

pub fn sys_mprotect(cr3: u64, addr: u64, length: u64, prot: u32) -> i64 {
    if addr & 0xFFF != 0 { return -22; }
    let size  = (length + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let flags = prot_to_flags(prot);
    let a = AddressSpace::from_raw(cr3);
    let mut p = addr;
    while p < addr + size {
        if let Some(phys) = a.virt_to_phys(p) { a.unmap_page_no_free(p); a.map_page(p, phys, flags); }
        p += PAGE_SIZE;
    }
    let _ = a.into_raw();
    0
}

pub fn sys_brk(cr3: u64, new_brk: u64) -> u64 {
    let cur = with_vma(cr3, |m| m.brk);
    if new_brk == 0 { return cur; }
    let new  = (new_brk + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let hhdm = crate::grub::hhdm();
    if new <= cur {
        let a = AddressSpace::from_raw(cr3);
        let mut p = new; while p < cur { a.unmap_page(p); p += PAGE_SIZE; }
        let _ = a.into_raw();
        with_vma(cr3, |m| m.brk = new); return new;
    }
    let flags = PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE;
    let a = AddressSpace::from_raw(cr3);
    let mut p = cur; let mut ok = true;
    while p < new {
        match pmm::alloc_frame() {
            Some(phys) => {
                unsafe { core::ptr::write_bytes((phys + hhdm) as *mut u8, 0, 4096); }
                if !a.map_page(p, phys, flags) { pmm::free_frame(phys); ok = false; break; }
            }
            None => { ok = false; break; }
        }
        p += PAGE_SIZE;
    }
    let _ = a.into_raw();
    if ok { with_vma(cr3, |m| m.brk = new); new } else { cur }
}
