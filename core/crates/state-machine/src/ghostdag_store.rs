//! Durable GHOSTDAG coloring records (compact merge-set form).

use agora_types::Hash;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::{StateError, StateStore};

const GHOSTDAG_PREFIX: &[u8] = b"ghostdag/";

/// Persisted GHOSTDAG coloring for one block (compact).
///
/// `mergeset_blues` + `selected_parent` reconstruct the full blue set by walking the
/// selected-parent chain — O(mergeset) storage per block instead of O(past).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GhostdagRecord {
    pub selected_parent: Option<Hash>,
    pub blue_score: u64,
    pub blue_work: u128,
    pub block_work: u128,
    pub mergeset_blues: Vec<Hash>,
}

pub fn ghostdag_key(hash: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(GHOSTDAG_PREFIX.len() + 32);
    key.extend_from_slice(GHOSTDAG_PREFIX);
    key.extend_from_slice(hash.as_bytes());
    key
}

pub fn store_ghostdag_record(
    store: &StateStore,
    hash: &Hash,
    record: &GhostdagRecord,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(record).map_err(|e| StateError::Storage(e.to_string()))?;
    store.put_cf(ColumnFamily::Warm, &ghostdag_key(hash), &bytes)
}

pub fn load_ghostdag_record(
    store: &StateStore,
    hash: &Hash,
) -> Result<Option<GhostdagRecord>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &ghostdag_key(hash))? else {
        return Ok(None);
    };
    match GhostdagRecord::try_from_slice(&bytes) {
        Ok(record) => Ok(Some(record)),
        // Legacy full-blues encoding cannot be mapped losslessly to mergeset form —
        // return None so callers recolor from headers.
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StateStore;

    #[test]
    fn ghostdag_record_roundtrip() {
        let store = StateStore::open_in_memory();
        let hash = Hash([3u8; 32]);
        let parent = Hash([1u8; 32]);
        let mut mergeset = vec![Hash([2u8; 32])];
        mergeset.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        let record = GhostdagRecord {
            selected_parent: Some(parent),
            blue_score: 3,
            blue_work: 42,
            block_work: 10,
            mergeset_blues: mergeset,
        };
        store_ghostdag_record(&store, &hash, &record).unwrap();
        assert_eq!(
            load_ghostdag_record(&store, &hash).unwrap().unwrap(),
            record
        );
    }
}
