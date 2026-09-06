//! Offline-only, version-gated Trident header commitment.
//!
//! This type deliberately does not replace [`crate::BlockHeader`] or appear in
//! [`crate::Block`]. Its Borsh representation is a domain/version envelope so a
//! future runtime can select Trident semantics without changing frozen v2 bytes.

use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

use crate::Hash;

/// Fixed domain at the start of every canonical Trident header encoding.
pub const TRIDENT_HEADER_ENCODING_DOMAIN: [u8; 32] = *b"agora-trident-header-envelope-v1";
/// Only encoding version understood by this offline prerequisite.
pub const TRIDENT_HEADER_ENCODING_VERSION: u16 = 1;

/// Network and Block 0 identities repeated in every Trident header commitment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentHeaderIdentity {
    pub protocol_version: u32,
    pub state_transition_version: String,
    pub block_zero_commitment: Hash,
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
}

/// Header fields required to identify a Trident BlockDAG vertex and its state.
///
/// Use [`Self::canonical_bytes`] for encoding. This type intentionally omits
/// serde and `ts-rs`: no current RPC, client, storage, or P2P path consumes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentHeader {
    pub identity: TridentHeaderIdentity,
    pub parents: Vec<Hash>,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub nonce: u64,
    pub body_root: Hash,
    pub state_root: Hash,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TridentHeaderError {
    #[error("malformed Trident header Borsh: {0}")]
    MalformedEncoding(String),
    #[error("Trident header encoding domain mismatch")]
    EncodingDomainMismatch,
    #[error("unsupported Trident header encoding version {0}")]
    UnsupportedEncodingVersion(u16),
    #[error("Trident header protocol version must be nonzero")]
    ZeroProtocolVersion,
    #[error("Trident header state-transition version must be nonempty")]
    EmptyStateTransitionVersion,
    #[error("Trident header Block 0 commitment must be nonzero")]
    ZeroBlockZeroCommitment,
    #[error("Trident header artifact identity must be nonzero")]
    ZeroArtifactIdentity,
    #[error("Trident header consensus policy hash must be nonzero")]
    ZeroConsensusPolicyHash,
    #[error("Trident header body root must be nonzero")]
    ZeroBodyRoot,
    #[error("Trident header state root must be nonzero")]
    ZeroStateRoot,
    #[error("Trident header body root mismatch")]
    BodyRootMismatch,
    #[error("Trident header state root mismatch")]
    StateRootMismatch,
    #[error("Trident header Block 0 commitment mismatch")]
    BlockZeroCommitmentMismatch,
    #[error("Trident header artifact identity mismatch")]
    ArtifactIdentityMismatch,
    #[error("Trident header consensus policy hash mismatch")]
    ConsensusPolicyHashMismatch,
    #[error("unsupported Trident protocol version {actual}; expected protocol version {expected}")]
    UnsupportedProtocolVersion { expected: u32, actual: u32 },
    #[error(
        "unsupported Trident state-transition version {actual}; expected state-transition version {expected}"
    )]
    UnsupportedStateTransitionVersion { expected: String, actual: String },
}

#[derive(BorshSerialize, BorshDeserialize)]
struct TridentHeaderEnvelope {
    domain: [u8; 32],
    version: u16,
    payload: Vec<u8>,
}

#[derive(BorshSerialize)]
struct TridentHeaderPayloadV1Ref<'a> {
    protocol_version: u32,
    state_transition_version: &'a str,
    block_zero_commitment: Hash,
    artifact_identity: Hash,
    consensus_policy_hash: Hash,
    parents: &'a [Hash],
    timestamp_ms: u64,
    bits: u32,
    nonce: u64,
    body_root: Hash,
    state_root: Hash,
}

#[derive(BorshDeserialize)]
struct TridentHeaderPayloadV1 {
    protocol_version: u32,
    state_transition_version: String,
    block_zero_commitment: Hash,
    artifact_identity: Hash,
    consensus_policy_hash: Hash,
    parents: Vec<Hash>,
    timestamp_ms: u64,
    bits: u32,
    nonce: u64,
    body_root: Hash,
    state_root: Hash,
}

