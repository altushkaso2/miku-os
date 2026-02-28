use crate::miku_extfs::structs::*;
use crate::miku_extfs::FsError;
use crate::{cprint, cprintln, print_error, print_success, println};
use crate::commands::ext2_cmds::{with_ext2_pub, is_ext2_ready};
use crate::miku_extfs::ext2::write::TreeResult;

fn split_parent_name(path: &str) -> (&str, &str) {
    let trimmed = path.trim_start_matches('/').trim_end_matches('/');
    if trimmed.is_empty() { return ("/", ""); }
    match trimmed.rfind('/') {
        Some(pos) => {
            let parent = &trimmed[..pos];
            let name = &trimmed[pos + 1..];
            if parent.is_empty() { ("/", name) } else { (parent, name) }
        }
        None => ("/", trimmed),
    }
}

fn resolve_parent_and_name(
    fs: &mut crate::miku_extfs::MikuFS,
    path: &str,
) -> Result<(u32, &'static str), FsError> {
    let (parent_path, name) = split_parent_name(path);
    if name.is_empty() { return Err(FsError::InvalidInode); }
    let parent_ino = fs.resolve_path(parent_path)?;
    let name_static: &'static str = unsafe { core::mem::transmute(name) };
    Ok((parent_ino, name_static))
}

pub fn cmd_ext3_mount(_args: &str) {.
    crate::commands::ext2_cmds::cmd_ext2_mount(_args);

    if !is_ext2_ready() {
        return; 
    }

    let has_journal = with_ext2_pub(|fs| fs.superblock.has_journal()).unwrap_or(false);
    if !has_journal {
        print_error!("  ext3mount: no journal found on this filesystem.");
        print_error!("  This is an ext2 image.  Run ext3mkjournal first,");
        print_error!("  then remount with ext3mount.");
        unsafe {
            crate::commands::ext2_cmds::force_unmount();
        }
        return;
    }

    let journal_ok = with_ext2_pub(|fs| {
        match fs.scan_journal() {
            Ok(info) => {
                if info.clean {
                    print_success!("  ext3 journal: active, clean (JBD{}, {} blocks)",
                        if info.version == 2 { "2" } else { "1" },
                        info.total_blocks);
                } else {
                    crate::print_warn!(
                        "  ext3 journal: active, dirty ({} uncommitted transactions)",
                        info.transaction_count);
                    crate::print_warn!("  Run ext3recover to replay, or ext3clean to discard.");
                }
                true
            }
            Err(_) => false,
        }
    }).unwrap_or(false);

    if !journal_ok {
        crate::print_warn!("  warning: could not read journal superblock");
    }
}

pub fn cmd_ext3_ls(path: &str) {
    let path = if path.is_empty() { "/" } else { path };
    let result = with_ext2_pub(|fs| -> Result<([DirEntry; 64], usize), FsError> {
        let ino = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        if !inode.is_directory() { return Err(FsError::NotDirectory); }
        let mut entries = [const { DirEntry::empty() }; 64];
        let count = fs.read_dir(&inode, &mut entries)?;
        Ok((entries, count))
    });
    match result {
        Some(Ok((entries, count))) => {
            println!("  ext3:{} ({} entries)", path, count);
            for i in 0..count {
                let e = &entries[i];
                let name = e.name_str();
                match e.file_type {
                    FT_DIR     => cprintln!(0, 220, 220, "  d {}/", name),
                    FT_SYMLINK => cprintln!(128, 222, 217, "  l {}@", name),
                    _          => println!("  - {} (ino={})", name, e.inode),
                }
            }
        }
        Some(Err(e)) => print_error!("  ext3ls: {:?}", e),
        None => print_error!("  not mounted (run ext3mount first)"),
    }
}

pub fn cmd_ext3_cat(path: &str) {
    if path.is_empty() { println!("Usage: ext3cat <path>"); return; }
    let result = with_ext2_pub(|fs| -> Result<([u8; 512], usize, u64), FsError> {
        let ino = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        if inode.is_directory() { return Err(FsError::IsDirectory); }
        let size = inode.size();
        let read_size = (size as usize).min(512);
        let mut buf = [0u8; 512];
        let n = fs.read_file(&inode, 0, &mut buf[..read_size])?;
        Ok((buf, n, size))
    });
    match result {
        Some(Ok((buf, n, size))) => {
            if size > 512 { println!("  (showing first 512 of {} bytes)", size); }
            let s = core::str::from_utf8(&buf[..n]).unwrap_or("(binary)");
            println!("{}", s);
        }
        Some(Err(e)) => print_error!("  ext3cat: {:?}", e),
        None => print_error!("  not mounted (run ext3mount first)"),
    }
}

