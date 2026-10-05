//! Public mempool admission helpers for DRC trust line lanes.

use std::sync::Mutex;

use agora_p2p::Mempool;
use agora_rpc::RpcError;
use agora_state_machine::{
    apply_drc_issued_transfer, apply_drc_trust_line_set, load_drc_trust_line_live, AccountJournal,
    StateStore, TxAuthContext, WriteBatch,
};
use agora_types::{DrcIssuedTransferTx, DrcTrustLineSetTx, Hash, IssuedAmount, NativeAssetId};

use crate::backend::min_relay_fee;

fn validate_issued_transfer_mempool_overlay(
    store: &StateStore,
    pool: &Mempool,
    tx: &DrcIssuedTransferTx,
) -> Result<(), RpcError> {
    let asset = tx.asset_id();
    let asset_key = asset.asset_key();
    let issuer = asset.issuer;

    let effective_line =
        |holder: agora_types::Address| -> Result<Option<(IssuedAmount, IssuedAmount)>, RpcError> {
            if pool.pending_trust_line_delete(&holder, &asset_key) {
                return Err(RpcError::Rejected(
                    "mempool rejects transfer while trust line delete is pending".into(),
                ));
            }
            if pool.pending_trust_line_create(&holder, &asset_key) {
                return Err(RpcError::Rejected(
                    "mempool rejects transfer while trust line create is pending".into(),
                ));
            }
            let canonical = load_drc_trust_line_live(store, &holder, &asset)
                .map_err(|e| RpcError::Rejected(e.to_string()))?;
            Ok(canonical.map(|live| (live.limit, live.balance)))
        };

    let pending = |holder: agora_types::Address| -> i128 {
        pool.trust_line_pending_balance_delta(&holder, &asset_key)
    };

    if tx.sender != issuer {
        let Some((limit, balance)) = effective_line(tx.sender)? else {
            return Err(RpcError::Rejected("sender missing trust line".into()));
        };
        let effective_balance = balance.as_units() as i128 + pending(tx.sender);
        if effective_balance < tx.amount.as_units() as i128 {
            return Err(RpcError::Rejected(
                "mempool rejects transfer: insufficient sender issued balance".into(),
            ));
        }
        let _ = limit;
    }

    if tx.recipient != issuer {
        let Some((limit, balance)) = effective_line(tx.recipient)? else {
            return Err(RpcError::Rejected("recipient missing trust line".into()));
        };
        let effective_balance = balance.as_units() as i128 + pending(tx.recipient);
        let available = limit.as_units() as i128 - effective_balance;
        if available < tx.amount.as_units() as i128 {
            return Err(RpcError::Rejected(
                "mempool rejects transfer: recipient limit exceeded".into(),
            ));
        }
    }

    if tx.sender == issuer && tx.recipient != issuer {
        let Some((limit, balance)) = effective_line(tx.recipient)? else {
            return Err(RpcError::Rejected("recipient missing trust line".into()));
        };
        let effective_balance = balance.as_units() as i128 + pending(tx.recipient);
        let available = limit.as_units() as i128 - effective_balance;
        if available < tx.amount.as_units() as i128 {
            return Err(RpcError::Rejected(
                "mempool rejects issue: recipient limit exceeded".into(),
            ));
        }
    }

    Ok(())
}

pub(crate) fn revalidate_trust_line_mempool(store: &StateStore, pool: &mut Mempool) {
    let pending = pool.pending_issued_transfer_txs();
    let stale: Vec<Hash> = pending
        .iter()
        .filter_map(|tx| {
            validate_issued_transfer_mempool_overlay(store, pool, tx)
                .err()
                .map(|_| tx.issued_transfer_tx_id())
        })
        .collect();
    for id in stale {
        let _ = pool.remove_drc_issued_transfer(&id);
    }
}

pub(crate) fn admit_drc_trust_line_set(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcTrustLineSetTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.holder) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    let asset_key = tx.asset_id().asset_key();
    if pool.trust_line_mutation_reserved(&tx.holder, &asset_key) {
        return Err(RpcError::Rejected(
            "trust line already has a pending set or delete".into(),
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
    apply_drc_trust_line_set(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC trust line set: {error}")))?;
    let asset = tx.asset_id();
    let existing = load_drc_trust_line_live(store, &tx.holder, &asset)
        .map_err(|e| RpcError::Rejected(e.to_string()))?;
    let pending_delete = tx.limit.is_zero();
    let pending_create = !pending_delete && existing.is_none();
    pool.admit_drc_trust_line_set(tx, pending_create, pending_delete)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_issued_transfer(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcIssuedTransferTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.sender) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    let asset_key = tx.asset_id().asset_key();
    if (tx.sender == tx.asset_id().issuer || tx.recipient == tx.asset_id().issuer)
        && pool.issuer_liability_mutation_reserved(&asset_key)
    {
        return Err(RpcError::Rejected(
            "issuer liability already has a pending issue or redeem".into(),
        ));
    }
    validate_issued_transfer_mempool_overlay(store, &pool, &tx)?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_issued_transfer(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC issued transfer: {error}")))?;
    pool.admit_drc_issued_transfer(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}
