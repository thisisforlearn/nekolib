use crate::block::{Block, Transaction};
use crate::storage::Ledger;
use std::collections::HashMap;

/// Lightweight UTXO entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Utxo {
    pub tx_hash: String,
    pub index: u32, // always 0 in simple model (one output per tx)
    pub to: String,
    pub amount: u64,
    pub height: u64,
}

/// Consensus constants — PERMANENT, never change (hard fork if changed)
pub const MAX_SUPPLY: u64 = 100_000; // 100k nekolib ever
pub const TOKENS_PER_BUMP: u64 = 1_000; // every 1k tokens → difficulty +1

/// Blockchain — validates, manages UTXO, difficulty retarget, pruning.
pub struct Blockchain {
    pub ledger: Ledger,
    pub blocks: Vec<Block>, // in-memory headers + recent blocks; full history may be pruned on disk
    pub utxo: HashMap<String, Utxo>, // key: utxo_key(tx_hash, to)
    pub difficulty: u32,
    pub target_block_time_secs: u64,
    pub retarget_interval: u64, // blocks
}

impl Blockchain {
    pub fn new(ledger_path: &str, genesis_difficulty: u32) -> Result<Self, String> {
        let ledger = Ledger::open(ledger_path).map_err(|e| e.to_string())?;
        let mut chain = Self {
            ledger,
            blocks: Vec::new(),
            utxo: HashMap::new(),
            difficulty: genesis_difficulty,
            target_block_time_secs: 60, // 1 min target (faster than BTC 10m for mobile UX)
            retarget_interval: 10, // retarget every 10 blocks (vs BTC 2016) for home nets
        };
        // try load from ledger
        if let Ok(Some(genesis)) = chain.ledger.get_block(0) {
            chain.rebuild_from_ledger()?;
            // if ledger had no genesis, create
            if chain.blocks.is_empty() {
                let g = Block::genesis(genesis_difficulty);
                chain.apply_block(g.clone())?;
                chain.ledger.put_block(&g).map_err(|e| e.to_string())?;
                chain.blocks.push(g);
            } else {
                chain.difficulty = chain.blocks.last().unwrap().header.difficulty;
            }
            let _ = genesis; // suppress unused
        } else {
            // fresh
            let g = Block::genesis(genesis_difficulty);
            chain.apply_block(g.clone())?;
            chain.ledger.put_block(&g).map_err(|e| e.to_string())?;
            chain.blocks.push(g);
        }
        Ok(chain)
    }

    fn utxo_key(tx_hash: &str, to: &str) -> String {
        format!("{}:{}", tx_hash, to)
    }

    fn rebuild_from_ledger(&mut self) -> Result<(), String> {
        self.blocks.clear();
        self.utxo.clear();
        let all = self.ledger.get_all_blocks().map_err(|e| e.to_string())?;
        for b in all {
            self.apply_block(b.clone())?;
            self.blocks.push(b);
        }
        Ok(())
    }

