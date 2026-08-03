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

/// Computes the PoW digest for a header (algorithm-specific).
pub trait PowHasher: Send + Sync {
    fn algorithm(&self) -> PowAlgorithm;
    fn pow_hash(&self, header: &BlockHeader) -> Hash;
}

/// Verifies header PoW without performing mining.
pub trait PowVerifier: Send + Sync {
    fn algorithm(&self) -> PowAlgorithm;
    fn verify(&self, header: &BlockHeader, hash: &Hash) -> Result<(), ConsensusError>;
}

/// SHA-256(borsh(header)) stand-in used when the `randomx` feature is disabled.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sha256PowHasher;

impl PowHasher for Sha256PowHasher {
    fn algorithm(&self) -> PowAlgorithm {
        PowAlgorithm::RandomX
    }

    fn pow_hash(&self, header: &BlockHeader) -> Hash {
        Hash::hash_borsh(header)
    }
}

/// Official RandomX digest over an Agora header (feature `randomx`).
///
/// - Key = `SHA-256(borsh(header))` with `nonce = 0`
/// - Input = full `borsh(header)` including the candidate nonce
#[derive(Debug, Clone, Copy, Default)]
pub struct RandomXPowHasher;

impl RandomXPowHasher {
    pub fn key_hash(header: &BlockHeader) -> Hash {
        let keyed = BlockHeader {
            nonce: 0,
            ..header.clone()
        };
        Hash::hash_borsh(&keyed)
    }
}

#[cfg(feature = "randomx")]
impl PowHasher for RandomXPowHasher {
    fn algorithm(&self) -> PowAlgorithm {
        PowAlgorithm::RandomX
    }

    fn pow_hash(&self, header: &BlockHeader) -> Hash {
        use std::sync::Arc;

        use rust_randomx::{Context, Hasher};

        let key = Self::key_hash(header);
        let input = borsh::to_vec(header).expect("borsh header is infallible");
        // Light mode (fast=false): lower memory, suitable for verify + sidecar smoke.
        let ctx = Arc::new(Context::new(key.as_bytes(), false));
        let hasher = Hasher::new(ctx);
        let out = hasher.hash(&input);
        let mut digest = [0u8; 32];
        digest.copy_from_slice(out.as_ref());
        Hash(digest)
    }
}

#[cfg(not(feature = "randomx"))]
impl PowHasher for RandomXPowHasher {
    fn algorithm(&self) -> PowAlgorithm {
        PowAlgorithm::RandomX
    }

    fn pow_hash(&self, header: &BlockHeader) -> Hash {
        // Fall back so workspace builds without a C++ toolchain still compile.
        Sha256PowHasher.pow_hash(header)
    }
}

/// Audited Kaspa kHeavyHash digest over an Agora header commitment.
///
/// Pre-PoW commitment = `borsh(header)` with `nonce = 0` and `timestamp_ms = 0`,
/// then Kaspa's `PowHash` + matrix `heavy_hash` with the real timestamp/nonce.
#[derive(Debug, Clone, Copy, Default)]
pub struct KHeavyHashPowHasher;

impl KHeavyHashPowHasher {
    /// Header commitment used as Kaspa `pre_pow_hash` (nonce + timestamp zeroed).
    pub fn pre_pow_hash(header: &BlockHeader) -> Hash {
        let committed = BlockHeader {
            nonce: 0,
            timestamp_ms: 0,
            ..header.clone()
        };
        Hash::hash_borsh(&committed)
    }
}

impl PowHasher for KHeavyHashPowHasher {
    fn algorithm(&self) -> PowAlgorithm {
        PowAlgorithm::KHeavyHash
    }

    fn pow_hash(&self, header: &BlockHeader) -> Hash {
        let pre = Self::pre_pow_hash(header);
        let digest =
            agora_kheavyhash::calculate_pow(*pre.as_bytes(), header.timestamp_ms, header.nonce);
        Hash(digest)
    }
}

/// Select the default hasher for an algorithm.
pub fn hasher_for(algo: PowAlgorithm) -> Box<dyn PowHasher> {
    match algo {
        PowAlgorithm::RandomX => Box::new(RandomXPowHasher),
        PowAlgorithm::KHeavyHash => Box::new(KHeavyHashPowHasher),
    }
}

/// Leading-zero difficulty check over an algorithm-specific PoW digest.
///
/// Interprets `header.bits` as the required number of leading zero bits in the digest.
pub struct LeadingZeroPow {
    hasher: Box<dyn PowHasher>,
}

impl Default for LeadingZeroPow {
    fn default() -> Self {
        Self::new(PowAlgorithm::RandomX)
    }
}

impl LeadingZeroPow {
    pub fn new(algo: PowAlgorithm) -> Self {
        Self {
            hasher: hasher_for(algo),
        }
    }

    pub fn with_hasher(hasher: Box<dyn PowHasher>) -> Self {
        Self { hasher }
    }

    pub fn hasher(&self) -> &dyn PowHasher {
        self.hasher.as_ref()
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
        self.hasher.algorithm()
    }

    fn verify(&self, header: &BlockHeader, hash: &Hash) -> Result<(), ConsensusError> {
        let expected = self.hasher.pow_hash(header);
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
        let hasher = Sha256PowHasher;
        let hash = hasher.pow_hash(&header);
        let verifier = LeadingZeroPow::with_hasher(Box::new(Sha256PowHasher));
        let bits = if LeadingZeroPow::leading_zero_bits(&hash) >= 16 {
            40
        } else {
            16
        };
        let header = BlockHeader { bits, ..header };
        let hash = hasher.pow_hash(&header);
        assert!(verifier.verify(&header, &hash).is_err());
    }

    #[test]
    fn kheavyhash_hasher_roundtrip_verify() {
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 1_700_000_000_000,
            bits: 0,
            nonce: 99,
            tx_root: Hash::ZERO,
        };
        let hasher = KHeavyHashPowHasher;
        let hash = hasher.pow_hash(&header);
        assert_ne!(hash, header.hash());
        let verifier = LeadingZeroPow::new(PowAlgorithm::KHeavyHash);
        assert!(verifier.verify(&header, &hash).is_ok());
        assert!(verifier.verify(&header, &Hash::ZERO).is_err());
    }

    #[cfg(feature = "randomx")]
    #[test]
    fn randomx_hasher_roundtrip_verify() {
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 1,
            bits: 0,
            nonce: 7,
            tx_root: Hash::ZERO,
        };
        let hasher = RandomXPowHasher;
        let hash = hasher.pow_hash(&header);
        assert_ne!(hash, Sha256PowHasher.pow_hash(&header));
        let verifier = LeadingZeroPow::new(PowAlgorithm::RandomX);
        assert!(verifier.verify(&header, &hash).is_ok());
    }
}
