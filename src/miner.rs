use crate::block::Block;
use crate::crypto::{blake3_hash, hash_meets_difficulty};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Mine result
#[derive(Debug, Clone)]
pub struct MineResult {
    pub nonce: u64,
    pub hash: String,
    pub attempts: u64,
    pub elapsed_ms: u128,
}

/// Miner — pure std::thread + AtomicBool, no Tokio, no async.
/// Designed for embedded/mobile: respects cancel flag for battery.
pub struct Miner {
    pub threads: usize,
    pub cancel: Arc<AtomicBool>,
}

impl Miner {
    pub fn new(threads: Option<usize>) -> Self {
        let t = threads.unwrap_or_else(|| {
            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
        });
        Self {
            threads: t,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.cancel.store(false, Ordering::Relaxed);
    }

    /// Mine a block template: fills header.nonce + hash
    /// Returns mined block or None if cancelled.
    pub fn mine(&self, mut block: Block) -> Option<Block> {
        let start = std::time::Instant::now();
        let difficulty = block.header.difficulty;
        let prefix = block.header.serialize_for_pow();
        let cancel = Arc::clone(&self.cancel);
        let threads = self.threads;

        // Each thread gets stride partition
        let prefix_arc = Arc::new(prefix);
        let found: Arc<std::sync::Mutex<Option<(u64, String)>>> = Arc::new(std::sync::Mutex::new(None));

        let mut handles = Vec::new();
        for tid in 0..threads {
            let cancel_c = Arc::clone(&cancel);
            let prefix_c = Arc::clone(&prefix_arc);
            let found_c = Arc::clone(&found);
            let handle = std::thread::spawn(move || {
                let mut nonce = tid as u64;
                let mut buf = Vec::with_capacity(prefix_c.len() + 8);
                buf.extend_from_slice(&prefix_c);
                buf.extend_from_slice(&[0u8; 8]);
                let plen = prefix_c.len();
                let mut local_attempts: u64 = 0;
                loop {
                    if cancel_c.load(Ordering::Relaxed) {
                        break;
                    }
                    // early exit if another thread found solution
                    if found_c.lock().unwrap().is_some() {
                        break;
                    }
                    buf[plen..].copy_from_slice(&nonce.to_le_bytes());
                    let h = blake3_hash(&buf);
                    let hex = hex::encode(h);
                    local_attempts = local_attempts.wrapping_add(1);
                    if hash_meets_difficulty(&hex, difficulty) {
                        let mut g = found_c.lock().unwrap();
                        if g.is_none() {
                            *g = Some((nonce, hex.clone()));
                            cancel_c.store(true, Ordering::Relaxed);
                        }
                        break;
                    }
                    nonce = nonce.wrapping_add(threads as u64);
                    // mobile yield: check cancel every 16384
                    if local_attempts & 0x3FFF == 0 && cancel_c.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.join();
        }

        let guard = found.lock().unwrap();
        if let Some((nonce, hash)) = guard.clone() {
            block.header.nonce = nonce;
            block.hash = hash;
            let elapsed = start.elapsed().as_millis();
            // stats could be logged
            let _ = elapsed;
            // reset cancel for next block
            self.cancel.store(false, Ordering::Relaxed);
            Some(block)
        } else {
            self.cancel.store(false, Ordering::Relaxed);
            None
        }
    }

    /// Mine with explicit cancel flag (for p2p interrupt: if new block arrives, stop)
    pub fn mine_with_cancel(&self, block: Block, external_cancel: Arc<AtomicBool>) -> Option<Block> {
        // Combine both cancels
        let original = Arc::clone(&self.cancel);
        let combined = Arc::new(AtomicBool::new(false));
        let combined_c = Arc::clone(&combined);
        let external_c = Arc::clone(&external_cancel);
        // watcher thread
        let watcher = std::thread::spawn(move || {
            while !combined_c.load(Ordering::Relaxed) {
                if original.load(Ordering::Relaxed) || external_c.load(Ordering::Relaxed) {
                    combined_c.store(true, Ordering::Relaxed);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
        let miner = Miner { threads: self.threads, cancel: combined };
        let res = miner.mine(block);
        // stop watcher
        miner.cancel.store(true, Ordering::Relaxed);
        let _ = watcher.join();
        self.cancel.store(false, Ordering::Relaxed);
        external_cancel.store(false, Ordering::Relaxed);
        res
    }
}

/// Simple benchmark helper
pub fn benchmark_hashes(per_thread: u64) -> u64 {
    let start = std::time::Instant::now();
    let data = b"benchmark nekolib blake3 simd";
    let mut count = 0;
    for _ in 0..per_thread {
        let _ = blake3_hash(data);
        count += 1;
    }
    let elapsed = start.elapsed().as_secs_f64();
    (count as f64 / elapsed) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{Block, Transaction};

    #[test]
    fn mine_block_easy() {
        let tx = Transaction::coinbase("miner".into(), 50);
        let block = Block::new("0".repeat(64), 1, vec![tx], 2, 1_700_000_001);
        let miner = Miner::new(Some(2));
        let mined = miner.mine(block).expect("should mine diff 2");
        assert!(mined.verify().is_ok());
        assert!(mined.header.meets_difficulty());
    }
}
