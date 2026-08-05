//! Consensus algorithms for Agora's BlockDAG.
//!
//! Ordering is GHOSTDAG-based; transaction finality is decided exclusively by the
//! [`acceptance`] layer (not by block color alone). PoW verification is
//! algorithm-pluggable (RandomX / kHeavyHash).

mod acceptance;
mod daa;
mod dag;
mod emission;
mod error;
mod ghostdag;
mod pow;

pub use acceptance::{
    accept_blue_blocks, coinbase_reward, fees_from_accepted, precheck_regular_tx, AcceptanceResult,
    BlockAcceptance, BlueBlockInput, MemoryUtxoView, TxAcceptanceOutcome, TxRejectReason,
    UtxoEntry, UtxoJournalOp, UtxoView, COINBASE_MATURITY, MAX_TX_BYTES, MAX_TX_INPUTS,
    MAX_TX_OUTPUTS, MIN_RELAY_FEE,
};
pub use daa::{next_difficulty, DaaConfig, Difficulty};
pub use dag::Dag;
pub use emission::EmissionSchedule;
pub use error::ConsensusError;
pub use ghostdag::{Ghostdag, GhostdagConfig, GhostdagData, OrderedBlock};
#[cfg(test)]
pub use pow::AcceptAllPow;
pub use pow::{LeadingZeroPow, PowAlgorithm, PowVerifier};
