use std::collections::HashMap;

use agora_crypto::verify_transaction;
use agora_types::{Hash, Transaction};

use crate::P2pError;

/// Local mempool with signature-gated admission.
#[derive(Debug, Default)]
pub struct Mempool {
    txs: HashMap<Hash, Transaction>,
    max_size: usize,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Self {
            txs: HashMap::new(),
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

    /// Admit a transaction after secp256k1 verification.
    pub fn admit(&mut self, tx: Transaction) -> Result<Hash, P2pError> {
        verify_transaction(&tx).map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
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

    /// Lookup by first 8 bytes of `tx_id` for compact-block inflation.
    pub fn get_by_short_id(&self, short_id: &[u8; 8]) -> Option<&Transaction> {
        self.txs
            .iter()
            .find(|(id, _)| &id.as_bytes()[..8] == short_id.as_slice())
            .map(|(_, tx)| tx)
    }

    pub fn remove(&mut self, tx_id: &Hash) -> Option<Transaction> {
        self.txs.remove(tx_id)
    }
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_types::{Amount, Hash, OutPoint, Transaction, TxIn, TxOut};

    use super::*;

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn admits_valid_signed_tx() {
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
        sign_transaction(&mut tx, &kp).unwrap();
        let mut pool = Mempool::new(16);
        let id = pool.admit(tx.clone()).unwrap();
        assert_eq!(id, tx.tx_id());
        assert!(pool.contains(&id));
    }

    #[test]
    fn rejects_unsigned_tx() {
        let tx = Transaction::unsigned(1, vec![], vec![], 1);
        let mut pool = Mempool::new(16);
        assert!(pool.admit(tx).is_err());
    }
}
