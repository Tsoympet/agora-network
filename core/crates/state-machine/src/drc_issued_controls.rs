//! Issued-asset policy, line freeze/auth, and clawback (issuer liabilities only).

use agora_types::{
    resolve_drc_account_sequence, DrcIssuedAssetPolicyAction, DrcIssuedAssetPolicyLive,
    DrcIssuedAssetPolicyReceipt, DrcIssuedAssetPolicySetTx, DrcIssuedClawbackReceipt,
    DrcIssuedClawbackTx, DrcTrustLineIssuerControlAction, DrcTrustLineIssuerControlReceipt,
    DrcTrustLineIssuerControlTx, DrcTrustLineLive, Hash, IssuedAssetId, NativeAssetId,
    DRC_ISSUED_ASSET_POLICY_RECEIPT_VERSION, DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION,
    DRC_ISSUED_CLAWBACK_RECEIPT_VERSION, DRC_ISSUED_CLAWBACK_TICKET_VERSION,
    DRC_TRUST_LINE_ISSUER_CONTROL_RECEIPT_VERSION, DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_VERSION,
    DRC_TRUST_LINE_LIVE_STATE_V2,
};

use crate::accounts::{load_account, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_account_auth::{
    verify_drc_issued_asset_policy_set_operation, verify_drc_issued_clawback_operation,
    verify_drc_trust_line_issuer_control_operation,
};
use crate::drc_ticket::begin_drc_account_sequence;
use crate::drc_trust_line::{
    issuer_liability_key, load_drc_issuer_liability, load_drc_trust_line_live, put_liability,
    put_live, trust_line_key,
};
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

use agora_types::drc_issued_asset_policy_meta_key;

const POLICY_RECEIPT_PREFIX: &[u8] = b"trust/drc/asset-policy-rcpt/";
const CONTROL_RECEIPT_PREFIX: &[u8] = b"trust/drc/line-control-rcpt/";
const CLAWBACK_RECEIPT_PREFIX: &[u8] = b"trust/drc/clawback-rcpt/";

pub const DRC_ISSUED_CONTROLS_ROOT_DOMAIN: &[u8] = b"agora-drc-issued-controls-root-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssuedMovementKind {
    Issue,
    HolderTransfer,
    Redeem,
}

pub fn load_drc_issued_asset_policy(
    store: &StateStore,
    asset: &IssuedAssetId,
) -> Result<DrcIssuedAssetPolicyLive, StateError> {
    let key = drc_issued_asset_policy_meta_key(asset);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? else {
        return Ok(DrcIssuedAssetPolicyLive::default_for_asset(*asset));
    };
    let live = DrcIssuedAssetPolicyLive::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    live.validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(live)
}

pub fn normalize_trust_line_live(
    live: DrcTrustLineLive,
    policy: &DrcIssuedAssetPolicyLive,
) -> DrcTrustLineLive {
    let mut out = live;
    if out.version == agora_types::DRC_TRUST_LINE_LIVE_STATE_VERSION {
        out.authorized = !policy.require_auth;
    }
    out
}

pub fn issued_movement_allowed(
    policy: &DrcIssuedAssetPolicyLive,
    line: &DrcTrustLineLive,
    kind: IssuedMovementKind,
) -> Result<(), StateError> {
    if !line.authorized {
        return Err(StateError::InvalidTx("trust line not authorized".into()));
    }
    let movement_frozen = policy.global_freeze || line.line_frozen;
    match kind {
        IssuedMovementKind::Issue | IssuedMovementKind::HolderTransfer => {
            if movement_frozen {
                return Err(StateError::InvalidTx("issued movement frozen".into()));
            }
        }
        IssuedMovementKind::Redeem => {
            if line.line_deep_frozen {
                return Err(StateError::InvalidTx("issued redeem deep-frozen".into()));
            }
        }
    }
    Ok(())
}

fn policy_receipt_key(tx_id: &Hash) -> Vec<u8> {
    let mut k = Vec::with_capacity(POLICY_RECEIPT_PREFIX.len() + 32);
    k.extend_from_slice(POLICY_RECEIPT_PREFIX);
    k.extend_from_slice(tx_id.as_bytes());
    k
}

fn control_receipt_key(tx_id: &Hash) -> Vec<u8> {
    let mut k = Vec::with_capacity(CONTROL_RECEIPT_PREFIX.len() + 32);
    k.extend_from_slice(CONTROL_RECEIPT_PREFIX);
    k.extend_from_slice(tx_id.as_bytes());
    k
}

fn clawback_receipt_key(tx_id: &Hash) -> Vec<u8> {
    let mut k = Vec::with_capacity(CLAWBACK_RECEIPT_PREFIX.len() + 32);
    k.extend_from_slice(CLAWBACK_RECEIPT_PREFIX);
    k.extend_from_slice(tx_id.as_bytes());
    k
}

