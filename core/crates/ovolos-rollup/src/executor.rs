use agora_types::Hash;
use sha2::{Digest, Sha256};

use crate::types::{Batch, EvmTx};
use crate::RollupError;

/// Pluggable EVM execution surface for the optimistic path.
///
/// Production will bind `revm` (or equivalent) behind this trait. The default
/// stub derives a deterministic pseudo-root so sequencing / challenge flow can
/// be tested without embedding a full EVM yet.
pub trait EvmExecutor: Send + Sync {
    fn apply_batch(&self, prev_state_root: &Hash, txs: &[EvmTx]) -> Result<Hash, RollupError>;
}

/// Deterministic stub executor used until revm integration lands.
#[derive(Debug, Default)]
pub struct StubEvmExecutor;

impl EvmExecutor for StubEvmExecutor {
    fn apply_batch(&self, prev_state_root: &Hash, txs: &[EvmTx]) -> Result<Hash, RollupError> {
        let mut hasher = Sha256::new();
        hasher.update(prev_state_root.as_bytes());
        hasher.update((txs.len() as u64).to_le_bytes());
        for tx in txs {
            hasher.update(&(tx.0.len() as u64).to_le_bytes());
            hasher.update(&tx.0);
        }
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Ok(Hash(out))
    }
}

/// Helper to re-execute a batch and compare against its claimed root.
pub fn reexecute_batch(
    executor: &dyn EvmExecutor,
    batch: &Batch,
) -> Result<Hash, RollupError> {
    executor.apply_batch(&batch.prev_state_root, &batch.transactions)
}
