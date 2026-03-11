use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::Duration,
};

fn ask_user(prompt: &str, timeout_secs: u64) -> bool {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            let _ = tx.send(input.trim().to_lowercase());
        }
    });
    match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
        Ok(input) => input == "y" || input == "yes",
        Err(_) => { println!("Auto: N"); false }
    }
}

fn ask_mb(prompt: &str, default_mb: u32, timeout_secs: u64) -> u32 {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            let _ = tx.send(input.trim().to_string());
        }
    });
    match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
        Ok(ref s) if s.is_empty() => {
            println!("Auto: {} MB", default_mb);
            default_mb
        }
        Ok(s) => s.parse::<u32>().unwrap_or_else(|_| {
            println!("Invalid, using {} MB", default_mb);
            default_mb
        }),
        Err(_) => { println!("Auto: {} MB", default_mb); default_mb }
    }
}

fn parse_meminfo(content: &str, field: &str) -> u64 {
    content.lines()
        .find(|l| l.starts_with(field))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

fn detect_qemu_ram() -> String {
    let content  = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let total_mb = parse_meminfo(&content, "MemTotal:") / 1024;
    let free_mb  = parse_meminfo(&content, "MemFree:")  / 1024;
    let buffers  = parse_meminfo(&content, "Buffers:")  / 1024;
    let cached   = parse_meminfo(&content, "Cached:")   / 1024;
    let phys_free = free_mb + buffers + cached;
    let target_mb = ((phys_free as f64 * 0.8) as u64).min(total_mb).max(512);
    let ram = format!("{}M", target_mb);
    println!("[*] Host RAM: {} MB  Phys free: {} MB  → QEMU gets: {}", total_mb, phys_free, ram);
    ram
}

fn check_grub_mkrescue() {
    let ok = Command::new("grub-mkrescue")
        .arg("--version").output()
        .map(|o| o.status.success()).unwrap_or(false);
    if !ok { panic!("grub-mkrescue not found"); }
    println!("[ok] grub-mkrescue found");
}

fn build_kernel(root: &Path, low_ram: bool) {
    println!("Building kernel");
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root)
        .arg("build")
        .arg("--target").arg("x86_64-unknown-none")
        .arg("-Z").arg("build-std=core,compiler_builtins,alloc")
        .arg("-Z").arg("build-std-features=compiler-builtins-mem");

    let mut rustflags =
        "-C relocation-model=static -C link-arg=-Tlinker.ld -C link-arg=--no-dynamic-linker"
            .to_string();
    if low_ram {
        cmd.arg("--jobs").arg("1");
        rustflags.push_str(" -C codegen-units=1");
    }
    cmd.env("RUSTFLAGS", &rustflags);

    if !cmd.status().expect("cargo build failed").success() {
        panic!("Kernel compilation failed");
    }
}

fn create_iso(root: &Path) {
    let out_dir  = root.join("miku-os");
    fs::create_dir_all(&out_dir).unwrap();

    let iso_root = root.join("iso_root");
    if iso_root.exists() { fs::remove_dir_all(&iso_root).unwrap(); }
    fs::create_dir_all(iso_root.join("boot/grub")).unwrap();

    let kernel_src = root.join("target/x86_64-unknown-none/debug/miku-os-release");
    let kernel_dst = iso_root.join("boot/kernel.elf");
    fs::copy(&kernel_src, &kernel_dst).unwrap_or_else(|e| {
        panic!("Cannot copy kernel: {}", e)
    });

    let grub_cfg_src = root.join("grub.cfg");
    let grub_cfg_dst = iso_root.join("boot/grub/grub.cfg");
    let cfg = fs::read_to_string(&grub_cfg_src)
        .unwrap_or_else(|e| panic!("Cannot read grub.cfg: {}", e));
    let mut new_cfg = String::from("set timeout=-1\n");
    for line in cfg.lines() {
        let t = line.trim();
        if !t.starts_with("set timeout=") && !t.starts_with("timeout=") {
            new_cfg.push_str(line);
            new_cfg.push('\n');
        }
    }
    fs::write(&grub_cfg_dst, new_cfg)
        .unwrap_or_else(|e| panic!("Cannot write grub.cfg: {}", e));

    let iso_path = out_dir.join("miku-os.iso");
    println!("Creating ISO: {}", iso_path.display());
    let status = Command::new("grub-mkrescue")
        .args(["-o", iso_path.to_str().unwrap(), iso_root.to_str().unwrap()])
        .status().expect("grub-mkrescue failed");
    if !status.success() { panic!("grub-mkrescue failed"); }

    println!("[ok] ISO created: {}", iso_path.display());
    println!("    Size: {} KB", fs::metadata(&iso_path).unwrap().len() / 1024);
    fs::remove_dir_all(&iso_root).ok();
}

