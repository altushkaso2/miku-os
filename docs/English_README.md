<div align="center">

# Miku OS

**An experimental operating system kernel written in Rust**

*Powered by Rust and a small team of developers :D*

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

**Miku OS** is a UNIX-like operating system built from scratch in a `no_std` environment.
It uses no standard library (`libc`) whatsoever, giving full control over hardware and memory architecture.
ELF dynamic linking, shared libraries, and userspace processes are implemented from scratch.

> All code is written in Rust. Assembly is used only in the bootloader, syscall handler, and context switch routines.

---

## Technical Specification

### Kernel

| Component | Description |
|:--|:--|
| **Architecture** | x86_64, `#![no_std]`, `#![no_main]` |
| **Bootloader** | GRUB2 + Multiboot2, framebuffer (BGR/RGB auto-detect) |
| **Protection** | GDT + TSS + IST (double fault, page fault, GPF), ring 0 / ring 3 |
| **Interrupts** | IDT: timer, keyboard, page fault, GPF, #UD, #NM, double fault |
| **PIC** | PIC8259 (offsets 32/40) |
| **SSE** | CR0.EM=0, CR0.MP=1, CR4.OSFXSR=1, CR4.OSXMMEXCPT=1 |
| **Heap** | 32 MB, linked-list allocator |
| **Syscall** | SYSCALL/SYSRET via MSR, naked asm handler, R8/R9/R10 preserved |

---

### ELF Loader and Dynamic Linking

<details>
<summary><b>ELF Loader</b></summary>

#### Features

| Feature | Description |
|:--|:--|
| **Supported formats** | ET_EXEC (static), ET_DYN (PIE) |
| **Segments** | PT_LOAD, PT_INTERP, PT_DYNAMIC, PT_TLS, PT_GNU_RELRO, PT_GNU_STACK |
| **Relocations** | R_X86_64_RELATIVE, R_X86_64_JUMP_SLOT, R_X86_64_GLOB_DAT, R_X86_64_64 |
| **Security** | W^X enforcement (W+X segments rejected), RELRO |
| **ASLR** | 16-bit entropy for PIE binaries (65536 positions, 4 KB step) |
| **Stack** | SysV ABI: argc, argv, envp, auxv (16-byte aligned) |
| **TLS** | Thread Local Storage (via FS.base register) |

#### auxv Entries

| Key | Description |
|:--|:--|
| AT_PHDR | Virtual address of program headers |
| AT_PHENT | Size of one program header entry |
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

#### Execution Flow

```
1. Kernel loads ELF, detects PT_INTERP
2. ld-miku.so is mapped into memory from INCLUDE_BYTES
3. ld-miku starts, parses AT_PHDR/AT_ENTRY from auxv
4. Identifies required libraries from DT_NEEDED
5. Maps shared libraries via SYS_MAP_LIB syscall
6. Applies PLT/GOT relocations
7. Exports symbols to global table
8. Runs DT_INIT / DT_INIT_ARRAY
9. Jumps to executable entry point
```

#### Features

- Global symbol table (up to 1024 symbols)
- Weak symbol resolution
- Recursive dependency loading (up to 16 libraries)
- R_X86_64_COPY relocation support
- Correct auxv parsing with envp skip

</details>

<details>
<summary><b>Shared Libraries (solib)</b></summary>

#### Global Library Cache

| Parameter | Value |
|:--|:--|
| **Max cache** | 32 libraries |
| **Search paths** | /lib, /usr/lib |
| **Shared pages** | .text/.rodata - same physical pages across all processes |
| **Private pages** | .data/.bss - new allocation per process |

#### SYS_MAP_LIB Syscall (nr=15)

The kernel parses ELF segments and maps a shared library directly into the process address space.

- read-only segments - shared physical pages (identical across all processes)
- writable segments - fresh allocation per process

```
Process A: libmiku.so .text -> phys page 0x1234000 (shared)
Process B: libmiku.so .text -> phys page 0x1234000 (same!)
Process A: libmiku.so .data -> phys page 0x5678000 (private)
Process B: libmiku.so .data -> phys page 0x9ABC000 (private)
```

