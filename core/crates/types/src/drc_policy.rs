//! Owner-authorized DRC account-policy operations.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::{Address, Amount, Hash};

/// Frozen destination-tag-only account-policy envelope.
pub const DRC_ACCOUNT_POLICY_LEGACY_TX_VERSION: u32 = 1;
/// Current account-policy envelope; adds DepositAuth set/clear actions.
pub const DRC_ACCOUNT_POLICY_TX_VERSION: u32 = 2;
/// Master-key disable set/clear (DRC-scoped; rippled `asfDisableMaster` subset).
pub const DRC_ACCOUNT_POLICY_MASTER_KEY_TX_VERSION: u32 = 3;
/// Frozen destination-tag-only persisted policy encoding.
pub const DRC_ACCOUNT_POLICY_LEGACY_STATE_VERSION: u32 = 1;
/// Current persisted policy encoding; appends `deposit_auth_required`.
pub const DRC_ACCOUNT_POLICY_STATE_VERSION: u32 = 2;
/// Persisted policy encoding; appends `master_key_disabled`.
pub const DRC_ACCOUNT_POLICY_MASTER_KEY_STATE_VERSION: u32 = 3;
/// Frozen v1 signature domain.
pub const DRC_ACCOUNT_POLICY_V1_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-account-policy-v1";
/// V2 signature domain for DepositAuth policy actions.
pub const DRC_ACCOUNT_POLICY_V2_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-account-policy-v2";
/// V3 signature domain for master-key disable policy actions.
pub const DRC_ACCOUNT_POLICY_V3_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-account-policy-v3";
/// Backward-compatible name for the frozen v1 signing domain.
pub const DRC_ACCOUNT_POLICY_SIGNING_DOMAIN: &[u8] = DRC_ACCOUNT_POLICY_V1_SIGNING_DOMAIN;
/// Explicit operation type bound by v2 signatures.
pub const DRC_ACCOUNT_POLICY_TX_TYPE: &[u8] = b"drc_account_policy";

/// The only DRC account-policy actions activated in this bounded slice.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcAccountPolicyAction {
    SetRequireDestinationTag = 0,
    ClearRequireDestinationTag = 1,
    SetDepositAuthRequired = 2,
    ClearDepositAuthRequired = 3,
    SetMasterKeyDisabled = 4,
    ClearMasterKeyDisabled = 5,
}

impl DrcAccountPolicyAction {
    pub const fn destination_tag_requirement(self) -> Option<bool> {
        match self {
            Self::SetRequireDestinationTag => Some(true),
            Self::ClearRequireDestinationTag => Some(false),
            Self::SetDepositAuthRequired | Self::ClearDepositAuthRequired => None,
            Self::SetMasterKeyDisabled | Self::ClearMasterKeyDisabled => None,
        }
    }

    pub const fn deposit_auth_requirement(self) -> Option<bool> {
        match self {
            Self::SetDepositAuthRequired => Some(true),
            Self::ClearDepositAuthRequired => Some(false),
            Self::SetRequireDestinationTag | Self::ClearRequireDestinationTag => None,
            Self::SetMasterKeyDisabled | Self::ClearMasterKeyDisabled => None,
        }
    }

    pub const fn master_key_disabled_requirement(self) -> Option<bool> {
        match self {
            Self::SetMasterKeyDisabled => Some(true),
            Self::ClearMasterKeyDisabled => Some(false),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SetRequireDestinationTag => "set_require_destination_tag",
            Self::ClearRequireDestinationTag => "clear_require_destination_tag",
            Self::SetDepositAuthRequired => "set_deposit_auth_required",
            Self::ClearDepositAuthRequired => "clear_deposit_auth_required",
            Self::SetMasterKeyDisabled => "set_master_key_disabled",
            Self::ClearMasterKeyDisabled => "clear_master_key_disabled",
        }
    }
}

