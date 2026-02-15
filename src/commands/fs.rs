use crate::vfs::{self, FileMode, OpenFlags, VfsError, VNodeKind, MAX_CHILDREN, MAX_VNODES, with_vfs, with_vfs_ro};
use crate::shell::SESSION;
use crate::console;
use crate::{print, println, cprint, cprintln, print_error, print_success, serial_println};

pub fn cmd_ls(path: &str) {
    serial_println!("[ls] path={}", path);
    let cwd = SESSION.lock().cwd;

    let mut names: [[u8; 23]; 16] = [[0; 23]; 16];
    let mut nlens: [u8; 16] = [0; 16];
    let mut kinds: [u8; 16] = [0; 16];
    let mut sizes: [u64; 16] = [0; 16];
    let mut count: usize = 0;
    let mut err = false;
    let mut notdir = false;

    with_vfs(|vfs| {
        let did = match vfs.resolve_path(cwd, path) {
            Ok(v) => v,
            Err(_) => { err = true; return; }
        };
        if !vfs.nodes[did].is_dir() { notdir = true; return; }

        if vfs.nodes[did].fs_type == vfs::FsType::Ext2 && !vfs.nodes[did].children_loaded {
            let _ = vfs.ext2_ensure_children_loaded(did);
        }

        let eff = vfs.xm(did);
        for i in 0..MAX_CHILDREN {
            if count >= 16 { break; }
            if !vfs.nodes[eff].children.slots[i].used() { continue; }
            let cid = vfs.nodes[eff].children.slots[i].id as usize;
            if cid >= MAX_VNODES || !vfs.nodes[cid].active { continue; }
            let nm = vfs.nodes[cid].get_name().as_bytes();
            let l = nm.len().min(23);
            names[count][..l].copy_from_slice(&nm[..l]);
            nlens[count] = l as u8;
            kinds[count] = vfs.nodes[cid].kind as u8;
            sizes[count] = vfs.nodes[cid].size;
            count += 1;
        }
    });

    if err { print_error!("ls: not found"); return; }
    if notdir { print_error!("ls: not a directory"); return; }
    cprintln!(120, 140, 140, "  ./");
    cprintln!(120, 140, 140, "  ../");
    for i in 0..count {
        let nm = unsafe { core::str::from_utf8_unchecked(&names[i][..nlens[i] as usize]) };
        match kinds[i] {
            1 => cprintln!(0, 220, 220, "  {}/", nm),
            2 => cprintln!(128, 222, 217, "  {} -> (symlink)", nm),
            3 | 4 => cprintln!(220, 220, 100, "  {}*", nm),
            5 => cprintln!(220, 220, 100, "  {}|", nm),
            6 => cprintln!(220, 220, 100, "  {}=", nm),
            _ => cprintln!(230, 240, 240, "  {} ({}b)", nm, sizes[i]),
        }
    }
}

pub fn cmd_cd(arg: &str) {
    if arg.is_empty() {
        let mut s = SESSION.lock();
        s.cwd = 0; s.path[0] = b'/'; s.plen = 1;
        return;
    }
    let cwd = SESSION.lock().cwd;
    let result = with_vfs(|vfs| {
        match vfs.resolve_path(cwd, arg) {
            Ok(id) => {
                if vfs.nodes[id].is_dir() { Ok(id) }
                else { Err(vfs::VfsError::NotDirectory) }
            }
            Err(e) => Err(e),
        }
    });
    match result {
        Ok(new_id) => {
            let mut s = SESSION.lock();
            s.cwd = new_id;
            crate::shell::update_path(&mut s, arg);
        }
        Err(e) => print_error!("cd: {:?}", e),
    }
}

pub fn cmd_pwd() {
    let s = SESSION.lock();
    let p = unsafe { core::str::from_utf8_unchecked(&s.path[..s.plen]) };
    cprintln!(0, 220, 220, "{}", p);
}

pub fn cmd_mkdir(name: &str) {
    let cwd = SESSION.lock().cwd;
    match with_vfs(|v| v.mkdir(cwd, name, FileMode::default_dir())) {
        Ok(_) => {}
        Err(e) => print_error!("mkdir: {:?}", e),
    }
}

