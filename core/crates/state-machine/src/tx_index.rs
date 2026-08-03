//! Transaction location index in `cf_warm` for RPC / explorer lookups.
//!
//! Key: `tx/` ‖ `tx_id` (35 bytes) — prefixed so it never collides with raw block hashes.
//! Value: `block_id` ‖ `index` LE (36 bytes).

use agora_types::{Block, Hash};

use crate::columns::ColumnFamily;
use crate::{StateError, StateStore};

const TX_INDEX_PREFIX: &[u8] = b"tx/";

pub fn tx_index_key(tx_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(TX_INDEX_PREFIX.len() + 32);
    key.extend_from_slice(TX_INDEX_PREFIX);
    key.extend_from_slice(tx_id.as_bytes());
    key
}

pub fn encode_tx_location(block_id: &Hash, index: u32) -> [u8; 36] {
    let mut value = [0u8; 36];
    value[..32].copy_from_slice(block_id.as_bytes());
    value[32..].copy_from_slice(&index.to_le_bytes());
    value
}

pub fn decode_tx_location(bytes: &[u8]) -> Option<(Hash, u32)> {
    if bytes.len() != 36 {
        return None;
    }
    let mut id = [0u8; 32];
    id.copy_from_slice(&bytes[..32]);
    let index = u32::from_le_bytes(bytes[32..36].try_into().ok()?);
    Some((Hash(id), index))
}

/// Index every transaction in `block` under `cf_warm`.
pub fn index_block_transactions(store: &StateStore, block: &Block) -> Result<(), StateError> {
    let block_id = block.id();
    for (index, tx) in block.transactions.iter().enumerate() {
        let key = tx_index_key(&tx.tx_id());
        let value = encode_tx_location(&block_id, index as u32);
        store.put_cf(ColumnFamily::Warm, &key, &value)?;
    }
    Ok(())
}

/// Resolve `tx_id` → `(block_id, index)` if indexed.
pub fn lookup_tx_location(
    store: &StateStore,
    tx_id: &Hash,
) -> Result<Option<(Hash, u32)>, StateError> {
    let key = tx_index_key(tx_id);
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &key)? else {
        return Ok(None);
    };
    Ok(decode_tx_location(&bytes))
}
