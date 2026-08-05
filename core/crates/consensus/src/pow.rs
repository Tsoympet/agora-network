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

/// Length of a RandomX seed epoch in milliseconds (~35 min).
///
/// Retained for legacy timestamp-bucket helpers and tests. Production admission and
/// mining use [`RANDOMX_EPOCH_BLOCKS`] (height / blue-score anchoring).
pub const RANDOMX_EPOCH_MS: u64 = 1 << 21;

/// Blue-score length of a RandomX seed epoch (~35 min at ~1 block/sec).
///
/// Anchoring the key to parent-derived blue score (not wall-clock) stops miners from
/// nudging `timestamp_ms` near a boundary to force an expensive context rebuild / DoS.
pub const RANDOMX_EPOCH_BLOCKS: u64 = 2_048;

/// Domain separator for the RandomX epoch seed (v2 = blue-score anchored).
const RANDOMX_SEED_DOMAIN: &[u8] = b"agora-randomx-epoch-v2";

/// Official RandomX digest over an Agora header (feature `randomx`).
///
/// - Key (seed) = `SHA-256(domain ‖ epoch)` where
///   `epoch = blue_score_anchor / RANDOMX_EPOCH_BLOCKS`
/// - Input = full `borsh(header)` including the candidate nonce
///
/// The epoch seed keeps the RandomX key stable across a mining window so the dataset is
/// built once and cached, instead of once per candidate header (which was both wasteful
/// for miners and a CPU/memory DoS vector for verifiers).
#[derive(Debug, Clone, Copy, Default)]
pub struct RandomXPowHasher;

impl RandomXPowHasher {
    /// Epoch index from a parent-derived blue-score anchor.
    pub fn epoch_from_blue_score(blue_score_anchor: u64) -> u64 {
        blue_score_anchor / RANDOMX_EPOCH_BLOCKS
    }

    /// Legacy timestamp-bucket epoch (tests / transitional tooling only).
    pub fn epoch(header: &BlockHeader) -> u64 {
        header.timestamp_ms / RANDOMX_EPOCH_MS
    }

    /// Stable per-epoch RandomX key/seed from an explicit epoch index.
    pub fn key_hash_for_epoch(epoch: u64) -> Hash {
        Hash::hash_borsh(&(RANDOMX_SEED_DOMAIN, epoch))
    }

    /// Key from a blue-score anchor (preferred production path).
    pub fn key_hash_for_blue_score(blue_score_anchor: u64) -> Hash {
        Self::key_hash_for_epoch(Self::epoch_from_blue_score(blue_score_anchor))
    }

    /// Legacy helper: timestamp-bucket key (does **not** match production admission).
    pub fn key_hash(header: &BlockHeader) -> Hash {
        Self::key_hash_for_epoch(Self::epoch(header))
    }

    /// RandomX digest using an explicit epoch (blue-score anchored in production).
    pub fn pow_hash_with_epoch(&self, header: &BlockHeader, epoch: u64) -> Hash {
        self.pow_hash_with_key(header, Self::key_hash_for_epoch(epoch))
    }

    fn pow_hash_with_key(&self, header: &BlockHeader, key: Hash) -> Hash {
        #[cfg(feature = "randomx")]
        {
            use rust_randomx::Hasher;

            let input = borsh::to_vec(header).expect("borsh header is infallible");
            let ctx = cached_context(key.as_bytes());
            let hasher = Hasher::new(ctx);
            let out = hasher.hash(&input);
            let mut digest = [0u8; 32];
            digest.copy_from_slice(out.as_ref());
            Hash(digest)
        }
        #[cfg(not(feature = "randomx"))]
        {
            let _ = key;
            Sha256PowHasher.pow_hash(header)
        }
    }
}

#[cfg(feature = "randomx")]
fn cached_context(key: &[u8; 32]) -> std::sync::Arc<rust_randomx::Context> {
    use std::sync::{Arc, Mutex, OnceLock};

    use rust_randomx::Context;

    // Keep several recent epoch contexts. Context construction is done **outside** the
    // mutex so concurrent verifies of a cold epoch do not serialize on dataset init.
    // Parent age is relay-only (not consensus); announce→getblock paths soft-drop stale
    // parents in the node to bound how far back gossip can force epoch rotation.
    // Do **not** sleep here — admit holds the chain lock during PoW verify.
    const CACHE_CAP: usize = 16;
    type EpochCache = Vec<([u8; 32], Arc<Context>)>;
    static CACHE: OnceLock<Mutex<EpochCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
    {
        let guard = cache.lock().expect("randomx context cache poisoned");
        if let Some((_, ctx)) = guard.iter().find(|(k, _)| k == key) {
            return ctx.clone();
        }
    }
    // Light mode (fast=false): lower memory, suitable for verify + sidecar smoke.
    let ctx = Arc::new(Context::new(key, false));
    let mut guard = cache.lock().expect("randomx context cache poisoned");
    if let Some((_, existing)) = guard.iter().find(|(k, _)| k == key) {
        return existing.clone();
    }
    guard.push((*key, ctx.clone()));
    while guard.len() > CACHE_CAP {
        guard.remove(0);
    }
    ctx
}

impl PowHasher for RandomXPowHasher {
    fn algorithm(&self) -> PowAlgorithm {
        PowAlgorithm::RandomX
    }

    fn pow_hash(&self, header: &BlockHeader) -> Hash {
        // Trait default uses the legacy timestamp bucket so callers without a DAG
        // still get a stable cached key. Production admission/mining must call
        // [`Self::pow_hash_with_epoch`] with the parent blue-score epoch.
        self.pow_hash_with_epoch(header, Self::epoch(header))
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

    #[test]
    fn randomx_key_is_epoch_stable_across_nonce() {
        // Same blue-score epoch + different nonce/timestamp => same RandomX key.
        let base = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 5,
            bits: 0,
            nonce: 1,
            tx_root: Hash::ZERO,
        };
        let other_nonce = BlockHeader {
            nonce: 999,
            timestamp_ms: base.timestamp_ms + 60_000,
            ..base.clone()
        };
        let epoch = RandomXPowHasher::epoch_from_blue_score(100);
        assert_eq!(
            RandomXPowHasher::key_hash_for_epoch(epoch),
            RandomXPowHasher::key_hash_for_blue_score(100)
        );
        // Key is stable for any blue-score inside the same epoch bucket.
        let _ = (base, other_nonce);
        assert_eq!(
            RandomXPowHasher::key_hash_for_blue_score(100),
            RandomXPowHasher::key_hash_for_blue_score(RANDOMX_EPOCH_BLOCKS - 1)
        );

        // Crossing a blue-score epoch boundary rotates the key.
        assert_ne!(
            RandomXPowHasher::key_hash_for_blue_score(RANDOMX_EPOCH_BLOCKS - 1),
            RandomXPowHasher::key_hash_for_blue_score(RANDOMX_EPOCH_BLOCKS)
        );
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
