//! Explicit transaction acceptance records for the Virtual apply path.
//!
//! Soft-skip under [`crate::ApplyMode::Virtual`] remains the conflict-resolution
//! engine; this module makes its outcomes durable and typed so fees, confirmations,
//! mempool eviction, and explorers never treat “blue” as “accepted.”

use agora_types::{AcceptanceBitmap, Hash, TransactionAcceptance};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const ACCEPTANCE_PREFIX: &[u8] = b"acceptance/";

/// Per-block acceptance outcomes for multi-lane Trident bodies.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Default)]
pub struct BlockAcceptanceRecord {
    pub block_hash: Hash,
    /// Aligned to `block.transactions` indices (TLT UTXO lane).
    pub statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.account_transfers`.
    pub account_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.stake_ops`.
    pub stake_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.ovl_executions`.
    pub execution_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_payments`.
    pub payment_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.data_commitments`.
    pub data_commitment_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_account_policies`.
    pub drc_policy_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_deposit_preauths`.
    pub drc_deposit_preauth_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_regular_keys`.
    pub drc_regular_key_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_signer_lists`.
    pub drc_signer_list_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_ticket_creates`.
    pub drc_ticket_create_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_escrow_creates`.
    pub drc_escrow_create_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_escrow_finishes`.
    pub drc_escrow_finish_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_escrow_cancels`.
    pub drc_escrow_cancel_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_check_creates`.
    pub drc_check_create_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_check_cashes`.
    pub drc_check_cash_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_check_cancels`.
    pub drc_check_cancel_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_payment_channel_creates`.
    pub drc_payment_channel_create_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_payment_channel_funds`.
    pub drc_payment_channel_fund_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_payment_channel_claims`.
    pub drc_payment_channel_claim_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_payment_channel_closes`.
    pub drc_payment_channel_close_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_trust_line_sets`.
    pub drc_trust_line_set_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_issued_transfers`.
    pub drc_issued_transfer_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_issued_asset_policy_sets`.
    pub drc_issued_asset_policy_set_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_trust_line_issuer_controls`.
    pub drc_trust_line_issuer_control_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_issued_clawbacks`.
    pub drc_issued_clawback_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_offer_creates`.
    pub drc_offer_create_statuses: Vec<TransactionAcceptance>,
    /// Aligned to `block.drc_offer_cancels`.
    pub drc_offer_cancel_statuses: Vec<TransactionAcceptance>,
}

