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
            let name   = &trimmed[pos + 1..];
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

pub fn cmd_ext4_mount(_args: &str) {
    crate::commands::ext2_cmds::cmd_ext2_mount(_args);

    if !is_ext2_ready() {
        return;
    }

    let _ = with_ext2_pub(|fs| {
        let sb = &fs.superblock;

        cprintln!(57, 197, 187, "  ext4 Feature Report");

        let has_journal   = sb.has_journal();
        let has_dir_index = sb.has_dir_index();
        let has_ext_attr  = sb.has_ext_attr();
        println!("  compat:    journal={} dir_index={} ext_attr={}",
            yn(has_journal), yn(has_dir_index), yn(has_ext_attr));

        let has_extents  = sb.has_extents();
        let has_filetype = sb.has_filetype();
        let has_64bit    = sb.has_64bit();
        let has_flex_bg  = sb.has_flex_bg();
        println!("  incompat:  extents={} filetype={} 64bit={} flex_bg={}",
            yn(has_extents), yn(has_filetype), yn(has_64bit), yn(has_flex_bg));

        let has_sparse  = sb.has_sparse_super();
        let has_large   = sb.has_large_file();
        let has_huge    = sb.has_huge_file();
        let has_nlink   = sb.feature_ro_compat() & FEATURE_RO_COMPAT_DIR_NLINK != 0;
        let has_eisize  = sb.feature_ro_compat() & FEATURE_RO_COMPAT_EXTRA_ISIZE != 0;
        let has_csum    = sb.has_metadata_csum();
        println!("  ro_compat: sparse={} large_file={} huge={} dir_nlink={} extra_isize={} metadata_csum={}",
            yn(has_sparse), yn(has_large), yn(has_huge), yn(has_nlink), yn(has_eisize), yn(has_csum));

        println!("  inode size: {} bytes  rev_level: {}",
            fs.inode_size(), sb.rev_level());

        if fs.ext4_features_complete() {
            print_success!("  All mandatory ext4 features present.");
        } else {
            let (mi, mr) = fs.ext4_missing_features();
            crate::print_warn!("  warning: this is NOT a complete ext4 filesystem.");
            if mi & FEATURE_INCOMPAT_EXTENTS  != 0 { crate::print_warn!("    missing: INCOMPAT_EXTENTS"); }
            if mi & FEATURE_INCOMPAT_FILETYPE != 0 { crate::print_warn!("    missing: INCOMPAT_FILETYPE"); }
            if mr & FEATURE_RO_COMPAT_SPARSE_SUPER != 0 { crate::print_warn!("    missing: RO_SPARSE_SUPER"); }
            if mr & FEATURE_RO_COMPAT_LARGE_FILE   != 0 { crate::print_warn!("    missing: RO_LARGE_FILE"); }
            if mr & FEATURE_RO_COMPAT_DIR_NLINK    != 0 { crate::print_warn!("    missing: RO_DIR_NLINK"); }
            crate::print_warn!("  Run 'ext4upgrade' to fix, then remount.");
        }

        if has_journal {
            if let Ok(info) = fs.scan_journal() {
                if info.clean {
                    print_success!("  Journal: active, clean ({} blocks)", info.total_blocks);
                } else {
                    crate::print_warn!("  Journal: dirty - run ext3recover");
                }
            }
        } else {
            crate::print_warn!("  Journal: none (run ext3mkjournal + remount for ext3/ext4)");
        }
    });
}

#[inline(always)]
fn yn(b: bool) -> &'static str { if b { "yes" } else { "no" } }

