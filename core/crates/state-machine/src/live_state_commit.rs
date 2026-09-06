//! Atomic durable consumer for a verified Trident Block 0 live-state plan.
//!
//! This module proves storage readiness only. It deliberately exposes no
//! networking, RPC, mining, or legacy-block conversion capability.

use std::collections::BTreeMap;

use agora_types::{Hash, TridentHeader};

use crate::block_zero::{
    encode_block_zero_records, load_verified_trident_block_zero, verify_trident_datadir_identity,
    TridentBlockZeroState, TridentBlockZeroStorageRecord, TridentDatadirIdentity,
};
use crate::columns::{meta_keys, ColumnFamily};
use crate::live_state_plan::{TridentLiveStatePlan, TridentLiveStateRoots};
use crate::store::{KvPair, StateStore, WriteBatch};
use crate::StateError;

type StoreSnapshot = Vec<(ColumnFamily, Vec<KvPair>)>;

mod readiness_seal {
    #[derive(Debug, PartialEq, Eq)]
    pub(super) struct Seal;
}

/// Proof that the exact initial Trident state has been durably reread and
/// independently verified.
///
/// Fields and construction are sealed inside this crate. This capability only
/// clears the atomic-storage prerequisite; it does not authorize node, P2P,
/// RPC, mining, or consensus startup.
#[derive(Debug, PartialEq, Eq)]
#[must_use = "verified Trident storage readiness must be retained by its future consumer"]
pub struct TridentLiveStateReadiness {
    manifest_root: Hash,
    body_root: Hash,
    state_roots: TridentLiveStateRoots,
    state_root: Hash,
    header_hash: Hash,
    datadir_identity: TridentDatadirIdentity,
    _seal: readiness_seal::Seal,
}

impl TridentLiveStateReadiness {
    pub fn manifest_root(&self) -> Hash {
        self.manifest_root
    }

    pub fn body_root(&self) -> Hash {
        self.body_root
    }

    pub fn state_roots(&self) -> &TridentLiveStateRoots {
        &self.state_roots
    }

    pub fn state_root(&self) -> Hash {
        self.state_root
    }

    pub fn header_hash(&self) -> Hash {
        self.header_hash
    }

    pub fn datadir_identity(&self) -> &TridentDatadirIdentity {
        &self.datadir_identity
    }
}

impl TridentLiveStatePlan {
    /// Commit this independently verified plan, Block 0 envelope, and datadir
    /// identity through one durable [`WriteBatch`].
    ///
    /// A fresh store is COW-preflighted before the sole durable write. An exact
    /// already-committed store is verified and returned idempotently without a
    /// write. Any other existing state is rejected without overwrite.
    pub fn commit_atomically(
        &self,
        store: &StateStore,
        state: &TridentBlockZeroState,
        header: &TridentHeader,
    ) -> Result<TridentLiveStateReadiness, StateError> {
        self.verify(state, header)?;
        let before = snapshot_store(store)?;
        if !snapshot_is_empty(&before) {
            return reopen_verified_trident_live_state(store, state, header);
        }

        let expected_record =
            TridentBlockZeroStorageRecord::from_state_and_header(state, Some(header))
                .map_err(storage_error)?;
        let expected = expected_snapshot(self, &expected_record)?;
        let batch = batch_from_snapshot(&expected);

        let overlay = store.cow_overlay();
        overlay.write_batch(batch.clone())?;
        verify_exact_materialization(&overlay, state, header, self, &expected_record, &expected)?;
        if snapshot_store(store)? != before {
            return Err(storage_error(
                "Trident live-state COW preflight observed concurrent base mutation",
            ));
        }

        store.write_batch(batch)?;

        // Re-derive the plan instead of trusting preflight objects before
        // constructing the only capability exposed to future startup code.
        reopen_verified_trident_live_state(store, state, header)
    }
}

/// Reopen an exact committed Trident initial state and return a sealed storage
/// readiness capability only after every root, envelope, and identity rereads
/// and verifies.
pub fn reopen_verified_trident_live_state(
    store: &StateStore,
    state: &TridentBlockZeroState,
    header: &TridentHeader,
) -> Result<TridentLiveStateReadiness, StateError> {
    let plan = TridentLiveStatePlan::derive_verified(state, header)?;
    let expected_record = TridentBlockZeroStorageRecord::from_state_and_header(state, Some(header))
        .map_err(storage_error)?;
    let expected = expected_snapshot(&plan, &expected_record)?;
    let loaded =
        verify_exact_materialization(store, state, header, &plan, &expected_record, &expected)?;

    Ok(TridentLiveStateReadiness {
        manifest_root: loaded.manifest.manifest_root(),
        body_root: plan.body_root,
        state_roots: plan.state_roots,
        state_root: loaded.manifest.state_root(),
        header_hash: plan.header_hash,
        datadir_identity: loaded.datadir_identity,
        _seal: readiness_seal::Seal,
    })
}

