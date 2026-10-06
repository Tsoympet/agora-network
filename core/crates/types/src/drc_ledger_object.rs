//! Common identity and query types for protocol-native DRC ledger objects.
//!
//! These types describe Agora state. They do not reproduce XRPL wire encoding,
//! object hashes, account reserves, or programmable objects.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AccountTransfer, Address, DrcAccountPolicy, DrcAccountPolicyTx, DrcAccountRegularKey,
    DrcAccountSignerList, DrcAccountTickets, DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx,
    DrcCheckLive, DrcDepositPreauth, DrcDepositPreauthTx, DrcEscrowCancelTx, DrcEscrowCreateTx,
    DrcEscrowFinishTx, DrcEscrowLive, DrcIssuedAssetPolicyLive, DrcIssuedAssetPolicySetTx,
    DrcIssuedClawbackTx, DrcIssuedTransferTx, DrcMultisignAuth, DrcOfferCancelTx, DrcOfferCreateTx,
    DrcOfferLive, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx,
    DrcPaymentChannelFundTx, DrcPaymentChannelLive, DrcPaymentTx, DrcRegularKeyTx, DrcSignerListTx,
    DrcTicketCreateTx, DrcTrustLineIssuerControlTx, DrcTrustLineLive, DrcTrustLineSetTx, Hash,
    IssuedAssetId, SignedStakeTx,
};

pub const DRC_LEDGER_OBJECT_DESCRIPTOR_VERSION: u32 = 1;
pub const DRC_ACCEPTED_OPERATION_RECEIPT_VERSION: u32 = 1;
pub const DRC_LEDGER_OBJECT_ID_DOMAIN: &[u8] = b"agora-trident-drc-ledger-object-id-v1";
pub const DRC_ACCEPTED_OPERATION_ID_DOMAIN: &[u8] = b"agora-trident-drc-accepted-operation-id-v1";

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcLedgerObjectKind {
    AccountPolicy = 1,
    DepositPreauthorization = 2,
    RegularKey = 3,
    SignerList = 4,
    TicketSet = 5,
    Escrow = 6,
    Check = 7,
    PaymentChannel = 8,
    TrustLine = 9,
    IssuedAssetPolicy = 10,
    Offer = 11,
}

impl DrcLedgerObjectKind {
    pub const ALL: [Self; 11] = [
        Self::AccountPolicy,
        Self::DepositPreauthorization,
        Self::RegularKey,
        Self::SignerList,
        Self::TicketSet,
        Self::Escrow,
        Self::Check,
        Self::PaymentChannel,
        Self::TrustLine,
        Self::IssuedAssetPolicy,
        Self::Offer,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccountPolicy => "account_policy",
            Self::DepositPreauthorization => "deposit_preauthorization",
            Self::RegularKey => "regular_key",
            Self::SignerList => "signer_list",
            Self::TicketSet => "ticket_set",
            Self::Escrow => "escrow",
            Self::Check => "check",
            Self::PaymentChannel => "payment_channel",
            Self::TrustLine => "trust_line",
            Self::IssuedAssetPolicy => "issued_asset_policy",
            Self::Offer => "offer",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }

