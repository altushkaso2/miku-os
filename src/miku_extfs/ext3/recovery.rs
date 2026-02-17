use crate::miku_extfs::{MikuFS, FsError};
use crate::miku_extfs::structs::*;
use super::journal::*;

impl MikuFS {
    pub fn ext3_clean_journal(&mut self) -> Result<(), FsError> {
        if !self.has_journal() {
            return Err(FsError::NoJournal);
        }
        let j_inode = self.read_inode(EXT2_JOURNAL_INO)?;
        let disk_blk = j_inode.block(0);
        if disk_blk == 0 {
            return Err(FsError::CorruptedFs);
        }
        let bs = self.block_size as usize;
        let mut buf = [0u8; 4096];
        self.read_block_into(disk_blk, &mut buf[..bs])?;
        let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != JBD_MAGIC {
            return Err(FsError::CorruptedFs);
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

    pub fn ext3_recover(&mut self) -> Result<u32, FsError> {
    if !self.has_journal() {
        return Err(FsError::NoJournal);
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
    let mut revoked: [u32; 128] = [0; 128];
    let mut revoke_count = 0usize;

    while scanned < max_scan {
        if self.read_journal_block_data(block, &mut buf[..read_size]).is_err() {
            break;
        }
        let header = JournalHeader::from_buf(&buf);
        if !header.is_valid() {
            break;
        }

        if header.blocktype == JBD_REVOKE_BLOCK {
            if read_size >= 16 {
                let rev_size = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]) as usize;
                let mut roff = 16;
                while roff + 4 <= rev_size && roff + 4 <= read_size {
                    let rblk = u32::from_be_bytes([buf[roff], buf[roff+1], buf[roff+2], buf[roff+3]]);
                    if revoke_count < 128 {
                        revoked[revoke_count] = rblk;
                        revoke_count += 1;
                    }
                    roff += 4;
                }
            }
            block = self.next_journal_block(block, first, maxlen);
            scanned += 1;
            continue;
        }

        if !header.is_descriptor() {
            block = self.next_journal_block(block, first, maxlen);
            scanned += 1;
            continue;
        }

        tag_count = 0;
        committed = false;

        let mut offset = 12usize;
        let mut data_pos = self.next_journal_block(block, first, maxlen);
        loop {
            if offset + 8 > read_size {
                break;
            }
            let tag = JournalBlockTag::from_buf(&buf, offset);
            if tag_count < 64 {
                tags[tag_count] = (tag.blocknr, data_pos);
                tag_count += 1;
                data_pos = self.next_journal_block(data_pos, first, maxlen);
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

        if self.read_journal_block_data(commit_pos, &mut buf[..read_size]).is_ok() {
            let ch = JournalHeader::from_buf(&buf);
            if ch.is_valid() && ch.is_commit() && ch.sequence == header.sequence {
                committed = true;
            }
        }

        if committed {
            for i in 0..tag_count {
                let (fs_block, j_pos) = tags[i];
                let mut is_revoked = false;
                for r in 0..revoke_count {
                    if revoked[r] == fs_block {
                        is_revoked = true;
                        break;
                    }
                }
                if !is_revoked {
                    let mut data = [0u8; 4096];
                    if self.read_journal_block_data(j_pos, &mut data[..read_size]).is_ok() {
                        let _ = self.write_block_data(fs_block, &data[..bs]);
                        replayed += 1;
                    }
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

    pub fn scan_journal(&mut self) -> Result<JournalInfo, FsError> {
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
        &mut self, jsb: &JournalSuperblock, info: &mut JournalInfo,
    ) -> Result<(), FsError> {
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
            if self.read_journal_block_data(block, &mut buf[..read_size]).is_err() {
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

    pub fn next_journal_block(&self, current: u32, first: u32, maxlen: u32) -> u32 {
        let next = current + 1;
        if next >= maxlen {
            first
        } else {
            next
        }
    }
}
