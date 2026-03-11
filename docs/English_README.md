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

> All code is written in Rust. Assembly is used only in the bootloader, syscall handler, and context switch routines.

---

## Technical Specification

### Kernel

| Component | Description |
|:--|:--|
| **Architecture** | x86_64, `#![no_std]`, `#![no_main]` |
| **Bootloader** | GRUB2 + Multiboot2, framebuffer (BGR/RGB auto-detect) |
| **Protection** | GDT + TSS + IST (double fault), ring 0 / ring 3 |
| **Interrupts** | IDT - timer, keyboard, page fault, GPF, double fault |
| **PIC** | PIC8259 (offsets 32/40) |
| **Heap** | 128 MB, linked-list allocator |
| **Syscall** | SYSCALL/SYSRET via MSR, naked asm handler |

---

### Memory Management

<details>
<summary><b>Physical Memory (PMM)</b></summary>

#### Frame Allocator

- Bitmap allocator - up to 4M frames (16 GB RAM), 1 bit = 1 frame of 4 KB
- `free_hint` and `contiguous_hint` - fast search for free and contiguous frames
- Contiguous alloc - reserve N frames in one request
- Regions - dynamic RAM ranges registered from Multiboot2 memory map

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
- HHDM - Higher Half Direct Map (`0xFFFF800000000000`)
- `mark_swapped()` - writes swap PTE when a page is swapped out
- ring 0 / ring 3 mapping support

</details>

<details>
<summary><b>Swap</b></summary>

#### Reverse Mapping (swap_map)

- Records `(cr3, virt_addr, age, pinned)` per physical frame
- Tracks up to 512K frames (2 GB RAM)

#### Eviction Algorithm - Clock Sweep

```
Pass 1: find frames with age >= 3 (oldest first)
Pass 2: emergency - grab any unpinned frame
```

- `touch(phys)` - resets age to 1 on page access
- `age_all()` - increments all frame ages via timer

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
| **Implementation** | Lock-free - ISR uses atomics only, no mutexes |

Context switch implemented in naked asm. `schedule_from_isr` acquires zero mutexes.

---

### Syscalls

| Nr | Name | Description |
|:--:|:--|:--|
| **0** | `sys_write` | write to stdout/stderr (fd 1/2), up to 4096 bytes |
| **1** | `sys_read` | read (stub) |
| **2** | `sys_exit` | exit process + yield |
| **3** | `sys_sleep` | sleep for N ticks |
| **4** | `sys_getpid` | get current process PID |

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
<summary><b>TLS 1.3 - Full implementation from scratch</b></summary>

- ECDH - X25519 key exchange (`tls_ecdh.rs`)
- RSA - ASN.1/DER certificate parsing, PKCS#1 signature verification (`tls_rsa.rs`)
- BigNum - custom big-integer arithmetic for RSA 2048-bit (`tls_bignum.rs`)
- AES-GCM - authenticated symmetric encryption (`tls_gcm.rs`)
- SHA-256, HMAC, HKDF - hashing, key derivation (`tls_crypto.rs`)
- Handshake - ClientHello -> ServerHello -> Certificate -> Finished

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

Children are managed by a dynamic `Vec`-based hashmap. Initial capacity is 16 slots, automatically doubling at 75% load. The old fixed limit of 32 children per directory has been removed.

- Node types: `Regular`, `Directory`, `Symlink`, `CharDevice`, `BlockDevice`, `Pipe`, `Fifo`, `Socket`
- Full metadata: permissions, uid/gid, timestamps, size, nlinks

#### Cache

| Cache | Size |
|:--|:--|
| **Page cache** | 128 pages x 512 bytes, LRU eviction |
| **Dentry cache** | 128 entries, FNV32 hash |

#### Navigation

- Path walking - max depth 32 components
- Symlink resolution - loop protection (8 levels)
- FNV32 hash - O(1) name lookup

#### Security

- UNIX permission model: `owner/group/other`, `setuid/setgid/sticky`
- Security labels (MAC), byte and inode quotas
- File locks: shared/exclusive with deadlock detection (max 16 locks)

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
| **ext3** | `/mnt` | Journaling on top of ext2 (JBD2) |
| **ext4** | `/mnt` | Extent-based files + crc32c checksums |

---

### MikuFS - Ext2/3/4 Driver

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

#### Ext3 Journal (JBD2)

- Journal creation (`ext2 -> ext3` conversion)
- Transaction writing: descriptor block, commit block, revoke block
- Recovery - replay incomplete transactions on mount

#### Utilities

- `fsck`, `tree`, `du`, `cp`, `mv`, `chmod`, `chown`, hard links

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

---

### ATA Driver

| Parameter | Value |
|:--|:--|
| **Mode** | PIO (Programmed I/O) |
| **Operations** | Sector read/write (512 bytes) |
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
[1/5] Compile miku-os kernel
[2/5] Create file structure (enter disk size and swap size)
[3/5] Generate system image (miku-os.iso)
[4/5] Prepare disk
[5/5] Launch QEMU (optional (y/N))
```

> First build takes a few minutes to download dependencies and compile the kernel.
> Subsequent builds complete in seconds.

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
  <sub>Kernel - VFS - MikuFS - Shell - Network - TLS - Scheduler - PMM - VMM - Swap</sub>
</div>

---

## From the Author

> It all started with a simple question: what would happen if I wrote my own OS?
> Since then it has become a hobby. Every evening - new features, new bugs, new discoveries.
> From the first character on screen to a full TLS 1.3 stack and a lock-free scheduler, all written by hand.
> No ready-made libraries, no wrappers. Just Rust, documentation, and persistence :D
>
> The project keeps growing. Next up: ELF loader, userspace, user processes.
> But that is a story for the next chapter of Miku OS :)

<div align="center">

**Miku OS** - a pure OS written from scratch in Rust

*With love*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">

</div>
