//! In-memory civic governance engine (normative rules before node wiring).

use std::collections::HashMap;

use agora_types::Address;
use serde::{Deserialize, Serialize};

use crate::chamber::{primary_chamber, VotingChamber};
use crate::constitution::EnactedConstitution;
use crate::office::OfficeBoard;
use crate::params::GovernanceParams;
use crate::proposal::{Ballot, Proposal, ProposalKind, ProposalStatus, VoteChoice, VoteTally};
use crate::quadratic::{effective_votes_for, VoterBalance};
use crate::ranks::CivicRank;
use crate::whale::WhaleCapConfig;
use crate::GovernanceError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceState {
    pub constitution: EnactedConstitution,
    pub params: GovernanceParams,
    pub whale_cap: WhaleCapConfig,
    pub offices: OfficeBoard,
    pub proposals: HashMap<u64, Proposal>,
    pub next_proposal_id: u64,
    /// Eligible Ecclesia power snapshot for the open vote (optional override).
    pub ecclesia_eligible_power: u64,
}

impl GovernanceState {
    pub fn genesis(eligible_power: u64) -> Self {
        let params = GovernanceParams::default();
        Self {
            constitution: EnactedConstitution::v1(),
            offices: OfficeBoard::with_defaults(params.boule_seats, params.tamias_seats),
            params,
            whale_cap: WhaleCapConfig::default(),
            proposals: HashMap::new(),
            next_proposal_id: 1,
            ecclesia_eligible_power: eligible_power,
        }
    }

    pub fn submit_proposal(
        &mut self,
        author: Address,
        title: impl Into<String>,
        summary: impl Into<String>,
        kind: ProposalKind,
        now_slot: u64,
    ) -> Result<u64, GovernanceError> {
        let id = self.next_proposal_id;
        self.next_proposal_id = self
            .next_proposal_id
            .checked_add(1)
            .ok_or(GovernanceError::Overflow)?;
        let proposal = Proposal {
            id,
            title: title.into(),
            summary: summary.into(),
            kind,
            status: ProposalStatus::Deposit,
            author,
            deposit: 0,
            sponsors: Vec::new(),
            created_slot: now_slot,
            voting_start_slot: None,
            voting_end_slot: None,
            timelock_end_slot: None,
            tally: VoteTally::default(),
            ballots: Vec::new(),
            archon_assents: Vec::new(),
        };
        self.proposals.insert(id, proposal);
        Ok(id)
    }

    pub fn add_deposit(&mut self, id: u64, amount: u64) -> Result<(), GovernanceError> {
        let p = self
            .proposals
            .get_mut(&id)
            .ok_or(GovernanceError::UnknownProposal)?;
        if p.status != ProposalStatus::Deposit {
            return Err(GovernanceError::NotAcceptingDeposit);
        }
        p.deposit = p
            .deposit
            .checked_add(amount)
            .ok_or(GovernanceError::Overflow)?;
        Ok(())
    }

    pub fn sponsor_as_tamias(&mut self, id: u64, who: Address) -> Result<(), GovernanceError> {
        if !self.offices.is_seated(CivicRank::Tamias, who) {
            return Err(GovernanceError::IneligibleVoter);
        }
        let p = self
            .proposals
            .get_mut(&id)
            .ok_or(GovernanceError::UnknownProposal)?;
        if !matches!(p.kind, ProposalKind::TreasurySpend { .. }) {
            return Err(GovernanceError::WrongChamber);
        }
        if !p.sponsors.contains(&who) {
            p.sponsors.push(who);
        }
        Ok(())
    }

    pub fn record_archon_assent(&mut self, id: u64, who: Address) -> Result<(), GovernanceError> {
        if !self.offices.is_any_archon(who) {
            return Err(GovernanceError::IneligibleVoter);
        }
        let p = self
            .proposals
            .get_mut(&id)
            .ok_or(GovernanceError::UnknownProposal)?;
        if !p.archon_assents.contains(&who) {
            p.archon_assents.push(who);
        }
        Ok(())
    }

