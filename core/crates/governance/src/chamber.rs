//! Voting chambers — where binding votes are cast.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

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
