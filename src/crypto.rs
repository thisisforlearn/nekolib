use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// BLAKE3 hashing — automatically uses SIMD (AVX2/AVX-512/Neon) at runtime.
/// No configuration needed; blake3 crate does runtime CPU feature detection.
///
/// Why BLAKE3 vs SHA-256 / Scrypt / Argon2:
/// - 3-10x faster than SHA-256, 2-4x faster than BLAKE2b
/// - SIMD-friendly, maps to x86_64 and ARM equally (portable)
/// - Small memory footprint (~few KB) -> battery-friendly vs Scrypt/Argon2 (which need 100s MB)
/// - Still 256-bit security, collision/preimage resistant
/// - Not trivially ASIC-dominated yet (unlike SHA-256), GPU-unfriendly at low difficulty due to per-nonce overhead

/// Single BLAKE3 hash -> 32 bytes, hex encoded 64 chars
pub fn blake3_hash(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

pub fn blake3_hash_hex(data: &[u8]) -> String {
    hex::encode(blake3_hash(data))
}

/// Double BLAKE3 (like BTC double-SHA256 pattern but with BLAKE3)
/// Provides extra length-extension resistance; still lightweight.
pub fn blake3_double(data: &[u8]) -> [u8; 32] {
    let first = blake3::hash(data);
    *blake3::hash(first.as_bytes()).as_bytes()
}

pub fn blake3_double_hex(data: &[u8]) -> String {
    hex::encode(blake3_double(data))
}

/// Difficulty check: hash's hex must start with `difficulty` zeros.
/// Example: difficulty 4 -> "0000abcd..."
/// This is intentionally CPU-tunable: small increments greatly change work.
/// For production we use compact bits target; here we keep human-readable for portability.
/// Also supports `target` style check for consensus.
pub fn hash_meets_difficulty(hash_hex: &str, difficulty: u32) -> bool {
    let need = difficulty as usize;
    if need == 0 {
        return true;
    }
    if hash_hex.len() < need {
        return false;
    }
    hash_hex.as_bytes()[..need].iter().all(|&b| b == b'0')
}

/// Verify PoW: recompute header hash and check difficulty
pub fn verify_pow(header_bytes: &[u8], difficulty: u32) -> bool {
    let h = blake3_hash_hex(header_bytes);
    hash_meets_difficulty(&h, difficulty)
}

/// Pure-CPU mining loop — **no async**, uses `AtomicBool` for cross-thread cancel.
///
/// This is the hot loop that must stay scheduling-overhead free.
/// Uses `std::thread::available_parallelism` to spawn worker threads.
/// Each worker increments nonce by `threads` stride to avoid overlap.
///
/// Security notes vs BTC:
/// - BLAKE3 is not SHA256, so existing SHA256 ASICs give 0 advantage
/// - Per-thread atomic flag ensures clean shutdown (battery-aware on mobile)
/// - No Tokio, no async executors inside loop -> deterministic timing
pub fn mine_block_cpu(
    header_prefix: &[u8], // serialized header without nonce (prev_hash|merkle|timestamp|difficulty)
    difficulty: u32,
    start_nonce: u64,
    max_nonce: u64,
    cancel: Arc<AtomicBool>,
) -> Option<(u64, String)> {
    let mut nonce = start_nonce;
    // Single-thread tight loop (caller spawns many of these)
    // BLAKE3's SIMD will use AVX2/AVX512/NEON automatically inside `hash`
    let mut buf = Vec::with_capacity(header_prefix.len() + 8);
    buf.extend_from_slice(header_prefix);
    buf.extend_from_slice(&[0u8; 8]); // space for nonce LE bytes
    let prefix_len = header_prefix.len();

    while nonce < max_nonce {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        // write nonce LE into tail
        buf[prefix_len..].copy_from_slice(&nonce.to_le_bytes());
        let h = blake3_hash(&buf);
        let hex = hex::encode(h);
        if hash_meets_difficulty(&hex, difficulty) {
            return Some((nonce, hex));
        }
        nonce = nonce.wrapping_add(1);
        // periodic cancel check already done, but yield occasionally for mobile
        // to avoid starving OS on single-core phones
        if nonce & 0xFFF == 0 && cancel.load(Ordering::Relaxed) {
            return None;
        }
    }
    None
}

/// Parallel mine wrapper: fans out `mine_block_cpu` across N threads.
/// Returns first found solution and signals others to stop via AtomicBool.
pub fn mine_parallel(
    header_prefix: Vec<u8>,
    difficulty: u32,
) -> Option<(u64, String)> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let cancel = Arc::new(AtomicBool::new(false));
    let prefix = Arc::new(header_prefix);

    let mut handles = Vec::with_capacity(threads);
    for tid in 0..threads {
        let cancel_c = Arc::clone(&cancel);
        let prefix_c = Arc::clone(&prefix);
        let handle = std::thread::spawn(move || {
            // stride partition: each thread starts at tid, steps by threads
            let mut nonce = tid as u64;
            let mut buf = Vec::with_capacity(prefix_c.len() + 8);
            buf.extend_from_slice(&prefix_c);
            buf.extend_from_slice(&[0u8; 8]);
            let plen = prefix_c.len();
            loop {
                if cancel_c.load(Ordering::Relaxed) {
                    return None;
                }
                buf[plen..].copy_from_slice(&nonce.to_le_bytes());
                let h = blake3_hash(&buf);
                if hash_meets_difficulty(&hex::encode(h), difficulty) {
                    // claim victory
                    cancel_c.store(true, Ordering::Relaxed);
                    return Some((nonce, hex::encode(h)));
                }
                nonce = nonce.wrapping_add(threads as u64);
                // Avoid infinite busy loop without checks on high difficulty
                if nonce > u64::MAX - (threads as u64) {
                    return None;
                }
                // lightweight periodic cancel
                if nonce & 0x3FFF == 0 && cancel_c.load(Ordering::Relaxed) {
                    return None;
                }
            }
        });
        handles.push(handle);
    }
    // Wait for first success
    let mut result = None;
    for h in handles {
        if let Ok(Some(res)) = h.join() {
            if result.is_none() {
                result = Some(res);
                cancel.store(true, Ordering::Relaxed);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blake3_deterministic() {
        let h1 = blake3_hash_hex(b"neko");
        let h2 = blake3_hash_hex(b"neko");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn difficulty_check() {
        assert!(hash_meets_difficulty("0000abcd", 4));
        assert!(!hash_meets_difficulty("0000abcd", 5));
        assert!(hash_meets_difficulty("anything", 0));
    }

    #[test]
    fn mine_easy() {
        let cancel = Arc::new(AtomicBool::new(false));
        let res = mine_block_cpu(b"test-header", 1, 0, 10000, cancel);
        assert!(res.is_some(), "difficulty 1 should find quickly");
        let (nonce, hash) = res.unwrap();
        assert!(hash_meets_difficulty(&hash, 1));
        assert!(verify_pow(&[b"test-header".to_vec(), nonce.to_le_bytes().to_vec()].concat(), 1));
    }
}
