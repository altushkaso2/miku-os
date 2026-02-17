pub mod ext2_cmds;
pub mod fs;
pub mod system;

use crate::{println, serial_println};

pub fn execute(input: &str) {
    let t = input.trim();
    if t.is_empty() {
        return;
    }
    let mut parts = t.split_whitespace();
    let cmd = parts.next().unwrap_or("");
    let a1 = parts.next().unwrap_or("");
    let a2 = parts.next().unwrap_or("");
    let a3 = parts.next().unwrap_or("");
    let rest = if t.len() > cmd.len() {
        t[cmd.len()..].trim_start()
    } else {
        ""
    };

    match cmd {
        "ls" => fs::cmd_ls(if a1.is_empty() { "." } else { a1 }),
        "cd" => fs::cmd_cd(a1),
        "pwd" => fs::cmd_pwd(),
        "mkdir" => {
            if a1.is_empty() {
                println!("Usage: mkdir <name>");
            } else {
                fs::cmd_mkdir(a1);
            }
        }
        "touch" => {
            if a1.is_empty() {
                println!("Usage: touch <name>");
            } else {
                fs::cmd_touch(a1);
            }
        }
        "cat" => {
            if a1.is_empty() {
                println!("Usage: cat <file>");
            } else {
                fs::cmd_cat(a1);
            }
        }
        "write" => {
            if a1.is_empty() || rest.len() <= a1.len() {
                println!("Usage: write <file> <text>");
            } else {
                fs::cmd_write(a1, rest[a1.len()..].trim_start());
            }
        }
        "stat" => {
            if a1.is_empty() {
                println!("Usage: stat <path>");
            } else {
                fs::cmd_stat(a1);
            }
        }
        "rm" => {
            if a1.is_empty() {
                println!("Usage: rm [-rf] <path>");
            } else if a1 == "-rf" || a1 == "-r" || a1 == "-f" {
                if a2.is_empty() {
                    println!("Usage: rm -rf <path>");
                } else {
                    fs::cmd_rm_rf(a2);
                }
            } else {
                fs::cmd_rm(a1);
            }
        }
        "rmdir" => {
            if a1.is_empty() {
                println!("Usage: rmdir <dir>");
            } else {
                fs::cmd_rmdir(a1);
            }
        }
        "mv" => {
            if a1.is_empty() || a2.is_empty() {
                println!("Usage: mv <old> <new>");
            } else {
                fs::cmd_mv(a1, a2);
            }
        }
        "ln" => {
            if a1 == "-s" {
                if a2.is_empty() || a3.is_empty() {
                    println!("Usage: ln -s <target> <linkname>");
                } else {
                    fs::cmd_symlink(a2, a3);
                }
            } else {
                if a1.is_empty() || a2.is_empty() {
                    println!("Usage: ln <existing> <newname>");
                } else {
                    fs::cmd_link(a1, a2);
                }
            }
        }
        "readlink" => {
            if a1.is_empty() {
                println!("Usage: readlink <path>");
            } else {
                fs::cmd_readlink(a1);
            }
        }
        "chmod" => {
            if a1.is_empty() || a2.is_empty() {
                println!("Usage: chmod <mode> <path>");
            } else {
                fs::cmd_chmod(a1, a2);
            }
        }
        "df" => fs::cmd_df(),
        "mount" => {
            if a1.is_empty() {
                fs::cmd_mount_list();
            } else {
                fs::cmd_mount(a1, a2);
            }
        }
        "umount" => {
            if a1.is_empty() {
                println!("Usage: umount <path>");
            } else {
                fs::cmd_umount(a1);
            }
        }
        "echo" => system::cmd_echo(rest),
        "history" => system::cmd_history(),
        "info" => system::cmd_info(),
        "help" => system::cmd_help(),
        "clear" => system::cmd_clear(),
        "heap" => system::cmd_heap(),
        "poweroff" | "shutdown" | "halt" => system::cmd_poweroff(),
        "reboot" | "restart" => system::cmd_reboot(),

        "ext2mount" => ext2_cmds::cmd_ext2_mount(rest),
        "ext2ls" => ext2_cmds::cmd_ext2_ls(a1),
        "ext2cat" => ext2_cmds::cmd_ext2_cat(a1),
        "ext2stat" => ext2_cmds::cmd_ext2_stat(a1),
        "ext2info" => ext2_cmds::cmd_ext2_info(),
        "ext2write" => {
            if a1.is_empty() {
                println!("Usage: ext2write <path> <text>");
            } else {
                let text = if rest.len() > a1.len() {
                    rest[a1.len()..].trim_start()
                } else {
                    ""
                };
                ext2_cmds::cmd_ext2_write(a1, text);
            }
        }
        "ext2append" => {
            if a1.is_empty() {
                println!("Usage: ext2append <path> <text>");
            } else {
                let text = if rest.len() > a1.len() {
                    rest[a1.len()..].trim_start()
                } else {
                    ""
                };
                ext2_cmds::cmd_ext2_append(a1, text);
            }
        }
        "ext2mkdir" => {
            if a1.is_empty() {
                println!("Usage: ext2mkdir <path>");
            } else {
                ext2_cmds::cmd_ext2_mkdir(a1);
            }
        }
        "ext2rm" => {
            if a1.is_empty() {
                println!("Usage: ext2rm [-rf] <path>");
            } else if a1 == "-rf" || a1 == "-r" {
                if a2.is_empty() {
                    println!("Usage: ext2rm -rf <path>");
                } else {
                    ext2_cmds::cmd_ext2_rm_rf(a2);
                }
            } else {
                ext2_cmds::cmd_ext2_rm(a1);
            }
        }
        "ext2rmdir" => {
            if a1.is_empty() {
                println!("Usage: ext2rmdir <path>");
            } else {
                ext2_cmds::cmd_ext2_rmdir(a1);
            }
        }
        "ext2ln" => {
            if a1 == "-s" {
                if a2.is_empty() || a3.is_empty() {
                    println!("Usage: ext2ln -s <target> <linkname>");
                } else {
                    ext2_cmds::cmd_ext2_symlink(a2, a3);
                }
            } else {
                println!("Usage: ext2ln -s <target> <linkname>");
            }
        }
        "ext2link" => {
            if a1.is_empty() || a2.is_empty() {
                println!("Usage: ext2link <existing> <linkname>");
            } else {
                ext2_cmds::cmd_ext2_hardlink(a1, a2);
            }
        }
        "ext2mv" => {
            if a1.is_empty() || a2.is_empty() {
                println!("Usage: ext2mv <path> <newname>");
            } else {
                ext2_cmds::cmd_ext2_rename(a1, a2);
            }
        }
        "ext2chmod" => {
            if a1.is_empty() || a2.is_empty() {
                println!("Usage: ext2chmod <mode> <path>");
            } else {
                ext2_cmds::cmd_ext2_chmod(a1, a2);
            }
        }
        "ext2chown" => {
            if a1.is_empty() || a2.is_empty() || a3.is_empty() {
                println!("Usage: ext2chown <uid> <gid> <path>");
            } else {
                ext2_cmds::cmd_ext2_chown(a1, a2, a3);
            }
        }
        "ext2cp" => {
            if a1.is_empty() || a2.is_empty() {
                println!("Usage: ext2cp <src> <dst>");
            } else {
                ext2_cmds::cmd_ext2_cp(a1, a2);
            }
        }
        "ext2du" => ext2_cmds::cmd_ext2_du(a1),
        "ext2tree" => ext2_cmds::cmd_ext2_tree(a1),
        "ext2fsck" => ext2_cmds::cmd_ext2_fsck(),
        "ext2cache" => ext2_cmds::cmd_ext2_cache(),
        "ext2cacheflush" => ext2_cmds::cmd_ext2_cache_flush(),

        "ext3mkjournal" => ext2_cmds::cmd_ext3_mkjournal(),
        "ext3info" => ext2_cmds::cmd_ext3_info(),
        "ext3journal" => ext2_cmds::cmd_ext3_journal(),
        "ext3clean" => ext2_cmds::cmd_ext3_clean(),
        "ext3recover" => ext2_cmds::cmd_ext3_recover(),

        "ext4info" => ext2_cmds::cmd_ext4_info(),
        "ext4extents" => ext2_cmds::cmd_ext4_enable_extents(),
        "ext4checksums" => ext2_cmds::cmd_ext4_checksums(),
        "ext4extinfo" => {
            if a1.is_empty() {
                println!("Usage: ext4extinfo <path>");
            } else {
                ext2_cmds::cmd_ext4_extent_info(a1);
            }
        }

        _ => println!("Unknown: '{}'", cmd),
    }
}