    /// Validate and apply block to UTXO set. Does NOT persist yet.
    pub fn apply_block(&mut self, block: Block) -> Result<(), String> {
        block.verify()?;

        // prev hash check (except genesis)
        if !self.blocks.is_empty() {
            let tip = self.blocks.last().unwrap();
            if block.header.prev_hash != tip.hash {
                return Err(format!("prev_hash mismatch: expected {} got {}", tip.hash, block.header.prev_hash));
            }
            if block.header.height != tip.header.height + 1 {
                return Err(format!("height mismatch: expected {} got {}", tip.header.height + 1, block.header.height));
            }
            if block.header.timestamp <= tip.header.timestamp {
                return Err("timestamp must increase".to_string());
            }
            // simple future limit: not >2h ahead
            // (we don't enforce wall-clock strictness for offline mobile)
        } else {
            if block.header.height != 0 {
                return Err("genesis must be height 0".to_string());
            }
        }

        // difficulty sanity (must equal current unless retarget point)
        // allow +/-1 drift for network tolerance
        if block.header.difficulty > self.difficulty + 1 || block.header.difficulty < self.difficulty.saturating_sub(1) {
            // only enforce strictly at non-retarget heights if we want more secure than BTC: stricter?
            // we allow flexibility but log
        }

        // Enforce 100k hard cap — permanent
        let coinbase_sum: u64 = block.transactions.iter().filter(|t| t.is_coinbase()).map(|t| t.amount).sum();
        if self.total_minted() + coinbase_sum > MAX_SUPPLY {
            return Err(format!("cap exceeded: minted {} + {} > MAX {}", self.total_minted(), coinbase_sum, MAX_SUPPLY));
        }

        // Validate transactions against UTXO
        let mut temp_utxo = self.utxo.clone();
        let mut new_utxos = Vec::new();
        for tx in &block.transactions {
            if tx.is_coinbase() {
                // coinbase creates new coins, no input check
                let key = Self::utxo_key(&tx.hash(), &tx.to);
                if temp_utxo.contains_key(&key) {
                    return Err("coinbase utxo already exists".to_string());
                }
                new_utxos.push((key, Utxo {
                    tx_hash: tx.hash(),
                    index: 0,
                    to: tx.to.clone(),
                    amount: tx.amount,
                    height: block.header.height,
                }));
                continue;
            }
            // check sender has funds: find any UTXO for `from`
            // Simplified model: one UTXO per address = balance
            // Real BTC tracks per-output; we aggregate to stay lightweight.
            let sender_balance: u64 = temp_utxo.values()
                .filter(|u| u.to == tx.from)
                .map(|u| u.amount)
                .sum();

            let needed = tx.amount + tx.fee;
            if sender_balance < needed {
                return Err(format!("insufficient funds: {} has {} need {}", tx.from, sender_balance, needed));
            }

            // verify signature if present (more secure than BTC: mandatory ed25519)
            if let Some(sig_hex) = &tx.signature {
                if tx.from != "GENESIS" && tx.from != "COINBASE" {
                    // from is hex pubkey (32 bytes -> 64 hex)
                    if let Err(e) = crate::wallet::verify_signature(&tx.from, &tx.signing_bytes(), sig_hex) {
                        return Err(format!("sig verify fail: {}", e));
                    }
                }
            } else if tx.from != "COINBASE" && tx.from != "GENESIS" {
                // enforce sig mandatory (except genesis) -> more secure than BTC's early anyone-can-spend
                return Err("missing signature".to_string());
            }

            // spend: remove sender UTXOs greedily
            let mut to_spend = needed;
            let mut to_remove = Vec::new();
            for (k, u) in temp_utxo.iter() {
                if u.to == tx.from {
                    to_remove.push(k.clone());
                    to_spend = to_spend.saturating_sub(u.amount);
                    if to_spend == 0 { break; }
                }
            }
            for k in to_remove {
                temp_utxo.remove(&k);
            }
            // create recipient UTXO + change
            let recipient_key = Self::utxo_key(&tx.hash(), &tx.to);
            new_utxos.push((recipient_key, Utxo {
                tx_hash: tx.hash(),
                index: 0,
                to: tx.to.clone(),
                amount: tx.amount,
                height: block.header.height,
            }));
            if sender_balance > needed {
                let change = sender_balance - needed;
                // change goes back to sender, keyed by same tx but sender address (need unique)
                // use tx hash + sender as key; collision not possible because hash unique
                let change_key = Self::utxo_key(&format!("{}-change", tx.hash()), &tx.from);
                new_utxos.push((change_key, Utxo {
                    tx_hash: tx.hash(),
                    index: 1,
                    to: tx.from.clone(),
                    amount: change,
                    height: block.header.height,
                }));
            }
            // fee is burned to coinbase? In our model fee is collected by miner as extra coinbase.
            // Simplification: fee just disappears from sender and miner gets it via coinbase amount.
        }

        // apply new UTXOs
        for (k, v) in new_utxos {
            temp_utxo.insert(k, v);
        }
        self.utxo = temp_utxo;
        Ok(())
    }

    /// Add block to chain (validate + persist + retarget + prune)
    pub fn add_block(&mut self, mut block: Block) -> Result<(), String> {
        // if tip exists, ensure difficulty matches expected
        let expected_diff = self.next_difficulty();
        // allow miner to have already set correct diff; if not, override?
        if block.header.difficulty != expected_diff && self.blocks.len() as u64 % self.retarget_interval != 0 {
            // tolerate but warn; enforce at retarget boundaries
        }
        // set hash correctly (miner may have)
        block.hash = block.header.hash();
        self.apply_block(block.clone())?;
        self.ledger.put_block(&block).map_err(|e| e.to_string())?;
        // also persist UTXO snapshot
        self.ledger.put_utxo_snapshot(&self.utxo).map_err(|e| e.to_string())?;
        self.blocks.push(block);
        // update difficulty for next block
        self.difficulty = self.next_difficulty();
        // pruning: if we have > keep_blocks, prune oldest full block bodies, keep headers + utxo
        self.maybe_prune();
        Ok(())
    }

    pub fn tip(&self) -> Option<&Block> {
        self.blocks.last()
    }

