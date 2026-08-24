# NekoLib 🐾 — Pure-CPU Portable Layer-1 Blockchain

> **100k cap ever • harder every 1k • 10 MB binary • runs on your laptop, Pi, and phone**
>
> **Author & ultimate authority:** `Vaibhav` — all decisions, merges, releases, and consensus constants require Vaibhav's approval. Licensed **GPLv3**.

<p align="center">
  <img src="https://img.shields.io/badge/license-GPLv3-blue" alt="GPLv3">
  <img src="https://img.shields.io/badge/Rust-stable-orange" alt="Rust stable">
  <img src="https://img.shields.io/badge/cost-%240.00-brightgreen" alt="zero cost">
  <img src="https://img.shields.io/badge/supply-100k_neko-magenta" alt="100k">
</p>

---

## 🚀 SUPER EASY — For Anyone (Never Opened Terminal Before)

**Copy → Paste → Enter. That's it. Script asks permission, shows time, does everything.**

### Linux / macOS / WSL / Raspberry Pi
```bash
curl -fsSL https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.sh | bash
```
*or if `curl` missing:*
```bash
wget -qO- https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.sh | bash
```

### Windows (PowerShell — Run as Admin)
```powershell
irm https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.ps1 | iex
```
*or download `install.ps1` → right-click → Run with PowerShell*

