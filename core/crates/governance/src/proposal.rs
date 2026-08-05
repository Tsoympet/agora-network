//! Proposal kinds, ballots, and tallies.

use agora_types::Address;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::ranks::CivicRank;

/// How far a parameter change may go without Ecclesia.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ParameterScope {
    /// Boule may decide (operational tweaks).
    Minor,
    /// Ecclesia must decide (consensus-affecting).
    Major,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposalKind {
    TextSignal,
    ParameterChange {
        scope: ParameterScope,
        key: String,
        value: String,
    },
    TreasurySpend {
        amount: u64,
        beneficiary_note: String,
    },
    SoftwareUpgrade {
        version: String,
        manifest_hash_hex: String,
    },
    RankElection {
        rank: CivicRank,
        seat_index: u16,
        candidate: Address,
    },
    RankImpeachment {
        rank: CivicRank,
        seat_index: u16,
    },
    ConstitutionAmendment {
        new_constitution_id: String,
        new_body_markdown: String,
    },
    EmergencyAction {
        reason: String,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Draft,
    Deposit,
    Voting,
    Passed,
    Rejected,
    FailedQuorum,
    Vetoed,
    Timelock,
    Executed,
    Expired,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum VoteChoice {
    Yes,
    No,
    Abstain,
    /// Cosmos-style veto; if veto power ≥ threshold, proposal fails as Vetoed.
    NoWithVeto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ballot {
    pub voter: Address,
    pub choice: VoteChoice,
    /// Effective weight counted (quadratic power or 1 for seat votes).
    pub weight: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteTally {
    pub yes: u64,
    pub no: u64,
    pub abstain: u64,
    pub no_with_veto: u64,
}

impl VoteTally {
    pub fn add(&mut self, choice: VoteChoice, weight: u64) -> Result<(), crate::GovernanceError> {
        let bucket = match choice {
            VoteChoice::Yes => &mut self.yes,
            VoteChoice::No => &mut self.no,
            VoteChoice::Abstain => &mut self.abstain,
            VoteChoice::NoWithVeto => &mut self.no_with_veto,
        };
        *bucket = bucket
            .checked_add(weight)
            .ok_or(crate::GovernanceError::Overflow)?;
        Ok(())
    }

    pub fn total_voted(&self) -> Result<u64, crate::GovernanceError> {
        self.yes
            .checked_add(self.no)
            .and_then(|v| v.checked_add(self.abstain))
            .and_then(|v| v.checked_add(self.no_with_veto))
            .ok_or(crate::GovernanceError::Overflow)
    }

    /// Yes / (Yes + No + NoWithVeto) in basis points; Abstain excluded from denominator.
    pub fn yes_ratio_bps(&self) -> u64 {
        let decided = self
            .yes
            .saturating_add(self.no)
            .saturating_add(self.no_with_veto);
        if decided == 0 {
            return 0;
        }
        self.yes.saturating_mul(10_000) / decided
    }

    pub fn veto_ratio_bps(&self) -> u64 {
        let total = self.total_voted().unwrap_or(0);
        if total == 0 {
            return 0;
        }
        self.no_with_veto.saturating_mul(10_000) / total
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: u64,
    pub title: String,
    pub summary: String,
    pub kind: ProposalKind,
    pub status: ProposalStatus,
    pub author: Address,
    pub deposit: u64,
    pub sponsors: Vec<Address>,
    pub created_slot: u64,
    pub voting_start_slot: Option<u64>,
    pub voting_end_slot: Option<u64>,
    pub timelock_end_slot: Option<u64>,
    pub tally: VoteTally,
    pub ballots: Vec<Ballot>,
    /// Archon addresses that recorded assent (amendments / emergencies).
    pub archon_assents: Vec<Address>,
}
