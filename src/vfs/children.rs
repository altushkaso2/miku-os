extern crate alloc;
use alloc::vec::Vec;
use crate::vfs::hash::name_hash;
use crate::vfs::types::{InodeId, INVALID_ID};

const INITIAL_CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotState {
    Empty     = 0,
    Occupied  = 1,
    Tombstone = 2,
}

#[derive(Clone, Copy)]
pub struct ChildSlot {
    pub hash:  u32,
    pub id:    InodeId,
    pub state: SlotState,
}

impl ChildSlot {
    pub const EMPTY: Self = Self {
        hash:  0,
        id:    INVALID_ID,
        state: SlotState::Empty,
    };

    #[inline]
    pub fn used(&self) -> bool {
        self.state == SlotState::Occupied
    }
}

pub struct Children {
    pub slots: Vec<ChildSlot>,
    pub count: u32,
}

impl Children {
    pub const fn new() -> Self {
        Self { slots: Vec::new(), count: 0 }
    }

    fn ensure_init(&mut self) {
        if self.slots.is_empty() {
            self.slots = Vec::with_capacity(INITIAL_CAPACITY);
            self.slots.resize(INITIAL_CAPACITY, ChildSlot::EMPTY);
        }
    }

    #[inline]
    fn cap(&self) -> usize {
        self.slots.len()
    }

    fn needs_rehash(&self) -> bool {
        self.count as usize * 4 >= self.cap() * 3
    }

    fn rehash(&mut self) {
        let new_cap = self.cap() * 2;
        let mut new_slots = Vec::with_capacity(new_cap);
        new_slots.resize(new_cap, ChildSlot::EMPTY);

        for slot in &self.slots {
            if slot.state != SlotState::Occupied {
                continue;
            }
            let start = (slot.hash as usize) % new_cap;
            for i in 0..new_cap {
                let idx = (start + i) % new_cap;
                if new_slots[idx].state == SlotState::Empty {
                    new_slots[idx] = *slot;
                    break;
                }
            }
        }
        self.slots = new_slots;
    }

    pub fn insert(&mut self, name: &str, id: InodeId) -> bool {
        self.ensure_init();
        if self.needs_rehash() {
            self.rehash();
        }
        let h = name_hash(name);

        if self.contains_hash_and_id(h, id) {
            return false;
        }

        let cap = self.cap();
        let start = (h as usize) % cap;
        let mut tombstone_idx: Option<usize> = None;

        for i in 0..cap {
            let idx = (start + i) % cap;
            match self.slots[idx].state {
                SlotState::Empty => {
                    let dst = tombstone_idx.unwrap_or(idx);
                    self.slots[dst] = ChildSlot { hash: h, id, state: SlotState::Occupied };
                    self.count += 1;
                    return true;
                }
                SlotState::Tombstone => {
                    if tombstone_idx.is_none() {
                        tombstone_idx = Some(idx);
                    }
                }
                SlotState::Occupied => continue,
            }
        }

        if let Some(idx) = tombstone_idx {
            self.slots[idx] = ChildSlot { hash: h, id, state: SlotState::Occupied };
            self.count += 1;
            return true;
        }

        false
    }

    fn contains_hash_and_id(&self, h: u32, id: InodeId) -> bool {
        let cap = self.cap();
        let start = (h as usize) % cap;
        for i in 0..cap {
            let idx = (start + i) % cap;
            match self.slots[idx].state {
                SlotState::Empty     => return false,
                SlotState::Tombstone => continue,
                SlotState::Occupied  => {
                    if self.slots[idx].hash == h && self.slots[idx].id == id {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn find_by_hash(&self, h: u32) -> ChildHashIter<'_> {
        let cap = self.cap();
        let start = if cap == 0 { 0 } else { (h as usize) % cap };
        ChildHashIter {
            children: self,
            hash: h,
            pos: 0,
            start,
        }
    }

    pub fn get_first(&self, h: u32) -> Option<InodeId> {
        let cap = self.cap();
        if cap == 0 { return None; }
        let start = (h as usize) % cap;
        for i in 0..cap {
            let idx = (start + i) % cap;
            match self.slots[idx].state {
                SlotState::Empty     => return None,
                SlotState::Tombstone => continue,
                SlotState::Occupied  => {
                    if self.slots[idx].hash == h {
                        return Some(self.slots[idx].id);
                    }
                }
            }
        }
        None
    }

    pub fn remove(&mut self, h: u32, id: InodeId) -> bool {
        let cap = self.cap();
        if cap == 0 { return false; }
        let start = (h as usize) % cap;
        for i in 0..cap {
            let idx = (start + i) % cap;
            match self.slots[idx].state {
                SlotState::Empty     => return false,
                SlotState::Tombstone => continue,
                SlotState::Occupied  => {
                    if self.slots[idx].hash == h && self.slots[idx].id == id {
                        self.slots[idx].state = SlotState::Tombstone;
                        if self.count > 0 { self.count -= 1; }
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn remove_by_hash(&mut self, h: u32) -> Option<InodeId> {
        let cap = self.cap();
        if cap == 0 { return None; }
        let start = (h as usize) % cap;
        for i in 0..cap {
            let idx = (start + i) % cap;
            match self.slots[idx].state {
                SlotState::Empty     => return None,
                SlotState::Tombstone => continue,
                SlotState::Occupied  => {
                    if self.slots[idx].hash == h {
                        let id = self.slots[idx].id;
                        self.slots[idx].state = SlotState::Tombstone;
                        if self.count > 0 { self.count -= 1; }
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    pub fn clear(&mut self) {
        for slot in self.slots.iter_mut() {
            *slot = ChildSlot::EMPTY;
        }
        self.count = 0;
    }

    pub fn is_empty(&self) -> bool { self.count == 0 }
    pub fn len(&self) -> usize { self.count as usize }
    pub fn is_full(&self) -> bool { false }

    pub fn iter(&self) -> ChildIter<'_> {
        ChildIter { children: self, pos: 0 }
    }
}

pub struct ChildIter<'a> {
    children: &'a Children,
    pos:      usize,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = (u32, InodeId);

    fn next(&mut self) -> Option<Self::Item> {
        while self.pos < self.children.slots.len() {
            let idx = self.pos;
            self.pos += 1;
            if self.children.slots[idx].state == SlotState::Occupied {
                return Some((self.children.slots[idx].hash, self.children.slots[idx].id));
            }
        }
        None
    }
}

pub struct ChildHashIter<'a> {
    children: &'a Children,
    hash:     u32,
    pos:      usize,
    start:    usize,
}

impl<'a> Iterator for ChildHashIter<'a> {
    type Item = InodeId;

    fn next(&mut self) -> Option<InodeId> {
        let cap = self.children.slots.len();
        if cap == 0 { return None; }
        while self.pos < cap {
            let idx = (self.start + self.pos) % cap;
            self.pos += 1;
            match self.children.slots[idx].state {
                SlotState::Empty     => return None,
                SlotState::Tombstone => continue,
                SlotState::Occupied  => {
                    if self.children.slots[idx].hash == self.hash {
                        return Some(self.children.slots[idx].id);
                    }
                }
            }
        }
        None
    }
}
