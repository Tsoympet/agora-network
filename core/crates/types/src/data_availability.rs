//! Provenance-bound data-commitment types for the Trident L1 block lane.
//!
//! These types define canonical bytes and operator authorization consumed by
//! [`crate::Block::data_commitments`]. Standalone mempool/RPC submission remains
//! disabled until Trident defines the TLT inclusion-fee policy.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{Address, Hash};

/// Domain for the source commitment bytes. Changing the payload requires a new version/domain.
pub const DA_COMMITMENT_PAYLOAD_DOMAIN: &[u8] = b"agora-trident-da-commitment-v1";
/// Domain for an operator's network-bound authorization of one commitment.
pub const DA_COMMITMENT_AUTHORIZATION_DOMAIN: &[u8] = b"agora-trident-da-authorization-v1";
/// Domain for the complete signed authorization identifier.
pub const DA_COMMITMENT_AUTHORIZATION_ID_DOMAIN: &[u8] = b"agora-trident-da-authorization-id-v1";
pub const DA_COMMITMENT_VERSION: u32 = 1;
pub const DA_COMMITMENT_AUTHORIZATION_VERSION: u32 = 1;
pub const MAX_DA_CHAIN_ID_BYTES: usize = 128;

/// Append-only source discriminant for explicitly non-canonical producer data.
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
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DataCommitmentSource {
    /// Historical `agora-layers` Ovolos batch data; never canonical OVL monetary state.
    AgoraLayersOvolosBatchLab = 0x00,
}

impl DataCommitmentSource {
    /// Stable key/wire byte. Future source variants must be appended.
    pub const fn wire_byte(self) -> u8 {
        self as u8
    }
}

/// Deterministic commitment to one historical `agora-layers` Ovolos batch.
///
/// This commits integrity and provenance. It does not prove that the underlying
/// transaction data is available and does not make lab balances canonical.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DataAvailabilityCommitment {
    pub version: u32,
    pub source: DataCommitmentSource,
    pub source_chain_id: String,
    pub source_genesis_hash: Hash,
    pub batch_id: Hash,
    pub sequence: u64,
    pub prev_state_root: Hash,
    pub post_state_root: Hash,
    pub tx_merkle_root: Hash,
    pub tx_count: u32,
    pub source_timestamp_ms: u64,
}

impl DataAvailabilityCommitment {
    #[allow(clippy::too_many_arguments)]
    pub fn agora_layers_ovolos_batch(
        source_chain_id: String,
        source_genesis_hash: Hash,
        batch_id: Hash,
        sequence: u64,
        prev_state_root: Hash,
        post_state_root: Hash,
        tx_merkle_root: Hash,
        tx_count: u32,
        source_timestamp_ms: u64,
    ) -> Self {
        Self {
            version: DA_COMMITMENT_VERSION,
            source: DataCommitmentSource::AgoraLayersOvolosBatchLab,
            source_chain_id,
            source_genesis_hash,
            batch_id,
            sequence,
            prev_state_root,
            post_state_root,
            tx_merkle_root,
            tx_count,
            source_timestamp_ms,
        }
    }

    pub fn validate(&self) -> Result<(), DataCommitmentError> {
        if self.version != DA_COMMITMENT_VERSION {
            return Err(DataCommitmentError::UnsupportedCommitmentVersion(
                self.version,
            ));
        }
        validate_chain_id(&self.source_chain_id)?;
        if self.source_genesis_hash == Hash::ZERO {
            return Err(DataCommitmentError::MissingSourceGenesis);
        }
        if self.batch_id == Hash::ZERO {
            return Err(DataCommitmentError::MissingBatchId);
        }
        Ok(())
    }

    /// Canonical, domain-separated Borsh payload consumed by consensus.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        borsh::to_vec(&(DA_COMMITMENT_PAYLOAD_DOMAIN, self))
            .expect("borsh serialize data commitment")
    }

    pub fn commitment_id(&self) -> Hash {
        Hash::hash_bytes(&self.canonical_bytes())
    }
}

