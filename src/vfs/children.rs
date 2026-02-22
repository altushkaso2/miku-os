use crate::vfs::hash::name_hash;
use crate::vfs::types::{InodeId, INVALID_ID, MAX_CHILDREN};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotState {
    Empty = 0,
    Occupied = 1,
    Tombstone = 2,
}

#[derive(Clone, Copy)]
pub struct ChildSlot {
    pub hash: u32,
    pub id: InodeId,
    pub state: SlotState,
}

impl ChildSlot {
    pub const EMPTY: Self = Self {
        hash: 0,
        id: INVALID_ID,
        state: SlotState::Empty,
    };

    #[inline]
    pub fn used(&self) -> bool {
        self.state == SlotState::Occupied
    }
}

pub struct Children {
    pub slots: [ChildSlot; MAX_CHILDREN],
    pub count: u8,
}

impl Children {
    pub const fn new() -> Self {
        Self {
            slots: [ChildSlot::EMPTY; MAX_CHILDREN],
            count: 0,
        }
    }

    pub fn insert(&mut self, name: &str, id: InodeId) -> bool {
        if self.count as usize >= MAX_CHILDREN {
            return false;
        }
        let h = name_hash(name);

        if self.contains_hash_and_id(h, id) {
            return false;
        }

        let start = (h as usize) % MAX_CHILDREN;
        for i in 0..MAX_CHILDREN {
            let idx = (start + i) % MAX_CHILDREN;
            match self.slots[idx].state {
                SlotState::Empty | SlotState::Tombstone => {
                    self.slots[idx] = ChildSlot {
                        hash: h,
                        id,
                        state: SlotState::Occupied,
                    };
                    self.count += 1;
                    return true;
                }
                SlotState::Occupied => continue,
            }
        }
        false
    }

    fn contains_hash_and_id(&self, h: u32, id: InodeId) -> bool {
        let start = (h as usize) % MAX_CHILDREN;
        for i in 0..MAX_CHILDREN {
            let idx = (start + i) % MAX_CHILDREN;
            match self.slots[idx].state {
                SlotState::Empty => return false,
                SlotState::Tombstone => continue,
                SlotState::Occupied => {
                    if self.slots[idx].hash == h && self.slots[idx].id == id {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn find_by_hash(&self, h: u32) -> ChildHashIter<'_> {
        ChildHashIter {
            children: self,
            hash: h,
            pos: 0,
            start: (h as usize) % MAX_CHILDREN,
        }
    }

    pub fn get_first(&self, h: u32) -> Option<InodeId> {
        let start = (h as usize) % MAX_CHILDREN;
        for i in 0..MAX_CHILDREN {
            let idx = (start + i) % MAX_CHILDREN;
            match self.slots[idx].state {
                SlotState::Empty => return None,
                SlotState::Tombstone => continue,
                SlotState::Occupied => {
                    if self.slots[idx].hash == h {
                        return Some(self.slots[idx].id);
                    }
                }
            }
        }
        None
    }

    pub fn remove(&mut self, h: u32, id: InodeId) -> bool {
        let start = (h as usize) % MAX_CHILDREN;
        for i in 0..MAX_CHILDREN {
            let idx = (start + i) % MAX_CHILDREN;
            match self.slots[idx].state {
                SlotState::Empty => return false,
                SlotState::Tombstone => continue,
                SlotState::Occupied => {
                    if self.slots[idx].hash == h && self.slots[idx].id == id {
                        self.slots[idx].state = SlotState::Tombstone;
                        if self.count > 0 {
                            self.count -= 1;
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn remove_by_hash(&mut self, h: u32) -> Option<InodeId> {
        let start = (h as usize) % MAX_CHILDREN;
        for i in 0..MAX_CHILDREN {
            let idx = (start + i) % MAX_CHILDREN;
            match self.slots[idx].state {
                SlotState::Empty => return None,
                SlotState::Tombstone => continue,
                SlotState::Occupied => {
                    if self.slots[idx].hash == h {
                        let id = self.slots[idx].id;
                        self.slots[idx].state = SlotState::Tombstone;
                        if self.count > 0 {
                            self.count -= 1;
                        }
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

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn len(&self) -> usize {
        self.count as usize
    }
    pub fn is_full(&self) -> bool {
        self.count as usize >= MAX_CHILDREN
    }

    pub fn iter(&self) -> ChildIter<'_> {
        ChildIter {
            children: self,
            pos: 0,
        }
    }
}

pub struct ChildIter<'a> {
    children: &'a Children,
    pos: usize,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = (u32, InodeId);

    fn next(&mut self) -> Option<Self::Item> {
        while self.pos < MAX_CHILDREN {
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
    hash: u32,
    pos: usize,
    start: usize,
}

impl<'a> Iterator for ChildHashIter<'a> {
    type Item = InodeId;

    fn next(&mut self) -> Option<InodeId> {
        while self.pos < MAX_CHILDREN {
            let idx = (self.start + self.pos) % MAX_CHILDREN;
            self.pos += 1;
            match self.children.slots[idx].state {
                SlotState::Empty => return None,
                SlotState::Tombstone => continue,
                SlotState::Occupied => {
                    if self.children.slots[idx].hash == self.hash {
                        return Some(self.children.slots[idx].id);
                    }
                }
            }
        }
        None
    }
}
