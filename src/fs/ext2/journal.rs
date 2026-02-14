use super::structs::*;
use super::{Ext2Error, Ext2Fs};

pub const JBD_MAGIC: u32 = 0xC03B3998;

pub const JBD_DESCRIPTOR_BLOCK: u32 = 1;
pub const JBD_COMMIT_BLOCK: u32 = 2;
pub const JBD_SUPERBLOCK_V1: u32 = 3;
pub const JBD_SUPERBLOCK_V2: u32 = 4;
pub const JBD_REVOKE_BLOCK: u32 = 5;

pub const JBD_FLAG_ESCAPE: u32 = 1;
pub const JBD_FLAG_SAME_UUID: u32 = 2;
pub const JBD_FLAG_DELETED: u32 = 4;
pub const JBD_FLAG_LAST_TAG: u32 = 8;

pub const DEFAULT_JOURNAL_BLOCKS: u32 = 256;

#[derive(Clone, Copy)]
pub struct TxnTag {
    pub fs_block: u32,
    pub journal_pos: u32,
}

impl TxnTag {
    pub const fn empty() -> Self {
        Self {
            fs_block: 0,
            journal_pos: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct JournalSuperblock {
    pub data: [u8; 1024],
}

impl JournalSuperblock {
    pub const fn zeroed() -> Self {
        Self { data: [0; 1024] }
    }

    fn read_be32(&self, offset: usize) -> u32 {
        u32::from_be_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }

    pub fn write_be32(&mut self, offset: usize, val: u32) {
        let bytes = val.to_be_bytes();
        self.data[offset..offset + 4].copy_from_slice(&bytes);
    }

    pub fn magic(&self) -> u32 {
        self.read_be32(0)
    }
    pub fn blocktype(&self) -> u32 {
        self.read_be32(4)
    }
    pub fn blocksize(&self) -> u32 {
        self.read_be32(12)
    }
    pub fn maxlen(&self) -> u32 {
        self.read_be32(16)
    }
    pub fn first(&self) -> u32 {
        self.read_be32(20)
    }
    pub fn start_sequence(&self) -> u32 {
        self.read_be32(24)
    }
    pub fn start(&self) -> u32 {
        self.read_be32(28)
    }
    pub fn errno_val(&self) -> i32 {
        self.read_be32(32) as i32
    }

    pub fn uuid(&self) -> &[u8] {
        &self.data[48..64]
    }

    pub fn is_valid(&self) -> bool {
        self.magic() == JBD_MAGIC
    }
    pub fn is_clean(&self) -> bool {
        self.start() == 0
    }
    pub fn is_v2(&self) -> bool {
        self.blocktype() == JBD_SUPERBLOCK_V2
    }

    pub fn version_str(&self) -> &'static str {
        match self.blocktype() {
            JBD_SUPERBLOCK_V1 => "JBD1",
            JBD_SUPERBLOCK_V2 => "JBD2",
            _ => "unknown",
        }
    }
}

#[derive(Clone, Copy)]
pub struct JournalHeader {
    pub magic: u32,
    pub blocktype: u32,
    pub sequence: u32,
}

impl JournalHeader {
    pub fn from_buf(buf: &[u8]) -> Self {
        Self {
            magic: u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
            blocktype: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
            sequence: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == JBD_MAGIC
    }
    pub fn is_descriptor(&self) -> bool {
        self.blocktype == JBD_DESCRIPTOR_BLOCK
    }
    pub fn is_commit(&self) -> bool {
        self.blocktype == JBD_COMMIT_BLOCK
    }

    pub fn type_str(&self) -> &'static str {
        match self.blocktype {
            JBD_DESCRIPTOR_BLOCK => "descriptor",
            JBD_COMMIT_BLOCK => "commit",
            JBD_SUPERBLOCK_V1 => "sb_v1",
            JBD_SUPERBLOCK_V2 => "sb_v2",
            JBD_REVOKE_BLOCK => "revoke",
            _ => "unknown",
        }
    }
}

#[derive(Clone, Copy)]
pub struct JournalBlockTag {
    pub blocknr: u32,
    pub flags: u32,
}

impl JournalBlockTag {
    pub fn from_buf(buf: &[u8], offset: usize) -> Self {
        Self {
            blocknr: u32::from_be_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]),
            flags: u32::from_be_bytes([
                buf[offset + 4],
                buf[offset + 5],
                buf[offset + 6],
                buf[offset + 7],
            ]),
        }
    }

