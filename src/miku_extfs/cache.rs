extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

const MAX_CACHE_ENTRIES: usize = 32;

#[derive(Clone, Copy)]
struct CacheEntry {
    block_num: u32,
    valid: bool,
    dirty: bool,
    last_access: u64,
}

impl CacheEntry {
    const fn empty() -> Self {
        Self {
            block_num: 0,
            valid: false,
            dirty: false,
            last_access: 0,
        }
    }
}

pub struct BlockCache {
    buffer: Vec<u8>,
    entries: [CacheEntry; MAX_CACHE_ENTRIES],
    block_size: usize,
    count: usize,
    access_counter: u64,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

impl BlockCache {
    pub fn new(block_size: usize, max_entries: usize) -> Self {
        let count = max_entries.min(MAX_CACHE_ENTRIES);
        let buffer = vec![0u8; count * block_size];
        crate::serial_println!(
            "[cache] allocated {} entries x {} bytes = {} KB",
            count,
            block_size,
            (count * block_size) / 1024
        );
        Self {
            buffer,
            entries: [CacheEntry::empty(); MAX_CACHE_ENTRIES],
            block_size,
            count,
            access_counter: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    pub fn get(&mut self, block_num: u32, buf: &mut [u8]) -> bool {
        for i in 0..self.count {
            if self.entries[i].valid && self.entries[i].block_num == block_num {
                let offset = i * self.block_size;
                let copy_len = buf.len().min(self.block_size);
                buf[..copy_len].copy_from_slice(&self.buffer[offset..offset + copy_len]);
                self.access_counter += 1;
                self.entries[i].last_access = self.access_counter;
                self.hits += 1;
                return true;
            }
        }
        self.misses += 1;
        false
    }

    pub fn put(&mut self, block_num: u32, data: &[u8]) {
        for i in 0..self.count {
            if self.entries[i].valid && self.entries[i].block_num == block_num {
                let offset = i * self.block_size;
                let copy_len = data.len().min(self.block_size);
                self.buffer[offset..offset + copy_len].copy_from_slice(&data[..copy_len]);
                self.access_counter += 1;
                self.entries[i].last_access = self.access_counter;
                return;
            }
        }

        let slot = self.find_slot();
        if self.entries[slot].valid {
            self.evictions += 1;
        }
        let offset = slot * self.block_size;
        let copy_len = data.len().min(self.block_size);
        self.buffer[offset..offset + copy_len].copy_from_slice(&data[..copy_len]);
        self.access_counter += 1;
        self.entries[slot] = CacheEntry {
            block_num,
            valid: true,
            dirty: false,
            last_access: self.access_counter,
        };
    }

    fn find_slot(&self) -> usize {
        for i in 0..self.count {
            if !self.entries[i].valid {
                return i;
            }
        }
        let mut lru_idx = 0;
        let mut lru_val = u64::MAX;
        for i in 0..self.count {
            if self.entries[i].last_access < lru_val {
                lru_val = self.entries[i].last_access;
                lru_idx = i;
            }
        }
        lru_idx
    }

    pub fn invalidate(&mut self, block_num: u32) {
        for i in 0..self.count {
            if self.entries[i].valid && self.entries[i].block_num == block_num {
                self.entries[i].valid = false;
            }
        }
    }

    pub fn clear(&mut self) {
        for i in 0..self.count {
            self.entries[i].valid = false;
        }
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
        self.access_counter = 0;
    }

    pub fn cached_entries(&self) -> usize {
        let mut n = 0;
        for i in 0..self.count {
            if self.entries[i].valid {
                n += 1;
            }
        }
        n
    }

    pub fn capacity(&self) -> usize {
        self.count
    }

    pub fn hit_rate(&self) -> u64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0
        } else {
            (self.hits * 100) / total
        }
    }

    pub fn total_bytes(&self) -> usize {
        self.count * self.block_size
    }
}