### Android (Termux — no laptop needed)
1. Install **Termux** from [F-Droid](https://f-droid.org/en/packages/com.termux/) (not Play Store)
2. Open Termux and paste:
```bash
curl -fsSL https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.sh | bash
```
That's it — binary runs natively in Termux.

### iOS
Requires Mac + Xcode. See **Nerd Guide → iOS** below.

---

### What the one-liner does (asks before each step)
```
 ███╗   NEKOLIB installer
 1. Check git/curl + install Rust if missing      ~1-2 min
 2. Download NekoLib to ~/nekolib                ~10s
 3. Build optimized for YOUR CPU (RUSTFLAGS=-C target-cpu=native) ~1-3 min
 4. Create wallet + ask "Start mining?"           immediate

 Estimated total: 2–5 min (3–8 min if Rust not installed)
 Disk: 15 MB binary + 1 MB chain, RAM ~30 MB
 All systems: x86_64 Linux/Windows/macOS, ARM64 Pi, Termux, WSL
```
- **Beautiful colored prompts** + **progress spinner**
- **Asks permission** at every install: `Install Rust? [Y/n]` `Start mining? [Y/n]`
- **Idempotent** — re-run anytime to update, never overwrites wallet

### After install — 3 commands you need
```bash
cd ~/nekolib
./target/release/nekod wallet   # creates colorful wallet, shows address/secret
./target/release/nekod start --mine  # mines 50 neko/block, see ✓ FOUND logs
./target/release/nekod info     # height, supply 2550/100000, utxos (run after stopping miner)
```
Stop mining: `Enter` or `Ctrl+C`. Check balance: `info` shows `●` green for *your* coins.

> **Plug & play:** No cloud, no AWS, no Docker needed. Your laptop *is* the network.

---

## 🎨 What it looks like

```
 ███╗   NEKOLIB WALLET
┌──────────────────────────────────────────────┐
│ address bcf86df0dc19f189368c7454f6c55c4132be752e...
│ secret  2ca225... (KEEP SAFE)
└──────────────────────────────────────────────┘
✓ saved to ./nekodata/wallet.json
● balance 0 neko

⛓ CHAIN INFO  height 40  Supply 2050/100000 (2.1%)  ◆ 2 bumps
✓ FOUND h=41 nonce 1761569 hash 000000df... in 0.40s
  ⛓ tip h=41 utxos 42 minted 2100/100000
```

---

## 🧠 FOR NERDS — Full Guide (all systems, builds, common bugs)

<details>
<summary><b>Click to expand — Toolchain, P2P, storage, mobile, troubleshooting</b></summary>

### 1. Consensus — Permanent 100k Cap

| Param | Value | Where |
|-------|-------|-------|
| `MAX_SUPPLY` | `100,000` neko ever | `src/chain.rs:15` |
| `TOKENS_PER_BUMP` | `1,000` → `difficulty+1` | `src/chain.rs:16` |
| `block_reward` | `50` per block | `src/config.rs:43` / `nekolib.json` |
| Genesis | `50` | `src/block.rs:142` |
| Floor | `token_floor + 3` | `src/chain.rs:235` |

- **2000 blocks** total (`100000/50`). After `100k` mined → `cap exceeded` reject `src/chain.rs:109`.
- **Harder every 1k:** floor rises `0→1→2` at `1000,2000,3000` minted. At `h40 2050` → `2 bumps`, diff `6`. Never drops below floor.
- **No changes can be made** without hard fork — Vaibhav holds ultimate power on constants.

### 2. Toolchain & Compilation (bare-metal, zero-cost)

**Rust stable required**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
```

**Production flags — host SIMD (AVX2/AVX-512/Neon auto via blake3 crate `src/crypto.rs:15`)**
```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release        # desktop (>13M H/s)
RUSTFLAGS="-C target-cpu=native" cargo build --profile mobile # <10 MB
```

`Cargo.toml:37`:
```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

**Targets**
```bash
rustup target add x86_64-unknown-linux-gnu x86_64-pc-windows-msvc aarch64-unknown-linux-gnu aarch64-linux-android aarch64-apple-ios
cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
```

### 3. P2P — Zero-Cost `std::net` Only

- Stack `TcpListener`/`TcpStream` `src/p2p.rs:39` + JSON lines, no libp2p/cloud.
- **Bootstrap/DuckDNS:** edit `nekolib.json:13`:
```json
"bootstrap_peers": ["neko-seed.duckdns.org:9333","100.64.0.5:9333"]
```
- **Dynamic DNS:** `curl "https://www.duckdns.org/update?domains=YOUR&token=TOKEN&ip="`
- **CGNAT fallback → Tailscale (free):** `curl -fsSL https://tailscale.com/install.sh | sh && tailscale up` → `100.x` stable, no port forward. `nekod nat-guide` prints steps.
- **Local mesh:** `bootstrap_peers=["192.168.1.51:9333"]` on same WiFi.

### 4. Storage — Sled + Pruning

- Engine `sled` `src/storage.rs:22` pure Rust, `flush_every_ms=500` (SSD/battery friendly). Trees `blocks`/`utxo`/`meta`.
- Pruning `src/chain.rs:258` keeps last `100` full blocks, older truncated to header+coinbase, headers+UTXO retained → phone `<100 MB` vs BTC 500 GB.
- Check: `nekod info` → `disk 1048576 bytes`.

### 5. Crypto — BLAKE3 CPU Tuning

- `blake3` SIMD auto-detects `AVX2/NEON`, `mine_block_cpu` `src/crypto.rs:40` + `std::thread` + `AtomicBool` (no Tokio overhead). `miner.rs:13` stride partition.
- Why secure vs BTC: not SHA256 (no ASIC), `Ed25519` `src/wallet.rs:10` (non-malleable), mandatory sigs, double-BLAKE3 txid.

### 6. Mobile

**Android Termux (easiest, no NDK):**
```bash
pkg update && pkg install rust git termux-tools
git clone https://github.com/thisisforlearn/nekolib && cd nekolib
RUSTFLAGS="-C target-cpu=native" cargo build --release
./target/release/nekod start --mine
```

**Android NDK cross:**
```bash
cargo install cargo-ndk
rustup target add aarch64-linux-android
export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/26.1.10909125
cargo ndk -t arm64-v8a build --release
adb push target/aarch64-linux-android/release/nekod /data/local/tmp/
adb shell /data/local/tmp/nekod start --mine
```

**iOS (Xcode + staticlib):**
```bash
rustup target add aarch64-apple-ios
cargo build --release --target aarch64-apple-ios
# add crate-type=["staticlib"] + link in Xcode
```

### 7. CLI Reference

```bash
nekod init --listen 0.0.0.0:9333 --bootstrap seed.duckdns.org:9333
nekod start --mine --miner-threads 4
nekod wallet
nekod send <TO_HEX> <AMOUNT>
nekod info
nekod bench
nekod nat-guide
```

Mining tip: `--miner-threads` defaults to cores. Battery pause `80ms` `src/main.rs:160`.

### 8. Fix Common Bugs

| Error | Fix |
|-------|-----|
| `warning: unused ...` | `cargo fix` or ignore — non-blocking |
| `could not acquire lock on ./nekodata/db WouldBlock` | Stop miner first (`Enter`/`pkill nekod`) then `info`. Sled lock `src/storage.rs:22` is exclusive |
| `timestamp must increase` | Fixed `src/chain.rs:284` `max(now,tip+1)` |
| `coinbase utxo already exists` | Fixed `src/main.rs:131` `nonce=height` unique |
| `cap exceeded ... > MAX 100000` | Cap reached — no more coinbase possible, chain complete |
| `bind failed 0.0.0.0:9333` | Port in use: `lsof -i:9333` or change `nekolib.json listen_addr` |
| Build `linker cc not found` | `sudo apt install build-essential` / `xcode-select --install` |
| `cargo: command not found` | `source $HOME/.cargo/env` + restart terminal |
| Slow mining `diff 7 30s/block` | Normal after `7000` minted — floor rise. Lower `genesis_difficulty` in `nekolib.json` for private net |
| Termux `pkg install rust` fails | `pkg update && pkg upgrade && pkg install rust` |
| Windows `curl` not found | Use `install.ps1` via PowerShell |

Verbose log: `RUST_LOG=debug`.

### 9. Project Layout

```
src/lib.rs crypto.rs block.rs chain.rs storage.rs wallet.rs miner.rs p2p.rs config.rs main.rs
nekolib.json.example .cargo/config.toml install.sh install.ps1
```

### 10. License & Governance

**GPLv3** `LICENSE:679` — Copyright (C) 2026 Vaibhav.  
**Vaibhav holds ultimate power** — final authority on merges, releases, constants (`100k`, `1k bump`). Contributions welcome via PR but require approval. See `LICENSE` footer.

</details>

---

## 📊 Current chain (you are here)

- Height `h40→h50` mined to `dc0b...` / `bcf8...`, diff `6`, `1 MB` disk, `2.5%` minted. Keep mining → `100k` cap → difficulty `~103`.

## 🤝 Contribute

PRs welcome — Vaibhav merges. Run `cargo test` (`9 passed`) + `cargo build --release` before PR.

**Ask Vaibhav unlimited questions** — terminal, cross-compile, NAT, security.

