use crate::miku_extfs::{FsError, MikuFS};

impl MikuFS {
    pub fn alloc_block(&mut self, preferred_group: usize) -> Result<u32, FsError> {
        let gc = self.group_count as usize;

        for offset in 0..gc {
            let group = (preferred_group + offset) % gc;

            if self.groups[group].free_blocks() == 0 {
                continue;
            }

            let bitmap_block = self.groups[group].block_bitmap();
            let blocks_in_group = self.blocks_per_group;
            let bytes_to_scan = ((blocks_in_group + 7) / 8) as usize;
            let sectors_to_read = (bytes_to_scan + 511) / 512;
            let base_lba = self.block_to_lba(bitmap_block);

            for s in 0..sectors_to_read as u32 {
                let mut sector = [0u8; 512];
                self.reader.read_sector(base_lba + s, &mut sector)?;
                let sector_base_bit = s as u32 * 512 * 8;

                for byte_idx in 0..512usize {
                    let b = sector[byte_idx];
                    if b == 0xFF {
                        continue;
                    }

                    for bit in 0..8u32 {
                        if b & (1 << bit) == 0 {
                            let bit_index = sector_base_bit + byte_idx as u32 * 8 + bit;
                            if bit_index >= blocks_in_group {
                                break;
                            }

                            sector[byte_idx] |= 1 << bit;
                            self.reader.write_sector(base_lba + s, &sector)?;
                            self.update_group_free_blocks(group, -1)?;
                            self.update_superblock_free_blocks(-1)?;

                            let absolute_block = group as u32 * self.blocks_per_group
                                + bit_index
                                + self.superblock.first_data_block();
                            return Ok(absolute_block);
                        }
                    }
                }
            }
        }

        Err(FsError::NoSpace)
    }

    pub fn free_block(&mut self, block_num: u32) -> Result<(), FsError> {
        let first = self.superblock.first_data_block();
        if block_num < first {
            return Err(FsError::InvalidBlock);
        }

        let adjusted = block_num - first;
        let group = (adjusted / self.blocks_per_group) as usize;
        let bit = adjusted % self.blocks_per_group;

        if group >= self.group_count as usize || group >= 32 {
            return Err(FsError::InvalidBlock);
        }

        let bitmap_block = self.groups[group].block_bitmap();
        self.set_bitmap_bit(bitmap_block, bit, false)?;
        self.update_group_free_blocks(group, 1)?;
        self.update_superblock_free_blocks(1)?;

        Ok(())
    }

    pub fn alloc_inode(&mut self, preferred_group: usize) -> Result<u32, FsError> {
        let gc = self.group_count as usize;

        for offset in 0..gc {
            let group = (preferred_group + offset) % gc;

            if self.groups[group].free_inodes() == 0 {
                continue;
            }

            let bitmap_block = self.groups[group].inode_bitmap();
            let inodes_in_group = self.inodes_per_group;
            let bytes_to_scan = ((inodes_in_group + 7) / 8) as usize;
            let sectors_to_read = (bytes_to_scan + 511) / 512;
            let base_lba = self.block_to_lba(bitmap_block);

            for s in 0..sectors_to_read as u32 {
                let mut sector = [0u8; 512];
                self.reader.read_sector(base_lba + s, &mut sector)?;
                let sector_base_bit = s as u32 * 512 * 8;

                for byte_idx in 0..512usize {
                    let b = sector[byte_idx];
                    if b == 0xFF {
                        continue;
                    }

                    for bit in 0..8u32 {
                        if b & (1 << bit) == 0 {
                            let bit_index = sector_base_bit + byte_idx as u32 * 8 + bit;
                            if bit_index >= inodes_in_group {
                                break;
                            }

                            sector[byte_idx] |= 1 << bit;
                            self.reader.write_sector(base_lba + s, &sector)?;
                            self.update_group_free_inodes(group, -1)?;
                            self.update_superblock_free_inodes(-1)?;

                            let inode_num = group as u32 * self.inodes_per_group + bit_index + 1;
                            return Ok(inode_num);
                        }
                    }
                }
            }
        }

        Err(FsError::NoSpace)
    }

    pub fn free_inode(&mut self, inode_num: u32) -> Result<(), FsError> {
        if inode_num == 0 {
            return Err(FsError::InvalidInode);
        }

        let idx = inode_num - 1;
        let group = (idx / self.inodes_per_group) as usize;
        let bit = idx % self.inodes_per_group;

        if group >= self.group_count as usize || group >= 32 {
            return Err(FsError::InvalidInode);
        }

        let bitmap_block = self.groups[group].inode_bitmap();
        self.set_bitmap_bit(bitmap_block, bit, false)?;
        self.update_group_free_inodes(group, 1)?;
        self.update_superblock_free_inodes(1)?;

        Ok(())
    }

    pub fn set_bitmap_bit(
        &mut self,
        bitmap_block: u32,
        bit_index: u32,
        value: bool,
    ) -> Result<(), FsError> {
        let byte_index = bit_index / 8;
        let bit_offset = bit_index % 8;
        let sector_in_block = byte_index / 512;
        let offset_in_sector = (byte_index % 512) as usize;
        let lba = self.block_to_lba(bitmap_block) + sector_in_block;

        let mut sector = [0u8; 512];
        self.reader.read_sector(lba, &mut sector)?;

        if value {
            sector[offset_in_sector] |= 1 << bit_offset;
        } else {
            sector[offset_in_sector] &= !(1 << bit_offset);
        }

        self.reader.write_sector(lba, &sector)?;
        Ok(())
    }

    pub fn update_group_free_blocks(&mut self, group: usize, delta: i16) -> Result<(), FsError> {
        if group >= 32 {
            return Err(FsError::InvalidBlock);
        }
        let current = self.groups[group].free_blocks();
        let new_val = (current as i32 + delta as i32).max(0) as u16;
        self.groups[group].write_u16(12, new_val);
        self.flush_group_desc(group)
    }

    pub fn update_group_free_inodes(&mut self, group: usize, delta: i16) -> Result<(), FsError> {
        if group >= 32 {
            return Err(FsError::InvalidInode);
        }
        let current = self.groups[group].free_inodes();
        let new_val = (current as i32 + delta as i32).max(0) as u16;
        self.groups[group].write_u16(14, new_val);
        self.flush_group_desc(group)
    }

    pub fn update_superblock_free_blocks(&mut self, delta: i32) -> Result<(), FsError> {
        let current = self.superblock.free_blocks_count();
        let new_val = (current as i64 + delta as i64).max(0) as u32;
        self.superblock.write_u32(12, new_val);
        self.flush_superblock()
    }

    pub fn update_superblock_free_inodes(&mut self, delta: i32) -> Result<(), FsError> {
        let current = self.superblock.free_inodes_count();
        let new_val = (current as i64 + delta as i64).max(0) as u32;
        self.superblock.write_u32(16, new_val);
        self.flush_superblock()
    }
}
