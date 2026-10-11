use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AccountTransfer, DataCommitmentAuthorization, DrcAccountPolicyTx, DrcCheckCancelTx,
    DrcCheckCashTx, DrcCheckCreateTx, DrcDepositPreauthTx, DrcEscrowCancelTx, DrcEscrowCreateTx,
    DrcEscrowFinishTx, DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx, DrcIssuedTransferTx,
    DrcMultisignBlockAttachment, DrcOfferCancelTx, DrcOfferCreateTx, DrcPaymentChannelClaimTx,
    DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, DrcPaymentTx,
    DrcRegularKeyTx, DrcSignerListTx, DrcTicketCreateTx, DrcTrustLineIssuerControlTx,
    DrcTrustLineSetTx, GrantRegistration, Hash, HubRegistration, MissionRegistration,
    OvlExecutionTx, PassportAttestation, SignedStakeTx, TltCovenantTx, Transaction,
    TreasuryDisbursement, VestingUnlock,
};

/// Explicit version/domain for bodies carrying native DRC offer operations.
pub const TRIDENT_BLOCK_BODY_V18_VERSION: u16 = 18;
pub const TRIDENT_BLOCK_BODY_V18_DOMAIN: &[u8] = b"agora-block-body-v18";
/// Combined trailing lane when TLT covenants are present (offers may also be).
///
/// Offer-only bodies keep the v18 wrap so DEX roots stay stable. Covenant bytes
/// cannot share that wrap: both prototypes used the same trailing slot.
pub const TRIDENT_BLOCK_BODY_V19_VERSION: u16 = 19;
pub const TRIDENT_BLOCK_BODY_V19_DOMAIN: &[u8] = b"agora-block-body-v19";
/// Combined trailing lane when signed passport attestations are present.
pub const TRIDENT_BLOCK_BODY_V20_VERSION: u16 = 20;
pub const TRIDENT_BLOCK_BODY_V20_DOMAIN: &[u8] = b"agora-block-body-v20";
/// Combined trailing lane when signed Hub / Grant / Mission registrations are present.
pub const TRIDENT_BLOCK_BODY_V21_VERSION: u16 = 21;
pub const TRIDENT_BLOCK_BODY_V21_DOMAIN: &[u8] = b"agora-block-body-v21";
/// Combined trailing lane when signed treasury disbursements are present.
pub const TRIDENT_BLOCK_BODY_V22_VERSION: u16 = 22;
pub const TRIDENT_BLOCK_BODY_V22_DOMAIN: &[u8] = b"agora-block-body-v22";
/// Combined trailing lane when signed vesting unlocks are present.
pub const TRIDENT_BLOCK_BODY_V23_VERSION: u16 = 23;
pub const TRIDENT_BLOCK_BODY_V23_DOMAIN: &[u8] = b"agora-block-body-v23";
/// Explicit version/domain for bodies carrying issued-control operations.
pub const TRIDENT_BLOCK_BODY_V17_VERSION: u16 = 17;
pub const TRIDENT_BLOCK_BODY_V17_DOMAIN: &[u8] = b"agora-block-body-v17";
pub const TRIDENT_BLOCK_BODY_V16_VERSION: u16 = 16;
pub const TRIDENT_BLOCK_BODY_V16_DOMAIN: &[u8] = b"agora-block-body-v16";
pub const TRIDENT_BLOCK_BODY_V15_VERSION: u16 = 15;
pub const TRIDENT_BLOCK_BODY_V15_DOMAIN: &[u8] = b"agora-block-body-v15";
/// Explicit version/domain for bodies carrying native DRC check operations.
pub const TRIDENT_BLOCK_BODY_V14_VERSION: u16 = 14;
pub const TRIDENT_BLOCK_BODY_V14_DOMAIN: &[u8] = b"agora-block-body-v14";
pub const TRIDENT_BLOCK_BODY_V13_VERSION: u16 = 13;
pub const TRIDENT_BLOCK_BODY_V13_DOMAIN: &[u8] = b"agora-block-body-v13";
/// Explicit version/domain for bodies carrying DRC ticket-create operations.
pub const TRIDENT_BLOCK_BODY_V12_VERSION: u16 = 12;
pub const TRIDENT_BLOCK_BODY_V12_DOMAIN: &[u8] = b"agora-block-body-v12";
/// Explicit version/domain for bodies carrying detached DRC multisign attachments.
pub const TRIDENT_BLOCK_BODY_V11_VERSION: u16 = 11;
pub const TRIDENT_BLOCK_BODY_V11_DOMAIN: &[u8] = b"agora-block-body-v11";
/// Explicit version/domain for bodies carrying DRC signer-list operations.
pub const TRIDENT_BLOCK_BODY_VERSION: u16 = 10;
pub const TRIDENT_BLOCK_BODY_DOMAIN: &[u8] = b"agora-block-body-v10";
const TRIDENT_BLOCK_BODY_V9_VERSION: u16 = 9;
const TRIDENT_BLOCK_BODY_V9_DOMAIN: &[u8] = b"agora-block-body-v9";
const TRIDENT_BLOCK_BODY_V8_VERSION: u16 = 8;
const TRIDENT_BLOCK_BODY_V8_DOMAIN: &[u8] = b"agora-block-body-v8";
const TRIDENT_BLOCK_BODY_V7_VERSION: u16 = 7;
const TRIDENT_BLOCK_BODY_V7_DOMAIN: &[u8] = b"agora-block-body-v7";
const TRIDENT_BLOCK_BODY_V6_VERSION: u16 = 6;
const TRIDENT_BLOCK_BODY_V6_DOMAIN: &[u8] = b"agora-block-body-v6";
const TRIDENT_BLOCK_BODY_V5_VERSION: u16 = 5;
const TRIDENT_BLOCK_BODY_V5_DOMAIN: &[u8] = b"agora-block-body-v5";

/// Block header for Agora's BlockDAG tips.
///
/// Multiple parents enable parallel block production; GHOSTDAG later imposes order.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct BlockHeader {
    pub version: u16,
    pub parents: Vec<Hash>,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub nonce: u64,
    pub tx_root: Hash,
}