    pub fn is_last(&self) -> bool {
        self.flags & JBD_FLAG_LAST_TAG != 0
    }
    pub fn same_uuid(&self) -> bool {
        self.flags & JBD_FLAG_SAME_UUID != 0
    }
}

#[derive(Clone, Copy)]
pub struct JournalTransaction {
    pub sequence: u32,
    pub start_block: u32,
    pub data_blocks: u32,
    pub committed: bool,
    pub active: bool,
}

impl JournalTransaction {
    pub const fn empty() -> Self {
        Self {
            sequence: 0,
            start_block: 0,
            data_blocks: 0,
            committed: false,
            active: false,
        }
    }
}

pub struct JournalInfo {
    pub valid: bool,
    pub version: u8,
    pub block_size: u32,
    pub total_blocks: u32,
    pub first_block: u32,
    pub start: u32,
    pub sequence: u32,
    pub clean: bool,
    pub errno: i32,
    pub transactions: [JournalTransaction; 32],
    pub transaction_count: usize,
    pub journal_inode: u32,
    pub journal_size: u64,
}

impl JournalInfo {
    pub const fn empty() -> Self {
        Self {
            valid: false,
            version: 0,
            block_size: 0,
            total_blocks: 0,
            first_block: 0,
            start: 0,
            sequence: 0,
            clean: false,
            errno: 0,
            transactions: [JournalTransaction::empty(); 32],
            transaction_count: 0,
            journal_inode: 0,
            journal_size: 0,
        }
    }
}

impl Ext2Fs {
    pub fn has_journal(&self) -> bool {
        self.superblock.has_journal()
    }

    pub fn journal_block_to_disk(&mut self, journal_block: u32) -> Result<u32, Ext2Error> {
        let journal_inode = self.read_inode(EXT2_JOURNAL_INO)?;
        self.get_file_block(&journal_inode, journal_block)
    }

    pub fn read_journal_superblock(&mut self) -> Result<JournalSuperblock, Ext2Error> {
        if !self.has_journal() {
            return Err(Ext2Error::NoJournal);
        }

        let disk_block = self.journal_block_to_disk(0)?;
        if disk_block == 0 {
            return Err(Ext2Error::CorruptedFs);
        }

        let mut jsb = JournalSuperblock::zeroed();
        let bs = self.block_size as usize;
        let mut buf = [0u8; 4096];
        self.read_block_into(disk_block, &mut buf[..bs])?;

        let copy_size = bs.min(1024);
        jsb.data[..copy_size].copy_from_slice(&buf[..copy_size]);

        if !jsb.is_valid() {
            return Err(Ext2Error::CorruptedFs);
        }

        Ok(jsb)
    }

    pub fn read_journal_block_data(
        &mut self,
        journal_block: u32,
        buf: &mut [u8],
    ) -> Result<(), Ext2Error> {
        let disk_block = self.journal_block_to_disk(journal_block)?;
        if disk_block == 0 {
            return Err(Ext2Error::CorruptedFs);
        }
        self.read_block_into(disk_block, buf)
    }

    pub fn init_journal(&mut self) -> Result<(), Ext2Error> {
        if !self.has_journal() {
            self.journal_active = false;
            return Ok(());
        }

        let jsb = self.read_journal_superblock()?;
        self.journal_seq = jsb.start_sequence();
        self.journal_maxlen = jsb.maxlen();
        self.journal_first = jsb.first();
        self.journal_active = true;
        self.txn_active = false;
        self.txn_tag_count = 0;

        if jsb.is_clean() {
            self.journal_pos = jsb.first();
        } else {
            self.journal_pos = jsb.start();
        }

        crate::serial_println!(
            "[ext3] journal init: seq={} pos={} max={} active=true",
            self.journal_seq,
            self.journal_pos,
            self.journal_maxlen
        );

        Ok(())
    }

