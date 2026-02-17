use crate::miku_extfs::{MikuFS, FsError};
use crate::miku_extfs::structs::*;

impl MikuFS {
    pub fn read_inode(&mut self, inode_num: u32) -> Result<Inode, FsError> {
        self.reader.read_inode(inode_num, &self.superblock, &self.groups)
    }

    pub fn get_file_block(
        &mut self,
        inode: &Inode,
        logical_block: u32,
    ) -> Result<u32, FsError> {
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
            if l1 == 0 { return Ok(0); }
            let l2 = self.read_indirect_entry(l1, idx2)?;
            if l2 == 0 { return Ok(0); }
            return self.read_indirect_entry(l2, idx3);
        }

        Err(FsError::FileTooLarge)
    }

    pub fn ensure_block(
        &mut self,
        inode: &mut Inode,
        inode_num: u32,
        logical_block: u32,
    ) -> Result<u32, FsError> {
        let group = ((inode_num - 1) / self.inodes_per_group) as usize;
        let ptrs_per_block = self.block_size / 4;

        if logical_block < 12 {
            let existing = inode.block(logical_block as usize);
            if existing != 0 {
                return Ok(existing);
            }
            let new_block = self.alloc_block(group)?;
            self.zero_block(new_block)?;
            inode.set_block(logical_block as usize, new_block);
            let blks = inode.blocks() + (self.block_size / 512);
            inode.set_blocks(blks);
            return Ok(new_block);
        }

        let adjusted = logical_block - 12;

        if adjusted < ptrs_per_block {
            let mut indirect_block = inode.block(12);
            if indirect_block == 0 {
                indirect_block = self.alloc_block(group)?;
                self.zero_block(indirect_block)?;
                inode.set_block(12, indirect_block);
                let blks = inode.blocks() + (self.block_size / 512);
                inode.set_blocks(blks);
            }
            return self.ensure_indirect_entry(indirect_block, adjusted, group, inode);
        }

        Err(FsError::FileTooLarge)
    }

    pub fn ensure_indirect_entry(
        &mut self,
        indirect_block: u32,
        index: u32,
        group: usize,
        inode: &mut Inode,
    ) -> Result<u32, FsError> {
        let existing = self.read_indirect_entry(indirect_block, index)?;
        if existing != 0 {
            return Ok(existing);
        }

        let new_block = self.alloc_block(group)?;
        self.zero_block(new_block)?;
        self.write_indirect_entry(indirect_block, index, new_block)?;

        let blks = inode.blocks() + (self.block_size / 512);
        inode.set_blocks(blks);

        Ok(new_block)
    }

    pub fn write_indirect_entry(
        &mut self,
        block_num: u32,
        index: u32,
        value: u32,
    ) -> Result<(), FsError> {
        let ptrs_per_block = self.block_size / 4;
        if index >= ptrs_per_block {
            return Err(FsError::InvalidBlock);
        }

        let byte_offset = index as usize * 4;
        let sector_in_block = byte_offset / 512;
        let offset_in_sector = byte_offset % 512;
        let lba = self.block_to_lba(block_num) + sector_in_block as u32;

        let mut sector = [0u8; 512];
        self.reader.read_sector(lba, &mut sector)?;

        sector[offset_in_sector..offset_in_sector + 4]
            .copy_from_slice(&value.to_le_bytes());

        self.reader.write_sector(lba, &sector)?;
        Ok(())
    }

    pub fn read_indirect_entry(
        &mut self,
        block_num: u32,
        index: u32,
    ) -> Result<u32, FsError> {
        let ptrs_per_block = self.block_size / 4;
        if index >= ptrs_per_block {
            return Err(FsError::InvalidBlock);
        }

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
    ) -> Result<usize, FsError> {
        if !inode.is_regular() && !inode.is_symlink() {
            return Err(FsError::NotRegularFile);
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
                self.read_block_range(
                    phys_block,
                    block_offset,
                    &mut buf[done..done + chunk],
                )?;
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
    ) -> Result<(), FsError> {
        let bs = self.block_size as usize;
        let mut block_buf = [0u8; 4096];
        let read_size = bs.min(4096);

        self.read_block_into(block_num, &mut block_buf[..read_size])?;

        let end = (offset + buf.len()).min(read_size);
        let copy_len = end - offset;
        buf[..copy_len].copy_from_slice(&block_buf[offset..offset + copy_len]);

        Ok(())
    }
}