fn expected_snapshot(
    plan: &TridentLiveStatePlan,
    storage_record: &TridentBlockZeroStorageRecord,
) -> Result<StoreSnapshot, StateError> {
    let mut entries = BTreeMap::<(u8, Vec<u8>), Vec<u8>>::new();
    for record in plan.records() {
        insert_expected(
            &mut entries,
            record.column_family(),
            record.key(),
            record.value(),
        )?;
    }
    for (key, value) in encode_block_zero_records(storage_record)? {
        insert_expected(&mut entries, ColumnFamily::Meta, &key, &value)?;
    }

    Ok(ColumnFamily::ALL
        .iter()
        .copied()
        .map(|cf| {
            let records = entries
                .iter()
                .filter(|((stored_cf, _), _)| *stored_cf == cf as u8)
                .map(|((_, key), value)| (key.clone(), value.clone()))
                .collect();
            (cf, records)
        })
        .collect())
}

fn insert_expected(
    entries: &mut BTreeMap<(u8, Vec<u8>), Vec<u8>>,
    cf: ColumnFamily,
    key: &[u8],
    value: &[u8],
) -> Result<(), StateError> {
    if entries
        .insert((cf as u8, key.to_vec()), value.to_vec())
        .is_some()
    {
        return Err(storage_error(
            "duplicate key across Trident live state and Block 0 envelope",
        ));
    }
    Ok(())
}

fn batch_from_snapshot(snapshot: &StoreSnapshot) -> WriteBatch {
    let mut batch = WriteBatch::new();
    for (cf, records) in snapshot {
        for (key, value) in records {
            batch.put_cf(*cf, key, value);
        }
    }
    batch
}

fn verify_exact_materialization(
    store: &StateStore,
    state: &TridentBlockZeroState,
    header: &TridentHeader,
    plan: &TridentLiveStatePlan,
    expected_record: &TridentBlockZeroStorageRecord,
    expected_snapshot: &StoreSnapshot,
) -> Result<TridentBlockZeroStorageRecord, StateError> {
    plan.verify(state, header)?;
    if snapshot_store(store)? != *expected_snapshot {
        return Err(storage_error(
            "existing Trident live state is partial, mismatched, or contains unexpected records",
        ));
    }

    // This reread recomputes every component root from the committed bytes and
    // verifies the body/header encodings against their independently supplied
    // identities.
    plan.verify_overlay(store)?;

    require_exact_value(
        store,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_PLAN_VERSION,
        &plan.version.to_le_bytes(),
        "live-state plan version",
    )?;
    require_exact_value(
        store,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_MANIFEST_ROOT,
        state.manifest_root().as_bytes(),
        "live-state manifest root",
    )?;
    require_exact_value(
        store,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_BODY_ROOT,
        plan.body.root().as_bytes(),
        "live-state body root",
    )?;
    require_exact_value(
        store,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_STATE_ROOT,
        plan.state_roots.state_root().as_bytes(),
        "live-state composed root",
    )?;
    require_exact_value(
        store,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_HEADER_HASH,
        plan.header_hash.as_bytes(),
        "live-state header hash",
    )?;
    require_exact_value(
        store,
        ColumnFamily::Meta,
        meta_keys::GENESIS_HASH,
        plan.header_hash.as_bytes(),
        "genesis header hash",
    )?;

    let loaded = load_verified_trident_block_zero(store)?;
    if loaded != *expected_record
        || loaded.manifest != *state
        || loaded.commitment.manifest_root != state.manifest_root()
        || loaded.commitment.state_root != plan.state_roots.state_root()
        || loaded.datadir_identity.committed_state_root != plan.state_root
        || loaded.datadir_identity.block_zero_header_hash != Some(plan.header_hash)
        || header.body_root != plan.body.root()
        || header.state_root != plan.state_roots.state_root()
    {
        return Err(storage_error(
            "durable Trident Block 0 roots or identities do not match verified inputs",
        ));
    }
    verify_trident_datadir_identity(store, &expected_record.datadir_identity)?;
    Ok(loaded)
}

fn require_exact_value(
    store: &StateStore,
    cf: ColumnFamily,
    key: &[u8],
    expected: &[u8],
    label: &str,
) -> Result<(), StateError> {
    match store.get_cf(cf, key)? {
        Some(actual) if actual == expected => Ok(()),
        _ => Err(storage_error(format!(
            "durable Trident {label} is missing or mismatched"
        ))),
    }
}

fn snapshot_store(store: &StateStore) -> Result<StoreSnapshot, StateError> {
    ColumnFamily::ALL
        .iter()
        .copied()
        .map(|cf| Ok((cf, store.scan_prefix(cf, &[])?)))
        .collect()
}

fn snapshot_is_empty(snapshot: &StoreSnapshot) -> bool {
    snapshot.iter().all(|(_, records)| records.is_empty())
}

fn storage_error(message: impl Into<String>) -> StateError {
    StateError::Storage(message.into())
}
