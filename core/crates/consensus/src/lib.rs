//! Consensus algorithms for Agora's BlockDAG.
//!
//! Ordering is GHOSTDAG-based; PoW verification is algorithm-pluggable (RandomX / kHeavyHash).

mod error;
mod ghostdag;
mod pow;
mod emission;

pub use emission::EmissionSchedule;
pub use error::ConsensusError;
pub use ghostdag::{Ghostdag, GhostdagConfig, OrderedBlock};
pub use pow::{AcceptAllPow, PowAlgorithm, PowVerifier};