/// Acceptance layout before the offer lanes.
#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV17IndexAcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
    drc_policy_statuses: Vec<TransactionAcceptance>,
    drc_deposit_preauth_statuses: Vec<TransactionAcceptance>,
    drc_regular_key_statuses: Vec<TransactionAcceptance>,
    drc_signer_list_statuses: Vec<TransactionAcceptance>,
    drc_ticket_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_finish_statuses: Vec<TransactionAcceptance>,
    drc_escrow_cancel_statuses: Vec<TransactionAcceptance>,
    drc_check_create_statuses: Vec<TransactionAcceptance>,
    drc_check_cash_statuses: Vec<TransactionAcceptance>,
    drc_check_cancel_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_create_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_fund_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_claim_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_close_statuses: Vec<TransactionAcceptance>,
    drc_trust_line_set_statuses: Vec<TransactionAcceptance>,
    drc_issued_transfer_statuses: Vec<TransactionAcceptance>,
    drc_issued_asset_policy_set_statuses: Vec<TransactionAcceptance>,
    drc_trust_line_issuer_control_statuses: Vec<TransactionAcceptance>,
    drc_issued_clawback_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct LegacyBlockAcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV2AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV3AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV4AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV5AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV6AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
    drc_policy_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV7AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
    drc_policy_statuses: Vec<TransactionAcceptance>,
    drc_deposit_preauth_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV8AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
    drc_policy_statuses: Vec<TransactionAcceptance>,
    drc_deposit_preauth_statuses: Vec<TransactionAcceptance>,
    drc_regular_key_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV9AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
    drc_policy_statuses: Vec<TransactionAcceptance>,
    drc_deposit_preauth_statuses: Vec<TransactionAcceptance>,
    drc_regular_key_statuses: Vec<TransactionAcceptance>,
    drc_signer_list_statuses: Vec<TransactionAcceptance>,
    drc_ticket_create_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV10AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
    drc_policy_statuses: Vec<TransactionAcceptance>,
    drc_deposit_preauth_statuses: Vec<TransactionAcceptance>,
    drc_regular_key_statuses: Vec<TransactionAcceptance>,
    drc_signer_list_statuses: Vec<TransactionAcceptance>,
    drc_ticket_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_finish_statuses: Vec<TransactionAcceptance>,
    drc_escrow_cancel_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV14AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
    drc_policy_statuses: Vec<TransactionAcceptance>,
    drc_deposit_preauth_statuses: Vec<TransactionAcceptance>,
    drc_regular_key_statuses: Vec<TransactionAcceptance>,
    drc_signer_list_statuses: Vec<TransactionAcceptance>,
    drc_ticket_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_finish_statuses: Vec<TransactionAcceptance>,
    drc_escrow_cancel_statuses: Vec<TransactionAcceptance>,
    drc_check_create_statuses: Vec<TransactionAcceptance>,
    drc_check_cash_statuses: Vec<TransactionAcceptance>,
    drc_check_cancel_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_create_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_fund_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_claim_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_close_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV13AcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
    drc_policy_statuses: Vec<TransactionAcceptance>,
    drc_deposit_preauth_statuses: Vec<TransactionAcceptance>,
    drc_regular_key_statuses: Vec<TransactionAcceptance>,
    drc_signer_list_statuses: Vec<TransactionAcceptance>,
    drc_ticket_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_finish_statuses: Vec<TransactionAcceptance>,
    drc_escrow_cancel_statuses: Vec<TransactionAcceptance>,
    drc_check_create_statuses: Vec<TransactionAcceptance>,
    drc_check_cash_statuses: Vec<TransactionAcceptance>,
    drc_check_cancel_statuses: Vec<TransactionAcceptance>,
}

#[derive(Debug, Clone, BorshDeserialize)]
struct MultiLaneV16TrustAcceptanceRecord {
    block_hash: Hash,
    statuses: Vec<TransactionAcceptance>,
    account_statuses: Vec<TransactionAcceptance>,
    stake_statuses: Vec<TransactionAcceptance>,
    execution_statuses: Vec<TransactionAcceptance>,
    payment_statuses: Vec<TransactionAcceptance>,
    data_commitment_statuses: Vec<TransactionAcceptance>,
    drc_policy_statuses: Vec<TransactionAcceptance>,
    drc_deposit_preauth_statuses: Vec<TransactionAcceptance>,
    drc_regular_key_statuses: Vec<TransactionAcceptance>,
    drc_signer_list_statuses: Vec<TransactionAcceptance>,
    drc_ticket_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_create_statuses: Vec<TransactionAcceptance>,
    drc_escrow_finish_statuses: Vec<TransactionAcceptance>,
    drc_escrow_cancel_statuses: Vec<TransactionAcceptance>,
    drc_check_create_statuses: Vec<TransactionAcceptance>,
    drc_check_cash_statuses: Vec<TransactionAcceptance>,
    drc_check_cancel_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_create_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_fund_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_claim_statuses: Vec<TransactionAcceptance>,
    drc_payment_channel_close_statuses: Vec<TransactionAcceptance>,
    drc_trust_line_set_statuses: Vec<TransactionAcceptance>,
    drc_issued_transfer_statuses: Vec<TransactionAcceptance>,
}