impl TridentHeaderIdentity {
    pub fn validate(&self) -> Result<(), TridentHeaderError> {
        if self.protocol_version == 0 {
            return Err(TridentHeaderError::ZeroProtocolVersion);
        }
        if self.state_transition_version.trim().is_empty() {
            return Err(TridentHeaderError::EmptyStateTransitionVersion);
        }
        if self.block_zero_commitment == Hash::ZERO {
            return Err(TridentHeaderError::ZeroBlockZeroCommitment);
        }
        if self.artifact_identity == Hash::ZERO {
            return Err(TridentHeaderError::ZeroArtifactIdentity);
        }
        if self.consensus_policy_hash == Hash::ZERO {
            return Err(TridentHeaderError::ZeroConsensusPolicyHash);
        }
        Ok(())
    }
}

impl TridentHeader {
    pub fn new(
        identity: TridentHeaderIdentity,
        parents: Vec<Hash>,
        timestamp_ms: u64,
        bits: u32,
        nonce: u64,
        body_root: Hash,
        state_root: Hash,
    ) -> Result<Self, TridentHeaderError> {
        let header = Self {
            identity,
            parents,
            timestamp_ms,
            bits,
            nonce,
            body_root,
            state_root,
        };
        header.validate()?;
        Ok(header)
    }

    pub fn validate(&self) -> Result<(), TridentHeaderError> {
        self.identity.validate()?;
        if self.body_root == Hash::ZERO {
            return Err(TridentHeaderError::ZeroBodyRoot);
        }
        if self.state_root == Hash::ZERO {
            return Err(TridentHeaderError::ZeroStateRoot);
        }
        Ok(())
    }

    /// Verify roots and network identities recomputed by the offline caller.
    pub fn verify_against(
        &self,
        expected_identity: &TridentHeaderIdentity,
        expected_body_root: Hash,
        expected_state_root: Hash,
    ) -> Result<(), TridentHeaderError> {
        self.validate()?;
        expected_identity.validate()?;
        if expected_body_root == Hash::ZERO {
            return Err(TridentHeaderError::ZeroBodyRoot);
        }
        if expected_state_root == Hash::ZERO {
            return Err(TridentHeaderError::ZeroStateRoot);
        }
        if self.body_root != expected_body_root {
            return Err(TridentHeaderError::BodyRootMismatch);
        }
        if self.state_root != expected_state_root {
            return Err(TridentHeaderError::StateRootMismatch);
        }
        if self.identity.block_zero_commitment != expected_identity.block_zero_commitment {
            return Err(TridentHeaderError::BlockZeroCommitmentMismatch);
        }
        if self.identity.artifact_identity != expected_identity.artifact_identity {
            return Err(TridentHeaderError::ArtifactIdentityMismatch);
        }
        if self.identity.consensus_policy_hash != expected_identity.consensus_policy_hash {
            return Err(TridentHeaderError::ConsensusPolicyHashMismatch);
        }
        if self.identity.protocol_version != expected_identity.protocol_version {
            return Err(TridentHeaderError::UnsupportedProtocolVersion {
                expected: expected_identity.protocol_version,
                actual: self.identity.protocol_version,
            });
        }
        if self.identity.state_transition_version != expected_identity.state_transition_version {
            return Err(TridentHeaderError::UnsupportedStateTransitionVersion {
                expected: expected_identity.state_transition_version.clone(),
                actual: self.identity.state_transition_version.clone(),
            });
        }
        Ok(())
    }

