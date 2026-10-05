//! Owner-authorized DRC account-policy operations.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{Address, Amount, Hash};

/// First owner-authorized DRC account-policy envelope.
pub const DRC_ACCOUNT_POLICY_TX_VERSION: u32 = 1;
/// Current persisted DRC account-policy state.
pub const DRC_ACCOUNT_POLICY_STATE_VERSION: u32 = 1;
/// Domain separator for network-bound DRC account-policy signatures.
pub const DRC_ACCOUNT_POLICY_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-account-policy-v1";

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
}

impl DrcAccountPolicyAction {
    pub const fn require_destination_tag(self) -> bool {
        matches!(self, Self::SetRequireDestinationTag)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SetRequireDestinationTag => "set_require_destination_tag",
            Self::ClearRequireDestinationTag => "clear_require_destination_tag",
        }
    }
}

/// Signed DRC account-policy operation.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DrcAccountPolicyTx {
    pub version: u32,
    pub account: Address,
    pub action: DrcAccountPolicyAction,
    /// Explicit DRC fee credited to the DRC validator reward pool on acceptance.
    pub fee: Amount,
    /// Shared DRC account nonce used by transfers, stake ops, payments, and policies.
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl DrcAccountPolicyTx {
    pub fn validate_version(&self) -> Result<(), DrcAccountPolicyError> {
        if self.version != DRC_ACCOUNT_POLICY_TX_VERSION {
            return Err(DrcAccountPolicyError::UnsupportedVersion(self.version));
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        borsh::to_vec(&(
            DRC_ACCOUNT_POLICY_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.account,
            self.action,
            self.fee,
            self.nonce,
        ))
        .expect("borsh serialize DRC account-policy body")
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
            version: DRC_ACCOUNT_POLICY_TX_VERSION,
            account,
            action,
            fee,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
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
}

/// Canonical recipient policy. Missing state is exactly [`Self::default`].
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcAccountPolicy {
    pub version: u32,
    pub require_destination_tag: bool,
}

impl Default for DrcAccountPolicy {
    fn default() -> Self {
        Self {
            version: DRC_ACCOUNT_POLICY_STATE_VERSION,
            require_destination_tag: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcAccountPolicyError {
    #[error("unsupported DRC account-policy version {0}")]
    UnsupportedVersion(u32),
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

        assert!(set.action.require_destination_tag());
        assert!(!clear.action.require_destination_tag());
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
        tx.version += 1;
        assert_eq!(
            tx.validate_version(),
            Err(DrcAccountPolicyError::UnsupportedVersion(2))
        );
    }
}