/// Signed DRC account-policy operation.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DrcAccountPolicyTx {
    pub version: u32,
    pub account: Address,
    pub action: DrcAccountPolicyAction,
    /// Explicit DRC fee credited to the DRC validator reward pool on acceptance.
    pub fee: Amount,
    /// Shared DRC nonce used by transfers, stake ops, policies, preauths, and payments.
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcAccountPolicyTx {
    pub fn validate_version(&self) -> Result<(), DrcAccountPolicyError> {
        match (self.version, self.action) {
            (
                DRC_ACCOUNT_POLICY_LEGACY_TX_VERSION,
                DrcAccountPolicyAction::SetRequireDestinationTag
                | DrcAccountPolicyAction::ClearRequireDestinationTag,
            )
            | (
                DRC_ACCOUNT_POLICY_TX_VERSION,
                DrcAccountPolicyAction::SetDepositAuthRequired
                | DrcAccountPolicyAction::ClearDepositAuthRequired,
            )
            | (
                DRC_ACCOUNT_POLICY_MASTER_KEY_TX_VERSION,
                DrcAccountPolicyAction::SetMasterKeyDisabled
                | DrcAccountPolicyAction::ClearMasterKeyDisabled,
            ) => Ok(()),
            (
                DRC_ACCOUNT_POLICY_LEGACY_TX_VERSION
                | DRC_ACCOUNT_POLICY_TX_VERSION
                | DRC_ACCOUNT_POLICY_MASTER_KEY_TX_VERSION,
                action,
            ) => Err(DrcAccountPolicyError::ActionVersionMismatch {
                version: self.version,
                action,
            }),
            (_, _) if self.version > DRC_ACCOUNT_POLICY_MASTER_KEY_TX_VERSION => {
                Err(DrcAccountPolicyError::UnsupportedVersion(self.version))
            }
            (version, _) => Err(DrcAccountPolicyError::UnsupportedVersion(version)),
        }
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version == DRC_ACCOUNT_POLICY_LEGACY_TX_VERSION {
            return borsh::to_vec(&(
                DRC_ACCOUNT_POLICY_V1_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                self.version,
                self.account,
                self.action,
                self.fee,
                self.nonce,
            ))
            .expect("borsh serialize DRC account-policy v1 body");
        }
        if self.version == DRC_ACCOUNT_POLICY_MASTER_KEY_TX_VERSION {
            return borsh::to_vec(&(
                DRC_ACCOUNT_POLICY_V3_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_ACCOUNT_POLICY_TX_TYPE,
                self.version,
                self.account,
                self.action,
                self.fee,
                self.nonce,
            ))
            .expect("borsh serialize DRC account-policy v3 body");
        }
        borsh::to_vec(&(
            DRC_ACCOUNT_POLICY_V2_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_ACCOUNT_POLICY_TX_TYPE,
            self.version,
            self.account,
            self.action,
            self.fee,
            self.nonce,
        ))
        .expect("borsh serialize DRC account-policy v2 body")
    }

    /// Hashes the complete signed envelope, including authorization material.
    pub fn policy_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn unsigned(
        account: Address,
        action: DrcAccountPolicyAction,
        fee: Amount,
        nonce: u64,
    ) -> Self {
        Self {
            version: DRC_ACCOUNT_POLICY_LEGACY_TX_VERSION,
            account,
            action,
            fee,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        }
    }

    pub fn set_require_destination_tag(account: Address, fee: Amount, nonce: u64) -> Self {
        Self::unsigned(
            account,
            DrcAccountPolicyAction::SetRequireDestinationTag,
            fee,
            nonce,
        )
    }

    pub fn clear_require_destination_tag(account: Address, fee: Amount, nonce: u64) -> Self {
        Self::unsigned(
            account,
            DrcAccountPolicyAction::ClearRequireDestinationTag,
            fee,
            nonce,
        )
    }

    pub fn set_deposit_auth_required(account: Address, fee: Amount, nonce: u64) -> Self {
        let mut tx = Self::unsigned(
            account,
            DrcAccountPolicyAction::SetDepositAuthRequired,
            fee,
            nonce,
        );
        tx.version = DRC_ACCOUNT_POLICY_TX_VERSION;
        tx
    }

    pub fn clear_deposit_auth_required(account: Address, fee: Amount, nonce: u64) -> Self {
        let mut tx = Self::unsigned(
            account,
            DrcAccountPolicyAction::ClearDepositAuthRequired,
            fee,
            nonce,
        );
        tx.version = DRC_ACCOUNT_POLICY_TX_VERSION;
        tx
    }

    pub fn set_master_key_disabled(account: Address, fee: Amount, nonce: u64) -> Self {
        let mut tx = Self::unsigned(
            account,
            DrcAccountPolicyAction::SetMasterKeyDisabled,
            fee,
            nonce,
        );
        tx.version = DRC_ACCOUNT_POLICY_MASTER_KEY_TX_VERSION;
        tx
    }

    pub fn clear_master_key_disabled(account: Address, fee: Amount, nonce: u64) -> Self {
        let mut tx = Self::unsigned(
            account,
            DrcAccountPolicyAction::ClearMasterKeyDisabled,
            fee,
            nonce,
        );
        tx.version = DRC_ACCOUNT_POLICY_MASTER_KEY_TX_VERSION;
        tx
    }
}

