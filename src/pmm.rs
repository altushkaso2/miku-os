use spin::Mutex;

const MAX_FRAMES: usize = 524288;
const FRAME_SIZE: usize = 4096;

struct FrameAllocator {
    bitmap: [u64; MAX_FRAMES / 64],
    total:  usize,
    used:   usize,
}

impl FrameAllocator {
    const fn new() -> Self {
        Self {
            bitmap: [u64::MAX; MAX_FRAMES / 64],
            total:  0,
            used:   0,
        }
    }

    fn mark_used(&mut self, frame: usize) {
        self.bitmap[frame / 64] |= 1 << (frame % 64);
    }

    fn mark_free(&mut self, frame: usize) {
        self.bitmap[frame / 64] &= !(1 << (frame % 64));
    }

    fn is_used(&self, frame: usize) -> bool {
        self.bitmap[frame / 64] & (1 << (frame % 64)) != 0
    }

    fn add_region(&mut self, base: u64, size: u64) {
        let start_frame = (base as usize + FRAME_SIZE - 1) / FRAME_SIZE;
        let end_frame = (base as usize + size as usize) / FRAME_SIZE;

        for i in start_frame..end_frame {
            if i < MAX_FRAMES {
                self.mark_free(i);
                self.total += 1;
            }
        }
        crate::serial_println!(
            "[pmm] added region: base={:#x} size={}MB",
            base,
            size / 1024 / 1024
        );
    }

    fn alloc_frames(&mut self, count: usize) -> Option<u64> {
        let mut consecutive = 0;
        let mut start_idx = 0;

        for i in 0..MAX_FRAMES {
            if !self.is_used(i) {
                if consecutive == 0 {
                    start_idx = i;
                }
                consecutive += 1;
                if consecutive == count {
                    for j in start_idx..(start_idx + count) {
                        self.mark_used(j);
                    }
                    self.used += count;
                    return Some((start_idx * FRAME_SIZE) as u64);
                }
            } else {
                consecutive = 0;
            }
        }
        None
    }

    fn free_frames(&mut self, phys: u64, count: usize) {
        let start_frame = phys as usize / FRAME_SIZE;
        for i in start_frame..(start_frame + count) {
            if i < MAX_FRAMES && self.is_used(i) {
                self.mark_free(i);
                self.used -= 1;
            }
        }
    }
}

static PMM: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());

pub fn add_region(base: u64, size: u64) {
    PMM.lock().add_region(base, size);
}

pub fn alloc_frame() -> Option<u64> {
    PMM.lock().alloc_frames(1)
}

pub fn alloc_frames(count: usize) -> Option<u64> {
    PMM.lock().alloc_frames(count)
}

pub fn free_frame(phys: u64) {
    PMM.lock().free_frames(phys, 1);
}

pub fn free_frames(phys: u64, count: usize) {
    PMM.lock().free_frames(phys, count);
}

pub fn stats() -> (usize, usize) {
    let p = PMM.lock();
    (p.used, p.total)
}