    pub fn open_voting(&mut self, id: u64, now_slot: u64) -> Result<(), GovernanceError> {
        let min_deposit = self.params.min_deposit;
        let voting_period = self.params.voting_period_slots;
        let offices = &self.offices;
        let p = self
            .proposals
            .get_mut(&id)
            .ok_or(GovernanceError::UnknownProposal)?;
        if p.status != ProposalStatus::Deposit {
            return Err(GovernanceError::NotAcceptingDeposit);
        }
        if p.deposit < min_deposit {
            return Err(GovernanceError::InsufficientDeposit);
        }
        if matches!(p.kind, ProposalKind::TreasurySpend { .. }) && p.sponsors.is_empty() {
            return Err(GovernanceError::MissingSponsorship);
        }
        // Ensure chamber mapping is valid for current board (no-op check).
        let _ = primary_chamber(&p.kind);
        let _ = offices;
        p.status = ProposalStatus::Voting;
        p.voting_start_slot = Some(now_slot);
        p.voting_end_slot = Some(now_slot.saturating_add(voting_period));
        Ok(())
    }

    /// Cast a ballot. For Ecclesia, pass the voter's TLT balance + total supply.
    /// For Boule / ArchonCollegium, `raw_balance` / `total_supply` are ignored (weight = 1).
    pub fn cast_vote(
        &mut self,
        id: u64,
        voter: Address,
        choice: VoteChoice,
        raw_balance: u64,
        total_supply: u64,
    ) -> Result<(), GovernanceError> {
        let chamber = {
            let p = self
                .proposals
                .get(&id)
                .ok_or(GovernanceError::UnknownProposal)?;
            if p.status != ProposalStatus::Voting {
                return Err(GovernanceError::NotAcceptingVotes);
            }
            if p.ballots.iter().any(|b| b.voter == voter) {
                return Err(GovernanceError::DuplicateVote);
            }
            primary_chamber(&p.kind)
        };

        let weight = match chamber {
            VotingChamber::Ecclesia => {
                let (_capped, eff, _) =
                    effective_votes_for(raw_balance, total_supply, &self.whale_cap)?;
                if eff == 0 {
                    return Err(GovernanceError::IneligibleVoter);
                }
                eff
            }
            VotingChamber::Boule => {
                if !self.offices.is_seated(CivicRank::Bouleutes, voter)
                    && !self.offices.is_any_archon(voter)
                {
                    return Err(GovernanceError::IneligibleVoter);
                }
                1
            }
            VotingChamber::ArchonCollegium => {
                if !self.offices.is_any_archon(voter) {
                    return Err(GovernanceError::IneligibleVoter);
                }
                1
            }
        };

        let p = self
            .proposals
            .get_mut(&id)
            .ok_or(GovernanceError::UnknownProposal)?;
        p.tally.add(choice, weight)?;
        p.ballots.push(Ballot {
            voter,
            choice,
            weight,
        });
        Ok(())
    }

    pub fn tally(&mut self, id: u64) -> Result<ProposalStatus, GovernanceError> {
        let (kind, tally, chamber) = {
            let p = self
                .proposals
                .get(&id)
                .ok_or(GovernanceError::UnknownProposal)?;
            if p.status != ProposalStatus::Voting {
                return Err(GovernanceError::NotReadyToTally);
            }
            (p.kind.clone(), p.tally.clone(), primary_chamber(&p.kind))
        };

        let status = match chamber {
            VotingChamber::Ecclesia => self.tally_ecclesia(&kind, &tally)?,
            VotingChamber::Boule => {
                simple_majority(&tally, self.params.boule_pass_bps, /*quorum_voters*/ 0)?
            }
            VotingChamber::ArchonCollegium => {
                simple_majority(&tally, self.params.archon_pass_bps, 0)?
            }
        };

        let p = self
            .proposals
            .get_mut(&id)
            .ok_or(GovernanceError::UnknownProposal)?;
        p.status = status;
        Ok(status)
    }

    fn tally_ecclesia(
        &self,
        kind: &ProposalKind,
        tally: &VoteTally,
    ) -> Result<ProposalStatus, GovernanceError> {
        let voted = tally.total_voted()?;
        let eligible = self.ecclesia_eligible_power.max(1);
        let (quorum_bps, pass_bps) = if matches!(kind, ProposalKind::ConstitutionAmendment { .. }) {
            (
                self.params.amendment_quorum_bps,
                self.params.amendment_pass_bps,
            )
        } else {
            (
                self.params.ecclesia_quorum_bps,
                self.params.ecclesia_pass_bps,
            )
        };

        let turnout_bps = voted.saturating_mul(10_000) / eligible;
        if turnout_bps < quorum_bps {
            return Ok(ProposalStatus::FailedQuorum);
        }
        if tally.veto_ratio_bps() >= self.params.ecclesia_veto_bps {
            return Ok(ProposalStatus::Vetoed);
        }
        if tally.yes_ratio_bps() >= pass_bps {
            Ok(ProposalStatus::Passed)
        } else {
            Ok(ProposalStatus::Rejected)
        }
    }