pub fn cmd_ext4_upgrade() {
    let result = with_ext2_pub(|fs| -> Result<crate::miku_extfs::ext4::upgrade::Ext4UpgradeReport, FsError> {
        fs.ext4_upgrade()
    });
    match result {
        None => {
            print_error!("  not mounted (run ext2mount / ext4mount first)");
        }
        Some(Err(e)) => {
            print_error!("  ext4upgrade: {:?}", e);
        }
        Some(Ok(rep)) => {
            if rep.already_ext4 && !rep.any_new() {
                print_success!("  filesystem is already fully ext4 - nothing changed.");
                return;
            }

            cprintln!(57, 197, 187, "  ext4 upgrade");

            if rep.set_rev_level {
                print_success!("  rev_level bumped to 1 (EXT2_DYNAMIC_REV)");
            }
            if rep.set_extents {
                print_success!("  FEATURE_INCOMPAT_EXTENTS         enabled");
            }
            if rep.set_filetype {
                print_success!("  FEATURE_INCOMPAT_FILETYPE        enabled");
            }
            if rep.set_sparse_super {
                print_success!("  FEATURE_RO_COMPAT_SPARSE_SUPER   enabled");
            }
            if rep.set_large_file {
                print_success!("  FEATURE_RO_COMPAT_LARGE_FILE     enabled");
            }
            if rep.set_dir_nlink {
                print_success!("  FEATURE_RO_COMPAT_DIR_NLINK      enabled");
            }
            if rep.set_extra_isize {
                print_success!("  FEATURE_RO_COMPAT_EXTRA_ISIZE    enabled (inode extra fields active)");
            }
            if rep.set_dir_index {
                print_success!("  FEATURE_COMPAT_DIR_INDEX         enabled (HTree directories)");
            }

            if !rep.had_journal {
                crate::print_warn!("  note: no journal - run ext3mkjournal for full ext3/ext4 safety");
            }

            if rep.inode_size_warning {
                crate::print_warn!("  inode_size = {} bytes (< 256)", rep.inode_size);
                crate::print_warn!("  EXTRA_ISIZE not set: nanosecond timestamps, i_version, etc.");
                crate::print_warn!("  are only available with 256-byte inodes.");
                crate::print_warn!("  To get 256-byte inodes: mkfs.ext4 with -I 256 (requires mkfs).");
            }

            if rep.any_new() {
                print_success!("  Superblock written.  Remount with ext4mount to verify.");
            }
        }
    }
}

pub fn cmd_ext4_ls(path: &str) {
    let path = if path.is_empty() { "/" } else { path };
    let result = with_ext2_pub(|fs| -> Result<(alloc::vec::Vec<DirEntry>, usize), FsError> {
        let ino = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        if !inode.is_directory() { return Err(FsError::NotDirectory); }
        let mut entries = alloc::vec![DirEntry::empty(); 64];
        let count = fs.read_dir(&inode, &mut entries)?;
        Ok((entries, count))
    });
    match result {
        Some(Ok((entries, count))) => {
            println!("  ext4:{} ({} entries)", path, count);
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
        Some(Err(e)) => print_error!("  ext4ls: {:?}", e),
        None => print_error!("  not mounted (run ext4mount first)"),
    }
}

pub fn cmd_ext4_cat(path: &str) {
    if path.is_empty() { println!("Usage: ext4cat <path>"); return; }
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
        Some(Err(e)) => print_error!("  ext4cat: {:?}", e),
        None => print_error!("  not mounted (run ext4mount first)"),
    }
}

