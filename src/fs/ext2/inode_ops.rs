use super::structs::*;
use super::{Ext2Error, Ext2Fs};

impl Ext2Fs {
    pub fn read_inode(&mut self, inode_num: u32) -> Result<Inode, Ext2Error> {
        self.reader
            .read_inode(inode_num, &self.superblock, &self.groups)
    }

    pub fn get_file_block(&mut self, inode: &Inode, logical_block: u32) -> Result<u32, Ext2Error> {
        if inode.uses_extents() {
            return self.get_file_block_extent(inode, logical_block);
        }

        let ptrs_per_block = self.block_size / 4;

        if logical_block < 12 {
            return Ok(inode.block(logical_block as usize));
        }

        let adjusted = logical_block - 12;

        if adjusted < ptrs_per_block {
            let indirect_block = inode.block(12);
            if indirect_block == 0 {
                return Ok(0);
            }
            return self.read_indirect_entry(indirect_block, adjusted);
        }

        let adjusted = adjusted - ptrs_per_block;

        if adjusted < ptrs_per_block * ptrs_per_block {
            let dindirect_block = inode.block(13);
            if dindirect_block == 0 {
                return Ok(0);
            }
            let idx1 = adjusted / ptrs_per_block;
            let idx2 = adjusted % ptrs_per_block;
            let indirect = self.read_indirect_entry(dindirect_block, idx1)?;
            if indirect == 0 {
                return Ok(0);
            }
            return self.read_indirect_entry(indirect, idx2);
        }

        let adjusted = adjusted - ptrs_per_block * ptrs_per_block;

        if adjusted < ptrs_per_block * ptrs_per_block * ptrs_per_block {
            let tindirect_block = inode.block(14);
            if tindirect_block == 0 {
                return Ok(0);
            }
            let idx1 = adjusted / (ptrs_per_block * ptrs_per_block);
            let rem = adjusted % (ptrs_per_block * ptrs_per_block);
            let idx2 = rem / ptrs_per_block;
            let idx3 = rem % ptrs_per_block;
            let l1 = self.read_indirect_entry(tindirect_block, idx1)?;
            if l1 == 0 {
                return Ok(0);
            }
            let l2 = self.read_indirect_entry(l1, idx2)?;
            if l2 == 0 {
                return Ok(0);
            }
            return self.read_indirect_entry(l2, idx3);
        }

        Err(Ext2Error::FileTooLarge)
    }

    fn get_file_block_extent(
        &mut self,
        inode: &Inode,
        logical_block: u32,
    ) -> Result<u32, Ext2Error> {
        let header = inode.extent_header();
        if !header.valid() {
            return Err(Ext2Error::CorruptedFs);
        }

        if header.depth == 0 {
            for i in 0..header.entries as usize {
                let ext = inode.extent_at(i);
                if logical_block >= ext.block && logical_block < ext.block + ext.actual_len() {
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
    ) -> Result<u32, Ext2Error> {
        let mut buf = [0u8; 4096];
        let bs = self.block_size as usize;
        self.read_block_into_buf(block_num, &mut buf[..bs])?;

        let magic = u16::from_le_bytes([buf[0], buf[1]]);
        let entries = u16::from_le_bytes([buf[2], buf[3]]);

        if magic != EXT4_EXT_MAGIC {
            return Err(Ext2Error::CorruptedFs);
        }

        if depth == 0 {
            for i in 0..entries as usize {
                let base = 12 + i * 12;
                let ee_block =
                    u32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]);
                let ee_len = u16::from_le_bytes([buf[base + 4], buf[base + 5]]);
                let ee_start_hi = u16::from_le_bytes([buf[base + 6], buf[base + 7]]);
                let ee_start_lo = u32::from_le_bytes([
                    buf[base + 8],
                    buf[base + 9],
                    buf[base + 10],
                    buf[base + 11],
                ]);

                let actual_len = if ee_len > 32768 {
                    ee_len - 32768
                } else {
                    ee_len
                } as u32;
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
            let ei_block =
                u32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]);
            let ei_leaf_lo =
                u32::from_le_bytes([buf[base + 4], buf[base + 5], buf[base + 6], buf[base + 7]]);
            let ei_leaf_hi = u16::from_le_bytes([buf[base + 8], buf[base + 9]]);

            if logical_block >= ei_block {
                target_block = (ei_leaf_lo as u64) | ((ei_leaf_hi as u64) << 32);
            }
        }

        if target_block == 0 {
            return Ok(0);
        }

        self.search_extent_tree(target_block as u32, logical_block, depth - 1)
    }