impl BorshSerialize for DrcAccountPolicyTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.account, writer)?;
        BorshSerialize::serialize(&self.action, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcAccountPolicyTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            account: Address::deserialize_reader(reader)?,
            action: DrcAccountPolicyAction::deserialize_reader(reader)?,
            fee: Amount::deserialize_reader(reader)?,
            nonce: u64::deserialize_reader(reader)?,
            public_key: Vec::<u8>::deserialize_reader(reader)?,
            signature: Vec::<u8>::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

/// Canonical recipient policy. Missing state is exactly [`Self::default`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrcAccountPolicy {
    pub version: u32,
    pub require_destination_tag: bool,
    /// Incoming non-self payments require an address preauthorization when enabled.
    #[serde(default)]
    pub deposit_auth_required: bool,
    /// When set, the owner master secp256k1 key cannot authorize DRC account operations.
    #[serde(default)]
    pub master_key_disabled: bool,
}

impl Default for DrcAccountPolicy {
    fn default() -> Self {
        Self {
            // The all-false default retains the frozen v1 representation. State
            // upgrades to v2 only when DepositAuth is explicitly touched.
            version: DRC_ACCOUNT_POLICY_LEGACY_STATE_VERSION,
            require_destination_tag: false,
            deposit_auth_required: false,
            master_key_disabled: false,
        }
    }
}

impl DrcAccountPolicy {
    pub fn validate(&self) -> Result<(), DrcAccountPolicyError> {
        match self.version {
            DRC_ACCOUNT_POLICY_LEGACY_STATE_VERSION
                if self.deposit_auth_required || self.master_key_disabled =>
            {
                Err(DrcAccountPolicyError::LegacyExtendedField)
            }
            DRC_ACCOUNT_POLICY_STATE_VERSION if self.master_key_disabled => {
                Err(DrcAccountPolicyError::V2MasterKeyDisabled)
            }
            DRC_ACCOUNT_POLICY_LEGACY_STATE_VERSION
            | DRC_ACCOUNT_POLICY_STATE_VERSION
            | DRC_ACCOUNT_POLICY_MASTER_KEY_STATE_VERSION => Ok(()),
            version => Err(DrcAccountPolicyError::UnsupportedStateVersion(version)),
        }
    }

    pub fn canonical_state_version(&self) -> u32 {
        if self.master_key_disabled {
            DRC_ACCOUNT_POLICY_MASTER_KEY_STATE_VERSION
        } else if self.deposit_auth_required {
            DRC_ACCOUNT_POLICY_STATE_VERSION
        } else {
            DRC_ACCOUNT_POLICY_LEGACY_STATE_VERSION
        }
    }
}

