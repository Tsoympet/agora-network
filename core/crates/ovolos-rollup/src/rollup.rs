use std::collections::HashMap;

use agora_types::Hash;

use crate::executor::{reexecute_batch, EvmExecutor};
use crate::types::{Batch, BatchStatus, FraudProof};
use crate::RollupError;

/// Optimistic rollup configuration.
#[derive(Debug, Clone)]
pub struct RollupConfig {
    /// Challenge window length in milliseconds.
    pub challenge_window_ms: u64,
}

impl Default for RollupConfig {
    fn default() -> Self {
        Self {
            // 7 days — classic optimistic default; tune per network params.
            challenge_window_ms: 7 * 24 * 60 * 60 * 1000,
        }
    }
}

#[derive(Debug, Clone)]
struct TrackedBatch {
    batch: Batch,
    status: BatchStatus,
}

/// In-memory Ovolos rollup sequencer / verifier state.
pub struct OvolosRollup<E: EvmExecutor> {
    config: RollupConfig,
    executor: E,
    next_sequence: u64,
    head_state_root: Hash,
    batches: HashMap<Hash, TrackedBatch>,
}

impl<E: EvmExecutor> OvolosRollup<E> {
    pub fn new(config: RollupConfig, executor: E, genesis_state_root: Hash) -> Self {
        Self {
            config,
            executor,
            next_sequence: 0,
            head_state_root: genesis_state_root,
            batches: HashMap::new(),
        }
    }

    pub fn head_state_root(&self) -> Hash {
        self.head_state_root
    }

    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn batch_status(&self, batch_id: &Hash) -> Option<BatchStatus> {
        self.batches.get(batch_id).map(|b| b.status)
    }

    /// Sequence a batch after local execution validates the claimed post-root.
    pub fn submit_batch(&mut self, batch: Batch) -> Result<Hash, RollupError> {
        if batch.sequence != self.next_sequence {
            return Err(RollupError::SequenceGap {
                expected: self.next_sequence,
                got: batch.sequence,
            });
        }
        if batch.prev_state_root != self.head_state_root {
            return Err(RollupError::Execution(
                "prev_state_root does not match rollup head".into(),
            ));
        }

        let computed = reexecute_batch(&self.executor, &batch)?;
        if computed != batch.post_state_root {
            return Err(RollupError::Execution(
                "claimed post_state_root mismatch".into(),
            ));
        }

        let id = batch.id();
        self.batches.insert(
            id,
            TrackedBatch {
                batch: batch.clone(),
                status: BatchStatus::Pending,
            },
        );
        self.head_state_root = batch.post_state_root;
        self.next_sequence += 1;
        Ok(id)
    }

    /// Submit a fraud proof against a pending batch.
    pub fn challenge(&mut self, proof: FraudProof) -> Result<(), RollupError> {
        let tracked = self
            .batches
            .get_mut(&proof.batch_id)
            .ok_or_else(|| RollupError::InvalidFraudProof("unknown batch".into()))?;

        if tracked.status == BatchStatus::Finalized {
            return Err(RollupError::AlreadyFinalized);
        }

        let computed = reexecute_batch(&self.executor, &tracked.batch)?;
        if computed != proof.computed_post_state_root {
            return Err(RollupError::InvalidFraudProof(
                "computed root does not match local re-execution".into(),
            ));
        }
        if proof.claimed_post_state_root != tracked.batch.post_state_root {
            return Err(RollupError::InvalidFraudProof(
                "claimed root does not match batch".into(),
            ));
        }
        if computed == tracked.batch.post_state_root {
            return Err(RollupError::InvalidFraudProof(
                "batch re-executes correctly; not fraudulent".into(),
            ));
        }

        tracked.status = BatchStatus::Challenged;
        Ok(())
    }

    /// Finalize batches whose challenge window has elapsed.
    pub fn finalize_due(&mut self, now_ms: u64) -> Result<Vec<Hash>, RollupError> {
        let mut finalized = Vec::new();
        for (id, tracked) in self.batches.iter_mut() {
            if tracked.status != BatchStatus::Pending {
                continue;
            }
            let due = tracked
                .batch
                .posted_at_ms
                .saturating_add(self.config.challenge_window_ms);
            if now_ms < due {
                continue;
            }
            tracked.status = BatchStatus::Finalized;
            finalized.push(*id);
        }
        Ok(finalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::StubEvmExecutor;
    use crate::types::EvmTx;

    fn sample_batch(seq: u64, prev: Hash, executor: &StubEvmExecutor, posted_at_ms: u64) -> Batch {
        let txs = vec![EvmTx(vec![1, 2, 3]), EvmTx(vec![4, 5])];
        let post = executor.apply_batch(&prev, &txs).unwrap();
        Batch {
            sequence: seq,
            prev_state_root: prev,
            post_state_root: post,
            transactions: txs,
            posted_at_ms,
        }
    }

    #[test]
    fn submit_and_finalize_happy_path() {
        let exec = StubEvmExecutor;
        let genesis = Hash::ZERO;
        let mut rollup = OvolosRollup::new(
            RollupConfig {
                challenge_window_ms: 1000,
            },
            exec,
            genesis,
        );
        let batch = sample_batch(0, genesis, &StubEvmExecutor, 0);
        let id = rollup.submit_batch(batch).unwrap();
        assert_eq!(rollup.batch_status(&id), Some(BatchStatus::Pending));
        assert!(rollup.finalize_due(999).unwrap().is_empty());
        let done = rollup.finalize_due(1000).unwrap();
        assert_eq!(done, vec![id]);
        assert_eq!(rollup.batch_status(&id), Some(BatchStatus::Finalized));
    }

    #[test]
    fn rejects_bad_claimed_root() {
        let exec = StubEvmExecutor;
        let mut rollup = OvolosRollup::new(RollupConfig::default(), exec, Hash::ZERO);
        let mut batch = sample_batch(0, Hash::ZERO, &StubEvmExecutor, 0);
        batch.post_state_root = Hash([9u8; 32]);
        assert!(matches!(
            rollup.submit_batch(batch),
            Err(RollupError::Execution(_))
        ));
    }
}