pub fn cmd_ext4_stat(path: &str) {
    if path.is_empty() { println!("Usage: ext4stat <path>"); return; }
    let result = with_ext2_pub(|fs| -> Result<(u32, Inode), FsError> {
        let ino = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        Ok((ino, inode))
    });
    match result {
        Some(Ok((ino, inode))) => {
            println!("  Inode:   {}", ino);
            println!("  Type:    {:?}", inode.file_type());
            println!("  Mode:    0o{:o}", inode.permissions());
            println!("  Size:    {} bytes", inode.size());
            println!("  Links:   {}", inode.links_count());
            println!("  Extents: {}", if inode.uses_extents() { "yes" } else { "no" });
            println!("  Inline:  {}", if inode.has_inline_data() { "yes" } else { "no" });
        }
        Some(Err(e)) => print_error!("  ext4stat: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext4_info() {
    crate::commands::ext2_cmds::cmd_ext4_info();
}

pub fn cmd_ext4_write(path: &str, text: &str) {
    if path.is_empty() || text.is_empty() { println!("Usage: ext4write <path> <text>"); return; }

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
        Some(Ok(ino)) => print_success!("  [ext4] written to inode {} (extents+journal)  [disk {}ms]", ino, disk_ms),
        Some(Err(e))  => print_error!("  ext4write: {:?}", e),
        None          => print_error!("  not mounted"),
    }
    let render_us = render_sw.elapsed_us();
    crate::serial_println!("[timing] ext4write disk={}ms render={}us", disk_ms, render_us);
}

pub fn cmd_ext4_mkdir(path: &str) {
    if path.is_empty() { println!("Usage: ext4mkdir <path>"); return; }
    let result = with_ext2_pub(|fs| -> Result<u32, FsError> {
        let (parent_ino, dirname) = resolve_parent_and_name(fs, path)?;
        fs.ext3_create_dir(parent_ino, dirname, 0o755)
    });
    match result {
        Some(Ok(ino)) => print_success!("  [ext4] created dir inode {}", ino),
        Some(Err(e)) => print_error!("  ext4mkdir: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext4_rm(path: &str) {
    if path.is_empty() { println!("Usage: ext4rm <path>"); return; }
    let result = with_ext2_pub(|fs| -> Result<(), FsError> {
        let (parent_ino, name) = resolve_parent_and_name(fs, path)?;
        fs.ext3_delete_file(parent_ino, name)
    });
    match result {
        Some(Ok(())) => print_success!("  [ext4] deleted"),
        Some(Err(e)) => print_error!("  ext4rm: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext4_rmdir(path: &str) {
    if path.is_empty() { println!("Usage: ext4rmdir <path>"); return; }
    let result = with_ext2_pub(|fs| -> Result<(), FsError> {
        let (parent_ino, name) = resolve_parent_and_name(fs, path)?;
        fs.ext3_delete_dir(parent_ino, name)
    });
    match result {
        Some(Ok(())) => print_success!("  [ext4] removed dir"),
        Some(Err(e)) => print_error!("  ext4rmdir: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext4_cp(src: &str, dst: &str) {
    if src.is_empty() || dst.is_empty() { println!("Usage: ext4cp <src> <dst>"); return; }
    let result = with_ext2_pub(|fs| -> Result<u32, FsError> {
        let src_ino = fs.resolve_path(src)?;
        let (dst_parent_ino, dst_name) = resolve_parent_and_name(fs, dst)?;
        fs.ext4_copy_file(src_ino, dst_parent_ino, dst_name)
    });
    match result {
        Some(Ok(ino)) => print_success!("  [ext4] copied to inode {}", ino),
        Some(Err(e)) => print_error!("  ext4cp: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext4_append(path: &str, text: &str) {
    if path.is_empty() || text.is_empty() { println!("Usage: ext4append <path> <text>"); return; }
    let result = with_ext2_pub(|fs| -> Result<usize, FsError> {
        let ino = fs.resolve_path(path)?;
        fs.ext3_append_file(ino, text.as_bytes())
    });
    match result {
        Some(Ok(n)) => print_success!("  [ext4] appended {} bytes", n),
        Some(Err(e)) => print_error!("  ext4append: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext4_tree(path: &str) {
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
                if e.is_last { cprint!(120, 140, 140, "/ "); } else { cprint!(120, 140, 140, "--- "); }
                if e.is_dir { cprintln!(0, 220, 220, "{}/", e.name_str()); }
                else        { cprintln!(230, 240, 240, "{} ({}b)", e.name_str(), e.size); }
            }
        }
        Some(Err(e)) => print_error!("  ext4tree: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext4_du(path: &str) {
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
        Some(Err(e)) => print_error!("  ext4du: {:?}", e),
        None => print_error!("  not mounted"),
    }
}

pub fn cmd_ext4_extinfo(path: &str) {
    crate::commands::ext2_cmds::cmd_ext4_extent_info(path);
}
pub fn cmd_ext4_enable_extents() {
    crate::commands::ext2_cmds::cmd_ext4_enable_extents();
}
pub fn cmd_ext4_checksums() {
    crate::commands::ext2_cmds::cmd_ext4_checksums();
}
pub fn cmd_ext4_fsck() {
    let result = with_ext2_pub(|fs| fs.ext2_fsck());
    match result {
        Some(r) => {
            if !r.checked { print_error!("  fsck failed"); return; }
            cprintln!(57, 197, 187, "  ext4 filesystem check");
            println!("  Blocks: {} / {} free", r.free_blocks, r.total_blocks);
            println!("  Inodes: {} used / {} total", r.used_inodes, r.total_inodes);
            if r.errors == 0 { print_success!("  filesystem ok"); }
            else { print_error!("  {} errors found", r.errors); }
        }
        None => print_error!("  not mounted"),
    }
}