#### System Library

`/lib/libmiku.so` is embedded into VFS (tmpfs) as an immutable file.
The immutable flag prevents unlink / write / rename.

#### Shell Commands

| Command | Description |
|:--|:--|
| `ldconfig` | Scan /lib and /usr/lib, update cache |
| `ldd` | List cached libraries |

</details>

---

### Memory Management

<details>
<summary><b>Physical Memory (PMM)</b></summary>

#### Frame Allocator

- Bitmap allocator: up to 4M frames (16 GB RAM), 1 bit = 1 frame of 4 KB
- `free_hint` and `contiguous_hint` for fast free frame search
- Contiguous alloc: reserve N frames in one request
- Regions: dynamic RAM ranges registered from Multiboot2 memory map

#### Emergency Pool

| Parameter | Value |
|:--|:--|
| **Pool size** | 64 frames (256 KB) |
| **Purpose** | swap-in inside page fault handler only |
| **Refill** | Timer ISR calls `refill_emergency_pool_tick()` every 250 Hz |

```
alloc_frame()           - normal alloc from PMM
alloc_frame_emergency() - emergency pool only (fault handler)
alloc_or_evict()        - alloc + evict when RAM is low
alloc_for_swapin()      - emergency pool only (fault context)
```

</details>

<details>
<summary><b>Virtual Memory (VMM)</b></summary>

- 4-level page tables (PML4 -> PDP -> PD -> PT)
- HHDM: Higher Half Direct Map (`0xFFFF800000000000`)
- `mark_swapped()`: writes swap PTE when a page is swapped out
- ring 0 / ring 3 mapping support
- Address space creation and destruction for user processes

</details>

<details>
<summary><b>mmap Subsystem</b></summary>

| Parameter | Value |
|:--|:--|
| **MMAP range** | 0x100000000 ~ 0x7F0000000000 |
| **BRK range** | 0x6000000000 ~ |
| **Max VMAs** | 64 entries |
| **Functions** | mmap, munmap, mprotect, brk |

</details>

<details>
<summary><b>Swap</b></summary>

#### Reverse Mapping (swap_map)

- Records `(cr3, virt_addr, age, pinned)` per physical frame
- Tracks up to 512K frames (2 GB RAM)

#### Eviction Algorithm: Clock Sweep

```
Pass 1: find frames with age >= 3 (oldest first)
Pass 2: emergency - grab any unpinned frame
```

- `touch(phys)`: resets age to 1 on page access
- `age_all()`: increments all frame ages via timer

#### Swap PTE Encoding

```
bit 0     = 0  (PRESENT=0)
bit 1     = 1  (SWAP_MARKER)
bits 12.. = swap slot number
```

</details>

---

### Scheduler

| Parameter | Value |
|:--|:--|
| **Type** | CFS (Completely Fair Scheduler), preemptive |
| **Max processes** | 4096 |
| **Timer frequency** | 250 Hz (PIT) |
| **CPU window** | 250 ticks (1 second) |
| **Stack** | 512 KB per process |
| **States** | Ready / Running / Sleeping / Blocked / Dead |
| **Implementation** | Lock-free: ISR uses atomics only, zero mutexes |

Context switch implemented in naked asm. `schedule_from_isr` acquires zero mutexes.

---

### Syscalls

| Nr | Name | Description |
|:--:|:--|:--|
| **0** | `sys_exit` | Exit process + yield |
| **1** | `sys_write` | Write to stdout/stderr (fd 1/2) |
| **2** | `sys_read` | Read from stdin (fd 0) or file descriptor |
| **3** | `sys_mmap` | Create memory mapping |
| **4** | `sys_munmap` | Remove memory mapping |
| **5** | `sys_mprotect` | Change memory protection flags |
| **6** | `sys_brk` | Extend heap |
| **7** | `sys_getpid` | Get current process PID |
| **8** | `sys_getcwd` | Get current working directory |
| **9** | `sys_set_tls` | Set FS.base register (TLS) |
| **10** | `sys_get_tls` | Get FS.base register |
| **11** | `sys_open` | Open file (VFS + ext2) |
| **12** | `sys_close` | Close file descriptor |
| **13** | `sys_seek` | Set file offset |
| **14** | `sys_fsize` | Get file size |
| **15** | `sys_map_lib` | Direct mapping of shared library |