    fn advance_journal_pos(&self, pos: u32) -> u32 {
        let next = pos + 1;
        if next >= self.journal_maxlen {
            self.journal_first
        } else {
            next
        }
    }

    pub fn ext3_begin_txn(&mut self) -> Result<(), Ext2Error> {
        if !self.journal_active {
            return Ok(());
        }
        if self.txn_active {
            return Ok(());
        }

        self.txn_active = true;
        self.txn_desc_pos = self.journal_pos;
        self.journal_pos = self.advance_journal_pos(self.journal_pos);
        self.txn_tag_count = 0;

        Ok(())
    }

    pub fn ext3_journal_current_block(&mut self, fs_block: u32) -> Result<(), Ext2Error> {
        if !self.journal_active || !self.txn_active {
            return Ok(());
        }

        if self.txn_tag_count >= 16 {
            return Ok(());
        }

        for i in 0..self.txn_tag_count as usize {
            if self.txn_tags[i].fs_block == fs_block {
                return Ok(());
            }
        }

        let bs = self.block_size as usize;
        let mut buf = [0u8; 4096];
        self.read_block_into(fs_block, &mut buf[..bs])?;

        let journal_disk_block = self.journal_block_to_disk(self.journal_pos)?;
        if journal_disk_block == 0 {
            return Err(Ext2Error::CorruptedFs);
        }
        self.write_block_data(journal_disk_block, &buf[..bs])?;

        let idx = self.txn_tag_count as usize;
        self.txn_tags[idx] = TxnTag {
            fs_block,
            journal_pos: self.journal_pos,
        };
        self.txn_tag_count += 1;

        self.journal_pos = self.advance_journal_pos(self.journal_pos);

        Ok(())
    }

    pub fn ext3_commit_txn(&mut self) -> Result<(), Ext2Error> {
        if !self.journal_active || !self.txn_active {
            return Ok(());
        }

        let tag_count = self.txn_tag_count as usize;
        if tag_count == 0 {
            self.txn_active = false;
            return Ok(());
        }

        let bs = self.block_size as usize;

        let mut desc = [0u8; 4096];
        desc[0..4].copy_from_slice(&JBD_MAGIC.to_be_bytes());
        desc[4..8].copy_from_slice(&JBD_DESCRIPTOR_BLOCK.to_be_bytes());
        desc[8..12].copy_from_slice(&self.journal_seq.to_be_bytes());

        let mut offset = 12;
        for i in 0..tag_count {
            let tag_block = self.txn_tags[i].fs_block;
            let mut flags = JBD_FLAG_SAME_UUID;
            if i == tag_count - 1 {
                flags |= JBD_FLAG_LAST_TAG;
            }
            desc[offset..offset + 4].copy_from_slice(&tag_block.to_be_bytes());
            desc[offset + 4..offset + 8].copy_from_slice(&flags.to_be_bytes());
            offset += 8;
        }

        let desc_disk_block = self.journal_block_to_disk(self.txn_desc_pos)?;
        self.write_block_data(desc_disk_block, &desc[..bs])?;

        let mut commit = [0u8; 4096];
        commit[0..4].copy_from_slice(&JBD_MAGIC.to_be_bytes());
        commit[4..8].copy_from_slice(&JBD_COMMIT_BLOCK.to_be_bytes());
        commit[8..12].copy_from_slice(&self.journal_seq.to_be_bytes());

        let commit_disk_block = self.journal_block_to_disk(self.journal_pos)?;
        self.write_block_data(commit_disk_block, &commit[..bs])?;

        self.journal_pos = self.advance_journal_pos(self.journal_pos);

        self.mark_journal_dirty()?;

        self.journal_seq += 1;
        self.txn_active = false;
        self.txn_tag_count = 0;

        crate::serial_println!(
            "[ext3] txn committed: seq={} blocks={}",
            self.journal_seq - 1,
            tag_count
        );

        Ok(())
    }

    pub fn ext3_abort_txn(&mut self) {
        self.txn_active = false;
        self.txn_tag_count = 0;
    }

