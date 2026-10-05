//! Consensus block lane for detached DRC multisign authorization.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{
    AccountTransfer, DrcAccountPolicyTx, DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx,
    DrcDepositPreauthTx, DrcEscrowCancelTx, DrcEscrowCreateTx, DrcEscrowFinishTx, DrcMultisignAuth,
    DrcMultisignError, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseTx,
    DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, DrcPaymentTx, DrcRegularKeyTx,
    DrcSignerListTx, DrcTicketCreateTx, Hash, NativeAssetId, SignedStakeTx,
};

pub const DRC_MULTISIGN_ATTACHMENT_KEY_VERSION: u32 = 1;
pub const DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION: u32 = 1;
pub const DRC_MULTISIGN_SIGNING_COMMITMENT_DOMAIN: &[u8] =
    b"agora-trident-drc-multisign-signing-commitment-v1";

/// Type tag disambiguates attachment keys across DRC operation lanes.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    PartialOrd,
    Ord,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcMultisignOperationKind {
    DrcAccountTransfer = 1,
    DrcStake = 2,
    DrcRegularKey = 3,
    DrcSignerList = 4,
    DrcAccountPolicy = 5,
    DrcDepositPreauth = 6,
    DrcPayment = 7,
    DrcTicketCreate = 8,
    DrcEscrowCreate = 9,
    DrcEscrowFinish = 10,
    DrcEscrowCancel = 11,
    DrcCheckCreate = 12,
    DrcCheckCash = 13,
    DrcCheckCancel = 14,
    DrcPaymentChannelCreate = 15,
    DrcPaymentChannelFund = 16,
    DrcPaymentChannelClaim = 17,
    DrcPaymentChannelClose = 18,
}

impl DrcMultisignOperationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DrcAccountTransfer => "drc_account_transfer",
            Self::DrcStake => "drc_stake",
            Self::DrcRegularKey => "drc_regular_key",
            Self::DrcSignerList => "drc_signer_list",
            Self::DrcAccountPolicy => "drc_account_policy",
            Self::DrcDepositPreauth => "drc_deposit_preauth",
            Self::DrcPayment => "drc_payment",
            Self::DrcTicketCreate => "drc_ticket_create",
            Self::DrcEscrowCreate => "drc_escrow_create",
            Self::DrcEscrowFinish => "drc_escrow_finish",
            Self::DrcEscrowCancel => "drc_escrow_cancel",
            Self::DrcCheckCreate => "drc_check_create",
            Self::DrcCheckCash => "drc_check_cash",
            Self::DrcCheckCancel => "drc_check_cancel",
            Self::DrcPaymentChannelCreate => "drc_payment_channel_create",
            Self::DrcPaymentChannelFund => "drc_payment_channel_fund",
            Self::DrcPaymentChannelClaim => "drc_payment_channel_claim",
            Self::DrcPaymentChannelClose => "drc_payment_channel_close",
        }
    }
}

/// Non-circular attachment lookup key: operation kind + hash(network-bound signing bytes).
///
/// The signing commitment excludes inline `public_key`, `signature`, and multisign material
/// while binding every field in each operation's `signing_bytes_bound(chain_id, genesis)`.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    PartialOrd,
    Ord,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
pub struct DrcMultisignAttachmentKey {
    pub version: u32,
    pub kind: DrcMultisignOperationKind,
    pub signing_commitment: Hash,
}

impl DrcMultisignAttachmentKey {
    pub fn validate(&self) -> Result<(), DrcMultisignAttachmentError> {
        if self.version != DRC_MULTISIGN_ATTACHMENT_KEY_VERSION {
            return Err(DrcMultisignAttachmentError::UnsupportedKeyVersion(
                self.version,
            ));
        }
        Ok(())
    }
}

/// One consensus attachment: canonical key + versioned auth bundle.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DrcMultisignBlockAttachment {
    pub version: u32,
    pub key: DrcMultisignAttachmentKey,
    pub auth: DrcMultisignAuth,
}

impl DrcMultisignBlockAttachment {
    pub fn validate(&self) -> Result<(), DrcMultisignAttachmentError> {
        if self.version != DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION {
            return Err(DrcMultisignAttachmentError::UnsupportedAttachmentVersion(
                self.version,
            ));
        }
        self.key.validate()?;
        self.auth
            .validate_structure()
            .map_err(|_| DrcMultisignAttachmentError::MalformedAuth)?;
        Ok(())
    }