pub fn cmd_touch(name: &str) {
    let cwd = SESSION.lock().cwd;
    with_vfs(|v| {
        match v.resolve_path(cwd, name) {
            Ok(_) => {}
            Err(VfsError::NotFound) => {
                match v.open(cwd, name, OpenFlags(OpenFlags::WRITE | OpenFlags::CREATE), FileMode::default_file()) {
                    Ok(fd) => { let _ = v.close(fd); }
                    Err(e) => print_error!("touch: {:?}", e),
                }
            }
            Err(e) => print_error!("touch: {:?}", e),
        }
    });
}

pub fn cmd_cat(name: &str) {
    let cwd = SESSION.lock().cwd;
    with_vfs(|v| {
        let is_dev = name.starts_with("/dev/") || name.starts_with("dev/");

        match v.open(cwd, name, OpenFlags(OpenFlags::READ), FileMode::default_file()) {
            Ok(fd) => {
                let mut buf = [0u8; 64];
                let mut total = 0usize;
                let max_read: usize = if is_dev { 256 } else { 4096 };
                console::set_color(230, 240, 240);
                loop {
                    match v.read(fd, &mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if is_dev {
                                for i in 0..n {
                                    let b = buf[i];
                                    let hi = b >> 4;
                                    let lo = b & 0x0F;
                                    let hex_hi = if hi < 10 { b'0' + hi } else { b'a' + hi - 10 };
                                    let hex_lo = if lo < 10 { b'0' + lo } else { b'a' + lo - 10 };
                                    print!("{}{} ", hex_hi as char, hex_lo as char);
                                    if (total + i + 1) % 16 == 0 {
                                        println!("");
                                    }
                                }
                            } else {
                                let s = unsafe { core::str::from_utf8_unchecked(&buf[..n]) };
                                print!("{}", s);
                            }
                            total += n;
                            if total >= max_read {
                                break;
                            }
                        }
                        Err(e) => { print_error!("read: {:?}", e); break; }
                    }
                }
                console::reset_color();
                if is_dev && total > 0 {
                    println!("");
                    cprintln!(120, 140, 140, "  ({} bytes)", total);
                } else {
                    println!("");
                }
                let _ = v.close(fd);
            }
            Err(e) => print_error!("cat: {:?}", e),
        }
    });
}

pub fn cmd_write(name: &str, text: &str) {
    let cwd = SESSION.lock().cwd;
    with_vfs(|v| {
        let fl = OpenFlags(OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE);
        match v.open(cwd, name, fl, FileMode::default_file()) {
            Ok(fd) => {
                match v.write(fd, text.as_bytes()) {
                    Ok(n) => print_success!("\nWrote {} bytes", n),
                    Err(e) => print_error!("write: {:?}", e),
                }
                let _ = v.close(fd);
            }
            Err(e) => print_error!("write: {:?}", e),
        }
    });
}

pub fn cmd_stat(path: &str) {
    let cwd = SESSION.lock().cwd;
    let result = with_vfs(|v| v.lstat(cwd, path));
    match result {
        Ok(st) => {
            cprintln!(128, 222, 217, "  File: {}", path);
            cprintln!(230, 240, 240, "  Type: {:?}", st.kind);
            cprintln!(230, 240, 240, "  Size: {} bytes", st.size);
            cprintln!(230, 240, 240, "  Mode: 0o{:o}", st.mode.0);
            cprintln!(230, 240, 240, "  Owner: {}:{}", st.uid, st.gid);
            cprintln!(230, 240, 240, "  Links: {}", st.nlinks);
            cprintln!(120, 140, 140, "  FS:    {:?}", st.fs_type);
        }
        Err(e) => print_error!("stat: {:?}", e),
    }
}

pub fn cmd_rm(path: &str) {
    let cwd = SESSION.lock().cwd;
    match with_vfs(|v| v.unlink(cwd, path)) {
        Ok(_) => {}
        Err(e) => print_error!("rm: {:?}", e),
    }
}

pub fn cmd_rm_rf(path: &str) {
    let cwd = SESSION.lock().cwd;

    let info = with_vfs(|v| {
        match v.resolve_path(cwd, path) {
            Ok(id) => Ok((id, v.nodes[id].kind)),
            Err(e) => Err(e),
        }
    });

    match info {
        Ok((_id, kind)) => {
            match kind {
                VNodeKind::Directory => {
                    recursive_rm(cwd, path);
                }
                _ => {
                    match with_vfs(|v| v.unlink(cwd, path)) {
                        Ok(_) => {}
                        Err(e) => print_error!("rm -rf: {:?}", e),
                    }
                }
            }
        }
        Err(VfsError::NotFound) => {}
        Err(e) => print_error!("rm -rf: {:?}", e),
    }
}

