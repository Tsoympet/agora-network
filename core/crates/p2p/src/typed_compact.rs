//! Versioned compact gossip that names consensus-lane kinds.
//!
//! Legacy [`crate::NetworkMessage::CompactBlock`] remains UTXO-only. Typed
//! lanes use this envelope so offers, covenants, and DA short ids cannot mix
//! into the UTXO list. Detached DRC multisign attachments have no mempool map,
//! so those blocks keep the full-body fallback.

use agora_types::{
    AccountTransfer, Block, BlockHeader, DataCommitmentAuthorization, DrcAccountPolicyTx,
    DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx, DrcDepositPreauthTx, DrcEscrowCancelTx,
    DrcEscrowCreateTx, DrcEscrowFinishTx, DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx,
    DrcIssuedTransferTx, DrcOfferCancelTx, DrcOfferCreateTx, DrcPaymentChannelClaimTx,
    DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, DrcPaymentTx,
    DrcRegularKeyTx, DrcSignerListTx, DrcTicketCreateTx, DrcTrustLineIssuerControlTx,
    DrcTrustLineSetTx, Hash, OvlExecutionTx, SignedStakeTx, TltCovenantTx, Transaction,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::ibd::{tx_short_id, ReconstructError};

pub const TYPED_COMPACT_VERSION: u8 = 1;

pub const COMPACT_LANE_UTXO: u8 = 0;
pub const COMPACT_LANE_ACCOUNT: u8 = 1;
pub const COMPACT_LANE_STAKE: u8 = 2;
pub const COMPACT_LANE_OVL_EXECUTION: u8 = 3;
pub const COMPACT_LANE_DRC_PAYMENT: u8 = 4;
pub const COMPACT_LANE_DATA_COMMITMENT: u8 = 5;
pub const COMPACT_LANE_DRC_POLICY: u8 = 6;
pub const COMPACT_LANE_DRC_PREAUTH: u8 = 7;
pub const COMPACT_LANE_DRC_REGULAR_KEY: u8 = 8;
pub const COMPACT_LANE_DRC_SIGNER_LIST: u8 = 9;
pub const COMPACT_LANE_DRC_TICKET: u8 = 10;
pub const COMPACT_LANE_DRC_ESCROW_CREATE: u8 = 11;
pub const COMPACT_LANE_DRC_ESCROW_FINISH: u8 = 12;
pub const COMPACT_LANE_DRC_ESCROW_CANCEL: u8 = 13;
pub const COMPACT_LANE_DRC_CHECK_CREATE: u8 = 14;
pub const COMPACT_LANE_DRC_CHECK_CASH: u8 = 15;
pub const COMPACT_LANE_DRC_CHECK_CANCEL: u8 = 16;
pub const COMPACT_LANE_DRC_CHANNEL_CREATE: u8 = 17;
pub const COMPACT_LANE_DRC_CHANNEL_FUND: u8 = 18;
pub const COMPACT_LANE_DRC_CHANNEL_CLAIM: u8 = 19;
pub const COMPACT_LANE_DRC_CHANNEL_CLOSE: u8 = 20;
pub const COMPACT_LANE_DRC_TRUST_LINE: u8 = 21;
pub const COMPACT_LANE_DRC_ISSUED_TRANSFER: u8 = 22;
pub const COMPACT_LANE_DRC_ASSET_POLICY: u8 = 23;
pub const COMPACT_LANE_DRC_ISSUER_CONTROL: u8 = 24;
pub const COMPACT_LANE_DRC_CLAWBACK: u8 = 25;
pub const COMPACT_LANE_DRC_OFFER_CREATE: u8 = 26;
pub const COMPACT_LANE_DRC_OFFER_CANCEL: u8 = 27;
pub const COMPACT_LANE_TLT_COVENANT: u8 = 28;

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TypedCompactLane {
    pub kind: u8,
    pub short_ids: Vec<[u8; 8]>,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TypedCompactBody {
    pub version: u8,
    pub header: BlockHeader,
    pub lanes: Vec<TypedCompactLane>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactLaneItem {
    Utxo(Transaction),
    Account(AccountTransfer),
    Stake(SignedStakeTx),
    OvlExecution(OvlExecutionTx),
    DrcPayment(DrcPaymentTx),
    DataCommitment(DataCommitmentAuthorization),
    DrcPolicy(DrcAccountPolicyTx),
    DrcPreauth(DrcDepositPreauthTx),
    DrcRegularKey(DrcRegularKeyTx),
    DrcSignerList(DrcSignerListTx),
    DrcTicket(DrcTicketCreateTx),
    DrcEscrowCreate(DrcEscrowCreateTx),
    DrcEscrowFinish(DrcEscrowFinishTx),
    DrcEscrowCancel(DrcEscrowCancelTx),
    DrcCheckCreate(DrcCheckCreateTx),
    DrcCheckCash(DrcCheckCashTx),
    DrcCheckCancel(DrcCheckCancelTx),
    DrcChannelCreate(DrcPaymentChannelCreateTx),
    DrcChannelFund(DrcPaymentChannelFundTx),
    DrcChannelClaim(DrcPaymentChannelClaimTx),
    DrcChannelClose(DrcPaymentChannelCloseTx),
    DrcTrustLine(DrcTrustLineSetTx),
    DrcIssuedTransfer(DrcIssuedTransferTx),
    DrcAssetPolicy(DrcIssuedAssetPolicySetTx),
    DrcIssuerControl(DrcTrustLineIssuerControlTx),
    DrcClawback(DrcIssuedClawbackTx),
    DrcOfferCreate(DrcOfferCreateTx),
    DrcOfferCancel(DrcOfferCancelTx),
    TltCovenant(TltCovenantTx),
}

impl TypedCompactBody {
    /// Named short-id lanes for every mempool-backed body field.
    ///
    /// Returns `None` when a lane cannot be named (today: detached multisign
    /// attachments), so callers keep the full `Block` envelope.
    pub fn from_block(block: &Block) -> Option<Self> {
        if block.typed_compact_unnamed_lanes() {
            return None;
        }
        let mut lanes = Vec::new();
        push_lane(
            &mut lanes,
            COMPACT_LANE_UTXO,
            &block.transactions,
            Transaction::tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_ACCOUNT,
            &block.account_transfers,
            AccountTransfer::transfer_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_STAKE,
            &block.stake_ops,
            SignedStakeTx::stake_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_OVL_EXECUTION,
            &block.ovl_executions,
            OvlExecutionTx::tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_PAYMENT,
            &block.drc_payments,
            DrcPaymentTx::payment_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DATA_COMMITMENT,
            &block.data_commitments,
            DataCommitmentAuthorization::authorization_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_POLICY,
            &block.drc_account_policies,
            DrcAccountPolicyTx::policy_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_PREAUTH,
            &block.drc_deposit_preauths,
            DrcDepositPreauthTx::preauth_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_REGULAR_KEY,
            &block.drc_regular_keys,
            DrcRegularKeyTx::regular_key_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_SIGNER_LIST,
            &block.drc_signer_lists,
            DrcSignerListTx::signer_list_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_TICKET,
            &block.drc_ticket_creates,
            DrcTicketCreateTx::ticket_create_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_ESCROW_CREATE,
            &block.drc_escrow_creates,
            DrcEscrowCreateTx::escrow_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_ESCROW_FINISH,
            &block.drc_escrow_finishes,
            DrcEscrowFinishTx::finish_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_ESCROW_CANCEL,
            &block.drc_escrow_cancels,
            DrcEscrowCancelTx::cancel_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_CHECK_CREATE,
            &block.drc_check_creates,
            DrcCheckCreateTx::check_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_CHECK_CASH,
            &block.drc_check_cashes,
            DrcCheckCashTx::cash_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_CHECK_CANCEL,
            &block.drc_check_cancels,
            DrcCheckCancelTx::cancel_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_CHANNEL_CREATE,
            &block.drc_payment_channel_creates,
            DrcPaymentChannelCreateTx::channel_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_CHANNEL_FUND,
            &block.drc_payment_channel_funds,
            DrcPaymentChannelFundTx::fund_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_CHANNEL_CLAIM,
            &block.drc_payment_channel_claims,
            DrcPaymentChannelClaimTx::claim_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_CHANNEL_CLOSE,
            &block.drc_payment_channel_closes,
            DrcPaymentChannelCloseTx::close_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_TRUST_LINE,
            &block.drc_trust_line_sets,
            DrcTrustLineSetTx::trust_line_set_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_ISSUED_TRANSFER,
            &block.drc_issued_transfers,
            DrcIssuedTransferTx::issued_transfer_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_ASSET_POLICY,
            &block.drc_issued_asset_policy_sets,
            DrcIssuedAssetPolicySetTx::policy_set_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_ISSUER_CONTROL,
            &block.drc_trust_line_issuer_controls,
            DrcTrustLineIssuerControlTx::issuer_control_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_CLAWBACK,
            &block.drc_issued_clawbacks,
            DrcIssuedClawbackTx::clawback_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_OFFER_CREATE,
            &block.drc_offer_creates,
            DrcOfferCreateTx::offer_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_DRC_OFFER_CANCEL,
            &block.drc_offer_cancels,
            DrcOfferCancelTx::cancel_tx_id,
        );
        push_lane(
            &mut lanes,
            COMPACT_LANE_TLT_COVENANT,
            &block.tlt_covenants,
            TltCovenantTx::tx_id,
        );
        Some(Self {
            version: TYPED_COMPACT_VERSION,
            header: block.header.clone(),
            lanes,
        })
    }
}

fn push_lane<T>(lanes: &mut Vec<TypedCompactLane>, kind: u8, items: &[T], id_of: fn(&T) -> Hash) {
    if items.is_empty() {
        return;
    }
    lanes.push(TypedCompactLane {
        kind,
        short_ids: items.iter().map(|item| tx_short_id(&id_of(item))).collect(),
    });
}

/// Inflate a typed compact body from mempool items. Unknown kinds fail closed.
pub fn reconstruct_typed_compact(
    body: TypedCompactBody,
    mut lookup: impl FnMut(u8, &[u8; 8]) -> Option<CompactLaneItem>,
) -> Result<Block, ReconstructError> {
    if body.version != TYPED_COMPACT_VERSION {
        return Err(ReconstructError::UnsupportedLane(body.version));
    }
    let mut block = Block::utxo(body.header.clone(), Vec::new());
    let mut missing = 0usize;
    for lane in &body.lanes {
        for sid in &lane.short_ids {
            match lookup(lane.kind, sid) {
                Some(item) => apply_item(&mut block, lane.kind, item)?,
                None => missing += 1,
            }
        }
    }
    if missing > 0 {
        return Err(ReconstructError::MissingShortIds(missing));
    }
    if block.compute_body_root() != body.header.tx_root {
        return Err(ReconstructError::TxRootMismatch);
    }
    Ok(block)
}

fn apply_item(block: &mut Block, kind: u8, item: CompactLaneItem) -> Result<(), ReconstructError> {
    match (kind, item) {
        (COMPACT_LANE_UTXO, CompactLaneItem::Utxo(tx)) => block.transactions.push(tx),
        (COMPACT_LANE_ACCOUNT, CompactLaneItem::Account(tx)) => block.account_transfers.push(tx),
        (COMPACT_LANE_STAKE, CompactLaneItem::Stake(tx)) => block.stake_ops.push(tx),
        (COMPACT_LANE_OVL_EXECUTION, CompactLaneItem::OvlExecution(tx)) => {
            block.ovl_executions.push(tx)
        }
        (COMPACT_LANE_DRC_PAYMENT, CompactLaneItem::DrcPayment(tx)) => block.drc_payments.push(tx),
        (COMPACT_LANE_DATA_COMMITMENT, CompactLaneItem::DataCommitment(tx)) => {
            block.data_commitments.push(tx)
        }
        (COMPACT_LANE_DRC_POLICY, CompactLaneItem::DrcPolicy(tx)) => {
            block.drc_account_policies.push(tx)
        }
        (COMPACT_LANE_DRC_PREAUTH, CompactLaneItem::DrcPreauth(tx)) => {
            block.drc_deposit_preauths.push(tx)
        }
        (COMPACT_LANE_DRC_REGULAR_KEY, CompactLaneItem::DrcRegularKey(tx)) => {
            block.drc_regular_keys.push(tx)
        }
        (COMPACT_LANE_DRC_SIGNER_LIST, CompactLaneItem::DrcSignerList(tx)) => {
            block.drc_signer_lists.push(tx)
        }
        (COMPACT_LANE_DRC_TICKET, CompactLaneItem::DrcTicket(tx)) => {
            block.drc_ticket_creates.push(tx)
        }
        (COMPACT_LANE_DRC_ESCROW_CREATE, CompactLaneItem::DrcEscrowCreate(tx)) => {
            block.drc_escrow_creates.push(tx)
        }
        (COMPACT_LANE_DRC_ESCROW_FINISH, CompactLaneItem::DrcEscrowFinish(tx)) => {
            block.drc_escrow_finishes.push(tx)
        }
        (COMPACT_LANE_DRC_ESCROW_CANCEL, CompactLaneItem::DrcEscrowCancel(tx)) => {
            block.drc_escrow_cancels.push(tx)
        }
        (COMPACT_LANE_DRC_CHECK_CREATE, CompactLaneItem::DrcCheckCreate(tx)) => {
            block.drc_check_creates.push(tx)
        }
        (COMPACT_LANE_DRC_CHECK_CASH, CompactLaneItem::DrcCheckCash(tx)) => {
            block.drc_check_cashes.push(tx)
        }
        (COMPACT_LANE_DRC_CHECK_CANCEL, CompactLaneItem::DrcCheckCancel(tx)) => {
            block.drc_check_cancels.push(tx)
        }
        (COMPACT_LANE_DRC_CHANNEL_CREATE, CompactLaneItem::DrcChannelCreate(tx)) => {
            block.drc_payment_channel_creates.push(tx)
        }
        (COMPACT_LANE_DRC_CHANNEL_FUND, CompactLaneItem::DrcChannelFund(tx)) => {
            block.drc_payment_channel_funds.push(tx)
        }
        (COMPACT_LANE_DRC_CHANNEL_CLAIM, CompactLaneItem::DrcChannelClaim(tx)) => {
            block.drc_payment_channel_claims.push(tx)
        }
        (COMPACT_LANE_DRC_CHANNEL_CLOSE, CompactLaneItem::DrcChannelClose(tx)) => {
            block.drc_payment_channel_closes.push(tx)
        }
        (COMPACT_LANE_DRC_TRUST_LINE, CompactLaneItem::DrcTrustLine(tx)) => {
            block.drc_trust_line_sets.push(tx)
        }
        (COMPACT_LANE_DRC_ISSUED_TRANSFER, CompactLaneItem::DrcIssuedTransfer(tx)) => {
            block.drc_issued_transfers.push(tx)
        }
        (COMPACT_LANE_DRC_ASSET_POLICY, CompactLaneItem::DrcAssetPolicy(tx)) => {
            block.drc_issued_asset_policy_sets.push(tx)
        }
        (COMPACT_LANE_DRC_ISSUER_CONTROL, CompactLaneItem::DrcIssuerControl(tx)) => {
            block.drc_trust_line_issuer_controls.push(tx)
        }
        (COMPACT_LANE_DRC_CLAWBACK, CompactLaneItem::DrcClawback(tx)) => {
            block.drc_issued_clawbacks.push(tx)
        }
        (COMPACT_LANE_DRC_OFFER_CREATE, CompactLaneItem::DrcOfferCreate(tx)) => {
            block.drc_offer_creates.push(tx)
        }
        (COMPACT_LANE_DRC_OFFER_CANCEL, CompactLaneItem::DrcOfferCancel(tx)) => {
            block.drc_offer_cancels.push(tx)
        }
        (COMPACT_LANE_TLT_COVENANT, CompactLaneItem::TltCovenant(tx)) => {
            block.tlt_covenants.push(tx)
        }
        _ => return Err(ReconstructError::UnsupportedLane(kind)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{Address, Amount, NativeAssetId};

    fn account_block() -> Block {
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block
            .account_transfers
            .push(AccountTransfer::unsigned_with_fee(
                NativeAssetId::OVL,
                Address::ZERO,
                Address([1; 20]),
                Amount::from_base_units(2),
                Amount::from_base_units(1),
                0,
            ));
        block.header.tx_root = block.compute_body_root();
        block
    }

    #[test]
    fn named_lanes_reconstruct_from_lookup() {
        let block = account_block();
        let body = TypedCompactBody::from_block(&block).expect("named");
        assert_eq!(body.lanes.len(), 1);
        assert_eq!(body.lanes[0].kind, COMPACT_LANE_ACCOUNT);
        let tx = block.account_transfers[0].clone();
        let rebuilt = reconstruct_typed_compact(body, |kind, _| {
            assert_eq!(kind, COMPACT_LANE_ACCOUNT);
            Some(CompactLaneItem::Account(tx.clone()))
        })
        .unwrap();
        assert_eq!(rebuilt.account_transfers, block.account_transfers);
        assert_eq!(rebuilt.header.tx_root, block.header.tx_root);
    }

    #[test]
    fn unknown_kind_fails_closed() {
        let block = account_block();
        let mut body = TypedCompactBody::from_block(&block).unwrap();
        body.lanes[0].kind = 99;
        let err = reconstruct_typed_compact(body, |_, _| {
            Some(CompactLaneItem::Account(block.account_transfers[0].clone()))
        })
        .unwrap_err();
        assert!(matches!(err, ReconstructError::UnsupportedLane(99)));
    }

    #[test]
    fn named_account_lane_is_compactable() {
        let block = account_block();
        assert!(!block.typed_compact_unnamed_lanes());
        assert!(TypedCompactBody::from_block(&block).is_some());
    }
}
