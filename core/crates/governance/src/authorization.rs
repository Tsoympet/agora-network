//! Pure authorization policy for each proposal class.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    chamber::VotingChamber,
    proposal::{ParameterScope, ProposalKind},
};

/// Stable proposal class used by consensus policy commitments.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ProposalClass {
    TextSignal,
    ParameterMinor,
    ParameterMajor,
    TreasurySpend,
    SoftwareUpgrade,
    RankElection,
    RankImpeachment,
    ConstitutionAmendment,
    EmergencyAction,
}

impl ProposalClass {
    pub const ALL: [Self; 9] = [
        Self::TextSignal,
        Self::ParameterMinor,
        Self::ParameterMajor,
        Self::TreasurySpend,
        Self::SoftwareUpgrade,
        Self::RankElection,
        Self::RankImpeachment,
        Self::ConstitutionAmendment,
        Self::EmergencyAction,
    ];
}

/// Authorization gates required before a proposal may execute.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ProposalAuthorization {
    pub chamber: VotingChamber,
    pub requires_tamias_sponsor: bool,
    pub required_archon_assents: u8,
    pub basileus_assent_suffices: bool,
    pub requires_timelock: bool,
}

/// Resolve the complete authorization policy for a proposal kind.
pub const fn authorization_for(kind: &ProposalKind) -> ProposalAuthorization {
    authorization_for_class(proposal_class(kind))
}

pub const fn proposal_class(kind: &ProposalKind) -> ProposalClass {
    match kind {
        ProposalKind::TextSignal => ProposalClass::TextSignal,
        ProposalKind::ParameterChange { scope, .. } => match scope {
            ParameterScope::Minor => ProposalClass::ParameterMinor,
            ParameterScope::Major => ProposalClass::ParameterMajor,
        },
        ProposalKind::TreasurySpend { .. } => ProposalClass::TreasurySpend,
        ProposalKind::SoftwareUpgrade { .. } => ProposalClass::SoftwareUpgrade,
        ProposalKind::RankElection { .. } => ProposalClass::RankElection,
        ProposalKind::RankImpeachment { .. } => ProposalClass::RankImpeachment,
        ProposalKind::ConstitutionAmendment { .. } => ProposalClass::ConstitutionAmendment,
        ProposalKind::EmergencyAction { .. } => ProposalClass::EmergencyAction,
    }
}

pub const fn authorization_for_class(class: ProposalClass) -> ProposalAuthorization {
    ProposalAuthorization {
        chamber: match class {
            ProposalClass::ParameterMinor => VotingChamber::Boule,
            ProposalClass::EmergencyAction => VotingChamber::ArchonCollegium,
            _ => VotingChamber::Ecclesia,
        },
        requires_tamias_sponsor: matches!(class, ProposalClass::TreasurySpend),
        required_archon_assents: match class {
            ProposalClass::ConstitutionAmendment => 2,
            _ => 0,
        },
        basileus_assent_suffices: matches!(class, ProposalClass::ConstitutionAmendment),
        // The v1 Constitution defines Passed → Timelock → Executed for every class,
        // including emergency actions; it does not authorize a bypass.
        requires_timelock: true,
    }
}

/// Resolve the primary chamber from the canonical authorization policy.
pub const fn primary_chamber(kind: &ProposalKind) -> VotingChamber {
    authorization_for(kind).chamber
}

#[cfg(test)]
mod tests {
    use agora_types::Address;

    use super::*;
    use crate::CivicRank;

    fn assert_policy(
        kind: ProposalKind,
        chamber: VotingChamber,
        requires_tamias_sponsor: bool,
        required_archon_assents: u8,
    ) {
        let authorization = authorization_for(&kind);
        assert_eq!(
            authorization,
            ProposalAuthorization {
                chamber,
                requires_tamias_sponsor,
                required_archon_assents,
                basileus_assent_suffices: matches!(
                    &kind,
                    ProposalKind::ConstitutionAmendment { .. }
                ),
                requires_timelock: true,
            }
        );
        assert_eq!(primary_chamber(&kind), authorization.chamber);
    }

    #[test]
    fn matrix_covers_every_proposal_kind() {
        assert_policy(ProposalKind::TextSignal, VotingChamber::Ecclesia, false, 0);
        assert_policy(
            ProposalKind::ParameterChange {
                scope: ParameterScope::Minor,
                key: String::new(),
                value: String::new(),
            },
            VotingChamber::Boule,
            false,
            0,
        );
        assert_policy(
            ProposalKind::ParameterChange {
                scope: ParameterScope::Major,
                key: String::new(),
                value: String::new(),
            },
            VotingChamber::Ecclesia,
            false,
            0,
        );
        assert_policy(
            ProposalKind::TreasurySpend {
                amount: 1,
                beneficiary_note: String::new(),
            },
            VotingChamber::Ecclesia,
            true,
            0,
        );
        assert_policy(
            ProposalKind::SoftwareUpgrade {
                version: String::new(),
                manifest_hash_hex: String::new(),
            },
            VotingChamber::Ecclesia,
            false,
            0,
        );
        assert_policy(
            ProposalKind::RankElection {
                rank: CivicRank::Bouleutes,
                seat_index: 0,
                candidate: Address::ZERO,
            },
            VotingChamber::Ecclesia,
            false,
            0,
        );
        assert_policy(
            ProposalKind::RankImpeachment {
                rank: CivicRank::Tamias,
                seat_index: 0,
            },
            VotingChamber::Ecclesia,
            false,
            0,
        );
        assert_policy(
            ProposalKind::ConstitutionAmendment {
                new_constitution_id: String::new(),
                new_body_markdown: String::new(),
            },
            VotingChamber::Ecclesia,
            false,
            2,
        );
        assert_policy(
            ProposalKind::EmergencyAction {
                reason: String::new(),
            },
            VotingChamber::ArchonCollegium,
            false,
            0,
        );
    }
}
