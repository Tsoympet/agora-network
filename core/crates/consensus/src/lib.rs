//! Consensus algorithms for Agora's BlockDAG.
//!
//! Ordering is GHOSTDAG-based; PoW verification is algorithm-pluggable (RandomX / kHeavyHash).

mod dag;
mod daa;
mod emission;
mod error;
mod ghostdag;
mod limits;
mod pow;

pub use dag::Dag;
pub use daa::{
    next_difficulty, next_difficulty_weighted, work_from_bits, DaaConfig, DaaSample, Difficulty,
};
pub use emission::EmissionSchedule;
pub use error::ConsensusError;
pub use ghostdag::{Ghostdag, GhostdagConfig, GhostdagData, OrderedBlock};
pub use limits::{
    ConsensusLimits, COINBASE_MATURITY, MAX_BLOCK_BYTES, MAX_BLOCK_PARENTS,
    MAX_BLOCK_TRANSACTIONS, MAX_TIMESTAMP_AHEAD_MS, MAX_TX_BYTES, MAX_TX_INPUTS, MAX_TX_OUTPUTS,
};
pub use pow::{
    hasher_for, AcceptAllPow, KHeavyHashPowHasher, LeadingZeroPow, PowAlgorithm, PowHasher,
    PowVerifier, RandomXPowHasher, Sha256PowHasher,
};
