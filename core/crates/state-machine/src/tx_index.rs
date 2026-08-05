//! Transaction location index in `cf_warm` for RPC / explorer lookups.
//!
//! A transaction may appear in multiple competing blocks. The index stores **every**
//! inclusion under `tx/` ‖ `tx_id` ‖ `block_id` → `index` LE, and maintains a primary
//! pointer `tx/` ‖ `tx_id` → currently preferred location (virtual-blue preferred).

use agora_types::{Block, Hash};

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const TX_INDEX_PREFIX: &[u8] = b"tx/";
const TX_INCL_PREFIX: &[u8] = b"txi/";

pub fn tx_index_key(tx_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(TX_INDEX_PREFIX.len() + 32);
    key.extend_from_slice(TX_INDEX_PREFIX);
    key.extend_from_slice(tx_id.as_bytes());
    key
}

/// Per-inclusion key: `txi/` ‖ tx_id ‖ block_id.
pub fn tx_inclusion_key(tx_id: &Hash, block_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(TX_INCL_PREFIX.len() + 64);
    key.extend_from_slice(TX_INCL_PREFIX);
    key.extend_from_slice(tx_id.as_bytes());
    key.extend_from_slice(block_id.as_bytes());
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

/// Index every transaction in `block` under `cf_warm` (legacy eager path).
pub fn index_block_transactions(store: &StateStore, block: &Block) -> Result<(), StateError> {
    let mut batch = WriteBatch::new();
    index_block_transactions_into(&mut batch, block);
    store.write_batch(batch)
}

/// Append tx-index mutations for `block` into an existing batch.
pub fn index_block_transactions_into(batch: &mut WriteBatch, block: &Block) {
    let block_id = block.id();
    for (index, tx) in block.transactions.iter().enumerate() {
        let tx_id = tx.tx_id();
        let value = encode_tx_location(&block_id, index as u32);
        // Multi-inclusion record (never overwritten by competing blocks).
        batch.put_cf(
            ColumnFamily::Warm,
            &tx_inclusion_key(&tx_id, &block_id),
            &value,
        );
        // Primary pointer — caller should refresh from virtual tip after reorg.
        batch.put_cf(ColumnFamily::Warm, &tx_index_key(&tx_id), &value);
    }
}

/// Point the primary `tx/` pointer at `block_id` when that inclusion exists.
pub fn set_primary_tx_location(batch: &mut WriteBatch, tx_id: &Hash, block_id: &Hash, index: u32) {
    let value = encode_tx_location(block_id, index);
    batch.put_cf(ColumnFamily::Warm, &tx_index_key(tx_id), &value);
}

/// Resolve primary `tx_id` → `(block_id, index)` if indexed.
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

/// List every known inclusion of `tx_id`.
pub fn list_tx_inclusions(
    store: &StateStore,
    tx_id: &Hash,
) -> Result<Vec<(Hash, u32)>, StateError> {
    let mut prefix = Vec::with_capacity(TX_INCL_PREFIX.len() + 32);
    prefix.extend_from_slice(TX_INCL_PREFIX);
    prefix.extend_from_slice(tx_id.as_bytes());
    let pairs = store.scan_prefix(ColumnFamily::Warm, &prefix)?;
    let mut out = Vec::new();
    for (_k, v) in pairs {
        if let Some(loc) = decode_tx_location(&v) {
            out.push(loc);
        }
    }
    Ok(out)
}
