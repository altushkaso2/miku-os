use crate::console;
use crate::shell::SESSION;
use crate::vfs::{
    self, with_vfs, with_vfs_ro, FileMode, OpenFlags, VNodeKind, VfsError, MAX_CHILDREN, MAX_VNODES,
};
use crate::{cprint, cprintln, print, print_error, print_success, println, serial_println};

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

    with_vfs_ro(|vfs| {
        let did = match vfs.resolve_path(cwd, path) {
            Ok(v) => v,
            Err(_) => {
                err = true;
                return;
            }
        };
        if !vfs.nodes[did].is_dir() {
            notdir = true;
            return;
        }
        let eff = vfs.xm(did);
        for i in 0..MAX_CHILDREN {
            if count >= 16 {
                break;
            }
            if !vfs.nodes[eff].children.slots[i].used() {
                continue;
            }
            let cid = vfs.nodes[eff].children.slots[i].id as usize;
            if cid >= MAX_VNODES || !vfs.nodes[cid].active {
                continue;
            }
            let nm = vfs.nodes[cid].get_name().as_bytes();
            let l = nm.len().min(23);
            names[count][..l].copy_from_slice(&nm[..l]);
            nlens[count] = l as u8;
            kinds[count] = vfs.nodes[cid].kind as u8;
            sizes[count] = vfs.nodes[cid].size;
            count += 1;
        }
    });

    if err {
        print_error!("ls: not found");
        return;
    }
    if notdir {
        print_error!("ls: not a directory");
        return;
    }
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
        s.cwd = 0;
        s.path[0] = b'/';
        s.plen = 1;
        return;
    }
    let cwd = SESSION.lock().cwd;
    let result = with_vfs_ro(|vfs| match vfs.resolve_path(cwd, arg) {
        Ok(id) => {
            if vfs.nodes[id].is_dir() {
                Ok(id)
            } else {
                Err(vfs::VfsError::NotDirectory)
            }
        }
        Err(e) => Err(e),
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
    with_vfs(|v| match v.resolve_path(cwd, name) {
        Ok(_) => {}
        Err(VfsError::NotFound) => {
            match v.open(
                cwd,
                name,
                OpenFlags(OpenFlags::WRITE | OpenFlags::CREATE),
                FileMode::default_file(),
            ) {
                Ok(fd) => {
                    let _ = v.close(fd);
                }
                Err(e) => print_error!("touch: {:?}", e),
            }
        }
        Err(e) => print_error!("touch: {:?}", e),
    });
}

pub fn cmd_cat(name: &str) {
    let cwd = SESSION.lock().cwd;
    with_vfs(|v| {
        match v.open(
            cwd,
            name,
            OpenFlags(OpenFlags::READ),
            FileMode::default_file(),
        ) {
            Ok(fd) => {
                let mut buf = [0u8; 64];
                console::set_color(230, 240, 240);
                loop {
                    match v.read(fd, &mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let s = unsafe { core::str::from_utf8_unchecked(&buf[..n]) };
                            print!("{}", s);
                        }
                        Err(e) => {
                            print_error!("read: {:?}", e);
                            break;
                        }
                    }
                }
                console::reset_color();
                println!("");
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
    let result = with_vfs_ro(|v| v.lstat(cwd, path));
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

    let info = with_vfs_ro(|v| match v.resolve_path(cwd, path) {
        Ok(id) => Ok((id, v.nodes[id].kind)),
        Err(e) => Err(e),
    });

    match info {
        Ok((_id, kind)) => match kind {
            VNodeKind::Directory => {
                recursive_rm(cwd, path);
            }
            _ => match with_vfs(|v| v.unlink(cwd, path)) {
                Ok(_) => {}
                Err(e) => print_error!("rm -rf: {:?}", e),
            },
        },
        Err(VfsError::NotFound) => {}
        Err(e) => print_error!("rm -rf: {:?}", e),
    }
}

fn recursive_rm(cwd: usize, path: &str) {
    let mut child_names: [[u8; 23]; 16] = [[0; 23]; 16];
    let mut child_lens: [u8; 16] = [0; 16];
    let mut child_kinds: [u8; 16] = [0; 16];
    let mut child_count: usize = 0;

    with_vfs_ro(|vfs| {
        let did = match vfs.resolve_path(cwd, path) {
            Ok(v) => v,
            Err(_) => return,
        };
        let eff = vfs.xm(did);
        for i in 0..MAX_CHILDREN {
            if child_count >= 16 {
                break;
            }
            if !vfs.nodes[eff].children.slots[i].used() {
                continue;
            }
            let cid = vfs.nodes[eff].children.slots[i].id as usize;
            if cid >= MAX_VNODES || !vfs.nodes[cid].active {
                continue;
            }
            let nm = vfs.nodes[cid].get_name().as_bytes();
            let l = nm.len().min(23);
            child_names[child_count][..l].copy_from_slice(&nm[..l]);
            child_lens[child_count] = l as u8;
            child_kinds[child_count] = vfs.nodes[cid].kind as u8;
            child_count += 1;
        }
    });

    for i in 0..child_count {
        let name =
            unsafe { core::str::from_utf8_unchecked(&child_names[i][..child_lens[i] as usize]) };

        let mut child_path = [0u8; 64];
        let mut cp_len = 0;
        for &b in path.as_bytes() {
            if cp_len < 63 {
                child_path[cp_len] = b;
                cp_len += 1;
            }
        }
        if cp_len < 63 && cp_len > 0 && child_path[cp_len - 1] != b'/' {
            child_path[cp_len] = b'/';
            cp_len += 1;
        }
        for &b in name.as_bytes() {
            if cp_len < 63 {
                child_path[cp_len] = b;
                cp_len += 1;
            }
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
            cprintln!(
                128,
                222,
                217,
                "  Filesystem  Type    Blocks  Free  Inodes  IFree"
            );
            cprintln!(
                230,
                240,
                240,
                "  /            {:?}    {:>6}  {:>4}  {:>6}  {:>5}",
                st.fs_type,
                st.total_blocks,
                st.free_blocks,
                st.total_inodes,
                st.free_inodes
            );
        }
        Err(e) => print_error!("df: {:?}", e),
    }
}

fn parse_octal(s: &str) -> Option<u16> {
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

pub fn cmd_mount_list() {
    cprintln!(128, 222, 217, "  Filesystem    Mountpoint    Type");
    cprintln!(230, 240, 240, "  tmpfs         /             tmpfs");
    cprintln!(230, 240, 240, "  devfs         /dev          devfs");
    cprintln!(230, 240, 240, "  procfs        /proc         procfs");

    let has_ext2 = with_vfs_ro(|vfs| {
        for i in 0..MAX_VNODES {
            if vfs.nodes[i].active
                && vfs.nodes[i].is_dir()
                && vfs.nodes[i].fs_type == vfs::FsType::Ext2
            {
                return true;
            }
        }
        false
    });

    if has_ext2 {
        cprintln!(230, 240, 240, "  ext2          /mnt          ext2 (ro)");
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

        for i in 1..MAX_VNODES {
            if vfs.nodes[i].active && vfs.nodes[i].fs_type == vfs::FsType::Ext2 && i != id {
                let pid = vfs.nodes[i].parent as usize;
                if pid < MAX_VNODES && vfs.nodes[pid].active {
                    let h = vfs::hash::name_hash(vfs.nodes[i].get_name());
                    vfs.nodes[pid].children.remove(h, i as u16);
                }
                vfs.free_file_pages(i);
                vfs.nodes[i].active = false;
            }
        }

        vfs.nodes[id].fs_type = vfs::FsType::TmpFS;
        vfs.nodes[id].children.clear();

        Ok(())
    });

    match result {
        Ok(()) => print_success!("  unmounted {}", path),
        Err(e) => print_error!("umount: {:?}", e),
    }
}

fn mount_ext2_to_vfs(mountpoint: &str) {
    use crate::commands::ext2_cmds;
    use crate::fs::ext2::structs::EXT2_ROOT_INO;

    if !ext2_cmds::is_ext2_ready() {
        print_error!("  ext2 not mounted. Run ext2mount first");
        return;
    }

    serial_println!("[mount] mounting ext2 at {}", mountpoint);

    let cwd = SESSION.lock().cwd;

    let mount_id = with_vfs(|vfs| match vfs.resolve_path(cwd, mountpoint) {
        Ok(id) => {
            if !vfs.nodes[id].is_dir() {
                return Err(VfsError::NotDirectory);
            }
            Ok(id)
        }
        Err(VfsError::NotFound) => {
            let (parent_path, dirname) = vfs::path::PathWalker::split_last(mountpoint);
            let parent_id = vfs.resolve_path(cwd, parent_path)?;
            vfs.mkdir(parent_id, dirname, FileMode::default_dir())
        }
        Err(e) => Err(e),
    });

    let mount_id = match mount_id {
        Ok(id) => id,
        Err(e) => {
            print_error!("mount: cannot create {}: {:?}", mountpoint, e);
            return;
        }
    };

    let result = ext2_cmds::with_ext2_pub(|fs| populate_ext2_dir(fs, EXT2_ROOT_INO, mount_id, 0));

    match result {
        Some(Ok(count)) => {
            with_vfs(|vfs| {
                vfs.nodes[mount_id].fs_type = vfs::FsType::Ext2;
            });
            print_success!("  ext2 mounted at {} ({} entries)", mountpoint, count);
        }
        Some(Err(e)) => print_error!("mount: ext2 error: {:?}", e),
        None => print_error!("mount: ext2 not available"),
    }
}

fn populate_ext2_dir(
    fs: &mut crate::fs::ext2::Ext2Fs,
    ext2_ino: u32,
    vfs_parent: usize,
    depth: usize,
) -> Result<usize, crate::fs::ext2::Ext2Error> {
    use crate::fs::ext2::structs as es;

    if depth > 4 {
        return Ok(0);
    }

    let inode = fs.read_inode(ext2_ino)?;
    let mut entries = [const { es::DirEntry::empty() }; 64];
    let count = fs.read_dir(&inode, &mut entries)?;

    let mut total = 0usize;

    for i in 0..count {
        let e = &entries[i];
        let name = e.name_str();

        if name == "." || name == ".." || name == "lost+found" {
            continue;
        }

        let entry_ino = e.inode;
        let entry_inode = fs.read_inode(entry_ino)?;

        if entry_inode.is_directory() {
            let dir_id = with_vfs(|vfs| {
                vfs.mkdir(vfs_parent, name, FileMode::new(entry_inode.permissions()))
            });

            match dir_id {
                Ok(did) => {
                    with_vfs(|vfs| {
                        vfs.nodes[did].fs_type = vfs::FsType::Ext2;
                    });
                    let sub = populate_ext2_dir(fs, entry_ino, did, depth + 1)?;
                    total += 1 + sub;
                }
                Err(e) => {
                    serial_println!("[mount] skip dir '{}': {:?}", name, e);
                }
            }
        } else if entry_inode.is_regular() {
            let size = entry_inode.size();

            let fid = with_vfs(|vfs| {
                vfs.create_file(vfs_parent, name, FileMode::new(entry_inode.permissions()))
            });

            match fid {
                Ok(fid) => {
                    with_vfs(|vfs| {
                        vfs.nodes[fid].fs_type = vfs::FsType::Ext2;
                    });

                    if size > 0 && size <= 6144 {
                        let mut offset = 0u64;
                        let mut buf = [0u8; 512];

                        while offset < size {
                            let to_read = ((size - offset) as usize).min(512);
                            let n = match fs.read_file(&entry_inode, offset, &mut buf[..to_read]) {
                                Ok(n) => n,
                                Err(_) => break,
                            };
                            if n == 0 {
                                break;
                            }

                            with_vfs(|vfs| {
                                let fl = OpenFlags(OpenFlags::WRITE);
                                if let Ok(fd) = vfs.fd_table.alloc(fid as u16, fl) {
                                    if let Ok(f) = vfs.fd_table.get_mut(fd) {
                                        f.offset = offset;
                                    }
                                    let _ = vfs.write(fd, &buf[..n]);
                                    let _ = vfs.fd_table.close(fd);
                                }
                            });

                            offset += n as u64;
                        }
                    } else if size > 6144 {
                        with_vfs(|vfs| {
                            vfs.nodes[fid].size = size;
                        });
                    }

                    total += 1;
                }
                Err(e) => {
                    serial_println!("[mount] skip file '{}': {:?}", name, e);
                }
            }
        } else if entry_inode.is_symlink() && entry_inode.is_fast_symlink() {
            let target_bytes = entry_inode.fast_symlink_target();
            if let Ok(target) = core::str::from_utf8(target_bytes) {
                let r = with_vfs(|vfs| vfs.symlink(vfs_parent, name, target));
                match r {
                    Ok(sid) => {
                        with_vfs(|vfs| {
                            vfs.nodes[sid].fs_type = vfs::FsType::Ext2;
                        });
                        total += 1;
                    }
                    Err(e) => {
                        serial_println!("[mount] skip symlink '{}': {:?}", name, e);
                    }
                }
            }
        }
    }

    Ok(total)
}
