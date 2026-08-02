//! UTXO key helpers (outpoint → CF key).

use agora_types::{Hash, OutPoint};

/// Encode `tx_id || index_le` (36 bytes), matching genesis ignition.
pub fn outpoint_key(outpoint: &OutPoint) -> [u8; 36] {
    let mut key = [0u8; 36];
    key[..32].copy_from_slice(outpoint.tx_id.as_bytes());
    key[32..].copy_from_slice(&outpoint.index.to_le_bytes());
    key
}

pub fn outpoint_key_parts(tx_id: &Hash, index: u32) -> [u8; 36] {
    outpoint_key(&OutPoint {
        tx_id: *tx_id,
        index,
    })
}