pub fn issued_controls_meta_keys_for_policy(tx: &DrcIssuedAssetPolicySetTx) -> Vec<Vec<u8>> {
    vec![drc_issued_asset_policy_meta_key(&tx.asset_id())]
}

pub fn issued_controls_meta_keys_for_issuer_control(
    tx: &DrcTrustLineIssuerControlTx,
) -> Vec<Vec<u8>> {
    agora_types::drc_trust_line_issuer_control_mutation_meta_keys(tx)
}

pub fn issued_controls_meta_keys_for_clawback(tx: &DrcIssuedClawbackTx) -> Vec<Vec<u8>> {
    agora_types::drc_issued_clawback_mutation_meta_keys(tx)
}

pub fn drc_issued_controls_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut policies: Vec<DrcIssuedAssetPolicyLive> = Vec::new();
    for (key, bytes) in store.scan_prefix(
        ColumnFamily::Meta,
        agora_types::DRC_ISSUED_ASSET_POLICY_META_PREFIX,
    )? {
        if key.len() != agora_types::DRC_ISSUED_ASSET_POLICY_META_PREFIX.len() + 32 {
            continue;
        }
        let live = DrcIssuedAssetPolicyLive::try_from_slice(&bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        live.validate()
            .map_err(|e| StateError::Storage(e.to_string()))?;
        policies.push(live);
    }
    policies.sort_by(|a, b| {
        a.asset
            .asset_key()
            .as_bytes()
            .cmp(b.asset.asset_key().as_bytes())
    });
    Ok(Hash::hash_borsh(&(
        DRC_ISSUED_CONTROLS_ROOT_DOMAIN,
        policies,
    )))
}

