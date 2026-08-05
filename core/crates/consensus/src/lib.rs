//! Consensus algorithms for Agora's BlockDAG.
//!
//! Ordering is GHOSTDAG-based; transaction finality is decided exclusively by the
//! [`acceptance`] layer (not by block color alone). PoW verification is
//! algorithm-pluggable (RandomX / kHeavyHash).

mod acceptance;
mod dag;
mod daa;
mod emission;
mod error;
mod ghostdag;
mod pow;

pub use acceptance::{
    accept_blue_blocks, coinbase_reward, fees_from_accepted, AcceptanceResult, BlockAcceptance,
    BlueBlockInput, MemoryUtxoView, TxAcceptanceOutcome, TxRejectReason, UtxoJournalOp, UtxoView,
};
pub use dag::Dag;
pub use daa::{next_difficulty, DaaConfig, Difficulty};
pub use emission::EmissionSchedule;
pub use error::ConsensusError;
pub use ghostdag::{Ghostdag, GhostdagConfig, GhostdagData, OrderedBlock};
pub use pow::{AcceptAllPow, LeadingZeroPow, PowAlgorithm, PowVerifier};
