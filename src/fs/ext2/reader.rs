use super::structs::*;
use super::Ext2Error;
use crate::ata::AtaDrive;

pub struct DiskReader {
    pub drive: AtaDrive,
}

impl DiskReader {
    pub fn new(drive: AtaDrive) -> Self {
        Self { drive }
    }

    pub fn read_sector(&mut self, lba: u32, buf: &mut [u8; 512]) -> Result<(), Ext2Error> {
        self.drive
            .read_sector(lba, buf)
            .map_err(|_| Ext2Error::IoError)
    }

    pub fn write_sector(&mut self, lba: u32, buf: &[u8; 512]) -> Result<(), Ext2Error> {
        self.drive
            .write_sector(lba, buf)
            .map_err(|_| Ext2Error::IoError)
    }

    pub fn read_superblock(&mut self) -> Result<Superblock, Ext2Error> {
        let mut sb = Superblock::zeroed();
        let mut sector = [0u8; 512];
        self.read_sector(2, &mut sector)?;
        sb.data[0..512].copy_from_slice(&sector);
        self.read_sector(3, &mut sector)?;
        sb.data[512..1024].copy_from_slice(&sector);
        Ok(sb)
    }

    pub fn read_group_descriptors(
        &mut self,
        gdt_block: u32,
        block_size: u32,
        count: usize,
        gd_size: usize,
        groups: &mut [GroupDesc],
    ) -> Result<(), Ext2Error> {
        let total_bytes = count * gd_size;
        let blocks_needed = (total_bytes as u32 + block_size - 1) / block_size;
        let sectors_per_block = block_size / 512;
        let start_lba = gdt_block * sectors_per_block;
        let total_sectors = blocks_needed * sectors_per_block;

        let mut sector_buf = [0u8; 512];
        let mut gd_idx = 0usize;
        let mut carry_buf = [0u8; 64];
        let mut carry_len = 0usize;

        for s in 0..total_sectors {
            self.read_sector(start_lba + s, &mut sector_buf)?;
            let mut pos = 0usize;

            if carry_len > 0 {
                let need = gd_size - carry_len;
                carry_buf[carry_len..gd_size].copy_from_slice(&sector_buf[..need]);
                if gd_idx < count {
                    groups[gd_idx].data[..gd_size].copy_from_slice(&carry_buf[..gd_size]);
                    gd_idx += 1;
                }
                pos = need;
                carry_len = 0;
            }

            while pos + gd_size <= 512 && gd_idx < count {
                groups[gd_idx].data[..gd_size].copy_from_slice(&sector_buf[pos..pos + gd_size]);
                gd_idx += 1;
                pos += gd_size;
            }

            if pos < 512 && gd_idx < count {
                let remaining = 512 - pos;
                carry_buf[..remaining].copy_from_slice(&sector_buf[pos..]);
                carry_len = remaining;
            }
        }

        Ok(())
    }

    pub fn read_inode(
        &mut self,
        inode_num: u32,
        sb: &Superblock,
        groups: &[GroupDesc],
    ) -> Result<Inode, Ext2Error> {
        if inode_num == 0 {
            return Err(Ext2Error::InvalidInode);
        }

        let inodes_per_group = sb.inodes_per_group();
        let inode_size = sb.inode_size_val();
        let block_size = sb.block_size();

        let idx = inode_num - 1;
        let group = (idx / inodes_per_group) as usize;
        let local_idx = idx % inodes_per_group;

        if group >= groups.len() {
            return Err(Ext2Error::InvalidInode);
        }

        let inode_table_block = groups[group].inode_table();
        let byte_offset = local_idx as u64 * inode_size as u64;
        let abs_byte = inode_table_block as u64 * block_size as u64 + byte_offset;
        let sector = (abs_byte / 512) as u32;
        let offset_in_sector = (abs_byte % 512) as usize;

        let mut inode = Inode::zeroed();
        let read_size = (inode_size as usize).min(256);
        inode.on_disk_size = read_size as u16;

        let mut buf = [0u8; 512];
        self.read_sector(sector, &mut buf)?;

        if offset_in_sector + read_size <= 512 {
            inode.data[..read_size]
                .copy_from_slice(&buf[offset_in_sector..offset_in_sector + read_size]);
        } else {
            let first_part = 512 - offset_in_sector;
            inode.data[..first_part].copy_from_slice(&buf[offset_in_sector..512]);

            let mut remaining = read_size - first_part;
            let mut data_pos = first_part;
            let mut next_sector = sector + 1;

            while remaining > 0 {
                self.read_sector(next_sector, &mut buf)?;
                let chunk = remaining.min(512);
                inode.data[data_pos..data_pos + chunk].copy_from_slice(&buf[..chunk]);
                data_pos += chunk;
                remaining -= chunk;
                next_sector += 1;
            }
        }

        Ok(inode)
    }
}
