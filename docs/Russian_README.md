<div align="center">

# Miku OS

**Экспериментальное ядро операционной системы на Rust**

*Работает на Rust и нескольких разработчиках :D*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">

[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/arch-x86__64-blue.svg)]()
[![Status](https://img.shields.io/badge/status-release-green.svg)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey.svg)]()

</div>

---

> **Документация:** [Russian](docs/Russian_README.md) | [English](docs/English_README.md) | [Japanese](Japanese_README.md)

---

## О проекте

**Miku OS** это UNIX-подобная операционная система, разработанная с нуля в `no_std` окружении.
Не использует стандартную библиотеку (`libc`), полностью контролирует железо и архитектуру памяти.
ELF динамическая линковка, разделяемые библиотеки и userspace процессы реализованы с нуля.

> Весь код написан на Rust. Ассемблер используется только для загрузчика, обработчика syscall и переключения контекста.

---

## Технические характеристики

### Ядро

| Компонент | Описание |
|:--|:--|
| **Архитектура** | x86_64, `#![no_std]`, `#![no_main]` |
| **Загрузчик** | GRUB2 + Multiboot2, фреймбуфер (BGR/RGB автоопределение) |
| **Защита** | GDT + TSS + IST (double fault, page fault, GPF), ring 0 / ring 3 |
| **Прерывания** | IDT: таймер, клавиатура, page fault, GPF, #UD, #NM, double fault |
| **PIC** | PIC8259 (смещение 32/40) |
| **SSE** | CR0.EM=0, CR0.MP=1, CR4.OSFXSR=1, CR4.OSXMMEXCPT=1 |
| **Куча** | 32 MB, linked list аллокатор |
| **Syscall** | SYSCALL/SYSRET через MSR, naked asm обработчик, сохранение R8/R9/R10 |

---

### ELF загрузчик и динамическая линковка

<details>
<summary><b>ELF загрузчик</b></summary>

#### Возможности

| Возможность | Описание |
|:--|:--|
| **Форматы** | ET_EXEC (статический), ET_DYN (PIE) |
| **Сегменты** | PT_LOAD, PT_INTERP, PT_DYNAMIC, PT_TLS, PT_GNU_RELRO, PT_GNU_STACK |
| **Релокации** | R_X86_64_RELATIVE, R_X86_64_JUMP_SLOT, R_X86_64_GLOB_DAT, R_X86_64_64 |
| **Безопасность** | W^X enforcement (запрет W+X сегментов), RELRO |
| **ASLR** | 20-бит энтропия для PIE (RDRAND + TSC fallback) |
| **Стек** | SysV ABI: argc, argv, envp, auxv (16-байт выравнивание) |
| **TLS** | Thread Local Storage (через FS.base регистр) |

#### Модульная структура

| Модуль | Описание |
|:--|:--|
| **elf_loader.rs** | Парсинг ELF, маппинг сегментов |
| **exec_elf.rs** | Создание процесса, построение стека |
| **dynlink.rs** | Динамическая линковка (делегирует в reloc.rs) |
| **reloc.rs** | Унифицированный движок релокаций |
| **vfs_read.rs** | Унифицированное чтение файлов (VFS + ext2) |
| **random.rs** | RDRAND/TSC случайные числа, ASLR |

#### Записи auxv

| Ключ | Описание |
|:--|:--|
| AT_PHDR | Виртуальный адрес заголовков программы |
| AT_PHENT | Размер записи заголовка |
| AT_PHNUM | Количество заголовков |
| AT_PAGESZ | Размер страницы (4096) |
| AT_ENTRY | Точка входа исполняемого файла |
| AT_BASE | Базовый адрес интерпретатора |
| AT_RANDOM | 16 байт случайных данных |

</details>

<details>
<summary><b>ld-miku (динамический линкер)</b></summary>

#### Обзор

`ld-miku` это ELF динамический линкер для MikuOS. Написан на Rust в `#![no_std]` окружении,
компилируется как статический PIE бинарь.

#### Процесс загрузки

```
1. Ядро загружает ELF -> обнаруживает PT_INTERP
2. ld-miku.so маппится из INCLUDE_BYTES в память
3. ld-miku запускается -> парсит auxv (AT_PHDR/AT_ENTRY)
4. Определяет необходимые библиотеки из DT_NEEDED
5. Маппит разделяемые библиотеки через SYS_MAP_LIB syscall
6. Применяет PLT/GOT релокации
7. Экспортирует символы в глобальную таблицу
8. Выполняет DT_INIT / DT_INIT_ARRAY
9. Прыжок на точку входа исполняемого файла
```

#### Особенности

- Глобальная таблица символов (до 1024 символов)
- Разрешение weak символов
- Рекурсивная загрузка зависимостей (до 16 библиотек)
- Поддержка R_X86_64_COPY релокаций
- DT_HASH / DT_GNU_HASH для точного подсчета символов
- Корректный пропуск envp при парсинге auxv

</details>

<details>
<summary><b>Разделяемые библиотеки (solib)</b></summary>

#### Глобальный кэш библиотек

| Параметр | Значение |
|:--|:--|
| **Макс. кэш** | 32 библиотеки |
| **Пути поиска** | /lib, /usr/lib |
| **Маппинг страниц** | Все сегменты копируются для каждого процесса |
| **OOM защита** | Прерывание parse_and_prepare при OOM без кэширования битых данных |

#### SYS_MAP_LIB syscall (nr=15)

Ядро парсит ELF сегменты и маппит разделяемую библиотеку напрямую в адресное пространство процесса.

- Read-only сегменты -> приватная копия из кэша
- Writable сегменты -> новая аллокация для каждого процесса
- Откат при неудаче map_page

#### Системные библиотеки

`libmiku.so` встроена в ядро через `include_bytes!` и регистрируется в кэше при старте через `solib::preload`.

#### Команды оболочки

| Команда | Описание |
|:--|:--|
| `ldconfig` | Сканирование /lib и /usr/lib, обновление кэша |
| `ldd` | Список кэшированных библиотек |

</details>

---

### libmiku.so (стандартная библиотека)

<details>
<summary><b>Развернуть</b></summary>

#### Обзор

libmiku это C-совместимая стандартная библиотека для MikuOS. Написана на Rust, экспортирует 79 функций в 12 модулях.
Загружается динамически через ld-miku, используется всеми userspace программами.

#### Модульная структура

```
src/lib/libmiku/
├── lib.rs       объявления модулей, точка входа, panic handler
├── sys.rs       syscall примитивы (sc0..sc4), константы
├── proc.rs      exit, getpid, brk, mmap, munmap, tls
├── io.rs        write, read, print, println, readline
├── mem.rs       memset, memcpy, memmove, memcmp
├── num.rs       itoa, utoa, atoi, print_int, print_hex
├── string.rs    strlen, strcmp, strcpy, strtok, strtol...
├── heap.rs      malloc, free, realloc, calloc
├── file.rs      open, close, seek, fsize, read_file
├── time.rs      sleep, uptime
├── util.rs      abs, min, max, rand, assert, panic
└── fmt.rs       printf, snprintf (asm трамплины)
```

#### Модуль: io (ввод/вывод)

| Функция | Описание |
|:--|:--|
| `miku_write(fd, buf, len)` | Запись в fd |
| `miku_read(fd, buf, len)` | Чтение из fd |
| `miku_print(str)` | Вывод строки |
| `miku_println(str)` | Вывод строки + перенос |
| `miku_puts(str)` | Совместимость с puts |
| `miku_putchar(c)` | Вывод 1 байта |
| `miku_getchar()` | Ввод 1 байта |
| `miku_readline(buf, max)` | Ввод строки (фикс. буфер) |
| `miku_getline()` | Ввод строки (malloc, нужен free) |

#### Модуль: string (строки)

| Функция | Описание |
|:--|:--|
| `miku_strlen` | Длина строки |
| `miku_strcmp` / `miku_strncmp` | Сравнение строк |
| `miku_strcpy` / `miku_strncpy` | Копирование строк |
| `miku_strcat` / `miku_strncat` | Конкатенация строк |
| `miku_strchr` / `miku_strrchr` | Поиск символа |
| `miku_strstr` | Поиск подстроки |
| `miku_strdup` | Дублирование строки (malloc) |
| `miku_toupper` / `miku_tolower` | Преобразование регистра |
| `miku_isdigit` / `miku_isalpha` / `miku_isalnum` / `miku_isspace` | Классификация символов |
| `miku_strtok` | Токенизация (stateful) |
| `miku_strpbrk` | Поиск набора символов |
| `miku_strspn` / `miku_strcspn` | Длина префикса |
| `miku_strtol` / `miku_strtoul` | Строка в число (base 0/8/10/16) |
| `miku_strlcpy` / `miku_strlcat` | BSD безопасные копирование/конкатенация |

#### Модуль: num (числа)

| Функция | Описание |
|:--|:--|
| `miku_itoa(val, buf)` | Целое в строку |
| `miku_utoa(val, buf)` | Беззнаковое в строку |
| `miku_atoi(str)` | Строка в целое |
| `miku_print_int(val)` | Вывод десятичного |
| `miku_print_hex(val)` | Вывод 0x... |

#### Модуль: mem (память)

| Функция | Описание |
|:--|:--|
| `miku_memset` | Заполнение (8-байт выравнивание) |
| `miku_memcpy` | Копирование (8-байт выравнивание) |
| `miku_memmove` | Копирование (с перекрытием) |
| `miku_memcmp` | Сравнение |
| `miku_bzero` | Обнуление |

#### Модуль: heap (динамическая память)

| Функция | Описание |
|:--|:--|
| `miku_malloc(size)` | Выделение памяти |
| `miku_free(ptr)` | Освобождение |
| `miku_realloc(ptr, size)` | Изменение размера |
| `miku_calloc(count, size)` | Выделение с обнулением |

Реализация: mmap-based slab аллокатор. < 32KB из 128KB slab, >= 32KB через mmap/munmap.

#### Модуль: fmt (форматированный вывод)

| Функция | Описание |
|:--|:--|
| `miku_printf(fmt, ...)` | Форматированный вывод |
| `miku_snprintf(buf, max, fmt, ...)` | Вывод в буфер |

Форматы: `%s` `%d` `%u` `%x` `%c` `%p` `%%`

Реализация: `global_asm!` трамплин сохраняет rsi/rdx/rcx/r8/r9 на стек. Без XMM регистров, без проблем с SSE alignment. `%d/%x/%u` 32-битные (i32/u32).

#### Модуль: file (файловый I/O)

| Функция | Описание |
|:--|:--|
| `miku_open(path, len)` | Открыть файл |
| `miku_open_cstr(path)` | Открыть файл (C-строка) |
| `miku_close(fd)` | Закрыть |
| `miku_seek(fd, offset)` | Установить смещение |
| `miku_fsize(fd)` | Размер файла |
| `miku_read_file(path, &size)` | Прочитать файл целиком (malloc) |

#### Модуль: time (время)

| Функция | Описание |
|:--|:--|
| `miku_sleep(ticks)` | Сон (~10мс/тик) |
| `miku_sleep_ms(ms)` | Сон в миллисекундах |
| `miku_uptime()` | Тики с загрузки |
| `miku_uptime_ms()` | Миллисекунды с загрузки |

#### Модуль: proc (процесс)

| Функция | Описание |
|:--|:--|
| `miku_exit(code)` | Завершение процесса |
| `miku_getpid()` | Получить PID |
| `miku_getcwd(buf, size)` | Текущая директория |
| `miku_brk(addr)` | Расширение кучи (0=запрос) |
| `miku_mmap` / `miku_munmap` / `miku_mprotect` | Маппинг памяти |
| `miku_set_tls` / `miku_get_tls` | TLS регистр |
| `miku_map_lib(name, len)` | Маппинг разделяемой библиотеки |

#### Модуль: util (утилиты)

| Функция | Описание |
|:--|:--|
| `miku_abs` / `miku_min` / `miku_max` / `miku_clamp` | Числовые утилиты |
| `miku_swap(a, b)` | Обмен значений |
| `miku_srand(seed)` / `miku_rand()` / `miku_rand_range(lo, hi)` | Псевдослучайные числа (xorshift64) |
| `miku_assert_fail(expr, file, line)` | Неудача assert |
| `miku_panic(msg)` | Паника (exit 134) |

</details>

---

### Userspace SDK

<details>
<summary><b>Развернуть</b></summary>

#### Обзор

MikuOS предоставляет Rust SDK для разработки userspace программ в `no_std` окружении.
C также поддерживается.

#### Структура SDK

```
src/lib/userspace/
├── Cargo.toml              конфигурация crate
├── build.rs                автогенерация stub libmiku.so
├── build.sh                скрипт сборки + деплоя
├── x86_64-miku-app.json    target спецификация
└── src/
    ├── miku.rs             SDK: extern привязки + безопасные обертки
    ├── hello.rs            пример Hello World
    └── test_full.rs        71 тест
```

#### Пример на Rust

```rust
#![no_std]
#![no_main]

#[path = "miku.rs"]
mod miku;

#[no_mangle]
pub extern "C" fn _start_main() -> ! {
    miku::println("Hello MikuOS!");
    miku::exit(0);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { miku::exit(1); }
```

#### Сборка и деплой

```bash
cd ~/miku-os/src/lib/userspace
./build.sh hello        # сборка + копирование на data.img
```

#### Запуск в MikuOS

```
miku@os:/ $ ext4mount 3
miku@os:/ $ exec hello
Hello MikuOS!
```

#### Безопасные обертки (miku.rs)

| Обертка | Описание |
|:--|:--|
| `miku::print(s: &str)` | Вывод строки |
| `miku::println(s: &str)` | Вывод строки + перенос |
| `miku::exit(code)` | Завершение процесса |
| `miku::open(path) -> Result` | Открытие файла |
| `miku::read_file(path) -> Option` | Чтение файла целиком |
| `miku::sleep_ms(ms)` | Сон в миллисекундах |
| `miku::rand_range(lo, hi)` | Случайное число в диапазоне |
| `cstr!("text")` | Макрос C-строки |

#### Точка входа

Используется `_start_main`, а не `_start`. `miku.rs` содержит `global_asm!` трамплин `_start`, который делает `and rsp, -16` для SSE alignment и вызывает `_start_main`.

#### Тестовый набор

71 тест по следующим категориям:

| Категория | Количество |
|:--|:--|
| strings (базовые) | 10 |
| strings (расширенные) | 14 |
| numbers | 7 |
| memory | 4 |
| utilities | 7 |
| heap | 7 |
| process | 2 |
| printf | 6 |
| snprintf | 5 |
| time | 5 |
| file I/O | 3+ |

</details>

---

### Управление памятью

<details>
<summary><b>Физическая память (PMM)</b></summary>

#### Фреймовый аллокатор

- Bitmap аллокатор: до 4M фреймов (16 GB RAM), 1 бит = 1 фрейм 4KB
- `free_hint` и `contiguous_hint` для ускорения поиска свободных фреймов
- Непрерывный alloc: N фреймов за один запрос
- Регионы: динамическая регистрация RAM из Multiboot2 memory map

#### Аварийный пул

| Параметр | Значение |
|:--|:--|
| **Размер пула** | 64 фрейма (256 KB) |
| **Назначение** | Только для swap-in в page fault обработчике |
| **Пополнение** | Timer ISR каждые 250Hz через `refill_emergency_pool_tick()` |

</details>

<details>
<summary><b>Виртуальная память (VMM)</b></summary>

- 4-уровневые таблицы страниц (PML4 -> PDP -> PD -> PT)
- HHDM: Higher Half Direct Map (`0xFFFF800000000000`)
- `mark_swapped()`: запись swap PTE при выгрузке страницы
- Поддержка маппинга ring 0 / ring 3
- Создание и уничтожение адресных пространств для процессов

</details>

<details>
<summary><b>mmap подсистема</b></summary>

| Параметр | Значение |
|:--|:--|
| **Диапазон MMAP** | 0x100000000 ~ 0x7F0000000000 |
| **Диапазон BRK** | 0x6000000000 ~ |
| **Макс. VMA** | 256 записей |
| **Функции** | mmap, munmap, mprotect, brk |
| **MAP_FIXED** | Unmap существующих маппингов + удаление перекрывающихся VMA |
| **Проверка VMA** | Откат при неудаче insert |

</details>

<details>
<summary><b>Swap</b></summary>

#### Обратное отображение (swap_map)

- Каждому физическому фрейму сопоставляется `(cr3, virt_addr, age, pinned)`
- Отслеживание до 512K фреймов (2 GB RAM)

#### Алгоритм вытеснения: clock sweep

```
Pass 1: поиск фреймов с age >= 3 (самые старые)
Pass 2: аварийный режим, любой unpinned фрейм
```

- `touch(phys)`: сброс age в 1 при обращении к странице
- `age_all()`: увеличение age всех фреймов по таймеру

#### Кодирование Swap PTE

```
bit 0     = 0  (PRESENT=0)
bit 1     = 1  (SWAP_MARKER)
bits 12.. = номер swap слота
Доп. проверка: номер слота != 0 (защита от false positive)
```

</details>

---

### Планировщик

| Параметр | Значение |
|:--|:--|
| **Алгоритм** | CFS, вытесняющий |
| **Макс. процессов** | 4096 |
| **Частота таймера** | 250 Hz (PIT) |
| **Окно CPU** | 250 тиков (1 секунда) |
| **Стек** | 512 KB на процесс |
| **Состояния** | Ready / Running / Sleeping / Blocked / Dead |
| **Реализация** | Lock-free: ISR использует только атомики |

---

### Системные вызовы

| Nr | Имя | Описание |
|:--:|:--|:--|
| **0** | `sys_exit` | Завершение процесса + yield |
| **1** | `sys_write` | Запись в stdout/stderr (fd 1/2) |
| **2** | `sys_read` | Чтение из stdin (fd 0) или файлового дескриптора |
| **3** | `sys_mmap` | Создание маппинга памяти |
| **4** | `sys_munmap` | Удаление маппинга памяти |
| **5** | `sys_mprotect` | Изменение атрибутов защиты памяти |
| **6** | `sys_brk` | Расширение кучи |
| **7** | `sys_getpid` | Получение PID текущего процесса |
| **8** | `sys_getcwd` | Получение текущей директории |
| **9** | `sys_set_tls` | Установка FS.base регистра (TLS) |
| **10** | `sys_get_tls` | Получение FS.base регистра |
| **11** | `sys_open` | Открытие файла (VFS + ext2) |
| **12** | `sys_close` | Закрытие файлового дескриптора |
| **13** | `sys_seek` | Установка смещения в файле |
| **14** | `sys_fsize` | Получение размера файла |
| **15** | `sys_map_lib` | Маппинг разделяемой библиотеки |
| **16** | `sys_sleep` | Сон процесса (~10мс/тик) |
| **17** | `sys_uptime` | Тики с момента загрузки |

Таблица FD управляется per-process (BTreeMap<pid, ProcessFds>).

---

### Сетевой стек

<details>
<summary><b>Драйверы сетевых карт</b></summary>

| Драйвер | Чип |
|:--|:--|
| **Intel E1000** | 82540EM, 82545EM, 82574L, 82579LM, I217 |
| **Realtek RTL8139** | RTL8139 |
| **Realtek RTL8168** | RTL8168, RTL8169 |
| **VirtIO Net** | QEMU/KVM виртуальная сетевая карта |

</details>

<details>
<summary><b>Протоколы</b></summary>

| Уровень | Протоколы |
|:--|:--|
| **L2** | Ethernet, ARP (с таблицей кэша) |
| **L3** | IPv4, ICMP |
| **L4** | UDP, TCP (с управлением состоянием соединений) |
| **Приложение** | DHCP, DNS, NTP, HTTP, HTTP/2, Traceroute |
| **Безопасность** | TLS 1.3 (ECDHE + RSA + AES-GCM) |

</details>

<details>
<summary><b>TLS 1.3: полная реализация с нуля</b></summary>

- ECDH: обмен ключами X25519 (`tls_ecdh.rs`)
- RSA: парсинг ASN.1/DER сертификатов, проверка PKCS#1 подписей (`tls_rsa.rs`)
- BigNum: собственная реализация больших чисел для RSA 2048-bit (`tls_bignum.rs`)
- AES-GCM: аутентифицированное симметричное шифрование (`tls_gcm.rs`)
- SHA-256, HMAC, HKDF: хэширование, вывод ключей (`tls_crypto.rs`)
- Рукопожатие: ClientHello -> ServerHello -> Certificate -> Finished

</details>

---

### VFS (виртуальная файловая система)

<details>
<summary><b>Развернуть</b></summary>

#### Основные возможности

| Параметр | Значение |
|:--|:--|
| **Количество VNode** | 256 |
| **Одновременно открытых файлов** | 32 |
| **Точки монтирования** | 8 |
| **Дочерние узлы** | Динамически (без ограничений) |

Дочерние узлы управляются через динамическую `Vec`-based хэш-таблицу. Начальное количество слотов 16, при заполнении 75% автоматически удваивается.

- Типы узлов: `Regular`, `Directory`, `Symlink`, `CharDevice`, `BlockDevice`, `Pipe`, `Fifo`, `Socket`
- Полные метаданные: права, uid/gid, временные метки, размер, nlinks

#### Системные библиотеки

При загрузке создается директория `/lib` в tmpfs, `libmiku.so` записывается как immutable файл.
Флаг immutable запрещает unlink / write / rename.

#### Кэш

| Кэш | Размер |
|:--|:--|
| **Page cache** | 128 страниц x 512 байт, LRU вытеснение |
| **Dentry cache** | 128 записей, FNV32 хэш |

#### Навигация

- Path walking: глубина до 32 компонентов
- Разрешение символических ссылок: защита от циклов (8 уровней)
- FNV32 хэш: O(1) поиск по имени

#### Безопасность

- UNIX модель прав: `owner/group/other`, `setuid/setgid/sticky`
- Метки безопасности (MAC), квоты по байтам и inode
- Блокировки файлов: shared/exclusive с обнаружением deadlock (до 16 блокировок)
- Флаг immutable: защита системных библиотек

#### Продвинутые возможности

| Возможность | Детали |
|:--|:--|
| **VFS журнал** | 16 записей операций |
| **Xattr** | 8 расширенных атрибутов на узел |
| **Notify события** | inotify-подобная подсистема (до 16 событий) |
| **Хранилище версий** | 16 снапшотов файлов |
| **CAS хранилище** | Контентно-адресуемая дедупликация (до 16 объектов) |
| **Очередь блочного I/O** | 8 асинхронных запросов |

</details>

---

### Файловые системы

| FS | Точка монтирования | Описание |
|:--:|:--:|:--|
| **tmpfs** | `/` | RAM-based корневая FS |
| **devfs** | `/dev` | Устройства: `null`, `zero`, `random`, `urandom`, `console` |
| **procfs** | `/proc` | `version`, `uptime`, `meminfo`, `mounts`, `cpuinfo`, `stat` |
| **ext2** | `/mnt` | Полная запись/чтение реального диска |
| **ext3** | `/mnt` | Журналирование (JBD2) поверх ext2, отложенная запись |
| **ext4** | `/mnt` | Файлы на основе экстентов + crc32c контрольные суммы |

---

### MikuFS: драйвер Ext2/3/4

<details>
<summary><b>Развернуть</b></summary>

#### Чтение

- Суперблок, дескрипторы групп, inode, записи директорий
- Непрямые блоки (одинарные / двойные / тройные)
- Дерево экстентов Ext4

#### Запись

- Создание и удаление файлов, директорий, символических ссылок
- Bitmap аллокатор для блоков и inode (с приоритетом групп)
- Рекурсивное удаление
- Отложенная запись (dirty cache + pdflush)

#### Ext3 журнал (JBD2)

- Создание журнала (конвертация `ext2 -> ext3`)
- Запись транзакций: descriptor block, commit block, revoke block
- Восстановление: воспроизведение незавершенных транзакций при монтировании
- Отложенный коммит: ускорение записи журнала через dirty cache

#### mkfs

- Форматирование ext2/ext3/ext4
- Lazy init: немедленная инициализация только метаданных group 0, остальное отложено
- Инициализация только суперблока журнала (без обнуления всех блоков)

#### Утилиты

- `fsck`, `tree`, `du`, `cp`, `mv`, `chmod`, `chown`, hard links

</details>

---

### Команды оболочки

#### Унифицированные ext команды (автоопределение версии FS)

| Команда | Синтаксис | Описание |
|:--|:--|:--|
| `ext2mount` | `ext2mount [drive]` | Монтирование ext2 |
| `ext3mount` | `ext3mount [drive]` | Монтирование ext3 |
| `ext4mount` | `ext4mount [drive]` | Монтирование ext4 |
| `extls` | `extls [path]` | Список директории |
| `extcat` | `extcat <path>` | Содержимое файла |
| `extstat` | `extstat <path>` | Детали inode |
| `extinfo` | `extinfo` | Информация суперблока |
| `extwrite` | `extwrite <path> <text>` | Запись в файл |
| `extappend` | `extappend <path> <text>` | Дозапись в файл |
| `exttouch` | `exttouch <path>` | Создание пустого файла |
| `extmkdir` | `extmkdir <path>` | Создание директории |
| `extrm` | `extrm [-rf] <path>` | Удаление файла |
| `extrmdir` | `extrmdir <path>` | Удаление пустой директории |
| `extmv` | `extmv <path> <newname>` | Переименование файла |
| `extcp` | `extcp <src> <dst>` | Копирование файла |
| `extln -s` | `extln -s <target> <link>` | Создание символической ссылки |
| `extlink` | `extlink <existing> <link>` | Создание жесткой ссылки |
| `extchmod` | `extchmod <mode> <path>` | Изменение прав |
| `extchown` | `extchown <uid> <gid> <path>` | Изменение владельца |
| `extdu` | `extdu [path]` | Использование диска |
| `exttree` | `exttree [path]` | Дерево директорий |
| `extfsck` | `extfsck` | Проверка целостности FS |
| `extcache` | `extcache` | Статистика блочного кэша |
| `extcacheflush` | `extcacheflush` | Сброс кэша |
| `extsync` / `sync` | `sync` | Запись на диск |

> Старые команды (`ext2ls`, `ext3cat`, `ext4write` и т.д.) оставлены для обратной совместимости.

#### VFS команды

| Команда | Описание |
|:--|:--|
| `ls [path]` | Список директории (ext + VFS объединенный вид) |
| `cd <path>` | Смена директории |
| `pwd` | Текущий путь |
| `mkdir <path>` | Создание директории |
| `touch <path>` | Создание файла (RAM) |
| `cat <path>` | Содержимое файла |
| `write <path> <text>` | Запись в файл (RAM) |
| `rm [-rf] <path>` | Удаление файла/директории |
| `rmdir <path>` | Удаление директории (ext совместимо) |
| `mv <old> <new>` | Переименование |
| `stat <path>` | Информация о файле |
| `chmod <mode> <path>` | Изменение прав |
| `df` | Информация о файловой системе |

#### Команды динамической линковки

| Команда | Описание |
|:--|:--|
| `exec <path>` | Запуск ELF бинаря (с динамической линковкой) |
| `ldconfig` | Обновление кэша разделяемых библиотек |
| `ldd` | Список кэшированных библиотек |

#### Команды mkfs

| Команда | Описание |
|:--|:--|
| `mkfs.ext2 <drive>` | Форматирование ext2 |
| `mkfs.ext3 <drive>` | Форматирование ext3 (с журналом) |
| `mkfs.ext4 <drive>` | Форматирование ext4 (экстенты + журнал) |

---

### ATA драйвер

| Параметр | Значение |
|:--|:--|
| **Режим** | PIO (программный I/O) |
| **Операции** | Чтение/запись секторов (512 байт), до 255 секторов/команда |
| **Количество дисков** | 4: Primary/Secondary x Master/Slave |
| **Защита** | Flush кэша после записи, таймаут 50K итераций |
| **Адресация** | LBA28 (до 128GB) |

---

## Сборка и запуск

### Необходимые инструменты

| Инструмент | Назначение |
|:--|:--|
| **Rust nightly** | `no_std` + нестабильные возможности компилятора |
| **QEMU** | Эмуляция x86_64 машины |
| **grub-mkrescue** | Создание загрузочного ISO |
| **GCC** | Генерация stub libmiku + компиляция C программ |
| **e2tools** | Копирование файлов на ext4 образ |
| **Cargo** | Сборка ядра |

### Порядок запуска

```bash
git clone https://github.com/altushkaso2/miku-os
cd miku-os/builder
cargo run
```

Builder делает все автоматически:

```
Режим экономии RAM? (y/N)
[1/7] Компиляция ld-miku.so
[2/7] Компиляция libmiku.so
[3/7] Компиляция ядра miku-os
[4/7] Создание файловой структуры
[5/7] Генерация системного образа (miku-os.iso)
[6/7] Подготовка диска
[7/7] Запуск QEMU (опционально (y/N))
```

### Сборка userspace программ

```bash
cd src/lib/userspace
./build.sh hello         # сборка + копирование на диск
./build.sh test_full     # тестовый набор
./build.sh               # все бинари
```

---

## MikuOS ABI

Полная документация по разработке userspace программ: [MikuOS_ABI.md](docs/MikuOS_ABI.md)

---

## Автор

<div align="center">
  <a href="https://github.com/altushkaso2">
    <img src="https://github.com/altushkaso2.png" width="100" style="border-radius:50%;" alt="altushkaso2">
  </a>
  <br><br>
  <a href="https://github.com/altushkaso2"><b>@altushkaso2</b></a>
  <br>
  <sub>Автор и единственный разработчик Miku OS</sub>
  <br>
  <sub>Ядро - VFS - MikuFS - ELF - ld-miku - libmiku - Оболочка - Сеть - TLS - Планировщик - PMM - VMM - Swap</sub>
</div>

---

## От автора

> Все начиналось с простой мысли: "А что если написать ОС самому?"
> Каждый вечер я добавляю новую функцию, чиню новый баг, делаю новое открытие.
> От первого символа на экране до полноценного TLS 1.3 стека, lock-free планировщика
> и динамического линкера, все написано вручную.
> Никаких готовых библиотек или оберток. Только Rust, документация и упорство :D
>
> Момент, когда ELF загрузчик и динамическая линковка заработали, когда "hello from dynamic linking!"
> появилось на экране, я не забуду никогда.
> А когда libmiku прошла все 71 тест, стало ясно что на этой ОС можно запускать настоящие программы.

<div align="center">

**Miku OS** - чистая ОС, написанная с нуля на Rust

*С любовью*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">
