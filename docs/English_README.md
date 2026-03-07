<div align="center">

# 💙 Miku OS

**An experimental operating system kernel written in Rust**

*Powered by Rust and a few developers :D*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">

[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/arch-x86__64-blue.svg)]()
[![Status](https://img.shields.io/badge/status-release-green.svg)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey.svg)]()

</div>

---

> 🌐 **Documentation:** [🇷🇺 Русский](Russian_README.md) | [🇬🇧 English](English_README.md) | [🇯🇵 日本語](Japanese_README.md)

---

## About

**Miku OS** is a UNIX-like operating system built from scratch in a `no_std` environment.
No standard library (`libc`) - full control over hardware and memory architecture.

> All code is written in Rust. Assembly is used only for the bootloader, syscall handler, and context switch.

---

## Technical Specifications

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

- **Bitmap allocator** - up to 4M frames (16 GB RAM), 1 bit = 1 frame at 4KB
- **free_hint** and **contiguous_hint** - accelerate free and contiguous frame searches
- **Contiguous alloc** - allocate N frames in a single request
- **Regions** - dynamic RAM range registration from Multiboot2 memory map

#### Emergency Pool

Reserved frames for swap-in inside the page fault handler:

| Parameter | Value |
|:--|:--|
| **Pool size** | 64 frames (256 KB) |
| **Purpose** | Swap-in only, inside page fault handler |
| **Refill** | Timer ISR calls `refill_emergency_pool_tick()` at 250 Hz |
| **Reason** | Normal evict_one() calls ATA I/O - cannot be used inside fault handler |

```
alloc_frame()           - normal alloc from PMM
alloc_frame_emergency() - emergency pool only (fault handler)
alloc_or_evict()        - alloc + evict when RAM is low
alloc_for_swapin()      - emergency pool only (fault context)
```

</details>

<details>
<summary><b>Virtual Memory (VMM)</b></summary>

- **4-level page tables** (PML4 → PDP → PD → PT)
- **HHDM** - Higher Half Direct Map for kernel access to physical memory (`0xFFFF800000000000`)
- **mark_swapped()** - writes swap PTE when a page is swapped out
- Supports ring 0 / ring 3 mappings

</details>

<details>
<summary><b>Swap</b></summary>

Full swap implementation on a block device (ATA disk):

#### Reverse Mapping (swap_map)

- Records `(cr3, virt_addr, age, pinned)` for each physical frame
- Tracks up to 512K frames (2 GB RAM)

#### Eviction Algorithm - Clock Sweep

```
Pass 1: find frames with age >= 3 (oldest)
Pass 2: emergency - take any unpinned frame
```

- `touch(phys)` - resets age to 1 on page access
- `age_all()` - increments age of all frames on timer tick

#### Swap PTE Encoding

```
bit 0     = 0  (PRESENT=0 - page is not in memory)
bit 1     = 1  (SWAP_MARKER - distinguishes from unmapped)
bits 12.. = swap slot number
```

#### Eviction Flow

```
evict_one():
  1. pick_victim() from swap_map + set pinned=true
  2. swap_out_internal() → write page to disk
  3. vmm::mark_swapped() → update PTE
  4. swap_map::untrack() → remove from reverse map
  5. pmm::free_frame() → return frame
```

</details>

---

### Scheduler

| Parameter | Value |
|:--|:--|
| **Algorithm** | CFS (Completely Fair Scheduler), preemptive |
| **Max processes** | 4096 |
| **Timer frequency** | 250 Hz (PIT) |
| **CPU window** | 250 ticks (1 second) |
| **Stack** | 512 KB per process |
| **States** | `Ready`, `Running`, `Sleeping`, `Blocked`, `Dead` |
| **Implementation** | Lock-free - ISR uses only atomics, zero mutexes |

Context switch is implemented in naked asm. `schedule_from_isr` acquires zero mutexes.

---

### System Calls

Implemented via `SYSCALL/SYSRET` (MSR), naked asm handler with stack switching via `swapgs`.

| Nr | Name | Description |
|:--:|:--|:--|
| **0** | `sys_write` | Write to stdout/stderr (fd 1/2), up to 4096 bytes |
| **1** | `sys_read` | Read (stub) |
| **2** | `sys_exit` | Terminate process + yield |
| **3** | `sys_sleep` | Sleep N ticks |
| **4** | `sys_getpid` | Get current process PID |

---

### Network Stack

A complete network stack built from scratch with no third-party libraries.

<details>
<summary><b>Network Card Drivers</b></summary>

| Driver | Chip |
|:--|:--|
| **Intel E1000** | 82540EM, 82545EM, 82574L, 82579LM, I217 |
| **Realtek RTL8139** | RTL8139 |
| **Realtek RTL8168** | RTL8168, RTL8169 |
| **VirtIO Net** | QEMU/KVM virtual network card |

All drivers are automatically detected via PCI scanner.

</details>

<details>
<summary><b>Protocols</b></summary>

| Layer | Protocol |
|:--|:--|
| **L2** | Ethernet, ARP (with cache table) |
| **L3** | IPv4, ICMP |
| **L4** | UDP, TCP (with connection state management) |
| **Application** | DHCP, DNS, NTP, HTTP, HTTP/2, Traceroute |
| **Security** | TLS 1.3 (ECDHE + RSA + AES-GCM) |

</details>

<details>
<summary><b>TLS 1.3 - complete implementation from scratch</b></summary>

- **ECDH** - X25519 key exchange (`tls_ecdh.rs`)
- **RSA** - ASN.1/DER certificate parsing, PKCS#1 signature verification (`tls_rsa.rs`)
- **BigNum** - custom big-number arithmetic for RSA 2048-bit (`tls_bignum.rs`)
- **AES-GCM** - authenticated symmetric encryption (`tls_gcm.rs`)
- **SHA-256, HMAC, HKDF** - hashing, key derivation (`tls_crypto.rs`)
- **Handshake** - ClientHello → ServerHello → Certificate → Finished (`tls.rs`)

No external crates, fully in `no_std` environment.

</details>

---

### VFS (Virtual File System)

<details>
<summary><b>Expand</b></summary>

#### Core Parameters

| Parameter | Value |
|:--|:--|
| **VNodes** | 256 |
| **Simultaneous open files** | 32 |
| **Mount points** | 8 |
| **Children per directory** | 32 |

- **Node types**: `Regular`, `Directory`, `Symlink`, `CharDevice`, `BlockDevice`, `Pipe`, `Fifo`, `Socket`
- Full metadata: permissions, uid/gid, timestamps, size, nlinks

#### Caches

| Cache | Size |
|:--|:--|
| **Page cache** | 128 pages x 512 bytes, LRU eviction |
| **Dentry cache** | 128 entries, FNV32 hash |

#### Navigation

- **Path walking** - depth up to 32 components
- **Symlink resolution** - loop protection (8 levels)
- **FNV32 hash** - name hashing for O(1) lookup

#### Security

- UNIX permission model: `owner/group/other`, `setuid/setgid/sticky`
- Security labels (MAC), byte and inode quotas
- File locks: shared/exclusive with deadlock detection (up to 16 locks)

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

- Create and delete files, directories, symbolic links
- Bitmap allocator for blocks and inodes (with preferred group support)
- Recursive deletion

#### Ext3 Journal (JBD2)

- Journal creation (`ext2 → ext3` conversion)
- Transaction writes: descriptor block, commit block, revoke block
- Recovery - replays incomplete transactions on mount

#### Utilities

- `fsck` - integrity check
- `tree` - directory tree visualization
- `du`, `cp`, `mv`, `chmod`, `chown`, hard links

</details>

---

### Shell

| Feature | Description |
|:--|:--|
| **Input** | Per-character processing, mid-line insertion |
| **Navigation** | `← → Home End Delete Backspace` |
| **History** | 16 commands, `↑ ↓` to navigate |
| **Colors** | Miku theme: teal, pink, white |
| **Font** | Custom bitmap 9x16 + noto-sans-mono fallback |
| **Console** | Framebuffer rendering, auto-scroll, per-character RGB |

---

### Console and Framebuffer

<details>
<summary><b>Expand</b></summary>

Rendering is fully manual with no graphics libraries:

- **Dual rendering** - custom bitmap glyphs 9x16 + noto-sans-mono fallback
- **Shadow buffer** - per-row u32 buffer for blit acceleration (bpp=4)
- **BGR/RGB support** - auto-detects framebuffer byte order
- **Scroll** - memmove of pixel rows + clear last row
- **Per-character color** - each Cell independently stores `(ch, r, g, b)`
- **Cursor** - 2-pixel-wide vertical cursor with custom color
- **COLOR_MIKU** 💙 - default teal color

</details>

---

### ATA Driver

| Parameter | Value |
|:--|:--|
| **Mode** | PIO (Programmed I/O) |
| **Operations** | Sector read/write (512 bytes) |
| **Drives** | 4: Primary/Secondary x Master/Slave |
| **Protection** | Cache flush after write, timeout 50K iterations |

---

## Commands

Full command list is available in the **[project Wiki](https://github.com/altushkaso2/miku-os/wiki)**.

---

## Build and Run

### Requirements

| Tool | Purpose |
|:--|:--|
| **Rust nightly** | `no_std` + unstable compiler features |
| **QEMU** | x86_64 machine emulation |
| **grub-mkrescue** | Bootable ISO creation |
| **Cargo** | Building the builder and kernel |

### Steps

```bash
git clone https://github.com/altushkaso2/miku-os
cd miku-os/builder
cargo run
```

Builder handles everything automatically:

```
Low RAM mode? (y/N)
[1/5] Compiling miku-os kernel
[2/5] Creating file structure (enter disk and swap sizes)
[3/5] Generating system image (miku-os.iso)
[4/5] Preparing disks
[5/5] Launching QEMU (optional (y/N))
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
  <sub>Kernel · VFS · MikuFS · Shell · Network · TLS · Scheduler · PMM · VMM · Swap</sub>
</div>

---

## From the Author

> It all started with a simple question: "What if I wrote an OS myself?"
> Since then it became a hobby. Every evening - a new feature, a new bug, a new discovery.
> From the first character on screen to a full TLS 1.3 stack and a lock-free scheduler - all written by hand.
> No ready-made libraries or wrappers. Just Rust, documentation, and persistence :D
>
> The project keeps growing. Next up: ELF loader, userspace, and user processes.
> But that's the next chapter of Miku OS :)

<div align="center">

**Miku OS** - a pure OS written from scratch in Rust

*With love 💙*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">

</div>
