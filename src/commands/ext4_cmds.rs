use crate::miku_extfs::structs::*;
use crate::miku_extfs::FsError;
use crate::{cprintln, print_error, print_success, println};
use crate::commands::ext2_cmds::{with_ext2_pub, is_ext2_ready};
use crate::commands::ext_cmds_common as common;
use crate::commands::ext_cmds_common::resolve_parent_and_name;

#[inline(always)]
fn yn(b: bool) -> &'static str { if b { "yes" } else { "no" } }

pub fn cmd_ext4_mount(args: &str) {
    crate::commands::ext2_cmds::cmd_ext2_mount(args);
    if !is_ext2_ready() { return; }

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

        let has_sparse = sb.has_sparse_super();
        let has_large  = sb.has_large_file();
        let has_huge   = sb.has_huge_file();
        let has_nlink  = sb.feature_ro_compat() & FEATURE_RO_COMPAT_DIR_NLINK != 0;
        let has_eisize = sb.feature_ro_compat() & FEATURE_RO_COMPAT_EXTRA_ISIZE != 0;
        let has_csum   = sb.has_metadata_csum();
        println!("  ro_compat: sparse={} large_file={} huge={} dir_nlink={} extra_isize={} metadata_csum={}",
            yn(has_sparse), yn(has_large), yn(has_huge), yn(has_nlink), yn(has_eisize), yn(has_csum));

        println!("  inode size: {} bytes  rev_level: {}", fs.inode_size(), sb.rev_level());

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
                if info.clean { print_success!("  Journal: active, clean ({} blocks)", info.total_blocks); }
                else          { crate::print_warn!("  Journal: dirty - run ext3recover"); }
            }
        } else {
            crate::print_warn!("  Journal: none (run ext3mkjournal + remount for ext3/ext4)");
        }
    });
}

pub fn cmd_ext4_upgrade() {
    let result = with_ext2_pub(|fs| -> Result<crate::miku_extfs::ext4::upgrade::Ext4UpgradeReport, FsError> {
        fs.ext4_upgrade()
    });
    match result {
        None             => { print_error!("  not mounted (run ext2mount / ext4mount first)"); }
        Some(Err(e))     => { print_error!("  ext4upgrade: {:?}", e); }
        Some(Ok(rep))    => {
            if rep.already_ext4 && !rep.any_new() {
                print_success!("  filesystem is already fully ext4 - nothing changed.");
                return;
            }
            cprintln!(57, 197, 187, "  ext4 upgrade");
            if rep.set_rev_level    { print_success!("  rev_level bumped to 1 (EXT2_DYNAMIC_REV)"); }
            if rep.set_extents      { print_success!("  FEATURE_INCOMPAT_EXTENTS         enabled"); }
            if rep.set_filetype     { print_success!("  FEATURE_INCOMPAT_FILETYPE        enabled"); }
            if rep.set_sparse_super { print_success!("  FEATURE_RO_COMPAT_SPARSE_SUPER   enabled"); }
            if rep.set_large_file   { print_success!("  FEATURE_RO_COMPAT_LARGE_FILE     enabled"); }
            if rep.set_dir_nlink    { print_success!("  FEATURE_RO_COMPAT_DIR_NLINK      enabled"); }
            if rep.set_extra_isize  { print_success!("  FEATURE_RO_COMPAT_EXTRA_ISIZE    enabled"); }
            if rep.set_dir_index    { print_success!("  FEATURE_COMPAT_DIR_INDEX         enabled"); }
            if !rep.had_journal {
                crate::print_warn!("  note: no journal - run ext3mkjournal for full ext3/ext4 safety");
            }
            if rep.inode_size_warning {
                crate::print_warn!("  inode_size = {} bytes (< 256)", rep.inode_size);
                crate::print_warn!("  EXTRA_ISIZE requires 256-byte inodes (mkfs.ext4 -I 256)");
            }
            if rep.any_new() {
                print_success!("  Superblock written.  Remount with ext4mount to verify.");
            }
        }
    }
}

pub fn cmd_ext4_ls(path: &str)                  { common::impl_ls(path, "ext4"); }
pub fn cmd_ext4_cat(path: &str)                 { common::impl_cat(path, "ext4"); }
pub fn cmd_ext4_stat(path: &str)                { common::impl_stat(path, "ext4"); }
pub fn cmd_ext4_info()                          { common::impl_info("ext4"); }
pub fn cmd_ext4_write(path: &str, text: &str)   { common::impl_write(path, text, "ext4"); }
pub fn cmd_ext4_mkdir(path: &str)               { common::impl_mkdir(path, "ext4"); }
pub fn cmd_ext4_rm(path: &str)                  { common::impl_rm(path, "ext4"); }
pub fn cmd_ext4_rmdir(path: &str)               { common::impl_rmdir(path, "ext4"); }
pub fn cmd_ext4_append(path: &str, text: &str)  { common::impl_append(path, text, "ext4"); }
pub fn cmd_ext4_tree(path: &str)                { common::impl_tree(path, "ext4"); }
pub fn cmd_ext4_du(path: &str)                  { common::impl_du(path, "ext4"); }
pub fn cmd_ext4_cp(src: &str, dst: &str)        { common::impl_cp(src, dst, "ext4"); }

pub fn cmd_ext4_extinfo(path: &str)             { crate::commands::ext2_cmds::cmd_ext4_extent_info(path); }
pub fn cmd_ext4_enable_extents()                { crate::commands::ext2_cmds::cmd_ext4_enable_extents(); }
pub fn cmd_ext4_checksums()                     { crate::commands::ext2_cmds::cmd_ext4_checksums(); }

pub fn cmd_ext4_fsck() {
    let result = with_ext2_pub(|fs| fs.ext2_fsck());
    match result {
        Some(r) => {
            if !r.checked { print_error!("  fsck failed"); return; }
            cprintln!(57, 197, 187, "  ext4 filesystem check");
            println!("  Blocks: {} / {} free", r.free_blocks, r.total_blocks);
            println!("  Inodes: {} used / {} total", r.used_inodes, r.total_inodes);
            if r.errors == 0 { print_success!("  filesystem ok"); }
            else             { print_error!("  {} errors found", r.errors); }
        }
        None => print_error!("  not mounted"),
    }
}
