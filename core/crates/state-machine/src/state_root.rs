//! Canonical Trident multi-asset state-root commitment.
//!
//! Composition (domain-separated), matching Phase 0 audit §5.5:
//! UTXO ∥ OVL accounts ∥ DRC accounts ∥ OVL stake snap ∥ DRC stake snap ∥
//! tip acceptance ∥ finalized tip ∥ gov/treasury placeholder.

use agora_types::{Hash, NativeAssetId, OutPoint, TxOut};
use borsh::BorshDeserialize;

use crate::acceptance::load_acceptance;
use crate::accounts::account_root;
use crate::columns::ColumnFamily;
use crate::finality_store::load_finalized_blue_score;
use crate::payments::drc_payment_root;
use crate::staking::{build_snapshot, load_epoch};
use crate::{StateError, StateStore, TRIDENT_STATE_TRANSITION_VERSION};

/// Domain tag for the composed state root (versioned).
pub const STATE_ROOT_DOMAIN: &[u8] = b"agora-trident-state-root-v3";

/// Deterministic UTXO-set commitment (sorted outpoint keys).
pub fn utxo_commitment(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries: Vec<(OutPoint, TxOut)> = Vec::new();
    store.for_each_cf(ColumnFamily::Utxo, |key, value| {
        if key.len() != 36 {
            return Ok(());
        }
        let mut tx_bytes = [0u8; 32];
        tx_bytes.copy_from_slice(&key[..32]);
        let index = u32::from_le_bytes(key[32..36].try_into().unwrap());
        let out = TxOut::try_from_slice(value).map_err(|e| StateError::Storage(e.to_string()))?;
        entries.push((
            OutPoint {
                tx_id: Hash(tx_bytes),
                index,
            },
            out,
        ));
        Ok(())
    })?;
    entries.sort_by(|a, b| {
        a.0.tx_id
            .as_bytes()
            .cmp(b.0.tx_id.as_bytes())
            .then(a.0.index.cmp(&b.0.index))
    });
    Ok(Hash::hash_borsh(&(b"utxo-v1", &entries)))
}

/// Tip-block acceptance commitment (empty record hash if missing).
pub fn acceptance_root(store: &StateStore, tip_block: &Hash) -> Result<Hash, StateError> {
    match load_acceptance(store, tip_block)? {
        Some(rec) => Ok(Hash::hash_borsh(&(b"acceptance-v2", &rec))),
        None => Ok(Hash::hash_borsh(&(b"acceptance-v2", tip_block, &[] as &[u8]))),
    }
}

/// Finalized-tip marker commitment (not the in-progress certificate — avoids cycles).
pub fn finalized_tip_commitment(store: &StateStore) -> Result<Hash, StateError> {
    let tip = load_finalized_blue_score(store)?.unwrap_or(0);
    Ok(Hash::hash_borsh(&(b"finality-tip-v1", tip)))
}

/// Compose the Trident state root for `tip_block` against current Meta/UTXO state.
pub fn compose_trident_state_root(
    store: &StateStore,
    tip_block: &Hash,
) -> Result<Hash, StateError> {
    let utxo = utxo_commitment(store)?;
    let ovl_accounts = account_root(store, NativeAssetId::OVL)?;
    let drc_accounts = account_root(store, NativeAssetId::DRC)?;
    let epoch_ovl = load_epoch(store, NativeAssetId::OVL)?;
    let epoch_drc = load_epoch(store, NativeAssetId::DRC)?;
    let ovl_stake = build_snapshot(store, NativeAssetId::OVL, epoch_ovl)?.commitment();
    let drc_stake = build_snapshot(store, NativeAssetId::DRC, epoch_drc)?.commitment();
    let drc_payments = drc_payment_root(store)?;
    let acceptance = acceptance_root(store, tip_block)?;
    let finality_tip = finalized_tip_commitment(store)?;
    // Gov/treasury roots activate in Phase 5 — keep explicit placeholder slot.
    let gov_treasury = Hash::ZERO;

    Ok(Hash::hash_borsh(&(
        STATE_ROOT_DOMAIN,
        TRIDENT_STATE_TRANSITION_VERSION,
        utxo,
        ovl_accounts,
        drc_accounts,
        ovl_stake,
        drc_stake,
        drc_payments,
        acceptance,
        finality_tip,
        gov_treasury,
    )))
}

#[cfg(test)]
mod tests {
    use agora_types::{Address, Amount, Hash};

    use super::*;
    use crate::accounts::credit_account_into;
    use crate::store::WriteBatch;
    use crate::StateStore;

    #[test]
    fn state_root_changes_when_account_balance_changes() {
        let store = StateStore::open_in_memory();
        let tip = Hash([1u8; 32]);
        let a = compose_trident_state_root(&store, &tip).unwrap();
        let b = compose_trident_state_root(&store, &tip).unwrap();
        assert_eq!(a, b);

        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            &store,
            NativeAssetId::OVL,
            &Address([9u8; 20]),
            Amount::from_base_units(50),
        )
        .unwrap();
        store.write_batch(batch).unwrap();
        let c = compose_trident_state_root(&store, &tip).unwrap();
        assert_ne!(a, c);
    }

    #[test]
    fn acceptance_root_stable_for_missing() {
        let store = StateStore::open_in_memory();
        let tip = Hash([2u8; 32]);
        assert_eq!(
            acceptance_root(&store, &tip).unwrap(),
            acceptance_root(&store, &tip).unwrap()
        );
    }
}
