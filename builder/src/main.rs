use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::Duration,
};

const LIMINE_REPO: &str = "https://github.com/limine-bootloader/limine.git";
const LIMINE_BRANCH: &str = "v8.x-binary";

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
        Err(_) => {
            println!("\nAuto: N");
            false
        }
    }
}

fn run(cmd: &str, args: &[&str]) {
    println!(">> {} {}", cmd, args.join(" "));
    let status = Command::new(cmd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("Failed to run '{}': {}", cmd, e));
    if !status.success() {
        panic!("'{}' failed with {:?}", cmd, status.code());
    }
}

fn run_in(dir: &Path, cmd: &str, args: &[&str]) {
    println!(">> [{}] {} {}", dir.display(), cmd, args.join(" "));
    let status = Command::new(cmd)
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("Failed to run '{}' in {}: {}", cmd, dir.display(), e));
    if !status.success() {
        panic!("'{}' failed in {}", cmd, dir.display());
    }
}

fn ensure_limine(root: &Path) {
    let limine_dir = root.join("limine");

    if !limine_dir.join("limine").exists() {
        println!("Cloning Limine");
        if limine_dir.exists() {
            fs::remove_dir_all(&limine_dir).ok();
        }
        run(
            "git",
            &[
                "clone",
                LIMINE_REPO,
                "--branch",
                LIMINE_BRANCH,
                "--depth=1",
                limine_dir.to_str().unwrap(),
            ],
        );
        run_in(&limine_dir, "make", &[]);
    } else {
        println!("Limine already present");
    }
}

fn build_kernel(root: &Path, low_ram: bool) {
    println!("Building kernel");

    let mut cmd = Command::new("cargo");
    cmd.current_dir(root)
        .arg("build")
        .arg("--target")
        .arg("x86_64-unknown-none")
        .arg("-Z")
        .arg("build-std=core,compiler_builtins,alloc")
        .arg("-Z")
        .arg("build-std-features=compiler-builtins-mem");

    if low_ram {
        cmd.arg("--jobs").arg("1");
        cmd.env("RUSTFLAGS", "-C codegen-units=1 -C link-arg=-Tlinker.ld -C link-arg=--no-dynamic-linker -C relocation-model=static");
    }

    let status = cmd.status().expect("cargo build failed");
    if !status.success() {
        panic!("Kernel compilation failed");
    }
}

fn create_iso(root: &Path) {
    let out_dir = root.join("miku-os");
    fs::create_dir_all(&out_dir).unwrap();

    let iso_root = root.join("iso_root");
    if iso_root.exists() {
        fs::remove_dir_all(&iso_root).unwrap();
    }

    fs::create_dir_all(iso_root.join("boot")).unwrap();
    fs::create_dir_all(iso_root.join("boot/limine")).unwrap();
    fs::create_dir_all(iso_root.join("EFI/BOOT")).unwrap();

    let kernel_src = root
        .join("target/x86_64-unknown-none/debug/miku-os-release");
    let kernel_dst = iso_root.join("boot/kernel");
    fs::copy(&kernel_src, &kernel_dst)
        .unwrap_or_else(|e| panic!("Cannot copy kernel: {} -> {}: {}", 
            kernel_src.display(), kernel_dst.display(), e));

    let conf_src = root.join("limine.conf");
    let conf_dst = iso_root.join("boot/limine/limine.conf");
    fs::copy(&conf_src, &conf_dst)
        .unwrap_or_else(|e| panic!("Cannot copy limine.conf: {}", e));

    let limine_dir = root.join("limine");

    let bios_files = [
        "limine-bios.sys",
        "limine-bios-cd.bin",
    ];
    for f in &bios_files {
        let src = limine_dir.join(f);
        let dst = iso_root.join(format!("boot/limine/{}", f));
        if src.exists() {
            fs::copy(&src, &dst).unwrap_or_else(|e| {
                println!("[warn] Cannot copy {}: {}", f, e);
                0
            });
        } else {
            println!("[warn] {} not found, BIOS boot may not work", f);
        }
    }

    let uefi_src = limine_dir.join("BOOTX64.EFI");
    let uefi_dst = iso_root.join("EFI/BOOT/BOOTX64.EFI");
    if uefi_src.exists() {
        fs::copy(&uefi_src, &uefi_dst).unwrap();
    } else {
        println!("[warn] BOOTX64.EFI not found, UEFI boot may not work");
    }

    let ia32_src = limine_dir.join("BOOTIA32.EFI");
    let ia32_dst = iso_root.join("EFI/BOOT/BOOTIA32.EFI");
    if ia32_src.exists() {
        fs::copy(&ia32_src, &ia32_dst).ok();
    }

    let iso_path = out_dir.join("miku-os.iso");
    println!("Creating ISO: {}", iso_path.display());

    let mut xorriso_args = vec![
        "-as", "mkisofs",
        "-b", "boot/limine/limine-bios-cd.bin",
        "-no-emul-boot",
        "-boot-load-size", "4",
        "-boot-info-table",
    ];

    if uefi_dst.exists() {
        let efi_img = iso_root.join("boot/limine/efi.img");
        create_efi_image(&iso_root, &efi_img);

        xorriso_args.extend_from_slice(&[
            "--efi-boot", "boot/limine/efi.img",
            "-efi-boot-part",
            "--efi-boot-image",
        ]);
    }

    let iso_root_str = iso_root.to_str().unwrap().to_string();
    let iso_path_str = iso_path.to_str().unwrap().to_string();

    xorriso_args.extend_from_slice(&[
        "--protective-msdos-label",
        &iso_root_str,
        "-o", &iso_path_str,
    ]);

    run("xorriso", &xorriso_args);

    let limine_bin = limine_dir.join("limine");
    if limine_bin.exists() {
        run(
            limine_bin.to_str().unwrap(),
            &["bios-install", iso_path.to_str().unwrap()],
        );
        println!("Limine BIOS installed on ISO");
    }

    println!("[OK] ISO created: {}", iso_path.display());
    println!("     Size: {} KB", fs::metadata(&iso_path).unwrap().len() / 1024);

    fs::remove_dir_all(&iso_root).ok();
}

