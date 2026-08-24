use crate::block::Block;
use crate::chain::Utxo;
use sled::{Db, Tree};
use std::collections::HashMap;

/// Ledger — Sled embedded KV, pure Rust, no C bindings.
/// Trees:
/// - blocks: height (u64 BE) -> Block JSON
/// - headers: height -> header hash (for pruning)
/// - meta: tip_height, difficulty, etc
/// - utxo: key -> Utxo JSON
///
/// Crash-safe: Sled is log-structured, guarantees atomic flush.
/// Lightweight: <1MB crate, no RocksDB C++ dependency.
pub struct Ledger {
    db: Db,
    blocks: Tree,
    utxo_tree: Tree,
    meta: Tree,
}

impl Ledger {
    pub fn open(path: &str) -> sled::Result<Self> {
        let db = sled::Config::default()
            .path(path)
            .mode(sled::Mode::HighThroughput) // faster for batch writes
            .flush_every_ms(Some(500)) // balance durability vs SSD wear (mobile)
            .open()?;
        let blocks = db.open_tree("blocks")?;
        let utxo_tree = db.open_tree("utxo")?;
        let meta = db.open_tree("meta")?;
        Ok(Self { db, blocks, utxo_tree, meta })
    }

    pub fn put_block(&self, block: &Block) -> sled::Result<()> {
        let key = height_key(block.header.height);
        let val = serde_json::to_vec(block).unwrap();
        self.blocks.insert(key, val)?;
        self.meta.insert(b"tip", &height_key(block.header.height))?;
        self.meta.insert(b"tip_hash", block.hash.as_bytes())?;
        self.db.flush()?;
        Ok(())
    }

    pub fn get_block(&self, height: u64) -> sled::Result<Option<Block>> {
        let key = height_key(height);
        if let Some(v) = self.blocks.get(key)? {
            let b: Block = serde_json::from_slice(&v).unwrap();
            Ok(Some(b))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_blocks(&self) -> sled::Result<Vec<Block>> {
        let mut out = Vec::new();
        for item in self.blocks.iter() {
            let (_, v) = item?;
            let b: Block = serde_json::from_slice(&v).unwrap();
            out.push(b);
        }
        out.sort_by_key(|b| b.header.height);
        Ok(out)
    }

    /// Pruning: delete full body but keep header hash for verification.
    /// For mobile: we keep utxo, not history. Headers + UTXO = full validation.
    pub fn prune_block_body(&self, height: u64) -> sled::Result<()> {
        // Instead of deleting, we replace block with pruned version (header only, empty txs)
        // To save space, we just remove it if height < tip - KEEP
        // Clients can still verify chain via headers kept in memory.
        // Here we implement aggressive pruning: delete body, keep pruned marker
        if let Some(v) = self.blocks.get(height_key(height))? {
            let mut block: Block = serde_json::from_slice(&v).unwrap();
            // keep header, clear txs to save space
            if block.transactions.len() > 1 {
                let coinbase = block.transactions[0].clone();
                block.transactions = vec![coinbase];
                let new_val = serde_json::to_vec(&block).unwrap();
                self.blocks.insert(height_key(height), new_val)?;
            }
        }
        Ok(())
    }

    pub fn put_utxo_snapshot(&self, utxo: &HashMap<String, Utxo>) -> sled::Result<()> {
        // clear and rewrite (small set; mobile only keeps active)
        self.utxo_tree.clear()?;
        let mut batch = sled::Batch::default();
        for (k, v) in utxo {
            batch.insert(k.as_bytes(), serde_json::to_vec(v).unwrap());
        }
        self.utxo_tree.apply_batch(batch)?;
        Ok(())
    }

    pub fn get_utxo_snapshot(&self) -> sled::Result<HashMap<String, Utxo>> {
        let mut map = HashMap::new();
        for item in self.utxo_tree.iter() {
            let (k, v) = item?;
            let key = String::from_utf8(k.to_vec()).unwrap();
            let utxo: Utxo = serde_json::from_slice(&v).unwrap();
            map.insert(key, utxo);
        }
        Ok(map)
    }

    pub fn tip_height(&self) -> Option<u64> {
        self.meta.get(b"tip").ok().flatten().map(|v| {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&v);
            u64::from_be_bytes(arr)
        })
    }

    pub fn flush(&self) -> sled::Result<()> {
        self.db.flush()?;
        Ok(())
    }

    pub fn size_on_disk(&self) -> u64 {
        self.db.size_on_disk().unwrap_or(0)
    }
}

fn height_key(h: u64) -> [u8; 8] {
    h.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;

    #[test]
    fn ledger_roundtrip() {
        let path = format!("/tmp/nekolib_ledger_{}", rand::random::<u64>());
        let ledger = Ledger::open(&path).unwrap();
        let b = Block::genesis(1);
        ledger.put_block(&b).unwrap();
        let got = ledger.get_block(0).unwrap().unwrap();
        assert_eq!(got.hash, b.hash);
        let _ = std::fs::remove_dir_all(&path);
    }
}