impl BlockHeader {
    pub fn hash(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

/// Full Trident body with an OVL-only execution lane.
///
/// Every `drc_*` field is a closed, protocol-native state-machine lane. DRC
/// has no bytecode, VM, deploy, contract-call, or generic execution lane.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub header: BlockHeader,
    /// TLT UTXO lane (coinbase + transfers).
    pub transactions: Vec<Transaction>,
    /// OVL/DRC liquid account transfers (Trident).
    #[serde(default)]
    pub account_transfers: Vec<AccountTransfer>,
    /// OVL/DRC staking ops (Trident).
    #[serde(default)]
    pub stake_ops: Vec<SignedStakeTx>,
    /// Signed, gas-metered OVL execution envelopes.
    #[serde(default)]
    pub ovl_executions: Vec<OvlExecutionTx>,
    /// Signed native DRC payments with routing metadata.
    #[serde(default)]
    pub drc_payments: Vec<DrcPaymentTx>,
    /// Provenance-bound, operator-authorized data commitments.
    #[serde(default)]
    pub data_commitments: Vec<DataCommitmentAuthorization>,
    /// Owner-authorized, contract-free DRC recipient-policy operations.
    #[serde(default)]
    pub drc_account_policies: Vec<DrcAccountPolicyTx>,
    /// Owner-authorized, address-based DRC deposit preauthorizations.
    #[serde(default)]
    pub drc_deposit_preauths: Vec<DrcDepositPreauthTx>,
    /// Owner-authorized DRC regular-key rotation operations.
    #[serde(default)]
    pub drc_regular_keys: Vec<DrcRegularKeyTx>,
    /// Owner-authorized DRC weighted signer-list operations.
    #[serde(default)]
    pub drc_signer_lists: Vec<DrcSignerListTx>,
    /// Owner-authorized DRC ticket creation (one ticket per operation).
    #[serde(default)]
    pub drc_ticket_creates: Vec<DrcTicketCreateTx>,
    /// Owner-authorized native DRC escrow creates.
    #[serde(default)]
    pub drc_escrow_creates: Vec<DrcEscrowCreateTx>,
    /// Submitter-authorized native DRC escrow finishes.
    #[serde(default)]
    pub drc_escrow_finishes: Vec<DrcEscrowFinishTx>,
    /// Submitter-authorized native DRC escrow cancels.
    #[serde(default)]
    pub drc_escrow_cancels: Vec<DrcEscrowCancelTx>,
    /// Owner-authorized native DRC check creates.
    #[serde(default)]
    pub drc_check_creates: Vec<DrcCheckCreateTx>,
    /// Destination-authorized native DRC check cash (exact value).
    #[serde(default)]
    pub drc_check_cashes: Vec<DrcCheckCashTx>,
    /// Authorized native DRC check cancels.
    #[serde(default)]
    pub drc_check_cancels: Vec<DrcCheckCancelTx>,
    /// Owner-authorized native DRC payment channel creates.
    #[serde(default)]
    pub drc_payment_channel_creates: Vec<DrcPaymentChannelCreateTx>,
    /// Owner-authorized native DRC payment channel funds.
    #[serde(default)]
    pub drc_payment_channel_funds: Vec<DrcPaymentChannelFundTx>,
    /// Destination-authorized native DRC payment channel claims.
    #[serde(default)]
    pub drc_payment_channel_claims: Vec<DrcPaymentChannelClaimTx>,
    /// Authorized native DRC payment channel closes.
    #[serde(default)]
    pub drc_payment_channel_closes: Vec<DrcPaymentChannelCloseTx>,
    /// Holder-authorized issuer-scoped trust line set/delete operations.
    #[serde(default)]
    pub drc_trust_line_sets: Vec<DrcTrustLineSetTx>,
    /// Exact issued-value transfers (issue/redeem/holder transfer).
    #[serde(default)]
    pub drc_issued_transfers: Vec<DrcIssuedTransferTx>,
    /// Issuer-scoped asset policy (auth/freeze/clawback flags).
    #[serde(default)]
    pub drc_issued_asset_policy_sets: Vec<DrcIssuedAssetPolicySetTx>,
    /// Issuer line authorize/freeze/deep-freeze controls.
    #[serde(default)]
    pub drc_trust_line_issuer_controls: Vec<DrcTrustLineIssuerControlTx>,
    /// Issuer exact clawback from one holder line.
    #[serde(default)]
    pub drc_issued_clawbacks: Vec<DrcIssuedClawbackTx>,
    /// Detached, body-root-committed DRC multisign authorization (consensus lane).
    #[serde(default)]
    pub drc_multisign_attachments: Vec<DrcMultisignBlockAttachment>,
    /// Native order-book offers. Empty lanes stay absent from frozen pre-v18 bytes.
    #[serde(default)]
    pub drc_offer_creates: Vec<DrcOfferCreateTx>,
    #[serde(default)]
    pub drc_offer_cancels: Vec<DrcOfferCancelTx>,
    /// TLT covenant spends. Absent from the wire when empty so v1 block bytes stay frozen.
    #[serde(default)]
    pub tlt_covenants: Vec<TltCovenantTx>,
    /// Signed Hub-coordinator passport attestations. Empty stays off the frozen wire.
    #[serde(default)]
    pub passport_attestations: Vec<PassportAttestation>,
    /// Signed hub registrations. Empty stays off the frozen wire.
    #[serde(default)]
    pub hub_registrations: Vec<HubRegistration>,
    /// Signed grant registrations. Empty stays off the frozen wire.
    #[serde(default)]
    pub grant_registrations: Vec<GrantRegistration>,
    /// Signed mission registrations. Empty stays off the frozen wire.
    #[serde(default)]
    pub mission_registrations: Vec<MissionRegistration>,
    /// Signed protocol treasury spends. Empty stays off the frozen wire.
    #[serde(default)]
    pub treasury_disbursements: Vec<TreasuryDisbursement>,
    /// Signed vesting unlock claims. Empty stays off the frozen wire.
    #[serde(default)]
    pub vesting_unlocks: Vec<VestingUnlock>,
}

impl Block {
    /// Construct a UTXO-only block (empty account/stake lanes).
    pub fn utxo(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
        Self {
            header,
            transactions,
            account_transfers: Vec::new(),
            stake_ops: Vec::new(),
            ovl_executions: Vec::new(),
            drc_payments: Vec::new(),
            data_commitments: Vec::new(),
            drc_account_policies: Vec::new(),
            drc_deposit_preauths: Vec::new(),
            drc_regular_keys: Vec::new(),
            drc_signer_lists: Vec::new(),
            drc_ticket_creates: Vec::new(),
            drc_escrow_creates: Vec::new(),
            drc_escrow_finishes: Vec::new(),
            drc_escrow_cancels: Vec::new(),
            drc_check_creates: Vec::new(),
            drc_check_cashes: Vec::new(),
            drc_check_cancels: Vec::new(),
            drc_payment_channel_creates: Vec::new(),
            drc_payment_channel_funds: Vec::new(),
            drc_payment_channel_claims: Vec::new(),
            drc_payment_channel_closes: Vec::new(),
            drc_trust_line_sets: Vec::new(),
            drc_issued_transfers: Vec::new(),
            drc_issued_asset_policy_sets: Vec::new(),
            drc_trust_line_issuer_controls: Vec::new(),
            drc_issued_clawbacks: Vec::new(),
            drc_offer_creates: Vec::new(),
            drc_offer_cancels: Vec::new(),
            drc_multisign_attachments: Vec::new(),
            tlt_covenants: Vec::new(),
            passport_attestations: Vec::new(),
            hub_registrations: Vec::new(),
            grant_registrations: Vec::new(),
            mission_registrations: Vec::new(),
            treasury_disbursements: Vec::new(),
            vesting_unlocks: Vec::new(),
        }
    }

    pub fn id(&self) -> Hash {
        self.header.hash()
    }

