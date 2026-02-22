use crate::vfs::types::VfsError;

pub struct Slab<const N: usize> {
    free_stack: [u16; 64],
    free_top: u16,
    total_allocated: u16,
    active_bits: [u64; 1],
}

impl<const N: usize> Slab<N> {
    const MAX_ITEMS: usize = if N <= 64 { N } else { 64 };

    pub const fn new() -> Self {
        let mut free_stack = [0u16; 64];
        let mut i = 0;
        while i < 64 {
            free_stack[i] = i as u16;
            i += 1;
        }
        Self {
            free_stack,
            free_top: if N <= 64 { N as u16 } else { 64 },
            total_allocated: 0,
            active_bits: [0u64; 1],
        }
    }

    pub fn alloc(&mut self) -> Result<usize, VfsError> {
        if self.free_top == 0 {
            return Err(VfsError::NoSpace);
        }
        self.free_top -= 1;
        let idx = self.free_stack[self.free_top as usize] as usize;
        self.set_active(idx, true);
        self.total_allocated += 1;
        Ok(idx)
    }

    pub fn free(&mut self, idx: usize) {
        if idx < Self::MAX_ITEMS && self.is_active(idx) {
            self.set_active(idx, false);
            if (self.free_top as usize) < 64 {
                self.free_stack[self.free_top as usize] = idx as u16;
                self.free_top += 1;
            }
            if self.total_allocated > 0 {
                self.total_allocated -= 1;
            }
        }
    }

    #[inline]
    pub fn is_active(&self, idx: usize) -> bool {
        if idx >= Self::MAX_ITEMS {
            return false;
        }
        self.active_bits[0] & (1u64 << idx) != 0
    }

    #[inline]
    fn set_active(&mut self, idx: usize, active: bool) {
        if idx >= Self::MAX_ITEMS {
            return;
        }
        if active {
            self.active_bits[0] |= 1u64 << idx;
        } else {
            self.active_bits[0] &= !(1u64 << idx);
        }
    }

    pub fn count(&self) -> usize {
        self.total_allocated as usize
    }
    pub fn free_count(&self) -> usize {
        self.free_top as usize
    }
    pub fn capacity(&self) -> usize {
        Self::MAX_ITEMS
    }

    pub fn iter_active(&self) -> SlabIter<'_, N> {
        SlabIter { slab: self, pos: 0 }
    }
}

pub struct SlabIter<'a, const N: usize> {
    slab: &'a Slab<N>,
    pos: usize,
}

impl<'a, const N: usize> Iterator for SlabIter<'a, N> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.pos < Slab::<N>::MAX_ITEMS {
            let idx = self.pos;
            self.pos += 1;
            if self.slab.is_active(idx) {
                return Some(idx);
            }
        }
        None
    }
}
