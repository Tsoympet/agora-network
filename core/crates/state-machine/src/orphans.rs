//! Durable orphan block bodies in `cf_warm` (survive node restart during IBD).

use agora_types::{Block, Hash};
use borsh::BorshDeserialize;

use crate::columns::ColumnFamily;
use crate::{StateError, StateStore};

const ORPHAN_PREFIX: &[u8] = b"orphan/";

pub fn orphan_key(hash: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(ORPHAN_PREFIX.len() + 32);
    key.extend_from_slice(ORPHAN_PREFIX);
    key.extend_from_slice(hash.as_bytes());
    key
}

pub fn store_orphan(store: &StateStore, block: &Block) -> Result<(), StateError> {
    let id = block.id();
    let bytes = borsh::to_vec(block).map_err(|e| StateError::Storage(e.to_string()))?;
    store.put_cf(ColumnFamily::Warm, &orphan_key(&id), &bytes)
}

pub fn delete_orphan(store: &StateStore, hash: &Hash) -> Result<(), StateError> {
    store.delete_cf(ColumnFamily::Warm, &orphan_key(hash))
}

pub fn load_orphan(store: &StateStore, hash: &Hash) -> Result<Option<Block>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &orphan_key(hash))? else {
        return Ok(None);
    };
    let block = Block::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(Some(block))
}

/// Load every persisted orphan body (unordered).
pub fn list_orphans(store: &StateStore) -> Result<Vec<Block>, StateError> {
    let mut out = Vec::new();
    for (key, value) in store.scan_prefix(ColumnFamily::Warm, ORPHAN_PREFIX)? {
        if key.len() != ORPHAN_PREFIX.len() + 32 {
            continue;
        }
        let block =
            Block::try_from_slice(&value).map_err(|e| StateError::Storage(e.to_string()))?;
        out.push(block);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{BlockHeader, Hash};

    #[test]
    fn orphan_roundtrip_and_list() {
        let store = StateStore::open_in_memory();
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 9,
                bits: 0,
                nonce: 1,
                tx_root: Hash::ZERO,
            },
            transactions: vec![],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
        };
        let id = block.id();
        store_orphan(&store, &block).unwrap();
        assert_eq!(load_orphan(&store, &id).unwrap().unwrap().id(), id);
        assert_eq!(list_orphans(&store).unwrap().len(), 1);
        delete_orphan(&store, &id).unwrap();
        assert!(list_orphans(&store).unwrap().is_empty());
    }
}
