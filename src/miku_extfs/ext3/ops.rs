use crate::miku_extfs::{MikuFS, FsError};
use crate::miku_extfs::structs::*;
use super::journal::*;

impl MikuFS {
    pub fn ext3_create_file(
        &mut self,
        parent_ino: u32,
        name: &str,
        mode: u16,
    ) -> Result<u32, FsError> {
        if !self.journal_active {
            return self.ext2_create_file(parent_ino, name, mode);
        }

        self.ext3_begin_txn()?;
        let result = self.ext2_create_file(parent_ino, name, mode);

        match result {
            Ok(new_ino) => {
                let group = ((new_ino - 1) / self.inodes_per_group) as usize;
                self.journal_inode_blocks(parent_ino)?;
                self.journal_inode_metadata(new_ino)?;
                self.journal_group_metadata(group)?;
                self.ext3_commit_txn()?;
                Ok(new_ino)
            }
            Err(e) => {
                self.ext3_abort_txn();
                Err(e)
            }
        }
    }

    pub fn ext3_create_dir(
        &mut self,
        parent_ino: u32,
        name: &str,
        mode: u16,
    ) -> Result<u32, FsError> {
        if !self.journal_active {
            return self.ext2_create_dir(parent_ino, name, mode);
        }

        self.ext3_begin_txn()?;
        let result = self.ext2_create_dir(parent_ino, name, mode);

        match result {
            Ok(new_ino) => {
                let group = ((new_ino - 1) / self.inodes_per_group) as usize;
                self.journal_inode_blocks(parent_ino)?;
                self.journal_inode_blocks(new_ino)?;
                self.journal_inode_metadata(new_ino)?;
                self.journal_inode_metadata(parent_ino)?;
                self.journal_group_metadata(group)?;
                self.ext3_commit_txn()?;
                Ok(new_ino)
            }
            Err(e) => {
                self.ext3_abort_txn();
                Err(e)
            }
        }
    }

    pub fn ext3_write_file(
        &mut self,
        inode_num: u32,
        data: &[u8],
        offset: u64,
    ) -> Result<usize, FsError> {
        if !self.journal_active {
            return self.ext2_write_file(inode_num, data, offset);
        }

        self.ext3_begin_txn()?;
        let result = self.ext2_write_file(inode_num, data, offset);

        match result {
            Ok(n) => {
                self.journal_inode_metadata(inode_num)?;
                let group = ((inode_num - 1) / self.inodes_per_group) as usize;
                self.journal_group_metadata(group)?;
                self.ext3_commit_txn()?;
                Ok(n)
            }
            Err(e) => {
                self.ext3_abort_txn();
                Err(e)
            }
        }
    }

    pub fn ext3_delete_file(
        &mut self,
        parent_ino: u32,
        name: &str,
    ) -> Result<(), FsError> {
        if !self.journal_active {
            return self.ext2_delete_file(parent_ino, name);
        }

        let target_ino = match self.ext2_lookup_in_dir(parent_ino, name)? {
            Some(ino) => ino,
            None => return Err(FsError::NotFound),
        };

        self.ext3_begin_txn()?;
        let result = self.ext2_delete_file(parent_ino, name);

        match result {
            Ok(()) => {
                self.journal_inode_blocks(parent_ino)?;
                self.journal_inode_metadata(target_ino)?;
                let group = ((target_ino - 1) / self.inodes_per_group) as usize;
                self.journal_group_metadata(group)?;
                self.ext3_commit_txn()?;
                Ok(())
            }
            Err(e) => {
                self.ext3_abort_txn();
                Err(e)
            }
        }
    }

    pub fn ext3_delete_dir(
        &mut self,
        parent_ino: u32,
        name: &str,
    ) -> Result<(), FsError> {
        if !self.journal_active {
            return self.ext2_delete_dir(parent_ino, name);
        }

        let target_ino = match self.ext2_lookup_in_dir(parent_ino, name)? {
            Some(ino) => ino,
            None => return Err(FsError::NotFound),
        };

        self.ext3_begin_txn()?;
        let result = self.ext2_delete_dir(parent_ino, name);

        match result {
            Ok(()) => {
                self.journal_inode_blocks(parent_ino)?;
                self.journal_inode_metadata(target_ino)?;
                self.journal_inode_metadata(parent_ino)?;
                let group = ((target_ino - 1) / self.inodes_per_group) as usize;
                self.journal_group_metadata(group)?;
                self.ext3_commit_txn()?;
                Ok(())
            }
            Err(e) => {
                self.ext3_abort_txn();
                Err(e)
            }
        }
    }

