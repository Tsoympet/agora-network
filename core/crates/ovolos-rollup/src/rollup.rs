use std::collections::HashMap;

use agora_types::{Address, Hash};

use crate::da::BatchCommitment;
use crate::executor::{reexecute_batch, EvmExecutor};
use crate::genesis::OvolosGenesis;
use crate::ovl::OvlLedger;
use crate::types::{Batch, BatchStatus, FraudProof};
use crate::RollupError;

/// Optimistic rollup configuration.
#[derive(Debug, Clone)]
pub struct RollupConfig {
    /// Challenge window length in milliseconds.
    pub challenge_window_ms: u64,
    /// When set, `submit_batch` charges OVL gas from this payer.
    pub gas_payer: Option<Address>,
}

impl Default for RollupConfig {
    fn default() -> Self {
        Self {
            // 7 days — classic optimistic default; tune per network params.
            challenge_window_ms: 7 * 24 * 60 * 60 * 1000,
            gas_payer: None,
        }
    }
}

#[derive(Debug, Clone)]
struct TrackedBatch {
    batch: Batch,
    status: BatchStatus,
    commitment: BatchCommitment,
}

/// In-memory Ovolos rollup sequencer / verifier state.
pub struct OvolosRollup<E: EvmExecutor> {
    config: RollupConfig,
    executor: E,
    next_sequence: u64,
    head_state_root: Hash,
    batches: HashMap<Hash, TrackedBatch>,
    /// Sequence → batch id for canonical chain rewind.
    by_sequence: HashMap<u64, Hash>,
    ovl: OvlLedger,
    /// L1 DA commitments accepted by the verifier (operator attestations).
    da_posted: HashMap<Hash, BatchCommitment>,
}

impl<E: EvmExecutor> OvolosRollup<E> {
    pub fn new(config: RollupConfig, executor: E, genesis_state_root: Hash) -> Self {
        Self {
            config,
            executor,
            next_sequence: 0,
            head_state_root: genesis_state_root,
            batches: HashMap::new(),
            by_sequence: HashMap::new(),
            ovl: OvlLedger::default(),
            da_posted: HashMap::new(),
        }
    }

    /// Boot rollup from a frozen Ovolos L2 genesis (caps, gas, premine, state root).
    pub fn from_genesis(
        genesis: &OvolosGenesis,
        executor: E,
        gas_payer: Option<Address>,
    ) -> Result<Self, RollupError> {
        genesis.validate()?;
        Ok(Self {
            config: genesis.rollup_config(gas_payer),
            executor,
            next_sequence: 0,
            head_state_root: genesis.genesis_state_root_hash()?,
            batches: HashMap::new(),
            by_sequence: HashMap::new(),
            ovl: genesis.ignite_ledger()?,
            da_posted: HashMap::new(),
        })
    }

    pub fn config(&self) -> &RollupConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut RollupConfig {
        &mut self.config
    }

    pub fn head_state_root(&self) -> Hash {
        self.head_state_root
    }

    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn ovl(&self) -> &OvlLedger {
        &self.ovl
    }

    pub fn ovl_mut(&mut self) -> &mut OvlLedger {
        &mut self.ovl
    }

    pub fn batch_status(&self, batch_id: &Hash) -> Option<BatchStatus> {
        self.batches.get(batch_id).map(|b| b.status)
    }

    pub fn get_batch(&self, batch_id: &Hash) -> Option<&Batch> {
        self.batches.get(batch_id).map(|b| &b.batch)
    }

    pub fn get_commitment(&self, batch_id: &Hash) -> Option<&BatchCommitment> {
        self.batches.get(batch_id).map(|b| &b.commitment)
    }

    pub fn list_batches(&self) -> Vec<(Hash, BatchStatus, u64)> {
        let mut out: Vec<_> = self
            .batches
            .iter()
            .map(|(id, t)| (*id, t.status, t.batch.sequence))
            .collect();
        out.sort_by_key(|(_, _, seq)| *seq);
        out
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

        if let Some(payer) = self.config.gas_payer {
            self.ovl.charge_gas(payer, batch.tx_count())?;
        }

        let id = batch.id();
        let commitment = BatchCommitment::from_batch(&batch);
        self.batches.insert(
            id,
            TrackedBatch {
                batch: batch.clone(),
                status: BatchStatus::Pending,
                commitment,
            },
        );
        self.by_sequence.insert(batch.sequence, id);
        self.head_state_root = batch.post_state_root;
        self.next_sequence += 1;
        Ok(id)
    }

    /// Record that a batch commitment was posted to L1 / DA (operator attestation).
    pub fn record_da_post(&mut self, commitment: BatchCommitment) -> Result<(), RollupError> {
        let tracked = self
            .batches
            .get(&commitment.batch_id)
            .ok_or(RollupError::UnknownBatch)?;
        if tracked.commitment != commitment {
            return Err(RollupError::Execution(
                "DA commitment does not match sequenced batch".into(),
            ));
        }
        self.da_posted.insert(commitment.batch_id, commitment);
        Ok(())
    }

    pub fn da_posted(&self, batch_id: &Hash) -> bool {
        self.da_posted.contains_key(batch_id)
    }

