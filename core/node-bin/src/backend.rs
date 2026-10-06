//! Live [`RpcBackend`] backed by chain admission + mempool.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use agora_consensus::PowAlgorithm;
use agora_governance::{
    civic_overview_json, list_proposals_json, list_topics_json, office_json, proposal_json,
    ProposalKind, TopicCategory, VoteChoice,
};
use agora_p2p::{
    Mempool, NetworkHandle, NetworkMessage, DEFAULT_MIN_RELAY_FEE, DEFAULT_TEMPLATE_TX_LIMIT,
};
use agora_rpc::{
    DrcDepositPreauthStatus, FeeEstimate, MempoolEntry, NodeInfo, RpcBackend, RpcError, TxLookup,
    UtxoEntry,
};
use agora_state_machine::{
    apply_account_transfer, apply_drc_account_policy, apply_drc_check_cancel, apply_drc_check_cash,
    apply_drc_check_create, apply_drc_deposit_preauth, apply_drc_escrow_cancel,
    apply_drc_escrow_create, apply_drc_escrow_finish, apply_drc_payment_at_blue_score,
    apply_drc_regular_key, apply_drc_signer_list, apply_drc_ticket_create, apply_ovl_execution,
    apply_signed_stake_tx, build_snapshot, canonical_community_root, governance_treasury_root,
    list_drc_account_objects, list_grants as list_canonical_grants,
    list_hubs as list_canonical_hubs, list_missions as list_canonical_missions,
    list_passport_attestations, load_canonical_community_summary, load_canonical_governance_policy,
    load_drc_account_policy, load_drc_check_receipt, load_drc_deposit_preauth,
    load_drc_escrow_receipt, load_drc_issued_asset_policy_receipt,
    load_drc_issued_clawback_receipt, load_drc_issued_transfer_receipt, load_drc_ledger_object,
    load_drc_operation, load_drc_payment_by_invoice, load_drc_payment_channel_claim_event,
    load_drc_payment_channel_fund_event, load_drc_payment_channel_live,
    load_drc_payment_channel_receipt, load_drc_payment_channel_schedule_event,
    load_drc_payment_receipt, load_drc_transaction, load_drc_trust_line_issuer_control_receipt,
    load_drc_trust_line_live, load_epoch, load_known_drc_account_keys,
    load_known_drc_account_policy, load_known_drc_account_signer_summary,
    load_known_drc_deposit_authorization, load_native_supply_state, load_protocol_treasuries,
    load_reward_pool, load_validator, lookup_drc_check_point, lookup_drc_escrow_point,
    lookup_drc_issuer_liability_point, lookup_drc_payment_channel_point, lookup_drc_ticket_point,
    lookup_drc_trust_line_point, lookup_tx_location, meta_keys, outpoint_key,
    plan_drc_mempool_reservation, validate_mempool_tx_with_auth, AccountJournal, ColumnFamily,
    DrcMempoolReservation, DrcTicketPointStatus, StakingParams, StateStore, TxAuthContext,
    WriteBatch,
};
use agora_types::{
    AccountTransfer, Address, Amount, Block, CheckpointAttestation, DrcAcceptedOperationReceipt,
    DrcAccountPolicy, DrcAccountPolicyTx, DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx,
    DrcDepositPreauthTx, DrcEscrowCancelTx, DrcEscrowCreateTx, DrcEscrowFinishTx,
    DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx, DrcIssuedTransferTx, DrcLedgerObjectDescriptor,
    DrcLedgerObjectKind, DrcLedgerObjectPage, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseTx,
    DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, DrcPaymentReceipt, DrcPaymentTx,
    DrcRegularKeyTx, DrcSignerListTx, DrcTicketCreateTx, DrcTrustLineIssuerControlTx,
    DrcTrustLineSetTx, Hash, NativeAssetId, OutPoint, OvlExecutionTx, SignedStakeTx, Transaction,
    TxOut, ACCOUNT_TRANSFER_DRC_TICKET_VERSION, DRC_ACCOUNT_POLICY_TICKET_TX_VERSION,
    DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION, DRC_PAYMENT_TICKET_VERSION,
    DRC_REGULAR_KEY_TICKET_TX_VERSION, DRC_SIGNER_LIST_TICKET_TX_VERSION, STAKE_TX_TICKET_VERSION,
};
use borsh::BorshDeserialize;
use serde_json::{json, Value};

use crate::admit::{BlockTemplateLanes, ChainState};
use crate::civic::{load_civic, save_civic};

