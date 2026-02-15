use bootloader::DiskImageBuilder;
use std::{path::PathBuf, process::Command, fs};

fn main() {
    println!(" [1/5] Компиляция ядра miku-os");

    let status = Command::new("cargo")
        .current_dir("..")
        .arg("build")
        .arg("--target").arg("x86_64-unknown-none")
        .arg("-Z").arg("build-std=core,compiler_builtins,alloc")
        .arg("-Z").arg("build-std-features=compiler-builtins-mem")
        .status()
        .expect("Не удалось запустить cargo build");

    if !status.success() {
        panic!("Ошибка компиляции ядра");
    }

    let kernel_path = PathBuf::from("../target/x86_64-unknown-none/debug/miku-os-release");
    if !kernel_path.exists() {
        panic!("Файл ядра не найден");
    }

    println!(" [2/5] Создание файловой структуры (miku-os)");

    let os_dir = PathBuf::from("miku-os");
    if !os_dir.exists() {
        fs::create_dir(&os_dir).expect("Не удалось создать папку miku-os");
    }

    let image_path = os_dir.join("system.img");

    println!(" [3/5] Генерация системного образа в {}...", image_path.display());
    let builder = DiskImageBuilder::new(kernel_path);
    builder.create_bios_image(&image_path).unwrap();

    println!(" [4/5] Подготовка ext2 диска");

    let disk_path = os_dir.join("disk.img");
    if !disk_path.exists() {
        let status = Command::new("dd")
            .arg("if=/dev/zero")
            .arg(format!("of={}", disk_path.display()))
            .arg("bs=1M")
            .arg("count=16")
            .status()
            .expect("Не удалось создать disk.img");

        if !status.success() {
            panic!("Ошибка создания disk.img!");
        }

        let status = Command::new("mkfs.ext2")
            .arg("-F")
            .arg(disk_path.to_str().unwrap())
            .status()
            .expect("Не удалось запустить mkfs.ext2");

        if !status.success() {
            panic!("Ошибка форматирования ext2!");
        }

        println!("    ext2 диск успешно создан: {}", disk_path.display());
    } else {
        println!("    ext2 диск уже существует: {}", disk_path.display());
    }

    println!(" [5/5] Запуск miku-os");
    let mut cmd = Command::new("qemu-system-x86_64");

    cmd.arg("-drive").arg(format!("format=raw,file={}", image_path.display()));
    cmd.arg("-drive").arg(format!("format=raw,file={},if=ide,index=2", disk_path.display()));
    cmd.arg("-serial").arg("stdio");
    cmd.arg("-display").arg("gtk");
    cmd.arg("-enable-kvm");

    let mut child = cmd.spawn().expect("Не удалось запустить QEMU");
    child.wait().unwrap();
}