impl BorshSerialize for DrcAccountPolicy {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        self.validate()
            .map_err(|error| borsh::io::Error::new(borsh::io::ErrorKind::InvalidData, error))?;
        let version = self.canonical_state_version();
        BorshSerialize::serialize(&version, writer)?;
        BorshSerialize::serialize(&self.require_destination_tag, writer)?;
        if version >= DRC_ACCOUNT_POLICY_STATE_VERSION {
            BorshSerialize::serialize(&self.deposit_auth_required, writer)?;
        }
        if version >= DRC_ACCOUNT_POLICY_MASTER_KEY_STATE_VERSION {
            BorshSerialize::serialize(&self.master_key_disabled, writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for DrcAccountPolicy {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        let version = u32::deserialize_reader(reader)?;
        let require_destination_tag = bool::deserialize_reader(reader)?;
        let (deposit_auth_required, master_key_disabled) = match version {
            DRC_ACCOUNT_POLICY_LEGACY_STATE_VERSION => (false, false),
            DRC_ACCOUNT_POLICY_STATE_VERSION => (bool::deserialize_reader(reader)?, false),
            DRC_ACCOUNT_POLICY_MASTER_KEY_STATE_VERSION => (
                bool::deserialize_reader(reader)?,
                bool::deserialize_reader(reader)?,
            ),
            _ => {
                return Err(borsh::io::Error::new(
                    borsh::io::ErrorKind::InvalidData,
                    format!("unsupported DRC account-policy state version {version}"),
                ));
            }
        };
        Ok(Self {
            version,
            require_destination_tag,
            deposit_auth_required,
            master_key_disabled,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcAccountPolicyError {
    #[error("unsupported DRC account-policy version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported DRC account-policy state version {0}")]
    UnsupportedStateVersion(u32),
    #[error("DRC account-policy action {action:?} is invalid for envelope version {version}")]
    ActionVersionMismatch {
        version: u32,
        action: DrcAccountPolicyAction,
    },
    #[error("DRC account-policy v1 state cannot carry extended fields")]
    LegacyExtendedField,
    #[error("DRC account-policy v2 state cannot carry master-key disable")]
    V2MasterKeyDisabled,
    #[error("DRC account-policy v1 state cannot carry DepositAuth")]
    LegacyDepositAuth,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_actions_and_signed_envelope_roundtrip() {
        let set = DrcAccountPolicyTx::set_require_destination_tag(
            Address([1; 20]),
            Amount::from_base_units(2),
            3,
        );
        let clear =
            DrcAccountPolicyTx::clear_require_destination_tag(set.account, set.fee, set.nonce);

        assert_eq!(set.action.destination_tag_requirement(), Some(true));
        assert_eq!(clear.action.destination_tag_requirement(), Some(false));
        assert_ne!(
            set.signing_bytes_bound("agora-dev", &Hash::ZERO),
            clear.signing_bytes_bound("agora-dev", &Hash::ZERO)
        );
        let bytes = borsh::to_vec(&set).unwrap();
        assert_eq!(DrcAccountPolicyTx::try_from_slice(&bytes).unwrap(), set);
    }

    #[test]
    fn policy_version_fails_closed() {
        let mut tx =
            DrcAccountPolicyTx::set_require_destination_tag(Address([1; 20]), Amount::ZERO, 0);
        tx.version = DRC_ACCOUNT_POLICY_MASTER_KEY_TX_VERSION + 1;
        assert_eq!(
            tx.validate_version(),
            Err(DrcAccountPolicyError::UnsupportedVersion(4))
        );
    }

    #[test]
    fn frozen_policy_v1_bytes_survive_v2_extension() {
        #[derive(BorshSerialize)]
        struct FrozenPolicyV1 {
            version: u32,
            require_destination_tag: bool,
        }

        let policy = DrcAccountPolicy {
            version: DRC_ACCOUNT_POLICY_LEGACY_STATE_VERSION,
            require_destination_tag: true,
            deposit_auth_required: false,
            master_key_disabled: false,
        };
        let expected = borsh::to_vec(&FrozenPolicyV1 {
            version: DRC_ACCOUNT_POLICY_LEGACY_STATE_VERSION,
            require_destination_tag: true,
        })
        .unwrap();
        assert_eq!(borsh::to_vec(&policy).unwrap(), expected);
        assert_eq!(DrcAccountPolicy::try_from_slice(&expected).unwrap(), policy);
    }

    #[test]
    fn deposit_auth_policy_uses_v2_envelope_and_state() {
        let set = DrcAccountPolicyTx::set_deposit_auth_required(
            Address([1; 20]),
            Amount::from_base_units(2),
            3,
        );
        assert_eq!(set.version, DRC_ACCOUNT_POLICY_TX_VERSION);
        assert_eq!(set.action.deposit_auth_requirement(), Some(true));
        set.validate_version().unwrap();
        assert!(DrcAccountPolicyTx {
            version: DRC_ACCOUNT_POLICY_LEGACY_TX_VERSION,
            ..set.clone()
        }
        .validate_version()
        .is_err());

        let state = DrcAccountPolicy {
            version: DRC_ACCOUNT_POLICY_STATE_VERSION,
            require_destination_tag: true,
            deposit_auth_required: true,
            master_key_disabled: false,
        };
        state.validate().unwrap();
        let bytes = borsh::to_vec(&state).unwrap();
        assert_eq!(DrcAccountPolicy::try_from_slice(&bytes).unwrap(), state);
    }
}