    /// Body-root leaf ID (full attachment bytes, including auth signatures).
    pub fn body_commitment_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

pub fn drc_multisign_signing_commitment(
    kind: DrcMultisignOperationKind,
    signing_bytes: &[u8],
) -> Hash {
    Hash::hash_borsh(&(
        DRC_MULTISIGN_SIGNING_COMMITMENT_DOMAIN,
        DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
        kind,
        signing_bytes,
    ))
}

pub fn drc_multisign_attachment_key(
    kind: DrcMultisignOperationKind,
    signing_bytes: &[u8],
) -> DrcMultisignAttachmentKey {
    DrcMultisignAttachmentKey {
        version: DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
        kind,
        signing_commitment: drc_multisign_signing_commitment(kind, signing_bytes),
    }
}

pub fn attachment_key_for_account_transfer(
    tx: &AccountTransfer,
    chain_id: &str,
    genesis: &Hash,
) -> Result<DrcMultisignAttachmentKey, DrcMultisignAttachmentError> {
    if tx.asset != NativeAssetId::DRC {
        return Err(DrcMultisignAttachmentError::WrongOperationKind);
    }
    Ok(drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcAccountTransfer,
        &tx.signing_bytes_bound(chain_id, genesis),
    ))
}

pub fn attachment_key_for_stake(
    tx: &SignedStakeTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<DrcMultisignAttachmentKey, DrcMultisignAttachmentError> {
    if tx.asset != NativeAssetId::DRC {
        return Err(DrcMultisignAttachmentError::WrongOperationKind);
    }
    Ok(drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcStake,
        &tx.signing_bytes_bound(chain_id, genesis),
    ))
}

pub fn attachment_key_for_regular_key(
    tx: &DrcRegularKeyTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcRegularKey,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_signer_list(
    tx: &DrcSignerListTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcSignerList,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_policy(
    tx: &DrcAccountPolicyTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcAccountPolicy,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_deposit_preauth(
    tx: &DrcDepositPreauthTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcDepositPreauth,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_payment(
    tx: &DrcPaymentTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcPayment,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_ticket_create(
    tx: &DrcTicketCreateTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcTicketCreate,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_escrow_create(
    tx: &DrcEscrowCreateTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcEscrowCreate,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_escrow_finish(
    tx: &DrcEscrowFinishTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcEscrowFinish,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_escrow_cancel(
    tx: &DrcEscrowCancelTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcEscrowCancel,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_check_create(
    tx: &DrcCheckCreateTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcCheckCreate,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_check_cash(
    tx: &DrcCheckCashTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcCheckCash,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_check_cancel(
    tx: &DrcCheckCancelTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcCheckCancel,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_payment_channel_create(
    tx: &DrcPaymentChannelCreateTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcPaymentChannelCreate,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_payment_channel_fund(
    tx: &DrcPaymentChannelFundTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcPaymentChannelFund,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_payment_channel_claim(
    tx: &DrcPaymentChannelClaimTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcPaymentChannelClaim,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn attachment_key_for_payment_channel_close(
    tx: &DrcPaymentChannelCloseTx,
    chain_id: &str,
    genesis: &Hash,
) -> DrcMultisignAttachmentKey {
    drc_multisign_attachment_key(
        DrcMultisignOperationKind::DrcPaymentChannelClose,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcMultisignAttachmentError {
    #[error("unsupported DRC multisign attachment key version {0}")]
    UnsupportedKeyVersion(u32),
    #[error("unsupported DRC multisign block attachment version {0}")]
    UnsupportedAttachmentVersion(u32),
    #[error("malformed multisign auth bundle")]
    MalformedAuth,
    #[error("attachment operation kind mismatch")]
    WrongOperationKind,
    #[error("DRC multisign attachments must be strictly sorted by kind then signing commitment")]
    UnsortedAttachments,
    #[error("duplicate DRC multisign attachment key")]
    DuplicateAttachmentKey,
    #[error("orphan DRC multisign attachment")]
    OrphanAttachment,
    #[error("missing DRC multisign attachment for multisigned operation")]
    MissingAttachment,
    #[error("unexpected DRC multisign attachment for single-signed operation")]
    UnexpectedAttachment,
    #[error("inline multisign on consensus operation envelope is forbidden")]
    InlineMultisignForbidden,
    #[error("mixed inline single-signature and attachment multisign")]
    MixedAuthorization,
    #[error("too many DRC multisign attachments")]
    TooManyAttachments,
    #[error("multisign signing_for does not match operation owner")]
    SigningForMismatch,
    #[error("DRC multisign attachment auth error: {0}")]
    Auth(DrcMultisignError),
}