    fn has_post_v4_body_lanes(&self) -> bool {
        !self.data_commitments.is_empty()
            || !self.drc_account_policies.is_empty()
            || !self.drc_deposit_preauths.is_empty()
            || !self.drc_regular_keys.is_empty()
            || !self.drc_signer_lists.is_empty()
            || !self.drc_ticket_creates.is_empty()
            || !self.drc_escrow_creates.is_empty()
            || !self.drc_escrow_finishes.is_empty()
            || !self.drc_escrow_cancels.is_empty()
            || !self.drc_check_creates.is_empty()
            || !self.drc_check_cashes.is_empty()
            || !self.drc_check_cancels.is_empty()
            || !self.drc_payment_channel_creates.is_empty()
            || !self.drc_payment_channel_funds.is_empty()
            || !self.drc_payment_channel_claims.is_empty()
            || !self.drc_payment_channel_closes.is_empty()
            || !self.drc_trust_line_sets.is_empty()
            || !self.drc_issued_transfers.is_empty()
            || !self.drc_issued_asset_policy_sets.is_empty()
            || !self.drc_trust_line_issuer_controls.is_empty()
            || !self.drc_issued_clawbacks.is_empty()
            || !self.drc_multisign_attachments.is_empty()
            || !self.drc_offer_creates.is_empty()
            || !self.drc_offer_cancels.is_empty()
            || !self.tlt_covenants.is_empty()
            || !self.passport_attestations.is_empty()
            || !self.hub_registrations.is_empty()
            || !self.grant_registrations.is_empty()
            || !self.mission_registrations.is_empty()
            || !self.treasury_disbursements.is_empty()
            || !self.vesting_unlocks.is_empty()
    }

    /// Compact gossip is UTXO-only unless a versioned typed compact names each
    /// lane. Typed compact names every live body field, including detached DRC
    /// multisign attachments (protocol v28 gossip + lane 29).
    pub fn requires_full_body_gossip(&self) -> bool {
        !self.account_transfers.is_empty()
            || !self.stake_ops.is_empty()
            || !self.ovl_executions.is_empty()
            || !self.drc_payments.is_empty()
            || self.has_post_v4_body_lanes()
    }

    /// Historical name: every current lane is named. Kept so callers can still
    /// fail closed if a future body field is added without a compact kind.
    pub fn typed_compact_unnamed_lanes(&self) -> bool {
        false
    }

    /// Compute a simple pairwise tx merkle root (duplicate last leaf when odd).
    ///
    /// Kept intentionally minimal for Phase 1 ID stability; consensus may harden this later.
    pub fn compute_tx_root(transactions: &[Transaction]) -> Hash {
        if transactions.is_empty() {
            return Hash::ZERO;
        }
        let mut level: Vec<Hash> = transactions.iter().map(Transaction::tx_id).collect();
        while level.len() > 1 {
            if level.len() % 2 == 1 {
                level.push(*level.last().expect("non-empty"));
            }
            level = level
                .chunks(2)
                .map(|pair| {
                    let mut buf = [0u8; 64];
                    buf[..32].copy_from_slice(pair[0].as_bytes());
                    buf[32..].copy_from_slice(pair[1].as_bytes());
                    Hash::hash_bytes(&buf)
                })
                .collect();
        }
        level[0]
    }

