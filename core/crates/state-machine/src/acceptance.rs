//! Explicit transaction acceptance records for the Virtual apply path.
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

/// Per-block acceptance outcomes for multi-lane Trident bodies.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Default)]
pub struct BlockAcceptanceRecord {
    pub block_hash: Hash,
    /// Aligned to `block.transactions` indices (TLT UTXO lane).
    pub statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.account_transfers`.
    pub account_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.stake_ops`.
    pub stake_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct LegacyBlockAcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
}

impl BlockAcceptanceRecord {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StateError> {
        if let Ok(rec) = Self::try_from_slice(bytes) {
            return Ok(rec);
        }
        let legacy = LegacyBlockAcceptanceRecord::try_from_slice(bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        Ok(Self {
            block_hash: legacy.block_hash,
            statuses: legacy.statuses,
            account_statuses: Vec::new(),
            stake_statuses: Vec::new(),
        })
    }

    pub fn bitmap(&self) -> AcceptanceBitmap {
        let flags: Vec<bool> = self.statuses.iter().map(|s| s.is_accepted()).collect();
        AcceptanceBitmap::from_bools(&flags)
    }

    pub fn status_at(&self, index: usize) -> Option<TransactionAcceptance> {
        self.statuses.get(index).copied()
    }

    pub fn accepted_count(&self) -> usize {
        self.statuses.iter().filter(|s| s.is_accepted()).count()
            + self.account_statuses.iter().filter(|s| s.is_accepted()).count()
            + self.stake_statuses.iter().filter(|s| s.is_accepted()).count()
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
    Ok(Some(BlockAcceptanceRecord::from_bytes(&bytes)?))
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
        let rec = BlockAcceptanceRecord {
            block_hash: Hash([1u8; 32]),
            statuses: vec![
                TransactionAcceptance::Accepted,
                TransactionAcceptance::ConflictLost,
            ],
            account_statuses: vec![TransactionAcceptance::Accepted],
            stake_statuses: vec![],
        };
        store_acceptance(&store, &rec.block_hash, &rec).unwrap();
        let loaded = load_acceptance(&store, &rec.block_hash).unwrap().unwrap();
        assert_eq!(loaded.accepted_count(), 2);
        let bm = loaded.bitmap();
        assert_eq!(bm.get(0), Some(true));
        assert_eq!(bm.get(1), Some(false));
    }
}