    pub fn enter_timelock(&mut self, id: u64, now_slot: u64) -> Result<(), GovernanceError> {
        let timelock = self.params.timelock_slots;
        let p = self
            .proposals
            .get_mut(&id)
            .ok_or(GovernanceError::UnknownProposal)?;
        if p.status != ProposalStatus::Passed {
            return Err(GovernanceError::NotReadyToExecute);
        }
        // Constitution amendments need Archon assent before timelock/execute.
        if matches!(p.kind, ProposalKind::ConstitutionAmendment { .. }) {
            let basileus = self.offices.holders(CivicRank::ArchonBasileus);
            let basileus_ok = basileus
                .first()
                .map(|a| p.archon_assents.contains(a))
                .unwrap_or(false);
            let archon_count = p
                .archon_assents
                .iter()
                .filter(|a| self.offices.is_any_archon(**a))
                .count();
            if !basileus_ok && archon_count < 2 {
                return Err(GovernanceError::MissingArchonAssent);
            }
        }
        p.status = ProposalStatus::Timelock;
        p.timelock_end_slot = Some(now_slot.saturating_add(timelock));
        Ok(())
    }

    pub fn execute(&mut self, id: u64, now_slot: u64) -> Result<(), GovernanceError> {
        let (kind, timelock_end) = {
            let p = self
                .proposals
                .get(&id)
                .ok_or(GovernanceError::UnknownProposal)?;
            if p.status != ProposalStatus::Timelock {
                return Err(GovernanceError::NotReadyToExecute);
            }
            let end = p.timelock_end_slot.ok_or(GovernanceError::TimelockActive)?;
            if now_slot < end {
                return Err(GovernanceError::TimelockActive);
            }
            (p.kind.clone(), end)
        };
        let _ = timelock_end;

        match kind {
            ProposalKind::RankElection {
                rank,
                seat_index,
                candidate,
            } => {
                self.offices.seat_holder(
                    rank,
                    seat_index,
                    candidate,
                    now_slot,
                    self.params.term_slots,
                )?;
            }
            ProposalKind::RankImpeachment { rank, seat_index } => {
                let _ = self.offices.vacate(rank, seat_index)?;
            }
            ProposalKind::ConstitutionAmendment {
                new_constitution_id,
                new_body_markdown,
            } => {
                if new_constitution_id.is_empty() || new_body_markdown.is_empty() {
                    return Err(GovernanceError::InvalidConstitution);
                }
                self.constitution =
                    EnactedConstitution::from_body(new_constitution_id, new_body_markdown);
            }
            _ => {
                // Text / params / treasury / upgrade / emergency: recorded as executed;
                // side-effects are applied by the node runtime later.
            }
        }

        let p = self
            .proposals
            .get_mut(&id)
            .ok_or(GovernanceError::UnknownProposal)?;
        p.status = ProposalStatus::Executed;
        Ok(())
    }

    pub fn proposal(&self, id: u64) -> Option<&Proposal> {
        self.proposals.get(&id)
    }

    /// Helper: compute Ecclesia weight for a balance snapshot.
    pub fn ecclesia_weight(
        &self,
        voter: &VoterBalance,
        total_supply: u64,
    ) -> Result<u64, GovernanceError> {
        let (_c, eff, _) = effective_votes_for(voter.raw_balance, total_supply, &self.whale_cap)?;
        Ok(eff)
    }
}

