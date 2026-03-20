pub mod ext2_cmds;
pub mod ext_cmds_common;
pub mod ext3_cmds;
pub mod ext4_cmds;
pub mod fs;
pub mod system;
pub mod mkfs_cmds;
pub mod disk_cmds;

extern crate alloc;

use crate::{println, serial_println};

macro_rules! ext_dispatch {
    ($fn2:expr, $fn3:expr, $fn4:expr) => {
        match fs::ext_version() {
            "ext4" => $fn4,
            "ext3" => $fn3,
            _      => $fn2,
        }
    };
}

fn cmd_exec(path: &str, args: &[&str]) {
    match crate::exec_elf::exec(path, args) {
        Ok(pid) => {
            crate::scheduler::waitpid(pid);
            crate::user_stdin::clear_foreground();
        }
        Err(e) => {
            crate::print_error!("  exec: {}", e.as_str());
        }
    }
}

pub fn execute(input: &str) {
    let t = input.trim();
    if t.is_empty() { return; }

    let mut parts = t.split_whitespace();
    let cmd  = parts.next().unwrap_or("");
    let a1   = parts.next().unwrap_or("");
    let a2   = parts.next().unwrap_or("");
    let a3   = parts.next().unwrap_or("");
    let rest = if t.len() > cmd.len() { t[cmd.len()..].trim_start() } else { "" };

    match cmd {
        "ls"  => fs::cmd_ls(if a1.is_empty() { "." } else { a1 }),
        "cd"  => fs::cmd_cd(a1),
        "pwd" => fs::cmd_pwd(),

        "mkdir" => {
            if a1.is_empty() { println!("Usage: mkdir <name>"); }
            else { fs::cmd_mkdir(a1); }
        }
        "touch" => {
            if a1.is_empty() { println!("Usage: touch <name>"); }
            else { fs::cmd_touch(a1); }
        }
        "cat" => {
            if a1.is_empty() { println!("Usage: cat <file>"); }
            else { fs::cmd_cat(a1); }
        }
        "write" => {
            if a1.is_empty() || rest.len() <= a1.len() { println!("Usage: write <file> <text>"); }
            else { fs::cmd_write(a1, rest[a1.len()..].trim_start()); }
        }
        "stat" => {
            if a1.is_empty() { println!("Usage: stat <path>"); }
            else { fs::cmd_stat(a1); }
        }
        "rm" => {
            if a1.is_empty() { println!("Usage: rm [-rf] <path>"); }
            else if a1 == "-rf" || a1 == "-r" || a1 == "-f" {
                if a2.is_empty() { println!("Usage: rm -rf <path>"); }
                else { fs::cmd_rm_rf(a2); }
            } else {
                fs::cmd_rm(a1);
            }
        }
        "rmdir" => {
            if a1.is_empty() { println!("Usage: rmdir <dir>"); }
            else { fs::cmd_rmdir(a1); }
        }
        "mv" => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: mv <old> <new>"); }
            else { fs::cmd_mv(a1, a2); }
        }
        "ln" => {
            if a1 == "-s" {
                if a2.is_empty() || a3.is_empty() { println!("Usage: ln -s <target> <linkname>"); }
                else { fs::cmd_symlink(a2, a3); }
            } else {
                if a1.is_empty() || a2.is_empty() { println!("Usage: ln <existing> <newname>"); }
                else { fs::cmd_link(a1, a2); }
            }
        }
        "readlink" => {
            if a1.is_empty() { println!("Usage: readlink <path>"); }
            else { fs::cmd_readlink(a1); }
        }
        "chmod" => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: chmod <mode> <path>"); }
            else { fs::cmd_chmod(a1, a2); }
        }
        "df"     => fs::cmd_df(),
        "mount"  => { if a1.is_empty() { fs::cmd_mount_list(); } else { fs::cmd_mount(a1, a2); } }
        "umount" => { if a1.is_empty() { println!("Usage: umount <path>"); } else { fs::cmd_umount(a1); } }

        "ext2mount" => ext2_cmds::cmd_ext2_mount(rest),
        "ext3mount" => ext3_cmds::cmd_ext3_mount(rest),
        "ext4mount" => ext4_cmds::cmd_ext4_mount(rest),

        "fs.list"   => ext2_cmds::cmd_fs_list(),
        "fs.select" => ext2_cmds::cmd_fs_select(rest),
        "fs.umount" => ext2_cmds::cmd_fs_umount(rest),

        "extls" => fs::cmd_extls(a1),

        "extcat" => {
            if a1.is_empty() { println!("Usage: extcat <path>"); }
            else { ext_dispatch!(
                ext2_cmds::cmd_ext2_cat(a1),
                ext3_cmds::cmd_ext3_cat(a1),
                ext4_cmds::cmd_ext4_cat(a1)
            ); }
        }
        "extstat" => {
            if a1.is_empty() { println!("Usage: extstat <path>"); }
            else { ext_dispatch!(
                ext2_cmds::cmd_ext2_stat(a1),
                ext3_cmds::cmd_ext3_stat(a1),
                ext4_cmds::cmd_ext4_stat(a1)
            ); }
        }
        "extinfo" => {
            ext_dispatch!(
                ext2_cmds::cmd_ext2_info(),
                ext2_cmds::cmd_ext3_info(),
                ext2_cmds::cmd_ext4_info()
            );
        }
        "extwrite" => {
            if a1.is_empty() || rest.len() <= a1.len() { println!("Usage: extwrite <path> <text>"); }
            else {
                let text = rest[a1.len()..].trim_start();
                fs::cmd_extwrite(a1, text);
            }
        }
        "extappend" => {
            if a1.is_empty() || rest.len() <= a1.len() { println!("Usage: extappend <path> <text>"); }
            else {
                let text = rest[a1.len()..].trim_start();
                ext_dispatch!(
                    ext2_cmds::cmd_ext2_append(a1, text),
                    ext3_cmds::cmd_ext3_append(a1, text),
                    ext4_cmds::cmd_ext4_append(a1, text)
                );
            }
        }
        "exttouch" => {
            if a1.is_empty() { println!("Usage: exttouch <path>"); }
            else { fs::cmd_exttouch(a1); }
        }
        "extmkdir" => {
            if a1.is_empty() { println!("Usage: extmkdir <path>"); }
            else { fs::cmd_extmkdir(a1); }
        }
        "extrm" => {
            if a1.is_empty() { println!("Usage: extrm [-rf] <path>"); }
            else if a1 == "-rf" || a1 == "-r" {
                if a2.is_empty() { println!("Usage: extrm -rf <path>"); }
                else {
                    let abs = fs::make_abs_path_pub(a2);
                    let abs_str = unsafe { core::str::from_utf8_unchecked(&abs.0[..abs.1]) };
                    ext_dispatch!(
                        ext2_cmds::cmd_ext2_rm_rf(abs_str),
                        ext3_cmds::cmd_ext3_rm(abs_str),
                        ext4_cmds::cmd_ext4_rm(abs_str)
                    );
                }
            } else {
                let abs = fs::make_abs_path_pub(a1);
                let abs_str = unsafe { core::str::from_utf8_unchecked(&abs.0[..abs.1]) };
                ext_dispatch!(
                    ext2_cmds::cmd_ext2_rm(abs_str),
                    ext3_cmds::cmd_ext3_rm(abs_str),
                    ext4_cmds::cmd_ext4_rm(abs_str)
                );
            }
        }
        "extrmdir" => {
            if a1.is_empty() { println!("Usage: extrmdir <path>"); }
            else { fs::cmd_rmdir(a1); }
        }
        "extmv" => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: extmv <path> <newname>"); }
            else { fs::cmd_extmv(a1, a2); }
        }
        "extcp" => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: extcp <src> <dst>"); }
            else { fs::cmd_extcp(a1, a2); }
        }
        "extln" => {
            if a1 == "-s" {
                if a2.is_empty() || a3.is_empty() { println!("Usage: extln -s <target> <linkname>"); }
                else {
                    ext_dispatch!(
                        ext2_cmds::cmd_ext2_symlink(a2, a3),
                        ext2_cmds::cmd_ext2_symlink(a2, a3),
                        ext2_cmds::cmd_ext2_symlink(a2, a3)
                    );
                }
            } else {
                println!("Usage: extln -s <target> <linkname>");
            }
        }
        "extlink" => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: extlink <existing> <linkname>"); }
            else {
                ext_dispatch!(
                    ext2_cmds::cmd_ext2_hardlink(a1, a2),
                    ext2_cmds::cmd_ext2_hardlink(a1, a2),
                    ext2_cmds::cmd_ext2_hardlink(a1, a2)
                );
            }
        }
        "extchmod" => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: extchmod <mode> <path>"); }
            else { fs::cmd_extchmod(a1, a2); }
        }
        "extchown" => {
            if a1.is_empty() || a2.is_empty() || a3.is_empty() {
                println!("Usage: extchown <uid> <gid> <path>");
            } else { fs::cmd_extchown(a1, a2, a3); }
        }
        "extdu"   => { ext_dispatch!(
            ext2_cmds::cmd_ext2_du(a1),
            ext3_cmds::cmd_ext3_du(a1),
            ext4_cmds::cmd_ext4_du(a1)
        ); }
        "exttree" => { ext_dispatch!(
            ext2_cmds::cmd_ext2_tree(a1),
            ext3_cmds::cmd_ext3_tree(a1),
            ext4_cmds::cmd_ext4_tree(a1)
        ); }
        "extfsck" => { ext_dispatch!(
            ext2_cmds::cmd_ext2_fsck(),
            ext2_cmds::cmd_ext2_fsck(),
            ext4_cmds::cmd_ext4_fsck()
        ); }
        "extcache"      => ext2_cmds::cmd_ext2_cache(),
        "extcacheflush" => ext2_cmds::cmd_ext2_cache_flush(),
        "extsync" | "sync" => ext4_cmds::cmd_ext4_sync(),

        "ext3mkjournal" => ext2_cmds::cmd_ext3_mkjournal(),
        "ext3journal"   => ext2_cmds::cmd_ext3_journal(),
        "ext3recover"   => ext2_cmds::cmd_ext3_recover(),
        "ext3clean"     => ext2_cmds::cmd_ext3_clean(),

        "ext4extents"   => ext2_cmds::cmd_ext4_enable_extents(),
        "ext4checksums" => ext2_cmds::cmd_ext4_checksums(),
        "ext4extinfo"   => {
            if a1.is_empty() { println!("Usage: ext4extinfo <path>"); }
            else { ext2_cmds::cmd_ext4_extent_info(a1); }
        }

        "ext2ls"     => ext2_cmds::cmd_ext2_ls(a1),
        "ext2cat"    => ext2_cmds::cmd_ext2_cat(a1),
        "ext2stat"   => ext2_cmds::cmd_ext2_stat(a1),
        "ext2info"   => ext2_cmds::cmd_ext2_info(),
        "ext2write"  => {
            let text = if rest.len() > a1.len() { rest[a1.len()..].trim_start() } else { "" };
            if a1.is_empty() { println!("Usage: ext2write <path> <text>"); }
            else { ext2_cmds::cmd_ext2_write(a1, text); }
        }
        "ext2append" => {
            let text = if rest.len() > a1.len() { rest[a1.len()..].trim_start() } else { "" };
            if a1.is_empty() { println!("Usage: ext2append <path> <text>"); }
            else { ext2_cmds::cmd_ext2_append(a1, text); }
        }
        "ext2touch"  => { if a1.is_empty() { println!("Usage: ext2touch <path>"); } else { fs::cmd_exttouch(a1); } }
        "ext2mkdir"  => { if a1.is_empty() { println!("Usage: ext2mkdir <path>"); } else { ext2_cmds::cmd_ext2_mkdir(a1); } }
        "ext2rm"     => {
            if a1.is_empty() { println!("Usage: ext2rm [-rf] <path>"); }
            else if a1 == "-rf" || a1 == "-r" {
                if a2.is_empty() { println!("Usage: ext2rm -rf <path>"); }
                else { ext2_cmds::cmd_ext2_rm_rf(a2); }
            } else { ext2_cmds::cmd_ext2_rm(a1); }
        }
        "ext2rmdir"  => { if a1.is_empty() { println!("Usage: ext2rmdir <path>"); } else { ext2_cmds::cmd_ext2_rmdir(a1); } }
        "ext2mv"     => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: ext2mv <path> <newname>"); }
            else { ext2_cmds::cmd_ext2_rename(a1, a2); }
        }
        "ext2cp"     => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: ext2cp <src> <dst>"); }
            else { ext2_cmds::cmd_ext2_cp(a1, a2); }
        }
        "ext2ln"     => {
            if a1 == "-s" {
                if a2.is_empty() || a3.is_empty() { println!("Usage: ext2ln -s <target> <linkname>"); }
                else { ext2_cmds::cmd_ext2_symlink(a2, a3); }
            } else { println!("Usage: ext2ln -s <target> <linkname>"); }
        }
        "ext2link"   => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: ext2link <existing> <linkname>"); }
            else { ext2_cmds::cmd_ext2_hardlink(a1, a2); }
        }
        "ext2chmod"  => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: ext2chmod <mode> <path>"); }
            else { ext2_cmds::cmd_ext2_chmod(a1, a2); }
        }
        "ext2chown"  => {
            if a1.is_empty() || a2.is_empty() || a3.is_empty() { println!("Usage: ext2chown <uid> <gid> <path>"); }
            else { ext2_cmds::cmd_ext2_chown(a1, a2, a3); }
        }
        "ext2du"        => ext2_cmds::cmd_ext2_du(a1),
        "ext2tree"      => ext2_cmds::cmd_ext2_tree(a1),
        "ext2fsck"      => ext2_cmds::cmd_ext2_fsck(),
        "ext2cache"     => ext2_cmds::cmd_ext2_cache(),
        "ext2cacheflush"=> ext2_cmds::cmd_ext2_cache_flush(),

        "ext3ls"     => ext3_cmds::cmd_ext3_ls(a1),
        "ext3cat"    => ext3_cmds::cmd_ext3_cat(a1),
        "ext3stat"   => ext3_cmds::cmd_ext3_stat(a1),
        "ext3info"   => ext2_cmds::cmd_ext3_info(),
        "ext3write"  => {
            let text = if rest.len() > a1.len() { rest[a1.len()..].trim_start() } else { "" };
            if a1.is_empty() { println!("Usage: ext3write <path> <text>"); }
            else { ext3_cmds::cmd_ext3_write(a1, text); }
        }
        "ext3append" => {
            let text = if rest.len() > a1.len() { rest[a1.len()..].trim_start() } else { "" };
            if a1.is_empty() { println!("Usage: ext3append <path> <text>"); }
            else { ext3_cmds::cmd_ext3_append(a1, text); }
        }
        "ext3mkdir"  => { if a1.is_empty() { println!("Usage: ext3mkdir <path>"); } else { ext3_cmds::cmd_ext3_mkdir(a1); } }
        "ext3rm"     => { if a1.is_empty() { println!("Usage: ext3rm <path>"); } else { ext3_cmds::cmd_ext3_rm(a1); } }
        "ext3rmdir"  => { if a1.is_empty() { println!("Usage: ext3rmdir <path>"); } else { ext3_cmds::cmd_ext3_rmdir(a1); } }
        "ext3tree"   => ext3_cmds::cmd_ext3_tree(a1),
        "ext3du"     => ext3_cmds::cmd_ext3_du(a1),

        "ext4ls"     => ext4_cmds::cmd_ext4_ls(a1),
        "ext4cat"    => ext4_cmds::cmd_ext4_cat(a1),
        "ext4stat"   => ext4_cmds::cmd_ext4_stat(a1),
        "ext4info"   => ext2_cmds::cmd_ext4_info(),
        "ext4sync"   => ext4_cmds::cmd_ext4_sync(),
        "ext4write"  => {
            let text = if rest.len() > a1.len() { rest[a1.len()..].trim_start() } else { "" };
            if a1.is_empty() { println!("Usage: ext4write <path> <text>"); }
            else { ext4_cmds::cmd_ext4_write(a1, text); }
        }
        "ext4append" => {
            let text = if rest.len() > a1.len() { rest[a1.len()..].trim_start() } else { "" };
            if a1.is_empty() { println!("Usage: ext4append <path> <text>"); }
            else { ext4_cmds::cmd_ext4_append(a1, text); }
        }
        "ext4mkdir"  => { if a1.is_empty() { println!("Usage: ext4mkdir <path>"); } else { ext4_cmds::cmd_ext4_mkdir(a1); } }
        "ext4rm"     => { if a1.is_empty() { println!("Usage: ext4rm <path>"); } else { ext4_cmds::cmd_ext4_rm(a1); } }
        "ext4rmdir"  => { if a1.is_empty() { println!("Usage: ext4rmdir <path>"); } else { ext4_cmds::cmd_ext4_rmdir(a1); } }
        "ext4cp"     => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: ext4cp <src> <dst>"); }
            else { ext4_cmds::cmd_ext4_cp(a1, a2); }
        }
        "ext4tree"   => ext4_cmds::cmd_ext4_tree(a1),
        "ext4du"     => ext4_cmds::cmd_ext4_du(a1),
        "ext4fsck"   => ext4_cmds::cmd_ext4_fsck(),

        "mkfs.ext2" => {
            if a1.is_empty() { println!("Usage: mkfs.ext2 <drive 0-3>"); }
            else { mkfs_cmds::cmd_mkfs_ext2(rest); }
        }
        "mkfs.ext3" => {
            if a1.is_empty() { println!("Usage: mkfs.ext3 <drive 0-3>"); }
            else { mkfs_cmds::cmd_mkfs_ext3(rest); }
        }
        "mkfs.ext4" => {
            if a1.is_empty() { println!("Usage: mkfs.ext4 <drive 0-3>"); }
            else { mkfs_cmds::cmd_mkfs_ext4(rest); }
        }
        "mkfs.dry"  => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: mkfs.dry <drive 0-3> <ext2|ext3|ext4>"); }
            else { mkfs_cmds::cmd_mkfs_dry(a1, a2); }
        }

        "gpt"      => disk_cmds::cmd_gpt_show(a1),
        "gpt.init" => disk_cmds::cmd_gpt_init(a1),
        "gpt.add"  => disk_cmds::cmd_gpt_add(rest),
        "gpt.del"  => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: gpt.del <drive> <partition>"); }
            else { disk_cmds::cmd_gpt_del(a1, a2); }
        }
        "mkswap"   => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: mkswap <drive> <partition>"); }
            else { disk_cmds::cmd_mkswap(a1, a2); }
        }
        "swapon"   => {
            if a1.is_empty() || a2.is_empty() { println!("Usage: swapon <drive> <partition>"); }
            else { disk_cmds::cmd_swapon(a1, a2); }
        }
        "swapoff"    => disk_cmds::cmd_swapoff(),
        "swapinfo"   => disk_cmds::cmd_swapinfo(),
        "swapon.raw" => disk_cmds::cmd_swapon_raw(rest),
        "swapon.auto"=> disk_cmds::cmd_swapon_auto(),
        "mkswap.raw" => disk_cmds::cmd_mkswap_raw(rest),

        "exec" => {
            if a1.is_empty() {
                println!("Usage: exec <path> [args...]");
            } else {
                let argv: alloc::vec::Vec<&str> =
                    core::iter::once(a1)
                    .chain(rest[a1.len()..].split_whitespace())
                    .collect();
                cmd_exec(a1, &argv);
            }
        }

        "echo"     => system::cmd_echo(rest),
        "history"  => system::cmd_history(),
        "info"     => system::cmd_info(),
        "memmap"   => system::cmd_memmap(),
        "help"     => system::cmd_help(),
        "clear"    => system::cmd_clear(),
        "heap"     => system::cmd_heap(),
        "poweroff" | "shutdown" | "halt" => system::cmd_poweroff(),
        "reboot"   | "restart"           => system::cmd_reboot(),
        "ps"       => system::cmd_ps(),
        "ldconfig" => system::cmd_ldconfig(rest),
        "ldd"      => system::cmd_ldd(a1),
        "top"      => system::cmd_top(),
        "swaptest" => system::cmd_swaptest(),
        "nice"     => system::cmd_nice(a1, a2),
        "affinity" => system::cmd_affinity(a1, a2),
        "kill"     => {
            if a1.is_empty() { println!("Usage: kill <pid>"); }
            else if let Ok(pid) = a1.parse::<u64>() {
                crate::scheduler::kill(pid);
                crate::cprintln!(100, 220, 150, "  killed pid={}", pid);
            } else {
                crate::print_error!("  invalid pid");
            }
        }

        "net"  => { crate::net::poll(); crate::net::cmd_net(rest); }
        "dhcp" => crate::net::cmd_dhcp(),
        "ping" => {
            if a1.is_empty() { println!("Usage: ping <ip|host> [count]"); }
            else {
                let count = a2.parse::<usize>().unwrap_or(usize::MAX);
                match parse_ip(a1) {
                    Some(ip) => crate::net::cmd_ping(a1, &ip, count),
                    None => {
                        crate::cprintln!(57, 197, 187, "ping: resolving {}...", a1);
                        let dns = crate::net::get_dns();
                        match crate::net::dns::resolve(a1, &dns) {
                            Some(ip) => crate::net::cmd_ping(a1, &ip, count),
                            None => crate::print_error!("ping: cannot resolve '{}'", a1),
                        }
                    }
                }
            }
        }
        "fetch" => {
            if a1.is_empty() { println!("Usage: fetch <url|host> [port]"); }
            else { cmd_fetch(a1, a2); }
        }
        "wget"  => {
            if a1.is_empty() { println!("Usage: wget <url> [-O <file>]"); }
            else {
                x86_64::instructions::interrupts::enable();
                crate::net::http::cmd_wget(rest);
            }
        }
        "curl"  => {
            if a1.is_empty() { println!("Usage: curl <url> [-X GET|POST] [-d <data>] [-o <file>] [-I]"); }
            else {
                x86_64::instructions::interrupts::enable();
                crate::net::http::cmd_curl(rest);
            }
        }
        "ntp"   => {
            x86_64::instructions::interrupts::enable();
            crate::net::ntp::cmd_ntp(a1);
        }
        "traceroute" | "tr" => {
            if a1.is_empty() { println!("Usage: traceroute <host|ip>"); }
            else {
                x86_64::instructions::interrupts::enable();
                crate::net::traceroute::cmd_traceroute(a1);
            }
        }

        _ => println!("Unknown: '{}'", cmd),
    }
}