    fn mark_journal_dirty(&mut self) -> Result<(), Ext2Error> {
        let j_inode = self.read_inode(EXT2_JOURNAL_INO)?;
        let disk_blk = j_inode.block(0);
        if disk_blk == 0 {
            return Err(Ext2Error::CorruptedFs);
        }

        let bs = self.block_size as usize;
        let mut buf = [0u8; 4096];
        self.read_block_into(disk_blk, &mut buf[..bs])?;

        buf[24..28].copy_from_slice(&self.journal_seq.to_be_bytes());
        buf[28..32].copy_from_slice(&self.txn_desc_pos.to_be_bytes());

        self.write_block_data(disk_blk, &buf[..bs])?;
        Ok(())
    }

    fn journal_inode_blocks(&mut self, inode_num: u32) -> Result<(), Ext2Error> {
        let inode = self.read_inode(inode_num)?;
        for i in 0..12 {
            let blk = inode.block(i);
            if blk != 0 {
                self.ext3_journal_current_block(blk)?;
            }
        }
        Ok(())
    }

    fn journal_inode_metadata(&mut self, inode_num: u32) -> Result<(), Ext2Error> {
        if inode_num == 0 {
            return Ok(());
        }
        let idx = inode_num - 1;
        let group = (idx / self.inodes_per_group) as usize;
        if group >= 32 {
            return Ok(());
        }

        let it_block = self.groups[group].inode_table();
        let inode_size = self.inode_size();
        let local_idx = idx % self.inodes_per_group;
        let byte_off = local_idx as u64 * inode_size as u64;
        let block_off = (byte_off / self.block_size as u64) as u32;
        self.ext3_journal_current_block(it_block + block_off)?;

        self.ext3_journal_current_block(self.groups[group].inode_bitmap())?;

        Ok(())
    }

    fn journal_group_metadata(&mut self, group: usize) -> Result<(), Ext2Error> {
        if group >= 32 {
            return Ok(());
        }
        self.ext3_journal_current_block(self.groups[group].block_bitmap())?;
        Ok(())
    }

