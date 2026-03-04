use spin::Mutex;
use crate::ata::AtaDrive;

const MAX_TRACKED: usize = 512 * 1024;

#[derive(Copy, Clone)]
struct ReverseEntry {
    cr3:       u64,
    virt_addr: u64,
    age:       u8,
    pinned:    bool,
}

impl ReverseEntry {
    const fn empty() -> Self { Self { cr3: 0, virt_addr: 0, age: 0, pinned: false } }
    #[inline] fn is_used(&self) -> bool { self.cr3 != 0 }
}

struct SwapMap {
    entries:    [ReverseEntry; MAX_TRACKED],
    clock_hand: usize,
    tracked:    usize,
}

impl SwapMap {
    const fn new() -> Self {
        Self { entries: [ReverseEntry::empty(); MAX_TRACKED], clock_hand: 0, tracked: 0 }
    }

    #[inline] fn frame_idx(phys: u64) -> usize { (phys / 4096) as usize }

    pub fn track(&mut self, phys: u64, cr3: u64, virt: u64, pinned: bool) {
        let idx = Self::frame_idx(phys);
        if idx >= MAX_TRACKED { return; }
        if !self.entries[idx].is_used() { self.tracked += 1; }
        self.entries[idx] = ReverseEntry { cr3, virt_addr: virt, age: 1, pinned };
    }

    pub fn untrack(&mut self, phys: u64) {
        let idx = Self::frame_idx(phys);
        if idx >= MAX_TRACKED { return; }
        if self.entries[idx].is_used() { self.tracked = self.tracked.saturating_sub(1); }
        self.entries[idx] = ReverseEntry::empty();
    }

    pub fn touch(&mut self, phys: u64) {
        let idx = Self::frame_idx(phys);
        if idx < MAX_TRACKED && self.entries[idx].is_used() {
            self.entries[idx].age = 1;
        }
    }

    pub fn age_all(&mut self) {
        for e in self.entries.iter_mut() {
            if e.is_used() && !e.pinned {
                e.age = e.age.saturating_add(1);
            }
        }
    }

    pub fn pick_victim(&mut self) -> Option<(u64, u64, u64)> {
        if self.tracked == 0 { return None; }
        let n = MAX_TRACKED;

        for _ in 0..n {
            let idx = self.clock_hand;
            self.clock_hand = (self.clock_hand + 1) % n;
            let e = &self.entries[idx];
            if e.is_used() && !e.pinned && e.age >= 3 {
                return Some((idx as u64 * 4096, e.cr3, e.virt_addr));
            }
        }

        for idx in 0..n {
            let e = &self.entries[idx];
            if e.is_used() && !e.pinned {
                self.clock_hand = (idx + 1) % n;
                return Some((idx as u64 * 4096, e.cr3, e.virt_addr));
            }
        }
        None
    }
}

static SWAP_MAP: Mutex<SwapMap> = Mutex::new(SwapMap::new());

pub fn track(phys: u64, cr3: u64, virt: u64, pinned: bool) {
    SWAP_MAP.lock().track(phys, cr3, virt, pinned);
}

pub fn untrack(phys: u64) {
    SWAP_MAP.lock().untrack(phys);
}

pub fn touch(phys: u64) {
    SWAP_MAP.lock().touch(phys);
}

pub fn age_all() {
    SWAP_MAP.lock().age_all();
}

const SWAP_PTE_MARKER: u64     = 0b10;
const SWAP_PTE_SLOT_SHIFT: u64 = 12;

pub fn make_swap_pte(slot: u32) -> u64 {
    SWAP_PTE_MARKER | ((slot as u64) << SWAP_PTE_SLOT_SHIFT)
}

pub fn is_swap_pte(raw: u64) -> bool {
    (raw & 1) == 0 && (raw & SWAP_PTE_MARKER) != 0 && raw != 0
}

pub fn slot_from_pte(raw: u64) -> u32 {
    ((raw >> SWAP_PTE_SLOT_SHIFT) & 0xF_FFFF) as u32
}

fn make_drive(idx: usize) -> AtaDrive {
    match idx {
        0 => AtaDrive::primary(),
        1 => AtaDrive::primary_slave(),
        2 => AtaDrive::secondary(),
        _ => AtaDrive::secondary_slave(),
    }
}

pub fn evict_one() -> Option<u64> {
    use crate::swap;
    if !swap::swap_is_active() { return None; }
    if swap::swap_free_pages() == 0 {
        crate::serial_println!("[swap_map] swap full - cannot evict");
        return None;
    }

    let (phys, cr3, virt) = SWAP_MAP.lock().pick_victim()?;
    let mut drive = make_drive(swap::swap_drive_idx());

    let slot = match swap::swap_out_internal(phys, &mut drive) {
        Ok(s) => s,
        Err(e) => {
            crate::serial_println!("[swap_map] swap_out failed: {:?}", e);
            return None;
        }
    };

    unsafe { crate::vmm::mark_swapped(cr3, virt, slot); }
    SWAP_MAP.lock().untrack(phys);
    crate::pmm::free_frame(phys);

    crate::serial_println!("[swap_map] evicted virt={:#x} slot={} phys={:#x}", virt, slot, phys);
    Some(phys)
}

pub fn alloc_or_evict() -> Option<u64> {
    if let Some(f) = crate::pmm::alloc_frame() { return Some(f); }
    evict_one()?;
    if let Some(f) = crate::pmm::alloc_frame() { return Some(f); }
    crate::pmm::alloc_frame_emergency()
}

pub fn alloc_for_swapin() -> Option<u64> {
    crate::pmm::alloc_frame_emergency()
}

pub fn refill_emergency_pool_tick() {
    if crate::pmm::emergency_frames_available() >= 32 {
        return;
    }
    while crate::pmm::emergency_frames_available() < 64 {
        if evict_one().is_none() { break; }
    }
}
