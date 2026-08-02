//! Audited **kHeavyHash** PoW digest, vendored from rusty-kaspa.
//!
//! Pipeline (same as Kaspa `kaspa-pow::State::calculate_pow`):
//! 1. `PowHash::new(pre_pow_hash, timestamp).finalize_with_nonce(nonce)`
//! 2. `Matrix::generate(pre_pow_hash).heavy_hash(...)` → final digest
//!
//! See `NOTICE` / `LICENSE-ISC` for attribution. No custom cryptography was invented.

mod hash;
mod matrix;
mod pow_hashers;
mod xoshiro;

pub use hash::Hash;
pub use pow_hashers::{KHeavyHash, PowHash};

use matrix::Matrix;

/// Full kHeavyHash PoW digest.
///
/// `pre_pow_hash` should be the header commitment with nonce (and typically
/// timestamp) zeroed — matching Kaspa's pre-PoW hash convention.
pub fn calculate_pow(pre_pow_hash: [u8; 32], timestamp: u64, nonce: u64) -> [u8; 32] {
    let pre = Hash::from_bytes(pre_pow_hash);
    let intermediate = PowHash::new(pre, timestamp).finalize_with_nonce(nonce);
    let matrix = Matrix::generate(pre);
    matrix.heavy_hash(intermediate).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha3::digest::{ExtendableOutput, Update, XofReader};
    use sha3::{CShake256, CShake256Core};

    #[test]
    fn heavy_hash_matches_cshake_domain() {
        let val = Hash::from_bytes([42; 32]);
        let hash1 = KHeavyHash::hash(val);

        let hasher = CShake256::from_core(CShake256Core::new(b"HeavyHash")).chain(val.0);
        let mut hash2 = [0u8; 32];
        hasher.finalize_xof().read(&mut hash2);
        assert_eq!(Hash::from_bytes(hash2), hash1);
    }

    #[test]
    fn calculate_pow_is_deterministic() {
        let pre = [7u8; 32];
        let a = calculate_pow(pre, 1_700_000_000_000, 42);
        let b = calculate_pow(pre, 1_700_000_000_000, 42);
        let c = calculate_pow(pre, 1_700_000_000_000, 43);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