impl BlockAcceptanceRecord {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StateError> {
        if let Ok(rec) = Self::try_from_slice(bytes) {
            return Ok(rec);
        }
        if let Ok(v17) = MultiLaneV17IndexAcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v17.block_hash,
                statuses: v17.statuses,
                account_statuses: v17.account_statuses,
                stake_statuses: v17.stake_statuses,
                execution_statuses: v17.execution_statuses,
                payment_statuses: v17.payment_statuses,
                data_commitment_statuses: v17.data_commitment_statuses,
                drc_policy_statuses: v17.drc_policy_statuses,
                drc_deposit_preauth_statuses: v17.drc_deposit_preauth_statuses,
                drc_regular_key_statuses: v17.drc_regular_key_statuses,
                drc_signer_list_statuses: v17.drc_signer_list_statuses,
                drc_ticket_create_statuses: v17.drc_ticket_create_statuses,
                drc_escrow_create_statuses: v17.drc_escrow_create_statuses,
                drc_escrow_finish_statuses: v17.drc_escrow_finish_statuses,
                drc_escrow_cancel_statuses: v17.drc_escrow_cancel_statuses,
                drc_check_create_statuses: v17.drc_check_create_statuses,
                drc_check_cash_statuses: v17.drc_check_cash_statuses,
                drc_check_cancel_statuses: v17.drc_check_cancel_statuses,
                drc_payment_channel_create_statuses: v17.drc_payment_channel_create_statuses,
                drc_payment_channel_fund_statuses: v17.drc_payment_channel_fund_statuses,
                drc_payment_channel_claim_statuses: v17.drc_payment_channel_claim_statuses,
                drc_payment_channel_close_statuses: v17.drc_payment_channel_close_statuses,
                drc_trust_line_set_statuses: v17.drc_trust_line_set_statuses,
                drc_issued_transfer_statuses: v17.drc_issued_transfer_statuses,
                drc_issued_asset_policy_set_statuses: v17.drc_issued_asset_policy_set_statuses,
                drc_trust_line_issuer_control_statuses: v17.drc_trust_line_issuer_control_statuses,
                drc_issued_clawback_statuses: v17.drc_issued_clawback_statuses,
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v16) = MultiLaneV16TrustAcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v16.block_hash,
                statuses: v16.statuses,
                account_statuses: v16.account_statuses,
                stake_statuses: v16.stake_statuses,
                execution_statuses: v16.execution_statuses,
                payment_statuses: v16.payment_statuses,
                data_commitment_statuses: v16.data_commitment_statuses,
                drc_policy_statuses: v16.drc_policy_statuses,
                drc_deposit_preauth_statuses: v16.drc_deposit_preauth_statuses,
                drc_regular_key_statuses: v16.drc_regular_key_statuses,
                drc_signer_list_statuses: v16.drc_signer_list_statuses,
                drc_ticket_create_statuses: v16.drc_ticket_create_statuses,
                drc_escrow_create_statuses: v16.drc_escrow_create_statuses,
                drc_escrow_finish_statuses: v16.drc_escrow_finish_statuses,
                drc_escrow_cancel_statuses: v16.drc_escrow_cancel_statuses,
                drc_check_create_statuses: v16.drc_check_create_statuses,
                drc_check_cash_statuses: v16.drc_check_cash_statuses,
                drc_check_cancel_statuses: v16.drc_check_cancel_statuses,
                drc_payment_channel_create_statuses: v16.drc_payment_channel_create_statuses,
                drc_payment_channel_fund_statuses: v16.drc_payment_channel_fund_statuses,
                drc_payment_channel_claim_statuses: v16.drc_payment_channel_claim_statuses,
                drc_payment_channel_close_statuses: v16.drc_payment_channel_close_statuses,
                drc_trust_line_set_statuses: v16.drc_trust_line_set_statuses,
                drc_issued_transfer_statuses: v16.drc_issued_transfer_statuses,
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v14) = MultiLaneV14AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v14.block_hash,
                statuses: v14.statuses,
                account_statuses: v14.account_statuses,
                stake_statuses: v14.stake_statuses,
                execution_statuses: v14.execution_statuses,
                payment_statuses: v14.payment_statuses,
                data_commitment_statuses: v14.data_commitment_statuses,
                drc_policy_statuses: v14.drc_policy_statuses,
                drc_deposit_preauth_statuses: v14.drc_deposit_preauth_statuses,
                drc_regular_key_statuses: v14.drc_regular_key_statuses,
                drc_signer_list_statuses: v14.drc_signer_list_statuses,
                drc_ticket_create_statuses: v14.drc_ticket_create_statuses,
                drc_escrow_create_statuses: v14.drc_escrow_create_statuses,
                drc_escrow_finish_statuses: v14.drc_escrow_finish_statuses,
                drc_escrow_cancel_statuses: v14.drc_escrow_cancel_statuses,
                drc_check_create_statuses: v14.drc_check_create_statuses,
                drc_check_cash_statuses: v14.drc_check_cash_statuses,
                drc_check_cancel_statuses: v14.drc_check_cancel_statuses,
                drc_payment_channel_create_statuses: v14.drc_payment_channel_create_statuses,
                drc_payment_channel_fund_statuses: v14.drc_payment_channel_fund_statuses,
                drc_payment_channel_claim_statuses: v14.drc_payment_channel_claim_statuses,
                drc_payment_channel_close_statuses: v14.drc_payment_channel_close_statuses,
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v13) = MultiLaneV13AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v13.block_hash,
                statuses: v13.statuses,
                account_statuses: v13.account_statuses,
                stake_statuses: v13.stake_statuses,
                execution_statuses: v13.execution_statuses,
                payment_statuses: v13.payment_statuses,
                data_commitment_statuses: v13.data_commitment_statuses,
                drc_policy_statuses: v13.drc_policy_statuses,
                drc_deposit_preauth_statuses: v13.drc_deposit_preauth_statuses,
                drc_regular_key_statuses: v13.drc_regular_key_statuses,
                drc_signer_list_statuses: v13.drc_signer_list_statuses,
                drc_ticket_create_statuses: v13.drc_ticket_create_statuses,
                drc_escrow_create_statuses: v13.drc_escrow_create_statuses,
                drc_escrow_finish_statuses: v13.drc_escrow_finish_statuses,
                drc_escrow_cancel_statuses: v13.drc_escrow_cancel_statuses,
                drc_check_create_statuses: v13.drc_check_create_statuses,
                drc_check_cash_statuses: v13.drc_check_cash_statuses,
                drc_check_cancel_statuses: v13.drc_check_cancel_statuses,
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v10) = MultiLaneV10AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v10.block_hash,
                statuses: v10.statuses,
                account_statuses: v10.account_statuses,
                stake_statuses: v10.stake_statuses,
                execution_statuses: v10.execution_statuses,
                payment_statuses: v10.payment_statuses,
                data_commitment_statuses: v10.data_commitment_statuses,
                drc_policy_statuses: v10.drc_policy_statuses,
                drc_deposit_preauth_statuses: v10.drc_deposit_preauth_statuses,
                drc_regular_key_statuses: v10.drc_regular_key_statuses,
                drc_signer_list_statuses: v10.drc_signer_list_statuses,
                drc_ticket_create_statuses: v10.drc_ticket_create_statuses,
                drc_escrow_create_statuses: v10.drc_escrow_create_statuses,
                drc_escrow_finish_statuses: v10.drc_escrow_finish_statuses,
                drc_escrow_cancel_statuses: v10.drc_escrow_cancel_statuses,
                drc_check_create_statuses: Vec::new(),
                drc_check_cash_statuses: Vec::new(),
                drc_check_cancel_statuses: Vec::new(),
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v9) = MultiLaneV9AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v9.block_hash,
                statuses: v9.statuses,
                account_statuses: v9.account_statuses,
                stake_statuses: v9.stake_statuses,
                execution_statuses: v9.execution_statuses,
                payment_statuses: v9.payment_statuses,
                data_commitment_statuses: v9.data_commitment_statuses,
                drc_policy_statuses: v9.drc_policy_statuses,
                drc_deposit_preauth_statuses: v9.drc_deposit_preauth_statuses,
                drc_regular_key_statuses: v9.drc_regular_key_statuses,
                drc_signer_list_statuses: v9.drc_signer_list_statuses,
                drc_ticket_create_statuses: v9.drc_ticket_create_statuses,
                drc_escrow_create_statuses: Vec::new(),
                drc_escrow_finish_statuses: Vec::new(),
                drc_escrow_cancel_statuses: Vec::new(),
                drc_check_create_statuses: Vec::new(),
                drc_check_cash_statuses: Vec::new(),
                drc_check_cancel_statuses: Vec::new(),
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v8) = MultiLaneV8AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v8.block_hash,
                statuses: v8.statuses,
                account_statuses: v8.account_statuses,
                stake_statuses: v8.stake_statuses,
                execution_statuses: v8.execution_statuses,
                payment_statuses: v8.payment_statuses,
                data_commitment_statuses: v8.data_commitment_statuses,
                drc_policy_statuses: v8.drc_policy_statuses,
                drc_deposit_preauth_statuses: v8.drc_deposit_preauth_statuses,
                drc_regular_key_statuses: v8.drc_regular_key_statuses,
                drc_signer_list_statuses: Vec::new(),
                drc_ticket_create_statuses: Vec::new(),
                drc_escrow_create_statuses: Vec::new(),
                drc_escrow_finish_statuses: Vec::new(),
                drc_escrow_cancel_statuses: Vec::new(),
                drc_check_create_statuses: Vec::new(),
                drc_check_cash_statuses: Vec::new(),
                drc_check_cancel_statuses: Vec::new(),
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v7) = MultiLaneV7AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v7.block_hash,
                statuses: v7.statuses,
                account_statuses: v7.account_statuses,
                stake_statuses: v7.stake_statuses,
                execution_statuses: v7.execution_statuses,
                payment_statuses: v7.payment_statuses,
                data_commitment_statuses: v7.data_commitment_statuses,
                drc_policy_statuses: v7.drc_policy_statuses,
                drc_deposit_preauth_statuses: v7.drc_deposit_preauth_statuses,
                drc_regular_key_statuses: Vec::new(),
                drc_signer_list_statuses: Vec::new(),
                drc_ticket_create_statuses: Vec::new(),
                drc_escrow_create_statuses: Vec::new(),
                drc_escrow_finish_statuses: Vec::new(),
                drc_escrow_cancel_statuses: Vec::new(),
                drc_check_create_statuses: Vec::new(),
                drc_check_cash_statuses: Vec::new(),
                drc_check_cancel_statuses: Vec::new(),
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v6) = MultiLaneV6AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v6.block_hash,
                statuses: v6.statuses,
                account_statuses: v6.account_statuses,
                stake_statuses: v6.stake_statuses,
                execution_statuses: v6.execution_statuses,
                payment_statuses: v6.payment_statuses,
                data_commitment_statuses: v6.data_commitment_statuses,
                drc_policy_statuses: v6.drc_policy_statuses,
                drc_deposit_preauth_statuses: Vec::new(),
                drc_regular_key_statuses: Vec::new(),
                drc_signer_list_statuses: Vec::new(),
                drc_ticket_create_statuses: Vec::new(),
                drc_escrow_create_statuses: Vec::new(),
                drc_escrow_finish_statuses: Vec::new(),
                drc_escrow_cancel_statuses: Vec::new(),
                drc_check_create_statuses: Vec::new(),
                drc_check_cash_statuses: Vec::new(),
                drc_check_cancel_statuses: Vec::new(),
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v5) = MultiLaneV5AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v5.block_hash,
                statuses: v5.statuses,
                account_statuses: v5.account_statuses,
                stake_statuses: v5.stake_statuses,
                execution_statuses: v5.execution_statuses,
                payment_statuses: v5.payment_statuses,
                data_commitment_statuses: v5.data_commitment_statuses,
                drc_policy_statuses: Vec::new(),
                drc_deposit_preauth_statuses: Vec::new(),
                drc_regular_key_statuses: Vec::new(),
                drc_signer_list_statuses: Vec::new(),
                drc_ticket_create_statuses: Vec::new(),
                drc_escrow_create_statuses: Vec::new(),
                drc_escrow_finish_statuses: Vec::new(),
                drc_escrow_cancel_statuses: Vec::new(),
                drc_check_create_statuses: Vec::new(),
                drc_check_cash_statuses: Vec::new(),
                drc_check_cancel_statuses: Vec::new(),
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v4) = MultiLaneV4AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v4.block_hash,
                statuses: v4.statuses,
                account_statuses: v4.account_statuses,
                stake_statuses: v4.stake_statuses,
                execution_statuses: v4.execution_statuses,
                payment_statuses: v4.payment_statuses,
                data_commitment_statuses: Vec::new(),
                drc_policy_statuses: Vec::new(),
                drc_deposit_preauth_statuses: Vec::new(),
                drc_regular_key_statuses: Vec::new(),
                drc_signer_list_statuses: Vec::new(),
                drc_ticket_create_statuses: Vec::new(),
                drc_escrow_create_statuses: Vec::new(),
                drc_escrow_finish_statuses: Vec::new(),
                drc_escrow_cancel_statuses: Vec::new(),
                drc_check_create_statuses: Vec::new(),
                drc_check_cash_statuses: Vec::new(),
                drc_check_cancel_statuses: Vec::new(),
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v3) = MultiLaneV3AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v3.block_hash,
                statuses: v3.statuses,
                account_statuses: v3.account_statuses,
                stake_statuses: v3.stake_statuses,
                execution_statuses: v3.execution_statuses,
                payment_statuses: Vec::new(),
                data_commitment_statuses: Vec::new(),
                drc_policy_statuses: Vec::new(),
                drc_deposit_preauth_statuses: Vec::new(),
                drc_regular_key_statuses: Vec::new(),
                drc_signer_list_statuses: Vec::new(),
                drc_ticket_create_statuses: Vec::new(),
                drc_escrow_create_statuses: Vec::new(),
                drc_escrow_finish_statuses: Vec::new(),
                drc_escrow_cancel_statuses: Vec::new(),
                drc_check_create_statuses: Vec::new(),
                drc_check_cash_statuses: Vec::new(),
                drc_check_cancel_statuses: Vec::new(),
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        if let Ok(v2) = MultiLaneV2AcceptanceRecord::try_from_slice(bytes) {
            return Ok(Self {
                block_hash: v2.block_hash,
                statuses: v2.statuses,
                account_statuses: v2.account_statuses,
                stake_statuses: v2.stake_statuses,
                execution_statuses: Vec::new(),
                payment_statuses: Vec::new(),
                data_commitment_statuses: Vec::new(),
                drc_policy_statuses: Vec::new(),
                drc_deposit_preauth_statuses: Vec::new(),
                drc_regular_key_statuses: Vec::new(),
                drc_signer_list_statuses: Vec::new(),
                drc_ticket_create_statuses: Vec::new(),
                drc_escrow_create_statuses: Vec::new(),
                drc_escrow_finish_statuses: Vec::new(),
                drc_escrow_cancel_statuses: Vec::new(),
                drc_check_create_statuses: Vec::new(),
                drc_check_cash_statuses: Vec::new(),
                drc_check_cancel_statuses: Vec::new(),
                drc_payment_channel_create_statuses: Vec::new(),
                drc_payment_channel_fund_statuses: Vec::new(),
                drc_payment_channel_claim_statuses: Vec::new(),
                drc_payment_channel_close_statuses: Vec::new(),
                drc_trust_line_set_statuses: Vec::new(),
                drc_issued_transfer_statuses: Vec::new(),
                drc_issued_asset_policy_set_statuses: Vec::new(),
                drc_trust_line_issuer_control_statuses: Vec::new(),
                drc_issued_clawback_statuses: Vec::new(),
                drc_offer_create_statuses: Vec::new(),
                drc_offer_cancel_statuses: Vec::new(),
            });
        }
        let legacy = LegacyBlockAcceptanceRecord::try_from_slice(bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        Ok(Self {
            block_hash: legacy.block_hash,
            statuses: legacy.statuses,
            account_statuses: Vec::new(),
            stake_statuses: Vec::new(),
            execution_statuses: Vec::new(),
            payment_statuses: Vec::new(),
            data_commitment_statuses: Vec::new(),
            drc_policy_statuses: Vec::new(),
            drc_deposit_preauth_statuses: Vec::new(),
            drc_regular_key_statuses: Vec::new(),
            drc_signer_list_statuses: Vec::new(),
            drc_ticket_create_statuses: Vec::new(),
            drc_escrow_create_statuses: Vec::new(),
            drc_escrow_finish_statuses: Vec::new(),
            drc_escrow_cancel_statuses: Vec::new(),
            drc_check_create_statuses: Vec::new(),
            drc_check_cash_statuses: Vec::new(),
            drc_check_cancel_statuses: Vec::new(),
            drc_payment_channel_create_statuses: Vec::new(),
            drc_payment_channel_fund_statuses: Vec::new(),
            drc_payment_channel_claim_statuses: Vec::new(),
            drc_payment_channel_close_statuses: Vec::new(),
            drc_trust_line_set_statuses: Vec::new(),
            drc_issued_transfer_statuses: Vec::new(),
            drc_issued_asset_policy_set_statuses: Vec::new(),
            drc_trust_line_issuer_control_statuses: Vec::new(),
            drc_issued_clawback_statuses: Vec::new(),
            drc_offer_create_statuses: Vec::new(),
            drc_offer_cancel_statuses: Vec::new(),
        })
    }

    pub fn bitmap(&self) -> AcceptanceBitmap {
        let flags: Vec<bool> = self.statuses.iter().map(|s| s.is_accepted()).collect();
        AcceptanceBitmap::from_bools(&flags)
    }

    pub fn status_at(&self, index: usize) -> Option<TransactionAcceptance> {
        self.statuses.get(index).copied()
    }

    pub fn accepted_count(&self) -> usize {
        self.statuses.iter().filter(|s| s.is_accepted()).count()
            + self
                .account_statuses
                .iter()
                .filter(|s| s.is_accepted())
                .count()
            + self
                .stake_statuses
                .iter()
                .filter(|s| s.is_accepted())
                .count()
            + self
                .execution_statuses
                .iter()
                .filter(|s| s.is_accepted())
                .count()
            + self
                .payment_statuses
                .iter()
                .filter(|s| s.is_accepted())
                .count()
            + self
                .data_commitment_statuses
                .iter()
                .filter(|s| s.is_accepted())
                .count()
            + self
                .drc_policy_statuses
                .iter()
                .filter(|s| s.is_accepted())
                .count()
            + self
                .drc_deposit_preauth_statuses
                .iter()
                .filter(|s| s.is_accepted())
                .count()
            + self
                .drc_regular_key_statuses
                .iter()
                .filter(|s| s.is_accepted())
                .count()
            + self
                .drc_signer_list_statuses
                .iter()
                .filter(|s| s.is_accepted())
                .count()
    }
}