    /// Body commitment for `header.tx_root`.
    ///
    /// UTXO-only blocks keep the legacy merkle root; account/stake-only bodies
    /// retain v2; OVL execution uses v3; DRC payments use v4; authenticated
    /// data commitments use v5; DRC account policies use v6; address-based DRC
    /// deposit preauthorizations use v7; payment-v4 expiry uses v8; regular keys use v9;
    /// signer lists use v10; detached multisign attachments use v11; ticket creates use v12;
    /// native DRC escrow uses v13; native DRC checks use v14; native DRC payment channels use v15;
    /// trust lines use v16; issued controls use v17; native DRC offers use v18;
    /// TLT covenants wrap v19 so they cannot collide with the offer v18 slot.
    pub fn compute_body_root(&self) -> Hash {
        let mut inner = self.compute_body_root_with_multisign_attachments();
        if !self.drc_ticket_creates.is_empty() {
            let ticket_ids: Vec<Hash> = self
                .drc_ticket_creates
                .iter()
                .map(DrcTicketCreateTx::ticket_create_tx_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V12_DOMAIN,
                TRIDENT_BLOCK_BODY_V12_VERSION,
                inner,
                ticket_ids,
            ));
        }
        if !self.drc_escrow_creates.is_empty()
            || !self.drc_escrow_finishes.is_empty()
            || !self.drc_escrow_cancels.is_empty()
        {
            let create_ids: Vec<Hash> = self
                .drc_escrow_creates
                .iter()
                .map(DrcEscrowCreateTx::escrow_id)
                .collect();
            let finish_ids: Vec<Hash> = self
                .drc_escrow_finishes
                .iter()
                .map(DrcEscrowFinishTx::finish_tx_id)
                .collect();
            let cancel_ids: Vec<Hash> = self
                .drc_escrow_cancels
                .iter()
                .map(DrcEscrowCancelTx::cancel_tx_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V13_DOMAIN,
                TRIDENT_BLOCK_BODY_V13_VERSION,
                inner,
                create_ids,
                finish_ids,
                cancel_ids,
            ));
        }
        if !self.drc_check_creates.is_empty()
            || !self.drc_check_cashes.is_empty()
            || !self.drc_check_cancels.is_empty()
        {
            let create_ids: Vec<Hash> = self
                .drc_check_creates
                .iter()
                .map(DrcCheckCreateTx::check_id)
                .collect();
            let cash_ids: Vec<Hash> = self
                .drc_check_cashes
                .iter()
                .map(DrcCheckCashTx::cash_tx_id)
                .collect();
            let cancel_ids: Vec<Hash> = self
                .drc_check_cancels
                .iter()
                .map(DrcCheckCancelTx::cancel_tx_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V14_DOMAIN,
                TRIDENT_BLOCK_BODY_V14_VERSION,
                inner,
                create_ids,
                cash_ids,
                cancel_ids,
            ));
        }
        if !self.drc_payment_channel_creates.is_empty()
            || !self.drc_payment_channel_funds.is_empty()
            || !self.drc_payment_channel_claims.is_empty()
            || !self.drc_payment_channel_closes.is_empty()
        {
            let create_ids: Vec<Hash> = self
                .drc_payment_channel_creates
                .iter()
                .map(DrcPaymentChannelCreateTx::channel_id)
                .collect();
            let fund_ids: Vec<Hash> = self
                .drc_payment_channel_funds
                .iter()
                .map(DrcPaymentChannelFundTx::fund_tx_id)
                .collect();
            let claim_ids: Vec<Hash> = self
                .drc_payment_channel_claims
                .iter()
                .map(DrcPaymentChannelClaimTx::claim_tx_id)
                .collect();
            let close_ids: Vec<Hash> = self
                .drc_payment_channel_closes
                .iter()
                .map(DrcPaymentChannelCloseTx::close_tx_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V15_DOMAIN,
                TRIDENT_BLOCK_BODY_V15_VERSION,
                inner,
                create_ids,
                fund_ids,
                claim_ids,
                close_ids,
            ));
        }
        if !self.drc_trust_line_sets.is_empty() || !self.drc_issued_transfers.is_empty() {
            let set_ids: Vec<Hash> = self
                .drc_trust_line_sets
                .iter()
                .map(DrcTrustLineSetTx::trust_line_set_tx_id)
                .collect();
            let transfer_ids: Vec<Hash> = self
                .drc_issued_transfers
                .iter()
                .map(DrcIssuedTransferTx::issued_transfer_tx_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V16_DOMAIN,
                TRIDENT_BLOCK_BODY_V16_VERSION,
                inner,
                set_ids,
                transfer_ids,
            ));
        }
        if !self.drc_issued_asset_policy_sets.is_empty()
            || !self.drc_trust_line_issuer_controls.is_empty()
            || !self.drc_issued_clawbacks.is_empty()
        {
            let policy_ids: Vec<Hash> = self
                .drc_issued_asset_policy_sets
                .iter()
                .map(DrcIssuedAssetPolicySetTx::policy_set_tx_id)
                .collect();
            let control_ids: Vec<Hash> = self
                .drc_trust_line_issuer_controls
                .iter()
                .map(DrcTrustLineIssuerControlTx::issuer_control_tx_id)
                .collect();
            let clawback_ids: Vec<Hash> = self
                .drc_issued_clawbacks
                .iter()
                .map(DrcIssuedClawbackTx::clawback_tx_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V17_DOMAIN,
                TRIDENT_BLOCK_BODY_V17_VERSION,
                inner,
                policy_ids,
                control_ids,
                clawback_ids,
            ));
        }
        if !self.drc_offer_creates.is_empty() || !self.drc_offer_cancels.is_empty() {
            let create_ids: Vec<Hash> = self
                .drc_offer_creates
                .iter()
                .map(DrcOfferCreateTx::offer_id)
                .collect();
            let cancel_ids: Vec<Hash> = self
                .drc_offer_cancels
                .iter()
                .map(DrcOfferCancelTx::cancel_tx_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V18_DOMAIN,
                TRIDENT_BLOCK_BODY_V18_VERSION,
                inner,
                create_ids,
                cancel_ids,
            ));
        }
        if !self.tlt_covenants.is_empty() {
            let covenant_ids: Vec<Hash> = self
                .tlt_covenants
                .iter()
                .map(TltCovenantTx::tx_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V19_DOMAIN,
                TRIDENT_BLOCK_BODY_V19_VERSION,
                inner,
                covenant_ids,
            ));
        }
        if !self.passport_attestations.is_empty() {
            let ids: Vec<Hash> = self
                .passport_attestations
                .iter()
                .map(PassportAttestation::attestation_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V20_DOMAIN,
                TRIDENT_BLOCK_BODY_V20_VERSION,
                inner,
                ids,
            ));
        }
        if !self.hub_registrations.is_empty()
            || !self.grant_registrations.is_empty()
            || !self.mission_registrations.is_empty()
        {
            let hub_ids: Vec<Hash> = self
                .hub_registrations
                .iter()
                .map(HubRegistration::registration_id)
                .collect();
            let grant_ids: Vec<Hash> = self
                .grant_registrations
                .iter()
                .map(GrantRegistration::registration_id)
                .collect();
            let mission_ids: Vec<Hash> = self
                .mission_registrations
                .iter()
                .map(MissionRegistration::registration_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V21_DOMAIN,
                TRIDENT_BLOCK_BODY_V21_VERSION,
                inner,
                hub_ids,
                grant_ids,
                mission_ids,
            ));
        }
        if !self.treasury_disbursements.is_empty() {
            let ids: Vec<Hash> = self
                .treasury_disbursements
                .iter()
                .map(TreasuryDisbursement::disbursement_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V22_DOMAIN,
                TRIDENT_BLOCK_BODY_V22_VERSION,
                inner,
                ids,
            ));
        }
        if !self.vesting_unlocks.is_empty() {
            let ids: Vec<Hash> = self
                .vesting_unlocks
                .iter()
                .map(VestingUnlock::unlock_id)
                .collect();
            inner = Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V23_DOMAIN,
                TRIDENT_BLOCK_BODY_V23_VERSION,
                inner,
                ids,
            ));
        }
        inner
    }

    fn compute_body_root_with_multisign_attachments(&self) -> Hash {
        let inner = self.compute_body_root_with_signer_lists();
        if !self.drc_multisign_attachments.is_empty() {
            let attachment_ids: Vec<Hash> = self
                .drc_multisign_attachments
                .iter()
                .map(DrcMultisignBlockAttachment::body_commitment_id)
                .collect();
            return Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V11_DOMAIN,
                TRIDENT_BLOCK_BODY_V11_VERSION,
                inner,
                attachment_ids,
            ));
        }
        inner
    }

    fn compute_body_root_with_signer_lists(&self) -> Hash {
        let inner = self.compute_body_root_with_regular_keys();
        if !self.drc_signer_lists.is_empty() {
            let signer_list_ids: Vec<Hash> = self
                .drc_signer_lists
                .iter()
                .map(DrcSignerListTx::signer_list_tx_id)
                .collect();
            return Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_DOMAIN,
                TRIDENT_BLOCK_BODY_VERSION,
                inner,
                signer_list_ids,
            ));
        }
        inner
    }

    fn compute_body_root_with_regular_keys(&self) -> Hash {
        let inner = self.compute_body_root_with_payment_v4_expiry();
        if !self.drc_regular_keys.is_empty() {
            let regular_key_ids: Vec<Hash> = self
                .drc_regular_keys
                .iter()
                .map(DrcRegularKeyTx::regular_key_tx_id)
                .collect();
            return Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V9_DOMAIN,
                TRIDENT_BLOCK_BODY_V9_VERSION,
                inner,
                regular_key_ids,
            ));
        }
        inner
    }

    fn compute_body_root_with_payment_v4_expiry(&self) -> Hash {
        if self
            .drc_payments
            .iter()
            .any(|payment| payment.version >= crate::DRC_PAYMENT_VERSION)
        {
            let payment_ids: Vec<Hash> = self
                .drc_payments
                .iter()
                .map(DrcPaymentTx::payment_id)
                .collect();
            return Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V8_DOMAIN,
                TRIDENT_BLOCK_BODY_V8_VERSION,
                self.compute_body_root_v7(),
                payment_ids,
            ));
        }
        self.compute_body_root_v7()
    }

    fn compute_body_root_v7(&self) -> Hash {
        if !self.drc_deposit_preauths.is_empty() {
            let preauth_ids: Vec<Hash> = self
                .drc_deposit_preauths
                .iter()
                .map(DrcDepositPreauthTx::preauth_tx_id)
                .collect();
            return Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V7_DOMAIN,
                TRIDENT_BLOCK_BODY_V7_VERSION,
                self.compute_body_root_v6(),
                preauth_ids,
            ));
        }
        self.compute_body_root_v6()
    }

    fn compute_body_root_v6(&self) -> Hash {
        if !self.drc_account_policies.is_empty() {
            let policy_ids: Vec<Hash> = self
                .drc_account_policies
                .iter()
                .map(DrcAccountPolicyTx::policy_tx_id)
                .collect();
            return Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V6_DOMAIN,
                TRIDENT_BLOCK_BODY_V6_VERSION,
                self.compute_body_root_v5(),
                policy_ids,
            ));
        }
        self.compute_body_root_v5()
    }

    fn compute_body_root_v5(&self) -> Hash {
        if !self.data_commitments.is_empty() {
            let authorization_ids: Vec<Hash> = self
                .data_commitments
                .iter()
                .map(DataCommitmentAuthorization::authorization_id)
                .collect();
            return Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V5_DOMAIN,
                TRIDENT_BLOCK_BODY_V5_VERSION,
                self.compute_body_root_v4(),
                authorization_ids,
            ));
        }
        self.compute_body_root_v4()
    }

    fn compute_body_root_v4(&self) -> Hash {
        if !self.drc_payments.is_empty() {
            let payment_ids: Vec<Hash> = self
                .drc_payments
                .iter()
                .map(DrcPaymentTx::payment_id)
                .collect();
            return Hash::hash_borsh(&(
                b"agora-block-body-v4",
                self.compute_body_root_v3(),
                payment_ids,
            ));
        }
        self.compute_body_root_v3()
    }

    fn compute_body_root_v3(&self) -> Hash {
        if !self.ovl_executions.is_empty() {
            let execution_ids: Vec<Hash> = self
                .ovl_executions
                .iter()
                .map(OvlExecutionTx::tx_id)
                .collect();
            return Hash::hash_borsh(&(
                b"agora-block-body-v3",
                self.compute_body_root_v2(),
                execution_ids,
            ));
        }
        self.compute_body_root_v2()
    }

    fn compute_body_root_v2(&self) -> Hash {
        if self.account_transfers.is_empty() && self.stake_ops.is_empty() {
            return Self::compute_tx_root(&self.transactions);
        }
        let account_ids: Vec<Hash> = self
            .account_transfers
            .iter()
            .map(AccountTransfer::transfer_id)
            .collect();
        let stake_ids: Vec<Hash> = self
            .stake_ops
            .iter()
            .map(SignedStakeTx::stake_tx_id)
            .collect();
        Hash::hash_borsh(&(
            b"agora-block-body-v2",
            Self::compute_tx_root(&self.transactions),
            account_ids,
            stake_ids,
        ))
    }
}

