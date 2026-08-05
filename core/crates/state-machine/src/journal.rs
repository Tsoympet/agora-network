//! Atomic persistence of acceptance bitmaps together with UTXO journals.
//!
//! Acceptance status and UTXO mutations for a blue block are committed in a
//! single [`StateStore::write_batch`] — never piecemeal.

use agora_consensus::{AcceptanceResult, BlockAcceptance, UtxoJournalOp, UtxoView};
use agora_types::{
    AcceptanceBitmap, Amount, Hash, NetworkFingerprint, OutPoint, TxAcceptanceStatus,
    TxConfirmation, TxOut,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::{acceptance_keys, meta_keys, utxo_key_outpoint, ColumnFamily};
use crate::store::{StateStore, StoreOp};
use crate::StateError;

/// Per-tx index record used by RPC / explorer confirmations.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AcceptedTxRecord {
    pub block_hash: Hash,
    pub blue_score: u64,
    pub index: u32,
    pub accepted: bool,
}

/// Apply a full acceptance result: UTXO journal + bitmaps + tx index, atomically.
pub fn commit_acceptance(
    store: &StateStore,
    result: &AcceptanceResult,
) -> Result<(), StateError> {
    let mut ops = Vec::new();

    for block in &result.blocks {
        append_block_acceptance_ops(&mut ops, block)?;
    }

    // Apply UTXO journal ops (spends/creates) from the acceptance run.
    for journal_op in &result.journal {
        match journal_op {
            UtxoJournalOp::Spend { outpoint } => {
                ops.push(StoreOp::Delete {
                    cf: ColumnFamily::Utxo,
                    key: utxo_key_outpoint(outpoint),
                });
            }
            UtxoJournalOp::Create { outpoint, output } => {
                let value = borsh::to_vec(output)
                    .map_err(|e| StateError::Storage(e.to_string()))?;
                ops.push(StoreOp::Put {
                    cf: ColumnFamily::Utxo,
                    key: utxo_key_outpoint(outpoint),
                    value,
                });
            }
        }
    }

    store.write_batch(ops)
}

fn append_block_acceptance_ops(
    ops: &mut Vec<StoreOp>,
    block: &BlockAcceptance,
) -> Result<(), StateError> {
    let bitmap_bytes = borsh::to_vec(&block.bitmap)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    ops.push(StoreOp::Put {
        cf: ColumnFamily::Warm,
        key: acceptance_keys::bitmap(&block.block_hash),
        value: bitmap_bytes,
    });

    // Persist fee / reward summary beside the bitmap for explorer queries.
    let summary = AcceptanceSummary {
        accepted_fees: block.accepted_fees.as_base_units(),
        subsidy: block.subsidy.as_base_units(),
        coinbase_reward: block.coinbase_reward.as_base_units(),
        blue_score: block.blue_score,
    };
    let summary_bytes =
        borsh::to_vec(&summary).map_err(|e| StateError::Storage(e.to_string()))?;
    let mut summary_key = b"accept/summary/".to_vec();
    summary_key.extend_from_slice(block.block_hash.as_bytes());
    ops.push(StoreOp::Put {
        cf: ColumnFamily::Warm,
        key: summary_key,
        value: summary_bytes,
    });

    for outcome in &block.outcomes {
        let record = AcceptedTxRecord {
            block_hash: block.block_hash,
            blue_score: block.blue_score,
            index: outcome.index,
            accepted: outcome.accepted,
        };
        let value = borsh::to_vec(&record).map_err(|e| StateError::Storage(e.to_string()))?;
        ops.push(StoreOp::Put {
            cf: ColumnFamily::Warm,
            key: acceptance_keys::tx_index(&outcome.tx_id),
            value,
        });
    }

    Ok(())
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
struct AcceptanceSummary {
    accepted_fees: u64,
    subsidy: u64,
    coinbase_reward: u64,
    blue_score: u64,
}

/// Load the acceptance bitmap for a block, if committed.
pub fn load_acceptance_bitmap(
    store: &StateStore,
    block_hash: &Hash,
) -> Result<Option<AcceptanceBitmap>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &acceptance_keys::bitmap(block_hash))?
    else {
        return Ok(None);
    };
    let bitmap = AcceptanceBitmap::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(Some(bitmap))
}

