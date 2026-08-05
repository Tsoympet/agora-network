use std::collections::{HashMap, HashSet};

use agora_consensus::{precheck_regular_tx, AcceptanceResult, UtxoJournalOp, UtxoView};
use agora_types::{Hash, NetworkFingerprint, OutPoint, Transaction};

use crate::P2pError;

/// Local mempool with fingerprint-bound admission, UTXO precheck, fee/size policy,
/// and acceptance-driven eviction.
#[derive(Debug)]
pub struct Mempool {
    txs: HashMap<Hash, Transaction>,
    /// Outpoints already claimed by mempool txs (first admit wins).
    claimed: HashMap<OutPoint, Hash>,
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
            claimed: HashMap::new(),
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

    /// Admit a transaction after full precheck against the UTXO view.
    ///
    /// Checks: fingerprint signature, ownership, coinbase maturity, min fee,
    /// size/input caps, and mempool input conflicts.
    pub fn admit<V: UtxoView>(
        &mut self,
        tx: Transaction,
        fingerprint: &NetworkFingerprint,
        utxo: &V,
        tip_blue_score: u64,
    ) -> Result<Hash, P2pError> {
        precheck_regular_tx(&tx, utxo, fingerprint, tip_blue_score)
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;

        let id = tx.tx_id();
        if self.txs.len() >= self.max_size && !self.txs.contains_key(&id) {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }

        for input in &tx.inputs {
            if let Some(other) = self.claimed.get(&input.previous_outpoint) {
                if other != &id {
                    return Err(P2pError::MempoolRejected(
                        "input conflicts with mempool tx".into(),
                    ));
                }
            }
        }

        // Replace prior version of same tx_id if present.
        if let Some(prev) = self.txs.remove(&id) {
            for input in &prev.inputs {
                self.claimed.remove(&input.previous_outpoint);
            }
        }
        for input in &tx.inputs {
            self.claimed.insert(input.previous_outpoint, id);
        }
        self.txs.insert(id, tx);
        Ok(id)
    }

    pub fn get(&self, tx_id: &Hash) -> Option<&Transaction> {
        self.txs.get(tx_id)
    }

    pub fn remove(&mut self, tx_id: &Hash) -> Option<Transaction> {
        let tx = self.txs.remove(tx_id)?;
        for input in &tx.inputs {
            if self.claimed.get(&input.previous_outpoint) == Some(tx_id) {
                self.claimed.remove(&input.previous_outpoint);
            }
        }
        Some(tx)
    }

    /// Evict by transaction acceptance — not by block color.
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