impl BorshSerialize for Block {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        BorshSerialize::serialize(&self.header, writer)?;
        BorshSerialize::serialize(&self.transactions, writer)?;
        BorshSerialize::serialize(&self.account_transfers, writer)?;
        BorshSerialize::serialize(&self.stake_ops, writer)?;
        BorshSerialize::serialize(&self.ovl_executions, writer)?;
        BorshSerialize::serialize(&self.drc_payments, writer)?;

        // Frozen v2 bytes include the original account, stake, OVL, and DRC
        // lanes; later empty lanes must not extend that canonical encoding.
        // A covenant spend is the exception: the reader finds it after the
        // post-v4 lanes, so those lanes are written (empty if unused).
        if !self.has_post_v4_body_lanes() && self.tlt_covenants.is_empty() {
            return Ok(());
        }

        BorshSerialize::serialize(&self.data_commitments, writer)?;
        BorshSerialize::serialize(&self.drc_account_policies, writer)?;
        BorshSerialize::serialize(&self.drc_deposit_preauths, writer)?;
        BorshSerialize::serialize(&self.drc_regular_keys, writer)?;
        BorshSerialize::serialize(&self.drc_signer_lists, writer)?;
        BorshSerialize::serialize(&self.drc_ticket_creates, writer)?;
        BorshSerialize::serialize(&self.drc_escrow_creates, writer)?;
        BorshSerialize::serialize(&self.drc_escrow_finishes, writer)?;
        BorshSerialize::serialize(&self.drc_escrow_cancels, writer)?;
        BorshSerialize::serialize(&self.drc_check_creates, writer)?;
        BorshSerialize::serialize(&self.drc_check_cashes, writer)?;
        BorshSerialize::serialize(&self.drc_check_cancels, writer)?;
        BorshSerialize::serialize(&self.drc_payment_channel_creates, writer)?;
        BorshSerialize::serialize(&self.drc_payment_channel_funds, writer)?;
        BorshSerialize::serialize(&self.drc_payment_channel_claims, writer)?;
        BorshSerialize::serialize(&self.drc_payment_channel_closes, writer)?;
        BorshSerialize::serialize(&self.drc_trust_line_sets, writer)?;
        BorshSerialize::serialize(&self.drc_issued_transfers, writer)?;
        BorshSerialize::serialize(&self.drc_issued_asset_policy_sets, writer)?;
        BorshSerialize::serialize(&self.drc_trust_line_issuer_controls, writer)?;
        BorshSerialize::serialize(&self.drc_issued_clawbacks, writer)?;
        BorshSerialize::serialize(&self.drc_multisign_attachments, writer)?;
        let has_community_registrations = !self.hub_registrations.is_empty()
            || !self.grant_registrations.is_empty()
            || !self.mission_registrations.is_empty();
        let has_treasury = !self.treasury_disbursements.is_empty();
        let has_vesting = !self.vesting_unlocks.is_empty();
        if self.drc_offer_creates.is_empty()
            && self.drc_offer_cancels.is_empty()
            && self.tlt_covenants.is_empty()
            && self.passport_attestations.is_empty()
            && !has_community_registrations
            && !has_treasury
            && !has_vesting
        {
            return Ok(());
        }
        BorshSerialize::serialize(&self.drc_offer_creates, writer)?;
        BorshSerialize::serialize(&self.drc_offer_cancels, writer)?;
        if !self.tlt_covenants.is_empty()
            || !self.passport_attestations.is_empty()
            || has_community_registrations
            || has_treasury
            || has_vesting
        {
            BorshSerialize::serialize(&self.tlt_covenants, writer)?;
        }
        if !self.passport_attestations.is_empty()
            || has_community_registrations
            || has_treasury
            || has_vesting
        {
            BorshSerialize::serialize(&self.passport_attestations, writer)?;
        }
        if has_community_registrations || has_treasury || has_vesting {
            BorshSerialize::serialize(&self.hub_registrations, writer)?;
        }
        if !self.grant_registrations.is_empty()
            || !self.mission_registrations.is_empty()
            || has_treasury
            || has_vesting
        {
            BorshSerialize::serialize(&self.grant_registrations, writer)?;
        }
        if !self.mission_registrations.is_empty() || has_treasury || has_vesting {
            BorshSerialize::serialize(&self.mission_registrations, writer)?;
        }
        if has_treasury || has_vesting {
            BorshSerialize::serialize(&self.treasury_disbursements, writer)?;
        }
        if has_vesting {
            BorshSerialize::serialize(&self.vesting_unlocks, writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for Block {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            header: BlockHeader::deserialize_reader(reader)?,
            transactions: Vec::<Transaction>::deserialize_reader(reader)?,
            account_transfers: deserialize_trailing_vec(reader)?,
            stake_ops: deserialize_trailing_vec(reader)?,
            ovl_executions: deserialize_trailing_vec(reader)?,
            drc_payments: deserialize_trailing_vec(reader)?,
            data_commitments: deserialize_trailing_vec(reader)?,
            drc_account_policies: deserialize_trailing_vec(reader)?,
            drc_deposit_preauths: deserialize_trailing_vec(reader)?,
            drc_regular_keys: deserialize_trailing_vec(reader)?,
            drc_signer_lists: deserialize_trailing_vec(reader)?,
            drc_ticket_creates: deserialize_trailing_vec(reader)?,
            drc_escrow_creates: deserialize_trailing_vec(reader)?,
            drc_escrow_finishes: deserialize_trailing_vec(reader)?,
            drc_escrow_cancels: deserialize_trailing_vec(reader)?,
            drc_check_creates: deserialize_trailing_vec(reader)?,
            drc_check_cashes: deserialize_trailing_vec(reader)?,
            drc_check_cancels: deserialize_trailing_vec(reader)?,
            drc_payment_channel_creates: deserialize_trailing_vec(reader)?,
            drc_payment_channel_funds: deserialize_trailing_vec(reader)?,
            drc_payment_channel_claims: deserialize_trailing_vec(reader)?,
            drc_payment_channel_closes: deserialize_trailing_vec(reader)?,
            drc_trust_line_sets: deserialize_trailing_vec(reader)?,
            drc_issued_transfers: deserialize_trailing_vec(reader)?,
            drc_issued_asset_policy_sets: deserialize_trailing_vec(reader)?,
            drc_trust_line_issuer_controls: deserialize_trailing_vec(reader)?,
            drc_issued_clawbacks: deserialize_trailing_vec(reader)?,
            drc_multisign_attachments: deserialize_trailing_vec(reader)?,
            drc_offer_creates: deserialize_trailing_vec(reader)?,
            drc_offer_cancels: deserialize_trailing_vec(reader)?,
            tlt_covenants: deserialize_trailing_vec(reader)?,
            passport_attestations: deserialize_trailing_vec(reader)?,
            hub_registrations: deserialize_trailing_vec(reader)?,
            grant_registrations: deserialize_trailing_vec(reader)?,
            mission_registrations: deserialize_trailing_vec(reader)?,
            treasury_disbursements: deserialize_trailing_vec(reader)?,
            vesting_unlocks: deserialize_trailing_vec(reader)?,
        })
    }
}

