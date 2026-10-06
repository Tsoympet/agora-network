//! Canonical Trident multi-asset state-root commitment.
//!
//! Composition (domain-separated), matching Phase 0 audit §5.5:
//! UTXO ∥ OVL accounts ∥ DRC accounts ∥ OVL stake snap ∥ DRC stake snap ∥
//! DRC authorization/settlement state (including issuer-scoped trust lines and
//! controls) ∥ DRC policy ∥ DRC deposit preauthorization ∥ DRC payment state ∥
//! native supply accounting ∥ tip acceptance ∥ finalized tip ∥
//! governance/treasuries ∥ canonical community registry ∥ authenticated
//! data-commitment state.

use agora_types::{Hash, NativeAssetId, OutPoint, TxOut};
use borsh::BorshDeserialize;

use crate::acceptance::load_acceptance;
use crate::accounts::account_root;
use crate::columns::ColumnFamily;
use crate::community_state::canonical_community_root;
use crate::data_availability::data_availability_root;
use crate::drc_check::drc_check_root;
use crate::drc_deposit_preauth::drc_deposit_preauth_root;
use crate::drc_escrow::drc_escrow_root;
use crate::drc_ledger_object::drc_ledger_object_index_root;
use crate::drc_payment_channel::drc_payment_channel_root;
use crate::drc_policy::drc_account_policy_root;
use crate::drc_regular_key::drc_regular_key_root;
use crate::drc_signer_list::drc_signer_list_root;
use crate::drc_ticket::drc_ticket_root;
use crate::drc_trust_line::drc_trust_line_root;
use crate::finality_store::load_finalized_blue_score;
use crate::governance_state::governance_treasury_root;
use crate::payments::drc_payment_root;
use crate::staking::{build_snapshot, load_epoch};
use crate::supply::native_supply_root;
use crate::{StateError, StateStore, TRIDENT_STATE_TRANSITION_VERSION};

/// Domain tag for the composed state root (versioned).
pub const STATE_ROOT_DOMAIN: &[u8] = b"agora-trident-state-root-v15";

/// Sorted UTXO set exported for snapshots. The column family itself is not pruned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UtxoSetSnapshot {
    pub entries: Vec<(OutPoint, TxOut)>,
}

/// Read every 36-byte UTXO key and sort by transaction id, then index.
fn collect_utxo_entries(store: &StateStore) -> Result<Vec<(OutPoint, TxOut)>, StateError> {
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
    Ok(entries)
}

/// Commitment of an already sorted UTXO export. Same domain as [`utxo_commitment`].
pub fn utxo_entries_commitment(entries: &[(OutPoint, TxOut)]) -> Hash {
    Hash::hash_borsh(&(b"utxo-v1", entries))
}

/// True when `snapshot` is the set that produced `commitment`.
pub fn utxo_snapshot_binds(snapshot: &UtxoSetSnapshot, commitment: &Hash) -> bool {
    utxo_entries_commitment(&snapshot.entries) == *commitment
}

/// Export the live UTXO set without deleting `cf_utxo`.
pub fn export_utxo_snapshot(store: &StateStore) -> Result<UtxoSetSnapshot, StateError> {
    Ok(UtxoSetSnapshot {
        entries: collect_utxo_entries(store)?,
    })
}

/// Deterministic UTXO-set commitment (sorted outpoint keys).
pub fn utxo_commitment(store: &StateStore) -> Result<Hash, StateError> {
    let snapshot = export_utxo_snapshot(store)?;
    Ok(utxo_entries_commitment(&snapshot.entries))
}

/// Tip-block acceptance commitment (empty record hash if missing).
pub fn acceptance_root(store: &StateStore, tip_block: &Hash) -> Result<Hash, StateError> {
    match load_acceptance(store, tip_block)? {
        Some(rec) => Ok(Hash::hash_borsh(&(b"acceptance-v6", &rec))),
        None => Ok(Hash::hash_borsh(&(
            b"acceptance-v6",
            tip_block,
            &[] as &[u8],
        ))),
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
    let drc_regular_keys = drc_regular_key_root(store)?;
    let drc_signer_lists = drc_signer_list_root(store)?;
    let drc_tickets = drc_ticket_root(store)?;
    let drc_escrow = drc_escrow_root(store)?;
    let drc_checks = drc_check_root(store)?;
    let drc_payment_channels = drc_payment_channel_root(store)?;
    let drc_trust_lines = drc_trust_line_root(store)?;
    let drc_issued_controls = crate::drc_issued_controls::drc_issued_controls_root(store)?;
    let drc_issued_liability = Hash::hash_borsh(&(
        b"drc-issued-liability-v1",
        drc_trust_lines,
        drc_issued_controls,
    ));
    let drc_check_paychan = Hash::hash_borsh(&(
        b"drc-check-paychan-v2",
        drc_checks,
        drc_payment_channels,
        drc_issued_liability,
    ));
    let drc_account_policies = drc_account_policy_root(store)?;
    let drc_deposit_preauths = drc_deposit_preauth_root(store)?;
    let drc_payments = drc_payment_root(store)?;
    let drc_ledger_objects = drc_ledger_object_index_root(store)?;
    let native_supply = native_supply_root(store)?;
    let acceptance = acceptance_root(store, tip_block)?;
    let finality_tip = finalized_tip_commitment(store)?;
    let gov_treasury = governance_treasury_root(store)?;
    let community = canonical_community_root(store)?;
    let data_availability = data_availability_root(store)?;

    let components = [
        utxo,
        ovl_accounts,
        drc_accounts,
        ovl_stake,
        drc_stake,
        drc_regular_keys,
        drc_signer_lists,
        drc_tickets,
        drc_escrow,
        drc_check_paychan,
        drc_account_policies,
        drc_deposit_preauths,
        drc_payments,
        drc_ledger_objects,
        native_supply,
        acceptance,
        finality_tip,
        gov_treasury,
        community,
        data_availability,
    ];
    Ok(Hash::hash_borsh(&(
        STATE_ROOT_DOMAIN,
        TRIDENT_STATE_TRANSITION_VERSION,
        components,
    )))
}

#[cfg(test)]
mod tests {
    use agora_types::{Address, Amount, Hash};

    use super::*;
    use crate::accounts::credit_account_into;
    use crate::store::WriteBatch;
    use crate::supply::{put_burned_supply_into, put_issued_supply_into};
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
    fn utxo_snapshot_binds_to_the_same_commitment() {
        let store = StateStore::open_in_memory();
        crate::GenesisBuilder::default().ignite(&store).unwrap();
        let commitment = utxo_commitment(&store).unwrap();
        let snapshot = export_utxo_snapshot(&store).unwrap();
        assert!(utxo_snapshot_binds(&snapshot, &commitment));
        assert_eq!(utxo_entries_commitment(&snapshot.entries), commitment);
        assert!(!snapshot.entries.is_empty());
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

    #[test]
    fn state_root_commits_native_burn_accounting() {
        let store = StateStore::open_in_memory();
        let tip = Hash([3u8; 32]);
        let mut batch = WriteBatch::new();
        put_issued_supply_into(&mut batch, NativeAssetId::DRC, 10);
        put_burned_supply_into(&mut batch, NativeAssetId::DRC, 0);
        store.write_batch(batch).unwrap();
        let before = compose_trident_state_root(&store, &tip).unwrap();

        let mut batch = WriteBatch::new();
        put_burned_supply_into(&mut batch, NativeAssetId::DRC, 1);
        store.write_batch(batch).unwrap();
        let after = compose_trident_state_root(&store, &tip).unwrap();
        assert_ne!(before, after);
    }
}
