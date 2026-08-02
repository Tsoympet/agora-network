use std::collections::{HashMap, HashSet};

use agora_crypto::verify_transaction;
use agora_types::{Block, Hash, OutPoint, Transaction};

use crate::P2pError;

/// Default cap on how many transfer txs a mining template pulls from the pool.
pub const DEFAULT_TEMPLATE_TX_LIMIT: usize = 128;

/// Local mempool with signature-gated admission and outpoint reservation.
#[derive(Debug, Default)]
pub struct Mempool {
    txs: HashMap<Hash, Transaction>,
    /// Outpoints spent by txs currently in the pool (conflict detection).
    reserved: HashSet<OutPoint>,
    max_size: usize,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Self {
            txs: HashMap::new(),
            reserved: HashSet::new(),
            max_size,
        }
    }

    pub fn len(&self) -> usize {
        self.txs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }

    pub fn contains(&self, tx_id: &Hash) -> bool {
        self.txs.contains_key(tx_id)
    }

    /// Outpoints already claimed by mempool transactions.
    pub fn reserved(&self) -> &HashSet<OutPoint> {
        &self.reserved
    }

    /// Admit a transaction after secp256k1 verification and mempool conflict checks.
    ///
    /// Callers that have a live UTXO set should run
    /// [`agora_state_machine::validate_mempool_tx`] under the same mempool lock
    /// before this method so chain UTXO rules and reserved conflicts stay atomic.
    pub fn admit(&mut self, tx: Transaction) -> Result<Hash, P2pError> {
        if tx.inputs.is_empty() {
            return Err(P2pError::MempoolRejected(
                "coinbase not allowed in mempool".into(),
            ));
        }
        verify_transaction(&tx).map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        let id = tx.tx_id();
        if self.txs.contains_key(&id) {
            return Ok(id);
        }
        if self.txs.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }

        let mut claimed = HashSet::new();
        for input in &tx.inputs {
            let op = input.previous_outpoint;
            if !claimed.insert(op) || self.reserved.contains(&op) {
                return Err(P2pError::MempoolRejected(format!(
                    "double spend {}:{}",
                    op.tx_id.to_hex(),
                    op.index
                )));
            }
        }
        for op in &claimed {
            self.reserved.insert(*op);
        }
        self.txs.insert(id, tx);
        Ok(id)
    }

    pub fn get(&self, tx_id: &Hash) -> Option<&Transaction> {
        self.txs.get(tx_id)
    }

    /// Lookup by first 8 bytes of `tx_id` for compact-block inflation.
    pub fn get_by_short_id(&self, short_id: &[u8; 8]) -> Option<&Transaction> {
        self.txs
            .iter()
            .find(|(id, _)| &id.as_bytes()[..8] == short_id.as_slice())
            .map(|(_, tx)| tx)
    }

    pub fn remove(&mut self, tx_id: &Hash) -> Option<Transaction> {
        let tx = self.txs.remove(tx_id)?;
        for input in &tx.inputs {
            self.reserved.remove(&input.previous_outpoint);
        }
        Some(tx)
    }

    /// Deterministic transfer selection for mining templates (sorted by `tx_id`).
    pub fn select_transfers(&self, max: usize) -> Vec<Transaction> {
        let mut txs: Vec<Transaction> = self.txs.values().cloned().collect();
        txs.sort_by(|a, b| a.tx_id().as_bytes().cmp(b.tx_id().as_bytes()));
        if txs.len() > max {
            txs.truncate(max);
        }
        txs
    }

    /// Drop included txs and any remaining pool txs that spend the same outpoints.
    pub fn evict_for_block(&mut self, block: &Block) {
        let mut spent = HashSet::new();
        let mut included = HashSet::new();
        for tx in &block.transactions {
            included.insert(tx.tx_id());
            for input in &tx.inputs {
                spent.insert(input.previous_outpoint);
            }
        }
        let drop: Vec<Hash> = self
            .txs
            .iter()
            .filter(|(id, tx)| {
                included.contains(id)
                    || tx
                        .inputs
                        .iter()
                        .any(|i| spent.contains(&i.previous_outpoint))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in drop {
            let _ = self.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_types::{Amount, Hash, OutPoint, Transaction, TxIn, TxOut};

    use super::*;

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn signed_spend(index: u32, nonce: u64) -> Transaction {
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            nonce,
        );
        sign_transaction(&mut tx, &kp).unwrap();
        tx
    }

    #[test]
    fn admits_valid_signed_tx() {
        let tx = signed_spend(0, 1);
        let mut pool = Mempool::new(16);
        let id = pool.admit(tx.clone()).unwrap();
        assert_eq!(id, tx.tx_id());
        assert!(pool.contains(&id));
        assert!(pool.reserved().contains(&OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        }));
    }

    #[test]
    fn rejects_unsigned_tx() {
        let tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![],
            1,
        );
        let mut pool = Mempool::new(16);
        assert!(pool.admit(tx).is_err());
    }

    #[test]
    fn rejects_coinbase_shaped_tx() {
        let tx = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: agora_types::Address::ZERO,
            }],
            1,
        );
        let mut pool = Mempool::new(16);
        let err = pool.admit(tx).unwrap_err().to_string();
        assert!(err.contains("coinbase"), "{err}");
    }

    #[test]
    fn rejects_mempool_double_spend_and_frees_on_remove() {
        let tx_a = signed_spend(0, 1);
        let tx_b = signed_spend(0, 2);
        let mut pool = Mempool::new(16);
        let id_a = pool.admit(tx_a).unwrap();
        assert!(pool.admit(tx_b).is_err());
        pool.remove(&id_a).unwrap();
        assert!(pool.reserved().is_empty());
        let tx_c = signed_spend(0, 3);
        assert!(pool.admit(tx_c).is_ok());
    }

    #[test]
    fn select_transfers_is_sorted_and_capped() {
        let mut pool = Mempool::new(16);
        let a = signed_spend(0, 1);
        let b = signed_spend(1, 2);
        let c = signed_spend(2, 3);
        pool.admit(a.clone()).unwrap();
        pool.admit(b.clone()).unwrap();
        pool.admit(c.clone()).unwrap();
        let selected = pool.select_transfers(2);
        assert_eq!(selected.len(), 2);
        assert!(selected[0].tx_id().as_bytes() <= selected[1].tx_id().as_bytes());
    }

    #[test]
    fn evict_for_block_drops_included_and_conflicts() {
        use agora_types::{Block, BlockHeader};

        let mut pool = Mempool::new(16);
        let included = signed_spend(0, 1);
        let other = signed_spend(1, 2);
        pool.admit(included.clone()).unwrap();
        pool.admit(other.clone()).unwrap();
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![included.clone()],
        };
        pool.evict_for_block(&block);
        assert!(!pool.contains(&included.tx_id()));
        assert!(pool.contains(&other.tx_id()));
        assert!(!pool.reserved().contains(&OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        }));
    }
}
