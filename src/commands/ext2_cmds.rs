use crate::ata::AtaDrive;
use crate::miku_extfs::ext2::write::TreeResult;
use crate::miku_extfs::ext3::journal::{TxnTag, DEFAULT_JOURNAL_BLOCKS};
use crate::miku_extfs::reader::DiskReader;
use crate::miku_extfs::structs::*;
use crate::miku_extfs::{FsError, MikuFS};
use crate::{cprint, cprintln, print_error, print_success, println, serial_println};
use spin::Mutex;

static mut EXT2_STORAGE: MikuFS = MikuFS {
    superblock: Superblock { data: [0; 1024] },
    block_size: 0,
    inodes_per_group: 0,
    blocks_per_group: 0,
    group_count: 0,
    groups: [GroupDesc { data: [0; 64] }; 32],
    reader: DiskReader {
        drive: AtaDrive::EMPTY,
    },
    journal_seq: 0,
    journal_pos: 0,
    journal_maxlen: 0,
    journal_first: 0,
    journal_active: false,
    txn_active: false,
    txn_desc_pos: 0,
    txn_tags: [TxnTag {
        fs_block: 0,
        journal_pos: 0,
    }; 64],
    txn_tag_count: 0,
    txn_revokes: [0; 128],
    txn_revoke_count: 0,
    block_cache: None,
    superblock_dirty: false,
    groups_dirty: [false; 32],
};

static mut EXT2_READY: bool = false;

static EXT2_LOCK: Mutex<()> = Mutex::new(());

fn with_ext2<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut MikuFS) -> R,
{
    let _guard = EXT2_LOCK.lock();
    unsafe {
        if !EXT2_READY {
            return None;
        }
        Some(f(&mut EXT2_STORAGE))
    }
}

pub fn is_ext2_ready() -> bool {
    let _guard = EXT2_LOCK.lock();
    unsafe { EXT2_READY }
}

pub fn with_ext2_pub<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut MikuFS) -> R,
{
    let _guard = EXT2_LOCK.lock();
    unsafe {
        if !EXT2_READY {
            return None;
        }
        Some(f(&mut EXT2_STORAGE))
    }
}

pub unsafe fn force_unmount() {
    let _guard = EXT2_LOCK.lock();
    EXT2_READY = false;
    EXT2_STORAGE.block_cache = None;
}

fn split_parent_name(path: &str) -> (&str, &str) {
    let trimmed = path.trim_start_matches('/').trim_end_matches('/');
    if trimmed.is_empty() {
        return ("/", "");
    }
    match trimmed.rfind('/') {
        Some(pos) => {
            let parent = &trimmed[..pos];
            let name = &trimmed[pos + 1..];
            if parent.is_empty() {
                ("/", name)
            } else {
                (parent, name)
            }
        }
        None => ("/", trimmed),
    }
}

fn resolve_parent_and_name<'a>(fs: &mut MikuFS, path: &'a str) -> Result<(u32, &'a str), FsError> {
    let (parent_path, name) = split_parent_name(path);
    if name.is_empty() {
        return Err(FsError::InvalidInode);
    }
    let parent_ino = fs.resolve_path(parent_path)?;
    Ok((parent_ino, name))
}

fn parse_ext2_octal(s: &str) -> Option<u16> {
    let mut result: u16 = 0;
    for &b in s.as_bytes() {
        if b < b'0' || b > b'7' {
            return None;
        }
        result = result.checked_mul(8)?.checked_add((b - b'0') as u16)?;
    }
    if result > 0o7777 {
        return None;
    }
    Some(result)
}

