//! Public mempool admission helpers for DRC payment channel lanes.

use std::sync::Mutex;

use agora_p2p::Mempool;
use agora_rpc::RpcError;
use agora_state_machine::{
    apply_drc_payment_channel_claim, apply_drc_payment_channel_close,
    apply_drc_payment_channel_create, apply_drc_payment_channel_fund, AccountJournal, StateStore,
    TxAuthContext, WriteBatch,
};
use agora_types::{
    DrcPaymentChannelClaimTx, DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx,
    DrcPaymentChannelFundTx, Hash, NativeAssetId,
};

use crate::backend::min_relay_fee;

pub(crate) fn admit_drc_payment_channel_create(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcPaymentChannelCreateTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.owner) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
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
    apply_drc_payment_channel_create(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC payment channel create: {error}")))?;
    pool.admit_drc_payment_channel_create(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_payment_channel_fund(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcPaymentChannelFundTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.submitter) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    if pool.pending_payment_channel_create(&tx.channel_id) {
        return Err(RpcError::Rejected(
            "mempool rejects payment channel fund while create is pending".into(),
        ));
    }
    if pool.payment_channel_mutation_reserved(&tx.channel_id) {
        return Err(RpcError::Rejected(
            "payment channel already has a pending fund, claim, or close".into(),
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
    apply_drc_payment_channel_fund(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC payment channel fund: {error}")))?;
    pool.admit_drc_payment_channel_fund(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_payment_channel_claim(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcPaymentChannelClaimTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.submitter) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    if pool.pending_payment_channel_create(&tx.channel_id) {
        return Err(RpcError::Rejected(
            "mempool rejects payment channel claim while create is pending".into(),
        ));
    }
    if pool.payment_channel_mutation_reserved(&tx.channel_id) {
        return Err(RpcError::Rejected(
            "payment channel already has a pending fund, claim, or close".into(),
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
    apply_drc_payment_channel_claim(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC payment channel claim: {error}")))?;
    pool.admit_drc_payment_channel_claim(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_payment_channel_close(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcPaymentChannelCloseTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.submitter) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    if pool.pending_payment_channel_create(&tx.channel_id) {
        return Err(RpcError::Rejected(
            "mempool rejects payment channel close while create is pending".into(),
        ));
    }
    if pool.payment_channel_mutation_reserved(&tx.channel_id) {
        return Err(RpcError::Rejected(
            "payment channel already has a pending fund, claim, or close".into(),
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
    apply_drc_payment_channel_close(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC payment channel close: {error}")))?;
    pool.admit_drc_payment_channel_close(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}
