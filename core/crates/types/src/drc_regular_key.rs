//! Owner-controlled DRC regular-key rotation (secp256k1, contract-free).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{Address, Amount, Hash};

/// First dedicated regular-key operation envelope.
pub const DRC_REGULAR_KEY_TX_VERSION: u32 = 1;
/// First persisted regular-key state record.
pub const DRC_REGULAR_KEY_STATE_VERSION: u32 = 1;
/// Explicit operation type bound by every regular-key signature.
pub const DRC_REGULAR_KEY_TX_TYPE: &[u8] = b"drc_regular_key";
/// Domain separator for network-bound regular-key signatures.
pub const DRC_REGULAR_KEY_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-regular-key-v1";

/// Install or remove the single rotatable secondary key for one DRC account.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcRegularKeyAction {
    /// Bind `regular_key` + compressed `regular_key_public_key` (33 bytes).
    Set = 0,
    /// Remove any installed regular key.
    Clear = 1,
}

impl DrcRegularKeyAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Clear => "clear",
        }
    }
}

/// Owner-signed operation that sets, replaces, or clears one regular key.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DrcRegularKeyTx {
    pub version: u32,
    /// DRC account whose regular key is being changed.
    pub owner: Address,
    pub action: DrcRegularKeyAction,
    /// Address derived from `regular_key_public_key` when `action == Set`.
    pub regular_key: Address,
    /// Compressed secp256k1 public key (33 bytes) for `Set`; empty for `Clear`.
    pub regular_key_public_key: Vec<u8>,
    pub fee: Amount,
    /// Shared DRC nonce used by transfers, stake ops, policies, preauths, and payments.
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl DrcRegularKeyTx {
    pub fn validate_structure(&self) -> Result<(), DrcRegularKeyError> {
        if self.version != DRC_REGULAR_KEY_TX_VERSION {
            return Err(DrcRegularKeyError::UnsupportedVersion(self.version));
        }
        if self.owner == Address::ZERO {
            return Err(DrcRegularKeyError::ZeroOwner);
        }
        match self.action {
            DrcRegularKeyAction::Set => {
                if self.regular_key == Address::ZERO {
                    return Err(DrcRegularKeyError::ZeroRegularKey);
                }
                if self.regular_key == self.owner {
                    return Err(DrcRegularKeyError::MasterAsRegularKey);
                }
                if self.regular_key_public_key.len() != 33 {
                    return Err(DrcRegularKeyError::InvalidRegularKeyPublicKey);
                }
            }
            DrcRegularKeyAction::Clear => {
                if self.regular_key != Address::ZERO || !self.regular_key_public_key.is_empty() {
                    return Err(DrcRegularKeyError::ClearMustBeEmpty);
                }
            }
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        borsh::to_vec(&(
            DRC_REGULAR_KEY_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_REGULAR_KEY_TX_TYPE,
            self.version,
            self.owner,
            self.action,
            self.regular_key,
            self.regular_key_public_key.as_slice(),
            self.nonce,
            self.fee,
        ))
        .expect("borsh serialize DRC regular-key body")
    }

    pub fn regular_key_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn unsigned(
        owner: Address,
        action: DrcRegularKeyAction,
        regular_key: Address,
        regular_key_public_key: Vec<u8>,
        fee: Amount,
        nonce: u64,
    ) -> Self {
        Self {
            version: DRC_REGULAR_KEY_TX_VERSION,
            owner,
            action,
            regular_key,
            regular_key_public_key,
            fee,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn set(
        owner: Address,
        regular_key: Address,
        regular_key_public_key: Vec<u8>,
        fee: Amount,
        nonce: u64,
    ) -> Self {
        Self::unsigned(
            owner,
            DrcRegularKeyAction::Set,
            regular_key,
            regular_key_public_key,
            fee,
            nonce,
        )
    }

    pub fn clear(owner: Address, fee: Amount, nonce: u64) -> Self {
        Self::unsigned(
            owner,
            DrcRegularKeyAction::Clear,
            Address::ZERO,
            Vec::new(),
            fee,
            nonce,
        )
    }
}

/// Canonical installed regular key for one owner account.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcAccountRegularKey {
    pub version: u32,
    pub owner: Address,
    pub regular_key: Address,
}

impl DrcAccountRegularKey {
    pub fn new(owner: Address, regular_key: Address) -> Self {
        Self {
            version: DRC_REGULAR_KEY_STATE_VERSION,
            owner,
            regular_key,
        }
    }

    pub fn validate(&self) -> Result<(), DrcRegularKeyError> {
        if self.version != DRC_REGULAR_KEY_STATE_VERSION {
            return Err(DrcRegularKeyError::UnsupportedStateVersion(self.version));
        }
        if self.owner == Address::ZERO || self.regular_key == Address::ZERO {
            return Err(DrcRegularKeyError::ZeroRegularKey);
        }
        if self.owner == self.regular_key {
            return Err(DrcRegularKeyError::MasterAsRegularKey);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcRegularKeyError {
    #[error("unsupported DRC regular-key transaction version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported DRC regular-key state version {0}")]
    UnsupportedStateVersion(u32),
    #[error("DRC regular-key owner must be non-zero")]
    ZeroOwner,
    #[error("DRC regular key must be non-zero")]
    ZeroRegularKey,
    #[error("master key cannot be installed as its own regular key")]
    MasterAsRegularKey,
    #[error("regular-key public key must be 33-byte compressed secp256k1")]
    InvalidRegularKeyPublicKey,
    #[error("clear regular-key operation must leave key fields empty")]
    ClearMustBeEmpty,
    #[error("regular-key public key does not match regular_key address")]
    RegularKeyAddressMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_key_roundtrip_and_signing_bytes_differ_by_action() {
        let set = DrcRegularKeyTx::set(
            Address([1; 20]),
            Address([2; 20]),
            vec![3; 33],
            Amount::from_base_units(1),
            4,
        );
        set.validate_structure().unwrap();
        let clear = DrcRegularKeyTx::clear(Address([1; 20]), Amount::from_base_units(1), 4);
        clear.validate_structure().unwrap();
        assert_ne!(
            set.signing_bytes_bound("agora-dev", &Hash([5; 32])),
            clear.signing_bytes_bound("agora-dev", &Hash([5; 32]))
        );
        let bytes = borsh::to_vec(&set).unwrap();
        assert_eq!(DrcRegularKeyTx::try_from_slice(&bytes).unwrap(), set);
    }

    #[test]
    fn malformed_and_future_versions_fail_closed() {
        let valid = DrcRegularKeyTx::set(
            Address([1; 20]),
            Address([2; 20]),
            vec![3; 33],
            Amount::ZERO,
            0,
        );
        for malformed in [
            DrcRegularKeyTx {
                version: DRC_REGULAR_KEY_TX_VERSION + 1,
                ..valid.clone()
            },
            DrcRegularKeyTx {
                owner: Address::ZERO,
                ..valid.clone()
            },
            DrcRegularKeyTx {
                regular_key: Address::ZERO,
                ..valid.clone()
            },
            DrcRegularKeyTx {
                regular_key: valid.owner,
                ..valid.clone()
            },
            DrcRegularKeyTx {
                regular_key_public_key: vec![1; 32],
                ..valid.clone()
            },
            DrcRegularKeyTx {
                action: DrcRegularKeyAction::Clear,
                regular_key: Address([2; 20]),
                regular_key_public_key: vec![3; 33],
                ..valid.clone()
            },
        ] {
            assert!(malformed.validate_structure().is_err());
        }
    }
}
