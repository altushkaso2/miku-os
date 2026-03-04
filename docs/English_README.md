<div align="center">

# 💙 Miku OS

**An experimental operating system kernel written in Rust**

*Powered by Rust, and a couple of developers :D*

<img src="miku.png" width="220" alt="Miku Logo">

[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/arch-x86__64-blue.svg)]()
[![Status](https://img.shields.io/badge/status-release-green.svg)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey.svg)]()

</div>

---

> 🌐 **Documentation:** [🇷🇺 Русский](Russian_documentation.md) | [🇬🇧 English](English_documentation.md) | [🇯🇵 日本語](Japanese_documentation.md)

---

## About

**Miku OS** is a UNIX-like operating system developed from scratch in `no_std` mode.
No standard library (`libc`) - full control over hardware and memory architecture.

> All code is written in Rust. Assembly is used exclusively for the bootloader, syscall handler, and context switching.

---

## Technical Specifications

### Kernel

| Component | Description |
|:--|:--|
| **Architecture** | x86_64, `#![no_std]`, `#![no_main]` |
| **Bootloader** | Limine protocol, framebuffer 1280x800 (BGR) |
| **Protection** | GDT + TSS + IST for double fault, ring 0 / ring 3 |
| **Interrupts** | IDT - timer, keyboard, page fault, GPF, double fault |
| **PIC** | PIC8259 (offset 32/40) |
| **Heap** | 256 KB, linked-list allocator |
| **Syscall** | SYSCALL/SYSRET via MSR, naked asm handler |

---

### Memory Management

<details>
<summary><b>Physical Memory (PMM)</b></summary>

#### Frame Allocator

- **Bitmap allocator** - up to 4M frames (16 GB RAM), each bit = one 4KB frame
- **free_hint** and **contiguous_hint** - speed up search for free and contiguous frames
- **Contiguous alloc** - allocate N contiguous frames in a single request
- **Regions** - dynamic registration of RAM ranges from Multiboot2 memory map

#### Emergency Pool

A dedicated frame reserve exclusively for swap-in inside the page fault handler:

| Parameter | Value |
|:--|:--|
| **Pool size** | 64 frames (256 KB) |
| **Purpose** | Swap-in inside page fault handler only |
| **Refill** | Timer ISR every ~250ms via `refill_emergency_pool_tick()` |
| **Reason** | Normal evict_one() calls ATA I/O - cannot be used inside a fault handler |

```
alloc_frame()           - normal alloc from PMM
alloc_frame_emergency() - emergency pool only (for fault handler)
alloc_or_evict()        - alloc + evict if RAM is exhausted
alloc_for_swapin()      - emergency pool only (fault context)
```

</details>

<details>
<summary><b>Virtual Memory (VMM)</b></summary>

- **4-level page tables** (PML4 -> PDP -> PD -> PT)
- **HHDM** - Higher Half Direct Map for kernel access to physical memory
- **mark_swapped()** - writes swap PTE when a page is evicted
- Support for ring 0 / ring 3 mapping

</details>

<details>
<summary><b>Swap</b></summary>

Full swap implementation on a block device (ATA disk):

#### Reverse Mapping (swap_map)

- Each physical frame stores `(cr3, virt_addr, age, pinned)`
- Tracks up to 512K frames (2 GB RAM)

#### Eviction Algorithm - Clock Sweep

```
Pass 1: looks for a frame with age >= 3 (oldest)
Pass 2: emergency - takes any unpinned frame
```

- `touch(phys)` - resets age to 1 on page access
- `age_all()` - increments age for all frames (called by timer)

#### Swap PTE Encoding

```
bit 0     = 0  (PRESENT=0 - page is not in memory)
bit 1     = 1  (SWAP_MARKER - distinguishes from unmapped)
bits 12.. = swap slot number
```

#### Eviction Flow

```
evict_one():
  1. pick_victim() from swap_map
  2. swap_out_internal() -> write page to disk
  3. vmm::mark_swapped() -> update PTE
  4. swap_map::untrack() -> remove from reverse map
  5. pmm::free_frame() -> return frame
```

</details>

---

### Scheduler

| Parameter | Value |
|:--|:--|
| **Type** | Round-robin, preemptive |
| **Processes** | Up to 16 simultaneously |
| **Switch** | Every 20 timer ticks (~200ms) |
| **Context** | r15, r14, r13, r12, rbx, rbp, rip, rsp, rflags |
| **Stack** | 16 KB per process |
| **States** | `Ready`, `Running`, `Dead` |

Context switching is implemented in naked asm - full register save and restore without compiler involvement.

---

### System Calls

Implemented via `SYSCALL/SYSRET` (MSR), naked asm handler with `swapgs` for stack switching.

| Nr | Name | Description |
|:--:|:--|:--|
| **0** | `sys_write` | Write to stdout/stderr (fd 1/2), up to 4096 bytes |
| **1** | `sys_read` | Read (stub) |
| **2** | `sys_exit` | Terminate process + yield |
| **3** | `sys_sleep` | Sleep for N ticks |
| **4** | `sys_getpid` | Get PID of the current process |

---

### Network Stack

A complete network stack implemented from scratch without any third-party libraries.

<details>
<summary><b>Network Card Drivers</b></summary>

| Driver | Chips |
|:--|:--|
| **Intel E1000** | 82540EM, 82545EM, 82574L, 82579LM, I217 |
| **Realtek RTL8139** | RTL8139 |
| **Realtek RTL8168** | RTL8168, RTL8169 |
| **VirtIO Net** | QEMU/KVM virtual network card |

All drivers are detected automatically via PCI scanner.

</details>

<details>
<summary><b>Protocols</b></summary>

| Layer | Protocols |
|:--|:--|
| **L2** | Ethernet, ARP (cached table) |
| **L3** | IPv4, ICMP |
| **L4** | UDP, TCP (stateful) |
| **Application** | DHCP, DNS, NTP, HTTP, Traceroute |
| **Security** | TLS 1.2 (RSA + AES-128-CBC + SHA) |

</details>

<details>
<summary><b>TLS 1.2 - full implementation from scratch</b></summary>

- **RSA** - ASN.1/DER certificate parsing, PKCS#1 encryption
- **BigNum** - custom big number arithmetic for RSA 2048-bit
- **AES-128-CBC** - symmetric encryption
- **SHA-1, SHA-256, HMAC** - hashing and authentication
- **PRF** - key derivation per RFC 5246
- **Handshake** - full flow: ClientHello -> Certificate -> ClientKeyExchange -> Finished

Verified against real Google servers (TLS RSA 2048, port 443).

</details>

---

### VFS (Virtual File System)

<details>
<summary><b>Expand</b></summary>

#### Core
- **64 VNodes** with full metadata - permissions, uid/gid, timestamps, size, nlinks
- **Node types**: `Regular`, `Directory`, `Symlink`, `CharDevice`, `BlockDevice`, `Pipe`, `Fifo`, `Socket`
- **32 open files** simultaneously, **8 mount points**

#### Caching
- **Page Cache** - 32 pages x 512 bytes, LRU eviction
- **Dentry Cache** - 128 entries, FNV32 hashing, hit/miss statistics
- **Slab Allocator** - fast page allocation

#### Navigation
- **Path walking** - depth up to 32 components
- **Symlink resolution** - loop protection (8 levels)
- **FNV32 hashing** of names for O(1) lookup

#### Security
- UNIX permission model: `owner/group/other`, `setuid/setgid/sticky`
- Security labels (MAC), byte and inode quotas
- File locking: shared/exclusive with deadlock detection

#### Advanced Features
- **VFS journal** - 16 operation log entries
- **Transactions** - 4 concurrent with rollback
- **Xattr** - 8 extended attributes per node (16 byte name, 32 byte value)
- **Notify events** - inotify-like subsystem
- **Version store** - 16 file snapshots
- **CAS store** - content-addressable deduplication
- **Block I/O queue** - 8 async requests

</details>

---

### Filesystems

| FS | Mount point | Description |
|:--:|:--:|:--|
| **tmpfs** | `/` | RAM-based root FS |
| **devfs** | `/dev` | Devices: `null`, `zero`, `random`, `urandom`, `console` |
| **procfs** | `/proc` | `version`, `uptime`, `meminfo`, `mounts`, `cpuinfo`, `stat` |
| **ext2** | `/mnt` | Full read and write on real disk |
| **ext3** | `/mnt` | Journaling on top of ext2 (JBD2) |
| **ext4** | `/mnt` | Extent-based files + crc32c checksums |

---

### MikuFS - Ext2/3/4 Driver

<details>
<summary><b>Expand</b></summary>

#### Reading
- Superblock, group descriptors, inodes, directory entries
- Indirect blocks (single / double / triple)
- Ext4 extent tree

#### Writing
- Create/delete files, directories, symlinks
- Bitmap allocator for blocks and inodes (preferred group)
- Recursive deletion

#### Ext3 Journal (JBD2)
- Journal creation (`ext2 -> ext3` conversion)
- Transaction writing: descriptor blocks, commit blocks, revoke blocks
- Recovery - replay of incomplete transactions on mount

#### Utilities
- `fsck` - integrity check
- `tree` - directory tree visualization
- `du`, `cp`, `mv`, `chmod`, `chown`, hardlink

</details>

---

### Shell

| Feature | Description |
|:--|:--|
| **Input** | Per-character processing, mid-line insertion |
| **Navigation** | `<- -> Home End Delete Backspace` |
| **History** | 16 commands, navigate with `Up Down` |
| **Colors** | miku theme: teal, pink, white |
| **Font** | Custom bitmap 9x16 + noto-sans-mono fallback |
| **Console** | Framebuffer rendering, auto-scroll, RGB per-character |

---

### Console and Framebuffer

<details>
<summary><b>Expand</b></summary>

Rendering is implemented entirely by hand, without any graphics libraries:

- **Dual rendering** - custom bitmap glyphs 9x16 + noto-sans-mono as fallback
- **Shadow buffer** - per-row u32 buffer for fast blit operations (bpp=4)
- **BGR/RGB support** - automatic framebuffer byte order detection
- **Scrolling** - memmove of pixel rows + last row clear
- **Per-character color** - each Cell stores `(ch, r, g, b)` independently
- **Cursor** - 2-pixel wide vertical cursor with custom color
- **COLOR_MIKU** 💙 - the signature teal color by default

</details>

---

### ATA Driver

| Parameter | Value |
|:--|:--|
| **Mode** | PIO (Programmed I/O) |
| **Operations** | Read / Write sectors (512 bytes) |
| **Disks** | 4 units: Primary/Secondary x Master/Slave |
| **Protection** | Cache flush after write, timeout 500K iterations |

---

## Commands

The full list of commands is available in the **[project Wiki](https://github.com/altushkaso2/miku-os/wiki)**.

---

## Build and Run

### Requirements

| Tool | Purpose |
|:--|:--|
| **Rust nightly** | `no_std` + unstable compiler features |
| **QEMU** | x86_64 machine emulation |
| **Cargo** | Building the builder and kernel |

### Running

```bash
git clone https://github.com/altushkaso2/miku-os
cd miku-os/builder
cargo run
```

The builder does everything automatically:

```
Low RAM mode? (y/N)
[1/5] Compiling miku-os kernel
[2/5] Creating file structure (enter disk size and swap size)
[3/5] Generating system image (miku-os.iso)
[4/5] Preparing disk
[5/5] Launch QEMU? (y/N)
```

> The first build takes a couple of minutes - dependencies are downloaded and the kernel is compiled.
> Subsequent runs take seconds.

---

## Authors

<div align="center">
  <a href="https://github.com/altushkaso2">
    <img src="https://github.com/altushkaso2.png" width="100" style="border-radius:50%;" alt="altushkaso2">
  </a>
  <br><br>
  <a href="https://github.com/altushkaso2"><b>@altushkaso2</b></a>
  <br>
  <sub>Creator and sole developer of Miku OS</sub>
  <br>
  <sub>Kernel · VFS · MikuFS · Shell · Network · TLS · Scheduler · PMM · VMM · Swap</sub>
</div>

---

## From the Author

> It all started with a simple thought - "what if I just wrote my own operating system?".
> Since then it became a hobby. Every evening - a new feature, a new bug, a new discovery.
> From the first character on the screen to a full TLS stack and scheduler - everything is written by hand,
> no ready-made libraries or wrappers. Just Rust, documentation, and persistence :D
>
> The project is alive and evolving. Ahead - ELF loader, userspace, user processes.
> But that is the next chapter waiting for Miku OS :)

<div align="center">

**Miku OS** - a pure OS written in Rust from scratch

*With love 💙*

<img src="miku.png" width="70" alt="Miku">

If you like the project - give it a star! ⭐

</div>