pub fn cmd_ext3_stat(path: &str) {
    if path.is_empty() { println!("Usage: ext3stat <path>"); return; }
    let result = with_ext2_pub(|fs| -> Result<(u32, Inode, bool), FsError> {
        let ino = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        let journal_active = fs.journal_active;
        Ok((ino, inode, journal_active))
    });
    match result {
        Some(Ok((ino, inode, journal_active))) => {
            println!("  Inode:   {}", ino);
            println!("  Type:    {:?}", inode.file_type());
            println!("  Mode:    0o{:o}", inode.permissions());
            println!("  Size:    {} bytes", inode.size());
            println!("  Links:   {}", inode.links_count());
            if inode.uses_extents() { println!("  Extents: yes"); }
            println!("  Journal: {}", if journal_active { "active" } else { "inactive" });
        }
        Some(Err(e)) => print_error!("  ext3stat: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext3_info() {
    let result = with_ext2_pub(|fs| fs.fs_info());
    match result {
        Some(info) => {
            cprintln!(57, 197, 187, "  ext3 Filesystem Info");
            println!("  Version:    {}", info.version);
            println!("  Block size: {} bytes", info.block_size);
            println!("  Blocks:     {} / {} used",
                info.total_blocks - info.free_blocks, info.total_blocks);
            println!("  Inodes:     {} / {} used",
                info.total_inodes - info.free_inodes, info.total_inodes);
            println!("  Groups:     {}", info.groups);
            println!("  Journal:    {}", if info.has_journal { "yes" } else { "NO (not ext3!)" });
        }
        None => print_error!("  not mounted (run ext3mount first)"),
    }
}

pub fn cmd_ext3_write(path: &str, text: &str) {
    if path.is_empty() || text.is_empty() { println!("Usage: ext3write <path> <text>"); return; }

    let disk_sw = crate::timing::Stopwatch::start();
    let result = with_ext2_pub(|fs| -> Result<u32, FsError> {
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
        Some(Ok(ino)) => print_success!("  [ext3] written to inode {} (journaled)  [disk {}ms]", ino, disk_ms),
        Some(Err(e))  => print_error!("  ext3write: {:?}", e),
        None          => print_error!("  not mounted"),
    }
    let render_us = render_sw.elapsed_us();
    crate::serial_println!("[timing] ext3write disk={}ms render={}us", disk_ms, render_us);
}

pub fn cmd_ext3_mkdir(path: &str) {
    if path.is_empty() { println!("Usage: ext3mkdir <path>"); return; }
    let result = with_ext2_pub(|fs| -> Result<u32, FsError> {
        let (parent_ino, dirname) = resolve_parent_and_name(fs, path)?;
        fs.ext3_create_dir(parent_ino, dirname, 0o755)
    });
    match result {
        Some(Ok(ino)) => print_success!("  [ext3] created dir inode {} (journaled)", ino),
        Some(Err(e)) => print_error!("  ext3mkdir: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext3_rm(path: &str) {
    if path.is_empty() { println!("Usage: ext3rm <path>"); return; }
    let result = with_ext2_pub(|fs| -> Result<(), FsError> {
        let (parent_ino, name) = resolve_parent_and_name(fs, path)?;
        fs.ext3_delete_file(parent_ino, name)
    });
    match result {
        Some(Ok(())) => print_success!("  [ext3] deleted (journaled)"),
        Some(Err(e)) => print_error!("  ext3rm: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext3_rmdir(path: &str) {
    if path.is_empty() { println!("Usage: ext3rmdir <path>"); return; }
    let result = with_ext2_pub(|fs| -> Result<(), FsError> {
        let (parent_ino, name) = resolve_parent_and_name(fs, path)?;
        fs.ext3_delete_dir(parent_ino, name)
    });
    match result {
        Some(Ok(())) => print_success!("  [ext3] removed dir (journaled)"),
        Some(Err(e)) => print_error!("  ext3rmdir: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext3_append(path: &str, text: &str) {
    if path.is_empty() || text.is_empty() { println!("Usage: ext3append <path> <text>"); return; }
    let result = with_ext2_pub(|fs| -> Result<usize, FsError> {
        let ino = fs.resolve_path(path)?;
        fs.ext3_append_file(ino, text.as_bytes())
    });
    match result {
        Some(Ok(n)) => print_success!("  [ext3] appended {} bytes (journaled)", n),
        Some(Err(e)) => print_error!("  ext3append: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext3_tree(path: &str) {
    let path = if path.is_empty() { "/" } else { path };
    let mut tree = TreeResult::new();
    let result = with_ext2_pub(|fs| -> Result<(), FsError> {
        let ino = fs.resolve_path(path)?;
        fs.ext2_tree(ino, "", &mut tree)
    });
    match result {
        Some(Ok(())) => {
            cprintln!(0, 220, 220, "  {}", path);
            for i in 0..tree.count {
                let e = &tree.entries[i];
                for _ in 0..e.depth as usize { cprint!(120, 140, 140, "    "); }
                if e.is_last { cprint!(120, 140, 140, "/ "); }
                else         { cprint!(120, 140, 140, "--- "); }
                if e.is_dir { cprintln!(0, 220, 220, "{}/", e.name_str()); }
                else        { cprintln!(230, 240, 240, "{} ({}b)", e.name_str(), e.size); }
            }
        }
        Some(Err(e)) => print_error!("  ext3tree: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext3_du(path: &str) {
    let path = if path.is_empty() { "/" } else { path };
    let result = with_ext2_pub(|fs| -> Result<(u32, u64), FsError> {
        let ino = fs.resolve_path(path)?;
        fs.ext2_dir_size(ino)
    });
    match result {
        Some(Ok((files, bytes))) => {
            println!("  {} files, {} bytes", files, bytes);
            if bytes >= 1024 { println!("  ({} KB)", bytes / 1024); }
        }
        Some(Err(e)) => print_error!("  ext3du: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext3_journal_info() {
    crate::commands::ext2_cmds::cmd_ext3_info();
}
pub fn cmd_ext3_recover() {
    crate::commands::ext2_cmds::cmd_ext3_recover();
}
pub fn cmd_ext3_clean() {
    crate::commands::ext2_cmds::cmd_ext3_clean();
}
