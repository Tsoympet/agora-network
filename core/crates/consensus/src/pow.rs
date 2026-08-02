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

/// Leading-zero difficulty check used until RandomX / kHeavyHash FFI is wired.
///
/// Interprets `header.bits` as the required number of leading zero bits in `hash`.
#[derive(Debug, Clone, Copy)]
pub struct LeadingZeroPow {
    algo: PowAlgorithm,
}

impl Default for LeadingZeroPow {
    fn default() -> Self {
        Self {
            algo: PowAlgorithm::RandomX,
        }
    }
}

impl LeadingZeroPow {
    pub fn new(algo: PowAlgorithm) -> Self {
        Self { algo }
    }

    pub fn leading_zero_bits(hash: &Hash) -> u32 {
        let mut count = 0u32;
        for byte in hash.as_bytes() {
            if *byte == 0 {
                count += 8;
                continue;
            }
            count += byte.leading_zeros();
            break;
        }
        count
    }
}

impl PowVerifier for LeadingZeroPow {
    fn algorithm(&self) -> PowAlgorithm {
        self.algo
    }

    fn verify(&self, header: &BlockHeader, hash: &Hash) -> Result<(), ConsensusError> {
        let expected = Hash::hash_borsh(header);
        if expected != *hash {
            return Err(ConsensusError::InvalidPow);
        }
        if Self::leading_zero_bits(hash) < header.bits {
            return Err(ConsensusError::InvalidPow);
        }
        Ok(())
    }
}

/// Stub verifier that accepts any header — useful for DAG topology unit tests.
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

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::Hash;

    #[test]
    fn leading_zero_pow_rejects_insufficient_work() {
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 1,
            bits: 16,
            nonce: 0,
            tx_root: Hash::ZERO,
        };
        let hash = header.hash();
        let verifier = LeadingZeroPow::default();
        // Extremely unlikely a single nonce meets 16 leading zero bits; if it does, bump bits.
        let bits = if LeadingZeroPow::leading_zero_bits(&hash) >= 16 {
            40
        } else {
            16
        };
        let header = BlockHeader { bits, ..header };
        let hash = header.hash();
        assert!(verifier.verify(&header, &hash).is_err());
    }
}
