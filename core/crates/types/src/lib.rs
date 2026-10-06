//! Shared Agora BlockDAG primitives.
//!
//! Consensus-critical encoding uses `borsh`. Client bindings are generated with `ts-rs`.

mod acceptance;
mod account;
mod amount;
mod asset;
mod block;
mod data_availability;
mod drc_check;
mod drc_deposit_preauth;
mod drc_escrow;
mod drc_issued_controls;
mod drc_ledger_object;
mod drc_multisign;
mod drc_multisign_attachment;
mod drc_multisign_lane;
mod drc_offer;
mod drc_payment_channel;
mod drc_policy;
mod drc_regular_key;
mod drc_sequence;
mod drc_signer_list;
mod drc_ticket;
mod drc_trust_line;
mod execution;
mod finality;
mod hash;
mod hrp;
mod ovl_wei;
mod passport;
mod payment;
mod stake;
mod tlt_coinselect;
mod tlt_merkle;
mod tlt_script;
mod transaction;
mod treasury;
mod trident_header;

pub use acceptance::{AcceptanceBitmap, TransactionAcceptance};
pub use account::{
    AccountTransfer, ACCOUNT_TRANSFER_DRC_TICKET_VERSION, ACCOUNT_TRANSFER_LEGACY_VERSION,
    ACCOUNT_TRANSFER_VERSION, ACCOUNT_TX_SIGNING_DOMAIN, ACCOUNT_TX_SIGNING_DOMAIN_V3,
};
pub use amount::Amount;
pub use asset::{AssetTxOut, NativeAmount, NativeAssetId};
pub use block::{
    Block, BlockHeader, TRIDENT_BLOCK_BODY_DOMAIN, TRIDENT_BLOCK_BODY_V11_DOMAIN,
    TRIDENT_BLOCK_BODY_V11_VERSION, TRIDENT_BLOCK_BODY_V12_DOMAIN, TRIDENT_BLOCK_BODY_V12_VERSION,
    TRIDENT_BLOCK_BODY_V13_DOMAIN, TRIDENT_BLOCK_BODY_V13_VERSION, TRIDENT_BLOCK_BODY_V14_DOMAIN,
    TRIDENT_BLOCK_BODY_V14_VERSION, TRIDENT_BLOCK_BODY_V15_DOMAIN, TRIDENT_BLOCK_BODY_V15_VERSION,
    TRIDENT_BLOCK_BODY_V16_DOMAIN, TRIDENT_BLOCK_BODY_V16_VERSION, TRIDENT_BLOCK_BODY_V17_DOMAIN,
    TRIDENT_BLOCK_BODY_V17_VERSION, TRIDENT_BLOCK_BODY_VERSION,
};
pub use data_availability::{
    DataAvailabilityCommitment, DataCommitmentAuthorization, DataCommitmentError,
    DataCommitmentSource, DA_COMMITMENT_AUTHORIZATION_DOMAIN,
    DA_COMMITMENT_AUTHORIZATION_ID_DOMAIN, DA_COMMITMENT_AUTHORIZATION_VERSION,
    DA_COMMITMENT_PAYLOAD_DOMAIN, DA_COMMITMENT_VERSION, MAX_DA_CHAIN_ID_BYTES,
};
pub use drc_check::{
    check_cancel_submitter_allowed, check_cash_allowed, validate_check_expiration_bound,
    DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx, DrcCheckError, DrcCheckLive,
    DrcCheckOutcome, DrcCheckReceipt, DRC_CHECK_CANCEL_SIGNING_DOMAIN,
    DRC_CHECK_CANCEL_TICKET_SIGNING_DOMAIN, DRC_CHECK_CANCEL_TICKET_VERSION,
    DRC_CHECK_CANCEL_TX_VERSION, DRC_CHECK_CASH_SIGNING_DOMAIN,
    DRC_CHECK_CASH_TICKET_SIGNING_DOMAIN, DRC_CHECK_CASH_TICKET_VERSION, DRC_CHECK_CASH_TX_VERSION,
    DRC_CHECK_CREATE_SIGNING_DOMAIN, DRC_CHECK_CREATE_TICKET_SIGNING_DOMAIN,
    DRC_CHECK_CREATE_TICKET_VERSION, DRC_CHECK_CREATE_TX_VERSION, DRC_CHECK_LIVE_STATE_VERSION,
    DRC_CHECK_MAX_BLUE_SCORE_BOUND, DRC_CHECK_RECEIPT_VERSION, DRC_MAX_LIVE_CHECKS_PER_ACCOUNT,
};
pub use drc_deposit_preauth::{
    DrcDepositPreauth, DrcDepositPreauthAction, DrcDepositPreauthError, DrcDepositPreauthTx,
    DRC_DEPOSIT_PREAUTH_SIGNING_DOMAIN, DRC_DEPOSIT_PREAUTH_STATE_VERSION,
    DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION, DRC_DEPOSIT_PREAUTH_TX_TYPE,
    DRC_DEPOSIT_PREAUTH_TX_VERSION, DRC_DEPOSIT_PREAUTH_V2_SIGNING_DOMAIN,
};
pub use drc_escrow::{
    escrow_cancel_allowed, escrow_finish_allowed, validate_escrow_time_bounds, DrcEscrowCancelTx,
    DrcEscrowCreateTx, DrcEscrowError, DrcEscrowFinishTx, DrcEscrowLive, DrcEscrowOutcome,
    DrcEscrowReceipt, DRC_ESCROW_CANCEL_SIGNING_DOMAIN, DRC_ESCROW_CANCEL_TICKET_SIGNING_DOMAIN,
    DRC_ESCROW_CANCEL_TICKET_VERSION, DRC_ESCROW_CANCEL_TX_TYPE, DRC_ESCROW_CANCEL_TX_VERSION,
    DRC_ESCROW_CREATE_SIGNING_DOMAIN, DRC_ESCROW_CREATE_TICKET_SIGNING_DOMAIN,
    DRC_ESCROW_CREATE_TICKET_VERSION, DRC_ESCROW_CREATE_TX_TYPE, DRC_ESCROW_CREATE_TX_VERSION,
    DRC_ESCROW_FINISH_SIGNING_DOMAIN, DRC_ESCROW_FINISH_TICKET_SIGNING_DOMAIN,
    DRC_ESCROW_FINISH_TICKET_VERSION, DRC_ESCROW_FINISH_TX_TYPE, DRC_ESCROW_FINISH_TX_VERSION,
    DRC_ESCROW_LIVE_STATE_VERSION, DRC_ESCROW_MAX_BLUE_SCORE_BOUND, DRC_ESCROW_RECEIPT_VERSION,
    DRC_MAX_LIVE_ESCROWS_PER_ACCOUNT,
};
pub use drc_issued_controls::{
    drc_issued_asset_policy_meta_key, drc_issued_asset_policy_set_mutation_meta_keys,
    drc_issued_clawback_mutation_meta_keys, drc_trust_line_issuer_control_mutation_meta_keys,
    DrcIssuedAssetPolicyAction, DrcIssuedAssetPolicyLive, DrcIssuedAssetPolicyReceipt,
    DrcIssuedAssetPolicySetTx, DrcIssuedClawbackReceipt, DrcIssuedClawbackTx,
    DrcIssuedControlsError, DrcTrustLineIssuerControlAction, DrcTrustLineIssuerControlReceipt,
    DrcTrustLineIssuerControlTx, DRC_ISSUED_ASSET_POLICY_LIVE_VERSION,
    DRC_ISSUED_ASSET_POLICY_META_PREFIX, DRC_ISSUED_ASSET_POLICY_RECEIPT_VERSION,
    DRC_ISSUED_ASSET_POLICY_SET_SIGNING_DOMAIN, DRC_ISSUED_ASSET_POLICY_SET_TICKET_SIGNING_DOMAIN,
    DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION, DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
    DRC_ISSUED_CLAWBACK_RECEIPT_VERSION, DRC_ISSUED_CLAWBACK_SIGNING_DOMAIN,
    DRC_ISSUED_CLAWBACK_TICKET_SIGNING_DOMAIN, DRC_ISSUED_CLAWBACK_TICKET_VERSION,
    DRC_ISSUED_CLAWBACK_TX_VERSION, DRC_TRUST_LINE_ISSUER_CONTROL_RECEIPT_VERSION,
    DRC_TRUST_LINE_ISSUER_CONTROL_SIGNING_DOMAIN,
    DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_SIGNING_DOMAIN,
    DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_VERSION, DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION,
};
pub use drc_ledger_object::{
    DrcAcceptedOperationReceipt, DrcLedgerObject, DrcLedgerObjectDescriptor, DrcLedgerObjectKey,
    DrcLedgerObjectKind, DrcLedgerObjectPage, DrcOperation, DrcOperationKind,
    DRC_ACCEPTED_OPERATION_ID_DOMAIN, DRC_ACCEPTED_OPERATION_RECEIPT_VERSION,
    DRC_LEDGER_OBJECT_DESCRIPTOR_VERSION, DRC_LEDGER_OBJECT_ID_DOMAIN,
};
pub use drc_multisign::{
    read_multisign_trailer, validate_exclusive_authorization, write_multisign_trailer,
    DrcMultisignAuth, DrcMultisignEntry, DrcMultisignError, DRC_MULTISIGN_AUTH_VERSION,
    DRC_MULTISIGN_MAX_SIGNATURES, DRC_MULTISIGN_PARTICIPANT_DOMAIN, DRC_SIGNER_LIST_MAX_ENTRIES,
    DRC_SIGNER_MAX_WEIGHT,
};
pub use drc_multisign_attachment::{
    attachment_key_for_account_transfer, attachment_key_for_check_cancel,
    attachment_key_for_check_cash, attachment_key_for_check_create,
    attachment_key_for_deposit_preauth, attachment_key_for_escrow_cancel,
    attachment_key_for_escrow_create, attachment_key_for_escrow_finish,
    attachment_key_for_issued_asset_policy_set, attachment_key_for_issued_clawback,
    attachment_key_for_issued_transfer, attachment_key_for_offer_cancel,
    attachment_key_for_offer_create, attachment_key_for_payment,
    attachment_key_for_payment_channel_claim, attachment_key_for_payment_channel_close,
    attachment_key_for_payment_channel_create, attachment_key_for_payment_channel_fund,
    attachment_key_for_policy, attachment_key_for_regular_key, attachment_key_for_signer_list,
    attachment_key_for_stake, attachment_key_for_ticket_create,
    attachment_key_for_trust_line_issuer_control, attachment_key_for_trust_line_set,
    drc_multisign_attachment_key, drc_multisign_signing_commitment, DrcMultisignAttachmentError,
    DrcMultisignAttachmentKey, DrcMultisignBlockAttachment, DrcMultisignOperationKind,
    DRC_MULTISIGN_ATTACHMENT_KEY_VERSION, DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
    DRC_MULTISIGN_SIGNING_COMMITMENT_DOMAIN,
};
pub use drc_multisign_lane::{
    drc_multisign_attachment_capacity, materialize_drc_multisign_attachments,
    merge_drc_multisign_attachments, validate_drc_multisign_attachment_lane,
};
pub use drc_offer::{
    offer_fill_step, offer_quality_better, offers_cross, DrcBookAsset, DrcOfferBook,
    DrcOfferBookCursor, DrcOfferBookPage, DrcOfferCancelOutcome, DrcOfferCancelReceipt,
    DrcOfferCancelTx, DrcOfferCreateReceipt, DrcOfferCreateTx, DrcOfferCursor, DrcOfferError,
    DrcOfferFillMode, DrcOfferLive, DrcOfferPage, DrcOfferTimeInForce, DrcOfferView,
    DRC_MAX_LIVE_OFFERS_PER_ACCOUNT, DRC_MAX_OFFERS_PER_BOOK, DRC_MAX_OFFER_MATCHES_PER_BLOCK,
    DRC_MAX_OFFER_MATCHES_PER_TX, DRC_OFFER_CANCEL_RECEIPT_VERSION,
    DRC_OFFER_CANCEL_SIGNING_DOMAIN, DRC_OFFER_CANCEL_TICKET_SIGNING_DOMAIN,
    DRC_OFFER_CANCEL_TICKET_VERSION, DRC_OFFER_CANCEL_TX_VERSION, DRC_OFFER_CREATE_RECEIPT_VERSION,
    DRC_OFFER_CREATE_SIGNING_DOMAIN, DRC_OFFER_CREATE_TICKET_SIGNING_DOMAIN,
    DRC_OFFER_CREATE_TICKET_VERSION, DRC_OFFER_CREATE_TX_VERSION, DRC_OFFER_LIVE_STATE_VERSION,
    DRC_OFFER_PAGE_MAX,
};
pub use drc_payment_channel::{
    payment_channel_cancel_after_valid_at_create, payment_channel_claim_submitter_allowed,
    payment_channel_close_submitter_allowed, payment_channel_finalize_allowed,
    payment_channel_fund_submitter_allowed, payment_channel_offledger_claim_signing_bytes,
    payment_channel_onchain_claim_allowed, payment_channel_owner_schedule_deadline,
    validate_payment_channel_blue_score_bound, DrcPaymentChannelClaimEvent,
    DrcPaymentChannelClaimTx, DrcPaymentChannelCloseKind, DrcPaymentChannelCloseTx,
    DrcPaymentChannelCreateTx, DrcPaymentChannelError, DrcPaymentChannelFundEvent,
    DrcPaymentChannelFundTx, DrcPaymentChannelLive, DrcPaymentChannelOutcome,
    DrcPaymentChannelReceipt, DrcPaymentChannelScheduleEvent,
    DRC_MAX_LIVE_PAYMENT_CHANNELS_PER_ACCOUNT, DRC_PAYMENT_CHANNEL_CLAIM_EVENT_VERSION,
    DRC_PAYMENT_CHANNEL_CLAIM_SIGNING_DOMAIN, DRC_PAYMENT_CHANNEL_CLAIM_TICKET_SIGNING_DOMAIN,
    DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION, DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION,
    DRC_PAYMENT_CHANNEL_CLOSE_SIGNING_DOMAIN, DRC_PAYMENT_CHANNEL_CLOSE_TICKET_SIGNING_DOMAIN,
    DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION, DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION,
    DRC_PAYMENT_CHANNEL_CREATE_SIGNING_DOMAIN, DRC_PAYMENT_CHANNEL_CREATE_TICKET_SIGNING_DOMAIN,
    DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION, DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
    DRC_PAYMENT_CHANNEL_FUND_EVENT_VERSION, DRC_PAYMENT_CHANNEL_FUND_SIGNING_DOMAIN,
    DRC_PAYMENT_CHANNEL_FUND_TICKET_SIGNING_DOMAIN, DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
    DRC_PAYMENT_CHANNEL_FUND_TX_VERSION, DRC_PAYMENT_CHANNEL_LIVE_STATE_VERSION,
    DRC_PAYMENT_CHANNEL_MAX_BLUE_SCORE_BOUND, DRC_PAYMENT_CHANNEL_OFFLEDGER_CLAIM_DOMAIN,
    DRC_PAYMENT_CHANNEL_RECEIPT_VERSION, DRC_PAYMENT_CHANNEL_SCHEDULE_EVENT_VERSION,
};
pub use drc_policy::{
    DrcAccountPolicy, DrcAccountPolicyAction, DrcAccountPolicyError, DrcAccountPolicyTx,
    DRC_ACCOUNT_POLICY_LEGACY_STATE_VERSION, DRC_ACCOUNT_POLICY_LEGACY_TX_VERSION,
    DRC_ACCOUNT_POLICY_MASTER_KEY_STATE_VERSION, DRC_ACCOUNT_POLICY_MASTER_KEY_TX_VERSION,
    DRC_ACCOUNT_POLICY_SIGNING_DOMAIN, DRC_ACCOUNT_POLICY_STATE_VERSION,
    DRC_ACCOUNT_POLICY_TICKET_TX_VERSION, DRC_ACCOUNT_POLICY_TX_TYPE,
    DRC_ACCOUNT_POLICY_TX_VERSION, DRC_ACCOUNT_POLICY_V1_SIGNING_DOMAIN,
    DRC_ACCOUNT_POLICY_V2_SIGNING_DOMAIN, DRC_ACCOUNT_POLICY_V3_SIGNING_DOMAIN,
    DRC_ACCOUNT_POLICY_V4_SIGNING_DOMAIN,
};
pub use drc_regular_key::{
    DrcAccountRegularKey, DrcRegularKeyAction, DrcRegularKeyError, DrcRegularKeyTx,
    DRC_REGULAR_KEY_SIGNING_DOMAIN, DRC_REGULAR_KEY_STATE_VERSION,
    DRC_REGULAR_KEY_TICKET_TX_VERSION, DRC_REGULAR_KEY_TX_TYPE, DRC_REGULAR_KEY_TX_VERSION,
    DRC_REGULAR_KEY_V2_SIGNING_DOMAIN,
};
pub use drc_sequence::{
    resolve_drc_account_sequence, DrcAccountSequence, DrcAccountSequenceError,
    DrcAccountSequenceSelector, DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT,
};
pub use drc_signer_list::{
    canonical_sorted_entries, validate_signer_list_payload, DrcAccountSignerList,
    DrcSignerListAction, DrcSignerListEntry, DrcSignerListError, DrcSignerListTx,
    DRC_SIGNER_LIST_SIGNING_DOMAIN, DRC_SIGNER_LIST_STATE_VERSION,
    DRC_SIGNER_LIST_TICKET_TX_VERSION, DRC_SIGNER_LIST_TX_TYPE, DRC_SIGNER_LIST_TX_VERSION,
    DRC_SIGNER_LIST_V2_SIGNING_DOMAIN,
};
pub use drc_ticket::{
    DrcAccountTickets, DrcTicketCreateError, DrcTicketCreateTx, DRC_TICKET_CREATE_SIGNING_DOMAIN,
    DRC_TICKET_CREATE_TX_TYPE, DRC_TICKET_CREATE_TX_VERSION, DRC_TICKET_STATE_VERSION,
};
pub use drc_trust_line::{
    drc_issued_transfer_mutation_meta_keys, drc_trust_line_issuer_liability_meta_key,
    drc_trust_line_live_meta_key, drc_trust_line_set_mutation_meta_keys, DrcIssuedTransferReceipt,
    DrcIssuedTransferTx, DrcIssuerLiability, DrcTrustLineError, DrcTrustLineLive,
    DrcTrustLineSetTx, IssuedAmount, IssuedAssetId, IssuedCurrencyCode, IssuedCurrencyError,
    DRC_ISSUED_TRANSFER_RECEIPT_VERSION, DRC_ISSUED_TRANSFER_SIGNING_DOMAIN,
    DRC_ISSUED_TRANSFER_TICKET_SIGNING_DOMAIN, DRC_ISSUER_LIABILITY_STATE_VERSION,
    DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER, DRC_MAX_TRUST_LINE_HOLDERS_PER_ISSUER,
    DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION, DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
    DRC_TRUST_LINE_LIVE_STATE_V2, DRC_TRUST_LINE_LIVE_STATE_VERSION,
    DRC_TRUST_LINE_SET_SIGNING_DOMAIN, DRC_TRUST_LINE_SET_TICKET_SIGNING_DOMAIN,
    DRC_TRUST_LINE_SET_TICKET_VERSION, DRC_TRUST_LINE_SET_TX_VERSION,
};
pub use execution::{
    OvlExecutionTx, OVL_EXECUTION_RAW_EVM_VERSION, OVL_EXECUTION_SIGNING_DOMAIN,
    OVL_EXECUTION_VERSION,
};
pub use finality::{
    CheckpointAttestation, CheckpointBody, CheckpointState, FinalityCertificate,
    CHECKPOINT_ATTESTATION_DOMAIN,
};
pub use hash::Hash;
pub use hrp::{
    address_hrp_for_network, is_known_address_hrp, ADDRESS_HRP, ADDRESS_HRP_DEV,
    ADDRESS_HRP_MAINNET, ADDRESS_HRP_TESTNET,
};
pub use ovl_wei::{
    ovl_evm_chain_id_rejected, OvlFeeMarketParams, OvlWei, OVL_BASE_FEE_BURN_BPS,
    OVL_EVM_DEV_CHAIN_ID, OVL_EVM_PROFILE, OVL_EVM_REVM_VERSION, OVL_EVM_SPEC_ID,
    OVL_EVM_TESTNET_CHAIN_ID, OVL_LEGACY_DECIMALS, OVL_LEGACY_TO_WEI, OVL_WEI_DECIMALS,
};
pub use passport::{PassportAttestation, PassportCategory, PASSPORT_ATTESTATION_DOMAIN};
pub use payment::{
    DrcPaymentEnvelopeError, DrcPaymentOutboxEvent, DrcPaymentReceipt, DrcPaymentReceiptError,
    DrcPaymentResult, DrcPaymentTx, DRC_PAYMENT_DESTINATION_TAG_VERSION,
    DRC_PAYMENT_LEGACY_VERSION, DRC_PAYMENT_RECEIPT_DESTINATION_TAG_VERSION,
    DRC_PAYMENT_RECEIPT_LEGACY_VERSION, DRC_PAYMENT_RECEIPT_VERSION, DRC_PAYMENT_SIGNING_DOMAIN,
    DRC_PAYMENT_SOURCE_TAG_VERSION, DRC_PAYMENT_TICKET_VERSION, DRC_PAYMENT_V1_SIGNING_DOMAIN,
    DRC_PAYMENT_V2_SIGNING_DOMAIN, DRC_PAYMENT_V3_SIGNING_DOMAIN, DRC_PAYMENT_V4_SIGNING_DOMAIN,
    DRC_PAYMENT_V5_SIGNING_DOMAIN, DRC_PAYMENT_VERSION,
};
pub use stake::{
    SignedStakeTx, StakeOpKind, STAKE_TX_SIGNING_DOMAIN, STAKE_TX_SIGNING_DOMAIN_V2,
    STAKE_TX_TICKET_VERSION, STAKE_TX_VERSION,
};
pub use tlt_coinselect::{
    select_tlt_coins, TltCoinSelectError, TltCoinSelection, TltSpendCoin,
    TLT_COINSELECT_EXHAUSTIVE_CAP,
};
pub use tlt_merkle::{
    prove_tlt_tx_merkle, tlt_tx_merkle_root, verify_tlt_tx_merkle, TltTxMerkleProof,
};
pub use tlt_script::{
    eval_covenant_input, eval_tlt_script, is_p2sh_script, push_data, script_htlc, script_multisig,
    script_p2pkh, script_p2sh, sequence_signals_rbf, SigChecker, TltCovenantInput,
    TltCovenantOutput, TltCovenantTx, TltOutputOrigin, TltScriptError, TltSpendContext,
    TLT_COVENANT_TX_DOMAIN, TLT_COVENANT_TX_VERSION, TLT_CSV_TIME_STEP_SECS,
    TLT_LOCKTIME_TIME_THRESHOLD, TLT_MAX_MULTISIG, TLT_MAX_OPS, TLT_MAX_PUSH, TLT_MAX_SCRIPT_LEN,
    TLT_MAX_STACK, TLT_SEQUENCE_DISABLE_FLAG, TLT_SEQUENCE_FINAL, TLT_SEQUENCE_LOCK_MASK,
    TLT_SEQUENCE_TIME_FLAG,
};
pub use transaction::{Address, OutPoint, Transaction, TransactionBody, TxIn, TxOut};
pub use treasury::{TreasuryBalance, TreasuryId};
pub use trident_header::{
    TridentHeader, TridentHeaderError, TridentHeaderIdentity, TRIDENT_HEADER_ENCODING_DOMAIN,
    TRIDENT_HEADER_ENCODING_VERSION,
};

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshDeserialize;

    #[test]
    fn amount_whole_conversion() {
        let one = Amount::from_whole(1).expect("1 AGORA");
        assert_eq!(one.as_base_units(), 100_000_000);
        assert_eq!(one.checked_add(one).unwrap().as_base_units(), 200_000_000);
    }

    #[test]
    fn address_bech32m_roundtrip_and_parse() {
        let addr = Address::from_hex("ff9ec96f09eb154d038a552ecae59c50204ea9a9").unwrap();
        let encoded = addr.to_bech32();
        // Locked against apps/shared/light-client `encodeAddress` (@scure/base bech32m).
        assert_eq!(encoded, "agora1l70vjmcfav256qu225hv4evu2qsya2dfajrcqc");
        assert_eq!(Address::from_bech32(&encoded), Some(addr));
        assert_eq!(Address::from_bech32(&encoded.to_uppercase()), Some(addr));
        let testnet = addr.to_bech32_hrp(ADDRESS_HRP_TESTNET);
        assert!(testnet.starts_with("agoratest1"));
        assert_eq!(Address::from_bech32(&testnet), Some(addr));
        assert_eq!(Address::parse(&encoded), Some(addr));
        assert_eq!(Address::parse(&testnet), Some(addr));
        assert_eq!(Address::parse(&addr.to_hex()), Some(addr));
        assert_eq!(Address::parse(&format!("0x{}", addr.to_hex())), Some(addr));
        assert_eq!(format!("{addr}"), encoded);
        assert!(Address::from_bech32("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4").is_none());
    }

    #[test]
    fn transaction_borsh_roundtrip_and_id_stable() {
        let tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(42),
                address: Address::ZERO,
            }],
            7,
        );
        let bytes = borsh::to_vec(&tx).unwrap();
        let decoded = Transaction::try_from_slice(&bytes).unwrap();
        assert_eq!(tx, decoded);
        assert_eq!(tx.tx_id(), decoded.tx_id());
        assert!(!tx.signing_bytes().is_empty());
    }

    #[test]
    fn block_tx_root_deterministic() {
        let tx = Transaction::unsigned(1, vec![], vec![], 1);
        let root = Block::compute_tx_root(std::slice::from_ref(&tx));
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 1,
            bits: 1,
            nonce: 0,
            tx_root: root,
        };
        let block = Block {
            header: header.clone(),
            transactions: vec![tx],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_payment_channel_creates: vec![],
            drc_payment_channel_funds: vec![],
            drc_payment_channel_claims: vec![],
            drc_payment_channel_closes: vec![],
            drc_trust_line_sets: vec![],
            drc_issued_transfers: vec![],
            drc_issued_asset_policy_sets: vec![],
            drc_trust_line_issuer_controls: vec![],
            drc_issued_clawbacks: vec![],
            drc_offer_creates: vec![],
            drc_offer_cancels: vec![],
            drc_multisign_attachments: vec![],
        };
        assert_eq!(block.id(), header.hash());
        assert_eq!(Block::compute_tx_root(&block.transactions), root);
    }
}