    /// Submit a fraud proof against a pending batch. On success the batch and
    /// all later sequenced batches are reverted and the head rewinds.
    pub fn challenge(&mut self, proof: FraudProof) -> Result<(), RollupError> {
        let tracked = self
            .batches
            .get(&proof.batch_id)
            .ok_or_else(|| RollupError::InvalidFraudProof("unknown batch".into()))?;

        match tracked.status {
            BatchStatus::Finalized => return Err(RollupError::AlreadyFinalized),
            BatchStatus::Reverted => return Err(RollupError::AlreadyReverted),
            BatchStatus::Challenged | BatchStatus::Pending => {}
        }

        let batch = tracked.batch.clone();
        let computed = reexecute_batch(&self.executor, &batch)?;
        if computed != proof.computed_post_state_root {
            return Err(RollupError::InvalidFraudProof(
                "computed root does not match local re-execution".into(),
            ));
        }
        if proof.claimed_post_state_root != batch.post_state_root {
            return Err(RollupError::InvalidFraudProof(
                "claimed root does not match batch".into(),
            ));
        }
        if computed == batch.post_state_root {
            return Err(RollupError::InvalidFraudProof(
                "batch re-executes correctly; not fraudulent".into(),
            ));
        }

        // Mark challenged batch + all later sequences as reverted and rewind.
        let from_seq = batch.sequence;
        let rewind_root = batch.prev_state_root;
        let mut to_revert = Vec::new();
        for (seq, id) in &self.by_sequence {
            if *seq >= from_seq {
                to_revert.push((*seq, *id));
            }
        }
        for (seq, id) in to_revert {
            if let Some(t) = self.batches.get_mut(&id) {
                t.status = if id == proof.batch_id {
                    BatchStatus::Challenged
                } else {
                    BatchStatus::Reverted
                };
                // Challenged batch also ends as Reverted after hook.
                if id == proof.batch_id {
                    t.status = BatchStatus::Reverted;
                }
            }
            self.by_sequence.remove(&seq);
            self.da_posted.remove(&id);
        }
        self.next_sequence = from_seq;
        self.head_state_root = rewind_root;
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
    use agora_types::Amount;

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
                gas_payer: None,
            },
            exec,
            genesis,
        );
        let batch = sample_batch(0, genesis, &StubEvmExecutor, 0);
        let id = rollup.submit_batch(batch).unwrap();
        assert_eq!(rollup.batch_status(&id), Some(BatchStatus::Pending));
        let commitment = rollup.get_commitment(&id).unwrap().clone();
        rollup.record_da_post(commitment).unwrap();
        assert!(rollup.da_posted(&id));
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

    #[test]
    fn challenge_rewinds_head_and_later_batches() {
        let exec = StubEvmExecutor;
        let genesis = Hash::ZERO;
        let mut rollup = OvolosRollup::new(
            RollupConfig {
                challenge_window_ms: 10_000,
                gas_payer: None,
            },
            exec,
            genesis,
        );
        let b0 = sample_batch(0, genesis, &StubEvmExecutor, 0);
        let id0 = rollup.submit_batch(b0.clone()).unwrap();
        let b1 = sample_batch(1, b0.post_state_root, &StubEvmExecutor, 1);
        let b1_post = b1.post_state_root;
        let id1 = rollup.submit_batch(b1).unwrap();
        assert_eq!(rollup.next_sequence(), 2);

        // Inject a fraudulent tip batch (stub always re-executes honestly).
        let evil_post = Hash([0xEE; 32]);
        let mut evil = b0.clone();
        evil.sequence = 10;
        evil.prev_state_root = b1_post;
        evil.post_state_root = evil_post;
        let evil_id = evil.id();
        rollup.batches.insert(
            evil_id,
            TrackedBatch {
                commitment: BatchCommitment::from_batch(&evil),
                batch: evil.clone(),
                status: BatchStatus::Pending,
            },
        );
        // Tip is the fraudulent batch at sequence 10.
        rollup.next_sequence = 11;
        rollup.head_state_root = evil_post;
        rollup.by_sequence.insert(10, evil_id);

        let stub = StubEvmExecutor;
        let honest = stub
            .apply_batch(&evil.prev_state_root, &evil.transactions)
            .unwrap();
        let proof = FraudProof {
            batch_id: evil_id,
            claimed_post_state_root: evil_post,
            computed_post_state_root: honest,
            diverging_tx_index: 0,
        };
        rollup.challenge(proof).unwrap();
        assert_eq!(rollup.batch_status(&evil_id), Some(BatchStatus::Reverted));
        assert_eq!(rollup.next_sequence(), 10);
        assert_eq!(rollup.head_state_root(), evil.prev_state_root);

        // Original happy batches still present.
        assert_eq!(rollup.batch_status(&id0), Some(BatchStatus::Pending));
        assert_eq!(rollup.batch_status(&id1), Some(BatchStatus::Pending));
    }

    #[test]
    fn gas_payer_charged() {
        let payer = Address([7u8; 20]);
        let mut rollup = OvolosRollup::new(
            RollupConfig {
                challenge_window_ms: 1000,
                gas_payer: Some(payer),
            },
            StubEvmExecutor,
            Hash::ZERO,
        );
        rollup
            .ovl_mut()
            .mint(payer, Amount::from_base_units(1_000_000))
            .unwrap();
        let batch = sample_batch(0, Hash::ZERO, &StubEvmExecutor, 0);
        let before = rollup.ovl().balance(payer).as_base_units();
        rollup.submit_batch(batch).unwrap();
        let after = rollup.ovl().balance(payer).as_base_units();
        assert!(after < before);
    }
}
