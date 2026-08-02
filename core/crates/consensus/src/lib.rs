//! Consensus algorithms for Agora's BlockDAG.
//!
//! Ordering is GHOSTDAG-based; PoW verification is algorithm-pluggable (RandomX / kHeavyHash).

mod dag;
mod daa;
mod emission;
mod error;
mod ghostdag;
mod pow;

pub use dag::Dag;
pub use daa::{
    next_difficulty, next_difficulty_weighted, work_from_bits, DaaConfig, DaaSample, Difficulty,
};
pub use emission::EmissionSchedule;
pub use error::ConsensusError;
pub use ghostdag::{Ghostdag, GhostdagConfig, GhostdagData, OrderedBlock};
pub use pow::{
    hasher_for, AcceptAllPow, KHeavyHashPowHasher, LeadingZeroPow, PowAlgorithm, PowHasher,
    PowVerifier, RandomXPowHasher, Sha256PowHasher,
};