fn recursive_rm(cwd: usize, path: &str) {
    let mut child_names: [[u8; 23]; 16] = [[0; 23]; 16];
    let mut child_lens: [u8; 16] = [0; 16];
    let mut child_kinds: [u8; 16] = [0; 16];
    let mut child_count: usize = 0;

    with_vfs(|vfs| {
        let did = match vfs.resolve_path(cwd, path) {
            Ok(v) => v,
            Err(_) => return,
        };
        let eff = vfs.xm(did);
        for i in 0..MAX_CHILDREN {
            if child_count >= 16 { break; }
            if !vfs.nodes[eff].children.slots[i].used() { continue; }
            let cid = vfs.nodes[eff].children.slots[i].id as usize;
            if cid >= MAX_VNODES || !vfs.nodes[cid].active { continue; }
            let nm = vfs.nodes[cid].get_name().as_bytes();
            let l = nm.len().min(23);
            child_names[child_count][..l].copy_from_slice(&nm[..l]);
            child_lens[child_count] = l as u8;
            child_kinds[child_count] = vfs.nodes[cid].kind as u8;
            child_count += 1;
        }
    });

    for i in 0..child_count {
        let name = unsafe { core::str::from_utf8_unchecked(&child_names[i][..child_lens[i] as usize]) };

        let mut child_path = [0u8; 64];
        let mut cp_len = 0;
        for &b in path.as_bytes() {
            if cp_len < 63 { child_path[cp_len] = b; cp_len += 1; }
        }
        if cp_len < 63 && cp_len > 0 && child_path[cp_len - 1] != b'/' {
            child_path[cp_len] = b'/';
            cp_len += 1;
        }
        for &b in name.as_bytes() {
            if cp_len < 63 { child_path[cp_len] = b; cp_len += 1; }
        }
        let child_path_str = unsafe { core::str::from_utf8_unchecked(&child_path[..cp_len]) };

        if child_kinds[i] == 1 {
            recursive_rm(cwd, child_path_str);
        } else {
            let _ = with_vfs(|v| v.unlink(cwd, child_path_str));
        }
    }

    match with_vfs(|v| v.rmdir(cwd, path)) {
        Ok(_) => {}
        Err(e) => print_error!("rm -rf {}: {:?}", path, e),
    }
}

pub fn cmd_rmdir(path: &str) {
    let cwd = SESSION.lock().cwd;
    match with_vfs(|v| v.rmdir(cwd, path)) {
        Ok(_) => {}
        Err(e) => print_error!("rmdir: {:?}", e),
    }
}

pub fn cmd_mv(old: &str, new: &str) {
    let cwd = SESSION.lock().cwd;
    match with_vfs(|v| v.rename(cwd, old, new)) {
        Ok(_) => {}
        Err(e) => print_error!("mv: {:?}", e),
    }
}

pub fn cmd_symlink(target: &str, linkname: &str) {
    serial_println!("[symlink] target='{}' linkname='{}'", target, linkname);
    let cwd = SESSION.lock().cwd;
    match with_vfs(|v| v.symlink(cwd, linkname, target)) {
        Ok(_) => print_success!("  {} -> {}", linkname, target),
        Err(e) => print_error!("ln -s: {:?}", e),
    }
}

pub fn cmd_link(existing: &str, new_name: &str) {
    serial_println!("[link] existing='{}' new_name='{}'", existing, new_name);
    let cwd = SESSION.lock().cwd;
    match with_vfs(|v| v.link(cwd, existing, cwd, new_name)) {
        Ok(_) => print_success!("  {} => {}", new_name, existing),
        Err(e) => print_error!("ln: {:?}", e),
    }
}

pub fn cmd_readlink(path: &str) {
    let cwd = SESSION.lock().cwd;
    match with_vfs_ro(|v| v.readlink(cwd, path)) {
        Ok(target) => cprintln!(230, 240, 240, "  {}", target.as_str()),
        Err(e) => print_error!("readlink: {:?}", e),
    }
}

pub fn cmd_chmod(mode_str: &str, path: &str) {
    let mode = parse_octal(mode_str);
    if mode.is_none() {
        print_error!("chmod: invalid mode '{}'", mode_str);
        return;
    }
    let cwd = SESSION.lock().cwd;
    match with_vfs(|v| v.chmod(cwd, path, FileMode::new(mode.unwrap()))) {
        Ok(_) => {}
        Err(e) => print_error!("chmod: {:?}", e),
    }
}

