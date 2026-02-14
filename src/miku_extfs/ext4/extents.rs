use crate::miku_extfs::{MikuFS, FsError};
use crate::miku_extfs::structs::*;

impl MikuFS {
    pub fn get_file_block_extent(
        &mut self,
        inode: &Inode,
        logical_block: u32,
    ) -> Result<u32, FsError> {
        let header = inode.extent_header();
        if !header.valid() {
            return Err(FsError::CorruptedFs);
        }

        if header.depth == 0 {
            for i in 0..header.entries as usize {
                let ext = inode.extent_at(i);
                if logical_block >= ext.block
                    && logical_block < ext.block + ext.actual_len()
                {
                    let offset = logical_block - ext.block;
                    return Ok((ext.start() + offset as u64) as u32);
                }
            }
            return Ok(0);
        }

        let mut target_block = 0u64;
        for i in 0..header.entries as usize {
            let idx = inode.extent_idx_at(i);
            if logical_block >= idx.block {
                target_block = idx.leaf();
            }
        }

        if target_block == 0 {
            return Ok(0);
        }

        self.search_extent_tree(target_block as u32, logical_block, header.depth - 1)
    }

    fn search_extent_tree(
        &mut self,
        block_num: u32,
        logical_block: u32,
        depth: u16,
    ) -> Result<u32, FsError> {
        let mut buf = [0u8; 4096];
        let bs = self.block_size as usize;
        self.read_block_into(block_num, &mut buf[..bs])?;

        let magic = u16::from_le_bytes([buf[0], buf[1]]);
        let entries = u16::from_le_bytes([buf[2], buf[3]]);

        if magic != EXT4_EXT_MAGIC {
            return Err(FsError::CorruptedFs);
        }

        if depth == 0 {
            for i in 0..entries as usize {
                let base = 12 + i * 12;
                let ee_block = u32::from_le_bytes([buf[base], buf[base+1], buf[base+2], buf[base+3]]);
                let ee_len = u16::from_le_bytes([buf[base+4], buf[base+5]]);
                let ee_start_hi = u16::from_le_bytes([buf[base+6], buf[base+7]]);
                let ee_start_lo = u32::from_le_bytes([buf[base+8], buf[base+9], buf[base+10], buf[base+11]]);

                let actual_len = if ee_len > 32768 { ee_len - 32768 } else { ee_len } as u32;
                if logical_block >= ee_block && logical_block < ee_block + actual_len {
                    let offset = logical_block - ee_block;
                    let start = (ee_start_lo as u64) | ((ee_start_hi as u64) << 32);
                    return Ok((start + offset as u64) as u32);
                }
            }
            return Ok(0);
        }

        let mut target_block = 0u64;
        for i in 0..entries as usize {
            let base = 12 + i * 12;
            let ei_block = u32::from_le_bytes([buf[base], buf[base+1], buf[base+2], buf[base+3]]);
            let ei_leaf_lo = u32::from_le_bytes([buf[base+4], buf[base+5], buf[base+6], buf[base+7]]);
            let ei_leaf_hi = u16::from_le_bytes([buf[base+8], buf[base+9]]);

            if logical_block >= ei_block {
                target_block = (ei_leaf_lo as u64) | ((ei_leaf_hi as u64) << 32);
            }
        }

        if target_block == 0 {
            return Ok(0);
        }

        self.search_extent_tree(target_block as u32, logical_block, depth - 1)
    }
}