/// Regenerates TypeScript bindings into `bindings/` when tests run.
#[cfg(test)]
mod ts_export {
    use std::{fs, path::Path};

    use super::*;
    use ts_rs::TS;

    const NORMALIZED_BINDINGS: &[&str] = &[
        "AccountTransfer.ts",
        "Block.ts",
        "CheckpointAttestation.ts",
        "DataAvailabilityCommitment.ts",
        "DataCommitmentAuthorization.ts",
        "DrcDepositPreauth.ts",
        "DrcDepositPreauthAction.ts",
        "DrcDepositPreauthTx.ts",
        "DrcAccountPolicy.ts",
        "DrcAccountPolicyAction.ts",
        "DrcAccountPolicyTx.ts",
        "DrcAccountSequence.ts",
        "DrcAccountSequenceSelector.ts",
        "DrcAccountTickets.ts",
        "DrcTicketCreateTx.ts",
        "DrcEscrowCreateTx.ts",
        "DrcEscrowFinishTx.ts",
        "DrcEscrowCancelTx.ts",
        "DrcEscrowLive.ts",
        "DrcEscrowReceipt.ts",
        "DrcEscrowOutcome.ts",
        "DrcBookAsset.ts",
        "DrcOfferBook.ts",
        "DrcOfferBookCursor.ts",
        "DrcOfferBookPage.ts",
        "DrcOfferCancelOutcome.ts",
        "DrcOfferCancelReceipt.ts",
        "DrcOfferCancelTx.ts",
        "DrcOfferCreateReceipt.ts",
        "DrcOfferCreateTx.ts",
        "DrcOfferCursor.ts",
        "DrcOfferFillMode.ts",
        "DrcOfferLive.ts",
        "DrcOfferPage.ts",
        "DrcOfferTimeInForce.ts",
        "DrcOfferView.ts",
        "DrcIssuedAssetPolicyAction.ts",
        "DrcIssuedAssetPolicyLive.ts",
        "DrcIssuedAssetPolicyReceipt.ts",
        "DrcIssuedAssetPolicySetTx.ts",
        "DrcIssuedClawbackReceipt.ts",
        "DrcIssuedClawbackTx.ts",
        "DrcPaymentReceipt.ts",
        "DrcPaymentResult.ts",
        "DrcPaymentTx.ts",
        "DrcTrustLineIssuerControlAction.ts",
        "DrcTrustLineIssuerControlReceipt.ts",
        "DrcTrustLineIssuerControlTx.ts",
        "OvlExecutionTx.ts",
        "OvlFeeMarketParams.ts",
        "OvlWei.ts",
        "SignedStakeTx.ts",
        "Transaction.ts",
    ];

