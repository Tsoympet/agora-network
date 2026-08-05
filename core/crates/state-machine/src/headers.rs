//! Durable block headers in `cf_warm` (survive Hot body prune).

use agora_types::{BlockHeader, Hash};
use borsh::BorshDeserialize;

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const HEADER_PREFIX: &[u8] = b"header/";

pub fn header_key(hash: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(HEADER_PREFIX.len() + 32);
    key.extend_from_slice(HEADER_PREFIX);
    key.extend_from_slice(hash.as_bytes());
    key
}

pub fn store_header(
    store: &StateStore,
    hash: &Hash,
    header: &BlockHeader,
) -> Result<(), StateError> {
    let mut batch = WriteBatch::new();
    store_header_into(&mut batch, hash, header)?;
    store.write_batch(batch)
}

pub fn store_header_into(
    batch: &mut WriteBatch,
    hash: &Hash,
    header: &BlockHeader,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(header).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Warm, &header_key(hash), &bytes);
    Ok(())
}

pub fn load_header(store: &StateStore, hash: &Hash) -> Result<Option<BlockHeader>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &header_key(hash))? else {
        return Ok(None);
    };
    let header =
        BlockHeader::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(Some(header))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StateStore;
    use agora_types::Hash;

    #[test]
    fn header_roundtrip() {
        let store = StateStore::open_in_memory();
        let hash = Hash::hash_bytes(b"h1");
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 7,
            bits: 0,
            nonce: 3,
            tx_root: Hash::ZERO,
        };
        store_header(&store, &hash, &header).unwrap();
        let loaded = load_header(&store, &hash).unwrap().unwrap();
        assert_eq!(loaded.nonce, 3);
        assert_eq!(loaded.parents, vec![Hash::ZERO]);
    }
}