pub fn cmd_df() {
    let cwd = SESSION.lock().cwd;
    let result = with_vfs_ro(|v| v.statfs(cwd, "/"));
    match result {
        Ok(st) => {
            cprintln!(128, 222, 217, "  Filesystem  Type    Blocks  Free  Inodes  IFree");
            cprintln!(230, 240, 240, "  /            {:?}    {:>6}  {:>4}  {:>6}  {:>5}",
                st.fs_type, st.total_blocks, st.free_blocks,
                st.total_inodes, st.free_inodes);
        }
        Err(e) => print_error!("df: {:?}", e),
    }
}

fn parse_octal(s: &str) -> Option<u16> {
    let mut result: u16 = 0;
    for &b in s.as_bytes() {
        if b < b'0' || b > b'7' { return None; }
        result = result.checked_mul(8)?.checked_add((b - b'0') as u16)?;
    }
    if result > 0o7777 { return None; }
    Some(result)
}

pub fn cmd_mount_list() {
    cprintln!(128, 222, 217, "  Filesystem    Mountpoint    Type");
    cprintln!(230, 240, 240, "  tmpfs         /             tmpfs");
    cprintln!(230, 240, 240, "  devfs         /dev          devfs");
    cprintln!(230, 240, 240, "  procfs        /proc         procfs");

    let has_ext2 = with_vfs_ro(|vfs| vfs.ext2_mount_active);

    if has_ext2 {
        cprintln!(230, 240, 240, "  ext2          /mnt          ext2");
    }
}

pub fn cmd_mount(fstype: &str, target: &str) {
    match fstype {
        "ext2" => {
            let mountpoint = if target.is_empty() { "/mnt" } else { target };
            mount_ext2_to_vfs(mountpoint);
        }
        _ => {
            print_error!("mount: unknown filesystem '{}'", fstype);
            println!("  Supported: ext2");
        }
    }
}

pub fn cmd_umount(path: &str) {
    let cwd = SESSION.lock().cwd;
    let result = with_vfs(|vfs| {
        let id = vfs.resolve_path(cwd, path)?;

        if vfs.nodes[id].fs_type != vfs::FsType::Ext2 {
            return Err(VfsError::InvalidArgument);
        }

        vfs.evict_ext2_children(id);
        vfs.nodes[id].fs_type = vfs::FsType::TmpFS;
        vfs.nodes[id].ext2_ino = 0;
        vfs.nodes[id].children_loaded = false;
        vfs.ext2_mount_active = false;

        Ok(())
    });

    match result {
        Ok(()) => print_success!("  unmounted {}", path),
        Err(e) => print_error!("umount: {:?}", e),
    }
}

fn mount_ext2_to_vfs(mountpoint: &str) {
    use crate::commands::ext2_cmds;
    use crate::miku_extfs::structs::EXT2_ROOT_INO;

    if !ext2_cmds::is_ext2_ready() {
        print_error!("  ext2 not mounted. Run ext2mount first");
        return;
    }

    serial_println!("[mount] mounting ext2 at {} (lazy)", mountpoint);

    let cwd = SESSION.lock().cwd;

    let result = with_vfs(|vfs| {
        let mount_id = match vfs.resolve_path(cwd, mountpoint) {
            Ok(id) => {
                if !vfs.nodes[id].is_dir() {
                    return Err(VfsError::NotDirectory);
                }
                id
            }
            Err(VfsError::NotFound) => {
                let (parent_path, dirname) = vfs::path::PathWalker::split_last(mountpoint);
                let parent_id = vfs.resolve_path(cwd, parent_path)?;
                vfs.mkdir(parent_id, dirname, FileMode::default_dir())?
            }
            Err(e) => return Err(e),
        };

        vfs.nodes[mount_id].fs_type = vfs::FsType::Ext2;
        vfs.nodes[mount_id].ext2_ino = EXT2_ROOT_INO;
        vfs.nodes[mount_id].children_loaded = false;
        vfs.ext2_mount_active = true;

        Ok(mount_id)
    });

    match result {
        Ok(_id) => {
            print_success!("  ext2 mounted at {} (on-demand)", mountpoint);
        }
        Err(e) => print_error!("mount: {:?}", e),
    }
}
