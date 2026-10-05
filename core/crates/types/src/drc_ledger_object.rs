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
    DrcIssuedClawbackTx, DrcIssuedTransferTx, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseTx,
    DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, DrcPaymentChannelLive, DrcPaymentTx,
    DrcRegularKeyTx, DrcSignerListTx, DrcTicketCreateTx, DrcTrustLineIssuerControlTx,
    DrcTrustLineLive, DrcTrustLineSetTx, Hash, IssuedAssetId, SignedStakeTx,
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
}

impl DrcLedgerObjectKind {
    pub const ALL: [Self; 10] = [
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
        }
    }

    pub const fn owner(self) -> Address {
        match self {
            Self::AccountPolicy { account } => account,
            Self::DepositPreauthorization { owner, .. }
            | Self::RegularKey { owner }
            | Self::SignerList { owner }
            | Self::TicketSet { owner } => owner,
            Self::Escrow { .. } | Self::Check { .. } | Self::PaymentChannel { .. } => {
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
        }
    }

    pub fn operation_id(&self) -> Hash {
        Hash::hash_borsh(&(
            DRC_ACCEPTED_OPERATION_ID_DOMAIN,
            self.kind(),
            self.historical_transaction_id(),
        ))
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
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
                "32d1de7e77054089230f3b9f94e92f0a87dc4fd1f448c6b4c929d82ffd2d2550",
            ),
            (
                DrcLedgerObjectKey::DepositPreauthorization {
                    owner,
                    authorized_source: Address([2; 20]),
                },
                "f67dc24ad877e92ad8e083f3d3a8af3b694b85bf79c69d663a9770d765979b10",
            ),
            (
                DrcLedgerObjectKey::RegularKey { owner },
                "4818e092e2155dea65e91a43fc0e4ac603922be1ea007ed3129208cb1c989ef6",
            ),
            (
                DrcLedgerObjectKey::SignerList { owner },
                "de1ad993508e15c88cf9561f5bd047790438305742ccc0910490fde736ed9b8f",
            ),
            (
                DrcLedgerObjectKey::TicketSet { owner },
                "d9d4ff60e24e3ca15430a8f31430ca7daa4aa045610c53aca7a12871d060fb8e",
            ),
            (
                DrcLedgerObjectKey::Escrow {
                    escrow_id: Hash([6; 32]),
                },
                "b9cd844658d7837f341f105d7b7e83a1d3640654f0c38bc5f000b6b59bf2e1ef",
            ),
            (
                DrcLedgerObjectKey::Check {
                    check_id: Hash([7; 32]),
                },
                "6f96bdd108e2a70c66ae6a87cf27f30310fcf043b9ab7c5af5b681094f1523cf",
            ),
            (
                DrcLedgerObjectKey::PaymentChannel {
                    channel_id: Hash([8; 32]),
                },
                "5822f988fd0928063c7d7b502b5b16bb5c122ee4324e933d472344d8076ca135",
            ),
            (
                DrcLedgerObjectKey::TrustLine {
                    holder: owner,
                    asset,
                },
                "e9e9cbad113589a7c8f094e8f2cb54bbcfc7c05bfa1d50796eeb99bb5adf5a63",
            ),
            (
                DrcLedgerObjectKey::IssuedAssetPolicy { asset },
                "e1baf11ab8ac7bbc55d0e6ae438315a34472cbe250b09bc29f01f33d2af87b58",
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
        for forbidden in ["offer", "evm", "contract", "bytecode", "hook"] {
            assert_eq!(DrcLedgerObjectKind::parse(forbidden), None);
        }
    }
}