    pub const fn wire_byte(self) -> u8 {
        self as u8
    }
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DrcLedgerObjectKey {
    AccountPolicy {
        account: Address,
    },
    DepositPreauthorization {
        owner: Address,
        authorized_source: Address,
    },
    RegularKey {
        owner: Address,
    },
    SignerList {
        owner: Address,
    },
    TicketSet {
        owner: Address,
    },
    Escrow {
        escrow_id: Hash,
    },
    Check {
        check_id: Hash,
    },
    PaymentChannel {
        channel_id: Hash,
    },
    TrustLine {
        holder: Address,
        asset: IssuedAssetId,
    },
    IssuedAssetPolicy {
        asset: IssuedAssetId,
    },
    Offer {
        offer_id: Hash,
    },
}

impl DrcLedgerObjectKey {
    pub const fn kind(self) -> DrcLedgerObjectKind {
        match self {
            Self::AccountPolicy { .. } => DrcLedgerObjectKind::AccountPolicy,
            Self::DepositPreauthorization { .. } => DrcLedgerObjectKind::DepositPreauthorization,
            Self::RegularKey { .. } => DrcLedgerObjectKind::RegularKey,
            Self::SignerList { .. } => DrcLedgerObjectKind::SignerList,
            Self::TicketSet { .. } => DrcLedgerObjectKind::TicketSet,
            Self::Escrow { .. } => DrcLedgerObjectKind::Escrow,
            Self::Check { .. } => DrcLedgerObjectKind::Check,
            Self::PaymentChannel { .. } => DrcLedgerObjectKind::PaymentChannel,
            Self::TrustLine { .. } => DrcLedgerObjectKind::TrustLine,
            Self::IssuedAssetPolicy { .. } => DrcLedgerObjectKind::IssuedAssetPolicy,
            Self::Offer { .. } => DrcLedgerObjectKind::Offer,
        }
    }

    pub const fn owner(self) -> Address {
        match self {
            Self::AccountPolicy { account } => account,
            Self::DepositPreauthorization { owner, .. }
            | Self::RegularKey { owner }
            | Self::SignerList { owner }
            | Self::TicketSet { owner } => owner,
            Self::Escrow { .. }
            | Self::Check { .. }
            | Self::PaymentChannel { .. }
            | Self::Offer { .. } => {
                // These keys do not duplicate the owner. The descriptor validates
                // the owner against the canonical live value.
                Address::ZERO
            }
            Self::TrustLine { holder, .. } => holder,
            Self::IssuedAssetPolicy { asset } => asset.issuer,
        }
    }

