//! Public mempool admission for issued-asset policy, line issuer control, and clawback.

use std::sync::Mutex;

use agora_p2p::Mempool;
use agora_rpc::RpcError;
use agora_state_machine::{
    apply_drc_issued_asset_policy_set, apply_drc_issued_clawback,
    apply_drc_trust_line_issuer_control, load_drc_issued_asset_policy,
    require_auth_migration_meta_keys, AccountJournal, StateStore, TxAuthContext, WriteBatch,
};
use agora_types::{
    DrcIssuedAssetPolicyAction, DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx,
    DrcTrustLineIssuerControlTx, Hash, NativeAssetId,
};

use crate::backend::min_relay_fee;

pub(crate) fn revalidate_issued_controls_mempool(
    store: &StateStore,
    pool: &mut Mempool,
    auth: &TxAuthContext,
) {
    let policies = pool.pending_issued_asset_policy_set_txs();
    let stale_policies: Vec<Hash> = policies
        .iter()
        .filter_map(|tx| {
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            apply_drc_issued_asset_policy_set(store, tx, auth, &mut batch, &mut journal, 1)
                .err()
                .map(|_| tx.policy_set_tx_id())
        })
        .collect();
    for id in stale_policies {
        let _ = pool.remove_drc_issued_asset_policy_set(&id);
    }

    let controls = pool.pending_trust_line_issuer_control_txs();
    let stale_controls: Vec<Hash> = controls
        .iter()
        .filter_map(|tx| {
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            apply_drc_trust_line_issuer_control(store, tx, auth, &mut batch, &mut journal, 1)
                .err()
                .map(|_| tx.issuer_control_tx_id())
        })
        .collect();
    for id in stale_controls {
        let _ = pool.remove_drc_trust_line_issuer_control(&id);
    }

    let clawbacks = pool.pending_issued_clawback_txs();
    let stale_clawbacks: Vec<Hash> = clawbacks
        .iter()
        .filter_map(|tx| {
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            apply_drc_issued_clawback(store, tx, auth, &mut batch, &mut journal, 1)
                .err()
                .map(|_| tx.clawback_tx_id())
        })
        .collect();
    for id in stale_clawbacks {
        let _ = pool.remove_drc_issued_clawback(&id);
    }
}

pub(crate) fn admit_drc_issued_asset_policy_set(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcIssuedAssetPolicySetTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.issuer) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    let asset = tx.asset_id();
    let asset_key = asset.asset_key();
    if pool.asset_policy_mutation_reserved(&asset_key) {
        return Err(RpcError::Rejected(
            "asset policy mutation already pending".into(),
        ));
    }
    if pool.issuer_liability_mutation_reserved(&asset_key) {
        return Err(RpcError::Rejected(
            "issuer liability mutation pending on asset".into(),
        ));
    }
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_issued_asset_policy_set(
        store,
        &tx,
        auth,
        &mut batch,
        &mut journal,
        application_blue_score,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC issued asset policy set: {error}")))?;

    let mut extra_meta = Vec::new();
    if tx.action == DrcIssuedAssetPolicyAction::EnableRequireAuth {
        extra_meta = require_auth_migration_meta_keys(store, &asset)
            .map_err(|e| RpcError::Rejected(e.to_string()))?;
        for key in &extra_meta {
            if pool.trust_line_meta_key_reserved(key) {
                return Err(RpcError::Rejected(
                    "require_auth migration conflicts with pending line mutation".into(),
                ));
            }
        }
    }

    pool.admit_drc_issued_asset_policy_set(tx, extra_meta)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_trust_line_issuer_control(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcTrustLineIssuerControlTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.issuer) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    let asset_key = tx.asset_id().asset_key();
    if pool.asset_policy_mutation_reserved(&asset_key) {
        return Err(RpcError::Rejected("asset policy mutation pending".into()));
    }
    if pool.trust_line_mutation_reserved(&tx.holder, &asset_key) {
        return Err(RpcError::Rejected(
            "trust line set/delete pending on line".into(),
        ));
    }
    if pool.issuer_control_mutation_reserved(&tx.holder, &asset_key) {
        return Err(RpcError::Rejected(
            "issuer control already pending for line".into(),
        ));
    }
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_trust_line_issuer_control(
        store,
        &tx,
        auth,
        &mut batch,
        &mut journal,
        application_blue_score,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC trust line issuer control: {error}")))?;
    pool.admit_drc_trust_line_issuer_control(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_issued_clawback(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcIssuedClawbackTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.issuer) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    let asset_key = tx.asset_id().asset_key();
    if pool.asset_policy_mutation_reserved(&asset_key) {
        return Err(RpcError::Rejected("asset policy mutation pending".into()));
    }
    if pool.issuer_liability_mutation_reserved(&asset_key) {
        return Err(RpcError::Rejected(
            "issuer liability mutation pending".into(),
        ));
    }
    if pool.issuer_control_mutation_reserved(&tx.holder, &asset_key) {
        return Err(RpcError::Rejected(
            "issuer control pending on holder line".into(),
        ));
    }
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_issued_clawback(
        store,
        &tx,
        auth,
        &mut batch,
        &mut journal,
        application_blue_score,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC issued clawback: {error}")))?;
    pool.admit_drc_issued_clawback(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn get_drc_issued_asset_policy_json(
    store: &StateStore,
    asset: &agora_types::IssuedAssetId,
) -> Result<serde_json::Value, RpcError> {
    asset
        .validate()
        .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
    let live = load_drc_issued_asset_policy(store, asset)
        .map_err(|e| RpcError::Internal(e.to_string()))?;
    Ok(serde_json::json!({
        "issuer": asset.issuer.to_hex(),
        "currency": crate::backend::currency_hex(&asset.currency),
        "require_auth": live.require_auth,
        "global_freeze": live.global_freeze,
        "no_freeze": live.no_freeze,
        "clawback_enabled": live.clawback_enabled,
        "admission_not_finality": true,
    }))
}
