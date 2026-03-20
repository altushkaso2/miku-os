<div align="center">

# Miku OS

**Rustで開発された実験的なオペレーティングシステムカーネル**

*Rustと数人の開発者によって動いています :D*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">

[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/arch-x86__64-blue.svg)]()
[![Status](https://img.shields.io/badge/status-release-green.svg)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey.svg)]()

</div>

---

> **ドキュメント:** [Russian](docs/Russian_README.md) | [English](docs/English_README.md) | [Japanese](Japanese_README.md)

---

## プロジェクトについて

**Miku OS** は `no_std` 環境でゼロから開発されたUNIX系オペレーティングシステムです。
標準ライブラリ (`libc`) を一切使用せず、ハードウェアとメモリアーキテクチャを完全に制御します。
ELFダイナミックリンク、共有ライブラリ、ユーザースペースプロセスを独自実装で実現しています。

> すべてのコードはRustで書かれています。アセンブラはブートローダー、syscallハンドラー、コンテキストスイッチの部分にのみ使用しています。

---

## 技術仕様

### カーネル

| コンポーネント | 説明 |
|:--|:--|
| **アーキテクチャ** | x86_64、`#![no_std]`、`#![no_main]` |
| **ブートローダー** | GRUB2 + Multiboot2、フレームバッファ (BGR/RGB 自動検出) |
| **保護機能** | GDT + TSS + IST (ダブルフォルト、ページフォルト、GPF用)、ring 0 / ring 3 |
| **割り込み** | IDT: タイマー、キーボード、ページフォルト、GPF、#UD、#NM、ダブルフォルト |
| **PIC** | PIC8259 (オフセット 32/40) |
| **SSE** | CR0.EM=0、CR0.MP=1、CR4.OSFXSR=1、CR4.OSXMMEXCPT=1 |
| **ヒープ** | 32 MB、リンクリストアロケータ |
| **Syscall** | MSR経由のSYSCALL/SYSRET、naked asmハンドラー、R8/R9/R10保存 |

---

### ELFローダーとダイナミックリンク

<details>
<summary><b>ELFローダー</b></summary>

#### 機能

| 機能 | 説明 |
|:--|:--|
| **対応形式** | ET_EXEC (静的)、ET_DYN (PIE) |
| **セグメント** | PT_LOAD、PT_INTERP、PT_DYNAMIC、PT_TLS、PT_GNU_RELRO、PT_GNU_STACK |
| **リロケーション** | R_X86_64_RELATIVE、R_X86_64_JUMP_SLOT、R_X86_64_GLOB_DAT、R_X86_64_64 |
| **セキュリティ** | W^X enforcement (W+Xセグメント拒否)、RELRO |
| **ASLR** | PIEバイナリに16ビットエントロピー (65536位置、4KBステップ) |
| **スタック** | SysV ABI準拠: argc、argv、envp、auxv (16バイトアラインメント) |
| **TLS** | Thread Local Storage (FS.baseレジスタ経由) |

#### auxvエントリ

| キー | 説明 |
|:--|:--|
| AT_PHDR | プログラムヘッダーの仮想アドレス |
| AT_PHENT | プログラムヘッダーのエントリサイズ |
| AT_PHNUM | プログラムヘッダーの数 |
| AT_PAGESZ | ページサイズ (4096) |
| AT_ENTRY | 実行ファイルのエントリポイント |
| AT_BASE | インタープリターのベースアドレス |
| AT_RANDOM | 16バイトのランダムデータ |

</details>

<details>
<summary><b>ld-miku (ダイナミックリンカー)</b></summary>

#### 概要

`ld-miku` はMikuOS用のELFダイナミックリンカーです。Rustで `#![no_std]` 環境で書かれ、
静的PIEバイナリとしてコンパイルされます。

#### 処理フロー

```
1. カーネルがELFをロード → PT_INTERPを検出
2. ld-miku.soをINCLUDE_BYTESからメモリにマッピング
3. ld-miku起動 → auxvからAT_PHDR/AT_ENTRYを解析
4. DT_NEEDEDから必要なライブラリを特定
5. SYS_MAP_LIB syscallで共有ライブラリをマッピング
6. PLT/GOTリロケーションを適用
7. シンボルをグローバルテーブルにエクスポート
8. DT_INIT / DT_INIT_ARRAYを実行
9. 実行ファイルのエントリポイントにジャンプ
```

#### 特徴

- グローバルシンボルテーブル (最大1024シンボル)
- weakシンボルの解決
- 再帰的な依存ライブラリのロード (最大16ライブラリ)
- R_X86_64_COPY リロケーション対応
- envp を正しくスキップするauxv解析

</details>

<details>
<summary><b>共有ライブラリ (solib)</b></summary>

#### グローバルライブラリキャッシュ

| パラメータ | 値 |
|:--|:--|
| **最大キャッシュ数** | 32ライブラリ |
| **検索パス** | /lib、/usr/lib |
| **共有ページ** | .text/.rodata は全プロセスで同じ物理ページを共有 |
| **プライベートページ** | .data/.bss はプロセスごとにコピー |

#### SYS_MAP_LIB syscall (nr=15)

カーネルがELFセグメントを解析し、共有ライブラリを直接プロセスのアドレス空間にマッピングします。

- read-onlyセグメント → 共有物理ページ (全プロセスで同一)
- writableセグメント → プロセスごとに新規アロケーション

```
プロセスA: libmiku.so .text → 物理ページ 0x1234000 (共有)
プロセスB: libmiku.so .text → 物理ページ 0x1234000 (同じ!)
プロセスA: libmiku.so .data → 物理ページ 0x5678000 (プライベート)
プロセスB: libmiku.so .data → 物理ページ 0x9ABC000 (プライベート)
```

#### システムライブラリ

`/lib/libmiku.so` はVFS (tmpfs) にimmutableファイルとして組み込まれ、
削除、書き込み、リネームが不可能です。

#### シェルコマンド

| コマンド | 説明 |
|:--|:--|
| `ldconfig` | /lib と /usr/lib をスキャンしキャッシュを更新 |
| `ldd` | キャッシュされたライブラリの一覧表示 |

</details>

---

### メモリ管理

<details>
<summary><b>物理メモリ (PMM)</b></summary>

#### フレームアロケータ

- ビットマップアロケータ: 最大4Mフレーム (16 GB RAM)、1ビット = 1フレーム 4KB
- `free_hint` と `contiguous_hint` で空きフレームの検索を高速化
- 連続alloc: 1回のリクエストでNフレームをまとめて確保
- リージョン: Multiboot2メモリマップからRAM範囲を動的に登録

#### エマージェンシープール

| パラメータ | 値 |
|:--|:--|
| **プールサイズ** | 64フレーム (256 KB) |
| **用途** | ページフォルトハンドラー内のswap-inのみ |
| **補充** | `refill_emergency_pool_tick()` 経由でTimer ISRが250Hzごとに実行 |

```
alloc_frame()           - PMMからの通常alloc
alloc_frame_emergency() - エマージェンシープールのみ (フォルトハンドラー用)
alloc_or_evict()        - RAMが不足した場合にalloc + evict
alloc_for_swapin()      - エマージェンシープールのみ (faultコンテキスト)
```

</details>

<details>
<summary><b>仮想メモリ (VMM)</b></summary>

- 4レベルページテーブル (PML4 → PDP → PD → PT)
- HHDM: Higher Half Direct Map (`0xFFFF800000000000`)
- `mark_swapped()`: ページをスワップアウトした際のswap PTE書き込み
- ring 0 / ring 3 マッピングのサポート
- ユーザープロセス用アドレス空間の作成と破棄

</details>

<details>
<summary><b>mmap サブシステム</b></summary>

| パラメータ | 値 |
|:--|:--|
| **MMAP範囲** | 0x100000000 ~ 0x7F0000000000 |
| **BRK範囲** | 0x6000000000 ~ |
| **最大VMA** | 64エントリ |
| **機能** | mmap、munmap、mprotect、brk |

</details>

<details>
<summary><b>スワップ (Swap)</b></summary>

#### リバースマッピング (swap_map)

- 各物理フレームに `(cr3, virt_addr, age, pinned)` を記録
- 最大512Kフレーム (2 GB RAM) を追跡

#### 追い出しアルゴリズム: クロックスイープ

```
Pass 1: age >= 3 のフレームを検索 (最も古いもの)
Pass 2: 緊急時、unpinnedフレームを任意に取得
```

- `touch(phys)`: ページアクセス時にageを1にリセット
- `age_all()`: タイマーで全フレームのageを増加

#### Swap PTEエンコーディング

```
bit 0     = 0  (PRESENT=0)
bit 1     = 1  (SWAP_MARKER)
bits 12.. = スワップスロット番号
```

</details>

---

### スケジューラ

| パラメータ | 値 |
|:--|:--|
| **方式** | CFS、プリエンプティブ |
| **最大プロセス数** | 4096 |
| **タイマー周波数** | 250 Hz (PIT) |
| **CPU窓** | 250ティック (1秒) |
| **スタック** | プロセスあたり 512 KB |
| **状態** | Ready / Running / Sleeping / Blocked / Dead |
| **実装** | ロックフリー: ISRはアトミックのみ使用 |

---

### システムコール

| Nr | 名前 | 説明 |
|:--:|:--|:--|
| **0** | `sys_exit` | プロセス終了 + yield |
| **1** | `sys_write` | stdout/stderrへの書き込み (fd 1/2) |
| **2** | `sys_read` | stdin (fd 0) またはファイルディスクリプタからの読み込み |
| **3** | `sys_mmap` | メモリマッピングの作成 |
| **4** | `sys_munmap` | メモリマッピングの解除 |
| **5** | `sys_mprotect` | メモリ保護属性の変更 |
| **6** | `sys_brk` | ヒープの拡張 |
| **7** | `sys_getpid` | 現在のプロセスのPIDを取得 |
| **8** | `sys_getcwd` | カレントディレクトリの取得 |
| **9** | `sys_set_tls` | FS.baseレジスタの設定 (TLS) |
| **10** | `sys_get_tls` | FS.baseレジスタの取得 |
| **11** | `sys_open` | ファイルを開く (VFS + ext2) |
| **12** | `sys_close` | ファイルディスクリプタを閉じる |
| **13** | `sys_seek` | ファイルオフセットの設定 |
| **14** | `sys_fsize` | ファイルサイズの取得 |
| **15** | `sys_map_lib` | 共有ライブラリの直接マッピング |

---

### ネットワークスタック

<details>
<summary><b>ネットワークカードドライバ</b></summary>

| ドライバ | チップ |
|:--|:--|
| **Intel E1000** | 82540EM、82545EM、82574L、82579LM、I217 |
| **Realtek RTL8139** | RTL8139 |
| **Realtek RTL8168** | RTL8168、RTL8169 |
| **VirtIO Net** | QEMU/KVM仮想ネットワークカード |

</details>

<details>
<summary><b>プロトコル</b></summary>

| レイヤー | プロトコル |
|:--|:--|
| **L2** | Ethernet、ARP (キャッシュテーブル付き) |
| **L3** | IPv4、ICMP |
| **L4** | UDP、TCP (コネクション状態管理付き) |
| **アプリケーション** | DHCP、DNS、NTP、HTTP、HTTP/2、Traceroute |
| **セキュリティ** | TLS 1.3 (ECDHE + RSA + AES-GCM) |

</details>

<details>
<summary><b>TLS 1.3: ゼロからの完全実装</b></summary>

- ECDH: X25519鍵交換 (`tls_ecdh.rs`)
- RSA: ASN.1/DER証明書のパース、PKCS#1署名検証 (`tls_rsa.rs`)
- BigNum: RSA 2048-bit用の独自大数演算実装 (`tls_bignum.rs`)
- AES-GCM: 認証付き対称暗号化 (`tls_gcm.rs`)
- SHA-256、HMAC、HKDF: ハッシュ化、鍵導出 (`tls_crypto.rs`)
- ハンドシェイク: ClientHello → ServerHello → Certificate → Finished

</details>

---

### VFS (仮想ファイルシステム)

<details>
<summary><b>展開する</b></summary>

#### 基本機能

| パラメータ | 値 |
|:--|:--|
| **VNode数** | 256 |
| **同時オープンファイル数** | 32 |
| **マウントポイント** | 8 |
| **子ノード数** | 動的 (上限なし) |

子ノードは動的 `Vec` ベースのハッシュマップで管理されます。初期スロット数は16で、使用率75%に達すると自動的に2倍に拡張されます。

- ノードタイプ: `Regular`、`Directory`、`Symlink`、`CharDevice`、`BlockDevice`、`Pipe`、`Fifo`、`Socket`
- 権限、uid/gid、タイムスタンプ、サイズ、nlinksの完全なメタデータ付き

#### システムライブラリ

ブート時に `/lib` ディレクトリをtmpfsに作成し、`libmiku.so` をimmutableファイルとして書き込みます。
immutableフラグにより unlink / write / rename は拒否されます。

#### キャッシュ

| キャッシュ | サイズ |
|:--|:--|
| **ページキャッシュ** | 128ページ x 512バイト、LRU追い出し |
| **Dentryキャッシュ** | 128エントリ、FNV32ハッシュ |

#### ナビゲーション

- パスウォーキング: 深さ最大32コンポーネント
- シンボリックリンク解決: ループ保護 (8レベル)
- FNV32ハッシュ: O(1)ルックアップのための名前ハッシュ化

#### セキュリティ

- UNIXパーミッションモデル: `owner/group/other`、`setuid/setgid/sticky`
- セキュリティラベル (MAC)、バイトとinode単位のクォータ
- ファイルロック: デッドロック検出付きshared/exclusive (最大16ロック)
- immutableフラグ: システムライブラリの保護

#### 高度な機能

| 機能 | 詳細 |
|:--|:--|
| **VFSジャーナル** | 16件の操作ログ |
| **Xattr** | ノードあたり8つの拡張属性 |
| **Notifyイベント** | inotify的サブシステム (最大16イベント) |
| **バージョンストア** | ファイルの16スナップショット |
| **CASストア** | コンテンツアドレス指定の重複排除 (最大16オブジェクト) |
| **ブロックI/Oキュー** | 8件の非同期リクエスト |

</details>

---

### ファイルシステム

| FS | マウントポイント | 説明 |
|:--:|:--:|:--|
| **tmpfs** | `/` | RAMベースのルートFS |
| **devfs** | `/dev` | デバイス: `null`、`zero`、`random`、`urandom`、`console` |
| **procfs** | `/proc` | `version`、`uptime`、`meminfo`、`mounts`、`cpuinfo`、`stat` |
| **ext2** | `/mnt` | 実ディスクへの完全な読み書き |
| **ext3** | `/mnt` | ext2上のジャーナリング (JBD2)、遅延書き込み |
| **ext4** | `/mnt` | エクステントベースファイル + crc32cチェックサム |

---

### MikuFS: Ext2/3/4ドライバ

<details>
<summary><b>展開する</b></summary>

#### 読み込み

- スーパーブロック、グループディスクリプタ、inode、ディレクトリエントリ
- 間接ブロック (シングル / ダブル / トリプル)
- Ext4エクステントツリー

#### 書き込み

- ファイル、ディレクトリ、シンボリックリンクの作成と削除
- ブロックとinode用ビットマップアロケータ (優先グループ対応)
- 再帰的な削除
- 遅延書き込み (dirty cache + pdflush)

#### Ext3ジャーナル (JBD2)

- ジャーナルの作成 (`ext2 → ext3` 変換)
- トランザクションの書き込み: ディスクリプタブロック、コミットブロック、revokeブロック
- リカバリ: マウント時に未完了トランザクションをリプレイ
- 遅延コミット: journal書き込みをdirty cacheで高速化

#### mkfs

- ext2/ext3/ext4のフォーマット対応
- lazy init: group 0のメタデータのみ即時初期化、残りは遅延
- ジャーナルスーパーブロックのみ初期化 (全ブロックの零化を省略)

#### ユーティリティ

- `fsck`、`tree`、`du`、`cp`、`mv`、`chmod`、`chown`、ハードリンク

</details>

---

### ユーザースペース

<details>
<summary><b>プロセス実行</b></summary>

#### ELF実行フロー

```
1. exec("test_dynamic")
2. カーネル: ext2からファイル読み込み
3. カーネル: ELFヘッダー検証 (magic、class、machine)
4. カーネル: PT_LOADセグメントをユーザーアドレス空間にマッピング
5. カーネル: PT_INTERP検出 → ld-miku.soをロード
6. カーネル: スタック構築 (argc、argv、envp、auxv)
7. カーネル: ld-mikuのエントリポイントにジャンプ (ring 3)
8. ld-miku: DT_NEEDEDライブラリをSYS_MAP_LIBでロード
9. ld-miku: PLT/GOTリロケーション適用
10. ld-miku: 実行ファイルの_startにジャンプ
```

#### libmiku.so (標準ライブラリ)

| 関数 | 説明 |
|:--|:--|
| `miku_write(fd, buf, len)` | ファイルディスクリプタへの書き込み |
| `miku_read(fd, buf, len)` | ファイルディスクリプタからの読み込み |
| `miku_print(str)` | 文字列の出力 |
| `miku_println(str)` | 文字列の出力 + 改行 |
| `miku_exit(code)` | プロセス終了 |
| `miku_itoa(n, buf)` | 整数を文字列に変換 |
| `miku_strlen(str)` | 文字列の長さを取得 |
| `miku_strcmp(a, b)` | 文字列比較 |
| `miku_memset(dst, val, len)` | メモリ塗りつぶし |
| `miku_memcpy(dst, src, len)` | メモリコピー |

</details>

---

### シェルコマンド

#### 統合extコマンド (マウントされたFSバージョンを自動検出)

| コマンド | 構文 | 説明 |
|:--|:--|:--|
| `ext2mount` | `ext2mount [drive]` | ext2マウント |
| `ext3mount` | `ext3mount [drive]` | ext3マウント |
| `ext4mount` | `ext4mount [drive]` | ext4マウント |
| `extls` | `extls [path]` | ディレクトリ一覧 |
| `extcat` | `extcat <path>` | ファイル内容表示 |
| `extstat` | `extstat <path>` | inodeの詳細 |
| `extinfo` | `extinfo` | スーパーブロック情報 |
| `extwrite` | `extwrite <path> <text>` | ファイルへの書き込み |
| `extappend` | `extappend <path> <text>` | ファイルへの追記 |
| `exttouch` | `exttouch <path>` | 空ファイルの作成 |
| `extmkdir` | `extmkdir <path>` | ディレクトリの作成 |
| `extrm` | `extrm [-rf] <path>` | ファイルの削除 |
| `extrmdir` | `extrmdir <path>` | 空ディレクトリの削除 |
| `extmv` | `extmv <path> <newname>` | ファイルの改名 |
| `extcp` | `extcp <src> <dst>` | ファイルのコピー |
| `extln -s` | `extln -s <target> <link>` | シンボリックリンクの作成 |
| `extlink` | `extlink <existing> <link>` | ハードリンクの作成 |
| `extchmod` | `extchmod <mode> <path>` | パーミッションの変更 |
| `extchown` | `extchown <uid> <gid> <path>` | 所有者の変更 |
| `extdu` | `extdu [path]` | ディスク使用量 |
| `exttree` | `exttree [path]` | ディレクトリツリー |
| `extfsck` | `extfsck` | FSの整合性チェック |
| `extcache` | `extcache` | ブロックキャッシュ統計 |
| `extcacheflush` | `extcacheflush` | キャッシュのフラッシュ |
| `extsync` / `sync` | `sync` | ディスクへの書き込み |

> 旧コマンド (`ext2ls`、`ext3cat`、`ext4write` 等) は後方互換性のために残っています。

#### VFSコマンド

| コマンド | 説明 |
|:--|:--|
| `ls [path]` | ディレクトリ一覧 (ext + VFS統合表示) |
| `cd <path>` | ディレクトリ移動 |
| `pwd` | 現在のパス表示 |
| `mkdir <path>` | ディレクトリ作成 |
| `touch <path>` | ファイル作成 (RAM) |
| `cat <path>` | ファイル内容表示 |
| `write <path> <text>` | ファイルへの書き込み (RAM) |
| `rm [-rf] <path>` | ファイル/ディレクトリ削除 |
| `rmdir <path>` | ディレクトリ削除 (ext対応) |
| `mv <old> <new>` | 改名 |
| `stat <path>` | ファイル情報 |
| `chmod <mode> <path>` | パーミッション変更 |
| `df` | ファイルシステム情報 |

#### ダイナミックリンクコマンド

| コマンド | 説明 |
|:--|:--|
| `exec <path>` | ELFバイナリの実行 (ダイナミックリンク対応) |
| `ldconfig` | 共有ライブラリキャッシュの更新 |
| `ldd` | キャッシュされたライブラリの一覧表示 |

#### mkfsコマンド

| コマンド | 説明 |
|:--|:--|
| `mkfs.ext2 <drive>` | ext2フォーマット |
| `mkfs.ext3 <drive>` | ext3フォーマット (ジャーナル付き) |
| `mkfs.ext4 <drive>` | ext4フォーマット (エクステント + ジャーナル) |

---

### ATAドライバ

| パラメータ | 値 |
|:--|:--|
| **モード** | PIO (プログラムI/O) |
| **操作** | セクターの読み書き (512バイト)、最大255セクター/コマンド |
| **ディスク数** | 4台: Primary/Secondary x Master/Slave |
| **保護** | 書き込み後のキャッシュフラッシュ、タイムアウト 50Kイテレーション |
| **アドレス指定** | LBA28 (最大128GB) |

---

## ビルドと実行

### 必要なツール

| ツール | 用途 |
|:--|:--|
| **Rust nightly** | `no_std` + コンパイラの不安定な機能 |
| **QEMU** | x86_64マシンのエミュレーション |
| **grub-mkrescue** | ブータブルISOの作成 |
| **NASM** | libmiku.soとテストバイナリのアセンブル |
| **Cargo** | カーネルのビルド |

### 実行手順

```bash
git clone https://github.com/altushkaso2/miku-os
cd miku-os/builder
cargo run
```

Builderがすべて自動で行います:

```
RAMの節約モード? (y/N)
[1/6] ld-miku.soのコンパイル
[2/6] miku-osカーネルのコンパイル
[3/6] ファイル構造の作成
[4/6] システムイメージの生成 (miku-os.iso)
[5/6] ディスクの準備
[6/6] QEMUの起動 (任意 (y/N))
```

---

## 作者

<div align="center">
  <a href="https://github.com/altushkaso2">
    <img src="https://github.com/altushkaso2.png" width="100" style="border-radius:50%;" alt="altushkaso2">
  </a>
  <br><br>
  <a href="https://github.com/altushkaso2"><b>@altushkaso2</b></a>
  <br>
  <sub>Miku OSの作者および唯一の開発者</sub>
  <br>
  <sub>カーネル - VFS - MikuFS - ELF - ld-miku - シェル - ネットワーク - TLS - スケジューラ - PMM - VMM - Swap</sub>
</div>

---

## 作者より

> すべては「自分でOSを書いてみたらどうなるだろう?」というシンプルな思いから始まりました。
> 毎晩、新しい機能を追加し、新しいバグを直し、新しい発見をしています。
> 画面への最初の文字表示から、本格的なTLS 1.3スタック、ロックフリースケジューラ、
> そしてダイナミックリンカーまで、すべて手作業で書きました。
> 既製のライブラリやラッパーは一切なし。Rustとドキュメントと根気だけです :D
>
> ELFローダーとダイナミックリンクが動いた瞬間、「hello from dynamic linking!」が
> 画面に表示された時の感動は忘れられません。

<div align="center">

**Miku OS** - Rustでゼロから書かれた純粋なOS

*愛を込めて*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">