---

### Network Stack

<details>
<summary><b>Network Card Drivers</b></summary>

| Driver | Chips |
|:--|:--|
| **Intel E1000** | 82540EM, 82545EM, 82574L, 82579LM, I217 |
| **Realtek RTL8139** | RTL8139 |
| **Realtek RTL8168** | RTL8168, RTL8169 |
| **VirtIO Net** | QEMU/KVM virtual NIC |

All drivers are auto-detected by the PCI scanner.

</details>

<details>
<summary><b>Protocols</b></summary>

| Layer | Protocols |
|:--|:--|
| **L2** | Ethernet, ARP (with cache table) |
| **L3** | IPv4, ICMP |
| **L4** | UDP, TCP (with connection state machine) |
| **Application** | DHCP, DNS, NTP, HTTP, HTTP/2, Traceroute |
| **Security** | TLS 1.3 (ECDHE + RSA + AES-GCM) |

</details>

<details>
<summary><b>TLS 1.3: Full implementation from scratch</b></summary>

- ECDH: X25519 key exchange (`tls_ecdh.rs`)
- RSA: ASN.1/DER certificate parsing, PKCS#1 signature verification (`tls_rsa.rs`)
- BigNum: custom big-integer arithmetic for RSA 2048-bit (`tls_bignum.rs`)
- AES-GCM: authenticated symmetric encryption (`tls_gcm.rs`)
- SHA-256, HMAC, HKDF: hashing, key derivation (`tls_crypto.rs`)
- Handshake: ClientHello -> ServerHello -> Certificate -> Finished

No external crates, implemented from scratch in `no_std`.

</details>

---

### VFS (Virtual File System)

<details>
<summary><b>Expand</b></summary>

#### Core Parameters

| Parameter | Value |
|:--|:--|
| **VNodes** | 256 |
| **Open files** | 32 simultaneous |
| **Mount points** | 8 |
| **Children per directory** | Dynamic (no limit) |

Children are managed by a dynamic `Vec`-based hashmap. Initial capacity is 16 slots, automatically doubling at 75% load.

- Node types: `Regular`, `Directory`, `Symlink`, `CharDevice`, `BlockDevice`, `Pipe`, `Fifo`, `Socket`
- Full metadata: permissions, uid/gid, timestamps, size, nlinks

#### System Library

On boot, a `/lib` directory is created in tmpfs and `libmiku.so` is written as an immutable file.
The immutable flag prevents unlink / write / rename.

#### Cache

| Cache | Size |
|:--|:--|
| **Page cache** | 128 pages x 512 bytes, LRU eviction |
| **Dentry cache** | 128 entries, FNV32 hash |

#### Navigation

- Path walking: max depth 32 components
- Symlink resolution: loop protection (8 levels)
- FNV32 hash: O(1) name lookup

#### Security

- UNIX permission model: `owner/group/other`, `setuid/setgid/sticky`
- Security labels (MAC), byte and inode quotas
- File locks: shared/exclusive with deadlock detection (max 16 locks)
- Immutable flag: protects system libraries

#### Advanced Features

| Feature | Details |
|:--|:--|
| **VFS journal** | 16 operation log entries |
| **Xattr** | 8 extended attributes per node |
| **Notify events** | inotify-like subsystem (max 16 events) |
| **Version store** | 16 snapshots per file |
| **CAS store** | content-addressed deduplication (max 16 objects) |
| **Block I/O queue** | 8 async requests |

</details>

---

### Filesystems

