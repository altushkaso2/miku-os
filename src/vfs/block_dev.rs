use crate::vfs::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockDevType {
    RamDisk = 0,
    AtaDisk = 1,
}

#[derive(Clone, Copy)]
pub struct BlockDevice {
    pub id: BlockDevId,
    pub dev_type: BlockDevType,
    pub block_size: u32,
    pub total_blocks: u64,
    pub name: NameBuf,
    pub read_only: bool,
    pub active: bool,
}

impl BlockDevice {
    pub const fn empty() -> Self {
        Self {
            id: INVALID_U8,
            dev_type: BlockDevType::RamDisk,
            block_size: BLOCK_SIZE as u32,
            total_blocks: 0,
            name: NameBuf::empty(),
            read_only: false,
            active: false,
        }
    }

    pub fn size_bytes(&self) -> u64 {
        self.total_blocks * self.block_size as u64
    }
}

pub struct BlockDevManager {
    pub devices: [BlockDevice; MAX_BLOCK_DEVICES],
}

impl BlockDevManager {
    pub const fn new() -> Self {
        Self {
            devices: [BlockDevice::empty(); MAX_BLOCK_DEVICES],
        }
    }

    pub fn register(
        &mut self,
        dev_type: BlockDevType,
        block_size: u32,
        total_blocks: u64,
        name: &str,
    ) -> VfsResult<BlockDevId> {
        for (i, dev) in self.devices.iter_mut().enumerate() {
            if !dev.active {
                dev.id = i as BlockDevId;
                dev.dev_type = dev_type;
                dev.block_size = block_size;
                dev.total_blocks = total_blocks;
                dev.name = NameBuf::from_str(name);
                dev.active = true;
                return Ok(i as BlockDevId);
            }
        }
        Err(VfsError::NoSpace)
    }

    pub fn get(&self, id: BlockDevId) -> Option<&BlockDevice> {
        let i = id as usize;
        if i < MAX_BLOCK_DEVICES && self.devices[i].active {
            Some(&self.devices[i])
        } else {
            None
        }
    }

    pub fn unregister(&mut self, id: BlockDevId) -> VfsResult<()> {
        let i = id as usize;
        if i < MAX_BLOCK_DEVICES && self.devices[i].active {
            self.devices[i] = BlockDevice::empty();
            Ok(())
        } else {
            Err(VfsError::NotFound)
        }
    }

    pub fn count(&self) -> usize {
        self.devices.iter().filter(|d| d.active).count()
    }
}
