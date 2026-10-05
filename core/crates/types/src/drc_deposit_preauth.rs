//! Address-based, recipient-controlled DRC deposit preauthorization.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{Address, Amount, Hash};

/// First address-only DRC deposit-preauthorization envelope.
pub const DRC_DEPOSIT_PREAUTH_TX_VERSION: u32 = 1;
/// First persisted address-only DRC deposit-preauthorization record.
pub const DRC_DEPOSIT_PREAUTH_STATE_VERSION: u32 = 1;
/// Explicit operation type bound by every deposit-preauthorization signature.
pub const DRC_DEPOSIT_PREAUTH_TX_TYPE: &[u8] = b"drc_deposit_preauth";
/// Domain separator for network-bound deposit-preauthorization signatures.
pub const DRC_DEPOSIT_PREAUTH_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-deposit-preauth-v1";

/// Recipient-selected action for one source account.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcDepositPreauthAction {
    Authorize = 0,
    Unauthorize = 1,
}

impl DrcDepositPreauthAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authorize => "authorize",
            Self::Unauthorize => "unauthorize",
        }
    }
}

/// Owner-signed operation that grants or revokes one address-based preauthorization.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DrcDepositPreauthTx {
    pub version: u32,
    /// Recipient that owns and controls the preauthorization record.
    pub owner: Address,
    pub action: DrcDepositPreauthAction,
    /// Source account allowed to deposit to `owner` while DepositAuth is enabled.
    pub authorized_source: Address,
    /// Explicit DRC fee credited to the DRC validator reward pool on acceptance.
    pub fee: Amount,
    /// Shared DRC nonce used by transfers, stake ops, policies, preauths, and payments.
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl DrcDepositPreauthTx {
    pub fn validate_structure(&self) -> Result<(), DrcDepositPreauthError> {
        if self.version != DRC_DEPOSIT_PREAUTH_TX_VERSION {
            return Err(DrcDepositPreauthError::UnsupportedVersion(self.version));
        }
        if self.owner == Address::ZERO {
            return Err(DrcDepositPreauthError::ZeroOwner);
        }
        if self.authorized_source == Address::ZERO {
            return Err(DrcDepositPreauthError::ZeroAuthorizedSource);
        }
        if self.owner == self.authorized_source {
            return Err(DrcDepositPreauthError::SelfAuthorization);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        borsh::to_vec(&(
            DRC_DEPOSIT_PREAUTH_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_DEPOSIT_PREAUTH_TX_TYPE,
            self.version,
            self.owner,
            self.action,
            self.authorized_source,
            self.nonce,
            self.fee,
        ))
        .expect("borsh serialize DRC deposit-preauthorization body")
    }

    /// Hashes the complete signed envelope, including public authorization material.
    pub fn preauth_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn unsigned(
        owner: Address,
        action: DrcDepositPreauthAction,
        authorized_source: Address,
        fee: Amount,
        nonce: u64,
    ) -> Self {
        Self {
            version: DRC_DEPOSIT_PREAUTH_TX_VERSION,
            owner,
            action,
            authorized_source,
            fee,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn authorize(owner: Address, authorized_source: Address, fee: Amount, nonce: u64) -> Self {
        Self::unsigned(
            owner,
            DrcDepositPreauthAction::Authorize,
            authorized_source,
            fee,
            nonce,
        )
    }

    pub fn unauthorize(
        owner: Address,
        authorized_source: Address,
        fee: Amount,
        nonce: u64,
    ) -> Self {
        Self::unsigned(
            owner,
            DrcDepositPreauthAction::Unauthorize,
            authorized_source,
            fee,
            nonce,
        )
    }
}

/// Canonical active preauthorization record. Absence means unauthorized.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcDepositPreauth {
    pub version: u32,
    pub owner: Address,
    pub authorized_source: Address,
}

impl DrcDepositPreauth {
    pub fn new(owner: Address, authorized_source: Address) -> Self {
        Self {
            version: DRC_DEPOSIT_PREAUTH_STATE_VERSION,
            owner,
            authorized_source,
        }
    }

    pub fn validate(&self) -> Result<(), DrcDepositPreauthError> {
        if self.version != DRC_DEPOSIT_PREAUTH_STATE_VERSION {
            return Err(DrcDepositPreauthError::UnsupportedStateVersion(
                self.version,
            ));
        }
        if self.owner == Address::ZERO {
            return Err(DrcDepositPreauthError::ZeroOwner);
        }
        if self.authorized_source == Address::ZERO {
            return Err(DrcDepositPreauthError::ZeroAuthorizedSource);
        }
        if self.owner == self.authorized_source {
            return Err(DrcDepositPreauthError::SelfAuthorization);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcDepositPreauthError {
    #[error("unsupported DRC deposit-preauthorization transaction version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported DRC deposit-preauthorization state version {0}")]
    UnsupportedStateVersion(u32),
    #[error("DRC deposit-preauthorization owner must be non-zero")]
    ZeroOwner,
    #[error("DRC deposit-preauthorization source must be non-zero")]
    ZeroAuthorizedSource,
    #[error("DRC accounts cannot preauthorize themselves")]
    SelfAuthorization,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_preauthorization_roundtrips_and_binds_action() {
        let authorize = DrcDepositPreauthTx::authorize(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(3),
            4,
        );
        authorize.validate_structure().unwrap();
        let mut unauthorize = authorize.clone();
        unauthorize.action = DrcDepositPreauthAction::Unauthorize;

        assert_ne!(
            authorize.signing_bytes_bound("agora-dev", &Hash([5; 32])),
            unauthorize.signing_bytes_bound("agora-dev", &Hash([5; 32]))
        );
        let bytes = borsh::to_vec(&authorize).unwrap();
        assert_eq!(
            DrcDepositPreauthTx::try_from_slice(&bytes).unwrap(),
            authorize
        );
        let record = DrcDepositPreauth::new(authorize.owner, authorize.authorized_source);
        record.validate().unwrap();
        assert_eq!(
            DrcDepositPreauth::try_from_slice(&borsh::to_vec(&record).unwrap()).unwrap(),
            record
        );
    }

    #[test]
    fn invalid_and_future_records_fail_closed() {
        let valid =
            DrcDepositPreauthTx::authorize(Address([1; 20]), Address([2; 20]), Amount::ZERO, 0);
        for malformed in [
            DrcDepositPreauthTx {
                version: DRC_DEPOSIT_PREAUTH_TX_VERSION - 1,
                ..valid.clone()
            },
            DrcDepositPreauthTx {
                version: DRC_DEPOSIT_PREAUTH_TX_VERSION + 1,
                ..valid.clone()
            },
            DrcDepositPreauthTx {
                owner: Address::ZERO,
                ..valid.clone()
            },
            DrcDepositPreauthTx {
                authorized_source: Address::ZERO,
                ..valid.clone()
            },
            DrcDepositPreauthTx {
                authorized_source: valid.owner,
                ..valid
            },
        ] {
            assert!(malformed.validate_structure().is_err());
        }
    }
}
