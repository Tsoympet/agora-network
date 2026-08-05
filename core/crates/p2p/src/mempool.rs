use std::collections::{HashMap, HashSet};

use agora_consensus::{AcceptanceResult, UtxoJournalOp};
use agora_crypto::verify_transaction;
use agora_types::{Hash, NetworkFingerprint, OutPoint, Transaction};

use crate::P2pError;

/// Local mempool with fingerprint-bound signature admission and
/// acceptance-driven eviction.
#[derive(Debug)]
pub struct Mempool {
    txs: HashMap<Hash, Transaction>,
    max_size: usize,
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Self {
            txs: HashMap::new(),
            max_size: max_size.max(1),
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

    /// Admit a transaction after secp256k1 verification against the network fingerprint.
    pub fn admit(
        &mut self,
        tx: Transaction,
        fingerprint: &NetworkFingerprint,
    ) -> Result<Hash, P2pError> {
        verify_transaction(&tx, fingerprint)
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        if self.txs.len() >= self.max_size && !self.txs.contains_key(&tx.tx_id()) {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let id = tx.tx_id();
        self.txs.insert(id, tx);
        Ok(id)
    }

    pub fn get(&self, tx_id: &Hash) -> Option<&Transaction> {
        self.txs.get(tx_id)
    }

    pub fn remove(&mut self, tx_id: &Hash) -> Option<Transaction> {
        self.txs.remove(tx_id)
    }

    /// Evict by transaction acceptance — not by block color.
    ///
    /// Removes:
    /// - every accepted transaction id
    /// - any mempool tx that spends an outpoint spent by the acceptance journal
    ///   (conflicts with an accepted tx)
    pub fn evict_by_acceptance(&mut self, result: &AcceptanceResult) {
        let mut drop_ids: HashSet<Hash> = HashSet::new();
        let mut spent: HashSet<OutPoint> = HashSet::new();

        for block in &result.blocks {
            for outcome in &block.outcomes {
                if outcome.accepted {
                    drop_ids.insert(outcome.tx_id);
                }
            }
        }
        for op in &result.journal {
            if let UtxoJournalOp::Spend { outpoint } = op {
                spent.insert(*outpoint);
            }
        }

        self.txs.retain(|tx_id, tx| {
            if drop_ids.contains(tx_id) {
                return false;
            }
            !tx.inputs
                .iter()
                .any(|input| spent.contains(&input.previous_outpoint))
        });
    }
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_types::{Amount, Hash, NetworkFingerprint, OutPoint, Transaction, TxIn, TxOut};

    use super::*;

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn test_fp() -> NetworkFingerprint {
        NetworkFingerprint {
            network_name: "agora-test".into(),
            network_id: 1,
            genesis_hash: Hash::ZERO,
            ghostdag_k: 18,
            max_supply: 1,
            premine: 0,
            initial_reward: 50,
            halving_interval: 210_000,
        }
    }

    #[test]
    fn admits_valid_signed_tx() {
        let fp = test_fp();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            1,
        );
        sign_transaction(&mut tx, &kp, &fp).unwrap();
        let mut pool = Mempool::new(16);
        let id = pool.admit(tx.clone(), &fp).unwrap();
        assert_eq!(id, tx.tx_id());
        assert!(pool.contains(&id));
    }

    #[test]
    fn rejects_unsigned_tx() {
        let tx = Transaction::unsigned(1, vec![], vec![], 1);
        let mut pool = Mempool::new(16);
        assert!(pool.admit(tx, &test_fp()).is_err());
    }

    #[test]
    fn rejects_tx_signed_for_other_network() {
        let fp = test_fp();
        let mut other = test_fp();
        other.network_id = 7;
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            1,
        );
        sign_transaction(&mut tx, &kp, &fp).unwrap();
        let mut pool = Mempool::new(16);
        assert!(pool.admit(tx, &other).is_err());
    }

    #[test]
    fn evicts_accepted_and_conflicting_spends() {
        use agora_consensus::{
            AcceptanceResult, BlockAcceptance, TxAcceptanceOutcome, UtxoJournalOp,
        };
        use agora_types::AcceptanceBitmap;

        let fp = test_fp();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();

        let outpoint = OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        };
        let mut accepted = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: outpoint,
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            1,
        );
        sign_transaction(&mut accepted, &kp, &fp).unwrap();

        let mut conflict = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: outpoint,
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            2,
        );
        sign_transaction(&mut conflict, &kp, &fp).unwrap();

        let mut unrelated = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash([9u8; 32]),
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            3,
        );
        sign_transaction(&mut unrelated, &kp, &fp).unwrap();

        let mut pool = Mempool::new(16);
        pool.admit(accepted.clone(), &fp).unwrap();
        pool.admit(conflict.clone(), &fp).unwrap();
        pool.admit(unrelated.clone(), &fp).unwrap();

        let result = AcceptanceResult {
            blocks: vec![BlockAcceptance {
                block_hash: Hash::ZERO,
                blue_score: 1,
                bitmap: AcceptanceBitmap::from_bools(&[true]),
                outcomes: vec![TxAcceptanceOutcome {
                    tx_id: accepted.tx_id(),
                    index: 0,
                    is_coinbase: false,
                    structurally_valid: true,
                    accepted: true,
                    fee: Amount::ZERO,
                    reject_reason: None,
                }],
                accepted_fees: Amount::ZERO,
                subsidy: Amount::ZERO,
                coinbase_reward: Amount::ZERO,
            }],
            journal: vec![UtxoJournalOp::Spend { outpoint }],
        };

        pool.evict_by_acceptance(&result);
        assert!(!pool.contains(&accepted.tx_id()));
        assert!(!pool.contains(&conflict.tx_id()));
        assert!(pool.contains(&unrelated.tx_id()));
    }
}
