use crate::pmm;
use crate::net::{phys_to_virt, virt_to_phys, HHDM_OFFSET};
use core::sync::atomic::Ordering;
use x86_64::structures::paging::{page_table::PageTableEntry, PageTable, PageTableFlags};
use x86_64::registers::control::Cr3;

pub struct AddressSpace {
    pub cr3: u64,
}

impl AddressSpace {
    pub fn new_user() -> Option<Self> {
        let cr3 = pmm::alloc_frame()?;
        let hhdm = HHDM_OFFSET.load(Ordering::Relaxed);

        unsafe {
            let p4 = (cr3 + hhdm) as *mut PageTable;
            (*p4) = PageTable::new();

            let (kernel_p4_frame, _) = Cr3::read();
            let kernel_p4 = (kernel_p4_frame.start_address().as_u64() + hhdm) as *const PageTable;

            for i in 256..512 {
                (&mut *p4)[i] = (&*kernel_p4)[i].clone();
            }
        }

        Some(Self { cr3 })
    }

    pub fn map_page(&self, virt: u64, phys: u64, flags: PageTableFlags) -> bool {
        let hhdm = HHDM_OFFSET.load(Ordering::Relaxed);

        unsafe {
            let p4 = (self.cr3 + hhdm) as *mut PageTable;

            let p4_idx = ((virt >> 39) & 0x1FF) as usize;
            let p3_idx = ((virt >> 30) & 0x1FF) as usize;
            let p2_idx = ((virt >> 21) & 0x1FF) as usize;
            let p1_idx = ((virt >> 12) & 0x1FF) as usize;

            let Some(p3) = get_or_create(&mut (&mut *p4)[p4_idx], hhdm) else { return false; };
            let Some(p2) = get_or_create(&mut (&mut *p3)[p3_idx], hhdm) else { return false; };
            let Some(p1) = get_or_create(&mut (&mut *p2)[p2_idx], hhdm) else { return false; };

            (&mut *p1)[p1_idx].set_addr(
                x86_64::PhysAddr::new(phys),
                flags | PageTableFlags::PRESENT,
            );
        }

        true
    }

    pub fn map_range(&self, virt: u64, phys: u64, size: u64, flags: PageTableFlags) -> bool {
        let start_page = virt & !0xFFF;
        let end_page = (virt + size + 0xFFF) & !0xFFF;
        let mut current_virt = start_page;
        let mut current_phys = phys & !0xFFF;

        while current_virt < end_page {
            if !self.map_page(current_virt, current_phys, flags) {
                return false;
            }
            current_virt += 4096;
            current_phys += 4096;
        }
        
        x86_64::instructions::tlb::flush_all();
        true
    }

    pub fn unmap_page(&self, virt: u64) {
        let hhdm = HHDM_OFFSET.load(Ordering::Relaxed);
        unsafe {
            let p4 = (self.cr3 + hhdm) as *mut PageTable;
            let p4_idx = ((virt >> 39) & 0x1FF) as usize;
            if !(&*p4)[p4_idx].flags().contains(PageTableFlags::PRESENT) { return; }

            let p3 = ((&*p4)[p4_idx].addr().as_u64() + hhdm) as *mut PageTable;
            let p3_idx = ((virt >> 30) & 0x1FF) as usize;
            if !(&*p3)[p3_idx].flags().contains(PageTableFlags::PRESENT) { return; }

            let p2 = ((&*p3)[p3_idx].addr().as_u64() + hhdm) as *mut PageTable;
            let p2_idx = ((virt >> 21) & 0x1FF) as usize;
            if !(&*p2)[p2_idx].flags().contains(PageTableFlags::PRESENT) { return; }

            let p1 = ((&*p2)[p2_idx].addr().as_u64() + hhdm) as *mut PageTable;
            let p1_idx = ((virt >> 12) & 0x1FF) as usize;

            (&mut *p1)[p1_idx].set_unused();
            x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
        }
    }

    pub fn free_address_space(&mut self) {
        let hhdm = HHDM_OFFSET.load(Ordering::Relaxed);
        unsafe {
            let p4 = (self.cr3 + hhdm) as *mut PageTable;
            for i in 0..256 {
                if (&*p4)[i].flags().contains(PageTableFlags::PRESENT) {
                    let p3 = ((&*p4)[i].addr().as_u64() + hhdm) as *mut PageTable;
                    for j in 0..512 {
                        if (&*p3)[j].flags().contains(PageTableFlags::PRESENT) {
                            let p2 = ((&*p3)[j].addr().as_u64() + hhdm) as *mut PageTable;
                            for k in 0..512 {
                                if (&*p2)[k].flags().contains(PageTableFlags::PRESENT) {
                                    pmm::free_frame((&*p2)[k].addr().as_u64());
                                }
                            }
                            pmm::free_frame((&*p3)[j].addr().as_u64());
                        }
                    }
                    pmm::free_frame((&*p4)[i].addr().as_u64());
                }
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
}

unsafe fn get_or_create(entry: &mut PageTableEntry, hhdm: u64) -> Option<*mut PageTable> {
    if !entry.flags().contains(PageTableFlags::PRESENT) {
        let frame = pmm::alloc_frame()?;
        let table = (frame + hhdm) as *mut PageTable;
        (*table) = PageTable::new();
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
