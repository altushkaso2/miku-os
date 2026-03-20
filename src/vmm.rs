use crate::pmm;
use crate::grub;
use x86_64::structures::paging::{page_table::PageTableEntry, PageTable, PageTableFlags};
use x86_64::registers::control::Cr3;

pub struct AddressSpace {
    pub cr3: u64,
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        if self.cr3 == 0 { return; }
        if self.cr3 == kernel_cr3() { return; }
        self.free_address_space();
    }
}

impl AddressSpace {
    pub fn new_user() -> Option<Self> {
        let cr3  = pmm::alloc_frame()?;
        let hhdm = grub::hhdm();
        unsafe {
            let p4 = (cr3 + hhdm) as *mut PageTable;
            (*p4)  = PageTable::new();
            let (kf, _) = Cr3::read();
            let kp4 = (kf.start_address().as_u64() + hhdm) as *const PageTable;
            for i in 256..512 {
                (&mut *p4)[i] = (&*kp4)[i].clone();
            }
        }
        Some(Self { cr3 })
    }

    pub fn into_raw(mut self) -> u64 {
        let cr3 = self.cr3;
        self.cr3 = 0;
        cr3
    }

    pub fn from_raw(cr3: u64) -> Self {
        Self { cr3 }
    }

    pub fn free_address_space_manual(&mut self) {
        if self.cr3 == 0 { return; }
        self.free_address_space();
        self.cr3 = 0;
    }

    pub fn map_page(&self, virt: u64, phys: u64, flags: PageTableFlags) -> bool {
        let hhdm = grub::hhdm();
        unsafe {
            let p4 = (self.cr3 + hhdm) as *mut PageTable;
            let p4i = ((virt >> 39) & 0x1FF) as usize;
            let p3i = ((virt >> 30) & 0x1FF) as usize;
            let p2i = ((virt >> 21) & 0x1FF) as usize;
            let p1i = ((virt >> 12) & 0x1FF) as usize;
            let Some(p3) = get_or_create(&mut (&mut *p4)[p4i], hhdm) else { return false; };
            let Some(p2) = get_or_create(&mut (&mut *p3)[p3i], hhdm) else { return false; };
            let Some(p1) = get_or_create(&mut (&mut *p2)[p2i], hhdm) else { return false; };
            (&mut *p1)[p1i].set_addr(
                x86_64::PhysAddr::new(phys),
                flags | PageTableFlags::PRESENT,
            );
            let pinned = virt >= 0xFFFF_8000_0000_0000 || phys < 0x40_0000;
            crate::swap_map::track(phys, self.cr3, virt, pinned);
        }
        true
    }

    pub fn map_range(&self, virt: u64, phys: u64, size: u64, flags: PageTableFlags) -> bool {
        let mut cv = virt & !0xFFF;
        let mut cp = phys & !0xFFF;
        let end    = (virt + size + 0xFFF) & !0xFFF;
        while cv < end {
            if !self.map_page(cv, cp, flags) { return false; }
            cv += 4096;
            cp += 4096;
        }
        x86_64::instructions::tlb::flush_all();
        true
    }

    pub fn unmap_page(&self, virt: u64) {
        let hhdm = grub::hhdm();
        unsafe {
            let p4 = (self.cr3 + hhdm) as *mut PageTable;
            let p4i = ((virt >> 39) & 0x1FF) as usize;
            if !(&*p4)[p4i].flags().contains(PageTableFlags::PRESENT) { return; }
            let p3 = ((&*p4)[p4i].addr().as_u64() + hhdm) as *mut PageTable;
            let p3i = ((virt >> 30) & 0x1FF) as usize;
            if !(&*p3)[p3i].flags().contains(PageTableFlags::PRESENT) { return; }
            let p2 = ((&*p3)[p3i].addr().as_u64() + hhdm) as *mut PageTable;
            let p2i = ((virt >> 21) & 0x1FF) as usize;
            if !(&*p2)[p2i].flags().contains(PageTableFlags::PRESENT) { return; }
            let p1  = ((&*p2)[p2i].addr().as_u64() + hhdm) as *mut PageTable;
            let p1i = ((virt >> 12) & 0x1FF) as usize;
            let raw = &mut (&mut *p1)[p1i] as *mut _ as *mut u64;
            let pte = *raw;
            if crate::swap_map::is_swap_pte(pte) {
                crate::swap::free_swap_slot(crate::swap_map::slot_from_pte(pte));
            } else if (&*p1)[p1i].flags().contains(PageTableFlags::PRESENT) {
                let phys = (&*p1)[p1i].addr().as_u64();
                crate::swap_map::untrack(phys);
                pmm::free_frame(phys);
            }
            (&mut *p1)[p1i].set_unused();
            x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
        }
    }

