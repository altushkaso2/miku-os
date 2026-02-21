<div align="center">

# 💙 Miku OS

**Экспериментальная операционная система на Rust**

*Powered by Rust, and a couple of developers :D*

<img src="docs/miku.png" width="220" alt="Miku Logo">

[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/arch-x86__64-blue.svg)]()
[![Status](https://img.shields.io/badge/status-pre--release-yellow.svg)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey.svg)]()

</div>

---

## О проекте

**Miku OS** - UNIX-подобная операционная система, разрабатываемая с нуля в режиме `no_std`.  
Без стандартной библиотеки (`libc`) - полный контроль над железом и архитектурой памяти.

> Весь код написан на Rust. Ассемблер используется исключительно для загрузчика и обработки прерываний (IDT/GDT).

---

## Технические характеристики

### Ядро

| Компонент | Описание |
|:--|:--|
| **Архитектура** | x86_64 bare-metal, `#![no_std]`, `#![no_main]` |
| **Bootloader** | Bootloader API, фреймбуфер 1280×720 (BGR) |
| **Защита** | GDT + TSS + IST для double fault |
| **Прерывания** | IDT - timer, keyboard, page fault, GPF, double fault |
| **PIC** | PIC8259 (offset 32/40) |
| **Куча** | 256 KB, linked-list allocator |

---

### VFS (Virtual File System)

<details>
<summary><b>Развернуть</b></summary>

#### Основное
- **64 VNode** с полной метадатой - права, uid/gid, timestamps, размер, nlinks
- **Типы нод**: `Regular`, `Directory`, `Symlink`, `CharDevice`, `BlockDevice`, `Pipe`, `Fifo`, `Socket`
- **32 открытых файла** одновременно, **8 точек монтирования**

#### Кэширование
- **Page Cache** - 32 страницы × 512 байт, LRU вытеснение
- **Dentry Cache** - 128 записей, FNV32 хеширование, hit/miss статистика
- **Slab Allocator** - быстрое выделение страниц

#### Навигация
- **Path walking** - глубина до 32 компонентов
- **Symlink resolution** - защита от циклов (8 уровней)
- **FNV32 хеширование** имён для O(1) lookup

#### Безопасность
- UNIX-модель прав: `owner/group/other`, `setuid/setgid/sticky`
- Security labels (MAC), квоты по байтам и inodes
- File locking: shared/exclusive с deadlock detection

#### Продвинутые фичи
- **Журнал VFS** - 16 записей операций
- **Транзакции** - 4 одновременных с откатом
- **Xattr** - 8 расширенных атрибутов на ноду
- **Notify events** - inotify-подобная подсистема
- **Version store** - 16 снапшотов файлов
- **CAS store** - content-addressable дедупликация
- **Block I/O queue** - 8 асинхронных запросов

</details>

---

### Файловые системы

| FS | Точка монтирования | Описание |
|:--:|:--:|:--|
| **tmpfs** | `/` | RAM-based корневая FS |
| **devfs** | `/dev` | Устройства: `null`, `zero`, `random`, `urandom`, `console` |
| **procfs** | `/proc` | `version`, `uptime`, `meminfo`, `mounts`, `cpuinfo`, `stat` |
| **ext2** | `/mnt` | Полное чтение и запись реального диска |
| **ext3** | `/mnt` | Журналирование поверх ext2 (JBD2) |
| **ext4** | `/mnt` | Extent-based файлы |

---

### MikuFS - Ext2/3/4 драйвер

Собственный драйвер без сторонних зависимостей.

<details>
<summary><b>Развернуть</b></summary>

#### Чтение
- Superblock, group descriptors, inodes, directory entries
- Indirect blocks (single / double / triple)
- Ext4 extent tree

#### Запись
- Создание/удаление файлов, директорий, симлинков
- Bitmap allocator для блоков и inodes (preferred group)
- Рекурсивное удаление

#### Ext3 журнал
- Создание журнала (`ext2 → ext3` конвертация)
- Запись транзакций, commit, abort
- Recovery - replay незавершённых транзакций
- Clean - пометка журнала чистым

#### Утилиты
- `fsck` - проверка целостности (magic, root inode, groups, orphans)
- `tree` - визуализация дерева директорий
- `du` - подсчёт размера поддерева
- `cp`, `mv`, `chmod`, `chown`, hardlink

#### Интеграция с VFS
- **Lazy loading** - VNode создаётся при первом обращении
- **Прозрачный доступ** - `ls /mnt`, `cat /mnt/file`, `cd /mnt`
- **mount/umount** с корректным eviction

</details>

---

### Shell

| Фича | Описание |
|:--|:--|
| **Ввод** | Посимвольная обработка, вставка в середину строки |
| **Навигация** | `← → Home End Delete Backspace` |
| **История** | 16 команд, навигация `↑ ↓` |
| **Цвета** | Miku-тематика: бирюзовый, розовый, белый |
| **Шрифт** | Кастомный bitmap 9×16 + noto-sans-mono fallback |
| **Консоль** | Framebuffer рендеринг, автоскролл, RGB per-character |

---

### ATA драйвер

| Параметр | Значение |
|:--|:--|
| **Режим** | PIO (Programmed I/O) |
| **Операции** | Read / Write секторов (512 байт) |
| **Диски** | 4 шт: Primary/Secondary × Master/Slave |
| **Защита** | Cache flush после записи, timeout 500K итераций |

---

## Команды

### Навигация и файлы

| Команда | Описание | Пример |
|:--|:--|:--|
| `ls [path]` | Список файлов | `ls /dev` |
| `cd <path>` | Перейти в директорию | `cd /mnt` |
| `pwd` | Текущая директория | `pwd` |
| `mkdir <name>` | Создать директорию | `mkdir mydir` |
| `touch <name>` | Создать пустой файл | `touch file.txt` |
| `cat <file>` | Показать содержимое | `cat hello.txt` |
| `write <file> <text>` | Записать текст в файл | `write hello.txt Hello!` |
| `stat <path>` | Информация о файле | `stat /proc/version` |
| `rm <file>` | Удалить файл | `rm old.txt` |
| `rm -rf <path>` | Рекурсивное удаление | `rm -rf mydir` |
| `rmdir <dir>` | Удалить пустую директорию | `rmdir empty` |
| `mv <old> <new>` | Переименовать / переместить | `mv a.txt b.txt` |

### Ссылки и права

| Команда | Описание | Пример |
|:--|:--|:--|
| `ln -s <target> <link>` | Создать симлинк | `ln -s /mnt/file link` |
| `ln <existing> <new>` | Создать жёсткую ссылку | `ln file.txt hardlink` |
| `readlink <path>` | Показать цель симлинка | `readlink link` |
| `chmod <mode> <path>` | Изменить права доступа | `chmod 755 script` |

### Монтирование

| Команда | Описание | Пример |
|:--|:--|:--|
| `mount` | Список точек монтирования | `mount` |
| `mount ext2 <path>` | Монтировать ext2 диск | `mount ext2 /mnt` |
| `umount <path>` | Размонтировать | `umount /mnt` |
| `df` | Использование дискового пространства | `df` |

### Система

| Команда | Описание | Пример |
|:--|:--|:--|
| `info` | Информация об OS и uptime | `info` |
| `help` | Список всех команд | `help` |
| `history` | История введённых команд | `history` |
| `echo <text>` | Вывести текст | `echo Hello` |
| `clear` | Очистить экран | `clear` |
| `heap` | Статистика кучи (used/free) | `heap` |
| `poweroff` / `shutdown` / `halt` | Выключить систему | `poweroff` |
| `reboot` / `restart` | Перезагрузить систему | `reboot` |

---

### Ext2 - работа с диском

| Команда | Описание | Пример |
|:--|:--|:--|
| `ext2mount` | Найти и смонтировать ext2 диск | `ext2mount` |
| `ext2info` | Информация о файловой системе | `ext2info` |
| `ext2ls [path]` | Список файлов | `ext2ls /home` |
| `ext2cat <path>` | Прочитать файл | `ext2cat /etc/hosts` |
| `ext2stat <path>` | Информация об inode | `ext2stat /bin` |
| `ext2write <path> <text>` | Записать в файл | `ext2write /tmp/log hello` |
| `ext2append <path> <text>` | Дописать в файл | `ext2append /tmp/log world` |
| `ext2mkdir <path>` | Создать директорию | `ext2mkdir /home/miku` |
| `ext2rm <path>` | Удалить файл | `ext2rm /tmp/old` |
| `ext2rm -rf <path>` | Рекурсивное удаление | `ext2rm -rf /tmp` |
| `ext2rmdir <path>` | Удалить пустую директорию | `ext2rmdir /empty` |
| `ext2mv <src> <dst>` | Переименовать / переместить | `ext2mv old.txt new.txt` |
| `ext2cp <src> <dst>` | Копировать файл | `ext2cp /a.txt /b.txt` |
| `ext2ln -s <tgt> <name>` | Создать симлинк | `ext2ln -s /bin sh` |
| `ext2link <existing> <name>` | Создать жёсткую ссылку | `ext2link file.txt hard` |
| `ext2chmod <mode> <path>` | Изменить права | `ext2chmod 644 /file` |
| `ext2chown <uid> <gid> <path>` | Изменить владельца | `ext2chown 0 0 /file` |
| `ext2du [path]` | Размер директории | `ext2du /home` |
| `ext2tree [path]` | Дерево директорий | `ext2tree /` |
| `ext2fsck` | Проверка целостности FS | `ext2fsck` |
| `ext2cache` | Статистика блочного кэша | `ext2cache` |
| `ext2cacheflush` | Сбросить блочный кэш | `ext2cacheflush` |

### Ext3 - журналирование

| Команда | Описание | Пример |
|:--|:--|:--|
| `ext3mkjournal` | Создать журнал (ext2 → ext3) | `ext3mkjournal` |
| `ext3info` | Информация о журнале | `ext3info` |
| `ext3journal` | Показать транзакции | `ext3journal` |
| `ext3clean` | Пометить журнал чистым | `ext3clean` |
| `ext3recover` | Восстановить из журнала | `ext3recover` |

### Ext4 - расширенные возможности

| Команда | Описание | Пример |
|:--|:--|:--|
| `ext4info` | Информация о ext4 (extents, journal, checksums) | `ext4info` |
| `ext4extents` | Включить поддержку extent tree | `ext4extents` |
| `ext4checksums` | Верификация crc32c checksums | `ext4checksums` |
| `ext4extinfo <path>` | Extent tree конкретного файла | `ext4extinfo /mnt/file` |

---

## Быстрый старт

```bash
# Посмотреть систему
info
cat /proc/version
cat /proc/uptime
cat /proc/meminfo
ls /dev

# Поработать с файлами
mkdir test
write test/hello.txt Привет мику!
cat test/hello.txt
ls test
rm -rf test

# Подключить ext2 диск
ext2mount
mount ext2 /mnt
ls /mnt
ext2tree /

# Проверить журнал
ext3info
ext2fsck
ext2cache
```

---

## Сборка и запуск

### Требования

| Инструмент | Зачем |
|:--|:--|
| **Rust nightly** | `no_std` + нестабильные возможности компилятора |
| **QEMU** | Эмуляция x86_64 машины |
| **Cargo** | Сборка builder'а и ядра |

### Запуск

```bash
git clone https://github.com/altushkaso2/miku-os
cd miku-os/builder
cargo run
```

Builder делает всё сам:

```
Экономия оперативной памяти? (y/N)
[1/5] Компиляция ядра miku-os
[2/5] Создание файловой структуры
[3/5] Генерация системного образа (system.img)
[4/5] Подготовка ext2 диска (disk.img)
[5/5] Запуск QEMU (по желанию (y/N) )
```

> Первая сборка займёт пару минут - скачиваются зависимости и компилируется ядро.  
> Последующие запуски - секунды.

---

## Авторы

<div align="center">
  <a href="https://github.com/altushkaso2">
    <img src="https://github.com/altushkaso2.png" width="100" style="border-radius:50%;" alt="altushkaso2">
  </a>
  <br><br>
  <a href="https://github.com/altushkaso2"><b>@altushkaso2</b></a>
  <br>
  <sub>Создатель и единственный разработчик Miku OS</sub>
  <br>
  <sub>Ядро · VFS · MikuFS · Shell</sub>
</div>

---

## От автора

> Всё началось с простой мысли - «а что если взять и написать свою операционную систему?».  
> С тех пор это стало хобби. Каждый вечер - новая функция, новый баг, новое открытие.  
> От первого символа на экране до полноценного ext2 драйвера - всё написано вручную,  
> без готовых FS библиотек и обёрток. Только Rust, документация и упорство :D  
>
> Проект живёт и развивается. Впереди - сети, многозадачность, пользовательское пространство.  
> Но это уже следующая глава, которая ждёт Miku OS :)

<div align="center">

**Miku OS** - чистая OS на Rust, написанная с нуля

*С любовью 💙*

<img src="docs/miku.png" width="70" alt="Miku">

Если проект понравился — поставьте звезду! ⭐

</div>