/// Confirmation status for a transaction — based on acceptance, not block color.
pub fn tx_confirmation(
    store: &StateStore,
    tx_id: &Hash,
    tip_blue_score: u64,
) -> Result<TxConfirmation, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &acceptance_keys::tx_index(tx_id))? else {
        return Ok(TxConfirmation::pending());
    };
    let record = AcceptedTxRecord::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if record.accepted {
        Ok(TxConfirmation::accepted(
            record.block_hash,
            record.blue_score,
            tip_blue_score,
        ))
    } else {
        Ok(TxConfirmation::rejected())
    }
}

/// UTXO view backed by the durable store.
pub struct StoreUtxoView<'a> {
    store: &'a StateStore,
}

impl<'a> StoreUtxoView<'a> {
    pub fn new(store: &'a StateStore) -> Self {
        Self { store }
    }
}

impl UtxoView for StoreUtxoView<'_> {
    fn get(&self, outpoint: &OutPoint) -> Option<TxOut> {
        let key = utxo_key_outpoint(outpoint);
        let bytes = self.store.get_cf(ColumnFamily::Utxo, &key).ok()??;
        TxOut::try_from_slice(&bytes).ok()
    }
}

/// Persist / verify the network fingerprint that binds this datadir.
pub fn write_network_fingerprint(
    store: &StateStore,
    fingerprint: &NetworkFingerprint,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(fingerprint).map_err(|e| StateError::Storage(e.to_string()))?;
    store.put_cf(ColumnFamily::Meta, meta_keys::NETWORK_FINGERPRINT, &bytes)
}

pub fn load_network_fingerprint(
    store: &StateStore,
) -> Result<Option<NetworkFingerprint>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, meta_keys::NETWORK_FINGERPRINT)? else {
        return Ok(None);
    };
    let fp = NetworkFingerprint::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(Some(fp))
}

/// Refuse to use a datadir whose stored fingerprint does not match `expected`.
pub fn assert_datadir_fingerprint(
    store: &StateStore,
    expected: &NetworkFingerprint,
) -> Result<(), StateError> {
    match load_network_fingerprint(store)? {
        None => Err(StateError::FingerprintMismatch(
            "datadir has no network fingerprint".into(),
        )),
        Some(stored) if stored == *expected => Ok(()),
        Some(stored) => Err(StateError::FingerprintMismatch(format!(
            "datadir fingerprint {} != expected {}",
            stored.digest_hex(),
            expected.digest_hex()
        ))),
    }
}

/// Helper for explorer / RPC: was tx index `i` accepted in `block_hash`?
pub fn is_tx_accepted_in_block(
    store: &StateStore,
    block_hash: &Hash,
    index: usize,
) -> Result<Option<bool>, StateError> {
    Ok(load_acceptance_bitmap(store, block_hash)?.map(|b| b.is_accepted(index)))
}

/// Status enum convenience for RPC serialization.
pub fn acceptance_status(store: &StateStore, tx_id: &Hash) -> Result<TxAcceptanceStatus, StateError> {
    Ok(tx_confirmation(store, tx_id, 0)?.status)
}

/// Read accepted fees committed for a block (None if not committed).
pub fn load_accepted_fees(store: &StateStore, block_hash: &Hash) -> Result<Option<Amount>, StateError> {
    let mut key = b"accept/summary/".to_vec();
    key.extend_from_slice(block_hash.as_bytes());
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &key)? else {
        return Ok(None);
    };
    let summary = AcceptanceSummary::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(Some(Amount::from_base_units(summary.accepted_fees)))
}