fn parse_u16(s: &str) -> Option<u16> {
    let mut result: u16 = 0;
    for &b in s.as_bytes() {
        if b < b'0' || b > b'9' {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((b - b'0') as u16)?;
    }
    Some(result)
}

pub fn cmd_ext2_mount(_args: &str) {
    serial_println!("[miku_extfs] scanning drivers...");
    let drive_order: [usize; 4] = [2, 1, 3, 0];
    for &i in &drive_order {
        serial_println!("[miku_extfs] trying drive {} ...", i);
        if try_mount(i) {
            return;
        }
    }
    print_error!("  no extfs found on any drive");
}

fn try_mount(drive_index: usize) -> bool {
    let drive = match drive_index {
        0 => AtaDrive::primary(),
        1 => AtaDrive::primary_slave(),
        2 => AtaDrive::secondary(),
        _ => AtaDrive::secondary_slave(),
    };
    unsafe {
        EXT2_READY = false;
        EXT2_STORAGE.reader = DiskReader::new(drive);
        EXT2_STORAGE.block_cache = None;
    }
    let mut sector = [0u8; 512];
    let reader = unsafe { &mut EXT2_STORAGE.reader };
    match reader.read_sector(2, &mut sector) {
        Ok(()) => {}
        Err(e) => {
            serial_println!(
                "[miku_extfs] drive {} - cannot read sector 2: {:?}",
                drive_index,
                e
            );
            return false;
        }
    }
    unsafe {
        EXT2_STORAGE.superblock.data[0..512].copy_from_slice(&sector);
    }
    let magic_lo = u16::from_le_bytes([sector[56], sector[57]]);
    if magic_lo != EXT2_MAGIC {
        serial_println!(
            "[miku_extfs] drive {} - bad magic 0x{:04X}, skip",
            drive_index,
            magic_lo
        );
        return false;
    }
    match reader.read_sector(3, &mut sector) {
        Ok(()) => {}
        Err(e) => {
            serial_println!(
                "[miku_extfs] drive {} - cannot read sector 3: {:?}",
                drive_index,
                e
            );
            return false;
        }
    }
    unsafe {
        EXT2_STORAGE.superblock.data[512..1024].copy_from_slice(&sector);
    }
    serial_println!("[miku_extfs] drive {} - found!", drive_index);
    let block_size = unsafe { EXT2_STORAGE.superblock.block_size() };
    let inodes_per_group = unsafe { EXT2_STORAGE.superblock.inodes_per_group() };
    let blocks_per_group = unsafe { EXT2_STORAGE.superblock.blocks_per_group() };
    let blocks_count = unsafe { EXT2_STORAGE.superblock.blocks_count() };
    let group_count = (blocks_count + blocks_per_group - 1) / blocks_per_group;
    let gd_size = unsafe { EXT2_STORAGE.superblock.group_desc_size() } as usize;
    if group_count as usize > 32 {
        print_error!("  miku_extfs: too many block groups ({})", group_count);
        return false;
    }
    unsafe {
        EXT2_STORAGE.block_size = block_size;
        EXT2_STORAGE.inodes_per_group = inodes_per_group;
        EXT2_STORAGE.blocks_per_group = blocks_per_group;
        EXT2_STORAGE.group_count = group_count;
    }
    let gdt_block = if block_size == 1024 { 2 } else { 1 };
    let sectors_per_block = block_size / 512;
    let start_lba = gdt_block * sectors_per_block;
    let total_gd_bytes = group_count as usize * gd_size;
    let total_sectors = ((total_gd_bytes + 511) / 512) as u32;
    let reader = unsafe { &mut EXT2_STORAGE.reader };
    let mut carry = [0u8; 64];
    let mut carry_len = 0usize;
    let mut gd_idx = 0usize;
    for s in 0..total_sectors {
        if reader.read_sector(start_lba + s, &mut sector).is_err() {
            serial_println!("[miku_extfs] gdt read error at lba {}", start_lba + s);
            return false;
        }
        let mut pos = 0usize;
        if carry_len > 0 {
            let need = gd_size - carry_len;
            carry[carry_len..gd_size].copy_from_slice(&sector[..need]);
            if gd_idx < group_count as usize {
                unsafe {
                    EXT2_STORAGE.groups[gd_idx].data[..gd_size].copy_from_slice(&carry[..gd_size]);
                }
                gd_idx += 1;
            }
            pos = need;
            carry_len = 0;
        }
        while pos + gd_size <= 512 && gd_idx < group_count as usize {
            unsafe {
                EXT2_STORAGE.groups[gd_idx].data[..gd_size]
                    .copy_from_slice(&sector[pos..pos + gd_size]);
            }
            gd_idx += 1;
            pos += gd_size;
        }
        if pos < 512 && gd_idx < group_count as usize {
            let remaining = 512 - pos;
            carry[..remaining].copy_from_slice(&sector[pos..]);
            carry_len = remaining;
        }
    }
    unsafe {
        EXT2_READY = true;
        EXT2_STORAGE.init_cache();
        let _ = EXT2_STORAGE.init_journal();
        if EXT2_STORAGE.journal_active && !EXT2_STORAGE.read_journal_superblock()
            .map(|j| j.is_clean())
            .unwrap_or(true)
        {
            match EXT2_STORAGE.ext3_recover() {
                Ok(0) => {}
                Ok(n) => serial_println!("[ext3] recovery: replayed {} blocks", n),
                Err(e) => serial_println!("[ext3] recovery failed: {:?}", e),
            }
        }
    }
    let total_inodes = unsafe { EXT2_STORAGE.superblock.inodes_count() };
    let free_blocks = unsafe { EXT2_STORAGE.superblock.free_blocks_count() };
    let free_inodes = unsafe { EXT2_STORAGE.superblock.free_inodes_count() };
    let version = unsafe { EXT2_STORAGE.superblock.fs_version_str() };
    print_success!("  {} mounted (drive {})", version, drive_index);
    println!("  Block: {} bytes", block_size);
    println!("  Blocks: {} total, {} free", blocks_count, free_blocks);
    println!("  Inodes: {} total, {} free", total_inodes, free_inodes);
    println!("  Groups: {}", group_count);
    println!("  Cache: enabled");
    true
}

pub fn cmd_ext2_ls(path: &str) {
    let path = if path.is_empty() { "/" } else { path };
    let result = with_ext2(|fs| -> Result<([DirEntry; 64], usize), FsError> {
        let ino = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        if !inode.is_directory() {
            return Err(FsError::NotDirectory);
        }
        let mut entries = [const { DirEntry::empty() }; 64];
        let count = fs.read_dir(&inode, &mut entries)?;
        Ok((entries, count))
    });
    match result {
        Some(Ok((entries, count))) => {
            println!("  ext2:{} ({} entries)", path, count);
            for i in 0..count {
                let e = &entries[i];
                let name = e.name_str();
                match e.file_type {
                    FT_DIR => cprintln!(0, 220, 220, "  d {}/", name),
                    FT_SYMLINK => cprintln!(128, 222, 217, "  l {}@", name),
                    _ => println!("  - {} (ino={})", name, e.inode),
                }
            }
        }
        Some(Err(e)) => print_error!("  ext2ls: {:?}", e),
        None => print_error!("  ext2 not mounted (run ext2mount first)"),
    }
}

pub fn cmd_ext2_cat(path: &str) {
    if path.is_empty() {
        println!("Usage: ext2cat <path>");
        return;
    }
    let result = with_ext2(|fs| -> Result<([u8; 512], usize, u64), FsError> {
        let ino = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        if inode.is_directory() {
            return Err(FsError::IsDirectory);
        }
        if !inode.is_regular() && !inode.is_symlink() {
            return Err(FsError::NotRegularFile);
        }
        let size = inode.size();
        let read_size = (size as usize).min(512);
        let mut buf = [0u8; 512];
        let n = fs.read_file(&inode, 0, &mut buf[..read_size])?;
        Ok((buf, n, size))
    });
    match result {
        Some(Ok((buf, n, size))) => {
            if size > 512 {
                println!("  (showing first 512 of {} bytes)", size);
            }
            let s = core::str::from_utf8(&buf[..n]).unwrap_or("(binary data)");
            println!("{}", s);
        }
        Some(Err(e)) => print_error!("  ext2cat: {:?}", e),
        None => print_error!("  ext2 not mounted (run ext2mount first)"),
    }
}

pub fn cmd_ext2_stat(path: &str) {
    if path.is_empty() {
        println!("Usage: ext2stat <path>");
        return;
    }
    let result = with_ext2(|fs| -> Result<(u32, Inode), FsError> {
        let ino = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        Ok((ino, inode))
    });
    match result {
        Some(Ok((ino, inode))) => {
            println!("  Inode: {}", ino);
            println!("  Type:  {:?}", inode.file_type());
            println!("  Mode:  0o{:o}", inode.permissions());
            println!("  Size:  {} bytes", inode.size());
            println!("  Links: {}", inode.links_count());
            println!("  Blocks: {}", inode.blocks());
            println!("  UID:   {}", inode.uid_full());
            println!("  GID:   {}", inode.gid_full());
            if inode.uses_extents() {
                println!("  Extents: yes");
            }
            if inode.has_inline_data() {
                println!("  Inline: yes");
            }
            if inode.is_fast_symlink() {
                let target = inode.fast_symlink_target();
                if let Ok(t) = core::str::from_utf8(target) {
                    println!("  Target: {}", t);
                }
            }
        }
        Some(Err(e)) => print_error!("  ext2stat: {:?}", e),
        None => print_error!("  ext2 not mounted (run ext2mount first)"),
    }
}

pub fn cmd_ext2_info() {
    let result = with_ext2(|fs| fs.fs_info());
    match result {
        Some(info) => {
            println!("  Version: {}", info.version);
            println!("  Block size: {} bytes", info.block_size);
            println!(
                "  Blocks: {} / {} used",
                info.total_blocks - info.free_blocks,
                info.total_blocks
            );
            println!(
                "  Inodes: {} / {} used",
                info.total_inodes - info.free_inodes,
                info.total_inodes
            );
            println!("  Groups: {}", info.groups);
            println!("  Inode size: {} bytes", info.inode_size);
            println!("  Journal: {}", if info.has_journal { "yes" } else { "no" });
            println!("  Extents: {}", if info.has_extents { "yes" } else { "no" });
        }
        None => print_error!("  ext2 not mounted (run ext2mount first)"),
    }
}

pub fn cmd_ext2_write(path: &str, text: &str) {
    if path.is_empty() || text.is_empty() {
        println!("Usage: ext2write <path> <text>");
        return;
    }

    let disk_sw = crate::timing::Stopwatch::start();
    let result = with_ext2(|fs| -> Result<u32, FsError> {
        let (parent_ino, filename) = resolve_parent_and_name(fs, path)?;
        match fs.ext2_lookup_in_dir(parent_ino, filename)? {
            Some(ino) => {
                fs.ext2_truncate(ino)?;
                fs.ext3_write_file(ino, text.as_bytes(), 0)?;
                Ok(ino)
            }
            None => {
                let ino = fs.ext3_create_file(parent_ino, filename, 0o644)?;
                fs.ext3_write_file(ino, text.as_bytes(), 0)?;
                Ok(ino)
            }
        }
    });
    let disk_ms = disk_sw.elapsed_ms();

    let render_sw = crate::timing::Stopwatch::start();
    match result {
        Some(Ok(ino)) => print_success!("  written to inode {}  [disk {}ms]", ino, disk_ms),
        Some(Err(e))  => print_error!("  ext2write: {:?}", e),
        None          => print_error!("  ext2 not mounted"),
    }
    let render_us = render_sw.elapsed_us();
    crate::serial_println!("[timing] ext2write disk={}ms render={}us", disk_ms, render_us);
}

pub fn cmd_ext2_mkdir(path: &str) {
    if path.is_empty() {
        println!("Usage: ext2mkdir <path>");
        return;
    }
    let result = with_ext2(|fs| -> Result<u32, FsError> {
        let (parent_ino, dirname) = resolve_parent_and_name(fs, path)?;
        fs.ext3_create_dir(parent_ino, dirname, 0o755)
    });
    match result {
        Some(Ok(ino)) => print_success!("  created dir inode {}", ino),
        Some(Err(e)) => print_error!("  ext2mkdir: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_rm(path: &str) {
    if path.is_empty() {
        println!("Usage: ext2rm <path>");
        return;
    }
    let result = with_ext2(|fs| -> Result<(), FsError> {
        let (parent_ino, name) = resolve_parent_and_name(fs, path)?;
        fs.ext3_delete_file(parent_ino, name)
    });
    match result {
        Some(Ok(())) => print_success!("  deleted"),
        Some(Err(e)) => print_error!("  ext2rm: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_rmdir(path: &str) {
    if path.is_empty() {
        println!("Usage: ext2rmdir <path>");
        return;
    }
    let result = with_ext2(|fs| -> Result<(), FsError> {
        let (parent_ino, name) = resolve_parent_and_name(fs, path)?;
        fs.ext3_delete_dir(parent_ino, name)
    });
    match result {
        Some(Ok(())) => print_success!("  removed dir"),
        Some(Err(e)) => print_error!("  ext2rmdir: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_rm_rf(path: &str) {
    if path.is_empty() {
        println!("Usage: ext2rm -rf <path>");
        return;
    }
    let result = with_ext2(|fs| -> Result<u32, FsError> {
        let (parent_ino, name) = resolve_parent_and_name(fs, path)?;
        fs.ext2_delete_recursive(parent_ino, name)
    });
    match result {
        Some(Ok(n)) => print_success!("  removed {} entries", n),
        Some(Err(e)) => print_error!("  ext2rm -rf: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_symlink(target: &str, linkname: &str) {
    if target.is_empty() || linkname.is_empty() {
        println!("Usage: ext2ln -s <target> <linkname>");
        return;
    }
    let result = with_ext2(|fs| -> Result<u32, FsError> {
        let (parent_ino, name) = resolve_parent_and_name(fs, linkname)?;
        fs.ext2_create_symlink(parent_ino, name, target)
    });
    match result {
        Some(Ok(ino)) => print_success!("  symlink inode {} -> {}", ino, target),
        Some(Err(e)) => print_error!("  ext2ln: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_rename(old_path: &str, new_name: &str) {
    if old_path.is_empty() || new_name.is_empty() {
        println!("Usage: ext2mv <path> <newname>");
        return;
    }
    let result = with_ext2(|fs| -> Result<(), FsError> {
        let (parent_ino, old_name) = resolve_parent_and_name(fs, old_path)?;
        let actual_new_name = match new_name.rfind('/') {
            Some(pos) => &new_name[pos + 1..],
            None => new_name,
        };
        if actual_new_name.is_empty() {
            return Err(FsError::InvalidInode);
        }
        fs.ext2_rename(parent_ino, old_name, actual_new_name)
    });
    match result {
        Some(Ok(())) => print_success!("  renamed"),
        Some(Err(e)) => print_error!("  ext2mv: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_chmod(mode_str: &str, path: &str) {
    if mode_str.is_empty() || path.is_empty() {
        println!("Usage: ext2chmod <mode> <path>");
        return;
    }
    let mode = parse_ext2_octal(mode_str);
    if mode.is_none() {
        print_error!("  invalid mode '{}'", mode_str);
        return;
    }
    let result = with_ext2(|fs| -> Result<(), FsError> {
        let ino = fs.resolve_path(path)?;
        fs.ext2_chmod(ino, mode.unwrap())
    });
    match result {
        Some(Ok(())) => print_success!("  mode set to 0o{}", mode_str),
        Some(Err(e)) => print_error!("  ext2chmod: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_chown(uid_str: &str, gid_str: &str, path: &str) {
    if uid_str.is_empty() || path.is_empty() {
        println!("Usage: ext2chown <uid> <gid> <path>");
        return;
    }
    let uid = match parse_u16(uid_str) {
        Some(v) => v,
        None => {
            print_error!("  invalid uid");
            return;
        }
    };
    let gid = if gid_str.is_empty() {
        uid
    } else {
        match parse_u16(gid_str) {
            Some(v) => v,
            None => {
                print_error!("  invalid gid");
                return;
            }
        }
    };
    let result = with_ext2(|fs| -> Result<(), FsError> {
        let ino = fs.resolve_path(path)?;
        fs.ext2_chown(ino, uid, gid)
    });
    match result {
        Some(Ok(())) => print_success!("  owner set to {}:{}", uid, gid),
        Some(Err(e)) => print_error!("  ext2chown: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_cp(src: &str, dst: &str) {
    if src.is_empty() || dst.is_empty() {
        println!("Usage: ext2cp <src> <dst>");
        return;
    }
    let result = with_ext2(|fs| -> Result<u32, FsError> {
        let src_ino = fs.resolve_path(src)?;
        let (dst_parent_ino, dst_name) = resolve_parent_and_name(fs, dst)?;
        fs.ext4_copy_file(src_ino, dst_parent_ino, dst_name)
    });
    match result {
        Some(Ok(ino)) => print_success!("  copied to inode {}", ino),
        Some(Err(e)) => print_error!("  ext2cp: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_du(path: &str) {
    let path = if path.is_empty() { "/" } else { path };
    let result = with_ext2(|fs| -> Result<(u32, u64), FsError> {
        let ino = fs.resolve_path(path)?;
        fs.ext2_dir_size(ino)
    });
    match result {
        Some(Ok((files, bytes))) => {
            println!("  {} files, {} bytes total", files, bytes);
            if bytes >= 1024 {
                println!("  ({} KB)", bytes / 1024);
            }
        }
        Some(Err(e)) => print_error!("  ext2du: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_tree(path: &str) {
    let path = if path.is_empty() { "/" } else { path };
    let mut tree = TreeResult::new();
    let result = with_ext2(|fs| -> Result<(), FsError> {
        let ino = fs.resolve_path(path)?;
        fs.ext2_tree(ino, "", &mut tree)
    });
    match result {
        Some(Ok(())) => {
            cprintln!(0, 220, 220, "  {}", path);
            for i in 0..tree.count {
                let e = &tree.entries[i];
                let depth = e.depth as usize;
                for _ in 0..depth {
                    cprint!(120, 140, 140, "    ");
                }
                if e.is_last {
                    cprint!(120, 140, 140, "/ ");
                } else {
                    cprint!(120, 140, 140, "--- ");
                }
                if e.is_dir {
                    cprintln!(0, 220, 220, "{}/", e.name_str());
                } else if e.is_symlink {
                    cprintln!(128, 222, 217, "{}@", e.name_str());
                } else {
                    cprintln!(230, 240, 240, "{} ({}b)", e.name_str(), e.size);
                }
            }
            println!("  {} entries", tree.count);
        }
        Some(Err(e)) => print_error!("  ext2tree: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_fsck() {
    let result = with_ext2(|fs| fs.ext2_fsck());
    match result {
        Some(r) => {
            if !r.checked {
                print_error!("  fsck failed to run");
                return;
            }
            cprintln!(57, 197, 187, "  ext2 filesystem check");
            println!("  Block size:  {} bytes", r.block_size);
            println!("  Total blocks: {}", r.total_blocks);
            println!("  Free blocks:  {}", r.free_blocks);
            println!("  Total inodes: {}", r.total_inodes);
            println!("  Free inodes:  {}", r.free_inodes);
            println!("  Used inodes:  {}", r.used_inodes);
            if r.bad_magic {
                print_error!("  error: bad superblock magic");
            }
            if !r.root_ok {
                print_error!("  error: cannot read root inode");
            }
            if r.root_not_dir {
                print_error!("  error: root inode is not a directory");
            }
            if r.bad_groups > 0 {
                print_error!("  error: {} bad group descriptors", r.bad_groups);
            }
            if r.orphan_inodes > 0 {
                cprintln!(
                    220,
                    220,
                    100,
                    "  warning: {} orphan inodes",
                    r.orphan_inodes
                );
            }
            if r.errors == 0 {
                print_success!("  filesystem ok");
            } else {
                print_error!("  {} errors found", r.errors);
            }
        }
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_append(path: &str, text: &str) {
    if path.is_empty() || text.is_empty() {
        println!("Usage: ext2append <path> <text>");
        return;
    }
    let result = with_ext2(|fs| -> Result<usize, FsError> {
        let ino = fs.resolve_path(path)?;
        fs.ext2_append_file(ino, text.as_bytes())
    });
    match result {
        Some(Ok(n)) => print_success!("  appended {} bytes", n),
        Some(Err(e)) => print_error!("  ext2append: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_hardlink(existing: &str, linkname: &str) {
    if existing.is_empty() || linkname.is_empty() {
        println!("Usage: ext2link <existing> <linkname>");
        return;
    }
    let result = with_ext2(|fs| -> Result<(), FsError> {
        let target_ino = fs.resolve_path(existing)?;
        let (parent_ino, name) = resolve_parent_and_name(fs, linkname)?;
        fs.ext2_hardlink(parent_ino, name, target_ino)
    });
    match result {
        Some(Ok(())) => print_success!("  hardlink created"),
        Some(Err(e)) => print_error!("  ext2link: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext3_info() {
    let result = with_ext2(|fs| fs.scan_journal());
    match result {
        Some(Ok(info)) => {
            if !info.valid {
                print_error!("  no journal found");
                return;
            }
            cprintln!(57, 197, 187, "  ext3 Journal Info");
            println!(
                "  Version:    {}",
                if info.version == 2 { "JBD2" } else { "JBD1" }
            );
            println!("  Block size: {} bytes", info.block_size);
            println!("  Total:      {} blocks", info.total_blocks);
            println!("  Size:        {} KB", info.journal_size / 1024);
            println!("  First:      block {}", info.first_block);
            println!("  Sequence:    {}", info.sequence);
            println!("  Start:      {}", info.start);
            if info.clean {
                print_success!("  Status:      clean");
            } else {
                print_error!(
                    "  Status:      dirty ({} transactions)",
                    info.transaction_count
                );
            }
            if info.errno != 0 {
                print_error!("  Errno:       {}", info.errno);
            }
        }
        Some(Err(FsError::NoJournal)) => print_error!("  no journal"),
        Some(Err(e)) => print_error!("  ext3info: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext3_journal() {
    let result = with_ext2(|fs| fs.scan_journal());
    match result {
        Some(Ok(info)) => {
            if !info.valid {
                print_error!("  no journal found");
                return;
            }
            if info.clean {
                print_success!("  journal is clean");
                return;
            }
            cprintln!(
                57,
                197,
                187,
                "  Journal Transactions ({}):",
                info.transaction_count
            );
            for i in 0..info.transaction_count {
                let tx = &info.transactions[i];
                if !tx.active {
                    continue;
                }
                if tx.committed {
                    cprintln!(
                        100,
                        220,
                        150,
                        "  {:>6}  {:>8}  {:>6}  committed",
                        tx.sequence,
                        tx.start_block,
                        tx.data_blocks
                    );
                } else {
                    cprintln!(
                        255,
                        50,
                        50,
                        "  {:>6}  {:>8}  {:>6}  incomplete",
                        tx.sequence,
                        tx.start_block,
                        tx.data_blocks
                    );
                }
            }
        }
        Some(Err(FsError::NoJournal)) => print_error!("  no journal"),
        Some(Err(e)) => print_error!("  ext3journal: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext3_mkjournal() {
    let result =
        with_ext2(|fs| -> Result<(), FsError> { fs.ext3_create_journal(DEFAULT_JOURNAL_BLOCKS) });
    match result {
        Some(Ok(())) => {
            print_success!("  ext3 journal created ({} blocks)", DEFAULT_JOURNAL_BLOCKS);
        }
        Some(Err(FsError::AlreadyExists)) => print_error!("  journal already exists"),
        Some(Err(e)) => print_error!("  ext3mkjournal: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext3_clean() {
    let result = with_ext2(|fs| fs.ext3_clean_journal());
    match result {
        Some(Ok(())) => print_success!("  journal marked clean"),
        Some(Err(FsError::NoJournal)) => print_error!("  no journal found"),
        Some(Err(e)) => print_error!("  ext3clean: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext3_recover() {
    let result = with_ext2(|fs| fs.ext3_recover());
    match result {
        Some(Ok(n)) => {
            if n == 0 {
                print_success!("  no recovery needed");
            } else {
                print_success!("  recovered {} blocks", n);
            }
        }
        Some(Err(FsError::NoJournal)) => print_error!("  no journal found"),
        Some(Err(e)) => print_error!("  ext3recover: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext2_cache() {
    let result = with_ext2(|fs| match &fs.block_cache {
        Some(c) => {
            cprintln!(57, 197, 187, "  Block Cache");
            println!("  Entries:    {}/{}", c.cached_entries(), c.capacity());
            println!("  Memory:     {} KB", c.total_bytes() / 1024);
            println!("  Hits:       {}", c.hits);
            println!("  Misses:     {}", c.misses);
            println!("  Hit rate:   {}%", c.hit_rate());
            println!("  Evictions:  {}", c.evictions);
        }
        None => {
            print_error!("  cache not initialized");
        }
    });
    if result.is_none() {
        print_error!("  ext2 not mounted");
    }
}

pub fn cmd_ext2_cache_flush() {
    let result = with_ext2(|fs| {
        if let Some(ref mut c) = fs.block_cache {
            c.clear();
            print_success!("  cache flushed");
        } else {
            print_error!("  cache not initialized");
        }
    });
    if result.is_none() {
        print_error!("  ext2 not mounted");
    }
}

pub fn cmd_ext4_info() {
    let result = with_ext2(|fs| {
        let info = fs.fs_info();
        cprintln!(57, 197, 187, "  ext4 Filesystem Info");
        println!("  Version:  {}", info.version);
        println!(
            "  Extents:  {}",
            if info.has_extents {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!(
            "  Journal:  {}",
            if info.has_journal {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!(
            "  64bit:    {}",
            if fs.superblock.has_64bit() {
                "yes"
            } else {
                "no"
            }
        );
        println!(
            "  Checksums:{}",
            if fs.superblock.has_metadata_csum() {
                " crc32c"
            } else {
                " none"
            }
        );
        println!(
            "  Flex BG:  {}",
            if fs.superblock.has_flex_bg() {
                "yes"
            } else {
                "no"
            }
        );
        let sb_ok = fs.verify_superblock_csum();
        if fs.superblock.has_metadata_csum() {
            if sb_ok {
                print_success!("  SB csum:  valid");
            } else {
                print_error!("  SB csum:  invalid");
            }
        }
    });
    if result.is_none() {
        print_error!("  ext2 not mounted");
    }
}

pub fn cmd_ext4_enable_extents() {
    let result = with_ext2(|fs| -> Result<(), FsError> { fs.enable_extents_feature() });
    match result {
        Some(Ok(())) => print_success!("  extents enabled"),
        Some(Err(e)) => print_error!("  ext4extents: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext4_checksums() {
    let result = with_ext2(|fs| {
        cprintln!(57, 197, 187, "  Checksum Verification");
        let sb_ok = fs.verify_superblock_csum();
        println!("  Superblock: {}", if sb_ok { "ok" } else { "fail" });

        let gc = fs.group_count as usize;
        let mut gd_ok = 0u32;
        let mut gd_fail = 0u32;
        let mut bb_ok = 0u32;
        let mut bb_fail = 0u32;
        let mut ib_ok = 0u32;
        let mut ib_fail = 0u32;

        for g in 0..gc.min(32) {
            if fs.verify_group_desc_csum(g) {
                gd_ok += 1;
            } else {
                gd_fail += 1;
            }
            if fs.verify_block_bitmap_csum(g) {
                bb_ok += 1;
            } else {
                bb_fail += 1;
            }
            if fs.verify_inode_bitmap_csum(g) {
                ib_ok += 1;
            } else {
                ib_fail += 1;
            }
        }

        println!("  Group descs:    {} ok, {} fail", gd_ok, gd_fail);
        println!("  Block bitmaps:  {} ok, {} fail", bb_ok, bb_fail);
        println!("  Inode bitmaps:  {} ok, {} fail", ib_ok, ib_fail);

        let mut ino_ok = 0u32;
        let mut ino_fail = 0u32;
        let max_check = fs.superblock.inodes_count().min(64);
        for ino in 1..=max_check {
            if let Ok(inode) = fs.read_inode(ino) {
                if inode.mode() != 0 {
                    if fs.verify_inode_csum(ino, &inode) {
                        ino_ok += 1;
                    } else {
                        ino_fail += 1;
                    }
                }
            }
        }
        println!(
            "  Inodes (first {}): {} ok, {} fail",
            max_check, ino_ok, ino_fail
        );
    });
    if result.is_none() {
        print_error!("  ext2 not mounted");
    }
}

pub fn cmd_ext4_extent_info(path: &str) {
    if path.is_empty() {
        println!("Usage: ext4extinfo <path>");
        return;
    }
    let result = with_ext2(|fs| -> Result<(), FsError> {
        let ino = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        if !inode.uses_extents() {
            println!("  inode {} does not use extents (indirect blocks)", ino);
            return Ok(());
        }
        let header = inode.extent_header();
        println!("  Inode: {}", ino);
        println!("  Extent tree depth: {}", header.depth);
        println!("  Entries: {} / {}", header.entries, header.max);
        let count = fs.ext4_extent_count(&inode)?;
        println!("  Total extents: {}", count);
        if header.depth == 0 {
            for i in 0..header.entries as usize {
                let ext = inode.extent_at(i);
                println!(
                    "    [{}] logical={} len={} phys={}",
                    i,
                    ext.block,
                    ext.actual_len(),
                    ext.start()
                );
            }
        }
        Ok(())
    });
    match result {
        Some(Ok(())) => {}
        Some(Err(e)) => print_error!("  ext4extinfo: {:?}", e),
        None => print_error!("  ext2 not mounted"),
    }
}

pub fn cmd_ext4_write(path: &str, text: &str) {
    if path.is_empty() || text.is_empty() {
        println!("Usage: ext4write <path> <text>");
        return;
    }
    let disk_sw = crate::timing::Stopwatch::start();
    let result = with_ext2_pub(|fs| -> Result<u32, FsError> {
        let (parent_ino, filename) = resolve_parent_and_name(fs, path)?;
        fs.ext3_write_file_create_or_overwrite(parent_ino, filename, 0o644, text.as_bytes())
    });
    let disk_ms = disk_sw.elapsed_ms();
    let render_sw = crate::timing::Stopwatch::start();
    match result {
        Some(Ok(ino)) => print_success!(
            "  [ext4] written to inode {} (extents+journal)  [disk {}ms]",
            ino, disk_ms
        ),
        Some(Err(e)) => print_error!("  ext4write: {:?}", e),
        None => print_error!("  not mounted"),
    }
    let render_us = render_sw.elapsed_us();
    crate::serial_println!(
        "[timing] ext4write disk={}ms render={}us",
        disk_ms,
        render_us
    );
}
