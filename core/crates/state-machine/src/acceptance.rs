//! Explicit transaction acceptance records for the Virtual UTXO apply path.
//!
//! Soft-skip under [`crate::ApplyMode::Virtual`] remains the conflict-resolution
//! engine; this module makes its outcomes durable and typed so fees, confirmations,
//! mempool eviction, and explorers never treat “blue” as “accepted.”

use agora_types::{AcceptanceBitmap, Hash, TransactionAcceptance};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const ACCEPTANCE_PREFIX: &[u8] = b"acceptance/";

/// Per-block acceptance outcomes aligned to `block.transactions` indices.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Default)]
pub struct BlockAcceptanceRecord {
    pub block_hash: Hash,
    pub statuses: Vec<TransactionAcceptance>,
}

impl BlockAcceptanceRecord {
    pub fn bitmap(&self) -> AcceptanceBitmap {
        let flags: Vec<bool> = self.statuses.iter().map(|s| s.is_accepted()).collect();
        AcceptanceBitmap::from_bools(&flags)
    }

    pub fn status_at(&self, index: usize) -> Option<TransactionAcceptance> {
        self.statuses.get(index).copied()
    }

    pub fn accepted_count(&self) -> usize {
        self.statuses.iter().filter(|s| s.is_accepted()).count()
    }
}

pub fn acceptance_key(hash: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(ACCEPTANCE_PREFIX.len() + 32);
    key.extend_from_slice(ACCEPTANCE_PREFIX);
    key.extend_from_slice(hash.as_bytes());
    key
}

pub fn put_acceptance_into(
    batch: &mut WriteBatch,
    hash: &Hash,
    record: &BlockAcceptanceRecord,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(record).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Warm, &acceptance_key(hash), &bytes);
    Ok(())
}

pub fn store_acceptance(
    store: &StateStore,
    hash: &Hash,
    record: &BlockAcceptanceRecord,
) -> Result<(), StateError> {
    let mut batch = WriteBatch::new();
    put_acceptance_into(&mut batch, hash, record)?;
    store.write_batch(batch)
}

pub fn load_acceptance(
    store: &StateStore,
    hash: &Hash,
) -> Result<Option<BlockAcceptanceRecord>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &acceptance_key(hash))? else {
        return Ok(None);
    };
    BlockAcceptanceRecord::try_from_slice(&bytes)
        .map(Some)
        .map_err(|e| StateError::Storage(e.to_string()))
}

pub fn delete_acceptance_into(batch: &mut WriteBatch, hash: &Hash) {
    batch.delete_cf(ColumnFamily::Warm, &acceptance_key(hash));
}

/// Look up acceptance for a tx included in `block_hash` at `index`.
pub fn tx_acceptance_status(
    store: &StateStore,
    block_hash: &Hash,
    index: u32,
) -> Result<Option<TransactionAcceptance>, StateError> {
    let Some(record) = load_acceptance(store, block_hash)? else {
        return Ok(None);
    };
    Ok(record.status_at(index as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::TransactionAcceptance;

    #[test]
    fn record_roundtrip_and_bitmap() {
        let store = StateStore::open_in_memory();
        let hash = Hash([3u8; 32]);
        let record = BlockAcceptanceRecord {
            block_hash: hash,
            statuses: vec![
                TransactionAcceptance::Accepted,
                TransactionAcceptance::ConflictLost,
                TransactionAcceptance::ExactDuplicate,
            ],
        };
        store_acceptance(&store, &hash, &record).unwrap();
        let loaded = load_acceptance(&store, &hash).unwrap().unwrap();
        assert_eq!(loaded, record);
        assert_eq!(loaded.accepted_count(), 1);
        assert!(loaded.bitmap().is_accepted(0));
        assert!(!loaded.bitmap().is_accepted(1));
        assert_eq!(
            tx_acceptance_status(&store, &hash, 1).unwrap(),
            Some(TransactionAcceptance::ConflictLost)
        );
    }
}