    fn read_block_into_buf(&mut self, block_num: u32, buf: &mut [u8]) -> Result<(), Ext2Error> {
        let spb = self.sectors_per_block();
        let base_lba = self.block_to_lba(block_num);

        for s in 0..spb {
            let offset = (s * 512) as usize;
            if offset + 512 > buf.len() {
                break;
            }
            let mut sector = [0u8; 512];
            self.reader.read_sector(base_lba + s, &mut sector)?;
            buf[offset..offset + 512].copy_from_slice(&sector);
        }

        Ok(())
    }

    fn read_indirect_entry(&mut self, block_num: u32, index: u32) -> Result<u32, Ext2Error> {
        let byte_offset = index as usize * 4;
        let sector_in_block = byte_offset / 512;
        let offset_in_sector = byte_offset % 512;
        let lba = self.block_to_lba(block_num) + sector_in_block as u32;

        let mut sector = [0u8; 512];
        self.reader.read_sector(lba, &mut sector)?;

        let value = u32::from_le_bytes([
            sector[offset_in_sector],
            sector[offset_in_sector + 1],
            sector[offset_in_sector + 2],
            sector[offset_in_sector + 3],
        ]);

        Ok(value)
    }

    pub fn read_file(
        &mut self,
        inode: &Inode,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<usize, Ext2Error> {
        if !inode.is_regular() && !inode.is_symlink() {
            return Err(Ext2Error::NotRegularFile);
        }

        if inode.is_fast_symlink() {
            let target = inode.fast_symlink_target();
            let off = offset as usize;
            if off >= target.len() {
                return Ok(0);
            }
            let avail = target.len() - off;
            let to_copy = buf.len().min(avail);
            buf[..to_copy].copy_from_slice(&target[off..off + to_copy]);
            return Ok(to_copy);
        }

        let file_size = inode.size();
        if offset >= file_size {
            return Ok(0);
        }

        let avail = (file_size - offset) as usize;
        let to_read = buf.len().min(avail);
        let mut done = 0usize;

        while done < to_read {
            let current_offset = offset as usize + done;
            let logical_block = (current_offset / self.block_size as usize) as u32;
            let block_offset = current_offset % self.block_size as usize;
            let chunk = (self.block_size as usize - block_offset).min(to_read - done);

            let phys_block = self.get_file_block(inode, logical_block)?;

            if phys_block == 0 {
                buf[done..done + chunk].fill(0);
            } else {
                self.read_block_range(phys_block, block_offset, &mut buf[done..done + chunk])?;
            }

            done += chunk;
        }

        Ok(done)
    }

    fn read_block_range(
        &mut self,
        block_num: u32,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<(), Ext2Error> {
        let bs = self.block_size as usize;
        let mut block_buf = [0u8; 4096];
        let read_size = bs.min(4096);

        let spb = self.sectors_per_block();
        let base_lba = self.block_to_lba(block_num);

        for s in 0..spb {
            let off = (s * 512) as usize;
            if off + 512 > read_size {
                break;
            }
            let mut sector = [0u8; 512];
            self.reader.read_sector(base_lba + s, &mut sector)?;
            block_buf[off..off + 512].copy_from_slice(&sector);
        }

        let end = (offset + buf.len()).min(read_size);
        let copy_len = end - offset;
        buf[..copy_len].copy_from_slice(&block_buf[offset..offset + copy_len]);

        Ok(())
    }
}
