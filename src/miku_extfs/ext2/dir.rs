use crate::miku_extfs::structs::*;
use crate::miku_extfs::{FsError, MikuFS};

pub const EXT2_MAX_DIR_ENTRIES: usize = 64;

impl MikuFS {
    pub fn read_dir(&mut self, inode: &Inode, entries: &mut [DirEntry]) -> Result<usize, FsError> {
        if !inode.is_directory() {
            return Err(FsError::NotDirectory);
        }

        let dir_size = inode.size() as usize;
        let bs = self.block_size as usize;
        let mut count = 0usize;
        let mut file_offset = 0usize;

        while file_offset < dir_size && count < entries.len() {
            let logical_block = (file_offset / bs) as u32;
            let phys_block = self.get_file_block(inode, logical_block)?;

            if phys_block == 0 {
                file_offset += bs;
                continue;
            }

            let mut block_buf = [0u8; 4096];
            let read_size = bs.min(4096);
            self.read_block_into(phys_block, &mut block_buf[..read_size])?;

            let mut pos = 0usize;

            while pos + 8 <= read_size && count < entries.len() {
                let abs_pos = file_offset + pos;
                if abs_pos >= dir_size {
                    break;
                }

                let raw_inode = u32::from_le_bytes([
                    block_buf[pos],
                    block_buf[pos + 1],
                    block_buf[pos + 2],
                    block_buf[pos + 3],
                ]);
                let rec_len = u16::from_le_bytes([block_buf[pos + 4], block_buf[pos + 5]]) as usize;
                let name_len = block_buf[pos + 6] as usize;
                let file_type = block_buf[pos + 7];

                if rec_len == 0 || rec_len > bs {
                    break;
                }

                if raw_inode != 0 && name_len > 0 && pos + 8 + name_len <= read_size {
                    let mut entry = DirEntry::empty();
                    entry.inode = raw_inode;
                    entry.file_type = file_type;
                    let copy_len = name_len.min(MAX_NAME);
                    entry.name_len = copy_len as u8;
                    entry.name[..copy_len].copy_from_slice(&block_buf[pos + 8..pos + 8 + copy_len]);
                    entries[count] = entry;
                    count += 1;
                }

                pos += rec_len;
            }

            file_offset += bs;
        }

        Ok(count)
    }

    pub fn lookup(&mut self, dir_inode: &Inode, name: &str) -> Result<u32, FsError> {
        let mut entries = [const { DirEntry::empty() }; EXT2_MAX_DIR_ENTRIES];
        let count = self.read_dir(dir_inode, &mut entries)?;
        let name_bytes = name.as_bytes();

        for i in 0..count {
            let entry = &entries[i];
            let elen = entry.name_len as usize;
            if elen == name_bytes.len() && &entry.name[..elen] == name_bytes {
                return Ok(entry.inode);
            }
        }

        Err(FsError::NotFound)
    }

    pub fn resolve_path(&mut self, path: &str) -> Result<u32, FsError> {
        let mut current_ino = EXT2_ROOT_INO;

        if path.is_empty() || path == "/" {
            return Ok(current_ino);
        }

        for component in path.split('/') {
            if component.is_empty() || component == "." {
                continue;
            }
            if component == ".." {
                let inode = self.read_inode(current_ino)?;
                current_ino = self.lookup(&inode, "..")?;
                continue;
            }

            let inode = self.read_inode(current_ino)?;
            if !inode.is_directory() {
                return Err(FsError::NotDirectory);
            }

            current_ino = self.lookup(&inode, component)?;
        }

        Ok(current_ino)
    }
}
