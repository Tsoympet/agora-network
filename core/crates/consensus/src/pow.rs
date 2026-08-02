use agora_types::{BlockHeader, Hash};

use crate::ConsensusError;

/// Supported proof-of-work algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowAlgorithm {
    /// CPU-friendly RandomX path (desktop / miner-sidecar).
    RandomX,
    /// ASIC path used by stratum-pool (kHeavyHash).
    KHeavyHash,
}

/// Verifies header PoW without performing mining.
pub trait PowVerifier: Send + Sync {
    fn algorithm(&self) -> PowAlgorithm;
    fn verify(&self, header: &BlockHeader, hash: &Hash) -> Result<(), ConsensusError>;
}

/// Stub verifier that accepts any header — replaced once RandomX/kHeavyHash FFI lands.
#[derive(Debug)]
pub struct AcceptAllPow {
    algo: PowAlgorithm,
}

impl Default for AcceptAllPow {
    fn default() -> Self {
        Self {
            algo: PowAlgorithm::RandomX,
        }
    }
}

impl AcceptAllPow {
    pub fn new(algo: PowAlgorithm) -> Self {
        Self { algo }
    }
}

impl PowVerifier for AcceptAllPow {
    fn algorithm(&self) -> PowAlgorithm {
        self.algo
    }

    fn verify(&self, _header: &BlockHeader, _hash: &Hash) -> Result<(), ConsensusError> {
        Ok(())
    }
}
