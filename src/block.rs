use serde::{Deserialize, Serialize};
use crate::crypto::{blake3_hash_hex, blake3_double_hex, hash_meets_difficulty};

/// Transaction — UTXO model, lightweight.
/// For mobile pruning, we only keep UTXO set, not full history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    pub from: String,      // hex pubkey or "GENESIS"/"COINBASE"
    pub to: String,        // hex pubkey
    pub amount: u64,       // in smallest unit (neko = 1e8 meow)
    pub nonce: u64,        // replay protection, strictly increasing per `from`
    pub fee: u64,
    pub payload: Option<String>, // optional memo (max 256 bytes enforced)
    pub signature: Option<String>, // hex ed25519 signature (64 bytes)
    /// cached hash, not serialized as ID
    #[serde(skip, default)]
    pub hash_cache: Option<String>,
}

impl Transaction {
    pub fn new(from: String, to: String, amount: u64, nonce: u64) -> Self {
        Self {
            from,
            to,
            amount,
            nonce,
            fee: 1,
            payload: None,
            signature: None,
            hash_cache: None,
        }
    }

    pub fn coinbase(to: String, amount: u64) -> Self {
        Self {
            from: "COINBASE".to_string(),
            to,
            amount,
            nonce: 0,
            fee: 0,
            payload: Some("coinbase".to_string()),
            signature: None,
            hash_cache: None,
        }
    }

    /// Bytes that are signed / hashed (excludes signature)
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(self.from.as_bytes());
        v.extend_from_slice(b"|");
        v.extend_from_slice(self.to.as_bytes());
        v.extend_from_slice(b"|");
        v.extend_from_slice(&self.amount.to_le_bytes());
        v.extend_from_slice(&self.nonce.to_le_bytes());
        v.extend_from_slice(&self.fee.to_le_bytes());
        if let Some(p) = &self.payload {
            v.extend_from_slice(p.as_bytes());
        }
        v
    }

    pub fn hash(&self) -> String {
        // double BLAKE3 for txid (more secure than single, still fast)
        blake3_double_hex(&self.signing_bytes())
    }

    pub fn id(&self) -> String {
        self.hash()
    }

    pub fn is_coinbase(&self) -> bool {
        self.from == "COINBASE"
    }

    pub fn verify_size(&self) -> bool {
        if let Some(p) = &self.payload {
            if p.len() > 256 {
                return false;
            }
        }
        true
    }
}

/// BlockHeader — 80 bytes logical (like BTC but BLAKE3)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockHeader {
    pub version: u32,
    pub prev_hash: String,   // hex 64 chars, "0".repeat(64) for genesis
    pub merkle_root: String, // hex
    pub timestamp: u64,      // unix seconds
    pub difficulty: u32,     // leading zeros required (1-64)
    pub nonce: u64,
    pub height: u64,
}

impl BlockHeader {
    pub fn serialize_for_pow(&self) -> Vec<u8> {
        // Deterministic serialization for PoW
        let mut v = Vec::with_capacity(4 + 64 + 64 + 8 + 4 + 8);
        v.extend_from_slice(&self.version.to_le_bytes());
        v.extend_from_slice(self.prev_hash.as_bytes());
        v.extend_from_slice(self.merkle_root.as_bytes());
        v.extend_from_slice(&self.timestamp.to_le_bytes());
        v.extend_from_slice(&self.difficulty.to_le_bytes());
        v.extend_from_slice(&self.height.to_le_bytes());
        // nonce is appended inside miner loop separately; but we also serialize it for verification
        v
    }

    pub fn pow_bytes_with_nonce(&self, nonce: u64) -> Vec<u8> {
        let mut b = self.serialize_for_pow();
        b.extend_from_slice(&nonce.to_le_bytes());
        b
    }

    pub fn hash_with_nonce(&self, nonce: u64) -> String {
        blake3_hash_hex(&self.pow_bytes_with_nonce(nonce))
    }

    pub fn hash(&self) -> String {
        self.hash_with_nonce(self.nonce)
    }

