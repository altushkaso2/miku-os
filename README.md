<div align="center">

# Miku OS

**Экспериментальное ядро операционной системы на Rust.**
<br/>
Powered by Rust, and a couple of developers :D

<img src="docs/miku.png" width="250" alt="Miku Logo">

<br/><br/>

[![Rust](https://img.shields.io/badge/Language-Rust-39C5BB?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-39C5BB?style=flat-square)](LICENSE)
[![Build](https://img.shields.io/badge/Status-Experimental-orange?style=flat-square)]()

</div>

---

## О проекте

**Miku OS** — это UNIX-подобная операционная система, разрабатываемая с нуля в режиме `no_std`.
Мы отказались от стандартной библиотеки (`libc`), чтобы получить полный контроль над железом и архитектурой памяти.

### Статистика

![Rust](https://img.shields.io/badge/Rust-99.8%25-orange?style=flat-square&logo=rust)
![Assembly](https://img.shields.io/badge/Assembly-0.2%25-red?style=flat-square&logo=nasm)

> Почти весь код написан на Rust. Ассемблер используется исключительно для загрузчика и обработки прерываний (IDT/GDT).

---

## Функционал

### Файловая система (miku-extfs)
Изначальный упор на собственный драйвер, поддерживающий семейство ExtFS:

|Вариант | Описание |
| :--- | :--- |
| **Ext4** | Чтение современных разделов с поддержкой **Extents** (B-tree). |
| **Ext3** | **Журналирование (JBD2)**. Восстановление после сбоев питания. |
| **Ext2** | Базовая поддержка классических разделов. |
| **Write** | Создание файлов, директорий, ссылок (Hardlinks, Symlinks). |
| **VFS** | Виртуальная файловая система с **Page Cache** (кэш страниц). |

### Ядро и Система
* **Устройства:** Поддержка `/dev/null`, `/dev/zero`, `/dev/random` (PRNG), `/dev/console`.
* **Процессы:** Виртуальная фс `/proc` для отладки ядра.
* **Shell:** Интерактивная консоль с историей, цветами и редактированием.
* **Память:** Slab Allocator для эффективного управления объектами.

---

## Запуск

Для сборки требуются `Rust (nightly)`, `QEMU` и `bootimage`.

```bash
# 1. Клонирование
git clone [https://github.com/your-username/miku-os.git](https://github.com/your-username/miku-os.git)
cd miku-os

# 2. Запуск (запускается в директории builder)
cargo run