    pub fn object_id(&self) -> Hash {
        Hash::hash_borsh(&(DRC_LEDGER_OBJECT_ID_DOMAIN, self))
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DrcLedgerObject {
    AccountPolicy {
        account: Address,
        policy: DrcAccountPolicy,
    },
    DepositPreauthorization(DrcDepositPreauth),
    RegularKey(DrcAccountRegularKey),
    SignerList(DrcAccountSignerList),
    TicketSet(DrcAccountTickets),
    Escrow(DrcEscrowLive),
    Check(DrcCheckLive),
    PaymentChannel(DrcPaymentChannelLive),
    TrustLine(DrcTrustLineLive),
    IssuedAssetPolicy(DrcIssuedAssetPolicyLive),
    Offer(DrcOfferLive),
}

impl DrcLedgerObject {
    pub const fn kind(&self) -> DrcLedgerObjectKind {
        match self {
            Self::AccountPolicy { .. } => DrcLedgerObjectKind::AccountPolicy,
            Self::DepositPreauthorization(_) => DrcLedgerObjectKind::DepositPreauthorization,
            Self::RegularKey(_) => DrcLedgerObjectKind::RegularKey,
            Self::SignerList(_) => DrcLedgerObjectKind::SignerList,
            Self::TicketSet(_) => DrcLedgerObjectKind::TicketSet,
            Self::Escrow(_) => DrcLedgerObjectKind::Escrow,
            Self::Check(_) => DrcLedgerObjectKind::Check,
            Self::PaymentChannel(_) => DrcLedgerObjectKind::PaymentChannel,
            Self::TrustLine(_) => DrcLedgerObjectKind::TrustLine,
            Self::IssuedAssetPolicy(_) => DrcLedgerObjectKind::IssuedAssetPolicy,
            Self::Offer(_) => DrcLedgerObjectKind::Offer,
        }
    }

    pub const fn owner(&self) -> Address {
        match self {
            Self::AccountPolicy { account, .. } => *account,
            Self::DepositPreauthorization(value) => value.owner,
            Self::RegularKey(value) => value.owner,
            Self::SignerList(value) => value.owner,
            Self::TicketSet(value) => value.owner,
            Self::Escrow(value) => value.owner,
            Self::Check(value) => value.owner,
            Self::PaymentChannel(value) => value.owner,
            Self::TrustLine(value) => value.holder,
            Self::IssuedAssetPolicy(value) => value.asset.issuer,
            Self::Offer(value) => value.owner,
        }
    }

    pub const fn key(&self) -> DrcLedgerObjectKey {
        match self {
            Self::AccountPolicy { account, .. } => {
                DrcLedgerObjectKey::AccountPolicy { account: *account }
            }
            Self::DepositPreauthorization(value) => DrcLedgerObjectKey::DepositPreauthorization {
                owner: value.owner,
                authorized_source: value.authorized_source,
            },
            Self::RegularKey(value) => DrcLedgerObjectKey::RegularKey { owner: value.owner },
            Self::SignerList(value) => DrcLedgerObjectKey::SignerList { owner: value.owner },
            Self::TicketSet(value) => DrcLedgerObjectKey::TicketSet { owner: value.owner },
            Self::Escrow(value) => DrcLedgerObjectKey::Escrow {
                escrow_id: value.escrow_id,
            },
            Self::Check(value) => DrcLedgerObjectKey::Check {
                check_id: value.check_id,
            },
            Self::PaymentChannel(value) => DrcLedgerObjectKey::PaymentChannel {
                channel_id: value.channel_id,
            },
            Self::TrustLine(value) => DrcLedgerObjectKey::TrustLine {
                holder: value.holder,
                asset: value.asset,
            },
            Self::IssuedAssetPolicy(value) => {
                DrcLedgerObjectKey::IssuedAssetPolicy { asset: value.asset }
            }
            Self::Offer(value) => DrcLedgerObjectKey::Offer {
                offer_id: value.offer_id,
            },
        }
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcLedgerObjectDescriptor {
    pub version: u32,
    pub object_id: Hash,
    pub kind: DrcLedgerObjectKind,
    pub owner: Address,
    pub key: DrcLedgerObjectKey,
    pub object: DrcLedgerObject,
}

impl DrcLedgerObjectDescriptor {
    pub fn new(object: DrcLedgerObject) -> Self {
        let key = object.key();
        Self {
            version: DRC_LEDGER_OBJECT_DESCRIPTOR_VERSION,
            object_id: key.object_id(),
            kind: object.kind(),
            owner: object.owner(),
            key,
            object,
        }
    }

    pub fn is_consistent(&self) -> bool {
        self.version == DRC_LEDGER_OBJECT_DESCRIPTOR_VERSION
            && self.object_id == self.key.object_id()
            && self.kind == self.key.kind()
            && self.kind == self.object.kind()
            && self.owner == self.object.owner()
            && self.key == self.object.key()
    }
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcOperationKind {
    AccountTransfer = 1,
    Stake = 2,
    Payment = 3,
    AccountPolicy = 4,
    DepositPreauthorization = 5,
    RegularKey = 6,
    SignerList = 7,
    TicketCreate = 8,
    EscrowCreate = 9,
    EscrowFinish = 10,
    EscrowCancel = 11,
    CheckCreate = 12,
    CheckCash = 13,
    CheckCancel = 14,
    PaymentChannelCreate = 15,
    PaymentChannelFund = 16,
    PaymentChannelClaim = 17,
    PaymentChannelClose = 18,
    TrustLineSet = 19,
    IssuedTransfer = 20,
    IssuedAssetPolicySet = 21,
    TrustLineIssuerControl = 22,
    IssuedClawback = 23,
    OfferCreate = 24,
    OfferCancel = 25,
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DrcOperation {
    AccountTransfer(AccountTransfer),
    Stake(SignedStakeTx),
    Payment(DrcPaymentTx),
    AccountPolicy(DrcAccountPolicyTx),
    DepositPreauthorization(DrcDepositPreauthTx),
    RegularKey(DrcRegularKeyTx),
    SignerList(DrcSignerListTx),
    TicketCreate(DrcTicketCreateTx),
    EscrowCreate(DrcEscrowCreateTx),
    EscrowFinish(DrcEscrowFinishTx),
    EscrowCancel(DrcEscrowCancelTx),
    CheckCreate(DrcCheckCreateTx),
    CheckCash(DrcCheckCashTx),
    CheckCancel(DrcCheckCancelTx),
    PaymentChannelCreate(DrcPaymentChannelCreateTx),
    PaymentChannelFund(DrcPaymentChannelFundTx),
    PaymentChannelClaim(DrcPaymentChannelClaimTx),
    PaymentChannelClose(DrcPaymentChannelCloseTx),
    TrustLineSet(DrcTrustLineSetTx),
    IssuedTransfer(DrcIssuedTransferTx),
    IssuedAssetPolicySet(DrcIssuedAssetPolicySetTx),
    TrustLineIssuerControl(DrcTrustLineIssuerControlTx),
    IssuedClawback(DrcIssuedClawbackTx),
    OfferCreate(DrcOfferCreateTx),
    OfferCancel(DrcOfferCancelTx),
}

impl DrcOperation {
    pub const fn kind(&self) -> DrcOperationKind {
        match self {
            Self::AccountTransfer(_) => DrcOperationKind::AccountTransfer,
            Self::Stake(_) => DrcOperationKind::Stake,
            Self::Payment(_) => DrcOperationKind::Payment,
            Self::AccountPolicy(_) => DrcOperationKind::AccountPolicy,
            Self::DepositPreauthorization(_) => DrcOperationKind::DepositPreauthorization,
            Self::RegularKey(_) => DrcOperationKind::RegularKey,
            Self::SignerList(_) => DrcOperationKind::SignerList,
            Self::TicketCreate(_) => DrcOperationKind::TicketCreate,
            Self::EscrowCreate(_) => DrcOperationKind::EscrowCreate,
            Self::EscrowFinish(_) => DrcOperationKind::EscrowFinish,
            Self::EscrowCancel(_) => DrcOperationKind::EscrowCancel,
            Self::CheckCreate(_) => DrcOperationKind::CheckCreate,
            Self::CheckCash(_) => DrcOperationKind::CheckCash,
            Self::CheckCancel(_) => DrcOperationKind::CheckCancel,
            Self::PaymentChannelCreate(_) => DrcOperationKind::PaymentChannelCreate,
            Self::PaymentChannelFund(_) => DrcOperationKind::PaymentChannelFund,
            Self::PaymentChannelClaim(_) => DrcOperationKind::PaymentChannelClaim,
            Self::PaymentChannelClose(_) => DrcOperationKind::PaymentChannelClose,
            Self::TrustLineSet(_) => DrcOperationKind::TrustLineSet,
            Self::IssuedTransfer(_) => DrcOperationKind::IssuedTransfer,
            Self::IssuedAssetPolicySet(_) => DrcOperationKind::IssuedAssetPolicySet,
            Self::TrustLineIssuerControl(_) => DrcOperationKind::TrustLineIssuerControl,
            Self::IssuedClawback(_) => DrcOperationKind::IssuedClawback,
            Self::OfferCreate(_) => DrcOperationKind::OfferCreate,
            Self::OfferCancel(_) => DrcOperationKind::OfferCancel,
        }
    }

    pub const fn owner(&self) -> Address {
        match self {
            Self::AccountTransfer(value) => value.from,
            Self::Stake(value) => value.actor,
            Self::Payment(value) => value.from,
            Self::AccountPolicy(value) => value.account,
            Self::DepositPreauthorization(value) => value.owner,
            Self::RegularKey(value) => value.owner,
            Self::SignerList(value) => value.owner,
            Self::TicketCreate(value) => value.owner,
            Self::EscrowCreate(value) => value.owner,
            Self::EscrowFinish(value) => value.submitter,
            Self::EscrowCancel(value) => value.submitter,
            Self::CheckCreate(value) => value.owner,
            Self::CheckCash(value) => value.submitter,
            Self::CheckCancel(value) => value.submitter,
            Self::PaymentChannelCreate(value) => value.owner,
            Self::PaymentChannelFund(value) => value.submitter,
            Self::PaymentChannelClaim(value) => value.submitter,
            Self::PaymentChannelClose(value) => value.submitter,
            Self::TrustLineSet(value) => value.holder,
            Self::IssuedTransfer(value) => value.sender,
            Self::IssuedAssetPolicySet(value) => value.issuer,
            Self::TrustLineIssuerControl(value) => value.issuer,
            Self::IssuedClawback(value) => value.issuer,
            Self::OfferCreate(value) => value.owner,
            Self::OfferCancel(value) => value.submitter,
        }
    }

    pub fn historical_transaction_id(&self) -> Hash {
        match self {
            Self::AccountTransfer(value) => value.transfer_id(),
            Self::Stake(value) => value.stake_tx_id(),
            Self::Payment(value) => value.payment_id(),
            Self::AccountPolicy(value) => value.policy_tx_id(),
            Self::DepositPreauthorization(value) => value.preauth_tx_id(),
            Self::RegularKey(value) => value.regular_key_tx_id(),
            Self::SignerList(value) => value.signer_list_tx_id(),
            Self::TicketCreate(value) => value.ticket_create_tx_id(),
            Self::EscrowCreate(value) => value.escrow_id(),
            Self::EscrowFinish(value) => value.finish_tx_id(),
            Self::EscrowCancel(value) => value.cancel_tx_id(),
            Self::CheckCreate(value) => value.check_id(),
            Self::CheckCash(value) => value.cash_tx_id(),
            Self::CheckCancel(value) => value.cancel_tx_id(),
            Self::PaymentChannelCreate(value) => value.channel_id(),
            Self::PaymentChannelFund(value) => value.fund_tx_id(),
            Self::PaymentChannelClaim(value) => value.claim_tx_id(),
            Self::PaymentChannelClose(value) => value.close_tx_id(),
            Self::TrustLineSet(value) => value.trust_line_set_tx_id(),
            Self::IssuedTransfer(value) => value.issued_transfer_tx_id(),
            Self::IssuedAssetPolicySet(value) => value.policy_set_tx_id(),
            Self::TrustLineIssuerControl(value) => value.issuer_control_tx_id(),
            Self::IssuedClawback(value) => value.clawback_tx_id(),
            Self::OfferCreate(value) => value.offer_id(),
            Self::OfferCancel(value) => value.cancel_tx_id(),
        }
    }

    pub fn operation_id(&self) -> Hash {
        Hash::hash_borsh(&(
            DRC_ACCEPTED_OPERATION_ID_DOMAIN,
            self.kind(),
            self.historical_transaction_id(),
        ))
    }

    /// Detached authorization omitted from consensus transaction Borsh.
    ///
    /// Receipts persist this separately so lookup can return the signed envelope
    /// without changing frozen lane or block bytes.
    pub fn multisign(&self) -> Option<&DrcMultisignAuth> {
        match self {
            Self::AccountTransfer(value) => value.multisign.as_ref(),
            Self::Stake(value) => value.multisign.as_ref(),
            Self::Payment(value) => value.multisign.as_ref(),
            Self::AccountPolicy(value) => value.multisign.as_ref(),
            Self::DepositPreauthorization(value) => value.multisign.as_ref(),
            Self::RegularKey(value) => value.multisign.as_ref(),
            Self::SignerList(value) => value.multisign.as_ref(),
            Self::TicketCreate(value) => value.multisign.as_ref(),
            Self::EscrowCreate(value) => value.multisign.as_ref(),
            Self::EscrowFinish(value) => value.multisign.as_ref(),
            Self::EscrowCancel(value) => value.multisign.as_ref(),
            Self::CheckCreate(value) => value.multisign.as_ref(),
            Self::CheckCash(value) => value.multisign.as_ref(),
            Self::CheckCancel(value) => value.multisign.as_ref(),
            Self::PaymentChannelCreate(value) => value.multisign.as_ref(),
            Self::PaymentChannelFund(value) => value.multisign.as_ref(),
            Self::PaymentChannelClaim(value) => value.multisign.as_ref(),
            Self::PaymentChannelClose(value) => value.multisign.as_ref(),
            Self::TrustLineSet(value) => value.multisign.as_ref(),
            Self::IssuedTransfer(value) => value.multisign.as_ref(),
            Self::IssuedAssetPolicySet(value) => value.multisign.as_ref(),
            Self::TrustLineIssuerControl(value) => value.multisign.as_ref(),
            Self::IssuedClawback(value) => value.multisign.as_ref(),
            Self::OfferCreate(value) => value.multisign.as_ref(),
            Self::OfferCancel(value) => value.multisign.as_ref(),
        }
    }

    pub fn set_multisign(&mut self, auth: Option<DrcMultisignAuth>) {
        match self {
            Self::AccountTransfer(value) => value.multisign = auth,
            Self::Stake(value) => value.multisign = auth,
            Self::Payment(value) => value.multisign = auth,
            Self::AccountPolicy(value) => value.multisign = auth,
            Self::DepositPreauthorization(value) => value.multisign = auth,
            Self::RegularKey(value) => value.multisign = auth,
            Self::SignerList(value) => value.multisign = auth,
            Self::TicketCreate(value) => value.multisign = auth,
            Self::EscrowCreate(value) => value.multisign = auth,
            Self::EscrowFinish(value) => value.multisign = auth,
            Self::EscrowCancel(value) => value.multisign = auth,
            Self::CheckCreate(value) => value.multisign = auth,
            Self::CheckCash(value) => value.multisign = auth,
            Self::CheckCancel(value) => value.multisign = auth,
            Self::PaymentChannelCreate(value) => value.multisign = auth,
            Self::PaymentChannelFund(value) => value.multisign = auth,
            Self::PaymentChannelClaim(value) => value.multisign = auth,
            Self::PaymentChannelClose(value) => value.multisign = auth,
            Self::TrustLineSet(value) => value.multisign = auth,
            Self::IssuedTransfer(value) => value.multisign = auth,
            Self::IssuedAssetPolicySet(value) => value.multisign = auth,
            Self::TrustLineIssuerControl(value) => value.multisign = auth,
            Self::IssuedClawback(value) => value.multisign = auth,
            Self::OfferCreate(value) => value.multisign = auth,
            Self::OfferCancel(value) => value.multisign = auth,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrcAcceptedOperationReceipt {
    pub version: u32,
    pub operation_id: Hash,
    pub historical_transaction_id: Hash,
    pub owner: Address,
    pub kind: DrcOperationKind,
    pub canonical_block_id: Hash,
    pub application_blue_score: Option<u64>,
    pub operation: DrcOperation,
    pub affected_object_ids: Vec<Hash>,
}

impl DrcAcceptedOperationReceipt {
    pub fn new(
        canonical_block_id: Hash,
        application_blue_score: Option<u64>,
        operation: DrcOperation,
        mut affected_object_ids: Vec<Hash>,
    ) -> Self {
        affected_object_ids.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        affected_object_ids.dedup();
        Self {
            version: DRC_ACCEPTED_OPERATION_RECEIPT_VERSION,
            operation_id: operation.operation_id(),
            historical_transaction_id: operation.historical_transaction_id(),
            owner: operation.owner(),
            kind: operation.kind(),
            canonical_block_id,
            application_blue_score,
            operation,
            affected_object_ids,
        }
    }

    pub fn is_consistent(&self) -> bool {
        let mut canonical_ids = self.affected_object_ids.clone();
        canonical_ids.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        canonical_ids.dedup();
        self.version == DRC_ACCEPTED_OPERATION_RECEIPT_VERSION
            && self.operation_id == self.operation.operation_id()
            && self.historical_transaction_id == self.operation.historical_transaction_id()
            && self.owner == self.operation.owner()
            && self.kind == self.operation.kind()
            && canonical_ids == self.affected_object_ids
    }
}

impl BorshSerialize for DrcAcceptedOperationReceipt {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.operation_id, writer)?;
        BorshSerialize::serialize(&self.historical_transaction_id, writer)?;
        BorshSerialize::serialize(&self.owner, writer)?;
        BorshSerialize::serialize(&self.kind, writer)?;
        BorshSerialize::serialize(&self.canonical_block_id, writer)?;
        BorshSerialize::serialize(&self.application_blue_score, writer)?;
        // Inner transaction Borsh intentionally drops the multisign trailer.
        BorshSerialize::serialize(&self.operation, writer)?;
        BorshSerialize::serialize(&self.affected_object_ids, writer)?;
        BorshSerialize::serialize(&self.operation.multisign(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for DrcAcceptedOperationReceipt {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        let version = u32::deserialize_reader(reader)?;
        let operation_id = Hash::deserialize_reader(reader)?;
        let historical_transaction_id = Hash::deserialize_reader(reader)?;
        let owner = Address::deserialize_reader(reader)?;
        let kind = DrcOperationKind::deserialize_reader(reader)?;
        let canonical_block_id = Hash::deserialize_reader(reader)?;
        let application_blue_score = Option::<u64>::deserialize_reader(reader)?;
        let mut operation = DrcOperation::deserialize_reader(reader)?;
        let affected_object_ids = Vec::<Hash>::deserialize_reader(reader)?;
        let authorization = Option::<DrcMultisignAuth>::deserialize_reader(reader)?;
        operation.set_multisign(authorization);
        Ok(Self {
            version,
            operation_id,
            historical_transaction_id,
            owner,
            kind,
            canonical_block_id,
            application_blue_score,
            operation,
            affected_object_ids,
        })
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrcLedgerObjectPage {
    pub owner: Address,
    pub kind: Option<DrcLedgerObjectKind>,
    pub objects: Vec<DrcLedgerObjectDescriptor>,
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_ids_are_domain_separated_by_typed_key() {
        let owner = Address([1; 20]);
        let policy = DrcLedgerObjectKey::AccountPolicy { account: owner };
        let tickets = DrcLedgerObjectKey::TicketSet { owner };
        assert_eq!(policy.object_id(), policy.object_id());
        assert_ne!(policy.object_id(), tickets.object_id());
        assert_ne!(
            policy.object_id(),
            Hash::hash_borsh(&(b"unrelated-domain", policy))
        );
    }

    #[test]
    fn object_id_vectors_cover_every_closed_kind() {
        let owner = Address([1; 20]);
        let issuer = Address([3; 20]);
        let mut currency = [0u8; 20];
        currency[..3].copy_from_slice(b"USD");
        let asset = IssuedAssetId {
            issuer,
            currency: crate::IssuedCurrencyCode(currency),
        };
        let vectors = [
            (
                DrcLedgerObjectKey::AccountPolicy { account: owner },
                "81e441ff64f5eea9e394a54404555375a774501f198001e6c1ca58bf23f09fc5",
            ),
            (
                DrcLedgerObjectKey::DepositPreauthorization {
                    owner,
                    authorized_source: Address([2; 20]),
                },
                "164721a8924b15e03c6eb2a28e7468da34277832e6d9151fb803c2a4c7014526",
            ),
            (
                DrcLedgerObjectKey::RegularKey { owner },
                "238a6035e453e36d3d778b327221835960ef677348be3ab0f626c763938e7f15",
            ),
            (
                DrcLedgerObjectKey::SignerList { owner },
                "4818e092e2155dea65e91a43fc0e4ac603922be1ea007ed3129208cb1c989ef6",
            ),
            (
                DrcLedgerObjectKey::TicketSet { owner },
                "de1ad993508e15c88cf9561f5bd047790438305742ccc0910490fde736ed9b8f",
            ),
            (
                DrcLedgerObjectKey::Escrow {
                    escrow_id: Hash([6; 32]),
                },
                "347aa99270e32792771df4d8f335d5243abf6a1a601166816986c1ea35f1a5f8",
            ),
            (
                DrcLedgerObjectKey::Check {
                    check_id: Hash([7; 32]),
                },
                "90160e190543e32e2f431130c3e0705dbdd693e97c26344f1242466e0b3cc8d7",
            ),
            (
                DrcLedgerObjectKey::PaymentChannel {
                    channel_id: Hash([8; 32]),
                },
                "90648b8aa3c79f6a329970d27b62ff02094518acb4d34d10b9b3176eb58a81a6",
            ),
            (
                DrcLedgerObjectKey::TrustLine {
                    holder: owner,
                    asset,
                },
                "5e3402a428b2014b15787589947f45190dce1442051fbb7d95e1f66c9ed41abf",
            ),
            (
                DrcLedgerObjectKey::IssuedAssetPolicy { asset },
                "ce53f130a7d9249100e900fd42a36aae388e43a23c3cbf43e2d8cae29a5e3e87",
            ),
            (
                DrcLedgerObjectKey::Offer {
                    offer_id: Hash([9; 32]),
                },
                "0630cbb335ed44d2ce9ee71c83a2b08e04e0faa15c457c4a39aed99f3530dad6",
            ),
        ];

        let mut kinds = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        for (key, expected) in vectors {
            assert_eq!(key.object_id(), Hash::from_hex(expected).unwrap());
            assert!(kinds.insert(key.kind()));
            assert!(ids.insert(key.object_id()));
        }
        assert_eq!(kinds, DrcLedgerObjectKind::ALL.into_iter().collect());
        assert_eq!(ids.len(), DrcLedgerObjectKind::ALL.len());
    }

    #[test]
    fn object_kind_parser_is_closed() {
        assert_eq!(
            DrcLedgerObjectKind::parse("payment_channel"),
            Some(DrcLedgerObjectKind::PaymentChannel)
        );
        assert_eq!(
            DrcLedgerObjectKind::parse("offer"),
            Some(DrcLedgerObjectKind::Offer)
        );
        for forbidden in ["evm", "contract", "bytecode", "hook"] {
            assert_eq!(DrcLedgerObjectKind::parse(forbidden), None);
        }
    }

    #[test]
    fn receipt_borsh_preserves_detached_multisign() {
        let auth = crate::DrcMultisignAuth {
            version: 1,
            signing_for: Address([9; 20]),
            signatures: vec![crate::DrcMultisignEntry {
                signer: Address([4; 20]),
                public_key: vec![2; 33],
                signature: vec![3; 64],
            }],
        };
        let operation = DrcOperation::EscrowCreate(crate::DrcEscrowCreateTx {
            version: 1,
            owner: Address([9; 20]),
            recipient: Address([8; 20]),
            amount: crate::Amount::from_base_units(10),
            fee: crate::Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(50),
            nonce: 1,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: Some(auth),
        });
        let receipt = DrcAcceptedOperationReceipt::new(Hash([1; 32]), Some(4), operation, vec![]);
        let encoded = borsh::to_vec(&receipt).unwrap();
        let decoded = DrcAcceptedOperationReceipt::try_from_slice(&encoded).unwrap();
        assert_eq!(decoded, receipt);
        assert!(decoded.operation.multisign().is_some());
        assert!(decoded.is_consistent());
    }
}