    pub fn ext3_create_file(
        &mut self,
        parent_ino: u32,
        name: &str,
        mode: u16,
    ) -> Result<u32, Ext2Error> {
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
    ) -> Result<u32, Ext2Error> {
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
    ) -> Result<usize, Ext2Error> {
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

    pub fn ext3_delete_file(&mut self, parent_ino: u32, name: &str) -> Result<(), Ext2Error> {
        if !self.journal_active {
            return self.ext2_delete_file(parent_ino, name);
        }

        let target_ino = match self.ext2_lookup_in_dir(parent_ino, name)? {
            Some(ino) => ino,
            None => return Err(Ext2Error::NotFound),
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

    pub fn ext3_delete_dir(&mut self, parent_ino: u32, name: &str) -> Result<(), Ext2Error> {
        if !self.journal_active {
            return self.ext2_delete_dir(parent_ino, name);
        }

        let target_ino = match self.ext2_lookup_in_dir(parent_ino, name)? {
            Some(ino) => ino,
            None => return Err(Ext2Error::NotFound),
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

    pub fn ext3_create_journal(&mut self, num_blocks: u32) -> Result<(), Ext2Error> {
        if self.has_journal() {
            return Err(Ext2Error::AlreadyExists);
        }

        if num_blocks < 16 {
            return Err(Ext2Error::NoSpace);
        }

        let free = self.superblock.free_blocks_count();
        if num_blocks + 2 > free {
            return Err(Ext2Error::NoSpace);
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
        self.superblock
            .write_u32(92, compat | FEATURE_COMPAT_HAS_JOURNAL);
        self.superblock.write_u32(224, EXT2_JOURNAL_INO);

        let fs_uuid = self.superblock.uuid();
        let mut uuid_copy = [0u8; 16];
        uuid_copy.copy_from_slice(fs_uuid);
        self.superblock.data[208..224].copy_from_slice(&uuid_copy);

        self.flush_superblock()?;

        self.init_journal()?;

        Ok(())
    }

    fn write_journal_superblock(&mut self, num_blocks: u32) -> Result<(), Ext2Error> {
        let j_inode = self.read_inode(EXT2_JOURNAL_INO)?;
        let first_journal_block = j_inode.block(0);
        if first_journal_block == 0 {
            return Err(Ext2Error::CorruptedFs);
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

    pub fn ext3_clean_journal(&mut self) -> Result<(), Ext2Error> {
        if !self.has_journal() {
            return Err(Ext2Error::NoJournal);
        }

        let j_inode = self.read_inode(EXT2_JOURNAL_INO)?;
        let disk_blk = j_inode.block(0);
        if disk_blk == 0 {
            return Err(Ext2Error::CorruptedFs);
        }

        let bs = self.block_size as usize;
        let mut buf = [0u8; 4096];
        self.read_block_into(disk_blk, &mut buf[..bs])?;

        let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != JBD_MAGIC {
            return Err(Ext2Error::CorruptedFs);
        }

        buf[28..32].copy_from_slice(&0u32.to_be_bytes());

        let seq = u32::from_be_bytes([buf[24], buf[25], buf[26], buf[27]]);
        let new_seq = seq.wrapping_add(1);
        buf[24..28].copy_from_slice(&new_seq.to_be_bytes());

        self.write_block_data(disk_blk, &buf[..bs])?;

        self.journal_pos = self.journal_first;
        self.journal_seq = new_seq;

        Ok(())
    }

    pub fn ext3_recover(&mut self) -> Result<u32, Ext2Error> {
        if !self.has_journal() {
            return Err(Ext2Error::NoJournal);
        }

        let jsb = self.read_journal_superblock()?;

        if jsb.is_clean() {
            return Ok(0);
        }

        let maxlen = jsb.maxlen();
        let first = jsb.first();
        let mut block = jsb.start();
        let bs = self.block_size as usize;
        let mut buf = [0u8; 4096];
        let read_size = bs.min(4096);
        let max_scan = maxlen.min(512);
        let mut scanned = 0u32;
        let mut replayed = 0u32;

        let mut tags: [(u32, u32); 64] = [(0, 0); 64];
        let mut tag_count: usize;
        let mut committed: bool;

        while scanned < max_scan {
            if self
                .read_journal_block_data(block, &mut buf[..read_size])
                .is_err()
            {
                break;
            }

            let header = JournalHeader::from_buf(&buf);
            if !header.is_valid() {
                break;
            }

            if !header.is_descriptor() {
                block = self.next_journal_block(block, first, maxlen);
                scanned += 1;
                continue;
            }

            tag_count = 0;
            committed = false;

            let mut offset = 12usize;
            loop {
                if offset + 8 > read_size {
                    break;
                }
                let tag = JournalBlockTag::from_buf(&buf, offset);
                if tag_count < 64 {
                    let data_journal_pos = self.next_journal_block(block, first, maxlen);
                    let mut dpos = data_journal_pos;
                    for _ in 0..tag_count as u32 {
                        dpos = self.next_journal_block(dpos, first, maxlen);
                    }
                    tags[tag_count] = (tag.blocknr, dpos);
                    tag_count += 1;
                }
                offset += 8;
                if !tag.same_uuid() {
                    offset += 16;
                }
                if tag.is_last() {
                    break;
                }
            }

            let mut skip = block;
            for _ in 0..tag_count {
                skip = self.next_journal_block(skip, first, maxlen);
                scanned += 1;
            }

            let commit_pos = self.next_journal_block(skip, first, maxlen);
            scanned += 1;

            if self
                .read_journal_block_data(commit_pos, &mut buf[..read_size])
                .is_ok()
            {
                let ch = JournalHeader::from_buf(&buf);
                if ch.is_valid() && ch.is_commit() && ch.sequence == header.sequence {
                    committed = true;
                }
            }

            if committed {
                for i in 0..tag_count {
                    let (fs_block, j_pos) = tags[i];
                    let mut data = [0u8; 4096];
                    if self
                        .read_journal_block_data(j_pos, &mut data[..read_size])
                        .is_ok()
                    {
                        let _ = self.write_block_data(fs_block, &data[..bs]);
                        replayed += 1;
                    }
                }
            }

            block = self.next_journal_block(commit_pos, first, maxlen);
            scanned += 1;
        }

        if replayed > 0 {
            self.ext3_clean_journal()?;
        }

        Ok(replayed)
    }

    pub fn scan_journal(&mut self) -> Result<JournalInfo, Ext2Error> {
        let mut info = JournalInfo::empty();

        if !self.has_journal() {
            return Ok(info);
        }

        let jsb = self.read_journal_superblock()?;

        info.valid = true;
        info.version = if jsb.is_v2() { 2 } else { 1 };
        info.block_size = jsb.blocksize();
        info.total_blocks = jsb.maxlen();
        info.first_block = jsb.first();
        info.start = jsb.start();
        info.sequence = jsb.start_sequence();
        info.clean = jsb.is_clean();
        info.errno = jsb.errno_val();
        info.journal_inode = EXT2_JOURNAL_INO;

        let journal_inode = self.read_inode(EXT2_JOURNAL_INO)?;
        info.journal_size = journal_inode.size();

        if !info.clean && info.start > 0 {
            self.scan_journal_transactions(&jsb, &mut info)?;
        }

        Ok(info)
    }

    fn scan_journal_transactions(
        &mut self,
        jsb: &JournalSuperblock,
        info: &mut JournalInfo,
    ) -> Result<(), Ext2Error> {
        let maxlen = jsb.maxlen();
        let first = jsb.first();
        let mut block = jsb.start();
        let bs = self.block_size as usize;
        let mut buf = [0u8; 4096];
        let read_size = bs.min(4096);
        let max_scan = maxlen.min(512);
        let mut scanned = 0u32;
        let mut current_tx: Option<usize> = None;

        while scanned < max_scan && info.transaction_count < 32 {
            if self
                .read_journal_block_data(block, &mut buf[..read_size])
                .is_err()
            {
                break;
            }

            if read_size < 12 {
                break;
            }

            let header = JournalHeader::from_buf(&buf);
            if !header.is_valid() {
                break;
            }

            match header.blocktype {
                JBD_DESCRIPTOR_BLOCK => {
                    let tx_idx = info.transaction_count;
                    if tx_idx >= 32 {
                        break;
                    }
                    info.transactions[tx_idx].sequence = header.sequence;
                    info.transactions[tx_idx].start_block = block;
                    info.transactions[tx_idx].active = true;
                    current_tx = Some(tx_idx);

                    let data_count = self.count_descriptor_tags(&buf[..read_size]);
                    info.transactions[tx_idx].data_blocks = data_count;

                    for _ in 0..data_count {
                        block = self.next_journal_block(block, first, maxlen);
                        scanned += 1;
                        if scanned >= max_scan {
                            break;
                        }
                    }
                }
                JBD_COMMIT_BLOCK => {
                    if let Some(idx) = current_tx {
                        if idx < 32 {
                            info.transactions[idx].committed = true;
                        }
                    }
                    info.transaction_count += 1;
                    current_tx = None;
                }
                JBD_REVOKE_BLOCK => {}
                _ => {
                    break;
                }
            }

            block = self.next_journal_block(block, first, maxlen);
            scanned += 1;
        }

        if current_tx.is_some() && info.transaction_count < 32 {
            info.transaction_count += 1;
        }

        Ok(())
    }

    fn count_descriptor_tags(&self, buf: &[u8]) -> u32 {
        let mut offset = 12usize;
        let mut count = 0u32;
        let limit = buf.len();

        loop {
            if offset + 8 > limit {
                break;
            }

            let tag = JournalBlockTag::from_buf(buf, offset);
            count += 1;
            offset += 8;

            if !tag.same_uuid() {
                offset += 16;
            }

            if tag.is_last() {
                break;
            }
        }

        count
    }

    fn next_journal_block(&self, current: u32, first: u32, maxlen: u32) -> u32 {
        let next = current + 1;
        if next >= maxlen {
            first
        } else {
            next
        }
    }
}