fn simple_majority(
    tally: &VoteTally,
    pass_bps: u64,
    _min_voters: u64,
) -> Result<ProposalStatus, GovernanceError> {
    let voted = tally.total_voted()?;
    if voted == 0 {
        return Ok(ProposalStatus::FailedQuorum);
    }
    if tally.yes_ratio_bps() >= pass_bps {
        Ok(ProposalStatus::Passed)
    } else {
        Ok(ProposalStatus::Rejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address([b; 20])
    }

    #[test]
    fn full_election_flow_in_ecclesia() {
        let mut gov = GovernanceState::genesis(1_000);
        // Seed a Tamias so treasury tests can run separately; not needed here.
        let candidate = addr(9);
        let id = gov
            .submit_proposal(
                addr(1),
                "Elect Archon Eponymous",
                "Seat the first chair",
                ProposalKind::RankElection {
                    rank: CivicRank::ArchonEponymous,
                    seat_index: 0,
                    candidate,
                },
                0,
            )
            .unwrap();
        gov.add_deposit(id, gov.params.min_deposit).unwrap();
        gov.open_voting(id, 10).unwrap();

        // Turnout: two voters with weight ~31 each from balance 1000 → sqrt=31
        gov.cast_vote(id, addr(2), VoteChoice::Yes, 1_000, 100_000)
            .unwrap();
        gov.cast_vote(id, addr(3), VoteChoice::Yes, 1_000, 100_000)
            .unwrap();
        // eligible power 1000; voted ~62 → need lower eligible for quorum in test
        gov.ecclesia_eligible_power = 50;
        assert_eq!(gov.tally(id).unwrap(), ProposalStatus::Passed);
        gov.enter_timelock(id, 20).unwrap();
        assert!(matches!(
            gov.execute(id, 20),
            Err(GovernanceError::TimelockActive)
        ));
        gov.execute(id, 20 + gov.params.timelock_slots).unwrap();
        assert_eq!(
            gov.offices
                .seat(CivicRank::ArchonEponymous, 0)
                .unwrap()
                .holder,
            Some(candidate)
        );
        assert_eq!(gov.proposal(id).unwrap().status, ProposalStatus::Executed);
    }

    #[test]
    fn treasury_requires_tamias_sponsor() {
        let mut gov = GovernanceState::genesis(100);
        gov.offices
            .seat_holder(CivicRank::Tamias, 0, addr(7), 0, 100)
            .unwrap();
        let id = gov
            .submit_proposal(
                addr(1),
                "Fund explorers",
                "pay for infra",
                ProposalKind::TreasurySpend {
                    amount: 500,
                    beneficiary_note: "ops".into(),
                },
                0,
            )
            .unwrap();
        gov.add_deposit(id, gov.params.min_deposit).unwrap();
        assert_eq!(
            gov.open_voting(id, 1),
            Err(GovernanceError::MissingSponsorship)
        );
        gov.sponsor_as_tamias(id, addr(7)).unwrap();
        gov.open_voting(id, 1).unwrap();
    }

    #[test]
    fn constitution_amendment_needs_archon_assent() {
        let mut gov = GovernanceState::genesis(10);
        gov.offices
            .seat_holder(CivicRank::ArchonBasileus, 0, addr(5), 0, 100)
            .unwrap();
        let id = gov
            .submit_proposal(
                addr(1),
                "Amend",
                "v2",
                ProposalKind::ConstitutionAmendment {
                    new_constitution_id: "constitution-v2".into(),
                    new_body_markdown: "# v2\n".into(),
                },
                0,
            )
            .unwrap();
        gov.add_deposit(id, gov.params.min_deposit).unwrap();
        gov.open_voting(id, 1).unwrap();
        gov.cast_vote(id, addr(2), VoteChoice::Yes, 10_000, 10_000)
            .unwrap();
        gov.ecclesia_eligible_power = 10;
        assert_eq!(gov.tally(id).unwrap(), ProposalStatus::Passed);
        assert_eq!(
            gov.enter_timelock(id, 2),
            Err(GovernanceError::MissingArchonAssent)
        );
        gov.record_archon_assent(id, addr(5)).unwrap();
        gov.enter_timelock(id, 2).unwrap();
        gov.execute(id, 2 + gov.params.timelock_slots).unwrap();
        assert_eq!(gov.constitution.id, "constitution-v2");
    }

    #[test]
    fn boule_chamber_rejects_outsiders() {
        let mut gov = GovernanceState::genesis(1);
        gov.offices
            .seat_holder(CivicRank::Bouleutes, 0, addr(4), 0, 100)
            .unwrap();
        let id = gov
            .submit_proposal(
                addr(1),
                "Minor param",
                "bump",
                ProposalKind::ParameterChange {
                    scope: crate::proposal::ParameterScope::Minor,
                    key: "x".into(),
                    value: "1".into(),
                },
                0,
            )
            .unwrap();
        gov.add_deposit(id, gov.params.min_deposit).unwrap();
        gov.open_voting(id, 1).unwrap();
        assert_eq!(
            gov.cast_vote(id, addr(9), VoteChoice::Yes, 0, 1),
            Err(GovernanceError::IneligibleVoter)
        );
        gov.cast_vote(id, addr(4), VoteChoice::Yes, 0, 1).unwrap();
        assert_eq!(gov.tally(id).unwrap(), ProposalStatus::Passed);
    }
}