fn cmd_fetch(host_or_url: &str, port_str: &str) {
    if host_or_url.starts_with("http://") || host_or_url.starts_with("https://") {
        x86_64::instructions::interrupts::enable();
        if let Some(resp) = crate::net::http::get(host_or_url) {
            let sc = if resp.status < 400 { (100u8, 220u8, 150u8) } else { (255u8, 80u8, 80u8) };
            crate::cprintln!(sc.0, sc.1, sc.2, "HTTP {} {}", resp.status, resp.reason);
            if resp.body.is_empty() {
                crate::print_warn!("fetch: empty body");
            } else {
                crate::cprintln!(120, 200, 200, "fetch: {} bytes", resp.body.len());
                print_response(&resp.body);
            }
        }
        return;
    }
    let (host, port, use_tls) = {
        let p: u16 = port_str.parse().unwrap_or(80);
        (host_or_url, p, p == 443)
    };
    let dns = crate::net::get_dns();
    let ip  = match parse_ip(host) {
        Some(ip) => ip,
        None => {
            crate::cprintln!(57, 197, 187, "fetch: resolving {}...", host);
            match crate::net::dns::resolve(host, &dns) {
                Some(ip) => ip,
                None => { crate::print_error!("fetch: cannot resolve '{}'", host); return; }
            }
        }
    };
    crate::cprintln!(57, 197, 187,
        "fetch: connecting to {}.{}.{}.{}:{} ({})...",
        ip[0], ip[1], ip[2], ip[3], port,
        if use_tls { "TLS" } else { "plain" }
    );
    x86_64::instructions::interrupts::enable();
    let req_str  = alloc::format!(
        "GET / HTTP/1.1\r\nHost: {}\r\nUser-Agent: MikuOS/0.1\r\nConnection: close\r\n\r\n",
        host
    );
    let req_bytes = req_str.as_bytes();
    if use_tls {
        crate::cprintln!(120, 200, 200, "fetch: TLS handshake...");
        let mut stream = match crate::net::tls::TlsStream::connect(host, ip, port) {
            Some(s) => s,
            None    => { crate::print_error!("fetch: TLS failed"); return; }
        };
        crate::print_success!("fetch: TLS ok ({})", stream.cipher_name());
        if !stream.send(req_bytes) { crate::print_error!("fetch: send failed"); stream.close(); return; }
        crate::cprintln!(120, 200, 200, "fetch: waiting...");
        let data = stream.recv_all(8_000_000);
        print_response(data);
        stream.close();
    } else {
        let mut sock = match crate::net::tcp::TcpSocket::connect(ip, port) {
            Some(s) => s,
            None    => { crate::print_error!("fetch: connection failed"); return; }
        };
        crate::print_success!("fetch: connected");
        if !sock.send(req_bytes) { crate::print_error!("fetch: send failed"); sock.close(); return; }
        crate::cprintln!(120, 200, 200, "fetch: waiting...");
        let data = sock.recv_all(8_000_000);
        print_response(data);
        sock.close();
    }
}

fn print_response(data: &[u8]) {
    if data.is_empty() { crate::print_warn!("fetch: no data received"); return; }
    let show = data.len().min(4096);
    let mut text = alloc::string::String::with_capacity(show);
    for &b in &data[..show] {
        if b == b'\n' || b == b'\r' || b == b'\t' || (b >= 32 && b <= 126) {
            text.push(b as char);
        } else {
            text.push('.');
        }
    }
    crate::println!("{}", text);
    if data.len() > show {
        crate::cprintln!(120, 140, 140, "... ({} bytes total, showing first 4096)", data.len());
    }
}

fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let mut p = s.split('.');
    Some([p.next()?.parse().ok()?, p.next()?.parse().ok()?,
          p.next()?.parse().ok()?, p.next()?.parse().ok()?])
}