    pub fn ext3_create_journal(&mut self, num_blocks: u32) -> Result<(), FsError> {
        if self.has_journal() {
            return Err(FsError::AlreadyExists);
        }

        if num_blocks < 16 {
            return Err(FsError::NoSpace);
        }

        let free = self.superblock.free_blocks_count();
        if num_blocks + 2 > free {
            return Err(FsError::NoSpace);
        }

        crate::serial_println!("[ext3] creating journal: {} blocks", num_blocks);

        let now = self.get_timestamp();
        let mut j_inode = Inode::zeroed();
        j_inode.data = [0; 256];
        j_inode.set_mode(S_IFREG | 0o600);
        j_inode.set_uid(0);
        j_inode.set_gid(0);
        j_inode.set_atime(now);
        j_inode.set_ctime(now);
        j_inode.set_mtime(now);
        j_inode.set_links_count(1);

        let direct_count = num_blocks.min(12);
        for i in 0..direct_count {
            let blk = self.alloc_block(0)?;
            self.zero_block(blk)?;
            j_inode.set_block(i as usize, blk);
        }

        if num_blocks > 12 {
            let ptrs_per_block = self.block_size / 4;
            let indirect_blk = self.alloc_block(0)?;
            self.zero_block(indirect_blk)?;
            j_inode.set_block(12, indirect_blk);

            let remaining = (num_blocks - 12).min(ptrs_per_block);
            for i in 0..remaining {
                let blk = self.alloc_block(0)?;
                self.zero_block(blk)?;
                self.write_indirect_entry(indirect_blk, i, blk)?;
            }

            let total_disk_blocks = num_blocks + 1;
            j_inode.set_blocks(total_disk_blocks * (self.block_size / 512));
        } else {
            j_inode.set_blocks(num_blocks * (self.block_size / 512));
        }

        let journal_byte_size = num_blocks * self.block_size;
        j_inode.set_size(journal_byte_size);
        if self.inode_size() >= 128 {
            j_inode.write_u32(108, 0);
        }

        self.write_inode(EXT2_JOURNAL_INO, &j_inode)?;

        self.write_journal_superblock(num_blocks)?;

        let compat = self.superblock.feature_compat();
        self.superblock.write_u32(92, compat | FEATURE_COMPAT_HAS_JOURNAL);
        self.superblock.write_u32(224, EXT2_JOURNAL_INO);

        let fs_uuid = self.superblock.uuid();
        let mut uuid_copy = [0u8; 16];
        uuid_copy.copy_from_slice(fs_uuid);
        self.superblock.data[208..224].copy_from_slice(&uuid_copy);

        self.flush_superblock()?;

        self.init_journal()?;

        Ok(())
    }

    fn write_journal_superblock(&mut self, num_blocks: u32) -> Result<(), FsError> {
        let j_inode = self.read_inode(EXT2_JOURNAL_INO)?;
        let first_journal_block = j_inode.block(0);
        if first_journal_block == 0 {
            return Err(FsError::CorruptedFs);
        }

        let bs = self.block_size as usize;
        let mut jsb_data = [0u8; 4096];

        jsb_data[0..4].copy_from_slice(&JBD_MAGIC.to_be_bytes());
        jsb_data[4..8].copy_from_slice(&JBD_SUPERBLOCK_V2.to_be_bytes());
        jsb_data[8..12].copy_from_slice(&0u32.to_be_bytes());
        jsb_data[12..16].copy_from_slice(&self.block_size.to_be_bytes());
        jsb_data[16..20].copy_from_slice(&num_blocks.to_be_bytes());
        jsb_data[20..24].copy_from_slice(&1u32.to_be_bytes());
        jsb_data[24..28].copy_from_slice(&1u32.to_be_bytes());
        jsb_data[28..32].copy_from_slice(&0u32.to_be_bytes());
        jsb_data[32..36].copy_from_slice(&0u32.to_be_bytes());

        let fs_uuid = self.superblock.uuid();
        jsb_data[48..64].copy_from_slice(fs_uuid);
        jsb_data[64..68].copy_from_slice(&1u32.to_be_bytes());

        self.write_block_data(first_journal_block, &jsb_data[..bs])?;

        Ok(())
    }
}