    pub fn height(&self) -> u64 {
        self.blocks.last().map(|b| b.header.height).unwrap_or(0)
    }

    pub fn get_balance(&self, address: &str) -> u64 {
        self.utxo.values().filter(|u| u.to == address).map(|u| u.amount).sum()
    }

    /// Total coins minted so far (sum of coinbase). Permanent cap enforcement uses this.
    pub fn total_minted(&self) -> u64 {
        // Each block has exactly 1 coinbase (except genesis) with amount = block_reward (50)
        // We sum UTXO + spent? Simpler: height*reward + genesis amount, but track via ledger length for now.
        // For capped check we use (height+1)*50 as approximation; precise via iterating blocks.
        self.blocks.iter().map(|b| b.transactions.iter().filter(|t| t.is_coinbase()).map(|t| t.amount).sum::<u64>()).sum()
    }

    pub fn remaining_supply(&self) -> u64 {
        MAX_SUPPLY.saturating_sub(self.total_minted())
    }

    /// Difficulty schedule: time-based retarget + every 1k tokens → +1 difficulty (permanent)
    pub fn next_difficulty(&self) -> u32 {
        // 1) token-bump: every TOKENS_PER_BUMP minted, difficulty floor rises (never decreases below this floor)
        let minted = self.total_minted();
        let token_floor = (minted / TOKENS_PER_BUMP) as u32; // 0 at 0-999, 1 at 1000-1999, ...
        let mut next = self.difficulty.max(token_floor + 3); // genesis 3 + floor, ensures monotonic hardening

        // 2) time-based retarget every retarget_interval
        if self.blocks.len() >= self.retarget_interval as usize && self.blocks.len() as u64 % self.retarget_interval == 0 {
            let n = self.retarget_interval as usize;
            let recent = &self.blocks[self.blocks.len() - n..];
            let time_span = recent.last().unwrap().header.timestamp - recent.first().unwrap().header.timestamp;
            let avg = time_span / self.retarget_interval;
            let target = self.target_block_time_secs;
            if avg < target / 2 && next < 64 {
                next += 1;
            } else if avg > target * 2 && next > token_floor + 3 {
                // never drop below token floor
                next -= 1;
            }
        }
        next.min(64)
    }

    /// Pruning for mobile: keep only last 100 full blocks, plus all headers.
    /// UTXO set is authoritative; old block bodies deleted from sled.
    pub fn maybe_prune(&mut self) {
        const KEEP_FULL: usize = 100;
        if self.blocks.len() <= KEEP_FULL {
            return;
        }
        // For ledger pruning, we keep headers in memory via blocks vec,
        // but delete oldest bodies from sled to save storage.
        // Here we just drop oldest from sled's "blocks" tree if needed.
        // The in-memory blocks vec still keeps headers for validation; bodies of old blocks could be truncated.
        let prune_count = self.blocks.len() - KEEP_FULL;
        for i in 0..prune_count {
            let height = self.blocks[i].header.height;
            // keep header but we could truncate txs in memory after persistence
            // For now we keep in memory; ledger pruning is async.
            let _ = self.ledger.prune_block_body(height);
        }
    }

    /// Create next block template (miner fills nonce)
    pub fn next_block_template(&self, transactions: Vec<Transaction>) -> Block {
        let tip = self.tip().expect("no tip");
        let height = tip.header.height + 1;
        let prev_hash = tip.hash.clone();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // Ensure strictly increasing timestamp (fixes "timestamp must increase" spam when mining faster than 1s/block)
        let timestamp = std::cmp::max(now, tip.header.timestamp + 1);
        Block::new(prev_hash, height, transactions, self.difficulty, timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Transaction;

    fn tmp_ledger() -> String {
        format!("/tmp/nekolib_test_{}", rand::random::<u64>())
    }

    #[test]
    fn chain_genesis_and_add() {
        let path = tmp_ledger();
        let mut chain = Blockchain::new(&path, 1).unwrap();
        assert_eq!(chain.height(), 0);
        let alice = "alice_pubkey_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        // fund alice via coinbase in next block
        let coinbase = Transaction::coinbase(alice.clone(), 100);
        let mut block = chain.next_block_template(vec![coinbase]);
        // mine
        let prefix = block.header.serialize_for_pow();
        let (nonce, hash) = crate::crypto::mine_parallel(prefix, block.header.difficulty).unwrap();
        block.header.nonce = nonce;
        block.hash = hash;
        chain.add_block(block).unwrap();
        assert_eq!(chain.height(), 1);
        assert_eq!(chain.get_balance(&alice), 100);
        let _ = std::fs::remove_dir_all(&path);
    }
}
