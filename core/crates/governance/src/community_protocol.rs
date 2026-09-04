//! Pure protocol records for community hubs, grants, and missions.

use agora_types::{Address, Amount, Hash, TreasuryId};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum HubKind {
    Geographic,
    Specialist,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum HubAccreditationStatus {
    Pending,
    Active,
    Suspended,
    Revoked,
}

#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct HubRecord {
    pub id: Hash,
    pub public_name: String,
    pub classification: String,
    pub charter_hash: Hash,
    pub coordinators: Vec<Address>,
    pub accreditation_proposal_id: u64,
    pub status: HubAccreditationStatus,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum GrantKind {
    Micro,
    Milestone,
    Bounty,
    Retroactive,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum GrantStatus {
    Approved,
    Active,
    Completed,
    Cancelled,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum MilestoneStatus {
    Pending,
    Accepted,
}

#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct GrantMilestone {
    pub index: u32,
    pub amount: Amount,
    pub deliverable_hash: Hash,
    pub status: MilestoneStatus,
}

#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct GrantRecord {
    pub id: Hash,
    pub proposal_id: u64,
    pub treasury: TreasuryId,
    pub beneficiary: Address,
    pub total: Amount,
    pub released: Amount,
    pub kind: GrantKind,
    pub status: GrantStatus,
    pub milestones: Vec<GrantMilestone>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum CommunityProtocolError {
    #[error("amount must be nonzero")]
    ZeroAmount,
    #[error("milestone indexes must be strictly ordered and unique")]
    InvalidMilestoneOrder,
    #[error("milestone amount sum must equal grant total")]
    MilestoneTotalMismatch,
    #[error("record is not in the required lifecycle state")]
    InvalidTransition,
    #[error("milestone is not the next pending milestone")]
    MilestoneOutOfOrder,
    #[error("milestone deliverable evidence does not match")]
    EvidenceMismatch,
    #[error("amount arithmetic overflow or grant release cap exceeded")]
    AmountOverflow,
    #[error("completion evidence must be nonzero")]
    MissingEvidence,
}

impl GrantRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Hash,
        proposal_id: u64,
        treasury: TreasuryId,
        beneficiary: Address,
        total: Amount,
        kind: GrantKind,
        status: GrantStatus,
        milestones: Vec<GrantMilestone>,
    ) -> Result<Self, CommunityProtocolError> {
        if total == Amount::ZERO {
            return Err(CommunityProtocolError::ZeroAmount);
        }

        if kind == GrantKind::Milestone {
            if milestones
                .windows(2)
                .any(|window| window[0].index >= window[1].index)
            {
                return Err(CommunityProtocolError::InvalidMilestoneOrder);
            }
            let milestone_total = milestones.iter().try_fold(Amount::ZERO, |sum, milestone| {
                sum.checked_add(milestone.amount)
            });
            if milestone_total != Some(total) {
                return Err(CommunityProtocolError::MilestoneTotalMismatch);
            }
        }

        Ok(Self {
            id,
            proposal_id,
            treasury,
            beneficiary,
            total,
            released: Amount::ZERO,
            kind,
            status,
            milestones,
        })
    }

    /// Records protocol acceptance only; treasury movement belongs to execution.
    pub fn accept_milestone(
        &mut self,
        index: u32,
        evidence_hash: Hash,
    ) -> Result<(), CommunityProtocolError> {
        if self.status != GrantStatus::Active {
            return Err(CommunityProtocolError::InvalidTransition);
        }
        let milestone = self
            .milestones
            .iter_mut()
            .find(|milestone| milestone.status == MilestoneStatus::Pending)
            .ok_or(CommunityProtocolError::MilestoneOutOfOrder)?;
        if milestone.index != index {
            return Err(CommunityProtocolError::MilestoneOutOfOrder);
        }
        if milestone.deliverable_hash != evidence_hash {
            return Err(CommunityProtocolError::EvidenceMismatch);
        }

        let released = self
            .released
            .checked_add(milestone.amount)
            .ok_or(CommunityProtocolError::AmountOverflow)?;
        if released > self.total {
            return Err(CommunityProtocolError::AmountOverflow);
        }

        milestone.status = MilestoneStatus::Accepted;
        self.released = released;
        if self.released == self.total {
            self.status = GrantStatus::Completed;
        }
        Ok(())
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum MissionStatus {
    Open,
    Assigned,
    Completed,
    Cancelled,
}

#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct MissionRecord {
    pub id: Hash,
    pub sponsor: Address,
    pub reward_treasury: TreasuryId,
    pub reward: Amount,
    pub requirements_hash: Hash,
    pub assignee: Option<Address>,
    pub status: MissionStatus,
    pub completion_evidence: Hash,
}

impl MissionRecord {
    pub fn assign(&mut self, assignee: Address) -> Result<(), CommunityProtocolError> {
        if self.status != MissionStatus::Open || self.assignee.is_some() {
            return Err(CommunityProtocolError::InvalidTransition);
        }
        self.assignee = Some(assignee);
        self.status = MissionStatus::Assigned;
        Ok(())
    }

    /// Records completion only; reward payment belongs to treasury execution.
    pub fn complete(&mut self, evidence_hash: Hash) -> Result<(), CommunityProtocolError> {
        if self.status != MissionStatus::Assigned || self.assignee.is_none() {
            return Err(CommunityProtocolError::InvalidTransition);
        }
        if evidence_hash == Hash::ZERO {
            return Err(CommunityProtocolError::MissingEvidence);
        }
        self.completion_evidence = evidence_hash;
        self.status = MissionStatus::Completed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn milestone(index: u32, amount: u64, evidence: u8) -> GrantMilestone {
        GrantMilestone {
            index,
            amount: Amount::from_base_units(amount),
            deliverable_hash: Hash([evidence; 32]),
            status: MilestoneStatus::Pending,
        }
    }

    fn milestone_grant() -> GrantRecord {
        GrantRecord::new(
            Hash([1; 32]),
            2,
            TreasuryId::OvlBuilder,
            Address([3; 20]),
            Amount::from_base_units(100),
            GrantKind::Milestone,
            GrantStatus::Active,
            vec![milestone(0, 40, 4), milestone(1, 60, 5)],
        )
        .unwrap()
    }

    #[test]
    fn milestone_grant_rejects_skip_double_acceptance_and_cap_breach() {
        let mut grant = milestone_grant();
        assert_eq!(
            grant.accept_milestone(1, Hash([5; 32])),
            Err(CommunityProtocolError::MilestoneOutOfOrder)
        );
        grant.accept_milestone(0, Hash([4; 32])).unwrap();
        assert_eq!(grant.released, Amount::from_base_units(40));
        assert_eq!(
            grant.accept_milestone(0, Hash([4; 32])),
            Err(CommunityProtocolError::MilestoneOutOfOrder)
        );

        grant.released = Amount::from_base_units(90);
        assert_eq!(
            grant.accept_milestone(1, Hash([5; 32])),
            Err(CommunityProtocolError::AmountOverflow)
        );
        assert_eq!(grant.milestones[1].status, MilestoneStatus::Pending);
    }

    #[test]
    fn milestone_grant_validates_shape_and_completes_in_order() {
        let mut grant = milestone_grant();
        grant.accept_milestone(0, Hash([4; 32])).unwrap();
        grant.accept_milestone(1, Hash([5; 32])).unwrap();
        assert_eq!(grant.released, grant.total);
        assert_eq!(grant.status, GrantStatus::Completed);

        let error = GrantRecord::new(
            Hash::ZERO,
            0,
            TreasuryId::OvlBuilder,
            Address::ZERO,
            Amount::from_base_units(100),
            GrantKind::Milestone,
            GrantStatus::Approved,
            vec![milestone(1, 50, 1), milestone(1, 50, 2)],
        )
        .unwrap_err();
        assert_eq!(error, CommunityProtocolError::InvalidMilestoneOrder);
    }

    #[test]
    fn mission_enforces_assignment_and_completion_transitions() {
        let mut mission = MissionRecord {
            id: Hash([1; 32]),
            sponsor: Address([2; 20]),
            reward_treasury: TreasuryId::DrcCommunity,
            reward: Amount::from_base_units(10),
            requirements_hash: Hash([3; 32]),
            assignee: None,
            status: MissionStatus::Open,
            completion_evidence: Hash::ZERO,
        };
        assert_eq!(
            mission.complete(Hash([4; 32])),
            Err(CommunityProtocolError::InvalidTransition)
        );
        mission.assign(Address([5; 20])).unwrap();
        assert_eq!(
            mission.complete(Hash::ZERO),
            Err(CommunityProtocolError::MissingEvidence)
        );
        mission.complete(Hash([4; 32])).unwrap();
        assert_eq!(mission.status, MissionStatus::Completed);
        assert!(mission.assign(Address([6; 20])).is_err());
    }

    #[test]
    fn hub_record_borsh_roundtrip_preserves_schema() {
        assert_eq!(HubKind::Geographic, HubKind::Geographic);
        let hub = HubRecord {
            id: Hash([1; 32]),
            public_name: "Agora Athens".into(),
            classification: "Geographic".into(),
            charter_hash: Hash([2; 32]),
            coordinators: vec![Address([3; 20])],
            accreditation_proposal_id: 4,
            status: HubAccreditationStatus::Pending,
        };
        let bytes = borsh::to_vec(&hub).unwrap();
        assert_eq!(HubRecord::try_from_slice(&bytes).unwrap(), hub);
    }
}
