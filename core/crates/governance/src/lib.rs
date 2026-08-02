//! Launch-security governance primitives for Agora Network.
//!
//! - Quadratic voting: `EffectiveVotes = √RawBalance` (integer)
//! - Whale protection: 5% hard cap on countable balance vs total supply

mod error;
mod math;
mod quadratic;
mod whale;

pub use error::GovernanceError;
pub use math::isqrt;
pub use quadratic::{
    effective_votes_for, max_power_share_bps, quadratic_votes, tally_quadratic_votes,
    total_effective_power, EffectiveVote, VoterBalance,
};
pub use whale::{apply_whale_cap, WhaleCapConfig};
