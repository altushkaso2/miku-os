<div align="center">

# 💙 Miku OS

**Экспериментальное ядро операционной системы на Rust**

*Powered by Rust, and a couple of developers :D*

<img src="docs/miku.png" width="220" alt="Miku Logo">

[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/arch-x86__64-blue.svg)]()
[![Status](https://img.shields.io/badge/status-release-green.svg)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey.svg)]()

</div>

---

> 🌐 **Документация:** [🇷🇺 Русский](docs/Russian_documentation.md) | [🇬🇧 English](docs/English_documentation.md) | [🇯🇵 日本語](docs/Japanese_documentation.md)

---

## О проекте

**Miku OS** - UNIX-подобная операционная система, разрабатываемая с нуля в режиме `no_std`.
Без стандартной библиотеки (`libc`) - полный контроль над железом и архитектурой памяти.

> Весь код написан на Rust. Ассемблер используется исключительно для загрузчика, syscall-обработчика и переключения контекста.

---

## Технические характеристики

### Ядро

| Компонент | Описание |
|:--|:--|
| **Архитектура** | x86_64, `#![no_std]`, `#![no_main]` |
| **Bootloader** | Limine protocol, фреймбуфер 1280x800 (BGR) |
| **Защита** | GDT + TSS + IST для double fault, ring 0 / ring 3 |
| **Прерывания** | IDT - timer, keyboard, page fault, GPF, double fault |
| **PIC** | PIC8259 (offset 32/40) |
| **Куча** | 256 KB, linked-list allocator |
| **Syscall** | SYSCALL/SYSRET через MSR, обработчик на naked asm |

---

### Управление памятью

<details>
<summary><b>Физическая память (PMM)</b></summary>

#### Аллокатор фреймов

- **Bitmap-аллокатор** - до 4M фреймов (16 GB RAM), каждый бит = один фрейм 4KB
- **free_hint** и **contiguous_hint** - ускоряют поиск свободных и смежных фреймов
- **Contiguous alloc** - выделение N смежных фреймов за один запрос
- **Регионы** - динамическая регистрация диапазонов RAM из Multiboot2 memory map

#### Emergency Pool

Специальный резерв фреймов исключительно для swap-in внутри page fault handler:

| Параметр | Значение |
|:--|:--|
| **Размер пула** | 64 фрейма (256 KB) |
| **Назначение** | Только для swap-in в page fault handler |
| **Пополнение** | Timer ISR каждые ~250ms через `refill_emergency_pool_tick()` |
| **Причина** | Обычный evict_one() вызывает ATA I/O - нельзя использовать внутри fault handler |

```
alloc_frame()           - обычный alloc из PMM
alloc_frame_emergency() - только из emergency pool (для fault handler)
alloc_or_evict()        - alloc + evict если RAM закончилась
alloc_for_swapin()      - только emergency pool (fault context)
```

</details>

<details>
<summary><b>Виртуальная память (VMM)</b></summary>

- **4-уровневые page tables** (PML4 -> PDP -> PD -> PT)
- **HHDM** - Higher Half Direct Map для доступа к физической памяти из ядра
- **mark_swapped()** - запись swap PTE при выгрузке страницы
- Поддержка ring 0 / ring 3 mapping

</details>

<details>
<summary><b>Своп (Swap)</b></summary>

Полная реализация swap на блочном устройстве (ATA диск):

#### Reverse Mapping (swap_map)

- Для каждого физического фрейма хранится `(cr3, virt_addr, age, pinned)`
- Отслеживает до 512K фреймов (2 GB RAM)

#### Алгоритм вытеснения - Clock Sweep

```
Pass 1: ищет фрейм с age >= 3 (самый старый)
Pass 2: аварийный - берёт любой unpinned фрейм
```

- `touch(phys)` - сбрасывает age в 1 при обращении к странице
- `age_all()` - увеличивает age всем фреймам (вызывается по таймеру)

#### Кодирование Swap PTE

```
bit 0     = 0  (PRESENT=0 - страница не в памяти)
bit 1     = 1  (SWAP_MARKER - отличает от unmapped)
bits 12.. = номер swap слота
```

#### Поток вытеснения

```
evict_one():
  1. pick_victim() из swap_map
  2. swap_out_internal() -> записать страницу на диск
  3. vmm::mark_swapped() -> обновить PTE
  4. swap_map::untrack() -> убрать из reverse map
  5. pmm::free_frame() -> вернуть фрейм
```

</details>

---

### Планировщик

| Параметр | Значение |
|:--|:--|
| **Тип** | Round-robin, preemptive |
| **Процессы** | До 16 одновременно |
| **Переключение** | Каждые 20 тиков таймера (~200ms) |
| **Контекст** | r15, r14, r13, r12, rbx, rbp, rip, rsp, rflags |
| **Стек** | 16 KB на процесс |
| **Состояния** | `Ready`, `Running`, `Dead` |

Переключение контекста реализовано на naked asm - полное сохранение и восстановление регистров без участия компилятора.

---

### Системные вызовы

Реализованы через `SYSCALL/SYSRET` (MSR), обработчик на naked asm с `swapgs` для переключения стеков.

| Nr | Имя | Описание |
|:--:|:--|:--|
| **0** | `sys_write` | Запись в stdout/stderr (fd 1/2), до 4096 байт |
| **1** | `sys_read` | Чтение (заглушка) |
| **2** | `sys_exit` | Завершение процесса + yield |
| **3** | `sys_sleep` | Сон на N тиков |
| **4** | `sys_getpid` | Получить PID текущего процесса |

---

### Сетевой стек

Полный сетевой стек реализован с нуля, без каких-либо сторонних библиотек.

<details>
<summary><b>Драйверы сетевых карт</b></summary>

| Драйвер | Чипы |
|:--|:--|
| **Intel E1000** | 82540EM, 82545EM, 82574L, 82579LM, I217 |
| **Realtek RTL8139** | RTL8139 |
| **Realtek RTL8168** | RTL8168, RTL8169 |
| **VirtIO Net** | QEMU/KVM виртуальная сетевая карта |

Все драйверы обнаруживаются автоматически через PCI-сканер.

</details>

<details>
<summary><b>Протоколы</b></summary>

| Уровень | Протоколы |
|:--|:--|
| **L2** | Ethernet, ARP (таблица с кэшем) |
| **L3** | IPv4, ICMP |
| **L4** | UDP, TCP (с состоянием соединения) |
| **Прикладной** | DHCP, DNS, NTP, HTTP, Traceroute |
| **Безопасность** | TLS 1.2 (RSA + AES-128-CBC + SHA) |

</details>

<details>
<summary><b>TLS 1.2 - полная реализация с нуля</b></summary>

- **RSA** - парсинг ASN.1/DER сертификатов, PKCS#1 шифрование
- **BigNum** - собственная реализация арифметики больших чисел для RSA 2048-bit
- **AES-128-CBC** - симметричное шифрование
- **SHA-1, SHA-256, HMAC** - хеширование и аутентификация
- **PRF** - деривация ключей по RFC 5246
- **Handshake** - полный цикл: ClientHello -> Certificate -> ClientKeyExchange -> Finished

Проверено на реальном Google (TLS RSA 2048, порт 443).

</details>

---

### VFS (Virtual File System)

<details>
<summary><b>Развернуть</b></summary>

#### Основное
- **64 VNode** с полной метадатой - права, uid/gid, timestamps, размер, nlinks
- **Типы нод**: `Regular`, `Directory`, `Symlink`, `CharDevice`, `BlockDevice`, `Pipe`, `Fifo`, `Socket`
- **32 открытых файла** одновременно, **8 точек монтирования**

#### Кэширование
- **Page Cache** - 32 страницы x 512 байт, LRU вытеснение
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

#### Продвинутые возможности
- **Журнал VFS** - 16 записей операций
- **Транзакции** - 4 одновременных с откатом
- **Xattr** - 8 расширенных атрибутов на ноду (16 байт имя, 32 байт значение)
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
| **ext4** | `/mnt` | Extent-based файлы + crc32c checksums |

---

### MikuFS - Ext2/3/4 драйвер

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

#### Ext3 журнал (JBD2)
- Создание журнала (`ext2 -> ext3` конвертация)
- Запись транзакций: descriptor blocks, commit blocks, revoke blocks
- Recovery - replay незавершённых транзакций при монтировании

#### Утилиты
- `fsck` - проверка целостности
- `tree` - визуализация дерева директорий
- `du`, `cp`, `mv`, `chmod`, `chown`, hardlink

</details>

---

### Shell

| Фича | Описание |
|:--|:--|
| **Ввод** | Посимвольная обработка, вставка в середину строки |
| **Навигация** | `<- -> Home End Delete Backspace` |
| **История** | 16 команд, навигация `Up Down` |
| **Цвета** | miku-тематика: бирюзовый, розовый, белый |
| **Шрифт** | Кастомный bitmap 9x16 + noto-sans-mono fallback |
| **Консоль** | Framebuffer рендеринг, автоскролл, RGB per-character |

---

### Консоль и фреймбуфер

<details>
<summary><b>Развернуть</b></summary>

Рендеринг реализован полностью вручную, без каких-либо графических библиотек:

- **Двойной рендер** - кастомные bitmap-глифы 9x16 + noto-sans-mono как fallback
- **Shadow buffer** - построчный u32-буфер для ускорения blit операций (bpp=4)
- **Поддержка BGR/RGB** - автоопределение порядка байт фреймбуфера
- **Скроллинг** - memmove строк пикселей + очистка последней строки
- **Цвет на символ** - каждый Cell хранит `(ch, r, g, b)` отдельно
- **Курсор** - двухпиксельный вертикальный курсор с кастомным цветом
- **COLOR_MIKU** 💙 - фирменный бирюзовый цвет по умолчанию

</details>

---

### ATA драйвер

| Параметр | Значение |
|:--|:--|
| **Режим** | PIO (Programmed I/O) |
| **Операции** | Read / Write секторов (512 байт) |
| **Диски** | 4 шт: Primary/Secondary x Master/Slave |
| **Защита** | Cache flush после записи, timeout 500K итераций |

---

## Команды

Полный список команд доступен в **[Wiki проекта](https://github.com/altushkaso2/miku-os/wiki)**.

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
[2/5] Создание файловой структуры (ввести размер диска и своп файла)
[3/5] Генерация системного образа (miku-os.iso)
[4/5] Подготовка диска
[5/5] Запуск QEMU (по желанию (y/N))
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
  <sub>Ядро · VFS · MikuFS · Shell · Сеть · TLS · Планировщик · PMM · VMM · Swap</sub>
</div>

---

## От автора

> Всё началось с простой мысли - "а что если взять и написать свою операционную систему?".
> С тех пор это стало хобби. Каждый вечер - новая функция, новый баг, новое открытие.
> От первого символа на экране до полноценного TLS-стека и планировщика - всё написано вручную,
> без готовых библиотек и обёрток. Только Rust, документация и упорство :D
>
> Проект живёт и развивается. Впереди - ELF загрузчик, userspace, пользовательские процессы.
> Но это уже следующая глава, которая ждёт Miku OS :)

<div align="center">

**Miku OS** - чистая OS на Rust, написанная с нуля

*С любовью 💙*

<img src="docs/miku.png" width="70" alt="Miku">

Если проект понравился - поставьте звезду! ⭐

</div>
