//! Governance parameters (Cosmos-like thresholds).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceParams {
    pub min_deposit: u64,
    /// Ecclesia ordinary quorum (participating / eligible) in bps.
    pub ecclesia_quorum_bps: u64,
    /// Ecclesia ordinary Yes threshold among non-abstain in bps.
    pub ecclesia_pass_bps: u64,
    /// NoWithVeto share of total voted that kills the proposal.
    pub ecclesia_veto_bps: u64,
    pub amendment_quorum_bps: u64,
    pub amendment_pass_bps: u64,
    pub boule_pass_bps: u64,
    pub archon_pass_bps: u64,
    pub voting_period_slots: u64,
    pub timelock_slots: u64,
    pub emergency_ratify_slots: u64,
    pub term_slots: u64,
    pub boule_seats: u16,
    pub tamias_seats: u16,
}

impl Default for GovernanceParams {
    fn default() -> Self {
        Self {
            min_deposit: 1_000_000,
            ecclesia_quorum_bps: 4_000,  // 40%
            ecclesia_pass_bps: 5_000,    // 50%
            ecclesia_veto_bps: 3_340,    // ~33.4%
            amendment_quorum_bps: 5_000, // 50%
            amendment_pass_bps: 6_670,   // ~66.7%
            boule_pass_bps: 5_000,
            archon_pass_bps: 5_000,
            voting_period_slots: 10_000,
            timelock_slots: 1_000,
            emergency_ratify_slots: 5_000,
            term_slots: 1_000_000,
            boule_seats: 21,
            tamias_seats: 3,
        }
    }
}