    fn normalize_generated_bindings() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings");
        for filename in NORMALIZED_BINDINGS {
            let path = directory.join(filename);
            let generated = fs::read_to_string(&path).expect("read generated TypeScript binding");
            let mut normalized = generated
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n");
            normalized.push('\n');
            fs::write(path, normalized).expect("normalize generated TypeScript binding");
        }
    }

    #[test]
    fn export_shared_types() {
        Amount::export_all().expect("export Amount");
        Hash::export_all().expect("export Hash");
        Address::export_all().expect("export Address");
        OutPoint::export_all().expect("export OutPoint");
        TxIn::export_all().expect("export TxIn");
        TxOut::export_all().expect("export TxOut");
        Transaction::export_all().expect("export Transaction");
        BlockHeader::export_all().expect("export BlockHeader");
        Block::export_all().expect("export Block");
        NativeAssetId::export_all().expect("export NativeAssetId");
        NativeAmount::export_all().expect("export NativeAmount");
        AssetTxOut::export_all().expect("export AssetTxOut");
        TreasuryId::export_all().expect("export TreasuryId");
        TreasuryBalance::export_all().expect("export TreasuryBalance");
        TransactionAcceptance::export_all().expect("export TransactionAcceptance");
        AcceptanceBitmap::export_all().expect("export AcceptanceBitmap");
        AccountTransfer::export_all().expect("export AccountTransfer");
        DataCommitmentSource::export_all().expect("export DataCommitmentSource");
        DataAvailabilityCommitment::export_all().expect("export DataAvailabilityCommitment");
        DataCommitmentAuthorization::export_all().expect("export DataCommitmentAuthorization");
        DrcDepositPreauthAction::export_all().expect("export DrcDepositPreauthAction");
        DrcDepositPreauthTx::export_all().expect("export DrcDepositPreauthTx");
        DrcDepositPreauth::export_all().expect("export DrcDepositPreauth");
        DrcAccountPolicyAction::export_all().expect("export DrcAccountPolicyAction");
        DrcAccountPolicyTx::export_all().expect("export DrcAccountPolicyTx");
        DrcAccountPolicy::export_all().expect("export DrcAccountPolicy");
        DrcAccountSequence::export_all().expect("export DrcAccountSequence");
        DrcAccountSequenceSelector::export_all().expect("export DrcAccountSequenceSelector");
        DrcAccountTickets::export_all().expect("export DrcAccountTickets");
        DrcTicketCreateTx::export_all().expect("export DrcTicketCreateTx");
        DrcEscrowCreateTx::export_all().expect("export DrcEscrowCreateTx");
        DrcEscrowFinishTx::export_all().expect("export DrcEscrowFinishTx");
        DrcEscrowCancelTx::export_all().expect("export DrcEscrowCancelTx");
        DrcEscrowLive::export_all().expect("export DrcEscrowLive");
        DrcEscrowReceipt::export_all().expect("export DrcEscrowReceipt");
        DrcEscrowOutcome::export_all().expect("export DrcEscrowOutcome");
        DrcCheckCreateTx::export_all().expect("export DrcCheckCreateTx");
        DrcCheckCashTx::export_all().expect("export DrcCheckCashTx");
        DrcCheckCancelTx::export_all().expect("export DrcCheckCancelTx");
        DrcCheckLive::export_all().expect("export DrcCheckLive");
        DrcCheckReceipt::export_all().expect("export DrcCheckReceipt");
        DrcCheckOutcome::export_all().expect("export DrcCheckOutcome");
        DrcBookAsset::export_all().expect("export DrcBookAsset");
        DrcOfferBook::export_all().expect("export DrcOfferBook");
        DrcOfferFillMode::export_all().expect("export DrcOfferFillMode");
        DrcOfferTimeInForce::export_all().expect("export DrcOfferTimeInForce");
        DrcOfferCancelOutcome::export_all().expect("export DrcOfferCancelOutcome");
        DrcOfferCreateTx::export_all().expect("export DrcOfferCreateTx");
        DrcOfferCancelTx::export_all().expect("export DrcOfferCancelTx");
        DrcOfferLive::export_all().expect("export DrcOfferLive");
        DrcOfferCreateReceipt::export_all().expect("export DrcOfferCreateReceipt");
        DrcOfferCancelReceipt::export_all().expect("export DrcOfferCancelReceipt");
        DrcOfferView::export_all().expect("export DrcOfferView");
        DrcOfferCursor::export_all().expect("export DrcOfferCursor");
        DrcOfferBookCursor::export_all().expect("export DrcOfferBookCursor");
        DrcOfferPage::export_all().expect("export DrcOfferPage");
        DrcOfferBookPage::export_all().expect("export DrcOfferBookPage");
        DrcPaymentChannelCreateTx::export_all().expect("export DrcPaymentChannelCreateTx");
        DrcPaymentChannelFundTx::export_all().expect("export DrcPaymentChannelFundTx");
        DrcPaymentChannelClaimTx::export_all().expect("export DrcPaymentChannelClaimTx");
        DrcPaymentChannelCloseTx::export_all().expect("export DrcPaymentChannelCloseTx");
        DrcPaymentChannelCloseKind::export_all().expect("export DrcPaymentChannelCloseKind");
        DrcPaymentChannelLive::export_all().expect("export DrcPaymentChannelLive");
        DrcPaymentChannelReceipt::export_all().expect("export DrcPaymentChannelReceipt");
        DrcPaymentChannelOutcome::export_all().expect("export DrcPaymentChannelOutcome");
        DrcPaymentChannelFundEvent::export_all().expect("export DrcPaymentChannelFundEvent");
        DrcPaymentChannelClaimEvent::export_all().expect("export DrcPaymentChannelClaimEvent");
        DrcPaymentChannelScheduleEvent::export_all()
            .expect("export DrcPaymentChannelScheduleEvent");
        OvlExecutionTx::export_all().expect("export OvlExecutionTx");
        OvlWei::export_all().expect("export OvlWei");
        OvlFeeMarketParams::export_all().expect("export OvlFeeMarketParams");
        DrcPaymentTx::export_all().expect("export DrcPaymentTx");
        DrcPaymentOutboxEvent::export_all().expect("export DrcPaymentOutboxEvent");
        DrcPaymentResult::export_all().expect("export DrcPaymentResult");
        DrcPaymentReceipt::export_all().expect("export DrcPaymentReceipt");
        CheckpointState::export_all().expect("export CheckpointState");
        CheckpointBody::export_all().expect("export CheckpointBody");
        CheckpointAttestation::export_all().expect("export CheckpointAttestation");
        FinalityCertificate::export_all().expect("export FinalityCertificate");
        SignedStakeTx::export_all().expect("export SignedStakeTx");
        PassportCategory::export_all().expect("export PassportCategory");
        PassportAttestation::export_all().expect("export PassportAttestation");
        IssuedCurrencyCode::export_all().expect("export IssuedCurrencyCode");
        IssuedAmount::export_all().expect("export IssuedAmount");
        IssuedAssetId::export_all().expect("export IssuedAssetId");
        DrcTrustLineSetTx::export_all().expect("export DrcTrustLineSetTx");
        DrcTrustLineLive::export_all().expect("export DrcTrustLineLive");
        DrcIssuedTransferTx::export_all().expect("export DrcIssuedTransferTx");
        DrcIssuedTransferReceipt::export_all().expect("export DrcIssuedTransferReceipt");
        DrcIssuerLiability::export_all().expect("export DrcIssuerLiability");
        DrcIssuedAssetPolicyAction::export_all().expect("export DrcIssuedAssetPolicyAction");
        DrcIssuedAssetPolicySetTx::export_all().expect("export DrcIssuedAssetPolicySetTx");
        DrcIssuedAssetPolicyLive::export_all().expect("export DrcIssuedAssetPolicyLive");
        DrcIssuedAssetPolicyReceipt::export_all().expect("export DrcIssuedAssetPolicyReceipt");
        DrcTrustLineIssuerControlAction::export_all()
            .expect("export DrcTrustLineIssuerControlAction");
        DrcTrustLineIssuerControlTx::export_all().expect("export DrcTrustLineIssuerControlTx");
        DrcTrustLineIssuerControlReceipt::export_all()
            .expect("export DrcTrustLineIssuerControlReceipt");
        DrcIssuedClawbackTx::export_all().expect("export DrcIssuedClawbackTx");
        DrcIssuedClawbackReceipt::export_all().expect("export DrcIssuedClawbackReceipt");
        DrcMultisignOperationKind::export_all().expect("export DrcMultisignOperationKind");
        DrcLedgerObjectKind::export_all().expect("export DrcLedgerObjectKind");
        DrcLedgerObjectKey::export_all().expect("export DrcLedgerObjectKey");
        DrcLedgerObject::export_all().expect("export DrcLedgerObject");
        DrcLedgerObjectDescriptor::export_all().expect("export DrcLedgerObjectDescriptor");
        DrcOperationKind::export_all().expect("export DrcOperationKind");
        DrcOperation::export_all().expect("export DrcOperation");
        DrcAcceptedOperationReceipt::export_all().expect("export DrcAcceptedOperationReceipt");
        DrcLedgerObjectPage::export_all().expect("export DrcLedgerObjectPage");
        normalize_generated_bindings();

        let payment_binding = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/DrcPaymentTx.ts"),
        )
        .expect("read DRC payment binding");
        assert!(payment_binding.contains("source_tag: number | null"));
        assert!(payment_binding.contains("destination_tag: number | null"));
        let outbox_binding = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/DrcPaymentOutboxEvent.ts"),
        )
        .expect("read DRC outbox binding");
        assert!(outbox_binding.contains("source_tag: number | null"));
        assert!(outbox_binding.contains("destination_tag: number | null"));
        let receipt_binding = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/DrcPaymentReceipt.ts"),
        )
        .expect("read DRC receipt binding");
        assert!(receipt_binding.contains("requested_amount: Amount"));
        assert!(receipt_binding.contains("delivered_amount: Amount"));
        assert!(receipt_binding.contains("source_tag: number | null"));
        assert!(receipt_binding.contains("destination_tag: number | null"));
        let policy_binding = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/DrcAccountPolicyTx.ts"),
        )
        .expect("read DRC account-policy binding");
        assert!(policy_binding.contains("action: DrcAccountPolicyAction"));
        assert!(!policy_binding.contains("private"));
        let policy_state_binding = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/DrcAccountPolicy.ts"),
        )
        .expect("read DRC account-policy state binding");
        assert!(policy_state_binding.contains("deposit_auth_required: boolean"));
        assert!(policy_state_binding.contains("master_key_disabled: boolean"));
        let preauth_binding = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/DrcDepositPreauthTx.ts"),
        )
        .expect("read DRC deposit-preauthorization binding");
        assert!(preauth_binding.contains("authorized_source: Address"));
        assert!(!preauth_binding.contains("private"));
    }
}