    pub fn unmap_page_no_free(&self, virt: u64) -> bool {
        let hhdm = grub::hhdm();
        unsafe {
            let p4 = (self.cr3 + hhdm) as *mut PageTable;
            let p4i = ((virt >> 39) & 0x1FF) as usize;
            if !(&*p4)[p4i].flags().contains(PageTableFlags::PRESENT) { return false; }
            let p3 = ((&*p4)[p4i].addr().as_u64() + hhdm) as *mut PageTable;
            let p3i = ((virt >> 30) & 0x1FF) as usize;
            if !(&*p3)[p3i].flags().contains(PageTableFlags::PRESENT) { return false; }
            let p2 = ((&*p3)[p3i].addr().as_u64() + hhdm) as *mut PageTable;
            let p2i = ((virt >> 21) & 0x1FF) as usize;
            if !(&*p2)[p2i].flags().contains(PageTableFlags::PRESENT) { return false; }
            let p1  = ((&*p2)[p2i].addr().as_u64() + hhdm) as *mut PageTable;
            let p1i = ((virt >> 12) & 0x1FF) as usize;
            if (&*p1)[p1i].flags().contains(PageTableFlags::PRESENT) {
                let phys = (&*p1)[p1i].addr().as_u64();
                crate::swap_map::untrack(phys);
                (&mut *p1)[p1i].set_unused();
                x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
                return true;
            }
        }
        false
    }

    pub fn free_address_space(&mut self) {
        if self.cr3 == 0 { return; }
        let hhdm = grub::hhdm();
        unsafe {
            let p4 = (self.cr3 + hhdm) as *mut PageTable;
            for i in 0..256 {
                if !(&*p4)[i].flags().contains(PageTableFlags::PRESENT) { continue; }
                let p3 = ((&*p4)[i].addr().as_u64() + hhdm) as *mut PageTable;
                for j in 0..512 {
                    if !(&*p3)[j].flags().contains(PageTableFlags::PRESENT) { continue; }
                    let p2 = ((&*p3)[j].addr().as_u64() + hhdm) as *mut PageTable;
                    for k in 0..512 {
                        if !(&*p2)[k].flags().contains(PageTableFlags::PRESENT) { continue; }
                        let p1 = ((&*p2)[k].addr().as_u64() + hhdm) as *mut PageTable;
                        for m in 0..512 {
                            let raw = &mut (&mut *p1)[m] as *mut _ as *mut u64;
                            let pte = *raw;
                            if crate::swap_map::is_swap_pte(pte) {
                                crate::swap::free_swap_slot(crate::swap_map::slot_from_pte(pte));
                            } else if (&*p1)[m].flags().contains(PageTableFlags::PRESENT) {
                                let phys = (&*p1)[m].addr().as_u64();
                                crate::swap_map::untrack(phys);
                                pmm::free_frame(phys);
                            }
                        }
                        pmm::free_frame((&*p2)[k].addr().as_u64());
                    }
                    pmm::free_frame((&*p3)[j].addr().as_u64());
                }
                pmm::free_frame((&*p4)[i].addr().as_u64());
            }
        }
        pmm::free_frame(self.cr3);
        self.cr3 = 0;
    }

    pub fn activate(&self) {
        unsafe {
            core::arch::asm!(
                "mov cr3, {}",
                in(reg) self.cr3,
                options(nostack, preserves_flags)
            );
        }
    }

    pub fn virt_to_phys(&self, virt: u64) -> Option<u64> {
        let hhdm = grub::hhdm();
        unsafe {
            let p4 = (self.cr3 + hhdm) as *const PageTable;
            let e4 = &(&*p4)[(virt >> 39 & 0x1FF) as usize];
            if !e4.flags().contains(PageTableFlags::PRESENT) { return None; }
            let p3 = (e4.addr().as_u64() + hhdm) as *const PageTable;
            let e3 = &(&*p3)[(virt >> 30 & 0x1FF) as usize];
            if !e3.flags().contains(PageTableFlags::PRESENT) { return None; }
            let p2 = (e3.addr().as_u64() + hhdm) as *const PageTable;
            let e2 = &(&*p2)[(virt >> 21 & 0x1FF) as usize];
            if !e2.flags().contains(PageTableFlags::PRESENT) { return None; }
            let p1 = (e2.addr().as_u64() + hhdm) as *const PageTable;
            let e1 = &(&*p1)[(virt >> 12 & 0x1FF) as usize];
            if !e1.flags().contains(PageTableFlags::PRESENT) { return None; }
            Some(e1.addr().as_u64())
        }
    }