    pub fn meets_difficulty(&self) -> bool {
        let h = self.hash();
        hash_meets_difficulty(&h, self.difficulty)
    }
}

/// Block — header + transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub hash: String, // cached header hash
}

impl Block {
    pub fn genesis(difficulty: u32) -> Self {
        let tx = Transaction::coinbase("genesis".to_string(), 50);
        let merkle = merkle_root(&[tx.clone()]);
        let mut header = BlockHeader {
            version: 1,
            prev_hash: "0".repeat(64),
            merkle_root: merkle,
            timestamp: 1_700_000_000,
            difficulty,
            nonce: 0,
            height: 0,
        };
        // mine genesis quickly if difficulty low
        let prefix = header.serialize_for_pow();
        if let Some((nonce, hash)) = crate::crypto::mine_parallel(prefix, difficulty) {
            header.nonce = nonce;
            return Self {
                hash,
                header,
                transactions: vec![tx],
            };
        }
        // fallback
        let hash = header.hash();
        Self {
            header,
            transactions: vec![tx],
            hash,
        }
    }

    pub fn new(prev_hash: String, height: u64, transactions: Vec<Transaction>, difficulty: u32, timestamp: u64) -> Self {
        let merkle = merkle_root(&transactions);
        let header = BlockHeader {
            version: 1,
            prev_hash,
            merkle_root: merkle,
            timestamp,
            difficulty,
            nonce: 0,
            height,
        };
        let hash = header.hash();
        Self { header, transactions, hash }
    }

    /// Verify block integrity (without chain context)
    pub fn verify(&self) -> Result<(), String> {
        // size limits for mobile
        if self.transactions.len() > 4096 {
            return Err("too many txs (max 4096)".to_string());
        }
        if self.transactions.is_empty() {
            return Err("empty block".to_string());
        }
        // merkle
        let computed = merkle_root(&self.transactions);
        if computed != self.header.merkle_root {
            return Err(format!("merkle mismatch: {} vs {}", computed, self.header.merkle_root));
        }
        // pow
        if !self.header.meets_difficulty() {
            return Err(format!("pow failed: hash {} diff {}", self.header.hash(), self.header.difficulty));
        }
        // hash cache
        if self.hash != self.header.hash() {
            return Err("hash cache mismatch".to_string());
        }
        // tx sizes
        for tx in &self.transactions {
            if !tx.verify_size() {
                return Err("tx payload too large".to_string());
            }
        }
        Ok(())
    }

    pub fn size_bytes(&self) -> usize {
        serde_json::to_vec(self).map(|v| v.len()).unwrap_or(0)
    }
}

/// Merkle root using BLAKE3 double hashing (binary tree, duplicate last if odd)
pub fn merkle_root(txs: &[Transaction]) -> String {
    if txs.is_empty() {
        return blake3_hash_hex(b"");
    }
    let mut layer: Vec<String> = txs.iter().map(|t| t.hash()).collect();
    while layer.len() > 1 {
        let mut next = Vec::with_capacity((layer.len() + 1) / 2);
        for chunk in layer.chunks(2) {
            let left = &chunk[0];
            let right = if chunk.len() == 2 { &chunk[1] } else { left };
            let combined = format!("{}{}", left, right);
            next.push(blake3_double_hex(combined.as_bytes()));
        }
        layer = next;
    }
    layer[0].clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merkle_deterministic() {
        let tx1 = Transaction::new("a".into(), "b".into(), 10, 0);
        let tx2 = Transaction::new("b".into(), "c".into(), 5, 1);
        let r1 = merkle_root(&[tx1.clone(), tx2.clone()]);
        let r2 = merkle_root(&[tx1, tx2]);
        assert_eq!(r1, r2);
        assert_eq!(r1.len(), 64);
    }

    #[test]
    fn block_verify_easy() {
        let b = Block::genesis(1);
        assert!(b.verify().is_ok(), "{}", b.verify().err().unwrap_or_default());
    }
}
