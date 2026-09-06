//! Durable per-block UTXO journals for virtual-chain reorg.

use agora_types::Hash;

use crate::apply::UtxoJournal;
use crate::columns::ColumnFamily;
use crate::{StateError, StateStore};

const UTXO_DIFF_PREFIX: &[u8] = b"utxo_diff/";

pub fn utxo_diff_key(hash: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(UTXO_DIFF_PREFIX.len() + 32);
    key.extend_from_slice(UTXO_DIFF_PREFIX);
    key.extend_from_slice(hash.as_bytes());
    key
}

pub fn store_utxo_journal(
    store: &StateStore,
    hash: &Hash,
    journal: &UtxoJournal,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(journal).map_err(|e| StateError::Storage(e.to_string()))?;
    store.put_cf(ColumnFamily::Warm, &utxo_diff_key(hash), &bytes)
}

pub fn load_utxo_journal(
    store: &StateStore,
    hash: &Hash,
) -> Result<Option<UtxoJournal>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &utxo_diff_key(hash))? else {
        return Ok(None);
    };
    Ok(Some(UtxoJournal::from_bytes(&bytes)?))
}

pub fn delete_utxo_journal(store: &StateStore, hash: &Hash) -> Result<(), StateError> {
    store.delete_cf(ColumnFamily::Warm, &utxo_diff_key(hash))
}

#[cfg(test)]
mod tests {
    use agora_types::{Address, Amount, OutPoint, TxOut};

    use super::*;
    use crate::StateStore;

    #[test]
    fn journal_roundtrip() {
        let store = StateStore::open_in_memory();
        let hash = Hash([9u8; 32]);
        let journal = UtxoJournal {
            spent: vec![(
                OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
                TxOut {
                    value: Amount::from_base_units(7),
                    address: Address::ZERO,
                },
            )],
            created: vec![OutPoint {
                tx_id: hash,
                index: 0,
            }],
            fees: 1,
            subsidy: 6,
            coinbase_total: 7,
            account_before: Vec::new(),
            stake_meta_before: Vec::new(),
            payment_meta_before: Vec::new(),
            data_availability_meta_before: Vec::new(),
        };
        store_utxo_journal(&store, &hash, &journal).unwrap();
        let loaded = load_utxo_journal(&store, &hash).unwrap().unwrap();
        assert_eq!(loaded.spent.len(), 1);
        assert_eq!(loaded.created.len(), 1);
        delete_utxo_journal(&store, &hash).unwrap();
        assert!(load_utxo_journal(&store, &hash).unwrap().is_none());
    }
}