| FS | Mount point | Description |
|:--:|:--:|:--|
| **tmpfs** | `/` | RAM-based root FS |
| **devfs** | `/dev` | Devices: `null`, `zero`, `random`, `urandom`, `console` |
| **procfs** | `/proc` | `version`, `uptime`, `meminfo`, `mounts`, `cpuinfo`, `stat` |
| **ext2** | `/mnt` | Full read/write on real disk |
| **ext3** | `/mnt` | Journaling on top of ext2 (JBD2), delayed writes |
| **ext4** | `/mnt` | Extent-based files + crc32c checksums |

---

### MikuFS: Ext2/3/4 Driver

<details>
<summary><b>Expand</b></summary>

#### Read

- Superblock, group descriptors, inodes, directory entries
- Indirect blocks (single / double / triple)
- Ext4 extent tree

#### Write

- Create and delete files, directories, symlinks
- Bitmap allocator for blocks and inodes (preferred-group aware)
- Recursive delete
- Delayed write (dirty cache + pdflush)

#### Ext3 Journal (JBD2)

- Journal creation (`ext2 -> ext3` conversion)
- Transaction writing: descriptor block, commit block, revoke block
- Recovery: replay incomplete transactions on mount
- Delayed commit: accelerates journal writes via dirty cache

#### mkfs

- Format ext2/ext3/ext4
- Lazy init: only group 0 metadata initialized immediately, rest deferred
- Only journal superblock initialized (full block zeroing skipped)

#### Utilities

- `fsck`, `tree`, `du`, `cp`, `mv`, `chmod`, `chown`, hard links

</details>

---

### Userspace

<details>
<summary><b>Process Execution</b></summary>

#### ELF Execution Flow

```
1. exec("test_dynamic")
2. Kernel: reads file from ext2
3. Kernel: validates ELF header (magic, class, machine)
4. Kernel: maps PT_LOAD segments into user address space
5. Kernel: detects PT_INTERP, loads ld-miku.so
6. Kernel: builds stack (argc, argv, envp, auxv)
7. Kernel: jumps to ld-miku entry point (ring 3)
8. ld-miku: loads DT_NEEDED libraries via SYS_MAP_LIB
9. ld-miku: applies PLT/GOT relocations
10. ld-miku: jumps to executable _start
```

#### libmiku.so (Standard Library)

| Function | Description |
|:--|:--|
| `miku_write(fd, buf, len)` | Write to file descriptor |
| `miku_read(fd, buf, len)` | Read from file descriptor |
| `miku_print(str)` | Print string |
| `miku_println(str)` | Print string with newline |
| `miku_exit(code)` | Exit process |
| `miku_itoa(n, buf)` | Convert integer to string |
| `miku_strlen(str)` | Get string length |
| `miku_strcmp(a, b)` | Compare strings |
| `miku_memset(dst, val, len)` | Fill memory |
| `miku_memcpy(dst, src, len)` | Copy memory |

</details>

---

### Shell Commands

#### Unified ext Commands (auto-detect mounted FS version)

| Command | Syntax | Description |
|:--|:--|:--|
| `ext2mount` | `ext2mount [drive]` | Mount ext2 |
| `ext3mount` | `ext3mount [drive]` | Mount ext3 |
| `ext4mount` | `ext4mount [drive]` | Mount ext4 |
| `extls` | `extls [path]` | List directory |
| `extcat` | `extcat <path>` | Show file contents |
| `extstat` | `extstat <path>` | Show inode details |
| `extinfo` | `extinfo` | Show superblock info |
| `extwrite` | `extwrite <path> <text>` | Write to file (overwrites) |
| `extappend` | `extappend <path> <text>` | Append text to file |
| `exttouch` | `exttouch <path>` | Create empty file |
| `extmkdir` | `extmkdir <path>` | Create directory |
| `extrm` | `extrm [-rf] <path>` | Delete file (or recursively) |
| `extrmdir` | `extrmdir <path>` | Delete empty directory |
| `extmv` | `extmv <path> <newname>` | Rename file |
| `extcp` | `extcp <src> <dst>` | Copy file |
| `extln -s` | `extln -s <target> <link>` | Create symbolic link |
| `extlink` | `extlink <existing> <link>` | Create hard link |
| `extchmod` | `extchmod <mode> <path>` | Change permissions |
| `extchown` | `extchown <uid> <gid> <path>` | Change owner |
| `extdu` | `extdu [path]` | Show disk usage |
| `exttree` | `exttree [path]` | Show directory tree |
| `extfsck` | `extfsck` | Check filesystem integrity |
| `extcache` | `extcache` | Block cache statistics |
| `extcacheflush` | `extcacheflush` | Flush cache to disk |
| `extsync` / `sync` | `sync` | Flush everything to disk |

