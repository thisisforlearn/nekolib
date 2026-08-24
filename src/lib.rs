/*!
 * nekolib — Pure-CPU Portable Layer-1 Blockchain
 * Author: Vaibhav
 *
 * Design goals:
 * - Zero cloud deps, runs on home internet + mobile
 * - BLAKE3 + SIMD auto-detection (AVX2/AVX-512/NEON)
 * - std::thread + AtomicBool, no async in mining loops
 * - std::net TCP P2P, bootstrap + DDNS + Tailscale overlay fallback
 * - Sled embedded KV (pure Rust, no C bindings)
 * - UTXO + header-only pruning for <100MB mobile full-node
 */

pub mod block;
pub mod chain;
pub mod config;
pub mod crypto;
pub mod miner;
pub mod p2p;
pub mod storage;
pub mod wallet;

pub use block::{Block, BlockHeader, Transaction};
pub use chain::Blockchain;
pub use config::Config;
pub use crypto::{blake3_hash, blake3_double, hash_meets_difficulty, mine_block_cpu, verify_pow};
pub use miner::{MineResult, Miner};
pub use storage::Ledger;
pub use wallet::{KeyPair, Wallet};

/// nekolib version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: u32 = 1;
