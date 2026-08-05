//! Civic governance for Agora Network.
//!
//! ## What this crate provides
//!
//! 1. **Constitution v1** — higher-law text + content hash (`constitution`)
//! 2. **Elected ranks** — Archons, Bouleutai, Tamiai (`ranks`, `office`)
//! 3. **Voting chambers** — Ecclesia / Boule / Archon Collegium (`chamber`)
//! 4. **Proposal lifecycle** — deposit → vote → tally → timelock → execute (`engine`)
//! 5. **Launch-security math** — quadratic votes + 5% whale cap (`quadratic`, `whale`)
//!
//! See `docs/governance/CONSTITUTION.md` for the human-readable charter.
//! Analogues: Cosmos Hub `x/gov` lifecycle, Polkadot OpenGov tracks,
//! classical Athenian Ecclesia / Boule / Archons.

mod chamber;
mod constitution;
mod engine;
mod error;
mod math;
mod office;
mod params;
mod proposal;
mod quadratic;
mod ranks;
mod whale;

pub use chamber::{primary_chamber, VotingChamber};
pub use constitution::{
    constitution_v1_hash_hex, hash_constitution_body, EnactedConstitution, CONSTITUTION_V1_BODY,
    CONSTITUTION_V1_ID,
};
pub use engine::GovernanceState;
pub use error::GovernanceError;
pub use math::isqrt;
pub use office::{OfficeBoard, OfficeSeat};
pub use params::GovernanceParams;
pub use proposal::{
    Ballot, ParameterScope, Proposal, ProposalKind, ProposalStatus, VoteChoice, VoteTally,
};
pub use quadratic::{
    effective_votes_for, max_power_share_bps, quadratic_votes, tally_quadratic_votes,
    total_effective_power, EffectiveVote, VoterBalance,
};
pub use ranks::CivicRank;
pub use whale::{apply_whale_cap, WhaleCapConfig};