    pub fn get_page_flags(&self, virt: u64) -> Option<PageTableFlags> {
        let hhdm = grub::hhdm();
        unsafe {
            let p4 = (self.cr3 + hhdm) as *const PageTable;
            let e4 = &(&*p4)[(virt >> 39 & 0x1FF) as usize];
            if !e4.flags().contains(PageTableFlags::PRESENT) { return None; }
            let p3 = (e4.addr().as_u64() + hhdm) as *const PageTable;
            let e3 = &(&*p3)[(virt >> 30 & 0x1FF) as usize];
            if !e3.flags().contains(PageTableFlags::PRESENT) { return None; }
            let p2 = (e3.addr().as_u64() + hhdm) as *const PageTable;
            let e2 = &(&*p2)[(virt >> 21 & 0x1FF) as usize];
            if !e2.flags().contains(PageTableFlags::PRESENT) { return None; }
            let p1 = (e2.addr().as_u64() + hhdm) as *const PageTable;
            let e1 = &(&*p1)[(virt >> 12 & 0x1FF) as usize];
            if !e1.flags().contains(PageTableFlags::PRESENT) { return None; }
            Some(e1.flags())
        }
    }

    pub fn read_pte_raw(&self, virt: u64) -> Option<u64> {
        let hhdm = grub::hhdm();
        unsafe {
            let p4 = (self.cr3 + hhdm) as *const PageTable;
            let e4 = &(&*p4)[(virt >> 39 & 0x1FF) as usize];
            if !e4.flags().contains(PageTableFlags::PRESENT) { return None; }
            let p3 = (e4.addr().as_u64() + hhdm) as *const PageTable;
            let e3 = &(&*p3)[(virt >> 30 & 0x1FF) as usize];
            if !e3.flags().contains(PageTableFlags::PRESENT) { return None; }
            let p2 = (e3.addr().as_u64() + hhdm) as *const PageTable;
            let e2 = &(&*p2)[(virt >> 21 & 0x1FF) as usize];
            if !e2.flags().contains(PageTableFlags::PRESENT) { return None; }
            let p1 = (e2.addr().as_u64() + hhdm) as *mut PageTable;
            let raw = &mut (&mut *p1)[(virt >> 12 & 0x1FF) as usize] as *mut _ as *mut u64;
            Some(*raw)
        }
    }

    pub unsafe fn mark_swapped(&self, virt: u64, slot: u32) {
        let hhdm    = grub::hhdm();
        let pte_val = crate::swap_map::make_swap_pte(slot);
        unsafe {
            let p4 = (self.cr3 + hhdm) as *const PageTable;
            let e4 = &(&*p4)[(virt >> 39 & 0x1FF) as usize];
            if !e4.flags().contains(PageTableFlags::PRESENT) { return; }
            let p3 = (e4.addr().as_u64() + hhdm) as *const PageTable;
            let e3 = &(&*p3)[(virt >> 30 & 0x1FF) as usize];
            if !e3.flags().contains(PageTableFlags::PRESENT) { return; }
            let p2 = (e3.addr().as_u64() + hhdm) as *const PageTable;
            let e2 = &(&*p2)[(virt >> 21 & 0x1FF) as usize];
            if !e2.flags().contains(PageTableFlags::PRESENT) { return; }
            let p1 = (e2.addr().as_u64() + hhdm) as *mut PageTable;
            let raw = &mut (&mut *p1)[(virt >> 12 & 0x1FF) as usize] as *mut _ as *mut u64;
            *raw = pte_val;
            x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
        }
    }
}

unsafe fn get_or_create(entry: &mut PageTableEntry, hhdm: u64) -> Option<*mut PageTable> {
    if !entry.flags().contains(PageTableFlags::PRESENT) {
        let frame = pmm::alloc_frame()?;
        let table = (frame + hhdm) as *mut PageTable;
        (*table)  = PageTable::new();
        entry.set_addr(
            x86_64::PhysAddr::new(frame),
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
        );
        Some(table)
    } else {
        Some((entry.addr().as_u64() + hhdm) as *mut PageTable)
    }
}

pub fn kernel_cr3() -> u64 {
    let (frame, _) = Cr3::read();
    frame.start_address().as_u64()
}

pub unsafe fn mark_swapped(cr3: u64, virt: u64, slot: u32) {
    let aspace = AddressSpace::from_raw(cr3);
    unsafe { aspace.mark_swapped(virt, slot); }
    let _ = aspace.into_raw();
}

pub fn read_pte_raw(cr3: u64, virt: u64) -> Option<u64> {
    let aspace = AddressSpace::from_raw(cr3);
    let result = aspace.read_pte_raw(virt);
    let _ = aspace.into_raw();
    result
}
