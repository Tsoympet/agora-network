//! Signed Hub, Grant, and Mission registration envelopes.
//!
//! Records live in `agora-governance`. These types are the secp256k1 wire
//! envelopes so `agora-types` does not depend on governance.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Address, Amount, Hash, TreasuryId};

/// Domain separator for network-bound hub registrations.
pub const HUB_REGISTRATION_DOMAIN: &[u8] = b"agora-hub-registration-v1";
/// Domain separator for network-bound grant registrations.
pub const GRANT_REGISTRATION_DOMAIN: &[u8] = b"agora-grant-registration-v1";
/// Domain separator for network-bound mission registrations.
pub const MISSION_REGISTRATION_DOMAIN: &[u8] = b"agora-mission-registration-v1";

/// Wire grant kind. Borsh order matches `agora_governance::GrantKind`.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub enum CommunityGrantKind {
    Micro,
    Milestone,
    Bounty,
    Retroactive,
}

/// Milestone shape committed at grant registration (always pending on apply).
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct CommunityGrantMilestone {
    pub index: u32,
    pub amount: Amount,
    pub deliverable_hash: Hash,
}

/// First-coordinator-signed hub accreditation.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct HubRegistration {
    pub version: u32,
    pub public_name: String,
    pub classification: String,
    pub charter_hash: Hash,
    pub coordinators: Vec<Address>,
    pub treasury_multisig: Address,
    pub election_term_epochs: u64,
    pub reporting_interval_epochs: u64,
    pub coi_disclosure_root: Hash,
    pub deliverables_root: Hash,
    pub accreditation_proposal_id: u64,
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl HubRegistration {
    #[allow(clippy::too_many_arguments)]
    pub fn unsigned(
        public_name: String,
        classification: String,
        charter_hash: Hash,
        coordinators: Vec<Address>,
        treasury_multisig: Address,
        election_term_epochs: u64,
        reporting_interval_epochs: u64,
        coi_disclosure_root: Hash,
        deliverables_root: Hash,
        accreditation_proposal_id: u64,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            public_name,
            classification,
            charter_hash,
            coordinators,
            treasury_multisig,
            election_term_epochs,
            reporting_interval_epochs,
            coi_disclosure_root,
            deliverables_root,
            accreditation_proposal_id,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn first_coordinator(&self) -> Option<Address> {
        self.coordinators.first().copied()
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let body = (
            HUB_REGISTRATION_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            &self.public_name,
            &self.classification,
            self.charter_hash,
            &self.coordinators,
            self.treasury_multisig,
            self.election_term_epochs,
            self.reporting_interval_epochs,
            self.coi_disclosure_root,
            self.deliverables_root,
            self.accreditation_proposal_id,
            self.nonce,
        );
        borsh::to_vec(&body).expect("borsh serialize hub registration body")
    }

    pub fn registration_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

/// Active-hub-coordinator-signed grant registration.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct GrantRegistration {
    pub version: u32,
    pub registrar: Address,
    pub proposal_id: u64,
    pub treasury: TreasuryId,
    pub beneficiary: Address,
    pub total: Amount,
    pub kind: CommunityGrantKind,
    pub milestones: Vec<CommunityGrantMilestone>,
    pub coi_disclosure_hash: Hash,
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl GrantRegistration {
    #[allow(clippy::too_many_arguments)]
    pub fn unsigned(
        registrar: Address,
        proposal_id: u64,
        treasury: TreasuryId,
        beneficiary: Address,
        total: Amount,
        kind: CommunityGrantKind,
        milestones: Vec<CommunityGrantMilestone>,
        coi_disclosure_hash: Hash,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            registrar,
            proposal_id,
            treasury,
            beneficiary,
            total,
            kind,
            milestones,
            coi_disclosure_hash,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let body = (
            GRANT_REGISTRATION_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.registrar,
            self.proposal_id,
            self.treasury,
            self.beneficiary,
            self.total,
            self.kind,
            &self.milestones,
            self.coi_disclosure_hash,
            self.nonce,
        );
        borsh::to_vec(&body).expect("borsh serialize grant registration body")
    }

    pub fn registration_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

/// Sponsor-signed mission registration. Sponsor must be an active hub coordinator.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct MissionRegistration {
    pub version: u32,
    pub sponsor: Address,
    pub reward_treasury: TreasuryId,
    pub reward: Amount,
    pub requirements_hash: Hash,
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl MissionRegistration {
    pub fn unsigned(
        sponsor: Address,
        reward_treasury: TreasuryId,
        reward: Amount,
        requirements_hash: Hash,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            sponsor,
            reward_treasury,
            reward,
            requirements_hash,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let body = (
            MISSION_REGISTRATION_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.sponsor,
            self.reward_treasury,
            self.reward,
            self.requirements_hash,
            self.nonce,
        );
        borsh::to_vec(&body).expect("borsh serialize mission registration body")
    }

    pub fn registration_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_grant_mission_ids_cover_signatures() {
        let mut hub = HubRegistration::unsigned(
            "Agora Athens".into(),
            "Geographic".into(),
            Hash([2; 32]),
            vec![Address([3; 20])],
            Address([4; 20]),
            12,
            3,
            Hash([5; 32]),
            Hash([6; 32]),
            1,
            0,
        );
        let unsigned_hub = hub.registration_id();
        hub.signature = vec![8; 64];
        assert_ne!(hub.registration_id(), unsigned_hub);
        assert_ne!(
            hub.signing_bytes_bound("agora-dev", &Hash::ZERO),
            hub.signing_bytes_bound("agora-testnet", &Hash::ZERO)
        );

        let mut grant = GrantRegistration::unsigned(
            Address([1; 20]),
            7,
            TreasuryId::OvlBuilder,
            Address([2; 20]),
            Amount::from_base_units(10),
            CommunityGrantKind::Micro,
            vec![],
            Hash::ZERO,
            0,
        );
        let unsigned_grant = grant.registration_id();
        grant.signature = vec![9; 64];
        assert_ne!(grant.registration_id(), unsigned_grant);

        let mut mission = MissionRegistration::unsigned(
            Address([1; 20]),
            TreasuryId::DrcCommunity,
            Amount::from_base_units(5),
            Hash([3; 32]),
            0,
        );
        let unsigned_mission = mission.registration_id();
        mission.signature = vec![10; 64];
        assert_ne!(mission.registration_id(), unsigned_mission);
    }
}
