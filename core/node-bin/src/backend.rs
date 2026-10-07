//! Live [`RpcBackend`] backed by chain admission + mempool.

use std::collections::BTreeMap;
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
    AccountBalances, DataCommitmentLookup, DrcDepositPreauthStatus, FeeEstimate, MempoolEntry,
    NodeInfo, RpcBackend, RpcError, TltCovenantLookup, TxLookup, UtxoEntry,
};
use agora_state_machine::{
    apply_account_transfer, apply_drc_account_policy, apply_drc_check_cancel, apply_drc_check_cash,
    apply_drc_check_create, apply_drc_deposit_preauth, apply_drc_escrow_cancel,
    apply_drc_escrow_create, apply_drc_escrow_finish, apply_drc_payment_at_blue_score,
    apply_drc_regular_key, apply_drc_signer_list, apply_drc_ticket_create, apply_ovl_execution,
    apply_signed_stake_tx, build_snapshot, canonical_community_root, governance_treasury_root,
    issuer_is_active_hub_coordinator, list_drc_account_objects,
    list_grants as list_canonical_grants, list_hubs as list_canonical_hubs,
    list_missions as list_canonical_missions, list_passport_attestations, load_account,
    load_canonical_community_summary, load_canonical_governance_policy, load_data_commitment,
    load_drc_account_policy, load_drc_check_receipt, load_drc_deposit_preauth,
    load_drc_escrow_receipt, load_drc_issued_asset_policy_receipt,
    load_drc_issued_clawback_receipt, load_drc_issued_transfer_receipt, load_drc_ledger_object,
    load_drc_operation, load_drc_payment_by_invoice, load_drc_payment_channel_claim_event,
    load_drc_payment_channel_fund_event, load_drc_payment_channel_live,
    load_drc_payment_channel_receipt, load_drc_payment_channel_schedule_event,
    load_drc_payment_receipt, load_drc_transaction, load_drc_trust_line_issuer_control_receipt,
    load_drc_trust_line_live, load_epoch, load_grant_registrar_nonce, load_grant_registration,
    load_hub_coordinator_nonce, load_hub_registration, load_known_drc_account_keys,
    load_known_drc_account_policy, load_known_drc_account_signer_summary,
    load_known_drc_deposit_authorization, load_mission_registration, load_mission_sponsor_nonce,
    load_native_supply_state, load_passport_attestation, load_passport_issuer_nonce,
    load_protocol_treasuries, load_reward_pool, load_validator, lookup_covenant_tx_location,
    lookup_data_commitment_location, lookup_drc_check_point, lookup_drc_escrow_point,
    lookup_drc_issuer_liability_point, lookup_drc_payment_channel_point, lookup_drc_ticket_point,
    lookup_drc_trust_line_point, lookup_tx_location, meta_keys, outpoint_key,
    plan_drc_mempool_reservation, validate_mempool_covenant, validate_mempool_tx_with_auth,
    AccountJournal, ColumnFamily, DrcMempoolReservation, DrcTicketPointStatus, StakingParams,
    StateStore, TxAuthContext, WriteBatch,
};
use agora_types::{
    sequence_signals_rbf, AccountTransfer, Address, Amount, Block, CheckpointAttestation,
    DataCommitmentAuthorization, DataCommitmentSource, DrcAcceptedOperationReceipt,
    DrcAccountPolicy, DrcAccountPolicyTx, DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx,
    DrcDepositPreauthTx, DrcEscrowCancelTx, DrcEscrowCreateTx, DrcEscrowFinishTx,
    DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx, DrcIssuedTransferTx, DrcLedgerObjectDescriptor,
    DrcLedgerObjectKind, DrcLedgerObjectPage, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseTx,
    DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, DrcPaymentReceipt, DrcPaymentTx,
    DrcRegularKeyTx, DrcSignerListTx, DrcTicketCreateTx, DrcTrustLineIssuerControlTx,
    DrcTrustLineSetTx, GrantRegistration, Hash, HubRegistration, MissionRegistration,
    NativeAssetId, OutPoint, OvlExecutionTx, PassportAttestation, SignedStakeTx, TltCovenantTx,
    Transaction, TxOut, ACCOUNT_TRANSFER_DRC_TICKET_VERSION, DRC_ACCOUNT_POLICY_TICKET_TX_VERSION,
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

fn fallback_tx_auth(network: &str, genesis: Hash) -> TxAuthContext {
    let chain_id = match network.to_ascii_lowercase().as_str() {
        "mainnet" => "agora-mainnet-1",
        "testnet" => "agora-testnet-1",
        _ => "agora-dev",
    };
    TxAuthContext {
        chain_id: chain_id.into(),
        genesis,
        data_availability_network_fingerprint: None,
    }
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

/// Covenant lane admission. v1 wire and premine address locks are unchanged.
pub(crate) fn admit_tlt_covenant(
    store: &StateStore,
    chain: &Mutex<ChainState>,
    mempool: &Mutex<Mempool>,
    tx: TltCovenantTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let (blue_score, median_time_secs) = {
        let chain = chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?;
        let blue_score = chain
            .next_template_blue_score()
            .map_err(|err| RpcError::Internal(err.to_string()))?;
        let parents = chain
            .select_template_parents()
            .map_err(|err| RpcError::Internal(err.to_string()))?;
        let median_time_secs = chain
            .template_median_time_secs(&parents)
            .map_err(|err| RpcError::Internal(err.to_string()))?;
        (blue_score, median_time_secs)
    };
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    // A replacement must be checked as if the conflicting reservations were free.
    // `replace_covenant` still rejects v1 conflicts, missing signals, and low fees.
    let mut spent = pool.reserved().clone();
    let conflicts = tx
        .inputs
        .iter()
        .any(|input| spent.contains(&input.previous_outpoint));
    if conflicts {
        if !tx
            .inputs
            .iter()
            .any(|input| sequence_signals_rbf(input.sequence))
        {
            return Err(RpcError::Rejected(
                "covenant conflicts with the mempool and does not signal replace-by-fee".into(),
            ));
        }
        for input in &tx.inputs {
            spent.remove(&input.previous_outpoint);
        }
    }
    let fee =
        validate_mempool_covenant(store, &tx, &spent, Some(auth), blue_score, median_time_secs)
            .map_err(|err| RpcError::Rejected(format!("covenant: {err}")))?;
    let min_fee = min_relay_fee();
    if fee < min_fee {
        return Err(RpcError::Rejected(format!(
            "fee too low: {fee} < min relay {min_fee}"
        )));
    }
    if conflicts {
        return pool
            .replace_covenant(tx, fee)
            .map_err(|err| RpcError::Rejected(err.to_string()));
    }
    pool.admit_covenant(tx, fee)
        .map_err(|err| RpcError::Rejected(err.to_string()))
}

/// DA lane admission. Default Experimental boot binds the mesh fingerprint and
/// requires the operator to hold at least [`agora_types::DA_INCLUSION_FEE_TLT`].
pub(crate) fn admit_data_commitment(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    authorization: agora_types::DataCommitmentAuthorization,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let fingerprint = auth
        .data_availability_network_fingerprint
        .as_ref()
        .filter(|fingerprint| **fingerprint != Hash::ZERO)
        .ok_or_else(|| {
            RpcError::Rejected("data commitment requires a DA network fingerprint".into())
        })?;
    agora_crypto::verify_data_commitment_bound(
        &authorization,
        &auth.chain_id,
        &auth.genesis,
        fingerprint,
    )
    .map_err(|err| RpcError::Rejected(format!("invalid DA authorization: {err}")))?;
    if let Some(existing) = agora_state_machine::load_data_commitment(
        store,
        authorization.commitment.source,
        authorization.commitment.sequence,
    )
    .map_err(|err| RpcError::Internal(err.to_string()))?
    {
        if existing.authorization.authorization_id() == authorization.authorization_id() {
            return Ok(authorization.authorization_id());
        }
        return Err(RpcError::Rejected(
            "data commitment source sequence already accepted".into(),
        ));
    }
    let expected = agora_state_machine::load_data_commitment_nonce(store, &authorization.operator)
        .map_err(|err| RpcError::Internal(err.to_string()))?;
    if authorization.replay_nonce != expected {
        return Err(RpcError::Rejected(format!(
            "DA replay nonce {} does not match next {}",
            authorization.replay_nonce, expected
        )));
    }
    let spendable = agora_state_machine::balance_of(store, &authorization.operator)
        .map_err(|err| RpcError::Internal(err.to_string()))?
        .as_base_units();
    if spendable < agora_types::DA_INCLUSION_FEE_TLT {
        return Err(RpcError::Rejected(format!(
            "DA inclusion fee: insufficient TLT (need {}, have {spendable})",
            agora_types::DA_INCLUSION_FEE_TLT
        )));
    }
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    pool.admit_data_commitment(authorization)
        .map_err(|err| RpcError::Rejected(err.to_string()))
}

pub(crate) fn admit_passport_attestation(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    attestation: PassportAttestation,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    agora_crypto::verify_passport_attestation_bound(&attestation, &auth.chain_id, &auth.genesis)
        .map_err(|err| RpcError::Rejected(format!("invalid passport attestation: {err}")))?;
    if load_passport_attestation(store, &attestation.attestation_id())
        .map_err(|err| RpcError::Internal(err.to_string()))?
        .is_some()
    {
        return Ok(attestation.attestation_id());
    }
    if !issuer_is_active_hub_coordinator(store, &attestation.issuer)
        .map_err(|err| RpcError::Internal(err.to_string()))?
    {
        return Err(RpcError::Rejected(
            "passport issuer is not an active canonical hub coordinator".into(),
        ));
    }
    let expected = load_passport_issuer_nonce(store, &attestation.issuer)
        .map_err(|err| RpcError::Internal(err.to_string()))?;
    if attestation.nonce != expected {
        return Err(RpcError::Rejected("passport issuer nonce mismatch".into()));
    }
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    pool.admit_passport_attestation(attestation)
        .map_err(|err| RpcError::Rejected(err.to_string()))
}

pub(crate) fn admit_hub_registration(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    registration: HubRegistration,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    agora_crypto::verify_hub_registration_bound(&registration, &auth.chain_id, &auth.genesis)
        .map_err(|err| RpcError::Rejected(format!("invalid hub registration: {err}")))?;
    if load_hub_registration(store, &registration.registration_id())
        .map_err(|err| RpcError::Internal(err.to_string()))?
        .is_some()
    {
        return Ok(registration.registration_id());
    }
    let coordinator = registration
        .first_coordinator()
        .ok_or_else(|| RpcError::Rejected("hub coordinators must be nonempty".into()))?;
    let expected = load_hub_coordinator_nonce(store, &coordinator)
        .map_err(|err| RpcError::Internal(err.to_string()))?;
    if registration.nonce != expected {
        return Err(RpcError::Rejected("hub coordinator nonce mismatch".into()));
    }
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    pool.admit_hub_registration(registration)
        .map_err(|err| RpcError::Rejected(err.to_string()))
}

pub(crate) fn admit_grant_registration(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    registration: GrantRegistration,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    agora_crypto::verify_grant_registration_bound(&registration, &auth.chain_id, &auth.genesis)
        .map_err(|err| RpcError::Rejected(format!("invalid grant registration: {err}")))?;
    if load_grant_registration(store, &registration.registration_id())
        .map_err(|err| RpcError::Internal(err.to_string()))?
        .is_some()
    {
        return Ok(registration.registration_id());
    }
    if !issuer_is_active_hub_coordinator(store, &registration.registrar)
        .map_err(|err| RpcError::Internal(err.to_string()))?
    {
        return Err(RpcError::Rejected(
            "grant registrar is not an active canonical hub coordinator".into(),
        ));
    }
    let expected = load_grant_registrar_nonce(store, &registration.registrar)
        .map_err(|err| RpcError::Internal(err.to_string()))?;
    if registration.nonce != expected {
        return Err(RpcError::Rejected("grant registrar nonce mismatch".into()));
    }
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    pool.admit_grant_registration(registration)
        .map_err(|err| RpcError::Rejected(err.to_string()))
}

pub(crate) fn admit_mission_registration(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    registration: MissionRegistration,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    agora_crypto::verify_mission_registration_bound(&registration, &auth.chain_id, &auth.genesis)
        .map_err(|err| RpcError::Rejected(format!("invalid mission registration: {err}")))?;
    if load_mission_registration(store, &registration.registration_id())
        .map_err(|err| RpcError::Internal(err.to_string()))?
        .is_some()
    {
        return Ok(registration.registration_id());
    }
    if !issuer_is_active_hub_coordinator(store, &registration.sponsor)
        .map_err(|err| RpcError::Internal(err.to_string()))?
    {
        return Err(RpcError::Rejected(
            "mission sponsor is not an active canonical hub coordinator".into(),
        ));
    }
    let expected = load_mission_sponsor_nonce(store, &registration.sponsor)
        .map_err(|err| RpcError::Internal(err.to_string()))?;
    if registration.nonce != expected {
        return Err(RpcError::Rejected("mission sponsor nonce mismatch".into()));
    }
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    pool.admit_mission_registration(registration)
        .map_err(|err| RpcError::Rejected(err.to_string()))
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
    if tx.version != agora_types::OVL_EXECUTION_VERSION {
        return Err(RpcError::Rejected(
            "raw EVM transactions are not admitted to the Agora-signed mempool".into(),
        ));
    }
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

/// Admit a version-2 raw Ethereum envelope to the raw pool (not Agora-signed).
pub(crate) fn admit_ovl_raw_execution(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    pending_evm: &Mutex<BTreeMap<[u8; 32], agora_ovl_evm::PendingTx>>,
    tx: OvlExecutionTx,
) -> Result<Hash, RpcError> {
    if tx.version != agora_types::OVL_EXECUTION_RAW_EVM_VERSION {
        return Err(RpcError::Rejected(
            "OVL raw gossip requires execution version 2".into(),
        ));
    }
    let world = agora_state_machine::load_ovl_evm_world(store)
        .map_err(|err| RpcError::Internal(err.to_string()))?;
    if !world.active {
        return Err(RpcError::Rejected(
            "OVL-EVM-v1 is inactive on this genesis".into(),
        ));
    }
    let parsed = agora_ovl_evm::parse_raw_transaction(&tx.data)
        .map_err(|err| RpcError::Rejected(format!("raw EVM: {err}")))?;
    if parsed.chain_id != world.chain_id {
        return Err(RpcError::Rejected(
            "pending transaction chain id mismatch".into(),
        ));
    }
    agora_ovl_evm::measure_shanghai_gas(&world, &tx.data)
        .map_err(|err| RpcError::Rejected(format!("raw EVM: {err}")))?;
    let mut pending = pending_evm
        .lock()
        .map_err(|_| RpcError::Internal("OVL EVM pending lock poisoned".into()))?;
    pending.insert(
        parsed.hash,
        agora_ovl_evm::PendingTx {
            raw: tx.data.clone(),
            from: parsed.caller,
            nonce: parsed.nonce,
        },
    );
    drop(pending);
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    pool.admit_ovl_raw_execution(tx)
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
    /// Signing/DA domain captured at construction.
    ///
    /// RPC methods that already hold `chain` must not re-enter that mutex;
    /// `std::sync::Mutex` is not reentrant and `agora_getNodeInfo` /
    /// `agora_getBlockTemplate` would futex-wait forever.
    tx_auth: TxAuthContext,
    /// Process-local Ethereum pending inbox, shared with raw-EVM gossip admit.
    /// Not part of the state root.
    pending_evm: Arc<Mutex<BTreeMap<[u8; 32], agora_ovl_evm::PendingTx>>>,
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
        let tx_auth = chain
            .lock()
            .ok()
            .and_then(|guard| guard.auth_context())
            .unwrap_or_else(|| fallback_tx_auth(&config.network, config.genesis_hash));
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
            tx_auth,
            pending_evm: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub(crate) fn pending_evm(&self) -> Arc<Mutex<BTreeMap<[u8; 32], agora_ovl_evm::PendingTx>>> {
        self.pending_evm.clone()
    }

    fn tx_auth(&self) -> TxAuthContext {
        self.tx_auth.clone()
    }

    fn data_commitment_from_block(
        &self,
        authorization_id: Hash,
        block_id: Hash,
        index: u32,
        lane_enabled: bool,
    ) -> Result<DataCommitmentLookup, RpcError> {
        let Some(block) = self.get_block(&block_id) else {
            return Ok(DataCommitmentLookup::unknown(
                authorization_id,
                lane_enabled,
            ));
        };
        let Some(authorization) = block.data_commitments.get(index as usize).cloned() else {
            return Ok(DataCommitmentLookup::unknown(
                authorization_id,
                lane_enabled,
            ));
        };
        let confirmations = self
            .chain
            .lock()
            .ok()
            .and_then(|g| g.confirmations(&block_id));
        let (finalized, pow_work_met) = match self.get_finality(&block_id) {
            Ok(value) => (
                value
                    .get("finalized")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                value
                    .get("pow_work_met")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            ),
            Err(_) => (false, false),
        };
        let accepted = load_data_commitment(
            self.store.as_ref(),
            authorization.commitment.source,
            authorization.commitment.sequence,
        )
        .map_err(|e| RpcError::Internal(e.to_string()))?;
        let (status, acceptance) = match accepted {
            Some(record)
                if record.authorization.authorization_id() == authorization.authorization_id() =>
            {
                let status = if finalized {
                    "finalized"
                } else if confirmations.is_some() {
                    "confirmed"
                } else {
                    "accepted"
                };
                (status, Some("Accepted".into()))
            }
            Some(_) => ("conflict_lost", Some("ConflictLost".into())),
            None => ("reverted", None),
        };
        Ok(DataCommitmentLookup {
            authorization_id,
            status: status.into(),
            block_id: Some(block_id),
            index: Some(index),
            confirmations,
            finalized,
            pow_work_met,
            acceptance,
            lane_enabled,
            authorization: Some(authorization),
        })
    }

    fn data_commitment_from_state(
        &self,
        source: DataCommitmentSource,
        sequence: u64,
        expected_id: Option<Hash>,
        lane_enabled: bool,
    ) -> Result<DataCommitmentLookup, RpcError> {
        let Some(record) = load_data_commitment(self.store.as_ref(), source, sequence)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        else {
            return Ok(DataCommitmentLookup::unknown(
                expected_id.unwrap_or(Hash::ZERO),
                lane_enabled,
            ));
        };
        let id = record.authorization.authorization_id();
        if let Some(expected) = expected_id {
            if expected != id {
                return Ok(DataCommitmentLookup {
                    authorization_id: expected,
                    status: "conflict_lost".into(),
                    block_id: Some(record.accepted_in),
                    index: None,
                    confirmations: None,
                    finalized: false,
                    pow_work_met: false,
                    acceptance: Some("ConflictLost".into()),
                    lane_enabled,
                    authorization: Some(record.authorization),
                });
            }
        }
        if let Some((block_id, index)) = lookup_data_commitment_location(self.store.as_ref(), &id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        {
            return self.data_commitment_from_block(id, block_id, index, lane_enabled);
        }
        Ok(DataCommitmentLookup {
            authorization_id: id,
            status: "accepted".into(),
            block_id: Some(record.accepted_in),
            index: None,
            confirmations: None,
            finalized: false,
            pow_work_met: false,
            acceptance: Some("Accepted".into()),
            lane_enabled,
            authorization: Some(record.authorization),
        })
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

    fn eth_node_view(&self) -> agora_ovl_evm::EthNodeView {
        agora_ovl_evm::EthNodeView {
            listening: self.net.is_some(),
            peer_count: u64::from(self.connected_peers.load(Ordering::Relaxed)),
            // This node has no IBD cursor. `None` is Ethereum `false`, not a
            // synthetic starting/current/highest triple.
            syncing: None,
            accept_raw_transactions: !self.network.eq_ignore_ascii_case("mainnet"),
        }
    }

    /// Place raw Ethereum envelopes from the local inbox and the raw gossip
    /// pool onto this node's template. Version 2 stays off the Agora-signed
    /// mempool. Mainnet labels skip this path.
    fn append_local_evm_executions(&self, executions: &mut Vec<OvlExecutionTx>) {
        if self.network.eq_ignore_ascii_case("mainnet") {
            return;
        }
        let Ok(world) = agora_state_machine::load_ovl_evm_world(&self.store) else {
            return;
        };
        if !world.active {
            return;
        }
        let Ok(pending) = self.pending_evm.lock() else {
            return;
        };
        let mut seen = std::collections::HashSet::new();
        for tx in pending.values() {
            if executions.len() >= DEFAULT_TEMPLATE_TX_LIMIT {
                return;
            }
            if agora_ovl_evm::measure_shanghai_gas(&world, &tx.raw).is_err() {
                continue;
            }
            seen.insert(tx.raw.clone());
            executions.push(OvlExecutionTx::raw_ethereum(tx.raw.clone()));
        }
        drop(pending);
        if let Ok(pool) = self.mempool.lock() {
            for tx in pool.select_ovl_raw_executions(DEFAULT_TEMPLATE_TX_LIMIT) {
                if executions.len() >= DEFAULT_TEMPLATE_TX_LIMIT {
                    break;
                }
                if seen.contains(&tx.data) {
                    continue;
                }
                if agora_ovl_evm::measure_shanghai_gas(&world, &tx.data).is_err() {
                    continue;
                }
                executions.push(tx);
            }
        }
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

    fn submit_tlt_covenant(&mut self, tx: TltCovenantTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_tlt_covenant(&self.store, &self.chain, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::TltCovenant(tx)) {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn get_tlt_covenant(&self, tx_id: &Hash) -> Result<TltCovenantLookup, RpcError> {
        {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            if let Some(tx) = pool.get_covenant(tx_id) {
                return Ok(TltCovenantLookup::pending(tx.clone(), pool.fee_of(tx_id)));
            }
        }
        let Some((block_id, index)) = lookup_covenant_tx_location(self.store.as_ref(), tx_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        else {
            return Ok(TltCovenantLookup::unknown(*tx_id));
        };
        let Some(block) = self.get_block(&block_id) else {
            return Ok(TltCovenantLookup::unknown(*tx_id));
        };
        let Some(tx) = block.tlt_covenants.get(index as usize) else {
            return Ok(TltCovenantLookup::unknown(*tx_id));
        };
        match self
            .chain
            .lock()
            .ok()
            .and_then(|g| g.confirmations(&block_id))
        {
            Some(confirmations) => Ok(TltCovenantLookup::confirmed(
                tx.clone(),
                block_id,
                index,
                confirmations,
            )),
            None => Ok(TltCovenantLookup::orphaned(tx.clone(), block_id, index)),
        }
    }

    fn submit_data_commitment(
        &mut self,
        authorization: DataCommitmentAuthorization,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_data_commitment(&self.store, &self.mempool, authorization.clone(), &auth)?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::DataCommitment(authorization)) {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn get_data_commitment(
        &self,
        authorization_id: Option<&Hash>,
        source: Option<DataCommitmentSource>,
        sequence: Option<u64>,
    ) -> Result<DataCommitmentLookup, RpcError> {
        let lane_enabled = self
            .tx_auth()
            .data_availability_network_fingerprint
            .filter(|fingerprint| *fingerprint != Hash::ZERO)
            .is_some();
        if let Some(id) = authorization_id {
            {
                let pool = self
                    .mempool
                    .lock()
                    .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
                if let Some(authorization) = pool.get_data_commitment(id) {
                    return Ok(DataCommitmentLookup::pending(
                        authorization.clone(),
                        lane_enabled,
                    ));
                }
            }
            if let Some((block_id, index)) =
                lookup_data_commitment_location(self.store.as_ref(), id)
                    .map_err(|e| RpcError::Internal(e.to_string()))?
            {
                return self.data_commitment_from_block(*id, block_id, index, lane_enabled);
            }
            if let (Some(source), Some(sequence)) = (source, sequence) {
                return self.data_commitment_from_state(source, sequence, Some(*id), lane_enabled);
            }
            return Ok(DataCommitmentLookup::unknown(*id, lane_enabled));
        }
        let Some(source) = source else {
            return Err(RpcError::InvalidParams(
                "authorization_id or source+sequence required".into(),
            ));
        };
        let Some(sequence) = sequence else {
            return Err(RpcError::InvalidParams(
                "authorization_id or source+sequence required".into(),
            ));
        };
        {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            if let Some(authorization) = pool.get_data_commitment_by_sequence(source, sequence) {
                return Ok(DataCommitmentLookup::pending(
                    authorization.clone(),
                    lane_enabled,
                ));
            }
        }
        self.data_commitment_from_state(source, sequence, None, lane_enabled)
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

    fn ovl_ethereum_rpc(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        let world = agora_state_machine::load_ovl_evm_world(&self.store)
            .map_err(|err| RpcError::Internal(err.to_string()))?;
        let mut pending = self
            .pending_evm
            .lock()
            .map_err(|_| RpcError::Internal("OVL EVM pending lock poisoned".into()))?;
        pending.retain(|hash, _| world.receipts.iter().all(|receipt| receipt.hash != *hash));
        let view = self.eth_node_view();
        let result = agora_ovl_evm::dispatch_with_view(&world, &mut pending, &view, method, params)
            .map_err(|err| match err {
                agora_ovl_evm::EvmError::MethodNotFound(method) => RpcError::MethodNotFound(method),
                agora_ovl_evm::EvmError::Rejected(message) => RpcError::Rejected(message),
            })?;
        if method.eq_ignore_ascii_case("eth_sendRawTransaction") {
            if let Some(hash_hex) = result.as_str() {
                if let Some(hash) = Hash::from_hex(hash_hex) {
                    if let Some(pending_tx) = pending.get(hash.as_bytes()) {
                        let envelope = OvlExecutionTx::raw_ethereum(pending_tx.raw.clone());
                        drop(pending);
                        let mut pool = self
                            .mempool
                            .lock()
                            .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
                        let _ = pool.admit_ovl_raw_execution(envelope.clone());
                        drop(pool);
                        if let Some(net) = &self.net {
                            let _ = net.publish_message(NetworkMessage::OvlRawExecution(envelope));
                        }
                        return Ok(result);
                    }
                }
            }
        }
        Ok(result)
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

    fn get_account_balances(&self, address: &Address) -> Result<AccountBalances, RpcError> {
        let tlt = self.utxo_balance(address)?.as_base_units();
        let ovl = load_account(self.store.as_ref(), NativeAssetId::OVL, address)
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        let drc = load_account(self.store.as_ref(), NativeAssetId::DRC, address)
            .map_err(|error| RpcError::Internal(error.to_string()))?;
        Ok(AccountBalances {
            tlt,
            ovl: ovl.balance,
            ovl_nonce: ovl.nonce,
            drc: drc.balance,
            drc_nonce: drc.nonce,
        })
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
            mut ovl_executions,
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
            tlt_covenants,
            data_commitments,
            passport_attestations,
            hub_registrations,
            grant_registrations,
            mission_registrations,
        ) = {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            let data_commitments = if self
                .tx_auth()
                .data_availability_network_fingerprint
                .filter(|fingerprint| *fingerprint != Hash::ZERO)
                .is_some()
            {
                pool.select_data_commitments(DEFAULT_TEMPLATE_TX_LIMIT)
            } else {
                Vec::new()
            };
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
                pool.select_covenants(DEFAULT_TEMPLATE_TX_LIMIT),
                data_commitments,
                pool.select_passport_attestations(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_hub_registrations(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_grant_registrations(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_mission_registrations(DEFAULT_TEMPLATE_TX_LIMIT),
            )
        };
        self.append_local_evm_executions(&mut ovl_executions);
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
                    tlt_covenants: &tlt_covenants,
                    data_commitments: &data_commitments,
                    passport_attestations: &passport_attestations,
                    hub_registrations: &hub_registrations,
                    grant_registrations: &grant_registrations,
                    mission_registrations: &mission_registrations,
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
        if let Ok(mut pending) = self.pending_evm.lock() {
            for tx in &block.ovl_executions {
                if tx.version != agora_types::OVL_EXECUTION_RAW_EVM_VERSION {
                    continue;
                }
                if let Ok(parsed) = agora_ovl_evm::parse_raw_transaction(&tx.data) {
                    pending.remove(&parsed.hash);
                }
            }
        }
        if let Some(net) = &self.net {
            // Gossip detached attachments before typed compact so peers can
            // inflate lane 29 from the mempool. A miss still issues GetBlock.
            for attachment in &block.drc_multisign_attachments {
                let _ =
                    net.publish_message(NetworkMessage::DrcMultisignAttachment(attachment.clone()));
            }
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
            "maturity": "Experimental",
            "consensus_mutations_active": true,
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

    fn submit_passport_attestation(
        &mut self,
        attestation: PassportAttestation,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id =
            admit_passport_attestation(&self.store, &self.mempool, attestation.clone(), &auth)?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::PassportAttestation(attestation))
            {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn get_passport_attestation(&self, attestation_id: &Hash) -> Result<Value, RpcError> {
        {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            if let Some(attestation) = pool.get_passport_attestation(attestation_id) {
                return Ok(json!({
                    "attestation_id": attestation_id.to_hex(),
                    "status": "pending",
                    "attestation": attestation,
                }));
            }
        }
        match load_passport_attestation(self.store.as_ref(), attestation_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        {
            Some(attestation) => Ok(json!({
                "attestation_id": attestation_id.to_hex(),
                "status": "accepted",
                "attestation": attestation,
            })),
            None => Ok(json!({
                "attestation_id": attestation_id.to_hex(),
                "status": "unknown",
                "attestation": null,
            })),
        }
    }

    fn get_passport_issuer_nonce(&self, issuer: &Address) -> Result<Value, RpcError> {
        let nonce = load_passport_issuer_nonce(self.store.as_ref(), issuer)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "issuer": issuer.to_hex(),
            "nonce": nonce,
        }))
    }

    fn submit_hub_registration(&mut self, registration: HubRegistration) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_hub_registration(&self.store, &self.mempool, registration.clone(), &auth)?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::HubRegistration(registration)) {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn get_hub_registration(&self, registration_id: &Hash) -> Result<Value, RpcError> {
        {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            if let Some(registration) = pool.get_hub_registration(registration_id) {
                return Ok(json!({
                    "registration_id": registration_id.to_hex(),
                    "status": "pending",
                    "hub": registration,
                }));
            }
        }
        match load_hub_registration(self.store.as_ref(), registration_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        {
            Some(hub) => Ok(json!({
                "registration_id": registration_id.to_hex(),
                "status": "accepted",
                "hub": hub,
            })),
            None => Ok(json!({
                "registration_id": registration_id.to_hex(),
                "status": "unknown",
                "hub": null,
            })),
        }
    }

    fn get_hub_coordinator_nonce(&self, coordinator: &Address) -> Result<Value, RpcError> {
        let nonce = load_hub_coordinator_nonce(self.store.as_ref(), coordinator)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "coordinator": coordinator.to_hex(),
            "nonce": nonce,
        }))
    }

    fn submit_grant_registration(
        &mut self,
        registration: GrantRegistration,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_grant_registration(&self.store, &self.mempool, registration.clone(), &auth)?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::GrantRegistration(registration)) {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn get_grant_registration(&self, registration_id: &Hash) -> Result<Value, RpcError> {
        {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            if let Some(registration) = pool.get_grant_registration(registration_id) {
                return Ok(json!({
                    "registration_id": registration_id.to_hex(),
                    "status": "pending",
                    "grant": registration,
                }));
            }
        }
        match load_grant_registration(self.store.as_ref(), registration_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        {
            Some(grant) => Ok(json!({
                "registration_id": registration_id.to_hex(),
                "status": "accepted",
                "grant": grant,
            })),
            None => Ok(json!({
                "registration_id": registration_id.to_hex(),
                "status": "unknown",
                "grant": null,
            })),
        }
    }

    fn get_grant_registrar_nonce(&self, registrar: &Address) -> Result<Value, RpcError> {
        let nonce = load_grant_registrar_nonce(self.store.as_ref(), registrar)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "registrar": registrar.to_hex(),
            "nonce": nonce,
        }))
    }

    fn submit_mission_registration(
        &mut self,
        registration: MissionRegistration,
    ) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id =
            admit_mission_registration(&self.store, &self.mempool, registration.clone(), &auth)?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::MissionRegistration(registration))
            {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn get_mission_registration(&self, registration_id: &Hash) -> Result<Value, RpcError> {
        {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            if let Some(registration) = pool.get_mission_registration(registration_id) {
                return Ok(json!({
                    "registration_id": registration_id.to_hex(),
                    "status": "pending",
                    "mission": registration,
                }));
            }
        }
        match load_mission_registration(self.store.as_ref(), registration_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        {
            Some(mission) => Ok(json!({
                "registration_id": registration_id.to_hex(),
                "status": "accepted",
                "mission": mission,
            })),
            None => Ok(json!({
                "registration_id": registration_id.to_hex(),
                "status": "unknown",
                "mission": null,
            })),
        }
    }

    fn get_mission_sponsor_nonce(&self, sponsor: &Address) -> Result<Value, RpcError> {
        let nonce = load_mission_sponsor_nonce(self.store.as_ref(), sponsor)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "sponsor": sponsor.to_hex(),
            "nonce": nonce,
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
        assert_eq!(value["maturity"], "Experimental");
        assert_eq!(value["consensus_mutations_active"], true);
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
    fn public_ethereum_rpc_reads_stored_world_and_keeps_drc_and_tlt() {
        use agora_rpc::{RpcDispatcher, RpcRequest};
        use agora_state_machine::{load_account, load_ovl_evm_world, put_ovl_evm_world_into};
        use agora_types::OvlWei;
        use serde_json::json;

        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let drc_addr = Address([4; 20]);
        let tlt_addr = Address([5; 20]);
        let tlt_out = TxOut {
            value: Amount::from_base_units(40),
            address: tlt_addr,
        };
        let mut utxo_key = vec![0x71; 32];
        utxo_key.extend_from_slice(&0u32.to_le_bytes());
        let utxo_val = borsh::to_vec(&tlt_out).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &drc_addr,
            Amount::from_base_units(25),
        )
        .unwrap();
        funding.put_cf(ColumnFamily::Utxo, &utxo_key, &utxo_val);
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
        let mempool = Arc::new(Mutex::new(Mempool::new(8)));
        let mut config = backend_config(genesis);
        config.connected_peers = Arc::new(AtomicU32::new(4));
        let backend = NodeBackend::new(chain, store.clone(), mempool, config);
        let mut rpc = RpcDispatcher::new(backend);

        let inactive = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "eth_chainId".into(),
            params: json!([]),
        });
        assert_eq!(inactive.jsonrpc, "2.0");
        assert!(inactive.result.is_none());
        assert!(inactive.error.unwrap().message.contains("dev gate"));

        let proof = rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_getProof", &json!([]))
            .unwrap_err();
        assert!(matches!(proof, RpcError::MethodNotFound(_)));

        let key = agora_ovl_evm::dev_signing_key();
        let caller = agora_ovl_evm::ethereum_address_from_signing_key(&key);
        let caller_hex = format!("0x{}", hex_bytes(&caller));
        let mut world = agora_ovl_evm::OvlEvmWorld::dev();
        let funded = OvlWei::from_u128(10u128.pow(18));
        world.fund(caller, funded).unwrap();
        let mut activate = WriteBatch::new();
        put_ovl_evm_world_into(&mut activate, &world);
        store.write_batch(activate).unwrap();
        let subroot = load_ovl_evm_world(&store).unwrap().execution_subroot();

        let chain_id = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "eth_chainId".into(),
            params: json!([]),
        });
        assert_eq!(chain_id.result.unwrap(), json!("0x12110"));
        let balance = rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_getBalance", &json!([&caller_hex, "latest"]))
            .unwrap();
        assert_eq!(
            balance,
            json!(format!("0x{}", hex_bytes(&funded.to_be_bytes())))
        );
        let code = rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_getCode", &json!([&caller_hex, "0x0"]))
            .unwrap();
        assert_eq!(code, json!("0x"));
        let storage = rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_getStorageAt", &json!([&caller_hex, "0x0", "latest"]))
            .unwrap();
        assert_eq!(storage, json!(format!("0x{}", "00".repeat(32))));
        let nonce = rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_getTransactionCount", &json!([&caller_hex]))
            .unwrap();
        assert_eq!(nonce, json!("0x0"));
        let call = rpc
            .backend_mut()
            .ovl_ethereum_rpc(
                "eth_call",
                &json!([{ "from": &caller_hex, "to": "0x000000000000000000000000000000000000000a", "data": "0x" }]),
            )
            .unwrap();
        assert_eq!(call, json!("0x"));
        let gas = rpc
            .backend_mut()
            .ovl_ethereum_rpc(
                "eth_estimateGas",
                &json!([{ "from": &caller_hex, "to": "0x000000000000000000000000000000000000000a" }]),
            )
            .unwrap();
        assert_eq!(gas, json!("0x5208"));
        assert_eq!(
            rpc.backend_mut()
                .ovl_ethereum_rpc("net_peerCount", &json!([]))
                .unwrap(),
            json!("0x4")
        );
        assert_eq!(
            rpc.backend_mut()
                .ovl_ethereum_rpc("net_listening", &json!([]))
                .unwrap(),
            json!(false)
        );
        assert_eq!(
            rpc.backend_mut()
                .ovl_ethereum_rpc("eth_syncing", &json!([]))
                .unwrap(),
            json!(false)
        );
        let version = rpc
            .backend_mut()
            .ovl_ethereum_rpc("web3_clientVersion", &json!([]))
            .unwrap();
        assert!(version.as_str().unwrap().contains("OVL-EVM-v1"));
        let history = rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_feeHistory", &json!([8, "latest", []]))
            .unwrap();
        assert_eq!(history["gasUsedRatio"].as_array().unwrap().len(), 1);
        assert!(rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_call", &json!([{ "asset": "DRC" }]))
            .unwrap_err()
            .to_string()
            .contains("DRC"));
        assert!(rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_getBalance", &json!([{ "asset": "TLT" }]))
            .unwrap_err()
            .to_string()
            .contains("TLT"));
        assert!(rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_sendRawTransaction", &json!(["0x03"]))
            .unwrap_err()
            .to_string()
            .contains("blob"));
        assert!(rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_getBalance", &json!([&caller_hex, "0x5"]))
            .unwrap_err()
            .to_string()
            .contains("historical"));
        assert!(rpc
            .backend_mut()
            .ovl_ethereum_rpc("eth_getBlockByNumber", &json!(["0x9", false]))
            .unwrap()
            .is_null());

        let raw = agora_ovl_evm::sign_eip1559(
            &key,
            world.chain_id,
            0,
            0,
            u128::from(world.block.base_fee),
            21_000,
            Some([0x44; 20]),
            {
                let mut value = [0u8; 32];
                value[31] = 1;
                value
            },
            &[],
            &[],
        );
        let sent = rpc
            .backend_mut()
            .ovl_ethereum_rpc(
                "eth_sendRawTransaction",
                &json!([format!("0x{}", hex_bytes(&raw))]),
            )
            .unwrap();
        assert!(sent.as_str().unwrap().starts_with("0x"));
        assert_eq!(
            load_ovl_evm_world(&store).unwrap().execution_subroot(),
            subroot
        );
        assert!(rpc
            .backend()
            .mempool
            .lock()
            .unwrap()
            .select_ovl_executions(8)
            .is_empty());
        assert_eq!(
            rpc.backend()
                .mempool
                .lock()
                .unwrap()
                .select_ovl_raw_executions(8)
                .len(),
            1
        );
        let template = rpc.backend().get_block_template().unwrap();
        assert_eq!(template.ovl_executions.len(), 1);
        assert_eq!(template.ovl_executions[0].version, 2);
        assert_eq!(template.ovl_executions[0].data, raw);
        assert_eq!(
            rpc.backend_mut()
                .ovl_ethereum_rpc(
                    "eth_getBalance",
                    &json!([format!("0x{}", hex_bytes(&drc_addr.0)), "latest"]),
                )
                .unwrap(),
            json!(format!("0x{}", "00".repeat(32)))
        );
        assert_eq!(
            rpc.backend_mut()
                .ovl_ethereum_rpc(
                    "eth_getBalance",
                    &json!([format!("0x{}", hex_bytes(&tlt_addr.0)), "latest"]),
                )
                .unwrap(),
            json!(format!("0x{}", "00".repeat(32)))
        );
        assert_eq!(
            load_account(store.as_ref(), NativeAssetId::DRC, &drc_addr)
                .unwrap()
                .balance,
            25
        );
        assert_eq!(
            store
                .get_cf(ColumnFamily::Utxo, &utxo_key)
                .unwrap()
                .unwrap(),
            utxo_val
        );

        let mut mainnet_config = backend_config(genesis);
        mainnet_config.network = "mainnet".into();
        let mut mainnet = NodeBackend::new(
            rpc.backend().chain.clone(),
            rpc.backend().store.clone(),
            Arc::new(Mutex::new(Mempool::new(4))),
            mainnet_config,
        );
        let rejected = mainnet
            .ovl_ethereum_rpc("eth_sendRawTransaction", &json!(["0x02c0"]))
            .unwrap_err();
        assert!(rejected.to_string().contains("dev and test"));
    }

    fn hex_bytes(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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
    fn account_balance_rpc_reads_tlt_utxo_and_ovl_drc_accounts() {
        use agora_rpc::{RpcDispatcher, RpcRequest};
        use serde_json::json;

        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let addr = Address([6; 20]);
        let tlt_out = TxOut {
            value: Amount::from_base_units(40),
            address: addr,
        };
        let mut utxo_key = vec![0x71; 32];
        utxo_key.extend_from_slice(&0u32.to_le_bytes());
        let utxo_val = borsh::to_vec(&tlt_out).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::OVL,
            &addr,
            Amount::from_base_units(11),
        )
        .unwrap();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &addr,
            Amount::from_base_units(25),
        )
        .unwrap();
        funding.put_cf(ColumnFamily::Utxo, &utxo_key, &utxo_val);
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
        let mempool = Arc::new(Mutex::new(Mempool::new(8)));
        let backend = NodeBackend::new(chain, store, mempool, backend_config(genesis));
        let mut rpc = RpcDispatcher::new(backend);

        let tlt_only = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "agora_getBalance".into(),
            params: json!({"address": addr.to_bech32()}),
        });
        assert_eq!(tlt_only.result.unwrap()["balance"], json!(40));

        let accounts = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "agora_getAccountBalances".into(),
            params: json!({"address": addr.to_bech32()}),
        });
        let accounts_res = accounts.result.unwrap();
        assert_eq!(accounts_res["address"], json!(addr.to_bech32()));
        assert_eq!(accounts_res["tlt"]["balance"], json!(40));
        assert_eq!(accounts_res["ovl"], json!({"balance": 11, "nonce": 0}));
        assert_eq!(accounts_res["drc"], json!({"balance": 25, "nonce": 0}));
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

    #[test]
    fn tlt_covenant_submit_query_template_and_restart() {
        use agora_consensus::EmissionSchedule;
        use agora_crypto::sign_tlt_covenant_preimage;
        use agora_rpc::TxStatus;
        use agora_types::{
            prove_tlt_tx_merkle, push_data, script_p2pkh, tlt_tx_merkle_root, verify_tlt_tx_merkle,
            TltCovenantInput, TltCovenantOutput, TltCovenantTx, TLT_COVENANT_TX_VERSION,
            TLT_SEQUENCE_FINAL,
        };

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
        let premine = genesis_block.transactions[0].outputs[0]
            .value
            .as_base_units();
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
            chain.clone(),
            store.clone(),
            mempool,
            NodeBackendConfig {
                miner_address: Address([1; 20]),
                ..backend_config(genesis)
            },
        );

        let mut cheap = TltCovenantTx {
            version: TLT_COVENANT_TX_VERSION,
            inputs: vec![TltCovenantInput {
                previous_outpoint: OutPoint {
                    tx_id: genesis_block.transactions[0].tx_id(),
                    index: 0,
                },
                sequence: TLT_SEQUENCE_FINAL - 1,
                script_sig: Vec::new(),
            }],
            outputs: vec![TltCovenantOutput {
                value: Amount::from_base_units(premine),
                script_pubkey: script_p2pkh(&to),
            }],
            lock_time: 0,
            nonce: 1,
        };
        let cheap_preimage = cheap.sighash_preimage_bound("agora-dev", &genesis);
        let cheap_sig = sign_tlt_covenant_preimage(&from, &cheap_preimage).unwrap();
        let mut cheap_script = Vec::new();
        push_data(&mut cheap_script, &cheap_sig).unwrap();
        push_data(&mut cheap_script, &from.public_key_bytes()).unwrap();
        cheap.inputs[0].script_sig = cheap_script;
        assert!(backend.submit_tlt_covenant(cheap).is_err());

        let fee = 1u64;
        let mut tx = TltCovenantTx {
            version: TLT_COVENANT_TX_VERSION,
            inputs: vec![TltCovenantInput {
                previous_outpoint: OutPoint {
                    tx_id: genesis_block.transactions[0].tx_id(),
                    index: 0,
                },
                sequence: TLT_SEQUENCE_FINAL - 1,
                script_sig: Vec::new(),
            }],
            outputs: vec![TltCovenantOutput {
                value: Amount::from_base_units(premine - fee),
                script_pubkey: script_p2pkh(&to),
            }],
            lock_time: 0,
            nonce: 2,
        };
        let preimage = tx.sighash_preimage_bound("agora-dev", &genesis);
        let signature = sign_tlt_covenant_preimage(&from, &preimage).unwrap();
        let mut script_sig = Vec::new();
        push_data(&mut script_sig, &signature).unwrap();
        push_data(&mut script_sig, &from.public_key_bytes()).unwrap();
        tx.inputs[0].script_sig = script_sig;
        let id = backend.submit_tlt_covenant(tx.clone()).unwrap();
        assert_eq!(id, tx.tx_id());
        let pending = backend.get_tlt_covenant(&id).unwrap();
        assert_eq!(pending.status, TxStatus::Pending);
        assert_eq!(pending.fee, Some(fee));

        let mut replacement = tx.clone();
        replacement.nonce = 3;
        replacement.outputs[0].value = Amount::from_base_units(premine - 2);
        replacement.inputs[0].script_sig.clear();
        let replacement_preimage = replacement.sighash_preimage_bound("agora-dev", &genesis);
        let replacement_sig = sign_tlt_covenant_preimage(&from, &replacement_preimage).unwrap();
        let mut replacement_script = Vec::new();
        push_data(&mut replacement_script, &replacement_sig).unwrap();
        push_data(&mut replacement_script, &from.public_key_bytes()).unwrap();
        replacement.inputs[0].script_sig = replacement_script;
        let replaced = backend.submit_tlt_covenant(replacement.clone()).unwrap();
        assert!(backend.get_tlt_covenant(&id).unwrap().transaction.is_none());
        assert_eq!(
            backend.get_tlt_covenant(&replaced).unwrap().status,
            TxStatus::Pending
        );

        let mut template = backend.get_block_template().unwrap();
        assert_eq!(template.tlt_covenants.len(), 1);
        assert_eq!(template.tlt_covenants[0].tx_id(), replaced);
        assert_eq!(
            template.transactions[0].outputs[0].value.as_base_units(),
            EmissionSchedule::default().initial_reward + 2
        );
        let leaves: Vec<_> = template.transactions.iter().map(|tx| tx.tx_id()).collect();
        let merkle = tlt_tx_merkle_root(&leaves);
        assert_eq!(merkle, Block::compute_tx_root(&template.transactions));
        assert_ne!(template.header.tx_root, merkle);
        let proof = prove_tlt_tx_merkle(&leaves, 0).unwrap();
        assert!(verify_tlt_tx_merkle(&merkle, &proof));

        template.header.nonce = 1;
        let pow = RandomXPowHasher.pow_hash(&template.header);
        agora_consensus::LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&template.header, &pow)
            .unwrap();
        let block_id = backend.submit_block(template.clone()).unwrap();
        let confirmed = backend.get_tlt_covenant(&replaced).unwrap();
        assert_eq!(confirmed.status, TxStatus::Confirmed);
        assert_eq!(confirmed.block_id, Some(block_id));
        assert_eq!(backend.get_balance(&to).as_base_units(), premine - 2);
        assert!(backend.submit_tlt_covenant(replacement).is_err());

        let restarted = ChainState::bootstrap_with(
            store,
            genesis,
            crate::admit::ChainBootConfig {
                chain_id: "agora-dev".into(),
                ..crate::admit::ChainBootConfig::default()
            },
            crate::storage_policy::StoragePolicy::default(),
        )
        .unwrap();
        let reloaded = restarted.load_block(&block_id).unwrap().unwrap();
        assert_eq!(reloaded.tlt_covenants[0].tx_id(), replaced);
        assert_eq!(reloaded.header.tx_root, template.header.tx_root);
    }

    #[test]
    fn data_commitment_submit_requires_fingerprint_and_tlt_fee() {
        let store = Arc::new(StateStore::open_in_memory());
        let operator = agora_crypto::KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let genesis = GenesisBuilder::default()
            .with_premine_address(operator.address())
            .ignite(&store)
            .unwrap();
        let fingerprint = Hash([9; 32]);
        let disabled = Arc::new(Mutex::new(
            ChainState::bootstrap_with(
                store.clone(),
                genesis,
                crate::admit::ChainBootConfig {
                    chain_id: "agora-trident-testnet-1".into(),
                    ..crate::admit::ChainBootConfig::default()
                },
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut disabled_backend = NodeBackend::new(
            disabled,
            store.clone(),
            Arc::new(Mutex::new(Mempool::new(8))),
            backend_config(genesis),
        );
        let commitment = agora_types::DataAvailabilityCommitment::agora_layers_ovolos_batch(
            "agora-ovolos-testnet-1".into(),
            Hash([1; 32]),
            Hash([11; 32]),
            4,
            Hash([3; 32]),
            Hash([12; 32]),
            Hash([5; 32]),
            6,
            7,
        );
        let mut authorization =
            DataCommitmentAuthorization::unsigned(operator.address(), 0, commitment);
        agora_crypto::sign_data_commitment_bound(
            &mut authorization,
            &operator,
            "agora-trident-testnet-1",
            &genesis,
            &fingerprint,
        )
        .unwrap();
        let err = disabled_backend
            .submit_data_commitment(authorization.clone())
            .unwrap_err();
        assert!(err.to_string().contains("DA network fingerprint"));

        let enabled = Arc::new(Mutex::new(
            ChainState::bootstrap_with(
                store.clone(),
                genesis,
                crate::admit::ChainBootConfig {
                    chain_id: "agora-trident-testnet-1".into(),
                    data_availability_network_fingerprint: Some(fingerprint),
                    ..crate::admit::ChainBootConfig::default()
                },
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut enabled_backend = NodeBackend::new(
            enabled,
            store,
            Arc::new(Mutex::new(Mempool::new(8))),
            backend_config(genesis),
        );
        let id = enabled_backend
            .submit_data_commitment(authorization.clone())
            .unwrap();
        assert_eq!(id, authorization.authorization_id());
        let lookup = enabled_backend
            .get_data_commitment(Some(&id), None, None)
            .unwrap();
        assert_eq!(lookup.status, "pending");
        assert!(lookup.lane_enabled);
        assert_eq!(lookup.finalized, false);
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