pub fn acceptance_key(hash: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(ACCEPTANCE_PREFIX.len() + 32);
    key.extend_from_slice(ACCEPTANCE_PREFIX);
    key.extend_from_slice(hash.as_bytes());
    key
}

pub fn put_acceptance_into(
    batch: &mut WriteBatch,
    hash: &Hash,
    record: &BlockAcceptanceRecord,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(record).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Warm, &acceptance_key(hash), &bytes);
    Ok(())
}

pub fn store_acceptance(
    store: &StateStore,
    hash: &Hash,
    record: &BlockAcceptanceRecord,
) -> Result<(), StateError> {
    let mut batch = WriteBatch::new();
    put_acceptance_into(&mut batch, hash, record)?;
    store.write_batch(batch)
}

pub fn load_acceptance(
    store: &StateStore,
    hash: &Hash,
) -> Result<Option<BlockAcceptanceRecord>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Warm, &acceptance_key(hash))? else {
        return Ok(None);
    };
    Ok(Some(BlockAcceptanceRecord::from_bytes(&bytes)?))
}

pub fn delete_acceptance_into(batch: &mut WriteBatch, hash: &Hash) {
    batch.delete_cf(ColumnFamily::Warm, &acceptance_key(hash));
}

/// Look up acceptance for a tx included in `block_hash` at `index`.
pub fn tx_acceptance_status(
    store: &StateStore,
    block_hash: &Hash,
    index: u32,
) -> Result<Option<TransactionAcceptance>, StateError> {
    let Some(record) = load_acceptance(store, block_hash)? else {
        return Ok(None);
    };
    Ok(record.status_at(index as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::TransactionAcceptance;

    #[test]
    fn record_roundtrip_and_bitmap() {
        let store = StateStore::open_in_memory();
        let rec = BlockAcceptanceRecord {
            block_hash: Hash([1u8; 32]),
            statuses: vec![
                TransactionAcceptance::Accepted,
                TransactionAcceptance::ConflictLost,
            ],
            account_statuses: vec![TransactionAcceptance::Accepted],
            stake_statuses: vec![],
            execution_statuses: vec![],
            payment_statuses: vec![],
            data_commitment_statuses: vec![TransactionAcceptance::ExactDuplicate],
            drc_policy_statuses: vec![TransactionAcceptance::Accepted],
            drc_deposit_preauth_statuses: vec![TransactionAcceptance::Accepted],
            drc_regular_key_statuses: vec![TransactionAcceptance::Accepted],
            drc_signer_list_statuses: vec![TransactionAcceptance::Accepted],
            drc_ticket_create_statuses: vec![],
            drc_escrow_create_statuses: vec![],
            drc_escrow_finish_statuses: vec![],
            drc_escrow_cancel_statuses: vec![],
            drc_check_create_statuses: vec![],
            drc_check_cash_statuses: vec![],
            drc_check_cancel_statuses: vec![],
            drc_payment_channel_create_statuses: vec![],
            drc_payment_channel_fund_statuses: vec![],
            drc_payment_channel_claim_statuses: vec![],
            drc_payment_channel_close_statuses: vec![],
            drc_trust_line_set_statuses: vec![],
            drc_issued_transfer_statuses: vec![],
            drc_issued_asset_policy_set_statuses: vec![],
            drc_trust_line_issuer_control_statuses: vec![],
            drc_issued_clawback_statuses: vec![],
            drc_offer_create_statuses: vec![],
            drc_offer_cancel_statuses: vec![],
        };
        store_acceptance(&store, &rec.block_hash, &rec).unwrap();
        let loaded = load_acceptance(&store, &rec.block_hash).unwrap().unwrap();
        assert_eq!(loaded.accepted_count(), 6);
        let bm = loaded.bitmap();
        assert_eq!(bm.get(0), Some(true));
        assert_eq!(bm.get(1), Some(false));
    }

    #[test]
    fn execution_era_record_migrates_with_empty_payment_lane() {
        let bytes = borsh::to_vec(&(
            Hash([2; 32]),
            vec![TransactionAcceptance::Accepted],
            vec![TransactionAcceptance::ConflictLost],
            Vec::<TransactionAcceptance>::new(),
            vec![TransactionAcceptance::Accepted],
        ))
        .unwrap();
        let record = BlockAcceptanceRecord::from_bytes(&bytes).unwrap();
        assert_eq!(
            record.execution_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        assert!(record.payment_statuses.is_empty());
        assert!(record.data_commitment_statuses.is_empty());
        assert!(record.drc_policy_statuses.is_empty());
        assert!(record.drc_deposit_preauth_statuses.is_empty());
    }

    #[test]
    fn payment_era_record_migrates_with_empty_data_commitment_lane() {
        let bytes = borsh::to_vec(&(
            Hash([3; 32]),
            vec![TransactionAcceptance::Accepted],
            Vec::<TransactionAcceptance>::new(),
            Vec::<TransactionAcceptance>::new(),
            Vec::<TransactionAcceptance>::new(),
            vec![TransactionAcceptance::Accepted],
        ))
        .unwrap();
        let record = BlockAcceptanceRecord::from_bytes(&bytes).unwrap();
        assert_eq!(
            record.payment_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        assert!(record.data_commitment_statuses.is_empty());
        assert!(record.drc_policy_statuses.is_empty());
        assert!(record.drc_deposit_preauth_statuses.is_empty());
    }

    #[test]
    fn data_era_record_migrates_with_empty_policy_lane() {
        let bytes = borsh::to_vec(&(
            Hash([4; 32]),
            vec![TransactionAcceptance::Accepted],
            Vec::<TransactionAcceptance>::new(),
            Vec::<TransactionAcceptance>::new(),
            Vec::<TransactionAcceptance>::new(),
            Vec::<TransactionAcceptance>::new(),
            vec![TransactionAcceptance::Accepted],
        ))
        .unwrap();
        let record = BlockAcceptanceRecord::from_bytes(&bytes).unwrap();
        assert_eq!(
            record.data_commitment_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        assert!(record.drc_policy_statuses.is_empty());
        assert!(record.drc_deposit_preauth_statuses.is_empty());
    }

    #[test]
    fn policy_era_record_migrates_with_empty_deposit_preauth_lane() {
        let bytes = borsh::to_vec(&(
            Hash([5; 32]),
            vec![TransactionAcceptance::Accepted],
            Vec::<TransactionAcceptance>::new(),
            Vec::<TransactionAcceptance>::new(),
            Vec::<TransactionAcceptance>::new(),
            Vec::<TransactionAcceptance>::new(),
            Vec::<TransactionAcceptance>::new(),
            vec![TransactionAcceptance::Accepted],
        ))
        .unwrap();
        let record = BlockAcceptanceRecord::from_bytes(&bytes).unwrap();
        assert_eq!(
            record.drc_policy_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        assert!(record.drc_deposit_preauth_statuses.is_empty());
    }
}
