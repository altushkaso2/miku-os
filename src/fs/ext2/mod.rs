pub mod bitmap;
pub mod dir;
pub mod inode_ops;
pub mod journal;
pub mod reader;
pub mod structs;
pub mod write;

use crate::ata::AtaDrive;
use reader::DiskReader;
use structs::*;

pub struct Ext2Fs {
    pub superblock: Superblock,
    pub block_size: u32,
    pub inodes_per_group: u32,
    pub blocks_per_group: u32,
    pub group_count: u32,
    pub groups: [GroupDesc; 32],
    pub reader: DiskReader,
    pub journal_seq: u32,
    pub journal_pos: u32,
    pub journal_maxlen: u32,
    pub journal_first: u32,
    pub journal_active: bool,
    pub txn_active: bool,
    pub txn_desc_pos: u32,
    pub txn_tags: [journal::TxnTag; 16],
    pub txn_tag_count: u8,
}

pub const MAX_DIR_ENTRIES: usize = 64;

impl Ext2Fs {
    #[inline]
    pub fn inode_size(&self) -> u32 {
        self.superblock.inode_size_val()
    }

    #[inline]
    pub fn sectors_per_block(&self) -> u32 {
        self.block_size / 512
    }

    #[inline]
    pub fn block_to_lba(&self, block: u32) -> u32 {
        block * self.sectors_per_block()
    }

    pub fn flush_superblock(&mut self) -> Result<(), Ext2Error> {
        let mut s0 = [0u8; 512];
        let mut s1 = [0u8; 512];
        s0.copy_from_slice(&self.superblock.data[0..512]);
        s1.copy_from_slice(&self.superblock.data[512..1024]);
        self.reader.write_sector(2, &s0)?;
        self.reader.write_sector(3, &s1)?;
        Ok(())
    }

    pub fn flush_group_desc(&mut self, group: usize) -> Result<(), Ext2Error> {
        let gdt_block = if self.block_size == 1024 { 2 } else { 1 };
        let gd_size = self.superblock.group_desc_size() as usize;
        let gd_byte_offset = group * gd_size;
        let sector_offset = gd_byte_offset / 512;
        let offset_in_sector = gd_byte_offset % 512;

        let lba = self.block_to_lba(gdt_block) + sector_offset as u32;

        let mut sector = [0u8; 512];
        self.reader.read_sector(lba, &mut sector)?;

        let write_len = gd_size.min(64);
        sector[offset_in_sector..offset_in_sector + write_len]
            .copy_from_slice(&self.groups[group].data[..write_len]);

        self.reader.write_sector(lba, &sector)?;
        Ok(())
    }

    pub fn write_inode(&mut self, inode_num: u32, inode: &Inode) -> Result<(), Ext2Error> {
        if inode_num == 0 {
            return Err(Ext2Error::InvalidInode);
        }

        let idx = inode_num - 1;
        let group = (idx / self.inodes_per_group) as usize;
        let local_idx = idx % self.inodes_per_group;

        if group >= self.groups.len() {
            return Err(Ext2Error::InvalidInode);
        }

        let inode_table_block = self.groups[group].inode_table();
        let inode_size = self.superblock.inode_size_val();
        let write_size = (inode_size as usize).min(256);
        let byte_offset = local_idx as u64 * inode_size as u64;
        let abs_byte = inode_table_block as u64 * self.block_size as u64 + byte_offset;
        let sector = (abs_byte / 512) as u32;
        let offset_in_sector = (abs_byte % 512) as usize;

        let mut buf = [0u8; 512];
        self.reader.read_sector(sector, &mut buf)?;

        if offset_in_sector + write_size <= 512 {
            buf[offset_in_sector..offset_in_sector + write_size]
                .copy_from_slice(&inode.data[..write_size]);
            self.reader.write_sector(sector, &buf)?;
        } else {
            let first_part = 512 - offset_in_sector;
            buf[offset_in_sector..512].copy_from_slice(&inode.data[..first_part]);
            self.reader.write_sector(sector, &buf)?;

            let mut remaining = write_size - first_part;
            let mut data_pos = first_part;
            let mut next_sector = sector + 1;

            while remaining > 0 {
                self.reader.read_sector(next_sector, &mut buf)?;
                let chunk = remaining.min(512);
                buf[..chunk].copy_from_slice(&inode.data[data_pos..data_pos + chunk]);
                self.reader.write_sector(next_sector, &buf)?;
                data_pos += chunk;
                remaining -= chunk;
                next_sector += 1;
            }
        }

        Ok(())
    }

    pub fn write_block_data(&mut self, block_num: u32, data: &[u8]) -> Result<(), Ext2Error> {
        let spb = self.sectors_per_block();
        let base_lba = self.block_to_lba(block_num);
        let bs = self.block_size as usize;

        for s in 0..spb {
            let offset = (s * 512) as usize;
            if offset >= data.len() || offset >= bs {
                break;
            }

            let mut sector = [0u8; 512];
            let end = (offset + 512).min(data.len()).min(bs);
            let len = end - offset;
            sector[..len].copy_from_slice(&data[offset..offset + len]);

            self.reader.write_sector(base_lba + s, &sector)?;
        }

        Ok(())
    }

    pub fn zero_block(&mut self, block_num: u32) -> Result<(), Ext2Error> {
        let spb = self.sectors_per_block();
        let base_lba = self.block_to_lba(block_num);
        let zero = [0u8; 512];

        for s in 0..spb {
            self.reader.write_sector(base_lba + s, &zero)?;
        }

        Ok(())
    }

    pub fn fs_info(&self) -> Ext2Info {
        Ext2Info {
            block_size: self.block_size,
            total_blocks: self.superblock.blocks_count(),
            free_blocks: self.superblock.free_blocks_count(),
            total_inodes: self.superblock.inodes_count(),
            free_inodes: self.superblock.free_inodes_count(),
            groups: self.group_count,
            inode_size: self.inode_size(),
            has_journal: self.superblock.has_journal(),
            has_extents: self.superblock.has_extents(),
            version: self.superblock.fs_version_str(),
        }
    }
}

pub struct Ext2Info {
    pub block_size: u32,
    pub total_blocks: u32,
    pub free_blocks: u32,
    pub total_inodes: u32,
    pub free_inodes: u32,
    pub groups: u32,
    pub inode_size: u32,
    pub has_journal: bool,
    pub has_extents: bool,
    pub version: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub enum Ext2Error {
    BadMagic,
    IoError,
    UnsupportedVersion,
    TooManyGroups,
    InvalidInode,
    NotDirectory,
    NotFound,
    CorruptedFs,
    BufferTooSmall,
    FileTooLarge,
    NotRegularFile,
    InvalidBlock,
    NoSpace,
    AlreadyExists,
    NotEmpty,
    IsDirectory,
    NoJournal,
}
