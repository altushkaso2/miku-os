use crate::shell::SHELL;
use crate::vfs::with_vfs_ro;
use crate::{allocator, console, cprint, cprintln, print, print_info, print_success, println};

pub fn cmd_echo(text: &str) {
    if !text.is_empty() {
        println!("{}", text);
    }
}

pub fn cmd_info() {
    let (vn, mn) = with_vfs_ro(|v| (v.total_vnodes(), v.total_mounts()));
    let ticks = crate::vfs::procfs::uptime_ticks();
    let total_secs = ticks / 18;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    let heap_used = allocator::used();
    let heap_free = allocator::free();
    let heap_total = allocator::HEAP_SIZE;

    cprintln!(57, 197, 187, "  MikuOS v0.0.1");
    cprintln!(230, 240, 240, "  VNodes: {}/{}", vn, crate::vfs::MAX_VNODES);
    cprintln!(230, 240, 240, "  Mounts: {}", mn);
    cprintln!(
        230,
        240,
        240,
        "  Heap:   {} / {} KB used",
        heap_used / 1024,
        heap_total / 1024
    );
    cprintln!(230, 240, 240, "  Free:   {} KB", heap_free / 1024);
    cprintln!(120, 140, 140, "  Uptime: {}h {}m {}s", hours, mins, secs);
}

pub fn cmd_heap() {
    let used = allocator::used();
    let free = allocator::free();
    let total = allocator::HEAP_SIZE;

    cprintln!(57, 197, 187, "  Heap Allocator");
    println!("  Total:  {} bytes ({} KB)", total, total / 1024);
    println!("  Used:   {} bytes ({} KB)", used, used / 1024);
    println!("  Free:   {} bytes ({} KB)", free, free / 1024);

    let pct = if total > 0 { (used * 100) / total } else { 0 };
    println!("  Usage:  {}%", pct);

    if pct > 80 {
        cprintln!(220, 220, 100, "  WARNING: heap usage high");
    }
}

pub fn cmd_poweroff() {
    cprintln!(255, 105, 140, "  Shutting down...");
    crate::serial_println!("[kern] poweroff requested");
    crate::power::shutdown();
}

pub fn cmd_reboot() {
    cprintln!(57, 197, 187, "  Rebooting...");
    crate::serial_println!("[kern] reboot requested");
    crate::power::reboot();
}

pub fn cmd_help() {
    cprintln!(57, 197, 187, "  VFS Commands:");
    cprintln!(
        128,
        222,
        217,
        "  ls cd pwd mkdir touch cat write rm rmdir mv stat"
    );
    cprintln!(
        128,
        222,
        217,
        "  ln -s <target> <name>   readlink   chmod <mode> <path>"
    );
    cprintln!(
        128,
        222,
        217,
        "  df info mount umount echo clear help history heap"
    );
    cprintln!(128, 222, 217, "  rm -rf <path>");
    cprintln!(57, 197, 187, "  Ext2 Commands:");
    cprintln!(128, 222, 217, "  ext2mount                mount ext2 disk");
    cprintln!(128, 222, 217, "  ext2info                 filesystem info");
    cprintln!(128, 222, 217, "  ext2ls [path]            list directory");
    cprintln!(128, 222, 217, "  ext2cat <path>           show file");
    cprintln!(128, 222, 217, "  ext2stat <path>          inode info");
    cprintln!(128, 222, 217, "  ext2write <path> <text>  write file");
    cprintln!(128, 222, 217, "  ext2append <path> <text> append to file");
    cprintln!(128, 222, 217, "  ext2mkdir <path>         create dir");
    cprintln!(128, 222, 217, "  ext2rm <path>            delete file");
    cprintln!(128, 222, 217, "  ext2rm -rf <path>        recursive delete");
    cprintln!(128, 222, 217, "  ext2rmdir <path>         delete empty dir");
    cprintln!(128, 222, 217, "  ext2mv <path> <newname>  rename");
    cprintln!(128, 222, 217, "  ext2cp <src> <dst>       copy file");
    cprintln!(128, 222, 217, "  ext2ln -s <tgt> <name>   symlink");
    cprintln!(128, 222, 217, "  ext2chmod <mode> <path>  change mode");
    cprintln!(128, 222, 217, "  ext2chown <u> <g> <path> change owner");
    cprintln!(128, 222, 217, "  ext2du [path]            disk usage");
    cprintln!(128, 222, 217, "  ext2tree [path]          directory tree");
    cprintln!(128, 222, 217, "  ext2fsck                 check filesystem");
    cprintln!(128, 222, 217, "  ext2cache                cache statistics");
    cprintln!(
        128,
        222,
        217,
        "  ext2cacheflush           flush block cache"
    );
    cprintln!(57, 197, 187, "  Mount:");
    cprintln!(
        128,
        222,
        217,
        "  mount ext2 <path>        mount ext2 at path"
    );
    cprintln!(128, 222, 217, "  umount <path>            unmount");
    cprintln!(57, 197, 187, "  Ext3 Commands:");
    cprintln!(
        128,
        222,
        217,
        "  ext3mkjournal            create journal (ext2->ext3)"
    );
    cprintln!(128, 222, 217, "  ext3info                 journal info");
    cprintln!(
        128,
        222,
        217,
        "  ext3journal              show transactions"
    );
    cprintln!(128, 222, 217, "  ext3recover              replay journal");
    cprintln!(
        128,
        222,
        217,
        "  ext3clean                mark journal clean"
    );
    cprintln!(57, 197, 187, "  System:");
    cprintln!(
        128,
        222,
        217,
        "  heap                     heap allocator info"
    );
    cprintln!(128, 222, 217, "  reboot                   restart system");
    cprintln!(128, 222, 217, "  poweroff                 shutdown system");
}

pub fn cmd_clear() {
    console::clear_screen();
}

pub fn cmd_history() {
    let sh = SHELL.lock();
    if sh.history_count == 0 {
        cprintln!(120, 140, 140, "  (empty)");
        return;
    }
    let start = if sh.history_count > 16 {
        sh.history_count - 16
    } else {
        0
    };
    for i in start..sh.history_count {
        let idx = i % 16;
        let entry = &sh.history[idx];
        let s = unsafe { core::str::from_utf8_unchecked(&entry.buf[..entry.len]) };
        cprint!(120, 140, 140, "  {}: ", i + 1);
        cprintln!(230, 240, 240, "{}", s);
    }
}