/// Signed operator authorization carried in [`crate::Block::data_commitments`].
///
/// `replay_nonce` is cryptographically bound here and enforced atomically by
/// the state transition. This type alone is not replay protection and remains
/// deliberately unavailable through standalone RPC submission.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DataCommitmentAuthorization {
    pub version: u32,
    pub operator: Address,
    pub replay_nonce: u64,
    pub commitment: DataAvailabilityCommitment,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl DataCommitmentAuthorization {
    pub fn unsigned(
        operator: Address,
        replay_nonce: u64,
        commitment: DataAvailabilityCommitment,
    ) -> Self {
        Self {
            version: DA_COMMITMENT_AUTHORIZATION_VERSION,
            operator,
            replay_nonce,
            commitment,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), DataCommitmentError> {
        if self.version != DA_COMMITMENT_AUTHORIZATION_VERSION {
            return Err(DataCommitmentError::UnsupportedAuthorizationVersion(
                self.version,
            ));
        }
        self.commitment.validate()
    }

    /// Bind the source commitment to one L1 chain, genesis, mesh fingerprint,
    /// operator, and replay nonce.
    pub fn signing_bytes_bound(
        &self,
        l1_chain_id: &str,
        l1_genesis: &Hash,
        l1_network_fingerprint: &Hash,
    ) -> Vec<u8> {
        borsh::to_vec(&(
            DA_COMMITMENT_AUTHORIZATION_DOMAIN,
            l1_chain_id,
            l1_genesis.as_bytes(),
            l1_network_fingerprint.as_bytes(),
            self.version,
            self.operator,
            self.replay_nonce,
            self.commitment.commitment_id(),
        ))
        .expect("borsh serialize data commitment authorization")
    }

    /// Hash the complete authorization, including secp256k1 auth material.
    pub fn authorization_id(&self) -> Hash {
        Hash::hash_borsh(&(DA_COMMITMENT_AUTHORIZATION_ID_DOMAIN, self))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DataCommitmentError {
    #[error("unsupported data commitment version {0}")]
    UnsupportedCommitmentVersion(u32),
    #[error("unsupported data commitment authorization version {0}")]
    UnsupportedAuthorizationVersion(u32),
    #[error("source chain id is empty, oversized, or non-canonical")]
    InvalidSourceChainId,
    #[error("source genesis hash is required")]
    MissingSourceGenesis,
    #[error("source batch id is required")]
    MissingBatchId,
}

fn validate_chain_id(chain_id: &str) -> Result<(), DataCommitmentError> {
    let bytes = chain_id.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_DA_CHAIN_ID_BYTES
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-._".contains(byte))
    {
        return Err(DataCommitmentError::InvalidSourceChainId);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commitment() -> DataAvailabilityCommitment {
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
        )
    }

    #[test]
    fn commitment_bytes_are_domain_separated_and_deterministic() {
        let commitment = commitment();
        commitment.validate().unwrap();
        let first = commitment.canonical_bytes();
        let second = commitment.canonical_bytes();
        assert_eq!(first, second);
        assert_ne!(first, borsh::to_vec(&commitment).unwrap());
        assert!(first
            .windows(DA_COMMITMENT_PAYLOAD_DOMAIN.len())
            .any(|window| window == DA_COMMITMENT_PAYLOAD_DOMAIN));

        let decoded =
            DataAvailabilityCommitment::try_from_slice(&borsh::to_vec(&commitment).unwrap())
                .unwrap();
        assert_eq!(decoded, commitment);
        assert_eq!(decoded.commitment_id(), commitment.commitment_id());
    }

    #[test]
    fn every_provenance_field_changes_the_commitment() {
        let original = commitment();
        let original_id = original.commitment_id();

        let mut changed = original.clone();
        changed.source_chain_id = "agora-ovolos-dev-1".into();
        assert_ne!(changed.commitment_id(), original_id);

        let mut changed = original.clone();
        changed.source_genesis_hash = Hash([9; 32]);
        assert_ne!(changed.commitment_id(), original_id);

        let mut changed = original.clone();
        changed.sequence += 1;
        assert_ne!(changed.commitment_id(), original_id);

        let mut changed = original;
        changed.tx_merkle_root = Hash([9; 32]);
        assert_ne!(changed.commitment_id(), original_id);
    }

    #[test]
    fn validation_rejects_ambiguous_provenance() {
        let mut invalid = commitment();
        invalid.source_chain_id = "Agora Testnet".into();
        assert_eq!(
            invalid.validate(),
            Err(DataCommitmentError::InvalidSourceChainId)
        );

        let mut invalid = commitment();
        invalid.source_chain_id = "a".repeat(MAX_DA_CHAIN_ID_BYTES + 1);
        assert_eq!(
            invalid.validate(),
            Err(DataCommitmentError::InvalidSourceChainId)
        );

        let mut invalid = commitment();
        invalid.source_genesis_hash = Hash::ZERO;
        assert_eq!(
            invalid.validate(),
            Err(DataCommitmentError::MissingSourceGenesis)
        );
    }

    #[test]
    fn authorization_preimage_binds_l1_identity_and_nonce() {
        let mut authorization =
            DataCommitmentAuthorization::unsigned(Address([7; 20]), 11, commitment());
        let genesis = Hash([8; 32]);
        let fingerprint = Hash([9; 32]);
        let original =
            authorization.signing_bytes_bound("agora-trident-testnet-1", &genesis, &fingerprint);

        authorization.replay_nonce += 1;
        assert_ne!(
            authorization.signing_bytes_bound("agora-trident-testnet-1", &genesis, &fingerprint),
            original
        );
        authorization.replay_nonce -= 1;
        assert_ne!(
            authorization.signing_bytes_bound("agora-trident-dev-1", &genesis, &fingerprint),
            original
        );
        assert_ne!(
            authorization.signing_bytes_bound("agora-trident-testnet-1", &genesis, &Hash([10; 32])),
            original
        );
    }

    #[test]
    fn source_wire_discriminant_is_stable() {
        assert_eq!(
            borsh::to_vec(&DataCommitmentSource::AgoraLayersOvolosBatchLab).unwrap(),
            vec![0]
        );
        assert_eq!(
            DataCommitmentSource::AgoraLayersOvolosBatchLab.wire_byte(),
            0
        );
    }
}