#[allow(clippy::type_complexity)]
pub fn apply_drc_issued_asset_policy_set(
    store: &StateStore,
    tx: &DrcIssuedAssetPolicySetTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
    blue_score: u64,
) -> Result<Vec<(Vec<u8>, Option<Vec<u8>>)>, StateError> {
    verify_drc_issued_asset_policy_set_operation(store, tx, auth)?;
    let asset = tx.asset_id();
    if tx.issuer != asset.issuer {
        return Err(StateError::InvalidTx("issuer mismatch".into()));
    }
    let mut policy = load_drc_issued_asset_policy(store, &asset)?;
    let liability = load_drc_issuer_liability(store, &asset)?;
    let meta_key = drc_issued_asset_policy_meta_key(&asset);
    let prior = store.get_cf(ColumnFamily::Meta, &meta_key)?;
    let mut meta_before = vec![(meta_key.clone(), prior)];

    let sequence_ctx = if tx.version >= DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION {
        Some(begin_drc_account_sequence(
            store,
            &tx.issuer,
            resolve_drc_account_sequence(
                tx.version,
                DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION,
                tx.nonce,
                tx.account_sequence,
            )
            .map_err(|e| StateError::InvalidTx(e.to_string()))?,
        )?)
    } else {
        let acct = load_account(store, NativeAssetId::DRC, &tx.issuer)?;
        if acct.nonce != tx.nonce {
            return Err(StateError::InvalidTx("bad policy set nonce".into()));
        }
        None
    };

    match tx.action {
        DrcIssuedAssetPolicyAction::EnableRequireAuth => {
            if liability.as_units() != 0 {
                return Err(StateError::InvalidTx(
                    "require_auth only at zero liability".into(),
                ));
            }
            if policy.require_auth {
                return Err(StateError::InvalidTx("require_auth already enabled".into()));
            }
        }
        DrcIssuedAssetPolicyAction::EnableGlobalFreeze => {
            if policy.no_freeze {
                return Err(StateError::InvalidTx("no_freeze set".into()));
            }
        }
        DrcIssuedAssetPolicyAction::ClearGlobalFreeze => {
            if policy.no_freeze {
                return Err(StateError::InvalidTx("no_freeze set".into()));
            }
        }
        DrcIssuedAssetPolicyAction::EnableNoFreeze => {
            if policy.no_freeze {
                return Err(StateError::InvalidTx("no_freeze already set".into()));
            }
            if policy.global_freeze || policy.clawback_enabled {
                return Err(StateError::InvalidTx(
                    "no_freeze incompatible with active policy".into(),
                ));
            }
            if crate::drc_trust_line::asset_has_active_line_freeze(store, &asset, &policy)? {
                return Err(StateError::InvalidTx(
                    "no_freeze incompatible with line freeze".into(),
                ));
            }
        }
        DrcIssuedAssetPolicyAction::EnableClawback => {
            if liability.as_units() != 0 {
                return Err(StateError::InvalidTx(
                    "clawback only at zero liability".into(),
                ));
            }
            if policy.no_freeze {
                return Err(StateError::InvalidTx("no_freeze set".into()));
            }
            if policy.clawback_enabled {
                return Err(StateError::InvalidTx("clawback already enabled".into()));
            }
        }
    }

    crate::drc_trust_line::debit_drc_fee(store, batch, journal, &tx.issuer, tx.fee, sequence_ctx)?;

    match tx.action {
        DrcIssuedAssetPolicyAction::EnableRequireAuth => {
            meta_before.extend(crate::drc_trust_line::upgrade_trust_lines_for_require_auth(
                store, &asset, batch,
            )?);
            policy.require_auth = true;
        }
        DrcIssuedAssetPolicyAction::EnableGlobalFreeze => {
            policy.global_freeze = true;
        }
        DrcIssuedAssetPolicyAction::ClearGlobalFreeze => {
            policy.global_freeze = false;
        }
        DrcIssuedAssetPolicyAction::EnableNoFreeze => {
            policy.no_freeze = true;
        }
        DrcIssuedAssetPolicyAction::EnableClawback => {
            policy.clawback_enabled = true;
        }
    }
    policy
        .validate()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &meta_key,
        &borsh::to_vec(&policy).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    let receipt = DrcIssuedAssetPolicyReceipt {
        version: DRC_ISSUED_ASSET_POLICY_RECEIPT_VERSION,
        policy_set_tx_id: tx.policy_set_tx_id(),
        asset,
        action: tx.action,
        settlement_blue_score: blue_score,
    };
    batch.put_cf(
        ColumnFamily::Meta,
        &policy_receipt_key(&tx.policy_set_tx_id()),
        &borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(meta_before)
}

#[allow(clippy::type_complexity)]
pub fn apply_drc_trust_line_issuer_control(
    store: &StateStore,
    tx: &DrcTrustLineIssuerControlTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
    blue_score: u64,
) -> Result<Vec<(Vec<u8>, Option<Vec<u8>>)>, StateError> {
    verify_drc_trust_line_issuer_control_operation(store, tx, auth)?;
    let asset = tx.asset_id();
    let policy = load_drc_issued_asset_policy(store, &asset)?;
    let Some(mut line) = load_drc_trust_line_live(store, &tx.holder, &asset)? else {
        return Err(StateError::InvalidTx("missing trust line".into()));
    };
    line = normalize_trust_line_live(line, &policy);
    let line_key = trust_line_key(&tx.holder, &asset);
    let prior = store.get_cf(ColumnFamily::Meta, &line_key)?;
    let meta_before = vec![(line_key.clone(), prior)];

    let sequence_ctx = if tx.version >= DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_VERSION {
        Some(begin_drc_account_sequence(
            store,
            &tx.issuer,
            resolve_drc_account_sequence(
                tx.version,
                DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_VERSION,
                tx.nonce,
                tx.account_sequence,
            )
            .map_err(|e| StateError::InvalidTx(e.to_string()))?,
        )?)
    } else {
        let acct = load_account(store, NativeAssetId::DRC, &tx.issuer)?;
        if acct.nonce != tx.nonce {
            return Err(StateError::InvalidTx("bad issuer control nonce".into()));
        }
        None
    };

    match tx.action {
        DrcTrustLineIssuerControlAction::AuthorizeHolder => {
            if line.authorized {
                return Err(StateError::InvalidTx("already authorized".into()));
            }
        }
        DrcTrustLineIssuerControlAction::SetLineFrozen(true) => {
            if policy.no_freeze || policy.global_freeze {
                return Err(StateError::InvalidTx("freeze not allowed".into()));
            }
        }
        DrcTrustLineIssuerControlAction::SetLineFrozen(false) => {
            if line.line_deep_frozen {
                return Err(StateError::InvalidTx("clear deep freeze first".into()));
            }
        }
        DrcTrustLineIssuerControlAction::SetLineDeepFrozen(true) => {
            if policy.no_freeze || policy.global_freeze {
                return Err(StateError::InvalidTx("deep freeze not allowed".into()));
            }
            if !line.line_frozen {
                return Err(StateError::InvalidTx("line must be frozen first".into()));
            }
        }
        DrcTrustLineIssuerControlAction::SetLineDeepFrozen(false) => {}
    }

    crate::drc_trust_line::debit_drc_fee(store, batch, journal, &tx.issuer, tx.fee, sequence_ctx)?;

    match tx.action {
        DrcTrustLineIssuerControlAction::AuthorizeHolder => {
            line.authorized = true;
        }
        DrcTrustLineIssuerControlAction::SetLineFrozen(true) => {
            line.line_frozen = true;
        }
        DrcTrustLineIssuerControlAction::SetLineFrozen(false) => {
            line.line_frozen = false;
        }
        DrcTrustLineIssuerControlAction::SetLineDeepFrozen(true) => {
            line.line_deep_frozen = true;
        }
        DrcTrustLineIssuerControlAction::SetLineDeepFrozen(false) => {
            line.line_deep_frozen = false;
        }
    }
    line.version = DRC_TRUST_LINE_LIVE_STATE_V2;
    line.validate()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    put_live(batch, &line)?;
    let receipt = DrcTrustLineIssuerControlReceipt {
        version: DRC_TRUST_LINE_ISSUER_CONTROL_RECEIPT_VERSION,
        control_tx_id: tx.issuer_control_tx_id(),
        asset,
        holder: tx.holder,
        action: tx.action,
        settlement_blue_score: blue_score,
    };
    batch.put_cf(
        ColumnFamily::Meta,
        &control_receipt_key(&tx.issuer_control_tx_id()),
        &borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(meta_before)
}

#[allow(clippy::type_complexity)]
pub fn apply_drc_issued_clawback(
    store: &StateStore,
    tx: &DrcIssuedClawbackTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
    blue_score: u64,
) -> Result<Vec<(Vec<u8>, Option<Vec<u8>>)>, StateError> {
    verify_drc_issued_clawback_operation(store, tx, auth)?;
    let asset = tx.asset_id();
    let policy = load_drc_issued_asset_policy(store, &asset)?;
    if !policy.clawback_enabled {
        return Err(StateError::InvalidTx("clawback not enabled".into()));
    }
    if policy.no_freeze {
        return Err(StateError::InvalidTx(
            "clawback incompatible with no_freeze".into(),
        ));
    }
    let Some(mut line) = load_drc_trust_line_live(store, &tx.holder, &asset)? else {
        return Err(StateError::InvalidTx("missing trust line".into()));
    };
    line = normalize_trust_line_live(line, &policy);
    if !line.authorized {
        return Err(StateError::InvalidTx("line not authorized".into()));
    }
    let mut liability = load_drc_issuer_liability(store, &asset)?;
    let line_key = trust_line_key(&tx.holder, &asset);
    let liab_key = issuer_liability_key(&asset);
    let meta_before = vec![
        (
            line_key.clone(),
            store.get_cf(ColumnFamily::Meta, &line_key)?,
        ),
        (
            liab_key.clone(),
            store.get_cf(ColumnFamily::Meta, &liab_key)?,
        ),
    ];

    if tx.amount.as_units() == 0 {
        return Err(StateError::InvalidTx(
            "clawback amount must be positive".into(),
        ));
    }
    if tx.amount.as_units() > line.balance.as_units() {
        return Err(StateError::InvalidTx("clawback exceeds balance".into()));
    }

    let sequence_ctx = if tx.version >= DRC_ISSUED_CLAWBACK_TICKET_VERSION {
        Some(begin_drc_account_sequence(
            store,
            &tx.issuer,
            resolve_drc_account_sequence(
                tx.version,
                DRC_ISSUED_CLAWBACK_TICKET_VERSION,
                tx.nonce,
                tx.account_sequence,
            )
            .map_err(|e| StateError::InvalidTx(e.to_string()))?,
        )?)
    } else {
        let acct = load_account(store, NativeAssetId::DRC, &tx.issuer)?;
        if acct.nonce != tx.nonce {
            return Err(StateError::InvalidTx("bad clawback nonce".into()));
        }
        None
    };

    crate::drc_trust_line::debit_drc_fee(store, batch, journal, &tx.issuer, tx.fee, sequence_ctx)?;

    line.balance = line
        .balance
        .checked_sub(tx.amount)
        .ok_or_else(|| StateError::InvalidTx("clawback underflow".into()))?;
    liability = liability
        .checked_sub(tx.amount)
        .ok_or_else(|| StateError::InvalidTx("liability underflow".into()))?;
    line.version = DRC_TRUST_LINE_LIVE_STATE_V2;
    put_live(batch, &line)?;
    put_liability(batch, &asset, liability)?;
    let receipt = DrcIssuedClawbackReceipt {
        version: DRC_ISSUED_CLAWBACK_RECEIPT_VERSION,
        clawback_tx_id: tx.clawback_tx_id(),
        asset,
        holder: tx.holder,
        amount: tx.amount,
        settlement_blue_score: blue_score,
    };
    batch.put_cf(
        ColumnFamily::Meta,
        &clawback_receipt_key(&tx.clawback_tx_id()),
        &borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(meta_before)
}

use borsh::BorshDeserialize;