        let to_remove: Vec<Hash> = self
            .txs
            .iter()
            .filter(|(tx_id, tx)| {
                drop_ids.contains(tx_id)
                    || tx
                        .inputs
                        .iter()
                        .any(|input| spent.contains(&input.previous_outpoint))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in to_remove {
            self.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use agora_consensus::{
        AcceptanceResult, BlockAcceptance, MemoryUtxoView, TxAcceptanceOutcome, UtxoEntry,
        UtxoJournalOp, COINBASE_MATURITY,
    };
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_types::{
        AcceptanceBitmap, Amount, Hash, NetworkFingerprint, OutPoint, Transaction, TxIn, TxOut,
    };

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

    fn funded_view(kp_addr: agora_types::Address) -> (MemoryUtxoView, OutPoint) {
        let outpoint = OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        };
        let mut view = MemoryUtxoView::new();
        view.insert(
            outpoint,
            UtxoEntry::new(
                TxOut {
                    value: Amount::from_base_units(100),
                    address: kp_addr,
                },
                0,
                false, // non-coinbase so maturity is N/A
            ),
        );
        (view, outpoint)
    }

    #[test]
    fn admits_valid_signed_tx() {
        let fp = test_fp();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let (view, outpoint) = funded_view(kp.address());
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: outpoint,
            }],
            vec![TxOut {
                value: Amount::from_base_units(99),
                address: kp.address(),
            }],
            1,
        );
        sign_transaction(&mut tx, &kp, &fp).unwrap();
        let mut pool = Mempool::new(16);
        let id = pool
            .admit(tx.clone(), &fp, &view, COINBASE_MATURITY)
            .unwrap();
        assert_eq!(id, tx.tx_id());
        assert!(pool.contains(&id));
    }

    #[test]
    fn rejects_unsigned_tx() {
        let tx = Transaction::unsigned(1, vec![], vec![], 1);
        let view = MemoryUtxoView::new();
        let mut pool = Mempool::new(16);
        assert!(pool.admit(tx, &test_fp(), &view, 0).is_err());
    }

    #[test]
    fn rejects_tx_signed_for_other_network() {
        let fp = test_fp();
        let mut other = test_fp();
        other.network_id = 7;
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let (view, outpoint) = funded_view(kp.address());
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: outpoint,
            }],
            vec![TxOut {
                value: Amount::from_base_units(99),
                address: kp.address(),
            }],
            1,
        );
        sign_transaction(&mut tx, &kp, &fp).unwrap();
        let mut pool = Mempool::new(16);
        assert!(pool.admit(tx, &other, &view, 0).is_err());
    }

    #[test]
    fn rejects_zero_fee() {
        let fp = test_fp();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let (view, outpoint) = funded_view(kp.address());
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: outpoint,
            }],
            vec![TxOut {
                value: Amount::from_base_units(100), // fee = 0
                address: kp.address(),
            }],
            1,
        );
        sign_transaction(&mut tx, &kp, &fp).unwrap();
        let mut pool = Mempool::new(16);
        assert!(pool.admit(tx, &fp, &view, 0).is_err());
    }

    #[test]
    fn rejects_immature_coinbase_spend() {
        let fp = test_fp();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let outpoint = OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        };
        let mut view = MemoryUtxoView::new();
        view.insert(
            outpoint,
            UtxoEntry::new(
                TxOut {
                    value: Amount::from_base_units(100),
                    address: kp.address(),
                },
                1,
                true,
            ),
        );
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: outpoint,
            }],
            vec![TxOut {
                value: Amount::from_base_units(99),
                address: kp.address(),
            }],
            1,
        );
        sign_transaction(&mut tx, &kp, &fp).unwrap();
        let mut pool = Mempool::new(16);
        // tip blue score 5 < created(1)+maturity(10)
        assert!(pool.admit(tx, &fp, &view, 5).is_err());
    }

    #[test]
    fn evicts_accepted_and_conflicting_spends() {
        let fp = test_fp();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();

        let outpoint = OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        };
        let other_out = OutPoint {
            tx_id: Hash([9u8; 32]),
            index: 0,
        };
        let mut view = MemoryUtxoView::new();
        view.insert(
            outpoint,
            UtxoEntry::new(
                TxOut {
                    value: Amount::from_base_units(100),
                    address: kp.address(),
                },
                0,
                false,
            ),
        );
        view.insert(
            other_out,
            UtxoEntry::new(
                TxOut {
                    value: Amount::from_base_units(100),
                    address: kp.address(),
                },
                0,
                false,
            ),
        );

        let mut accepted = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: outpoint,
            }],
            vec![TxOut {
                value: Amount::from_base_units(99),
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
                value: Amount::from_base_units(98),
                address: kp.address(),
            }],
            2,
        );
        sign_transaction(&mut conflict, &kp, &fp).unwrap();

        let mut unrelated = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: other_out,
            }],
            vec![TxOut {
                value: Amount::from_base_units(99),
                address: kp.address(),
            }],
            3,
        );
        sign_transaction(&mut unrelated, &kp, &fp).unwrap();

        let mut pool = Mempool::new(16);
        pool.admit(accepted.clone(), &fp, &view, 0).unwrap();
        // conflict shares outpoint — rejected at admit
        assert!(pool.admit(conflict.clone(), &fp, &view, 0).is_err());
        pool.admit(unrelated.clone(), &fp, &view, 0).unwrap();

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
                    fee: Amount::from_base_units(1),
                    reject_reason: None,
                }],
                accepted_fees: Amount::from_base_units(1),
                subsidy: Amount::ZERO,
                coinbase_reward: Amount::ZERO,
            }],
            journal: vec![UtxoJournalOp::Spend { outpoint }],
        };

        pool.evict_by_acceptance(&result);
        assert!(!pool.contains(&accepted.tx_id()));
        assert!(pool.contains(&unrelated.tx_id()));
    }
}