pub(crate) fn min_relay_fee() -> u64 {
    std::env::var("AGORA_MIN_RELAY_FEE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MIN_RELAY_FEE)
}

fn map_drc_index_error(error: agora_state_machine::StateError) -> RpcError {
    match error {
        agora_state_machine::StateError::InvalidTx(message) => RpcError::InvalidParams(message),
        other => RpcError::Internal(other.to_string()),
    }
}

pub(crate) fn currency_hex(code: &agora_types::IssuedCurrencyCode) -> String {
    code.0.iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_stake_asset(asset: &str) -> Result<NativeAssetId, RpcError> {
    match asset.trim().to_ascii_uppercase().as_str() {
        "OVL" | "OVOLOS" => Ok(NativeAssetId::OVL),
        "DRC" | "DRACHMA" => Ok(NativeAssetId::DRC),
        other => Err(RpcError::InvalidParams(format!(
            "staking asset must be OVL or DRC, got {other}"
        ))),
    }
}

fn parse_native_asset(asset: &str) -> Result<NativeAssetId, RpcError> {
    match asset.trim().to_ascii_uppercase().as_str() {
        "TLT" | "TALANTON" => Ok(NativeAssetId::TLT),
        "OVL" | "OVOLOS" => Ok(NativeAssetId::OVL),
        "DRC" | "DRACHMA" => Ok(NativeAssetId::DRC),
        other => Err(RpcError::InvalidParams(format!(
            "native asset must be TLT, OVL, or DRC, got {other}"
        ))),
    }
}

fn assert_drc_mempool_slot(
    pool: &Mempool,
    owner: &Address,
    reservation: DrcMempoolReservation,
) -> Result<(), RpcError> {
    match reservation {
        DrcMempoolReservation::AccountNonce => {
            if pool.account_reserved(NativeAssetId::DRC, owner) {
                return Err(RpcError::Rejected(
                    "DRC account already has a pending nonce".into(),
                ));
            }
        }
        DrcMempoolReservation::Ticket { sequence } => {
            if pool.ticket_consumer_reserved(owner, sequence) {
                return Err(RpcError::Rejected(format!(
                    "DRC ticket {sequence} already has a pending consumer"
                )));
            }
        }
    }
    Ok(())
}

/// UTXO + network-bound signature + mempool reservation checks, then admit.
pub(crate) fn admit_transaction(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: Transaction,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    let fee = validate_mempool_tx_with_auth(store, &tx, pool.reserved(), Some(auth))
        .map_err(|e| RpcError::Rejected(format!("utxo: {e}")))?;
    let min_fee = min_relay_fee();
    if fee < min_fee {
        return Err(RpcError::Rejected(format!(
            "fee too low: {fee} < min relay {min_fee}"
        )));
    }
    // Auth already verified; mempool only tracks reservations / fee market.
    pool.admit_priced(tx, fee)
        .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Validate and reserve an OVL/DRC account transfer under the mempool lock.
pub(crate) fn admit_account_transfer(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: AccountTransfer,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if tx.asset == NativeAssetId::DRC {
        let reservation = plan_drc_mempool_reservation(
            store,
            &tx.from,
            tx.version,
            ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|error| RpcError::Rejected(error.to_string()))?;
        assert_drc_mempool_slot(&pool, &tx.from, reservation)?;
    } else if pool.account_reserved(tx.asset, &tx.from) {
        return Err(RpcError::Rejected(
            "account already has a pending nonce".into(),
        ));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_account_transfer(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|e| RpcError::Rejected(format!("account: {e}")))?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    pool.admit_account(tx)
        .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Validate and reserve a signed stake operation under the shared account nonce.
pub(crate) fn admit_stake_tx(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: SignedStakeTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if tx.asset == NativeAssetId::DRC {
        let reservation = plan_drc_mempool_reservation(
            store,
            &tx.actor,
            tx.version,
            STAKE_TX_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|error| RpcError::Rejected(error.to_string()))?;
        assert_drc_mempool_slot(&pool, &tx.actor, reservation)?;
    } else if pool.account_reserved(tx.asset, &tx.actor) {
        return Err(RpcError::Rejected(
            "account already has a pending nonce".into(),
        ));
    }
    let params = match tx.asset {
        NativeAssetId::OVL => StakingParams::ovl_default(),
        NativeAssetId::DRC => StakingParams::drc_default(),
        NativeAssetId::TLT => return Err(RpcError::Rejected("TLT cannot be staked".into())),
    };
    let mut batch = WriteBatch::new();
    apply_signed_stake_tx(store, &mut batch, &tx, auth, &params)
        .map_err(|e| RpcError::Rejected(format!("stake: {e}")))?;
    pool.admit_stake(tx)
        .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Validate and reserve a signed OVL execution envelope.
pub(crate) fn admit_ovl_execution(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: OvlExecutionTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::OVL, &tx.from) {
        return Err(RpcError::Rejected(
            "OVL account already has a pending nonce".into(),
        ));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_ovl_execution(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|e| RpcError::Rejected(format!("OVL execution: {e}")))?;
    pool.admit_ovl_execution(tx)
        .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Validate and reserve a signed native DRC payment.
pub(crate) fn admit_drc_payment(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcPaymentTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    let reservation = plan_drc_mempool_reservation(
        store,
        &tx.from,
        tx.version,
        DRC_PAYMENT_TICKET_VERSION,
        tx.nonce,
        tx.account_sequence,
    )
    .map_err(|error| RpcError::Rejected(error.to_string()))?;
    assert_drc_mempool_slot(&pool, &tx.from, reservation)?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_payment_at_blue_score(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|e| RpcError::Rejected(format!("DRC payment: {e}")))?;
    let canonical_policy = load_drc_account_policy(store, &tx.to)
        .map_err(|error| RpcError::Internal(error.to_string()))?;
    let canonical_preauthorized = tx.from == tx.to
        || load_drc_deposit_preauth(store, &tx.to, &tx.from)
            .map_err(|error| RpcError::Internal(error.to_string()))?;
    pool.admit_payment_with_deposit_auth_at_blue_score(
        tx,
        application_blue_score,
        canonical_policy.deposit_auth_required,
        canonical_preauthorized,
    )
    .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Validate and reserve an owner-authorized DRC account-policy operation.
pub(crate) fn admit_drc_account_policy(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcAccountPolicyTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    let reservation = plan_drc_mempool_reservation(
        store,
        &tx.account,
        tx.version,
        DRC_ACCOUNT_POLICY_TICKET_TX_VERSION,
        tx.nonce,
        tx.account_sequence,
    )
    .map_err(|error| RpcError::Rejected(error.to_string()))?;
    assert_drc_mempool_slot(&pool, &tx.account, reservation)?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_account_policy(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|error| RpcError::Rejected(format!("DRC account policy: {error}")))?;
    pool.admit_drc_policy(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

/// Validate and reserve an owner-authorized DRC deposit preauthorization.
pub(crate) fn admit_drc_deposit_preauth(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcDepositPreauthTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    let reservation = plan_drc_mempool_reservation(
        store,
        &tx.owner,
        tx.version,
        DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION,
        tx.nonce,
        tx.account_sequence,
    )
    .map_err(|error| RpcError::Rejected(error.to_string()))?;
    assert_drc_mempool_slot(&pool, &tx.owner, reservation)?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_deposit_preauth(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|error| RpcError::Rejected(format!("DRC deposit preauthorization: {error}")))?;
    pool.admit_drc_deposit_preauth(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

/// Validate and reserve an owner-authorized DRC regular-key operation.
pub(crate) fn admit_drc_regular_key(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcRegularKeyTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    let reservation = plan_drc_mempool_reservation(
        store,
        &tx.owner,
        tx.version,
        DRC_REGULAR_KEY_TICKET_TX_VERSION,
        tx.nonce,
        tx.account_sequence,
    )
    .map_err(|error| RpcError::Rejected(error.to_string()))?;
    assert_drc_mempool_slot(&pool, &tx.owner, reservation)?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_regular_key(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|error| RpcError::Rejected(format!("DRC regular key: {error}")))?;
    pool.admit_drc_regular_key(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_signer_list(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcSignerListTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    let reservation = plan_drc_mempool_reservation(
        store,
        &tx.owner,
        tx.version,
        DRC_SIGNER_LIST_TICKET_TX_VERSION,
        tx.nonce,
        tx.account_sequence,
    )
    .map_err(|error| RpcError::Rejected(error.to_string()))?;
    assert_drc_mempool_slot(&pool, &tx.owner, reservation)?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_signer_list(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|error| RpcError::Rejected(format!("DRC signer list: {error}")))?;
    pool.admit_drc_signer_list(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_ticket_create(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcTicketCreateTx,
    auth: &TxAuthContext,
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
    let ticket_sequence = tx
        .nonce
        .checked_add(1)
        .ok_or_else(|| RpcError::Rejected("DRC ticket sequence overflow".into()))?;
    if pool.ticket_consumer_reserved(&tx.owner, ticket_sequence) {
        return Err(RpcError::Rejected(format!(
            "DRC ticket {ticket_sequence} already reserved"
        )));
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
    apply_drc_ticket_create(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|error| RpcError::Rejected(format!("DRC ticket create: {error}")))?;
    pool.admit_drc_ticket_create(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_escrow_create(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcEscrowCreateTx,
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
    apply_drc_escrow_create(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC escrow create: {error}")))?;
    pool.admit_drc_escrow_create(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_escrow_finish(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcEscrowFinishTx,
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
    if pool.pending_escrow_create(&tx.escrow_id) {
        return Err(RpcError::Rejected(
            "mempool rejects escrow finish while create is pending".into(),
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
    apply_drc_escrow_finish(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC escrow finish: {error}")))?;
    pool.admit_drc_escrow_finish(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_escrow_cancel(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcEscrowCancelTx,
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
    if pool.pending_escrow_create(&tx.escrow_id) {
        return Err(RpcError::Rejected(
            "mempool rejects escrow cancel while create is pending".into(),
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
    apply_drc_escrow_cancel(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC escrow cancel: {error}")))?;
    pool.admit_drc_escrow_cancel(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_check_create(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcCheckCreateTx,
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
    apply_drc_check_create(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC check create: {error}")))?;
    pool.admit_drc_check_create(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_check_cash(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcCheckCashTx,
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
    if pool.pending_check_create(&tx.check_id) {
        return Err(RpcError::Rejected(
            "mempool rejects check cash while create is pending".into(),
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
    apply_drc_check_cash(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC check cash: {error}")))?;
    pool.admit_drc_check_cash(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_check_cancel(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcCheckCancelTx,
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
    if pool.pending_check_create(&tx.check_id) {
        return Err(RpcError::Rejected(
            "mempool rejects check cancel while create is pending".into(),
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
    apply_drc_check_cancel(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC check cancel: {error}")))?;
    pool.admit_drc_check_cancel(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

#[path = "payment_channel_admit.rs"]
mod payment_channel_admit;
pub(crate) use payment_channel_admit::{
    admit_drc_payment_channel_claim, admit_drc_payment_channel_close,
    admit_drc_payment_channel_create, admit_drc_payment_channel_fund,
};
#[path = "trust_line_admit.rs"]
mod trust_line_admit;
pub(crate) use trust_line_admit::{
    admit_drc_issued_transfer, admit_drc_trust_line_set, revalidate_trust_line_mempool,
};
#[path = "drc_offer_admit.rs"]
mod drc_offer_admit;
#[path = "issued_controls_admit.rs"]
mod issued_controls_admit;
pub(crate) use drc_offer_admit::{
    admit_drc_offer_cancel, admit_drc_offer_create, drc_offer_json, revalidate_drc_offer_mempool,
};
pub(crate) use issued_controls_admit::{
    admit_drc_issued_asset_policy_set, admit_drc_issued_clawback,
    admit_drc_trust_line_issuer_control, get_drc_issued_asset_policy_json,
    revalidate_issued_controls_mempool,
};

/// Node RPC surface: tips/blocks from store, signed tx → mempool + gossip.
pub struct NodeBackend {
    chain: Arc<Mutex<ChainState>>,
    store: Arc<StateStore>,
    mempool: Arc<Mutex<Mempool>>,
    net: Option<NetworkHandle>,
    /// When true, `agora_fundAddress` mints spendable `cf_utxo` credits (testnet).
    allow_fund: bool,
    /// Monotonic nonce so faucet mints never collide on outpoint keys.
    fund_nonce: u64,
    /// Coinbase payout address for `agora_getBlockTemplate`.
    miner_address: Address,
    /// Live connected-peer count (updated from the p2p event loop).
    connected_peers: Arc<AtomicU32>,
    /// `AGORA_NETWORK` label (`dev` / `testnet` / …).
    network: String,
    /// Block 0 id for this datadir.
    genesis_hash: Hash,
}

pub struct NodeBackendConfig {
    pub net: Option<NetworkHandle>,
    pub allow_fund: bool,
    pub miner_address: Address,
    pub connected_peers: Arc<AtomicU32>,
    pub network: String,
    pub genesis_hash: Hash,
}

impl NodeBackend {
    pub fn new(
        chain: Arc<Mutex<ChainState>>,
        store: Arc<StateStore>,
        mempool: Arc<Mutex<Mempool>>,
        config: NodeBackendConfig,
    ) -> Self {
        Self {
            chain,
            store,
            mempool,
            net: config.net,
            allow_fund: config.allow_fund,
            fund_nonce: 0,
            miner_address: config.miner_address,
            connected_peers: config.connected_peers,
            network: config.network,
            genesis_hash: config.genesis_hash,
        }
    }

    fn tx_auth(&self) -> TxAuthContext {
        let chain_id = match self.network.to_ascii_lowercase().as_str() {
            "mainnet" => "agora-mainnet-1",
            "testnet" => "agora-testnet-1",
            _ => "agora-dev",
        };
        TxAuthContext {
            chain_id: chain_id.into(),
            genesis: self.genesis_hash,
            data_availability_network_fingerprint: None,
        }
    }

    fn utxo_balance(&self, address: &Address) -> Result<Amount, RpcError> {
        let mut total = Amount::ZERO;
        for entry in self.list_utxos(address)? {
            total = total
                .checked_add(entry.value)
                .ok_or_else(|| RpcError::Internal("balance overflow".into()))?;
        }
        Ok(total)
    }

    fn list_utxos(&self, address: &Address) -> Result<Vec<UtxoEntry>, RpcError> {
        let mut out = Vec::new();
        self.store
            .for_each_cf(ColumnFamily::Utxo, |key, value| {
                if key.len() != 36 {
                    return Ok(());
                }
                let tx_out = TxOut::try_from_slice(value)
                    .map_err(|e| agora_state_machine::StateError::Storage(e.to_string()))?;
                if &tx_out.address != address {
                    return Ok(());
                }
                let mut tx_bytes = [0u8; 32];
                tx_bytes.copy_from_slice(&key[..32]);
                let index = u32::from_le_bytes(key[32..36].try_into().unwrap());
                out.push(UtxoEntry {
                    outpoint: OutPoint {
                        tx_id: Hash(tx_bytes),
                        index,
                    },
                    value: tx_out.value,
                });
                Ok(())
            })
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(out)
    }

    fn issued_supply(&self) -> u64 {
        self.store
            .get_cf(ColumnFamily::Meta, meta_keys::ISSUED_SUPPLY)
            .ok()
            .flatten()
            .and_then(|b| {
                if b.len() == 8 {
                    Some(u64::from_le_bytes(b.try_into().ok()?))
                } else {
                    None
                }
            })
            .unwrap_or(10_000)
            .max(1)
    }

    fn with_civic<R>(
        &self,
        f: impl FnOnce(&agora_governance::CivicSnapshot) -> Result<R, RpcError>,
    ) -> Result<R, RpcError> {
        let eligible = self.issued_supply();
        let mut snap = load_civic(self.store.as_ref(), eligible)?;
        snap.governance.ecclesia_eligible_power = eligible;
        f(&snap)
    }

    fn with_civic_mut<R>(
        &self,
        f: impl FnOnce(&mut agora_governance::CivicSnapshot) -> Result<R, RpcError>,
    ) -> Result<R, RpcError> {
        let eligible = self.issued_supply();
        let mut snap = load_civic(self.store.as_ref(), eligible)?;
        snap.governance.ecclesia_eligible_power = eligible;
        let out = f(&mut snap)?;
        save_civic(self.store.as_ref(), &snap)?;
        Ok(out)
    }
}

impl RpcBackend for NodeBackend {
    fn dag_tips(&self) -> Vec<Hash> {
        self.chain
            .lock()
            .ok()
            .and_then(|g| g.tips().ok())
            .unwrap_or_default()
    }

    fn get_block(&self, hash: &Hash) -> Option<Block> {
        self.chain
            .lock()
            .ok()
            .and_then(|g| g.load_block(hash).ok())
            .flatten()
    }

    fn get_transaction(&self, tx_id: &Hash) -> Result<TxLookup, RpcError> {
        {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            if let Some(tx) = pool.get(tx_id) {
                return Ok(TxLookup::pending(tx.clone(), pool.fee_of(tx_id)));
            }
        }
        let Some((block_id, index)) = lookup_tx_location(self.store.as_ref(), tx_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        else {
            return Ok(TxLookup::unknown(*tx_id));
        };
        let block = self.get_block(&block_id);
        let Some(block) = block else {
            return Ok(TxLookup::unknown(*tx_id));
        };
        let Some(tx) = block.transactions.get(index as usize) else {
            return Ok(TxLookup::unknown(*tx_id));
        };
        let acceptance =
            agora_state_machine::tx_acceptance_status(self.store.as_ref(), &block_id, index)
                .ok()
                .flatten();
        match self
            .chain
            .lock()
            .ok()
            .and_then(|g| g.confirmations(&block_id))
        {
            Some(confirmations) => {
                // Explicit acceptance wins over block color. Missing record =
                // legacy pre-acceptance blocks (treat as confirmed when blue).
                if let Some(status) = acceptance {
                    if !status.is_accepted() {
                        return Ok(TxLookup::orphaned(tx.clone(), block_id, index)
                            .with_acceptance(status.as_str()));
                    }
                    return Ok(
                        TxLookup::confirmed(tx.clone(), block_id, index, confirmations)
                            .with_acceptance(status.as_str()),
                    );
                }
                Ok(TxLookup::confirmed(
                    tx.clone(),
                    block_id,
                    index,
                    confirmations,
                ))
            }
            None => {
                let mut lookup = TxLookup::orphaned(tx.clone(), block_id, index);
                if let Some(status) = acceptance {
                    lookup.acceptance = Some(status.as_str().into());
                }
                Ok(lookup)
            }
        }
    }

    fn get_mempool(&self, limit: usize) -> Result<Vec<MempoolEntry>, RpcError> {
        let pool = self
            .mempool
            .lock()
            .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
        Ok(pool
            .pending_entries(limit)
            .into_iter()
            .map(|(tx, fee)| MempoolEntry {
                tx_id: tx.tx_id(),
                fee: Some(fee),
                transaction: tx,
            })
            .collect())
    }

    fn get_node_info(&self) -> Result<NodeInfo, RpcError> {
        let chain = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?;
        let tips = chain.tips().unwrap_or_default();
        let storage = chain.storage_policy();
        let bits = chain.difficulty().as_bits();
        let pow = match chain.pow_algorithm() {
            PowAlgorithm::RandomX => "randomx",
            PowAlgorithm::KHeavyHash => "kheavyhash",
        };
        let mempool_count = self.mempool.lock().map(|p| p.len()).unwrap_or(0);
        Ok(NodeInfo {
            network: self.network.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            peer_id: self.net.as_ref().map(|n| n.peer_id().to_string()),
            connected_peers: Some(self.connected_peers.load(Ordering::Relaxed)),
            tip_count: tips.len(),
            mempool_count,
            pow_algorithm: pow.into(),
            bits,
            archival: storage.archival,
            hot_window: storage.hot_window,
            allow_fund: self.allow_fund,
            miner_address: Some(self.miner_address.to_bech32()),
            genesis_hash: Some(self.genesis_hash.to_hex()),
            chain_id: Some(self.tx_auth().chain_id),
            min_relay_fee: min_relay_fee(),
        })
    }

    fn estimate_fee(&self) -> Result<FeeEstimate, RpcError> {
        let min = min_relay_fee();
        let pool = self
            .mempool
            .lock()
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        // Bitcoin-class guidance: max(min_relay, mempool median) + mild congestion premium.
        let median = pool.median_fee().unwrap_or(min);
        let congestion = (pool.len() as u64).saturating_mul(100);
        let suggested = median.max(min).saturating_add(congestion);
        Ok(FeeEstimate {
            min_relay_fee: min,
            suggested_fee: suggested,
        })
    }

    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_transaction(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::Transaction(tx)) {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn submit_account_transfer(&mut self, tx: AccountTransfer) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_account_transfer(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::AccountTransfer(tx))
                .map_err(|e| RpcError::Internal(e.to_string()))?;
        }
        Ok(id)
    }

    fn submit_ovl_execution(&mut self, tx: OvlExecutionTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_ovl_execution(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::OvlExecution(tx))
                .map_err(|e| RpcError::Internal(e.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_payment(&mut self, tx: DrcPaymentTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let application_blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .virtual_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_payment(
            &self.store,
            &self.mempool,
            tx.clone(),
            &auth,
            application_blue_score,
        )?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcPayment(tx))
                .map_err(|e| RpcError::Internal(e.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_account_policy(&mut self, tx: DrcAccountPolicyTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_drc_account_policy(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcAccountPolicy(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_deposit_preauth(&mut self, tx: DrcDepositPreauthTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_drc_deposit_preauth(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcDepositPreauth(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_regular_key(&mut self, tx: DrcRegularKeyTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_drc_regular_key(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcRegularKey(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_signer_list(&mut self, tx: DrcSignerListTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_drc_signer_list(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcSignerList(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_ticket_create(&mut self, tx: DrcTicketCreateTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_drc_ticket_create(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcTicketCreate(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn get_drc_ticket(&self, owner: &Address, ticket_sequence: u64) -> Result<Value, RpcError> {
        let status = lookup_drc_ticket_point(&self.store, owner, ticket_sequence)
            .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
        Ok(match status {
            DrcTicketPointStatus::Live => json!({
                "owner": owner.to_bech32(),
                "ticket_sequence": ticket_sequence,
                "status": "live",
            }),
            DrcTicketPointStatus::Unknown => json!({
                "owner": owner.to_bech32(),
                "ticket_sequence": ticket_sequence,
                "status": "unknown",
            }),
        })
    }

    fn submit_drc_escrow_create(&mut self, tx: DrcEscrowCreateTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id =
            admit_drc_escrow_create(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcEscrowCreate(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_escrow_finish(&mut self, tx: DrcEscrowFinishTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id =
            admit_drc_escrow_finish(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcEscrowFinish(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_escrow_cancel(&mut self, tx: DrcEscrowCancelTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id =
            admit_drc_escrow_cancel(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcEscrowCancel(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn get_drc_escrow(&self, escrow_id: &Hash) -> Result<Value, RpcError> {
        let status = lookup_drc_escrow_point(self.store.as_ref(), escrow_id)
            .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
        Ok(json!({
            "escrow_id": escrow_id.to_hex(),
            "status": status,
        }))
    }

    fn get_drc_escrow_receipt(&self, escrow_id: &Hash) -> Result<Value, RpcError> {
        match load_drc_escrow_receipt(self.store.as_ref(), escrow_id)
            .map_err(|error| RpcError::Internal(error.to_string()))?
        {
            Some(receipt) => Ok(json!({
                "escrow_id": escrow_id.to_hex(),
                "status": "known",
                "outcome": match receipt.outcome {
                    agora_types::DrcEscrowOutcome::Finished => "finished",
                    agora_types::DrcEscrowOutcome::Cancelled => "cancelled",
                },
                "settlement_blue_score": receipt.settlement_blue_score,
                "settlement_tx_id": receipt.settlement_tx_id.to_hex(),
            })),
            None => Ok(json!({
                "escrow_id": escrow_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn submit_drc_check_create(&mut self, tx: DrcCheckCreateTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_check_create(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcCheckCreate(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_check_cash(&mut self, tx: DrcCheckCashTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_check_cash(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcCheckCash(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_check_cancel(&mut self, tx: DrcCheckCancelTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_check_cancel(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcCheckCancel(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn get_drc_check(&self, check_id: &Hash) -> Result<Value, RpcError> {
        let status = lookup_drc_check_point(self.store.as_ref(), check_id)
            .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
        Ok(json!({
            "check_id": check_id.to_hex(),
            "status": status,
        }))
    }

    fn get_drc_check_receipt(&self, check_id: &Hash) -> Result<Value, RpcError> {
        match load_drc_check_receipt(self.store.as_ref(), check_id)
            .map_err(|error| RpcError::Internal(error.to_string()))?
        {
            Some(receipt) => Ok(json!({
                "check_id": check_id.to_hex(),
                "status": "known",
                "outcome": match receipt.outcome {
                    agora_types::DrcCheckOutcome::Cashed => "cashed",
                    agora_types::DrcCheckOutcome::Cancelled => "cancelled",
                },
                "settlement_blue_score": receipt.settlement_blue_score,
                "settlement_tx_id": receipt.settlement_tx_id.to_hex(),
            })),
            None => Ok(json!({
                "check_id": check_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn submit_drc_payment_channel_create(
        &mut self,
        tx: DrcPaymentChannelCreateTx,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_payment_channel_create(
            &self.store,
            &self.mempool,
            tx.clone(),
            &auth,
            blue_score,
        )?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcPaymentChannelCreate(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_payment_channel_fund(
        &mut self,
        tx: DrcPaymentChannelFundTx,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_payment_channel_fund(
            &self.store,
            &self.mempool,
            tx.clone(),
            &auth,
            blue_score,
        )?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcPaymentChannelFund(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_payment_channel_claim(
        &mut self,
        tx: DrcPaymentChannelClaimTx,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_payment_channel_claim(
            &self.store,
            &self.mempool,
            tx.clone(),
            &auth,
            blue_score,
        )?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcPaymentChannelClaim(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_payment_channel_close(
        &mut self,
        tx: DrcPaymentChannelCloseTx,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_payment_channel_close(
            &self.store,
            &self.mempool,
            tx.clone(),
            &auth,
            blue_score,
        )?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcPaymentChannelClose(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn get_drc_payment_channel(&self, channel_id: &Hash) -> Result<Value, RpcError> {
        let status = lookup_drc_payment_channel_point(self.store.as_ref(), channel_id)
            .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
        Ok(json!({
            "channel_id": channel_id.to_hex(),
            "status": status,
        }))
    }

    fn get_drc_payment_channel_receipt(&self, channel_id: &Hash) -> Result<Value, RpcError> {
        match load_drc_payment_channel_receipt(self.store.as_ref(), channel_id)
            .map_err(|error| RpcError::Internal(error.to_string()))?
        {
            Some(receipt) => Ok(json!({
                "channel_id": channel_id.to_hex(),
                "status": "known",
                "outcome": "closed",
                "settlement_blue_score": receipt.settlement_blue_score,
                "settlement_tx_id": receipt.settlement_tx_id.to_hex(),
            })),
            None => Ok(json!({
                "channel_id": channel_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn get_drc_payment_channel_fund_event(&self, fund_tx_id: &Hash) -> Result<Value, RpcError> {
        match load_drc_payment_channel_fund_event(self.store.as_ref(), fund_tx_id)
            .map_err(|error| RpcError::Internal(error.to_string()))?
        {
            Some(ev) => Ok(json!({
                "fund_tx_id": fund_tx_id.to_hex(),
                "status": "known",
                "channel_id": ev.channel_id.to_hex(),
                "application_blue_score": ev.application_blue_score,
            })),
            None => Ok(json!({
                "fund_tx_id": fund_tx_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn get_drc_payment_channel_claim_event(&self, claim_tx_id: &Hash) -> Result<Value, RpcError> {
        match load_drc_payment_channel_claim_event(self.store.as_ref(), claim_tx_id)
            .map_err(|error| RpcError::Internal(error.to_string()))?
        {
            Some(ev) => Ok(json!({
                "claim_tx_id": claim_tx_id.to_hex(),
                "status": "known",
                "channel_id": ev.channel_id.to_hex(),
                "application_blue_score": ev.application_blue_score,
            })),
            None => Ok(json!({
                "claim_tx_id": claim_tx_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn get_drc_payment_channel_schedule_event(
        &self,
        close_tx_id: &Hash,
    ) -> Result<Value, RpcError> {
        match load_drc_payment_channel_schedule_event(self.store.as_ref(), close_tx_id)
            .map_err(|error| RpcError::Internal(error.to_string()))?
        {
            Some(ev) => Ok(json!({
                "close_tx_id": close_tx_id.to_hex(),
                "status": "known",
                "channel_id": ev.channel_id.to_hex(),
                "close_finalizable_after": ev.close_finalizable_after,
            })),
            None => Ok(json!({
                "close_tx_id": close_tx_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn verify_drc_payment_channel_claim(
        &self,
        channel_id: &Hash,
        cumulative_authorized: Amount,
        channel_claim_signature: &[u8],
    ) -> Result<Value, RpcError> {
        let live = load_drc_payment_channel_live(self.store.as_ref(), channel_id)
            .map_err(|error| RpcError::Internal(error.to_string()))?
            .ok_or_else(|| RpcError::InvalidParams("unknown payment channel".into()))?;
        let auth = self.tx_auth();
        agora_crypto::verify_payment_channel_offledger_claim(
            &live.claim_public_key,
            channel_claim_signature,
            &auth.chain_id,
            &auth.genesis,
            channel_id,
            cumulative_authorized,
        )
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
        Ok(json!({ "valid": true }))
    }

    fn submit_drc_trust_line_set(&mut self, tx: DrcTrustLineSetTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id =
            admit_drc_trust_line_set(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcTrustLineSet(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_issued_transfer(&mut self, tx: DrcIssuedTransferTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id =
            admit_drc_issued_transfer(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcIssuedTransfer(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn get_drc_trust_line(
        &self,
        holder: &Address,
        asset: &agora_types::IssuedAssetId,
    ) -> Result<Value, RpcError> {
        asset
            .validate()
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        let status = lookup_drc_trust_line_point(self.store.as_ref(), holder, asset)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        if status == "live" {
            let live = load_drc_trust_line_live(self.store.as_ref(), holder, asset)
                .map_err(|e| RpcError::Internal(e.to_string()))?
                .expect("live trust line");
            return Ok(json!({
                "holder": holder.to_hex(),
                "issuer": asset.issuer.to_hex(),
                "currency": currency_hex(&asset.currency),
                "status": "live",
                "limit": live.limit.as_units().to_string(),
                "balance": live.balance.as_units().to_string(),
                "admission_not_finality": true,
            }));
        }
        Ok(json!({
            "holder": holder.to_hex(),
            "issuer": asset.issuer.to_hex(),
            "currency": currency_hex(&asset.currency),
            "status": "unknown",
            "admission_not_finality": true,
        }))
    }

    fn get_drc_issuer_liability(
        &self,
        asset: &agora_types::IssuedAssetId,
    ) -> Result<Value, RpcError> {
        let (status, outstanding) = lookup_drc_issuer_liability_point(self.store.as_ref(), asset)
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        Ok(json!({
            "issuer": asset.issuer.to_hex(),
            "currency": currency_hex(&asset.currency),
            "status": status,
            "outstanding": outstanding.as_units().to_string(),
            "admission_not_finality": true,
        }))
    }

    fn get_drc_issued_transfer_receipt(&self, transfer_tx_id: &Hash) -> Result<Value, RpcError> {
        match load_drc_issued_transfer_receipt(self.store.as_ref(), transfer_tx_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        {
            Some(receipt) => Ok(json!({
                "transfer_tx_id": transfer_tx_id.to_hex(),
                "status": "known",
                "sender": receipt.sender.to_hex(),
                "recipient": receipt.recipient.to_hex(),
                "amount": receipt.amount.as_units().to_string(),
                "settlement_blue_score": receipt.settlement_blue_score,
                "source_tag": receipt.source_tag,
                "destination_tag": receipt.destination_tag,
            })),
            None => Ok(json!({
                "transfer_tx_id": transfer_tx_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn submit_drc_issued_asset_policy_set(
        &mut self,
        tx: DrcIssuedAssetPolicySetTx,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_issued_asset_policy_set(
            &self.store,
            &self.mempool,
            tx.clone(),
            &auth,
            blue_score,
        )?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcIssuedAssetPolicySet(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_trust_line_issuer_control(
        &mut self,
        tx: DrcTrustLineIssuerControlTx,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_trust_line_issuer_control(
            &self.store,
            &self.mempool,
            tx.clone(),
            &auth,
            blue_score,
        )?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcTrustLineIssuerControl(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_issued_clawback(&mut self, tx: DrcIssuedClawbackTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id =
            admit_drc_issued_clawback(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcIssuedClawback(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_offer_create(
        &mut self,
        tx: agora_types::DrcOfferCreateTx,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_offer_create(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcOfferCreate(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_offer_cancel(
        &mut self,
        tx: agora_types::DrcOfferCancelTx,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let id = admit_drc_offer_cancel(&self.store, &self.mempool, tx.clone(), &auth, blue_score)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcOfferCancel(tx))
                .map_err(|error| RpcError::Internal(error.to_string()))?;
        }
        Ok(id)
    }

    fn get_drc_offer(&self, offer_id: &Hash) -> Result<Value, RpcError> {
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .virtual_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        drc_offer_json(self.store.as_ref(), offer_id, blue_score)
    }

    fn get_drc_account_offers(
        &self,
        account: &agora_types::Address,
        cursor: Option<agora_types::DrcOfferCursor>,
        limit: Option<usize>,
    ) -> Result<Value, RpcError> {
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .virtual_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let page = agora_state_machine::list_account_offers(
            self.store.as_ref(),
            account,
            cursor,
            limit.unwrap_or(agora_types::DRC_OFFER_PAGE_MAX),
            blue_score,
        )
        .map_err(|error| RpcError::Rejected(error.to_string()))?;
        serde_json::to_value(page).map_err(|error| RpcError::Internal(error.to_string()))
    }

    fn get_drc_book_offers(
        &self,
        book: &agora_types::DrcOfferBook,
        cursor: Option<agora_types::DrcOfferBookCursor>,
        limit: Option<usize>,
    ) -> Result<Value, RpcError> {
        let blue_score = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .virtual_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let page = agora_state_machine::list_book_offers(
            self.store.as_ref(),
            *book,
            cursor,
            limit.unwrap_or(agora_types::DRC_OFFER_PAGE_MAX),
            blue_score,
        )
        .map_err(|error| RpcError::Rejected(error.to_string()))?;
        let mut value =
            serde_json::to_value(page).map_err(|error| RpcError::Internal(error.to_string()))?;
        if let Some(object) = value.as_object_mut() {
            object.insert("simulated_fill".into(), serde_json::Value::Bool(false));
        }
        Ok(value)
    }

    fn get_drc_issued_asset_policy(
        &self,
        asset: &agora_types::IssuedAssetId,
    ) -> Result<Value, RpcError> {
        get_drc_issued_asset_policy_json(self.store.as_ref(), asset)
    }

    fn get_drc_issued_asset_policy_receipt(
        &self,
        policy_set_tx_id: &Hash,
    ) -> Result<Value, RpcError> {
        match load_drc_issued_asset_policy_receipt(self.store.as_ref(), policy_set_tx_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        {
            Some(receipt) => Ok(json!({
                "policy_set_tx_id": policy_set_tx_id.to_hex(),
                "status": "known",
                "action": format!("{:?}", receipt.action),
                "settlement_blue_score": receipt.settlement_blue_score,
            })),
            None => Ok(json!({
                "policy_set_tx_id": policy_set_tx_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn get_drc_trust_line_issuer_control_receipt(
        &self,
        control_tx_id: &Hash,
    ) -> Result<Value, RpcError> {
        match load_drc_trust_line_issuer_control_receipt(self.store.as_ref(), control_tx_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        {
            Some(receipt) => Ok(json!({
                "control_tx_id": control_tx_id.to_hex(),
                "status": "known",
                "holder": receipt.holder.to_hex(),
                "action": format!("{:?}", receipt.action),
                "settlement_blue_score": receipt.settlement_blue_score,
            })),
            None => Ok(json!({
                "control_tx_id": control_tx_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn get_drc_issued_clawback_receipt(&self, clawback_tx_id: &Hash) -> Result<Value, RpcError> {
        match load_drc_issued_clawback_receipt(self.store.as_ref(), clawback_tx_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        {
            Some(receipt) => Ok(json!({
                "clawback_tx_id": clawback_tx_id.to_hex(),
                "status": "known",
                "holder": receipt.holder.to_hex(),
                "amount": receipt.amount.as_units().to_string(),
                "settlement_blue_score": receipt.settlement_blue_score,
            })),
            None => Ok(json!({
                "clawback_tx_id": clawback_tx_id.to_hex(),
                "status": "unknown",
            })),
        }
    }

    fn get_drc_object(
        &self,
        object_id: &Hash,
    ) -> Result<Option<DrcLedgerObjectDescriptor>, RpcError> {
        load_drc_ledger_object(self.store.as_ref(), object_id).map_err(map_drc_index_error)
    }

    fn get_drc_account_objects(
        &self,
        owner: &Address,
        kind: Option<DrcLedgerObjectKind>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<DrcLedgerObjectPage, RpcError> {
        list_drc_account_objects(self.store.as_ref(), *owner, kind, limit, cursor)
            .map_err(map_drc_index_error)
    }

    fn get_drc_operation(
        &self,
        operation_id: &Hash,
    ) -> Result<Option<DrcAcceptedOperationReceipt>, RpcError> {
        load_drc_operation(self.store.as_ref(), operation_id).map_err(map_drc_index_error)
    }

    fn get_drc_transaction(
        &self,
        transaction_id: &Hash,
    ) -> Result<Option<DrcAcceptedOperationReceipt>, RpcError> {
        load_drc_transaction(self.store.as_ref(), transaction_id).map_err(map_drc_index_error)
    }

    fn get_drc_account_policy(
        &self,
        account: &Address,
    ) -> Result<Option<(DrcAccountPolicy, u64)>, RpcError> {
        load_known_drc_account_policy(self.store.as_ref(), account)
            .map(|known| known.map(|(policy, state)| (policy, state.nonce)))
            .map_err(|error| RpcError::Internal(error.to_string()))
    }

    fn get_drc_deposit_preauth(
        &self,
        owner: &Address,
        authorized_source: &Address,
    ) -> Result<Option<DrcDepositPreauthStatus>, RpcError> {
        load_known_drc_deposit_authorization(self.store.as_ref(), owner, authorized_source)
            .map(|status| {
                status.map(|status| DrcDepositPreauthStatus {
                    preauthorized: status.preauthorized,
                    deposit_auth_required: status.deposit_auth_required,
                    deposit_authorized: status.deposit_authorized,
                })
            })
            .map_err(|error| RpcError::Internal(error.to_string()))
    }

    fn get_drc_account_keys(
        &self,
        account: &Address,
    ) -> Result<Option<(Option<Address>, u64)>, RpcError> {
        load_known_drc_account_keys(self.store.as_ref(), account)
            .map_err(|error| RpcError::Internal(error.to_string()))
    }

    fn get_drc_account_signer_list(
        &self,
        account: &Address,
    ) -> Result<Option<(u32, u32, u64)>, RpcError> {
        load_known_drc_account_signer_summary(self.store.as_ref(), account)
            .map_err(|error| RpcError::Internal(error.to_string()))
    }

    fn get_drc_payment(&self, payment_id: &Hash) -> Result<Option<DrcPaymentReceipt>, RpcError> {
        load_drc_payment_receipt(self.store.as_ref(), payment_id)
            .map_err(|e| RpcError::Internal(e.to_string()))
    }

    fn get_drc_payment_by_invoice(
        &self,
        recipient: &Address,
        invoice_id: &Hash,
    ) -> Result<Option<DrcPaymentReceipt>, RpcError> {
        load_drc_payment_by_invoice(self.store.as_ref(), recipient, invoice_id)
            .map_err(|e| RpcError::Internal(e.to_string()))
    }

    fn get_balance(&self, address: &Address) -> Amount {
        self.utxo_balance(address).unwrap_or(Amount::ZERO)
    }

    fn get_utxos(&self, address: &Address) -> Result<Vec<UtxoEntry>, RpcError> {
        self.list_utxos(address)
    }

    fn fund_address(&mut self, address: Address, amount: Amount) -> Result<Amount, RpcError> {
        if self.network.eq_ignore_ascii_case("mainnet") {
            return Err(RpcError::Rejected(
                "agora_fundAddress is permanently disabled on mainnet".into(),
            ));
        }
        if !self.allow_fund {
            return Err(RpcError::Rejected(
                "agora_fundAddress disabled (set AGORA_RPC_ALLOW_FUND=1 for testnet)".into(),
            ));
        }
        if amount.as_base_units() == 0 {
            return Err(RpcError::InvalidParams("amount must be > 0".into()));
        }
        self.fund_nonce = self.fund_nonce.saturating_add(1);
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        // Synthetic outpoint — testnet mint only; not a consensus coinbase.
        let tx_id = Hash::hash_borsh(&(
            b"agora_fund",
            address,
            amount.as_base_units(),
            self.fund_nonce,
            timestamp_ms,
        ));
        let out = TxOut {
            value: amount,
            address,
        };
        let key = outpoint_key(&OutPoint { tx_id, index: 0 });
        let bytes = borsh::to_vec(&out).map_err(|e| RpcError::Internal(e.to_string()))?;
        self.store
            .put_cf(ColumnFamily::Utxo, &key, &bytes)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        self.utxo_balance(&address)
    }

    fn get_block_template(&self) -> Result<Block, RpcError> {
        let chain = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?;
        let application_blue_score = chain
            .next_template_blue_score()
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let (
            transfers,
            account_transfers,
            stake_ops,
            ovl_executions,
            drc_payments,
            drc_account_policies,
            drc_deposit_preauths,
            drc_regular_keys,
            drc_signer_lists,
            drc_ticket_creates,
            drc_escrow_creates,
            drc_escrow_finishes,
            drc_escrow_cancels,
            drc_check_creates,
            drc_check_cashes,
            drc_check_cancels,
            drc_payment_channel_creates,
            drc_payment_channel_funds,
            drc_payment_channel_claims,
            drc_payment_channel_closes,
            drc_trust_line_sets,
            drc_issued_transfers,
            drc_issued_asset_policy_sets,
            drc_trust_line_issuer_controls,
            drc_issued_clawbacks,
            drc_offer_creates,
            drc_offer_cancels,
        ) = {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            (
                pool.select_transfers(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_account_transfers(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_stake_ops(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_ovl_executions(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_payments_at_blue_score(
                    DEFAULT_TEMPLATE_TX_LIMIT,
                    application_blue_score,
                ),
                pool.select_drc_account_policies(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_deposit_preauths(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_regular_keys(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_signer_lists(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_ticket_creates(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_escrow_creates(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_escrow_finishes(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_escrow_cancels(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_check_creates(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_check_cashes(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_check_cancels(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_payment_channel_creates(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_payment_channel_funds(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_payment_channel_claims(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_payment_channel_closes(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_trust_line_sets(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_issued_transfers(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_issued_asset_policy_sets(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_trust_line_issuer_controls(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_issued_clawbacks(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_offer_creates(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_offer_cancels(DEFAULT_TEMPLATE_TX_LIMIT),
            )
        };
        chain
            .block_template_lanes(
                self.miner_address,
                BlockTemplateLanes {
                    transfers: &transfers,
                    account_transfers: &account_transfers,
                    stake_ops: &stake_ops,
                    ovl_executions: &ovl_executions,
                    drc_payments: &drc_payments,
                    drc_account_policies: &drc_account_policies,
                    drc_deposit_preauths: &drc_deposit_preauths,
                    drc_regular_keys: &drc_regular_keys,
                    drc_signer_lists: &drc_signer_lists,
                    drc_ticket_creates: &drc_ticket_creates,
                    drc_escrow_creates: &drc_escrow_creates,
                    drc_escrow_finishes: &drc_escrow_finishes,
                    drc_escrow_cancels: &drc_escrow_cancels,
                    drc_check_creates: &drc_check_creates,
                    drc_check_cashes: &drc_check_cashes,
                    drc_check_cancels: &drc_check_cancels,
                    drc_payment_channel_creates: &drc_payment_channel_creates,
                    drc_payment_channel_funds: &drc_payment_channel_funds,
                    drc_payment_channel_claims: &drc_payment_channel_claims,
                    drc_payment_channel_closes: &drc_payment_channel_closes,
                    drc_trust_line_sets: &drc_trust_line_sets,
                    drc_issued_transfers: &drc_issued_transfers,
                    drc_issued_asset_policy_sets: &drc_issued_asset_policy_sets,
                    drc_trust_line_issuer_controls: &drc_trust_line_issuer_controls,
                    drc_issued_clawbacks: &drc_issued_clawbacks,
                    drc_offer_creates: &drc_offer_creates,
                    drc_offer_cancels: &drc_offer_cancels,
                    ..BlockTemplateLanes::default()
                },
            )
            .map_err(|e| RpcError::Internal(e.to_string()))
    }

    fn randomx_epoch(&self, parents: &[Hash]) -> u64 {
        self.chain
            .lock()
            .map(|c| c.randomx_epoch_for_parents(parents))
            .unwrap_or(0)
    }

    fn submit_block(&mut self, block: Block) -> Result<Hash, RpcError> {
        let (id, virtual_blue_score) = {
            let mut chain = self
                .chain
                .lock()
                .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?;
            let id = chain.admit_block(block.clone()).map_err(|e| match e {
                crate::admit::AdmitError::InvalidPow => {
                    RpcError::Rejected("invalid proof of work".into())
                }
                crate::admit::AdmitError::Duplicate(h) => {
                    RpcError::Rejected(format!("duplicate block {h}"))
                }
                crate::admit::AdmitError::MissingParent(h) => {
                    RpcError::Rejected(format!("missing parent {}", h.to_hex()))
                }
                crate::admit::AdmitError::Utxo(msg) => RpcError::Rejected(format!("utxo: {msg}")),
                crate::admit::AdmitError::WrongDifficulty { expected, got } => RpcError::Rejected(
                    format!("wrong difficulty: expected bits={expected}, got={got}"),
                ),
                crate::admit::AdmitError::BadTxRoot => {
                    RpcError::Rejected("tx_root mismatch".into())
                }
                crate::admit::AdmitError::FinalityReorg {
                    finalized,
                    abandoned,
                } => RpcError::Rejected(format!(
                    "reorg beyond finality: abandoned {abandoned} <= finalized {finalized}"
                )),
                crate::admit::AdmitError::InvalidAttestation(msg) => {
                    RpcError::Rejected(format!("attestation: {msg}"))
                }
                other => RpcError::Internal(other.to_string()),
            })?;
            let score = chain
                .virtual_blue_score()
                .map_err(|error| RpcError::Internal(error.to_string()))?;
            (id, score)
        };
        if let Ok(mut pool) = self.mempool.lock() {
            pool.evict_for_block_at_blue_score(&block, virtual_blue_score);
            let auth = self.tx_auth();
            revalidate_trust_line_mempool(self.store.as_ref(), &mut pool);
            revalidate_issued_controls_mempool(self.store.as_ref(), &mut pool, &auth);
            revalidate_drc_offer_mempool(self.store.as_ref(), &mut pool, &auth, virtual_blue_score);
        }
        if let Some(net) = &self.net {
            // Prefer compact + announce; peers inflate from mempool or issue GetBlock.
            let _ = net.publish_message(NetworkMessage::compact_from_block(&block));
            let _ = net.publish_message(NetworkMessage::BlockAnnounce { hash: id });
        }
        Ok(id)
    }

    fn get_finality(&self, block_hash: &Hash) -> Result<Value, RpcError> {
        let chain = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?;
        let cert = chain
            .finality_certificate(block_hash)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let finalized_tip = chain
            .finalized_blue_score()
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        match cert {
            Some(c) => Ok(json!({
                "block_hash": c.body.block_hash.to_hex(),
                "blue_score": c.body.blue_score,
                "state": c.state.as_str(),
                "pow_work_met": c.pow_work_met,
                "ovl_signed_stake": c.ovl_signed_stake,
                "ovl_active_stake": c.ovl_active_stake,
                "drc_signed_stake": c.drc_signed_stake,
                "drc_active_stake": c.drc_active_stake,
                "finalized": c.state.is_finalized(),
                "finalized_tip_blue_score": finalized_tip,
            })),
            None => Ok(json!({
                "block_hash": block_hash.to_hex(),
                "state": "Proposed",
                "pow_work_met": false,
                "finalized": false,
                "finalized_tip_blue_score": finalized_tip,
            })),
        }
    }

    fn get_finalized_tip(&self) -> Result<Value, RpcError> {
        let chain = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?;
        let score = chain
            .finalized_blue_score()
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({ "blue_score": score }))
    }

    fn submit_attestation(&mut self, attestation: Value) -> Result<Value, RpcError> {
        let att: CheckpointAttestation = serde_json::from_value(attestation)
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        let cert = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .admit_attestation(att.clone())
            .map_err(|e| match e {
                crate::admit::AdmitError::InvalidAttestation(msg) => {
                    RpcError::Rejected(format!("attestation: {msg}"))
                }
                other => RpcError::Rejected(other.to_string()),
            })?;
        if let Some(net) = &self.net {
            let _ = net.publish_message(NetworkMessage::CheckpointAttestation(att));
        }
        Ok(json!({
            "block_hash": cert.body.block_hash.to_hex(),
            "state": cert.state.as_str(),
            "finalized": cert.state.is_finalized(),
            "ovl_signed_stake": cert.ovl_signed_stake,
            "drc_signed_stake": cert.drc_signed_stake,
        }))
    }

    fn get_validator_set(&self, asset: &str, epoch: Option<u64>) -> Result<Value, RpcError> {
        let asset = parse_stake_asset(asset)?;
        let epoch = match epoch {
            Some(e) => e,
            None => load_epoch(self.store.as_ref(), asset)
                .map_err(|e| RpcError::Internal(e.to_string()))?,
        };
        let snap = build_snapshot(self.store.as_ref(), asset, epoch)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "asset": asset.ticker(),
            "epoch": snap.epoch,
            "total_active_stake": snap.total_active_stake,
            "commitment": snap.commitment().to_hex(),
            "validators": snap.validators.iter().map(|(a, p)| json!({
                "operator": a.to_bech32(),
                "voting_power": p,
            })).collect::<Vec<_>>(),
        }))
    }

    fn get_validator(&self, asset: &str, operator: &Address) -> Result<Value, RpcError> {
        let asset = parse_stake_asset(asset)?;
        let Some(val) = load_validator(self.store.as_ref(), asset, operator)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        else {
            return Err(RpcError::NotFound(format!(
                "validator {}/{}",
                asset.ticker(),
                operator.to_bech32()
            )));
        };
        Ok(json!({
            "asset": asset.ticker(),
            "operator": val.operator.to_bech32(),
            "withdrawal": val.withdrawal.to_bech32(),
            "self_bond": val.self_bond,
            "delegated": val.delegated,
            "commission_bps": val.commission_bps,
            "status": format!("{:?}", val.status),
            "jailed_until_epoch": val.jailed_until_epoch,
        }))
    }

    fn get_reward_pool(&self, asset: &str) -> Result<Value, RpcError> {
        let asset = parse_stake_asset(asset)?;
        let amount = load_reward_pool(self.store.as_ref(), asset)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "asset": asset.ticker(),
            "amount": amount,
        }))
    }

    fn get_native_asset_supply(&self, asset: &str) -> Result<Value, RpcError> {
        let asset = parse_native_asset(asset)?;
        let supply = load_native_supply_state(self.store.as_ref(), asset)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "asset": supply.asset.ticker(),
            "maximum_supply": supply.maximum_supply.to_string(),
            "issued_supply": supply.issued_supply.to_string(),
            "burned_supply": supply.burned_supply.to_string(),
            "net_supply": supply.net_supply.to_string(),
        }))
    }

    fn get_protocol_treasuries(&self) -> Result<Value, RpcError> {
        let policy = load_canonical_governance_policy(self.store.as_ref())
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let root = governance_treasury_root(self.store.as_ref())
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let treasuries = load_protocol_treasuries(self.store.as_ref())
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "maturity": "Scaffold",
            "consensus_mutations_active": false,
            "governance_root": root.to_hex(),
            "policy": {
                "version": policy.version,
                "constitution_id": policy.constitution_id,
                "constitution_hash": policy.constitution_hash.to_hex(),
                "authorization_root": policy.authorization_root.to_hex(),
            },
            "treasuries": treasuries.iter().map(|t| json!({
                "id": t.treasury.as_str(),
                "asset": t.asset.ticker(),
                "balance": t.balance.as_base_units(),
            })).collect::<Vec<_>>(),
        }))
    }

    fn get_community_registry(&self, limit: usize) -> Result<Value, RpcError> {
        let summary = load_canonical_community_summary(self.store.as_ref())
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let root = canonical_community_root(self.store.as_ref())
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let hubs = list_canonical_hubs(self.store.as_ref(), limit)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let passports = list_passport_attestations(self.store.as_ref(), limit)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let grants = list_canonical_grants(self.store.as_ref(), limit)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let missions = list_canonical_missions(self.store.as_ref(), limit)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "maturity": "Scaffold",
            "consensus_mutations_active": false,
            "root": root.to_hex(),
            "counts": {
                "hubs": summary.hub_count,
                "passport_attestations": summary.passport_count,
                "grants": summary.grant_count,
                "missions": summary.mission_count,
            },
            "hubs": hubs,
            "passport_attestations": passports,
            "grants": grants,
            "missions": missions,
        }))
    }

    fn submit_stake_tx(&mut self, stake_tx: Value) -> Result<Value, RpcError> {
        let tx: SignedStakeTx =
            serde_json::from_value(stake_tx).map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        let auth = self.tx_auth();
        let id = admit_stake_tx(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::StakeTx(tx.clone()))
                .map_err(|e| RpcError::Internal(e.to_string()))?;
        }
        Ok(json!({
            "stake_tx_id": id.to_hex(),
            "kind": tx.kind.as_str(),
            "asset": tx.asset.ticker(),
            "actor": tx.actor.to_bech32(),
            "validator": tx.validator.to_bech32(),
            "amount": tx.amount,
            "nonce": tx.nonce,
            "path": "mempool",
        }))
    }

    fn get_constitution(&self) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            Ok(json!({
                "id": snap.governance.constitution.id,
                "content_hash": snap.governance.constitution.content_hash_hex(),
                "body_markdown": snap.governance.constitution.body_markdown,
            }))
        })
    }

    fn get_governance(&self) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            let mut value = civic_overview_json(snap);
            if let Some(object) = value.as_object_mut() {
                object.insert("scope".into(), json!("administrative_local"));
                object.insert("consensus_accepted".into(), json!(false));
            }
            Ok(value)
        })
    }

    fn list_proposals(&self, limit: usize) -> Result<Value, RpcError> {
        self.with_civic(|snap| Ok(list_proposals_json(&snap.governance, limit)))
    }

    fn get_proposal(&self, id: u64) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            let p = snap
                .governance
                .proposal(id)
                .ok_or_else(|| RpcError::NotFound(format!("proposal {id}")))?;
            Ok(proposal_json(p))
        })
    }

    fn list_offices(&self) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            Ok(json!({
                "offices": snap.governance.offices.seats.iter().map(office_json).collect::<Vec<_>>(),
            }))
        })
    }

    fn list_forum_topics(&self, limit: usize) -> Result<Value, RpcError> {
        self.with_civic(|snap| Ok(list_topics_json(&snap.community, limit)))
    }

    fn submit_proposal(
        &mut self,
        author: Address,
        title: String,
        summary: String,
        kind: ProposalKind,
        slot: u64,
    ) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let id = snap
                .governance
                .submit_proposal(author, title, summary, kind, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id }))
        })
    }

    fn deposit_proposal(&mut self, id: u64, amount: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .add_deposit(id, amount)
                .map_err(crate::civic::map_gov_err)?;
            let p = snap
                .governance
                .proposal(id)
                .ok_or_else(|| RpcError::NotFound(format!("proposal {id}")))?;
            Ok(json!({ "proposal_id": id, "deposit": p.deposit }))
        })
    }

    fn open_proposal_voting(&mut self, id: u64, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .open_voting(id, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "status": "voting" }))
        })
    }

    fn cast_gov_vote(
        &mut self,
        id: u64,
        voter: Address,
        choice: VoteChoice,
        raw_balance: u64,
        total_supply: u64,
    ) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .cast_vote(id, voter, choice, raw_balance, total_supply)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "voted": true }))
        })
    }

    fn tally_proposal(&mut self, id: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let status = snap
                .governance
                .tally(id)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "status": status }))
        })
    }

    fn enter_proposal_timelock(&mut self, id: u64, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .enter_timelock(id, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "status": "timelock" }))
        })
    }

    fn execute_proposal(&mut self, id: u64, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .execute(id, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "status": "executed" }))
        })
    }

    fn post_forum_topic(
        &mut self,
        author: Address,
        title: String,
        body: String,
        category: TopicCategory,
        slot: u64,
    ) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let id = snap
                .community
                .post_topic(author, title, body, category, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "topic_id": id }))
        })
    }

    fn ack_constitution(&mut self, address: Address, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let id = snap.governance.constitution.id.clone();
            let hash = snap.governance.constitution.content_hash_hex();
            snap.community
                .acknowledge_constitution(address, id.clone(), hash.clone(), slot);
            Ok(json!({
                "address": address.to_bech32(),
                "constitution_id": id,
                "constitution_hash": hash,
                "acked": true,
            }))
        })
    }

    fn sponsor_proposal(&mut self, id: u64, who: Address) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .sponsor_as_tamias(id, who)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "sponsored": true }))
        })
    }

    fn assent_proposal(&mut self, id: u64, who: Address) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .record_archon_assent(id, who)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "assented": true }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admit::ChainState;
    use agora_consensus::{PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher};
    use agora_crypto::{
        derive_bip44, seed_from_mnemonic, sign_account_transfer_bound,
        sign_drc_account_policy_bound, sign_drc_deposit_preauth_bound, sign_drc_payment_bound,
        sign_ovl_execution_bound, sign_transaction_bound, Bip44Path, KeyPair,
    };
    use agora_state_machine::{
        credit_account_into, put_burned_supply_into, put_issued_supply_into, ColumnFamily,
        GenesisBuilder, WriteBatch,
    };
    use agora_types::{Address, Block, OutPoint, TxIn, TxOut};
    use borsh::BorshDeserialize;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn backend_config(genesis_hash: Hash) -> NodeBackendConfig {
        NodeBackendConfig {
            net: None,
            allow_fund: false,
            miner_address: Address::ZERO,
            connected_peers: Arc::new(AtomicU32::new(0)),
            network: "dev".into(),
            genesis_hash,
        }
    }

    #[test]
    fn protocol_treasuries_rpc_is_canonical_read_only_state() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let backend = NodeBackend::new(
            chain,
            store,
            Arc::new(Mutex::new(Mempool::new(8))),
            backend_config(genesis),
        );

        let value = backend.get_protocol_treasuries().unwrap();
        assert_eq!(value["maturity"], "Scaffold");
        assert_eq!(value["consensus_mutations_active"], false);
        let treasuries = value["treasuries"].as_array().unwrap();
        assert_eq!(treasuries.len(), 3);
        assert_eq!(treasuries[0]["asset"], "TLT");
        assert_eq!(treasuries[1]["asset"], "OVL");
        assert_eq!(treasuries[2]["asset"], "DRC");
        assert!(treasuries.iter().all(|t| t["balance"] == 0));
    }

    #[test]
    fn native_asset_supply_rpc_reads_decimal_string_counters() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut batch = WriteBatch::new();
        put_issued_supply_into(&mut batch, NativeAssetId::DRC, 100);
        put_burned_supply_into(&mut batch, NativeAssetId::DRC, 7);
        store.write_batch(batch).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let backend = NodeBackend::new(
            chain,
            store,
            Arc::new(Mutex::new(Mempool::new(8))),
            backend_config(genesis),
        );

        let value = backend.get_native_asset_supply("drc").unwrap();
        assert_eq!(value["asset"], "DRC");
        assert_eq!(value["issued_supply"], "100");
        assert_eq!(value["burned_supply"], "7");
        assert_eq!(value["net_supply"], "93");
        assert!(backend.get_native_asset_supply("issued-drc").is_err());
    }

    #[test]
    fn community_registry_rpc_is_canonical_read_only_state() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let backend = NodeBackend::new(
            chain,
            store,
            Arc::new(Mutex::new(Mempool::new(8))),
            backend_config(genesis),
        );

        let value = backend.get_community_registry(10).unwrap();
        assert_eq!(value["maturity"], "Scaffold");
        assert_eq!(value["consensus_mutations_active"], false);
        assert_eq!(value["counts"]["hubs"], 0);
        assert_eq!(value["counts"]["passport_attestations"], 0);
        assert_eq!(value["counts"]["grants"], 0);
        assert_eq!(value["counts"]["missions"], 0);
        assert!(value["root"].as_str().is_some_and(|root| root.len() == 64));
    }

    #[test]
    fn account_transfer_enters_template_lane() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::OVL,
            &alice.address(),
            Amount::from_base_units(100),
        )
        .unwrap();
        store.write_batch(funding).unwrap();

        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(chain, store, mempool, backend_config(genesis));
        let mut tx = AccountTransfer::unsigned_with_fee(
            NativeAssetId::OVL,
            alice.address(),
            bob.address(),
            Amount::from_base_units(10),
            Amount::from_base_units(1),
            0,
        );
        sign_account_transfer_bound(&mut tx, &alice, "agora-dev", &genesis).unwrap();

        let id = backend.submit_account_transfer(tx.clone()).unwrap();
        assert_eq!(id, tx.transfer_id());
        let template = backend.get_block_template().unwrap();
        assert_eq!(template.account_transfers, vec![tx]);
        assert_eq!(template.header.tx_root, template.compute_body_root());
        assert_ne!(
            template.header.tx_root,
            Block::compute_tx_root(&template.transactions)
        );
    }

    #[test]
    fn ovl_execution_enters_template_lane() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::OVL,
            &alice.address(),
            Amount::from_base_units(50_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(chain, store, mempool, backend_config(genesis));
        let mut tx = OvlExecutionTx::unsigned(
            alice.address(),
            bob.address(),
            Amount::from_base_units(1_000),
            agora_state_machine::OVL_INTRINSIC_GAS,
            1,
            0,
            vec![],
        );
        sign_ovl_execution_bound(&mut tx, &alice, "agora-dev", &genesis).unwrap();

        let id = backend.submit_ovl_execution(tx.clone()).unwrap();
        assert_eq!(id, tx.tx_id());
        let template = backend.get_block_template().unwrap();
        assert_eq!(template.ovl_executions, vec![tx]);
        assert_eq!(template.header.tx_root, template.compute_body_root());
    }

    #[test]
    fn drc_payment_enters_template_lane() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &alice.address(),
            Amount::from_base_units(1_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(chain, store, mempool, backend_config(genesis));
        let mut tx = DrcPaymentTx::unsigned_v2(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(100),
            Amount::from_base_units(1),
            77,
            Some(88),
            Hash([8; 32]),
            0,
        );
        sign_drc_payment_bound(&mut tx, &alice, "agora-dev", &genesis).unwrap();

        let id = backend.submit_drc_payment(tx.clone()).unwrap();
        assert_eq!(id, tx.payment_id());
        assert!(
            backend.get_drc_payment(&id).unwrap().is_none(),
            "pending mempool payments are intentionally not reported as settled"
        );
        assert!(
            backend
                .get_drc_payment_by_invoice(&merchant.address(), &tx.invoice_id)
                .unwrap()
                .is_none(),
            "pending mempool invoices are intentionally not reported as settled"
        );
        let template = backend.get_block_template().unwrap();
        assert_eq!(template.drc_payments, vec![tx]);
        assert_eq!(template.drc_payments[0].source_tag, Some(88));
        assert_eq!(template.header.tx_root, template.compute_body_root());
    }

    #[test]
    fn drc_account_policy_enters_template_and_pending_does_not_mutate_query() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let owner = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &owner.address(),
            Amount::from_base_units(100),
        )
        .unwrap();
        store.write_batch(funding).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(chain, store, mempool, backend_config(genesis));
        assert_eq!(
            backend
                .get_drc_account_policy(&owner.address())
                .unwrap()
                .unwrap(),
            (DrcAccountPolicy::default(), 0)
        );

        let mut tx = DrcAccountPolicyTx::set_require_destination_tag(
            owner.address(),
            Amount::from_base_units(1),
            0,
        );
        sign_drc_account_policy_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();
        let id = backend.submit_drc_account_policy(tx.clone()).unwrap();
        assert_eq!(id, tx.policy_tx_id());
        assert_eq!(
            backend
                .get_drc_account_policy(&owner.address())
                .unwrap()
                .unwrap(),
            (DrcAccountPolicy::default(), 0),
            "pending policy is not canonical state"
        );

        let template = backend.get_block_template().unwrap();
        assert_eq!(template.drc_account_policies, vec![tx]);
        assert_eq!(template.header.tx_root, template.compute_body_root());
    }

    #[test]
    fn drc_deposit_preauth_enters_template_and_pending_does_not_mutate_query() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let owner = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let source = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut funding = WriteBatch::new();
        for account in [&owner, &source] {
            credit_account_into(
                &mut funding,
                &store,
                NativeAssetId::DRC,
                &account.address(),
                Amount::from_base_units(100),
            )
            .unwrap();
        }
        store.write_batch(funding).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(chain, store, mempool, backend_config(genesis));
        let before = backend
            .get_drc_deposit_preauth(&owner.address(), &source.address())
            .unwrap()
            .unwrap();
        assert!(!before.preauthorized);
        assert!(!before.deposit_auth_required);
        assert!(before.deposit_authorized);

        let mut tx = DrcDepositPreauthTx::authorize(
            owner.address(),
            source.address(),
            Amount::from_base_units(1),
            0,
        );
        sign_drc_deposit_preauth_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();
        let id = backend.submit_drc_deposit_preauth(tx.clone()).unwrap();
        assert_eq!(id, tx.preauth_tx_id());
        assert_eq!(
            backend
                .get_drc_deposit_preauth(&owner.address(), &source.address())
                .unwrap()
                .unwrap(),
            before,
            "pending preauthorization is not canonical state"
        );

        let template = backend.get_block_template().unwrap();
        assert_eq!(template.drc_deposit_preauths, vec![tx]);
        assert_eq!(template.header.tx_root, template.compute_body_root());
    }

    #[test]
    fn drc_payment_query_reads_root_committed_receipt() {
        let store = Arc::new(StateStore::open_in_memory());
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &alice.address(),
            Amount::from_base_units(1_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();

        let mut payment = DrcPaymentTx::unsigned_v2(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(100),
            Amount::from_base_units(1),
            77,
            Some(88),
            Hash([8; 32]),
            0,
        );
        sign_drc_payment_bound(&mut payment, &alice, "agora-dev", &genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let expected = apply_drc_payment_at_blue_score(
            store.as_ref(),
            &payment,
            &TxAuthContext {
                chain_id: "agora-dev".into(),
                genesis,
                data_availability_network_fingerprint: None,
            },
            1,
            &mut batch,
            &mut journal,
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let backend = NodeBackend::new(
            chain,
            store,
            Arc::new(Mutex::new(Mempool::new(64))),
            backend_config(genesis),
        );
        assert_eq!(
            backend
                .get_drc_payment(&expected.payment_id)
                .unwrap()
                .unwrap(),
            expected
        );
        assert_eq!(
            backend
                .get_drc_payment_by_invoice(&merchant.address(), &payment.invoice_id)
                .unwrap()
                .unwrap(),
            expected
        );
        assert!(backend
            .get_drc_payment_by_invoice(&Address([7; 20]), &payment.invoice_id)
            .unwrap()
            .is_none());
        assert!(backend.get_drc_payment(&Hash::ZERO).unwrap().is_none());
    }

    #[test]
    fn genesis_tips_and_admit_easy_block() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let premine = Address([9u8; 20]);
        let genesis = GenesisBuilder::default()
            .with_premine_address(premine)
            .ignite(&store)
            .unwrap();

        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let miner = Address([1u8; 20]);
        let mut backend = NodeBackend::new(
            chain.clone(),
            store,
            mempool,
            NodeBackendConfig {
                miner_address: miner,
                ..backend_config(genesis)
            },
        );
        assert_eq!(backend.dag_tips(), vec![genesis]);
        assert_eq!(
            backend.get_balance(&premine).as_base_units(),
            Amount::from_whole(10_000_000).unwrap().as_base_units()
        );

        let mut block = backend.get_block_template().unwrap();
        assert_eq!(block.header.bits, 0); // DAA initial bits from bootstrap
        assert_eq!(block.transactions.len(), 1);
        assert!(block.transactions[0].inputs.is_empty());
        assert_eq!(block.transactions[0].outputs[0].address, miner);
        assert_eq!(
            block.header.tx_root,
            Block::compute_tx_root(&block.transactions)
        );
        block.header.nonce = 1;
        let pow = RandomXPowHasher.pow_hash(&block.header);
        agora_consensus::LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &pow)
            .unwrap();
        let reward = block.transactions[0].outputs[0].value;
        let id = backend.submit_block(block).unwrap();
        assert_ne!(id, genesis);
        assert!(backend.dag_tips().contains(&id));
        assert_eq!(backend.get_balance(&miner), reward);
    }

    #[test]
    fn submit_transaction_requires_live_utxo() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let from = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let to = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();
        let genesis = GenesisBuilder::default()
            .with_premine_address(from.address())
            .ignite(&store)
            .unwrap();
        let genesis_block = {
            let bytes = store
                .get_cf(ColumnFamily::Hot, genesis.as_bytes())
                .unwrap()
                .unwrap();
            Block::try_from_slice(&bytes).unwrap()
        };
        let premine_txid = genesis_block.transactions[0].tx_id();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap_with(
                store.clone(),
                genesis,
                crate::admit::ChainBootConfig {
                    chain_id: "agora-dev".into(),
                    ..crate::admit::ChainBootConfig::default()
                },
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(chain, store, mempool, backend_config(genesis));

        let mut bad = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: to,
            }],
            1,
        );
        sign_transaction_bound(&mut bad, &from, "agora-dev", &genesis).unwrap();
        assert!(backend.submit_transaction(bad).is_err());

        let premine = Amount::from_whole(10_000_000).unwrap();
        let pay = Amount::from_whole(1).unwrap().as_base_units();
        let fee = 1u64;
        let mut good = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: premine_txid,
                    index: 0,
                },
            }],
            vec![
                TxOut {
                    value: Amount::from_base_units(pay),
                    address: to,
                },
                TxOut {
                    value: Amount::from_base_units(premine.as_base_units() - pay - fee),
                    address: from.address(),
                },
            ],
            2,
        );
        sign_transaction_bound(&mut good, &from, "agora-dev", &genesis).unwrap();
        let id = backend.submit_transaction(good.clone()).unwrap();
        assert_eq!(id, good.tx_id());
        // Second spend of the same outpoint must fail while the first is reserved.
        let mut conflict = good.clone();
        conflict.nonce = 3;
        sign_transaction_bound(&mut conflict, &from, "agora-dev", &genesis).unwrap();
        assert!(backend.submit_transaction(conflict).is_err());
    }

    #[test]
    fn template_includes_mempool_tx_and_evicts_on_submit() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let from = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let to = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();
        let genesis = GenesisBuilder::default()
            .with_premine_address(from.address())
            .ignite(&store)
            .unwrap();
        let genesis_block = {
            let bytes = store
                .get_cf(ColumnFamily::Hot, genesis.as_bytes())
                .unwrap()
                .unwrap();
            Block::try_from_slice(&bytes).unwrap()
        };
        let premine_txid = genesis_block.transactions[0].tx_id();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap_with(
                store.clone(),
                genesis,
                crate::admit::ChainBootConfig {
                    chain_id: "agora-dev".into(),
                    ..crate::admit::ChainBootConfig::default()
                },
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let miner = Address([2u8; 20]);
        let mut backend = NodeBackend::new(
            chain,
            store,
            mempool.clone(),
            NodeBackendConfig {
                miner_address: miner,
                ..backend_config(genesis)
            },
        );

        let premine = Amount::from_whole(10_000_000).unwrap();
        let pay = Amount::from_whole(1).unwrap().as_base_units();
        let fee = 1u64;
        let mut transfer = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: premine_txid,
                    index: 0,
                },
            }],
            vec![
                TxOut {
                    value: Amount::from_base_units(pay),
                    address: to,
                },
                TxOut {
                    value: Amount::from_base_units(premine.as_base_units() - pay - fee),
                    address: from.address(),
                },
            ],
            7,
        );
        sign_transaction_bound(&mut transfer, &from, "agora-dev", &genesis).unwrap();
        let tx_id = backend.submit_transaction(transfer.clone()).unwrap();

        let pending = backend.get_transaction(&tx_id).unwrap();
        assert_eq!(pending.status.as_str(), "pending");
        assert_eq!(pending.fee, Some(fee));

        let mut block = backend.get_block_template().unwrap();
        assert_eq!(block.transactions.len(), 2);
        assert!(block.transactions[0].inputs.is_empty());
        assert_eq!(block.transactions[1].tx_id(), tx_id);
        let coinbase_value = block.transactions[0].outputs[0].value.as_base_units();
        // Next block after genesis (blue_score 1) estimates blue_score 2.
        let emission = agora_consensus::EmissionSchedule::default().reward_at_blue_score(2);
        assert_eq!(
            coinbase_value,
            emission + fee,
            "coinbase should be emission + transfer fee"
        );
        assert_eq!(
            block.header.tx_root,
            Block::compute_tx_root(&block.transactions)
        );
        block.header.nonce = 1;
        let pow = RandomXPowHasher.pow_hash(&block.header);
        agora_consensus::LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &pow)
            .unwrap();
        let block_id = backend.submit_block(block).unwrap();
        assert!(!mempool.lock().unwrap().contains(&tx_id));
        assert_eq!(
            backend.get_balance(&to).as_base_units(),
            Amount::from_whole(1).unwrap().as_base_units()
        );
        assert_eq!(backend.get_balance(&miner).as_base_units(), emission + fee);

        let confirmed = backend.get_transaction(&tx_id).unwrap();
        assert_eq!(confirmed.status.as_str(), "confirmed");
        assert_eq!(confirmed.acceptance.as_deref(), Some("Accepted"));
        assert_eq!(confirmed.block_id, Some(block_id));
        assert_eq!(confirmed.index, Some(1));
    }

    #[test]
    fn fund_address_mints_spendable_utxo() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let funded = derive_bip44(&seed, &Bip44Path::external(5)).unwrap();
        let payee = derive_bip44(&seed, &Bip44Path::external(6))
            .unwrap()
            .address();
        let genesis = GenesisBuilder::default()
            .with_premine_address(Address([9u8; 20]))
            .ignite(&store)
            .unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap_with(
                store.clone(),
                genesis,
                crate::admit::ChainBootConfig {
                    chain_id: "agora-dev".into(),
                    ..crate::admit::ChainBootConfig::default()
                },
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(
            chain,
            store.clone(),
            mempool,
            NodeBackendConfig {
                allow_fund: true,
                ..backend_config(genesis)
            },
        );

        let drip = Amount::from_base_units(5_000);
        assert_eq!(backend.fund_address(funded.address(), drip).unwrap(), drip);
        assert_eq!(backend.get_balance(&funded.address()), drip);
        let minted = backend.get_utxos(&funded.address()).unwrap();
        assert_eq!(minted.len(), 1);
        assert_eq!(minted[0].value, drip);

        let (op, out) = {
            let mut found = None;
            store
                .for_each_cf(ColumnFamily::Utxo, |key, value| {
                    let tx_out = TxOut::try_from_slice(value)
                        .map_err(|e| agora_state_machine::StateError::Storage(e.to_string()))?;
                    if tx_out.address == funded.address() && key.len() == 36 {
                        let mut tx_bytes = [0u8; 32];
                        tx_bytes.copy_from_slice(&key[..32]);
                        let index = u32::from_le_bytes(key[32..36].try_into().unwrap());
                        found = Some((
                            OutPoint {
                                tx_id: Hash(tx_bytes),
                                index,
                            },
                            tx_out,
                        ));
                    }
                    Ok(())
                })
                .unwrap();
            found.expect("minted utxo")
        };
        assert_eq!(out.value, drip);

        let fee = 1u64;
        let mut spend = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: op,
            }],
            vec![TxOut {
                value: Amount::from_base_units(drip.as_base_units() - fee),
                address: payee,
            }],
            1,
        );
        sign_transaction_bound(&mut spend, &funded, "agora-dev", &genesis).unwrap();
        backend.submit_transaction(spend).unwrap();
        let mut block = backend.get_block_template().unwrap();
        assert_eq!(block.transactions.len(), 2);
        block.header.nonce = 1;
        backend.submit_block(block).unwrap();
        assert_eq!(backend.get_balance(&funded.address()), Amount::ZERO);
        assert_eq!(
            backend.get_balance(&payee).as_base_units(),
            drip.as_base_units() - fee
        );
    }

    #[test]
    fn fund_address_hard_disabled_on_mainnet_label() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        // Even with allow_fund=true, mainnet label must reject.
        let mut backend = NodeBackend::new(
            chain,
            store,
            mempool,
            NodeBackendConfig {
                allow_fund: true,
                network: "mainnet".into(),
                ..backend_config(genesis)
            },
        );
        let err = backend
            .fund_address(Address([1u8; 20]), Amount::from_base_units(1))
            .unwrap_err();
        match err {
            RpcError::Rejected(msg) => assert!(msg.contains("mainnet")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn drc_ticket_create_admit_and_block_eviction_release_reservations() {
        use agora_crypto::sign_drc_ticket_create_bound;
        use agora_state_machine::TxAuthContext;
        use agora_types::{BlockHeader, DrcTicketCreateTx, NativeAssetId};

        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let kp = KeyPair::from_secret_bytes(&[0x44; 32]).unwrap();
        let owner = kp.address();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &owner,
            Amount::from_base_units(1_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();
        let auth = TxAuthContext {
            chain_id: "dev".into(),
            genesis,
            data_availability_network_fingerprint: None,
        };
        let mut create = DrcTicketCreateTx::unsigned(owner, Amount::from_base_units(1), 0);
        sign_drc_ticket_create_bound(&mut create, &kp, &auth.chain_id, &auth.genesis).unwrap();
        let id = admit_drc_ticket_create(store.as_ref(), &mempool, create.clone(), &auth).unwrap();
        {
            let pool = mempool.lock().unwrap();
            assert!(pool.account_reserved(NativeAssetId::DRC, &owner));
            assert!(pool.ticket_consumer_reserved(&owner, 1));
        }
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_ticket_creates.push(create);
        block.header.tx_root = block.compute_body_root();
        {
            let mut pool = mempool.lock().unwrap();
            pool.evict_for_block(&block);
            assert!(!pool.contains(&id));
            assert!(!pool.account_reserved(NativeAssetId::DRC, &owner));
            assert!(!pool.ticket_consumer_reserved(&owner, 1));
        }
    }

    #[test]
    fn drc_escrow_submit_and_point_query_end_to_end() {
        use agora_crypto::{sign_drc_escrow_create_bound, KeyPair};
        use agora_types::{DrcEscrowCreateTx, Hash, DRC_ESCROW_CREATE_TX_VERSION};

        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let owner = KeyPair::from_secret_bytes(&[40; 32]).unwrap();
        let recipient = KeyPair::from_secret_bytes(&[41; 32]).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &owner.address(),
            Amount::from_base_units(10_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(
            chain,
            store.clone(),
            Arc::new(Mutex::new(Mempool::new(64))),
            backend_config(genesis),
        );
        let auth = backend.tx_auth();
        let mut create = DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TX_VERSION,
            owner: owner.address(),
            recipient: recipient.address(),
            amount: Amount::from_base_units(25),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(100),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_create_bound(&mut create, &owner, &auth.chain_id, &auth.genesis).unwrap();
        let escrow_id = backend.submit_drc_escrow_create(create).unwrap();
        assert_eq!(
            backend.get_drc_escrow(&escrow_id).unwrap()["status"],
            json!("unknown")
        );
        assert!(backend
            .mempool
            .lock()
            .unwrap()
            .pending_escrow_create(&escrow_id));
        let unknown = backend.get_drc_escrow(&Hash([0xab; 32])).unwrap();
        assert_eq!(unknown["status"], json!("unknown"));
    }
}

#[cfg(test)]
impl NodeBackend {
    pub(crate) fn test_mempool(&self) -> &Arc<Mutex<Mempool>> {
        &self.mempool
    }

    pub(crate) fn test_chain(&self) -> &Arc<Mutex<ChainState>> {
        &self.chain
    }

    pub(crate) fn test_miner(&self) -> Address {
        self.miner_address
    }
}

#[cfg(test)]
#[path = "drc_check_template_tests.rs"]
mod drc_check_template_tests;
#[cfg(test)]
#[path = "drc_escrow_template_tests.rs"]
mod drc_escrow_template_tests;
#[cfg(test)]
#[path = "drc_ledger_object_rpc_integration_tests.rs"]
mod drc_ledger_object_rpc_integration_tests;
#[cfg(test)]
#[path = "drc_payment_channel_public_helpers.rs"]
mod drc_payment_channel_public_helpers;
#[cfg(test)]
#[path = "drc_payment_channel_public_security_tests.rs"]
mod drc_payment_channel_public_security_tests;
#[cfg(test)]
#[path = "drc_payment_channel_reorg_reservation_tests.rs"]
mod drc_payment_channel_reorg_reservation_tests;
#[cfg(test)]
#[path = "drc_payment_channel_rpc_integration_tests.rs"]
mod drc_payment_channel_rpc_integration_tests;

#[cfg(test)]
#[path = "drc_issued_controls_public_helpers.rs"]
mod drc_issued_controls_public_helpers;
#[cfg(test)]
#[path = "drc_issued_controls_public_invariant_tests.rs"]
mod drc_issued_controls_public_invariant_tests;
#[cfg(test)]
#[path = "drc_issued_controls_public_security_tests.rs"]
mod drc_issued_controls_public_security_tests;
#[cfg(test)]
#[path = "drc_issued_controls_reorg_reservation_tests.rs"]
mod drc_issued_controls_reorg_reservation_tests;
#[cfg(test)]
#[path = "drc_issued_controls_rpc_integration_tests.rs"]
mod drc_issued_controls_rpc_integration_tests;
#[cfg(test)]
#[path = "drc_issued_controls_template_tests.rs"]
mod drc_issued_controls_template_tests;
#[cfg(test)]
#[path = "drc_payment_channel_template_tests.rs"]
mod drc_payment_channel_template_tests;
#[cfg(test)]
#[path = "drc_trust_line_public_helpers.rs"]
mod drc_trust_line_public_helpers;
#[cfg(test)]
#[path = "drc_trust_line_public_invariant_tests.rs"]
mod drc_trust_line_public_invariant_tests;
#[cfg(test)]
#[path = "drc_trust_line_public_security_tests.rs"]
mod drc_trust_line_public_security_tests;
#[cfg(test)]
#[path = "drc_trust_line_reorg_reservation_tests.rs"]
mod drc_trust_line_reorg_reservation_tests;
#[cfg(test)]
#[path = "drc_trust_line_rpc_integration_tests.rs"]
mod drc_trust_line_rpc_integration_tests;
#[cfg(test)]
#[path = "drc_trust_line_template_tests.rs"]
mod drc_trust_line_template_tests;
