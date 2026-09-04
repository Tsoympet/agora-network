//! Trident multi-chamber proposal classes and approval metadata.
//!
//! These are canonical policy schemas only. They do not make the unsigned
//! administrative civic RPC consensus-valid.

use agora_types::TreasuryId;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum TridentChamber {
    OvlTechnical,
    DrcCommunity,
    Ecclesia,
    MinerSignaling,
    SecurityCouncil,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum TridentProposalClass {
    TechnicalExecutionUpgrade,
    PaymentModuleUpgrade,
    ConsensusUpgrade,
    CommunityGrant,
    ProtocolDevelopmentGrant,
    HubAccreditation,
    TreasuryPolicy { treasury: TreasuryId },
    ConstitutionalAmendment,
    EmergencySecurityAction,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum TimelockClass {
    Standard,
    Extended,
    EmergencyExpiry,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct TridentApprovalMatrix {
    pub required_chambers: Vec<TridentChamber>,
    pub post_action_ratification: Vec<TridentChamber>,
    pub timelock: TimelockClass,
    pub milestone_release: bool,
    pub conflict_of_interest_review: bool,
    pub automatic_expiry: bool,
}

fn standard(required_chambers: Vec<TridentChamber>) -> TridentApprovalMatrix {
    TridentApprovalMatrix {
        required_chambers,
        post_action_ratification: Vec::new(),
        timelock: TimelockClass::Standard,
        milestone_release: false,
        conflict_of_interest_review: false,
        automatic_expiry: false,
    }
}

pub fn trident_approval_matrix(class: &TridentProposalClass) -> TridentApprovalMatrix {
    use TridentChamber::{DrcCommunity, Ecclesia, MinerSignaling, OvlTechnical, SecurityCouncil};
    use TridentProposalClass::*;

    match class {
        TechnicalExecutionUpgrade => {
            standard(vec![OvlTechnical, Ecclesia, MinerSignaling])
        }
        PaymentModuleUpgrade => standard(vec![DrcCommunity, Ecclesia]),
        ConsensusUpgrade => TridentApprovalMatrix {
            required_chambers: vec![OvlTechnical, DrcCommunity, MinerSignaling, Ecclesia],
            timelock: TimelockClass::Extended,
            ..standard(Vec::new())
        },
        CommunityGrant => TridentApprovalMatrix {
            required_chambers: vec![DrcCommunity],
            milestone_release: true,
            conflict_of_interest_review: true,
            ..standard(Vec::new())
        },
        ProtocolDevelopmentGrant => TridentApprovalMatrix {
            required_chambers: vec![OvlTechnical, Ecclesia],
            milestone_release: true,
            ..standard(Vec::new())
        },
        HubAccreditation => standard(vec![Ecclesia, DrcCommunity]),
        TreasuryPolicy { treasury } => {
            let relevant = match treasury {
                TreasuryId::TltSecurity => MinerSignaling,
                TreasuryId::OvlBuilder => OvlTechnical,
                TreasuryId::DrcCommunity => DrcCommunity,
            };
            standard(vec![relevant, Ecclesia])
        }
        ConstitutionalAmendment => TridentApprovalMatrix {
            required_chambers: vec![OvlTechnical, DrcCommunity, Ecclesia, MinerSignaling],
            timelock: TimelockClass::Extended,
            ..standard(Vec::new())
        },
        EmergencySecurityAction => TridentApprovalMatrix {
            required_chambers: vec![SecurityCouncil],
            post_action_ratification: vec![
                OvlTechnical,
                DrcCommunity,
                Ecclesia,
                MinerSignaling,
            ],
            timelock: TimelockClass::EmergencyExpiry,
            automatic_expiry: true,
            ..standard(Vec::new())
        },
    }
}

/// Stable catalog included in the canonical authorization commitment.
pub fn trident_policy_catalog() -> Vec<(TridentProposalClass, TridentApprovalMatrix)> {
    use TridentProposalClass::*;
    let classes = vec![
        TechnicalExecutionUpgrade,
        PaymentModuleUpgrade,
        ConsensusUpgrade,
        CommunityGrant,
        ProtocolDevelopmentGrant,
        HubAccreditation,
        TreasuryPolicy {
            treasury: TreasuryId::TltSecurity,
        },
        TreasuryPolicy {
            treasury: TreasuryId::OvlBuilder,
        },
        TreasuryPolicy {
            treasury: TreasuryId::DrcCommunity,
        },
        ConstitutionalAmendment,
        EmergencySecurityAction,
    ];
    classes
        .into_iter()
        .map(|class| {
            let matrix = trident_approval_matrix(&class);
            (class, matrix)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trident_matrix_covers_upgrade_grant_treasury_and_emergency_paths() {
        let consensus = trident_approval_matrix(&TridentProposalClass::ConsensusUpgrade);
        assert_eq!(consensus.required_chambers.len(), 4);
        assert_eq!(consensus.timelock, TimelockClass::Extended);

        let grant = trident_approval_matrix(&TridentProposalClass::CommunityGrant);
        assert!(grant.milestone_release);
        assert!(grant.conflict_of_interest_review);

        let drc_treasury = trident_approval_matrix(&TridentProposalClass::TreasuryPolicy {
            treasury: TreasuryId::DrcCommunity,
        });
        assert_eq!(
            drc_treasury.required_chambers,
            vec![TridentChamber::DrcCommunity, TridentChamber::Ecclesia]
        );

        let emergency =
            trident_approval_matrix(&TridentProposalClass::EmergencySecurityAction);
        assert_eq!(
            emergency.required_chambers,
            vec![TridentChamber::SecurityCouncil]
        );
        assert!(emergency.automatic_expiry);
        assert_eq!(emergency.post_action_ratification.len(), 4);
    }

    #[test]
    fn policy_catalog_has_all_three_asset_treasuries() {
        let catalog = trident_policy_catalog();
        assert_eq!(catalog.len(), 11);
        for treasury in TreasuryId::ALL {
            assert!(catalog.iter().any(|(class, _)| {
                matches!(
                    class,
                    TridentProposalClass::TreasuryPolicy { treasury: found }
                        if *found == treasury
                )
            }));
        }
    }
}