/// Appended body lanes are optional only at an exact legacy end-of-input
/// boundary. A partial vector length or element remains a hard decode failure.
fn deserialize_trailing_vec<T, R>(reader: &mut R) -> Result<Vec<T>, borsh::io::Error>
where
    T: BorshDeserialize,
    R: borsh::io::Read,
{
    let Some(len) = deserialize_optional_len(reader)? else {
        return Ok(Vec::new());
    };
    let mut values = Vec::with_capacity((len as usize).min(1024));
    for _ in 0..len {
        values.push(T::deserialize_reader(reader)?);
    }
    Ok(values)
}

fn deserialize_optional_len<R: borsh::io::Read>(
    reader: &mut R,
) -> Result<Option<u32>, borsh::io::Error> {
    let mut bytes = [0u8; 4];
    let mut filled = 0usize;
    while filled < bytes.len() {
        match reader.read(&mut bytes[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => {
                return Err(borsh::io::Error::new(
                    borsh::io::ErrorKind::UnexpectedEof,
                    "partial trailing block-lane length",
                ));
            }
            Ok(read) => filled += read,
            Err(err) if err.kind() == borsh::io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(Some(u32::from_le_bytes(bytes)))
}

#[cfg(test)]
mod tests {
    use crate::{
        Address, Amount, DataAvailabilityCommitment, DrcAccountPolicyTx, DrcCheckCreateTx,
        DrcDepositPreauthTx, DrcPaymentTx, DRC_CHECK_CREATE_TX_VERSION,
    };

    use super::*;

    const FROZEN_V2_HEADER_BORSH_HEX: &str = concat!(
        "0100000000000090ebc49f010000000000000000000000000000",
        "6069a3710cacc620ebb4bdb9d3d3107c85371c17782d94cf6b6fb76fb05111bd"
    );
    const FROZEN_V2_BLOCK_BORSH_HEX: &str = concat!(
        "0100000000000090ebc49f0100000000000000000000000000006069a3710cacc620ebb4",
        "bdb9d3d3107c85371c17782d94cf6b6fb76fb05111bd0100000001000000000000000100",
        "00000080c6a47e8d0300ff9ec96f09eb154d038a552ecae59c50204ea9a9000000000000",
        "0000000000000000000000000000000000000000000000000000"
    );
    const FROZEN_V2_GENESIS_HASH: &str =
        "afe59232cd20a16bd56948044149d2b8013e63f3694c113074fef75ab0cb9b98";

    fn frozen_v2_testnet_block() -> Block {
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![crate::TxOut {
                value: Amount::from_base_units(1_000_000_000_000_000),
                address: Address::from_hex("ff9ec96f09eb154d038a552ecae59c50204ea9a9").unwrap(),
            }],
            0,
        );
        Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 1_785_715_200_000,
                bits: 0,
                nonce: 0,
                tx_root: Block::compute_tx_root(std::slice::from_ref(&coinbase)),
            },
            vec![coinbase],
        )
    }

    #[test]
    fn frozen_v2_header_and_block_bytes_are_unchanged() {
        let block = frozen_v2_testnet_block();
        let header_bytes = borsh::to_vec(&block.header).unwrap();
        let block_bytes = borsh::to_vec(&block).unwrap();

        assert_eq!(hex::encode(&header_bytes), FROZEN_V2_HEADER_BORSH_HEX);
        assert_eq!(hex::encode(&block_bytes), FROZEN_V2_BLOCK_BORSH_HEX);
        assert_eq!(block.header.hash().to_hex(), FROZEN_V2_GENESIS_HASH);
        assert_eq!(
            borsh::from_slice::<BlockHeader>(&header_bytes).unwrap(),
            block.header
        );
        assert_eq!(borsh::from_slice::<Block>(&block_bytes).unwrap(), block);
    }

    #[test]
    fn drc_payment_activates_body_root_v4() {
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
        let legacy = block.compute_body_root();
        block.drc_payments.push(DrcPaymentTx::unsigned(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            9,
            Hash([3; 32]),
            0,
        ));
        assert_ne!(block.compute_body_root(), legacy);
        assert_eq!(
            block.compute_body_root(),
            Hash::hash_borsh(&(
                b"agora-block-body-v4",
                legacy,
                vec![block.drc_payments[0].payment_id()]
            ))
        );
    }

    #[test]
    fn drc_v2_source_tag_changes_the_committed_body_root() {
        let mut untagged = Block::utxo(
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
        untagged.drc_payments.push(DrcPaymentTx::unsigned_v2(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            9,
            None,
            Hash([3; 32]),
            0,
        ));
        let mut tagged = untagged.clone();
        tagged.drc_payments[0].source_tag = Some(u32::MAX);

        assert_ne!(
            untagged.drc_payments[0].payment_id(),
            tagged.drc_payments[0].payment_id()
        );
        assert_ne!(untagged.compute_body_root(), tagged.compute_body_root());
    }

    #[test]
    fn data_commitment_activates_body_root_v5() {
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
        let legacy = block.compute_body_root();
        block
            .data_commitments
            .push(DataCommitmentAuthorization::unsigned(
                Address([7; 20]),
                0,
                DataAvailabilityCommitment::agora_layers_ovolos_batch(
                    "agora-ovolos-testnet-1".into(),
                    Hash([1; 32]),
                    Hash([2; 32]),
                    3,
                    Hash([4; 32]),
                    Hash([5; 32]),
                    Hash([6; 32]),
                    7,
                    8,
                ),
            ));
        let first = block.compute_body_root();
        assert_ne!(first, legacy);
        assert_eq!(
            first,
            Hash::hash_borsh(&(
                b"agora-block-body-v5".as_slice(),
                5u16,
                legacy,
                vec![block.data_commitments[0].authorization_id()]
            ))
        );

        block.data_commitments[0].replay_nonce += 1;
        assert_ne!(block.compute_body_root(), first);
        let bytes = borsh::to_vec(&block).unwrap();
        assert_eq!(Block::try_from_slice(&bytes).unwrap(), block);
    }

    #[test]
    fn drc_policy_activates_body_root_v6_and_commits_action() {
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
        let legacy = block.compute_body_root();
        block
            .drc_account_policies
            .push(DrcAccountPolicyTx::set_require_destination_tag(
                Address([7; 20]),
                Amount::from_base_units(1),
                2,
            ));
        let set_root = block.compute_body_root();
        assert_eq!(
            set_root,
            Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V6_DOMAIN,
                TRIDENT_BLOCK_BODY_V6_VERSION,
                legacy,
                vec![block.drc_account_policies[0].policy_tx_id()]
            ))
        );
        block.drc_account_policies[0].action =
            crate::DrcAccountPolicyAction::ClearRequireDestinationTag;
        assert_ne!(block.compute_body_root(), set_root);
        let bytes = borsh::to_vec(&block).unwrap();
        assert_eq!(Block::try_from_slice(&bytes).unwrap(), block);
    }

    #[test]
    fn drc_deposit_preauth_activates_body_root_v7_and_commits_action() {
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
        let legacy = block.compute_body_root();
        block
            .drc_deposit_preauths
            .push(DrcDepositPreauthTx::authorize(
                Address([7; 20]),
                Address([8; 20]),
                Amount::from_base_units(1),
                2,
            ));
        let authorize_root = block.compute_body_root();
        assert_eq!(
            authorize_root,
            Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V7_DOMAIN,
                TRIDENT_BLOCK_BODY_V7_VERSION,
                legacy,
                vec![block.drc_deposit_preauths[0].preauth_tx_id()]
            ))
        );
        block.drc_deposit_preauths[0].action = crate::DrcDepositPreauthAction::Unauthorize;
        assert_ne!(block.compute_body_root(), authorize_root);
        let bytes = borsh::to_vec(&block).unwrap();
        assert_eq!(Block::try_from_slice(&bytes).unwrap(), block);
    }

    #[test]
    fn drc_payment_v4_activates_body_root_v8() {
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
        block.drc_payments.push(DrcPaymentTx::unsigned_v4(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            None,
            None,
            Hash([3; 32]),
            0,
            Some(7),
        ));
        let v7 = block.compute_body_root_v7();
        let payment_ids = vec![block.drc_payments[0].payment_id()];
        let expiring_root = block.compute_body_root();

        assert_eq!(
            expiring_root,
            Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V8_DOMAIN,
                TRIDENT_BLOCK_BODY_V8_VERSION,
                v7,
                payment_ids
            ))
        );

        let mut no_expiry = block;
        no_expiry.drc_payments[0].last_valid_blue_score = None;
        assert_ne!(no_expiry.compute_body_root(), v7);
        assert_ne!(no_expiry.compute_body_root(), expiring_root);
    }

    #[derive(BorshSerialize)]
    struct LegacyUtxoBlock {
        header: BlockHeader,
        transactions: Vec<Transaction>,
    }

    #[derive(BorshSerialize)]
    struct LegacyV4Block {
        header: BlockHeader,
        transactions: Vec<Transaction>,
        account_transfers: Vec<AccountTransfer>,
        stake_ops: Vec<SignedStakeTx>,
        ovl_executions: Vec<OvlExecutionTx>,
        drc_payments: Vec<DrcPaymentTx>,
    }

    #[test]
    fn empty_post_v4_lanes_preserve_v4_block_bytes() {
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash([7; 32])],
            timestamp_ms: 8,
            bits: 9,
            nonce: 10,
            tx_root: Hash([11; 32]),
        };
        let block = Block::utxo(header.clone(), Vec::new());
        let legacy = LegacyV4Block {
            header,
            transactions: Vec::new(),
            account_transfers: Vec::new(),
            stake_ops: Vec::new(),
            ovl_executions: Vec::new(),
            drc_payments: Vec::new(),
        };

        let bytes = borsh::to_vec(&block).unwrap();
        assert_eq!(bytes, borsh::to_vec(&legacy).unwrap());
        assert_eq!(Block::try_from_slice(&bytes).unwrap(), block);
    }

    #[derive(BorshSerialize)]
    struct LegacyV5Block {
        header: BlockHeader,
        transactions: Vec<Transaction>,
        account_transfers: Vec<AccountTransfer>,
        stake_ops: Vec<SignedStakeTx>,
        ovl_executions: Vec<OvlExecutionTx>,
        drc_payments: Vec<DrcPaymentTx>,
        data_commitments: Vec<DataCommitmentAuthorization>,
    }

    #[derive(BorshSerialize)]
    struct LegacyV6Block {
        header: BlockHeader,
        transactions: Vec<Transaction>,
        account_transfers: Vec<AccountTransfer>,
        stake_ops: Vec<SignedStakeTx>,
        ovl_executions: Vec<OvlExecutionTx>,
        drc_payments: Vec<DrcPaymentTx>,
        data_commitments: Vec<DataCommitmentAuthorization>,
        drc_account_policies: Vec<DrcAccountPolicyTx>,
    }

    #[test]
    fn legacy_utxo_block_bytes_decode_with_empty_appended_lanes() {
        let legacy = LegacyUtxoBlock {
            header: BlockHeader {
                version: 1,
                parents: vec![Hash([9; 32])],
                timestamp_ms: 11,
                bits: 2,
                nonce: 3,
                tx_root: Hash::ZERO,
            },
            transactions: Vec::new(),
        };
        let bytes = borsh::to_vec(&legacy).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.header, legacy.header);
        assert!(decoded.transactions.is_empty());
        assert!(decoded.account_transfers.is_empty());
        assert!(decoded.stake_ops.is_empty());
        assert!(decoded.ovl_executions.is_empty());
        assert!(decoded.drc_payments.is_empty());
        assert!(decoded.data_commitments.is_empty());
        assert!(decoded.drc_account_policies.is_empty());
        assert!(decoded.drc_deposit_preauths.is_empty());
    }

    #[test]
    fn legacy_v4_block_bytes_decode_with_empty_data_lane() {
        let payment = DrcPaymentTx::unsigned(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(3),
            Amount::ZERO,
            4,
            Hash([5; 32]),
            6,
        );
        let legacy = LegacyV4Block {
            header: BlockHeader {
                version: 1,
                parents: vec![Hash([7; 32])],
                timestamp_ms: 8,
                bits: 9,
                nonce: 10,
                tx_root: Hash([11; 32]),
            },
            transactions: Vec::new(),
            account_transfers: Vec::new(),
            stake_ops: Vec::new(),
            ovl_executions: Vec::new(),
            drc_payments: vec![payment.clone()],
        };

        let decoded = Block::try_from_slice(&borsh::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(decoded.header, legacy.header);
        assert_eq!(decoded.drc_payments, vec![payment]);
        assert!(decoded.data_commitments.is_empty());
        assert!(decoded.drc_account_policies.is_empty());
        assert!(decoded.drc_deposit_preauths.is_empty());
    }

    #[test]
    fn legacy_v5_block_bytes_decode_with_empty_policy_lane() {
        let legacy = LegacyV5Block {
            header: BlockHeader {
                version: 1,
                parents: vec![Hash([7; 32])],
                timestamp_ms: 8,
                bits: 9,
                nonce: 10,
                tx_root: Hash([11; 32]),
            },
            transactions: Vec::new(),
            account_transfers: Vec::new(),
            stake_ops: Vec::new(),
            ovl_executions: Vec::new(),
            drc_payments: Vec::new(),
            data_commitments: Vec::new(),
        };
        let decoded = Block::try_from_slice(&borsh::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(decoded.header, legacy.header);
        assert!(decoded.data_commitments.is_empty());
        assert!(decoded.drc_account_policies.is_empty());
        assert!(decoded.drc_deposit_preauths.is_empty());
    }

    #[test]
    fn legacy_v6_block_bytes_decode_with_empty_preauth_lane() {
        let policy =
            DrcAccountPolicyTx::set_require_destination_tag(Address([7; 20]), Amount::ZERO, 0);
        let legacy = LegacyV6Block {
            header: BlockHeader {
                version: 1,
                parents: vec![Hash([8; 32])],
                timestamp_ms: 9,
                bits: 10,
                nonce: 11,
                tx_root: Hash([12; 32]),
            },
            transactions: Vec::new(),
            account_transfers: Vec::new(),
            stake_ops: Vec::new(),
            ovl_executions: Vec::new(),
            drc_payments: Vec::new(),
            data_commitments: Vec::new(),
            drc_account_policies: vec![policy.clone()],
        };
        let decoded = Block::try_from_slice(&borsh::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(decoded.header, legacy.header);
        assert_eq!(decoded.drc_account_policies, vec![policy]);
        assert!(decoded.drc_deposit_preauths.is_empty());
    }

    #[test]
    fn drc_check_lane_activates_body_root_v14_and_commits_operation_ids() {
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
        let legacy = block.compute_body_root();
        block.drc_check_creates.push(DrcCheckCreateTx {
            version: DRC_CHECK_CREATE_TX_VERSION,
            owner: Address([1; 20]),
            destination: Address([2; 20]),
            amount: Amount::from_base_units(5),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash([3; 32]),
            expires_after_blue_score: Some(100),
            nonce: 0,
            account_sequence: None,
            public_key: vec![],
            signature: vec![],
            multisign: None,
        });
        let create_id = block.drc_check_creates[0].check_id();
        let v14_root = block.compute_body_root();
        assert_eq!(
            v14_root,
            Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V14_DOMAIN,
                TRIDENT_BLOCK_BODY_V14_VERSION,
                legacy,
                vec![create_id],
                Vec::<Hash>::new(),
                Vec::<Hash>::new(),
            ))
        );
        assert_ne!(v14_root, legacy);
        let bytes = borsh::to_vec(&block).unwrap();
        assert_eq!(Block::try_from_slice(&bytes).unwrap(), block);
    }

    #[test]
    fn drc_trust_line_lane_activates_body_root_v16_and_commits_operation_ids() {
        use crate::{
            Address, Amount, DrcIssuedTransferTx, DrcTrustLineSetTx, Hash, IssuedAmount,
            IssuedCurrencyCode, DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            DRC_TRUST_LINE_SET_TX_VERSION,
        };

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
        let legacy = block.compute_body_root();
        block.drc_trust_line_sets.push(DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TX_VERSION,
            holder: Address([1; 20]),
            issuer: Address([2; 20]),
            currency: IssuedCurrencyCode([0u8; 20]),
            limit: IssuedAmount::from_units(10),
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: vec![],
            signature: vec![],
            multisign: None,
        });
        let set_id = block.drc_trust_line_sets[0].trust_line_set_tx_id();
        let v16_set = block.compute_body_root();
        block.drc_issued_transfers.push(DrcIssuedTransferTx {
            version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            sender: Address([2; 20]),
            recipient: Address([1; 20]),
            issuer: Address([2; 20]),
            currency: IssuedCurrencyCode([0u8; 20]),
            amount: IssuedAmount::from_units(1),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            nonce: 0,
            account_sequence: None,
            public_key: vec![],
            signature: vec![],
            multisign: None,
        });
        let transfer_id = block.drc_issued_transfers[0].issued_transfer_tx_id();
        let v16_root = block.compute_body_root();
        assert_eq!(
            v16_set,
            Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V16_DOMAIN,
                TRIDENT_BLOCK_BODY_V16_VERSION,
                legacy,
                vec![set_id],
                Vec::<Hash>::new(),
            ))
        );
        assert_eq!(
            v16_root,
            Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V16_DOMAIN,
                TRIDENT_BLOCK_BODY_V16_VERSION,
                legacy,
                vec![set_id],
                vec![transfer_id],
            ))
        );
        assert_ne!(v16_root, legacy);
    }

    #[test]
    fn partial_appended_lane_length_is_not_treated_as_legacy_eof() {
        let legacy = LegacyUtxoBlock {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: Vec::new(),
        };
        let mut bytes = borsh::to_vec(&legacy).unwrap();
        bytes.push(1);
        assert!(Block::try_from_slice(&bytes).is_err());
    }

    #[test]
    fn json_rejects_unknown_drc_execution_lanes() {
        let block = Block::utxo(
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
        let mut value = serde_json::to_value(block).unwrap();
        value["drc_contract_calls"] = serde_json::json!([]);

        let error = serde_json::from_value::<Block>(value).unwrap_err();
        assert!(error
            .to_string()
            .contains("unknown field `drc_contract_calls`"));
    }

    #[test]
    fn tlt_covenant_lane_wraps_body_root_v19_and_stays_off_empty_wire() {
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
        let empty_bytes = borsh::to_vec(&block).unwrap();
        let frozen = frozen_v2_testnet_block();
        assert_eq!(
            hex::encode(borsh::to_vec(&frozen).unwrap()),
            FROZEN_V2_BLOCK_BORSH_HEX
        );
        let legacy = block.compute_body_root();
        block.tlt_covenants.push(TltCovenantTx {
            version: crate::TLT_COVENANT_TX_VERSION,
            inputs: vec![crate::TltCovenantInput {
                previous_outpoint: crate::OutPoint {
                    tx_id: Hash([9; 32]),
                    index: 0,
                },
                sequence: crate::TLT_SEQUENCE_FINAL,
                script_sig: vec![1],
            }],
            outputs: vec![crate::TltCovenantOutput {
                value: Amount::from_base_units(1),
                script_pubkey: vec![2],
            }],
            lock_time: 0,
            nonce: 1,
        });
        assert_eq!(
            block.compute_body_root(),
            Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_V19_DOMAIN,
                TRIDENT_BLOCK_BODY_V19_VERSION,
                legacy,
                vec![block.tlt_covenants[0].tx_id()]
            ))
        );
        let encoded = borsh::to_vec(&block).unwrap();
        assert_ne!(encoded, empty_bytes);
        let decoded: Block = borsh::from_slice(&encoded).unwrap();
        assert_eq!(decoded.tlt_covenants.len(), 1);
        assert!(decoded.drc_offer_creates.is_empty());
    }
}
