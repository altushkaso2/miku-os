<div align="center">

# Miku OS

**An experimental operating system kernel written in Rust**

*Powered by Rust and a few developers :D*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">

[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/arch-x86__64-blue.svg)]()
[![Status](https://img.shields.io/badge/status-release-green.svg)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey.svg)]()

</div>

---

> **Documentation:** [Russian](docs/Russian_README.md) | [English](docs/English_README.md) | [Japanese](Japanese_README.md)

---

## About

**Miku OS** is a UNIX-like operating system developed from scratch in a `no_std` environment.
It does not use any standard library (`libc`), maintaining full control over hardware and memory architecture.
ELF dynamic linking, shared libraries, and userspace processes are implemented from scratch.

> All code is written in Rust. Assembly is only used for the bootloader, syscall handler, and context switching.

---

## Technical Specifications

### Kernel

| Component | Description |
|:--|:--|
| **Architecture** | x86_64, `#![no_std]`, `#![no_main]` |
| **Bootloader** | GRUB2 + Multiboot2, framebuffer (BGR/RGB auto-detection) |
| **Protection** | GDT + TSS + IST (double fault, page fault, GPF), ring 0 / ring 3 |
| **Interrupts** | IDT: timer, keyboard, page fault, GPF, #UD, #NM, double fault |
| **PIC** | PIC8259 (offset 32/40) |
| **SSE** | CR0.EM=0, CR0.MP=1, CR4.OSFXSR=1, CR4.OSXMMEXCPT=1 |
| **Heap** | 32 MB, linked list allocator |
| **Syscall** | SYSCALL/SYSRET via MSR, naked asm handler, R8/R9/R10 preservation |

---

### ELF Loader and Dynamic Linking

<details>
<summary><b>ELF Loader</b></summary>

#### Features

| Feature | Description |
|:--|:--|
| **Formats** | ET_EXEC (static), ET_DYN (PIE) |
| **Segments** | PT_LOAD, PT_INTERP, PT_DYNAMIC, PT_TLS, PT_GNU_RELRO, PT_GNU_STACK |
| **Relocations** | R_X86_64_RELATIVE, R_X86_64_JUMP_SLOT, R_X86_64_GLOB_DAT, R_X86_64_64 |
| **Security** | W^X enforcement (W+X segments rejected), RELRO |
| **ASLR** | 20-bit entropy for PIE binaries (RDRAND + TSC fallback) |
| **Stack** | SysV ABI compliant: argc, argv, envp, auxv (16-byte aligned) |
| **TLS** | Thread Local Storage (via FS.base register) |

#### Modular Structure

| Module | Description |
|:--|:--|
| **elf_loader.rs** | ELF parsing, segment mapping |
| **exec_elf.rs** | Process creation, stack construction |
| **dynlink.rs** | Dynamic linking (delegates to reloc.rs) |
| **reloc.rs** | Unified relocation engine |
| **vfs_read.rs** | Unified file reading (VFS + ext2) |
| **random.rs** | RDRAND/TSC random numbers, ASLR |

#### auxv Entries

| Key | Description |
|:--|:--|
| AT_PHDR | Virtual address of program headers |
| AT_PHENT | Size of program header entry |
| AT_PHNUM | Number of program headers |
| AT_PAGESZ | Page size (4096) |
| AT_ENTRY | Executable entry point |
| AT_BASE | Interpreter base address |
| AT_RANDOM | 16 bytes of random data |

</details>

<details>
<summary><b>ld-miku (Dynamic Linker)</b></summary>

#### Overview

`ld-miku` is the ELF dynamic linker for MikuOS. Written in Rust in a `#![no_std]` environment,
compiled as a static PIE binary.

#### Loading Process

```
1. Kernel loads ELF -> detects PT_INTERP
2. ld-miku.so mapped from INCLUDE_BYTES into memory
3. ld-miku starts -> parses auxv (AT_PHDR/AT_ENTRY)
4. Identifies required libraries from DT_NEEDED
5. Maps shared libraries via SYS_MAP_LIB syscall
6. Applies PLT/GOT relocations
7. Exports symbols to global table
8. Executes DT_INIT / DT_INIT_ARRAY
9. Jumps to executable entry point
```

#### Features

- Global symbol table (up to 1024 symbols)
- Weak symbol resolution
- Recursive dependency loading (up to 16 libraries)
- R_X86_64_COPY relocation support
- DT_HASH / DT_GNU_HASH for accurate symbol counting
- Correct envp skipping during auxv parsing

</details>

<details>
<summary><b>Shared Libraries (solib)</b></summary>

#### Global Library Cache

| Parameter | Value |
|:--|:--|
| **Max cached** | 32 libraries |
| **Search paths** | /lib, /usr/lib |
| **Page mapping** | All segments copied per-process |
| **OOM protection** | parse_and_prepare aborts on OOM without caching broken data |

#### SYS_MAP_LIB Syscall (nr=15)

The kernel parses ELF segments and maps the shared library directly into the process address space.

- Read-only segments -> private copy from cache
- Writable segments -> fresh allocation per process
- Rollback on map_page failure

#### System Libraries

`libmiku.so` is embedded in the kernel via `include_bytes!` and registered in the cache at boot via `solib::preload`.

#### Shell Commands

| Command | Description |
|:--|:--|
| `ldconfig` | Scan /lib and /usr/lib, update cache |
| `ldd` | List cached libraries |

</details>

---

### libmiku.so (Standard Library)

<details>
<summary><b>Expand</b></summary>

#### Overview

libmiku is a C-compatible standard library for MikuOS. Written in Rust, it exports 79 functions across 12 modules.
Dynamically loaded by ld-miku, used by all userspace programs.

#### Modular Structure

```
src/lib/libmiku/
├── lib.rs       module declarations, entry point, panic handler
├── sys.rs       syscall primitives (sc0..sc4), constants
├── proc.rs      exit, getpid, brk, mmap, munmap, tls
├── io.rs        write, read, print, println, readline
├── mem.rs       memset, memcpy, memmove, memcmp
├── num.rs       itoa, utoa, atoi, print_int, print_hex
├── string.rs    strlen, strcmp, strcpy, strtok, strtol...
├── heap.rs      malloc, free, realloc, calloc
├── file.rs      open, close, seek, fsize, read_file
├── time.rs      sleep, uptime
├── util.rs      abs, min, max, rand, assert, panic
└── fmt.rs       printf, snprintf (asm trampolines)
```

#### Module: io (Input/Output)

| Function | Description |
|:--|:--|
| `miku_write(fd, buf, len)` | Write to fd |
| `miku_read(fd, buf, len)` | Read from fd |
| `miku_print(str)` | Print string |
| `miku_println(str)` | Print string + newline |
| `miku_puts(str)` | puts-compatible |
| `miku_putchar(c)` | Output 1 byte |
| `miku_getchar()` | Input 1 byte |
| `miku_readline(buf, max)` | Line input (fixed buffer) |
| `miku_getline()` | Line input (malloc, needs free) |

#### Module: string (Strings)

| Function | Description |
|:--|:--|
| `miku_strlen` | String length |
| `miku_strcmp` / `miku_strncmp` | String comparison |
| `miku_strcpy` / `miku_strncpy` | String copy |
| `miku_strcat` / `miku_strncat` | String concatenation |
| `miku_strchr` / `miku_strrchr` | Character search |
| `miku_strstr` | Substring search |
| `miku_strdup` | String duplicate (malloc) |
| `miku_toupper` / `miku_tolower` | Case conversion |
| `miku_isdigit` / `miku_isalpha` / `miku_isalnum` / `miku_isspace` | Character classification |
| `miku_strtok` | Tokenization (stateful) |
| `miku_strpbrk` | Character set search |
| `miku_strspn` / `miku_strcspn` | Prefix length |
| `miku_strtol` / `miku_strtoul` | String to number (base 0/8/10/16) |
| `miku_strlcpy` / `miku_strlcat` | BSD-safe copy/concatenation |

#### Module: num (Numbers)

| Function | Description |
|:--|:--|
| `miku_itoa(val, buf)` | Integer to string |
| `miku_utoa(val, buf)` | Unsigned integer to string |
| `miku_atoi(str)` | String to integer |
| `miku_print_int(val)` | Print decimal |
| `miku_print_hex(val)` | Print 0x... |

#### Module: mem (Memory)

| Function | Description |
|:--|:--|
| `miku_memset` | Memory fill (8-byte aligned optimization) |
| `miku_memcpy` | Memory copy (8-byte aligned optimization) |
| `miku_memmove` | Memory copy (overlap-safe) |
| `miku_memcmp` | Memory comparison |
| `miku_bzero` | Zero fill |

#### Module: heap (Dynamic Memory)

| Function | Description |
|:--|:--|
| `miku_malloc(size)` | Allocate memory |
| `miku_free(ptr)` | Free memory |
| `miku_realloc(ptr, size)` | Resize allocation |
| `miku_calloc(count, size)` | Zero-initialized allocation |

Implementation: mmap-based slab allocator. < 32KB from 128KB slab, >= 32KB via individual mmap/munmap.

#### Module: fmt (Formatted Output)

| Function | Description |
|:--|:--|
| `miku_printf(fmt, ...)` | Formatted output |
| `miku_snprintf(buf, max, fmt, ...)` | Formatted output to buffer |

Supported formats: `%s` `%d` `%u` `%x` `%c` `%p` `%%`

Implementation: `global_asm!` trampoline saves rsi/rdx/rcx/r8/r9 to stack. No XMM registers used, avoiding SSE alignment issues. `%d/%x/%u` are 32-bit (read as i32/u32).

#### Module: file (File I/O)

| Function | Description |
|:--|:--|
| `miku_open(path, len)` | Open file |
| `miku_open_cstr(path)` | Open file (C string) |
| `miku_close(fd)` | Close |
| `miku_seek(fd, offset)` | Set offset |
| `miku_fsize(fd)` | Get file size |
| `miku_read_file(path, &size)` | Read entire file (malloc) |

#### Module: time (Time)

| Function | Description |
|:--|:--|
| `miku_sleep(ticks)` | Sleep (~10ms/tick) |
| `miku_sleep_ms(ms)` | Sleep in milliseconds |
| `miku_uptime()` | Ticks since boot |
| `miku_uptime_ms()` | Milliseconds since boot |

#### Module: proc (Process)

| Function | Description |
|:--|:--|
| `miku_exit(code)` | Terminate process |
| `miku_getpid()` | Get PID |
| `miku_getcwd(buf, size)` | Get current directory |
| `miku_brk(addr)` | Expand heap (0=query) |
| `miku_mmap` / `miku_munmap` / `miku_mprotect` | Memory mapping |
| `miku_set_tls` / `miku_get_tls` | TLS register |
| `miku_map_lib(name, len)` | Map shared library |

#### Module: util (Utilities)

| Function | Description |
|:--|:--|
| `miku_abs` / `miku_min` / `miku_max` / `miku_clamp` | Numeric utilities |
| `miku_swap(a, b)` | Swap values |
| `miku_srand(seed)` / `miku_rand()` / `miku_rand_range(lo, hi)` | Pseudorandom numbers (xorshift64) |
| `miku_assert_fail(expr, file, line)` | Assertion failure |
| `miku_panic(msg)` | Panic (exit 134) |

</details>

---

### Userspace SDK

<details>
<summary><b>Expand</b></summary>

#### Overview

MikuOS provides a Rust SDK for developing userspace programs in a `no_std` environment.
C is also supported.

#### SDK Structure

```
src/lib/userspace/
├── Cargo.toml              crate configuration
├── build.rs                auto-generates stub libmiku.so
├── build.sh                build + deploy script
├── x86_64-miku-app.json    target specification
└── src/
    ├── miku.rs             SDK: extern bindings + safe wrappers
    ├── hello.rs            Hello World example
    └── test_full.rs        71 tests
```

#### Rust Example

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

#### Build and Deploy

```bash
cd ~/miku-os/src/lib/userspace
./build.sh hello        # build + copy to data.img
```

#### Running in MikuOS

```
miku@os:/ $ ext4mount 3
miku@os:/ $ exec hello
Hello MikuOS!
```

#### Safe Wrappers (miku.rs)

| Wrapper | Description |
|:--|:--|
| `miku::print(s: &str)` | Print string |
| `miku::println(s: &str)` | Print string + newline |
| `miku::exit(code)` | Terminate process |
| `miku::open(path) -> Result` | Open file |
| `miku::read_file(path) -> Option` | Read entire file |
| `miku::sleep_ms(ms)` | Sleep in milliseconds |
| `miku::rand_range(lo, hi)` | Random number in range |
| `cstr!("text")` | C string macro |

#### Entry Point

Use `_start_main`, not `_start`. `miku.rs` contains a `global_asm!` trampoline that defines `_start` with `and rsp, -16` for SSE alignment before calling `_start_main`.

#### Test Suite

71 tests across the following categories:

| Category | Count |
|:--|:--|
| strings (basic) | 10 |
| strings (extended) | 14 |
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

### Memory Management

<details>
<summary><b>Physical Memory (PMM)</b></summary>

#### Frame Allocator

- Bitmap allocator: up to 4M frames (16 GB RAM), 1 bit = 1 frame of 4KB
- `free_hint` and `contiguous_hint` for fast free frame lookup
- Contiguous alloc: N frames in a single request
- Regions: dynamic RAM range registration from Multiboot2 memory map

#### Emergency Pool

| Parameter | Value |
|:--|:--|
| **Pool size** | 64 frames (256 KB) |
| **Purpose** | Swap-in within page fault handler only |
| **Refill** | Timer ISR at 250Hz via `refill_emergency_pool_tick()` |

</details>

<details>
<summary><b>Virtual Memory (VMM)</b></summary>

- 4-level page tables (PML4 -> PDP -> PD -> PT)
- HHDM: Higher Half Direct Map (`0xFFFF800000000000`)
- `mark_swapped()`: write swap PTE when evicting a page
- Ring 0 / ring 3 mapping support
- Address space creation and destruction for user processes

</details>

<details>
<summary><b>mmap Subsystem</b></summary>

| Parameter | Value |
|:--|:--|
| **MMAP range** | 0x100000000 ~ 0x7F0000000000 |
| **BRK range** | 0x6000000000 ~ |
| **Max VMAs** | 256 entries |
| **Features** | mmap, munmap, mprotect, brk |
| **MAP_FIXED** | Unmaps existing mappings + removes overlapping VMAs |
| **VMA validation** | Rollback on insert failure |

</details>

<details>
<summary><b>Swap</b></summary>

#### Reverse Mapping (swap_map)

- Each physical frame records `(cr3, virt_addr, age, pinned)`
- Tracks up to 512K frames (2 GB RAM)

#### Eviction Algorithm: Clock Sweep

```
Pass 1: search for frames with age >= 3 (oldest)
Pass 2: emergency mode, any unpinned frame
```

- `touch(phys)`: reset age to 1 on page access
- `age_all()`: increment age of all frames on timer

#### Swap PTE Encoding

```
bit 0     = 0  (PRESENT=0)
bit 1     = 1  (SWAP_MARKER)
bits 12.. = swap slot number
Additional check: slot number != 0 (false positive prevention)
```

</details>

---

### Scheduler

| Parameter | Value |
|:--|:--|
| **Algorithm** | CFS, preemptive |
| **Max processes** | 4096 |
| **Timer frequency** | 250 Hz (PIT) |
| **CPU window** | 250 ticks (1 second) |
| **Stack** | 512 KB per process |
| **States** | Ready / Running / Sleeping / Blocked / Dead |
| **Implementation** | Lock-free: ISR uses atomics only |

---

### System Calls

| Nr | Name | Description |
|:--:|:--|:--|
| **0** | `sys_exit` | Terminate process + yield |
| **1** | `sys_write` | Write to stdout/stderr (fd 1/2) |
| **2** | `sys_read` | Read from stdin (fd 0) or file descriptor |
| **3** | `sys_mmap` | Create memory mapping |
| **4** | `sys_munmap` | Remove memory mapping |
| **5** | `sys_mprotect` | Change memory protection attributes |
| **6** | `sys_brk` | Expand heap |
| **7** | `sys_getpid` | Get current process PID |
| **8** | `sys_getcwd` | Get current directory |
| **9** | `sys_set_tls` | Set FS.base register (TLS) |
| **10** | `sys_get_tls` | Get FS.base register |
| **11** | `sys_open` | Open file (VFS + ext2) |
| **12** | `sys_close` | Close file descriptor |
| **13** | `sys_seek` | Set file offset |
| **14** | `sys_fsize` | Get file size |
| **15** | `sys_map_lib` | Direct shared library mapping |
| **16** | `sys_sleep` | Sleep process (~10ms/tick) |
| **17** | `sys_uptime` | Get ticks since boot |

FD table is managed per-process (BTreeMap<pid, ProcessFds>).

---

### Network Stack

<details>
<summary><b>Network Card Drivers</b></summary>

| Driver | Chip |
|:--|:--|
| **Intel E1000** | 82540EM, 82545EM, 82574L, 82579LM, I217 |
| **Realtek RTL8139** | RTL8139 |
| **Realtek RTL8168** | RTL8168, RTL8169 |
| **VirtIO Net** | QEMU/KVM virtual network card |

</details>

<details>
<summary><b>Protocols</b></summary>

| Layer | Protocols |
|:--|:--|
| **L2** | Ethernet, ARP (with cache table) |
| **L3** | IPv4, ICMP |
| **L4** | UDP, TCP (with connection state management) |
| **Application** | DHCP, DNS, NTP, HTTP, HTTP/2, Traceroute |
| **Security** | TLS 1.3 (ECDHE + RSA + AES-GCM) |

</details>

<details>
<summary><b>TLS 1.3: Complete Implementation from Scratch</b></summary>

- ECDH: X25519 key exchange (`tls_ecdh.rs`)
- RSA: ASN.1/DER certificate parsing, PKCS#1 signature verification (`tls_rsa.rs`)
- BigNum: custom big number implementation for RSA 2048-bit (`tls_bignum.rs`)
- AES-GCM: authenticated symmetric encryption (`tls_gcm.rs`)
- SHA-256, HMAC, HKDF: hashing, key derivation (`tls_crypto.rs`)
- Handshake: ClientHello -> ServerHello -> Certificate -> Finished

</details>

---

### VFS (Virtual File System)

<details>
<summary><b>Expand</b></summary>

#### Core Features

| Parameter | Value |
|:--|:--|
| **VNodes** | 256 |
| **Open files** | 32 |
| **Mount points** | 8 |
| **Child nodes** | Dynamic (unlimited) |

Child nodes are managed via a dynamic `Vec`-based hash map. Initial slot count is 16, automatically doubling at 75% utilization.

- Node types: `Regular`, `Directory`, `Symlink`, `CharDevice`, `BlockDevice`, `Pipe`, `Fifo`, `Socket`
- Full metadata: permissions, uid/gid, timestamps, size, nlinks

#### System Libraries

At boot, `/lib` directory is created in tmpfs and `libmiku.so` is written as an immutable file.
The immutable flag prevents unlink / write / rename.

#### Cache

| Cache | Size |
|:--|:--|
| **Page cache** | 128 pages x 512 bytes, LRU eviction |
| **Dentry cache** | 128 entries, FNV32 hash |

#### Navigation

- Path walking: depth up to 32 components
- Symlink resolution: loop protection (8 levels)
- FNV32 hash: O(1) lookup by name

#### Security

- UNIX permission model: `owner/group/other`, `setuid/setgid/sticky`
- Security labels (MAC), byte and inode quotas
- File locking: shared/exclusive with deadlock detection (up to 16 locks)
- Immutable flag: system library protection

#### Advanced Features

| Feature | Details |
|:--|:--|
| **VFS journal** | 16 operation log entries |
| **Xattr** | 8 extended attributes per node |
| **Notify events** | inotify-like subsystem (up to 16 events) |
| **Version store** | 16 file snapshots |
| **CAS store** | Content-addressed deduplication (up to 16 objects) |
| **Block I/O queue** | 8 async requests |

</details>

---

### File Systems

| FS | Mount Point | Description |
|:--:|:--:|:--|
| **tmpfs** | `/` | RAM-based root FS |
| **devfs** | `/dev` | Devices: `null`, `zero`, `random`, `urandom`, `console` |
| **procfs** | `/proc` | `version`, `uptime`, `meminfo`, `mounts`, `cpuinfo`, `stat` |
| **ext2** | `/mnt` | Full read-write to real disk |
| **ext3** | `/mnt` | Journaling (JBD2) on top of ext2, delayed writes |
| **ext4** | `/mnt` | Extent-based files + crc32c checksums |

---

### MikuFS: Ext2/3/4 Driver

<details>
<summary><b>Expand</b></summary>

#### Reading

- Superblock, group descriptors, inodes, directory entries
- Indirect blocks (single / double / triple)
- Ext4 extent trees

#### Writing

- Create and delete files, directories, symbolic links
- Bitmap allocator for blocks and inodes (with group priority)
- Recursive deletion
- Delayed writes (dirty cache + pdflush)

#### Ext3 Journal (JBD2)

- Journal creation (`ext2 -> ext3` conversion)
- Transaction writing: descriptor blocks, commit blocks, revoke blocks
- Recovery: replay incomplete transactions on mount
- Delayed commit: accelerate journal writes via dirty cache

#### mkfs

- ext2/ext3/ext4 formatting
- Lazy init: only group 0 metadata initialized immediately, rest deferred
- Journal superblock only initialization (skip full block zeroing)

#### Utilities

- `fsck`, `tree`, `du`, `cp`, `mv`, `chmod`, `chown`, hard links

</details>

---

### Shell Commands

#### Unified ext Commands (auto-detects mounted FS version)

| Command | Syntax | Description |
|:--|:--|:--|
| `ext2mount` | `ext2mount [drive]` | Mount ext2 |
| `ext3mount` | `ext3mount [drive]` | Mount ext3 |
| `ext4mount` | `ext4mount [drive]` | Mount ext4 |
| `extls` | `extls [path]` | Directory listing |
| `extcat` | `extcat <path>` | File contents |
| `extstat` | `extstat <path>` | Inode details |
| `extinfo` | `extinfo` | Superblock info |
| `extwrite` | `extwrite <path> <text>` | Write to file |
| `extappend` | `extappend <path> <text>` | Append to file |
| `exttouch` | `exttouch <path>` | Create empty file |
| `extmkdir` | `extmkdir <path>` | Create directory |
| `extrm` | `extrm [-rf] <path>` | Delete file |
| `extrmdir` | `extrmdir <path>` | Delete empty directory |
| `extmv` | `extmv <path> <newname>` | Rename file |
| `extcp` | `extcp <src> <dst>` | Copy file |
| `extln -s` | `extln -s <target> <link>` | Create symbolic link |
| `extlink` | `extlink <existing> <link>` | Create hard link |
| `extchmod` | `extchmod <mode> <path>` | Change permissions |
| `extchown` | `extchown <uid> <gid> <path>` | Change owner |
| `extdu` | `extdu [path]` | Disk usage |
| `exttree` | `exttree [path]` | Directory tree |
| `extfsck` | `extfsck` | FS integrity check |
| `extcache` | `extcache` | Block cache statistics |
| `extcacheflush` | `extcacheflush` | Flush cache |
| `extsync` / `sync` | `sync` | Write to disk |

> Legacy commands (`ext2ls`, `ext3cat`, `ext4write`, etc.) are kept for backward compatibility.

#### VFS Commands

| Command | Description |
|:--|:--|
| `ls [path]` | Directory listing (ext + VFS combined view) |
| `cd <path>` | Change directory |
| `pwd` | Print current path |
| `mkdir <path>` | Create directory |
| `touch <path>` | Create file (RAM) |
| `cat <path>` | File contents |
| `write <path> <text>` | Write to file (RAM) |
| `rm [-rf] <path>` | Delete file/directory |
| `rmdir <path>` | Delete directory (ext compatible) |
| `mv <old> <new>` | Rename |
| `stat <path>` | File info |
| `chmod <mode> <path>` | Change permissions |
| `df` | File system info |

#### Dynamic Linking Commands

| Command | Description |
|:--|:--|
| `exec <path>` | Run ELF binary (with dynamic linking) |
| `ldconfig` | Update shared library cache |
| `ldd` | List cached libraries |

#### mkfs Commands

| Command | Description |
|:--|:--|
| `mkfs.ext2 <drive>` | Format ext2 |
| `mkfs.ext3 <drive>` | Format ext3 (with journal) |
| `mkfs.ext4 <drive>` | Format ext4 (extents + journal) |

---

### ATA Driver

| Parameter | Value |
|:--|:--|
| **Mode** | PIO (Programmed I/O) |
| **Operations** | Sector read/write (512 bytes), up to 255 sectors/command |
| **Disks** | 4: Primary/Secondary x Master/Slave |
| **Protection** | Cache flush after write, 50K iteration timeout |
| **Addressing** | LBA28 (up to 128GB) |

---

## Build and Run

### Required Tools

| Tool | Purpose |
|:--|:--|
| **Rust nightly** | `no_std` + unstable compiler features |
| **QEMU** | x86_64 machine emulation |
| **grub-mkrescue** | Create bootable ISO |
| **GCC** | libmiku stub generation + C program compilation |
| **e2tools** | File copy to ext4 image |
| **Cargo** | Kernel build |

### Running

```bash
git clone https://github.com/altushkaso2/miku-os
cd miku-os/builder
cargo run
```

The builder handles everything automatically:

```
RAM saving mode? (y/N)
[1/7] Compile ld-miku.so
[2/7] Compile libmiku.so
[3/7] Compile miku-os kernel
[4/7] Create file structure
[5/7] Generate system image (miku-os.iso)
[6/7] Prepare disk
[7/7] Launch QEMU (optional (y/N))
```

### Building Userspace Programs

```bash
cd src/lib/userspace
./build.sh hello         # build + copy to disk
./build.sh test_full     # test suite
./build.sh               # all binaries
```

---

## MikuOS ABI

Complete documentation for developing userspace programs: [MikuOS_ABI.md](docs/MikuOS_ABI.md)

---

## Author

<div align="center">
  <a href="https://github.com/altushkaso2">
    <img src="https://github.com/altushkaso2.png" width="100" style="border-radius:50%;" alt="altushkaso2">
  </a>
  <br><br>
  <a href="https://github.com/altushkaso2"><b>@altushkaso2</b></a>
  <br>
  <sub>Author and sole developer of Miku OS</sub>
  <br>
  <sub>Kernel - VFS - MikuFS - ELF - ld-miku - libmiku - Shell - Network - TLS - Scheduler - PMM - VMM - Swap</sub>
</div>

---

## From the Author

> It all started with a simple thought: "What if I wrote an OS myself?"
> Every night I add a new feature, fix a new bug, make a new discovery.
> From the first character on screen to a full TLS 1.3 stack, a lock-free scheduler,
> and a dynamic linker, everything was written by hand.
> No pre-made libraries or wrappers. Just Rust, documentation, and persistence :D
>
> The moment the ELF loader and dynamic linking worked, when "hello from dynamic linking!"
> appeared on screen, I will never forget.
> And when libmiku passed all 71 tests, it became clear that real programs can run on this OS.

<div align="center">

**Miku OS** - A pure OS written from scratch in Rust

*With love*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">
