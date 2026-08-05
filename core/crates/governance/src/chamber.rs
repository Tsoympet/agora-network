//! Voting chambers — where binding votes are cast.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::proposal::{ParameterScope, ProposalKind};

/// Chamber in which a proposal’s binding vote is held.
///
/// Analogs: Cosmos Hub voter set (Ecclesia), elected council (Boule),
/// Polkadot/OpenGov track separation / tech fellowship-style officer assent
/// (ArchonCollegium).
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
pub enum VotingChamber {
    /// All TLT holders — quadratic + whale-capped weight.
    Ecclesia,
    /// Elected Bouleutai — one seat, one vote.
    Boule,
    /// The three Archons — one Archon, one vote.
    ArchonCollegium,
}

impl VotingChamber {
    pub fn title(self) -> &'static str {
        match self {
            VotingChamber::Ecclesia => "Ecclesia",
            VotingChamber::Boule => "Boule",
            VotingChamber::ArchonCollegium => "Archon Collegium",
        }
    }

    pub fn greek(self) -> &'static str {
        match self {
            VotingChamber::Ecclesia => "Ἐκκλησία",
            VotingChamber::Boule => "Βουλή",
            VotingChamber::ArchonCollegium => "Ἀρχόντων Συνέδριον",
        }
    }

    /// Whether this chamber uses quadratic TLT weight (vs seat-equality).
    pub fn uses_quadratic_weight(self) -> bool {
        matches!(self, VotingChamber::Ecclesia)
    }
}

/// Resolve the primary chamber for a proposal kind (Constitution Art. IV).
pub fn primary_chamber(kind: &ProposalKind) -> VotingChamber {
    match kind {
        ProposalKind::TextSignal
        | ProposalKind::TreasurySpend { .. }
        | ProposalKind::SoftwareUpgrade { .. }
        | ProposalKind::RankElection { .. }
        | ProposalKind::RankImpeachment { .. }
        | ProposalKind::ConstitutionAmendment { .. } => VotingChamber::Ecclesia,
        ProposalKind::ParameterChange { scope, .. } => match scope {
            ParameterScope::Minor => VotingChamber::Boule,
            ParameterScope::Major => VotingChamber::Ecclesia,
        },
        ProposalKind::EmergencyAction { .. } => VotingChamber::ArchonCollegium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treasury_and_amendments_are_ecclesia() {
        assert_eq!(
            primary_chamber(&ProposalKind::TreasurySpend {
                amount: 1,
                beneficiary_note: String::new(),
            }),
            VotingChamber::Ecclesia
        );
        assert_eq!(
            primary_chamber(&ProposalKind::ConstitutionAmendment {
                new_constitution_id: "constitution-v2".into(),
                new_body_markdown: String::new(),
            }),
            VotingChamber::Ecclesia
        );
    }

    #[test]
    fn minor_params_go_to_boule() {
        assert_eq!(
            primary_chamber(&ProposalKind::ParameterChange {
                scope: ParameterScope::Minor,
                key: "rpc.max_batch".into(),
                value: "64".into(),
            }),
            VotingChamber::Boule
        );
    }
}
