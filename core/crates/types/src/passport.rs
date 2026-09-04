//! Non-transferable, issuer-authenticated community contribution attestations.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Address, Hash};

/// Domain separator for network-bound passport attestations.
pub const PASSPORT_ATTESTATION_DOMAIN: &[u8] = b"agora-passport-attestation-v1";

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub enum PassportCategory {
    Code,
    Documentation,
    Translation,
    SecurityReport,
    Infrastructure,
    EventHosting,
    Teaching,
    MerchantOnboarding,
    GrantReview,
    GovernanceParticipation,
    Moderation,
    CommunitySupport,
}

/// A contribution claim whose issuer and subject are covered by its signature.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct PassportAttestation {
    pub version: u32,
    pub issuer: Address,
    pub subject: Address,
    pub category: PassportCategory,
    pub evidence_hash: Hash,
    pub issuer_policy_hash: Hash,
    pub issued_epoch: u64,
    pub expires_epoch: Option<u64>,
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl PassportAttestation {
    #[allow(clippy::too_many_arguments)]
    pub fn unsigned(
        issuer: Address,
        subject: Address,
        category: PassportCategory,
        evidence_hash: Hash,
        issuer_policy_hash: Hash,
        issued_epoch: u64,
        expires_epoch: Option<u64>,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            issuer,
            subject,
            category,
            evidence_hash,
            issuer_policy_hash,
            issued_epoch,
            expires_epoch,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    /// Binds every unsigned field to one chain and genesis.
    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let body = (
            PASSPORT_ATTESTATION_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.issuer,
            self.subject,
            self.category,
            self.evidence_hash,
            self.issuer_policy_hash,
            self.issued_epoch,
            self.expires_epoch,
            self.nonce,
        );
        borsh::to_vec(&body).expect("borsh serialize passport attestation body")
    }

    /// Hashes the complete signed attestation, including issuer authorization.
    pub fn attestation_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passport_attestation_roundtrips_and_id_covers_signature() {
        let mut attestation = PassportAttestation::unsigned(
            Address([1; 20]),
            Address([2; 20]),
            PassportCategory::Code,
            Hash([3; 32]),
            Hash([4; 32]),
            5,
            Some(6),
            7,
        );
        let bytes = borsh::to_vec(&attestation).unwrap();
        assert_eq!(
            PassportAttestation::try_from_slice(&bytes).unwrap(),
            attestation
        );

        let unsigned_id = attestation.attestation_id();
        attestation.signature = vec![8; 64];
        assert_ne!(attestation.attestation_id(), unsigned_id);
        assert_ne!(
            attestation.signing_bytes_bound("agora-dev", &Hash::ZERO),
            attestation.signing_bytes_bound("agora-testnet", &Hash::ZERO)
        );
    }
}
