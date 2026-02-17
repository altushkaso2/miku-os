use bootloader::DiskImageBuilder;
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
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
        Err(_) => {
            println!("\nАвтоматический выбор: N");
            false
        }
    }
}

fn main() {
    let ultra_low_ram = ask_user("Включить режим экономии ОЗУ? (Для слабых пк) [y/N]: ", 10);

    let mut cmd = Command::new("cargo");
    cmd.current_dir("..")
       .arg("build")
       .arg("--target").arg("x86_64-unknown-none")
       .arg("-Z").arg("build-std=core,compiler_builtins,alloc")
       .arg("-Z").arg("build-std-features=compiler-builtins-mem");

    if ultra_low_ram {
        cmd.arg("--jobs").arg("1");
        cmd.env("RUSTFLAGS", "-C codegen-units=1");
    }

    let status = cmd.status().expect("Ошибка cargo build");
    if !status.success() { panic!("Ошибка компиляции"); }

    let kernel_path = PathBuf::from("../target/x86_64-unknown-none/debug/miku-os-release");
    let os_dir = PathBuf::from("miku-os");
    if !os_dir.exists() { fs::create_dir(&os_dir).unwrap(); }

    let image_path = os_dir.join("system.img");
    let disk_path = os_dir.join("disk.img");

    let builder = DiskImageBuilder::new(kernel_path);
    builder.create_bios_image(&image_path).unwrap();

    if !disk_path.exists() {
        Command::new("dd").args(["if=/dev/zero", &format!("of={}", disk_path.display()), "bs=1M", "count=16"]).status().unwrap();
        Command::new("mkfs.ext2").args(["-F", disk_path.to_str().unwrap()]).status().unwrap();
    }

    if ask_user("Запустить QEMU? [y/N]: ", 10) {
        let mut qemu = Command::new("qemu-system-x86_64");
        qemu.args([
            "-drive", &format!("format=raw,file={}", image_path.display()),
            "-drive", &format!("format=raw,file={},if=ide,index=2", disk_path.display()),
            "-serial", "stdio",
            "-display", "gtk",
            "-enable-kvm"
        ]);
        qemu.spawn().expect("Ошибка QEMU").wait().unwrap();
    }
}