    /// Canonical, domain-separated Borsh envelope.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, TridentHeaderError> {
        self.validate()?;
        let payload = TridentHeaderPayloadV1Ref {
            protocol_version: self.identity.protocol_version,
            state_transition_version: &self.identity.state_transition_version,
            block_zero_commitment: self.identity.block_zero_commitment,
            artifact_identity: self.identity.artifact_identity,
            consensus_policy_hash: self.identity.consensus_policy_hash,
            parents: &self.parents,
            timestamp_ms: self.timestamp_ms,
            bits: self.bits,
            nonce: self.nonce,
            body_root: self.body_root,
            state_root: self.state_root,
        };
        let payload = borsh::to_vec(&payload)
            .map_err(|error| TridentHeaderError::MalformedEncoding(error.to_string()))?;
        borsh::to_vec(&TridentHeaderEnvelope {
            domain: TRIDENT_HEADER_ENCODING_DOMAIN,
            version: TRIDENT_HEADER_ENCODING_VERSION,
            payload,
        })
        .map_err(|error| TridentHeaderError::MalformedEncoding(error.to_string()))
    }

    /// Decode only the explicitly supported envelope version and validate it.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, TridentHeaderError> {
        let envelope = TridentHeaderEnvelope::try_from_slice(bytes)
            .map_err(|error| TridentHeaderError::MalformedEncoding(error.to_string()))?;
        if envelope.domain != TRIDENT_HEADER_ENCODING_DOMAIN {
            return Err(TridentHeaderError::EncodingDomainMismatch);
        }
        if envelope.version != TRIDENT_HEADER_ENCODING_VERSION {
            return Err(TridentHeaderError::UnsupportedEncodingVersion(
                envelope.version,
            ));
        }
        let payload = TridentHeaderPayloadV1::try_from_slice(&envelope.payload)
            .map_err(|error| TridentHeaderError::MalformedEncoding(error.to_string()))?;
        Self::new(
            TridentHeaderIdentity {
                protocol_version: payload.protocol_version,
                state_transition_version: payload.state_transition_version,
                block_zero_commitment: payload.block_zero_commitment,
                artifact_identity: payload.artifact_identity,
                consensus_policy_hash: payload.consensus_policy_hash,
            },
            payload.parents,
            payload.timestamp_ms,
            payload.bits,
            payload.nonce,
            payload.body_root,
            payload.state_root,
        )
    }

    /// SHA-256 of the complete domain/version envelope.
    pub fn commitment_hash(&self) -> Result<Hash, TridentHeaderError> {
        Ok(Hash::hash_bytes(&self.canonical_bytes()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENCODING_VECTOR: &str = concat!(
        "61676f72612d74726964656e742d6865616465722d656e76656c6f70652d763101001601",
        "0000040000001600000061676f72612d74726964656e742d73746174652d763611111111",
        "111111111111111111111111111111111111111111111111111111112222222222222222",
        "222222222222222222222222222222222222222222222222333333333333333333333333",
        "333333333333333333333333333333333333333302000000444444444444444444444444",
        "444444444444444444444444444444444444444455555555555555555555555555555555",
        "555555555555555555555555555555550807060504030201443322111817161514131211",
        "666666666666666666666666666666666666666666666666666666666666666677777777",
        "77777777777777777777777777777777777777777777777777777777"
    );
    const COMMITMENT_VECTOR: &str =
        "daeb33929a5050ed9039519010bee7dda8a953416f460a3940cd8176e6d5ca3a";

    fn sample_header() -> TridentHeader {
        TridentHeader::new(
            TridentHeaderIdentity {
                protocol_version: 4,
                state_transition_version: "agora-trident-state-v6".into(),
                block_zero_commitment: Hash([0x11; 32]),
                artifact_identity: Hash([0x22; 32]),
                consensus_policy_hash: Hash([0x33; 32]),
            },
            vec![Hash([0x44; 32]), Hash([0x55; 32])],
            0x0102_0304_0506_0708,
            0x1122_3344,
            0x1112_1314_1516_1718,
            Hash([0x66; 32]),
            Hash([0x77; 32]),
        )
        .unwrap()
    }

    #[test]
    fn canonical_roundtrip_matches_stable_vector() {
        let header = sample_header();
        let bytes = header.canonical_bytes().unwrap();
        assert_eq!(hex::encode(&bytes), ENCODING_VECTOR);
        assert_eq!(
            header.commitment_hash().unwrap().to_hex(),
            COMMITMENT_VECTOR
        );
        assert_eq!(TridentHeader::from_canonical_bytes(&bytes).unwrap(), header);
    }

    #[test]
    fn every_committed_field_changes_the_hash() {
        let baseline = sample_header();
        let baseline_hash = baseline.commitment_hash().unwrap();
        let mut mutations = Vec::new();

        let mut changed = baseline.clone();
        changed.identity.protocol_version += 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.identity.state_transition_version.push_str("-other");
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.identity.block_zero_commitment = Hash([0x12; 32]);
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.identity.artifact_identity = Hash([0x23; 32]);
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.identity.consensus_policy_hash = Hash([0x34; 32]);
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.parents[0] = Hash([0x45; 32]);
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.timestamp_ms += 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.bits += 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.nonce += 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.body_root = Hash([0x67; 32]);
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.state_root = Hash([0x78; 32]);
        mutations.push(changed);

        for changed in mutations {
            assert_ne!(changed.commitment_hash().unwrap(), baseline_hash);
        }
    }

    #[test]
    fn unknown_version_wrong_domain_and_malformed_bytes_are_rejected() {
        let bytes = sample_header().canonical_bytes().unwrap();

        let mut unknown = bytes.clone();
        unknown[TRIDENT_HEADER_ENCODING_DOMAIN.len()..][..2].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(
            TridentHeader::from_canonical_bytes(&unknown),
            Err(TridentHeaderError::UnsupportedEncodingVersion(2))
        );

        let mut wrong_domain = bytes.clone();
        wrong_domain[0] ^= 1;
        assert_eq!(
            TridentHeader::from_canonical_bytes(&wrong_domain),
            Err(TridentHeaderError::EncodingDomainMismatch)
        );

        assert!(matches!(
            TridentHeader::from_canonical_bytes(&bytes[..bytes.len() - 1]),
            Err(TridentHeaderError::MalformedEncoding(_))
        ));
    }

    #[test]
    fn zero_and_mismatched_commitments_are_rejected() {
        let baseline = sample_header();
        let identity = baseline.identity.clone();

        for error in [
            {
                let mut changed = baseline.clone();
                changed.body_root = Hash::ZERO;
                changed.validate().unwrap_err()
            },
            {
                let mut changed = baseline.clone();
                changed.state_root = Hash::ZERO;
                changed.validate().unwrap_err()
            },
            {
                let mut changed = baseline.clone();
                changed.identity.consensus_policy_hash = Hash::ZERO;
                changed.validate().unwrap_err()
            },
            {
                let mut changed = baseline.clone();
                changed.identity.block_zero_commitment = Hash::ZERO;
                changed.validate().unwrap_err()
            },
            {
                let mut changed = baseline.clone();
                changed.identity.artifact_identity = Hash::ZERO;
                changed.validate().unwrap_err()
            },
            {
                let mut changed = baseline.clone();
                changed.identity.protocol_version = 0;
                changed.validate().unwrap_err()
            },
            {
                let mut changed = baseline.clone();
                changed.identity.state_transition_version.clear();
                changed.validate().unwrap_err()
            },
        ] {
            assert!(matches!(
                error,
                TridentHeaderError::ZeroBodyRoot
                    | TridentHeaderError::ZeroStateRoot
                    | TridentHeaderError::ZeroConsensusPolicyHash
                    | TridentHeaderError::ZeroBlockZeroCommitment
                    | TridentHeaderError::ZeroArtifactIdentity
                    | TridentHeaderError::ZeroProtocolVersion
                    | TridentHeaderError::EmptyStateTransitionVersion
            ));
        }

        assert_eq!(
            baseline.verify_against(&identity, Hash([0x68; 32]), baseline.state_root),
            Err(TridentHeaderError::BodyRootMismatch)
        );
        assert_eq!(
            baseline.verify_against(&identity, baseline.body_root, Hash([0x79; 32])),
            Err(TridentHeaderError::StateRootMismatch)
        );

        let mut wrong_identity = identity.clone();
        wrong_identity.consensus_policy_hash = Hash([0x35; 32]);
        assert_eq!(
            baseline.verify_against(&wrong_identity, baseline.body_root, baseline.state_root),
            Err(TridentHeaderError::ConsensusPolicyHashMismatch)
        );

        wrong_identity = identity.clone();
        wrong_identity.block_zero_commitment = Hash([0x13; 32]);
        assert_eq!(
            baseline.verify_against(&wrong_identity, baseline.body_root, baseline.state_root),
            Err(TridentHeaderError::BlockZeroCommitmentMismatch)
        );
        wrong_identity = identity.clone();
        wrong_identity.artifact_identity = Hash([0x24; 32]);
        assert_eq!(
            baseline.verify_against(&wrong_identity, baseline.body_root, baseline.state_root),
            Err(TridentHeaderError::ArtifactIdentityMismatch)
        );
        wrong_identity = identity.clone();
        wrong_identity.protocol_version += 1;
        assert!(matches!(
            baseline.verify_against(&wrong_identity, baseline.body_root, baseline.state_root),
            Err(TridentHeaderError::UnsupportedProtocolVersion { .. })
        ));
        wrong_identity = identity;
        wrong_identity.state_transition_version.push_str("-other");
        assert!(matches!(
            baseline.verify_against(&wrong_identity, baseline.body_root, baseline.state_root),
            Err(TridentHeaderError::UnsupportedStateTransitionVersion { .. })
        ));
    }
}