fn create_efi_image(iso_root: &Path, efi_img: &Path) {
    let efi_size = "4096"; 

    run("dd", &[
        "if=/dev/zero",
        &format!("of={}", efi_img.display()),
        "bs=1K",
        &format!("count={}", efi_size),
    ]);

    run("mkfs.fat", &["-F", "12", efi_img.to_str().unwrap()]);

    run("mmd", &["-i", efi_img.to_str().unwrap(), "::EFI"]);
    run("mmd", &["-i", efi_img.to_str().unwrap(), "::EFI/BOOT"]);

    let bootx64 = iso_root.join("EFI/BOOT/BOOTX64.EFI");
    if bootx64.exists() {
        run("mcopy", &[
            "-i", efi_img.to_str().unwrap(),
            bootx64.to_str().unwrap(),
            "::EFI/BOOT/BOOTX64.EFI",
        ]);
    }

    let bootia32 = iso_root.join("EFI/BOOT/BOOTIA32.EFI");
    if bootia32.exists() {
        run("mcopy", &[
            "-i", efi_img.to_str().unwrap(),
            bootia32.to_str().unwrap(),
            "::EFI/BOOT/BOOTIA32.EFI",
        ]);
    }
}

fn create_ext2_disk(root: &Path) {
    let disk_path = root.join("miku-os/disk.img");
    if !disk_path.exists() {
        println!("[*] Creating ext2 disk image");
        run("dd", &[
            "if=/dev/zero",
            &format!("of={}", disk_path.display()),
            "bs=1M",
            "count=16",
        ]);
        run("mkfs.ext2", &["-F", disk_path.to_str().unwrap()]);
        println!("[OK] ext2 disk: {}", disk_path.display());
    }
}

fn main() {
    println!("MikuOS ISO Builder (Pre-Release)\n");

    let root = PathBuf::from("..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(".."));

    let low_ram = ask_user("Low RAM mode? (for weak PCs) [y/N]: ", 10);

    ensure_limine(&root);

    build_kernel(&root, low_ram);

    create_iso(&root);

    create_ext2_disk(&root);

    if ask_user("\nLaunch QEMU? [y/N]: ", 10) {
        let iso_path = root.join("miku-os/miku-os.iso");
        let disk_path = root.join("miku-os/disk.img");

        let mut args = vec![
            "-cdrom".to_string(),
            iso_path.to_str().unwrap().to_string(),
            "-drive".to_string(),
            format!("format=raw,file={},if=ide,index=1", disk_path.display()),
            "-serial".to_string(),
            "stdio".to_string(),
            "-display".to_string(),
            "gtk".to_string(),
            "-m".to_string(),
            "256M".to_string(),
        ];

        let kvm_test = Command::new("qemu-system-x86_64")
            .args(["-enable-kvm", "-version"])
            .output();
        if kvm_test.map(|o| o.status.success()).unwrap_or(false) {
            args.push("-enable-kvm".to_string());
        }

        println!("\nStarting QEMU");
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        Command::new("qemu-system-x86_64")
            .args(&args_refs)
            .spawn()
            .expect("QEMU failed to start")
            .wait()
            .unwrap();
    }
}
