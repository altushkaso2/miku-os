use crate::commands::ext_cmds_common as common;

pub fn cmd_ext3_mount(args: &str) {
    crate::commands::ext2_cmds::cmd_ext2_mount(args);
}

pub fn cmd_ext3_ls(path: &str)                  { common::impl_ls(path, "ext3"); }
pub fn cmd_ext3_cat(path: &str)                 { common::impl_cat(path, "ext3"); }
pub fn cmd_ext3_stat(path: &str)                { common::impl_stat(path, "ext3"); }
pub fn cmd_ext3_info()                          { common::impl_info("ext3"); }
pub fn cmd_ext3_write(path: &str, text: &str)   { common::impl_write(path, text, "ext3"); }
pub fn cmd_ext3_mkdir(path: &str)               { common::impl_mkdir(path, "ext3"); }
pub fn cmd_ext3_rm(path: &str)                  { common::impl_rm(path, "ext3"); }
pub fn cmd_ext3_rmdir(path: &str)               { common::impl_rmdir(path, "ext3"); }
pub fn cmd_ext3_append(path: &str, text: &str)  { common::impl_append(path, text, "ext3"); }
pub fn cmd_ext3_tree(path: &str)                { common::impl_tree(path, "ext3"); }
pub fn cmd_ext3_du(path: &str)                  { common::impl_du(path, "ext3"); }

pub fn cmd_ext3_journal_info()                  { crate::commands::ext2_cmds::cmd_ext3_info(); }
pub fn cmd_ext3_recover()                       { crate::commands::ext2_cmds::cmd_ext3_recover(); }
pub fn cmd_ext3_clean()                         { crate::commands::ext2_cmds::cmd_ext3_clean(); }
