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

> すべてのコードはRustで書かれています。アセンブラはブートローダー、syscallハンドラー、コンテキストスイッチの部分にのみ使用しています。

---

## 技術仕様

### カーネル

| コンポーネント | 説明 |
|:--|:--|
| **アーキテクチャ** | x86_64、`#![no_std]`、`#![no_main]` |
| **ブートローダー** | GRUB2 + Multiboot2、フレームバッファ (BGR/RGB 自動検出) |
| **保護機能** | GDT + TSS + IST (ダブルフォルト用)、ring 0 / ring 3 |
| **割り込み** | IDT - タイマー、キーボード、ページフォルト、GPF、ダブルフォルト |
| **PIC** | PIC8259 (オフセット 32/40) |
| **ヒープ** | 128 MB、リンクリストアロケータ |
| **Syscall** | MSR経由のSYSCALL/SYSRET、naked asmハンドラー |

---

### メモリ管理

<details>
<summary><b>物理メモリ (PMM)</b></summary>

#### フレームアロケータ

- ビットマップアロケータ - 最大4Mフレーム (16 GB RAM)、1ビット = 1フレーム 4KB
- `free_hint` と `contiguous_hint` - 空きフレームと連続フレームの検索を高速化
- 連続alloc - 1回のリクエストでNフレームをまとめて確保
- リージョン - Multiboot2メモリマップからRAM範囲を動的に登録

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

- 4レベルページテーブル (PML4 -> PDP -> PD -> PT)
- HHDM - Higher Half Direct Map (`0xFFFF800000000000`)
- `mark_swapped()` - ページをスワップアウトした際のswap PTE書き込み
- ring 0 / ring 3 マッピングのサポート

</details>

<details>
<summary><b>スワップ (Swap)</b></summary>

#### リバースマッピング (swap_map)

- 各物理フレームに `(cr3, virt_addr, age, pinned)` を記録
- 最大512Kフレーム (2 GB RAM) を追跡

#### 追い出しアルゴリズム - クロックスイープ

```
Pass 1: age >= 3 のフレームを検索 (最も古いもの)
Pass 2: 緊急時 - unpinnedフレームを任意に取得
```

- `touch(phys)` - ページアクセス時にageを1にリセット
- `age_all()` - タイマーで全フレームのageを増加

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
| **実装** | ロックフリー - ISRはアトミックのみ使用 |

---

### システムコール

| Nr | 名前 | 説明 |
|:--:|:--|:--|
| **0** | `sys_write` | stdout/stderrへの書き込み (fd 1/2)、最大4096バイト |
| **1** | `sys_read` | 読み込み (スタブ) |
| **2** | `sys_exit` | プロセス終了 + yield |
| **3** | `sys_sleep` | Nティック間スリープ |
| **4** | `sys_getpid` | 現在のプロセスのPIDを取得 |

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
<summary><b>TLS 1.3 - ゼロからの完全実装</b></summary>

- ECDH - X25519鍵交換 (`tls_ecdh.rs`)
- RSA - ASN.1/DER証明書のパース、PKCS#1署名検証 (`tls_rsa.rs`)
- BigNum - RSA 2048-bit用の独自大数演算実装 (`tls_bignum.rs`)
- AES-GCM - 認証付き対称暗号化 (`tls_gcm.rs`)
- SHA-256、HMAC、HKDF - ハッシュ化、鍵導出 (`tls_crypto.rs`)
- ハンドシェイク - ClientHello -> ServerHello -> Certificate -> Finished

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

子ノードは動的 `Vec` ベースのハッシュマップで管理されます。初期スロット数は16で、使用率75%に達すると自動的に2倍に拡張されます。以前の固定32件制限は廃止されました。

- ノードタイプ: `Regular`、`Directory`、`Symlink`、`CharDevice`、`BlockDevice`、`Pipe`、`Fifo`、`Socket`
- 権限、uid/gid、タイムスタンプ、サイズ、nlinksの完全なメタデータ付き

#### キャッシュ

| キャッシュ | サイズ |
|:--|:--|
| **ページキャッシュ** | 128ページ x 512バイト、LRU追い出し |
| **Dentryキャッシュ** | 128エントリ、FNV32ハッシュ |

#### ナビゲーション

- パスウォーキング - 深さ最大32コンポーネント
- シンボリックリンク解決 - ループ保護 (8レベル)
- FNV32ハッシュ - O(1)ルックアップのための名前ハッシュ化

#### セキュリティ

- UNIXパーミッションモデル: `owner/group/other`、`setuid/setgid/sticky`
- セキュリティラベル (MAC)、バイトとinode単位のクォータ
- ファイルロック: デッドロック検出付きshared/exclusive (最大16ロック)

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
| **ext3** | `/mnt` | ext2上のジャーナリング (JBD2) |
| **ext4** | `/mnt` | エクステントベースファイル + crc32cチェックサム |

---

### MikuFS - Ext2/3/4ドライバ

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

#### Ext3ジャーナル (JBD2)

- ジャーナルの作成 (`ext2 -> ext3` 変換)
- トランザクションの書き込み: ディスクリプタブロック、コミットブロック、revokeブロック
- リカバリ - マウント時に未完了トランザクションをリプレイ

#### ユーティリティ

- `fsck`、`tree`、`du`、`cp`、`mv`、`chmod`、`chown`、ハードリンク

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

---

### ATAドライバ

| パラメータ | 値 |
|:--|:--|
| **モード** | PIO (プログラムI/O) |
| **操作** | セクターの読み書き (512バイト) |
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
[1/5] miku-osカーネルのコンパイル
[2/5] ファイル構造の作成
[3/5] システムイメージの生成 (miku-os.iso)
[4/5] ディスクの準備
[5/5] QEMUの起動 (任意 (y/N))
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
  <sub>カーネル - VFS - MikuFS - シェル - ネットワーク - TLS - スケジューラ - PMM - VMM - Swap</sub>
</div>

---

## 作者より

> すべては「自分でOSを書いてみたらどうなるだろう?」というシンプルな思いから始まりました。
> 毎晩、新しい機能を追加し、新しいバグを直し、新しい発見をしています。
> 画面への最初の文字表示から、本格的なTLS 1.3スタックとロックフリースケジューラまで、すべて手作業で書きました。
> 既製のライブラリやラッパーは一切なし。Rustとドキュメントと根気だけです :D
>
> プロジェクトは成長し続けています。次はELFローダー、ユーザースペース、ユーザープロセスが待っています。

<div align="center">

**Miku OS** - Rustでゼロから書かれた純粋なOS

*愛を込めて*

<img src="https://raw.githubusercontent.com/altushkaso2/miku-os/main/docs/miku.png" width="220" alt="Miku Logo">

</div>