> Legacy commands (`ext2ls`, `ext3cat`, `ext4write`, etc.) remain as aliases for backward compatibility.

#### VFS Commands

| Command | Description |
|:--|:--|
| `ls [path]` | List directory (ext + VFS combined) |
| `cd <path>` | Change directory |
| `pwd` | Print working directory |
| `mkdir <path>` | Create directory |
| `touch <path>` | Create file in RAM |
| `cat <path>` | Show file contents |
| `write <path> <text>` | Write to file in RAM |
| `rm [-rf] <path>` | Remove file or directory |
| `rmdir <path>` | Remove directory (ext-aware) |
| `mv <old> <new>` | Rename |
| `stat <path>` | File information |
| `chmod <mode> <path>` | Change permissions |
| `df` | Filesystem info |

#### Dynamic Linking Commands

| Command | Description |
|:--|:--|
| `exec <path>` | Run ELF binary (dynamic linking supported) |
| `ldconfig` | Update shared library cache |
| `ldd` | List cached libraries |

#### mkfs Commands

| Command | Description |
|:--|:--|
| `mkfs.ext2 <drive>` | Format as ext2 |
| `mkfs.ext3 <drive>` | Format as ext3 (with journal) |
| `mkfs.ext4 <drive>` | Format as ext4 (extents + journal) |

---

### ATA Driver

| Parameter | Value |
|:--|:--|
| **Mode** | PIO (Programmed I/O) |
| **Operations** | Sector read/write (512 bytes), up to 255 sectors per command |
| **Disks** | 4: Primary/Secondary x Master/Slave |
| **Protection** | Cache flush after write, 50K iteration timeout |
| **Addressing** | LBA28 (up to 128 GB) |

---

## Build and Run

### Requirements

| Tool | Purpose |
|:--|:--|
| **Rust nightly** | `no_std` + unstable compiler features |
| **QEMU** | x86_64 machine emulation |
| **grub-mkrescue** | Bootable ISO creation |
| **NASM** | Assemble libmiku.so and test binaries |
| **Cargo** | Kernel build |

### Steps

```bash
git clone https://github.com/altushkaso2/miku-os
cd miku-os/builder
cargo run
```

The builder does everything automatically:

```
RAM saving mode? (y/N)
[1/6] Compile ld-miku.so
[2/6] Compile miku-os kernel
[3/6] Create file structure
[4/6] Generate system image (miku-os.iso)
[5/6] Prepare disk
[6/6] Launch QEMU (optional (y/N))
```

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
  <sub>Kernel - VFS - MikuFS - ELF - ld-miku - Shell - Network - TLS - Scheduler - PMM - VMM - Swap</sub>
</div>

---

## From the Author

> It all started with a simple question: what would happen if I wrote my own OS?
> Every evening - new features, new bugs, new discoveries.
> From the first character on screen to a full TLS 1.3 stack, a lock-free scheduler,
> and a dynamic linker - all written by hand.
> No ready-made libraries, no wrappers. Just Rust, documentation, and persistence :D
>
> The moment the ELF loader and dynamic linking finally worked, and "hello from dynamic linking!"
> appeared on screen - that feeling is unforgettable.

<div align="center">

**Miku OS** - a pure OS written from scratch in Rust

*With love*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">

</div>