fn ensure_disk(path: &Path, size_mb: u32, label: &str) {
    if path.exists() {
        println!("[ok] {} exists: {} ({} MB)", label, path.display(),
            fs::metadata(path).unwrap().len() / (1024 * 1024));
        return;
    }
    println!("[*] Creating {} disk: {} MB → {}", label, size_mb, path.display());
    let status = Command::new("dd")
        .args([
            "if=/dev/zero",
            &format!("of={}", path.display()),
            "bs=1M",
            &format!("count={}", size_mb),
        ])
        .status().expect("dd failed");
    if !status.success() { panic!("dd failed for {}", label); }
    println!("[ok] {} disk created: {} MB", label, size_mb);
}

struct DiskConfig {
    main_mb:  u32,
    data_mb:  u32,
}

impl DiskConfig {
    fn ask(root: &Path) -> Self {
        let main_exists = root.join("miku-os/disk.img").exists();
        let data_exists = root.join("miku-os/data.img").exists();

        if main_exists && data_exists {
            let main_mb = (fs::metadata(root.join("miku-os/disk.img")).unwrap().len()
                / (1024 * 1024)) as u32;
            let data_mb = (fs::metadata(root.join("miku-os/data.img")).unwrap().len()
                / (1024 * 1024)) as u32;
            return Self { main_mb, data_mb };
        }

        println!("Disk Setup");
        println!("  disk.img  →  drive 1  (swap + ext4 root, like /dev/sda)");
        println!("  data.img  →  drive 2  (extra data storage, optional)");

        let main_mb = if main_exists {
            (fs::metadata(root.join("miku-os/disk.img")).unwrap().len() / (1024*1024)) as u32
        } else {
            ask_mb("  disk.img size in MB (default 4096): ", 4096, 30)
        };

        let want_data = ask_user("  Create data.img for extra storage? [y/N]: ", 15);
        let data_mb = if want_data && !data_exists {
            ask_mb("  data.img size in MB (default 2048): ", 2048, 30)
        } else if data_exists {
            (fs::metadata(root.join("miku-os/data.img")).unwrap().len() / (1024*1024)) as u32
        } else {
            0
        };

        Self { main_mb, data_mb }
    }
}

fn main() {
    println!("MikuOS ISO Builder (Release)\n");

    let root = PathBuf::from("..").canonicalize()
        .unwrap_or_else(|_| PathBuf::from(".."));

    let low_ram = ask_user("Low RAM mode? (for weak PCs) [y/N]: ", 10);

    check_grub_mkrescue();
    build_kernel(&root, low_ram);
    create_iso(&root);

    let cfg = DiskConfig::ask(&root);

    let disk_path = root.join("miku-os/disk.img");
    ensure_disk(&disk_path, cfg.main_mb, "main");

    let data_path = root.join("miku-os/data.img");
    if cfg.data_mb > 0 {
        ensure_disk(&data_path, cfg.data_mb, "data");
    }

    if !ask_user("\nLaunch QEMU? [y/N]: ", 10) { return; }

    let ram      = detect_qemu_ram();
    let iso_path = root.join("miku-os/miku-os.iso");

    let mut args: Vec<String> = vec![
        "-boot".into(), "d".into(),
        "-cdrom".into(), iso_path.to_str().unwrap().into(),
        "-drive".into(),
        format!("file={},format=raw,if=none,id=disk0,cache=unsafe,aio=threads",
            disk_path.display()),
        "-device".into(),
        "ide-hd,drive=disk0,bus=ide.0,unit=1,rotation_rate=1".into(),

        "-serial".into(), "stdio".into(),
        "-display".into(), "gtk".into(),
        "-m".into(), ram,
    ];

    if cfg.data_mb > 0 && data_path.exists() {
        args.push("-drive".into());
        args.push(format!("file={},format=raw,if=none,id=disk1,cache=unsafe,aio=threads",
            data_path.display()));
        args.push("-device".into());
        args.push("ide-hd,drive=disk1,bus=ide.1,unit=1,rotation_rate=1".into());
        println!("[*] data.img attached as drive 2");
    }

    let kvm_ok = Command::new("qemu-system-x86_64")
        .args(["-enable-kvm", "-version"]).output()
        .map(|o| o.status.success()).unwrap_or(false);
    if kvm_ok { args.push("-enable-kvm".into()); }

    println!("  drive 1  →  disk.img  ({} MB)", cfg.main_mb);
    if cfg.data_mb > 0 {
        println!("  drive 2  →  data.img  ({} MB)", cfg.data_mb);
    }

    println!("Starting QEMU");
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    Command::new("qemu-system-x86_64")
        .args(&args_refs)
        .spawn().expect("QEMU failed to start")
        .wait().unwrap();
}
