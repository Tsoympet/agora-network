use std::collections::HashMap;

use agora_types::{Address, Amount, Hash};

use crate::da::BatchCommitment;
use crate::executor::{reexecute_batch, EvmExecutor};
use crate::genesis::OvolosGenesis;
use crate::ovl::OvlLedger;
use crate::pow::{mine_ovl_block, verify_pow, OvlBlock, OvlBlockHeader, OvlEmission};
use crate::sequencer::SequencerSet;
use crate::types::{Batch, BatchStatus, FraudProof};
use crate::RollupError;

/// Optimistic rollup configuration.
#[derive(Debug, Clone)]
pub struct RollupConfig {
    /// Challenge window length in milliseconds.
    pub challenge_window_ms: u64,
    /// When set, `submit_batch` charges OVL gas from this payer.
    pub gas_payer: Option<Address>,
    /// Minimum OVL bond for sequencers (hybrid PoS gate).
    pub sequencer_min_bond: u64,
}

impl Default for RollupConfig {
    fn default() -> Self {
        Self {
            // 7 days — classic optimistic default; tune per network params.
            challenge_window_ms: 7 * 24 * 60 * 60 * 1000,
            gas_payer: None,
            sequencer_min_bond: crate::sequencer::DEFAULT_SEQUENCER_MIN_BOND,
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
    /// Tip of the native OVL PoW chain (prev hash for the next block).
    tip_hash: Hash,
    /// Height of the last admitted PoW block (next height = tip_height).
    tip_height: u64,
    pow_bits: u32,
    emission: OvlEmission,
    /// Admitted PoW block ids by height.
    pow_blocks: HashMap<u64, Hash>,
    /// Bonded sequencers (hybrid PoS). Empty ⇒ permissionless submit/finalize.
    sequencers: SequencerSet,
}

impl<E: EvmExecutor> OvolosRollup<E> {
    pub fn new(config: RollupConfig, executor: E, genesis_state_root: Hash) -> Self {
        let sequencers = SequencerSet::new(config.sequencer_min_bond);
        Self {
            config,
            executor,
            next_sequence: 0,
            head_state_root: genesis_state_root,
            batches: HashMap::new(),
            by_sequence: HashMap::new(),
            ovl: OvlLedger::default(),
            da_posted: HashMap::new(),
            tip_hash: Hash::ZERO,
            tip_height: 0,
            pow_bits: crate::genesis::DEFAULT_OVL_POW_BITS,
            emission: OvlEmission::default(),
            pow_blocks: HashMap::new(),
            sequencers,
        }
    }

    /// Boot rollup from a frozen Ovolos L2 genesis (caps, gas, premine, state root, PoW).
    pub fn from_genesis(
        genesis: &OvolosGenesis,
        executor: E,
        gas_payer: Option<Address>,
    ) -> Result<Self, RollupError> {
        genesis.validate()?;
        let config = genesis.rollup_config(gas_payer);
        let sequencers = SequencerSet::new(config.sequencer_min_bond);
        Ok(Self {
            config,
            executor,
            next_sequence: 0,
            head_state_root: genesis.genesis_state_root_hash()?,
            batches: HashMap::new(),
            by_sequence: HashMap::new(),
            ovl: genesis.ignite_ledger()?,
            da_posted: HashMap::new(),
            tip_hash: Hash::ZERO,
            tip_height: 0,
            pow_bits: genesis.pow_bits,
            emission: genesis.emission(),
            pow_blocks: HashMap::new(),
            sequencers,
        })
    }

    pub fn config(&self) -> &RollupConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut RollupConfig {
        &mut self.config
    }

    pub fn executor(&self) -> &E {
        &self.executor
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

    pub fn sequencers(&self) -> &SequencerSet {
        &self.sequencers
    }

    /// Bond OVL as a hybrid sequencer (PoS gate for batch submit/finalize).
    pub fn bond_sequencer(
        &mut self,
        sequencer: Address,
        amount: Amount,
    ) -> Result<u64, RollupError> {
        self.sequencers.bond(&mut self.ovl, sequencer, amount)
    }

    pub fn unbond_sequencer(
        &mut self,
        sequencer: Address,
        amount: Amount,
    ) -> Result<u64, RollupError> {
        self.sequencers.unbond(&mut self.ovl, sequencer, amount)
    }

    pub fn tip_hash(&self) -> Hash {
        self.tip_hash
    }

    pub fn tip_height(&self) -> u64 {
        self.tip_height
    }

    pub fn pow_bits(&self) -> u32 {
        self.pow_bits
    }

    pub fn emission(&self) -> &OvlEmission {
        &self.emission
    }

    pub fn pow_block_id(&self, height: u64) -> Option<Hash> {
        self.pow_blocks.get(&height).copied()
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

    /// Sequence a batch (permissionless while no sequencers are bonded).
    pub fn submit_batch(&mut self, batch: Batch) -> Result<Hash, RollupError> {
        if self.sequencers.authorization_required() {
            return Err(RollupError::UnauthorizedSequencer);
        }
        self.submit_batch_inner(batch)
    }

    /// Sequence a batch as a bonded sequencer (hybrid PoS authorization).
    pub fn submit_batch_as(
        &mut self,
        sequencer: Address,
        batch: Batch,
    ) -> Result<Hash, RollupError> {
        self.sequencers.authorize(sequencer)?;
        self.submit_batch_inner(batch)
    }

    fn submit_batch_inner(&mut self, batch: Batch) -> Result<Hash, RollupError> {
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

    /// Finalize batches whose challenge window has elapsed (permissionless if no bonds).
    pub fn finalize_due(&mut self, now_ms: u64) -> Result<Vec<Hash>, RollupError> {
        if self.sequencers.authorization_required() {
            return Err(RollupError::UnauthorizedSequencer);
        }
        self.finalize_due_inner(now_ms)
    }

    /// Finalize due batches as a bonded sequencer.
    pub fn finalize_due_as(
        &mut self,
        sequencer: Address,
        now_ms: u64,
    ) -> Result<Vec<Hash>, RollupError> {
        self.sequencers.authorize(sequencer)?;
        self.finalize_due_inner(now_ms)
    }

    fn finalize_due_inner(&mut self, now_ms: u64) -> Result<Vec<Hash>, RollupError> {
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

    /// Admit a mined OVL PoW block: verify PoW, link tip, mint coinbase under cap.
    pub fn admit_mined_block(&mut self, block: OvlBlock) -> Result<Hash, RollupError> {
        verify_pow(&block.header)?;
        if block.header.bits != self.pow_bits {
            return Err(RollupError::Execution(format!(
                "OVL PoW bits {} != configured {}",
                block.header.bits, self.pow_bits
            )));
        }
        if block.header.height != self.tip_height {
            return Err(RollupError::Execution(format!(
                "OVL block height {} != tip {}",
                block.header.height, self.tip_height
            )));
        }
        if block.header.prev_block_hash != self.tip_hash {
            return Err(RollupError::Execution(
                "OVL prev_block_hash does not match tip".into(),
            ));
        }
        if block.header.batch_id != block.batch.id() {
            return Err(RollupError::Execution(
                "OVL header batch_id mismatch".into(),
            ));
        }
        let expected_reward = self.emission.reward_at_height(block.header.height);
        if block.header.reward != expected_reward {
            return Err(RollupError::Execution(format!(
                "OVL coinbase reward {} != expected {}",
                block.header.reward, expected_reward
            )));
        }

        // Ensure the sealed batch is already sequenced (or sequence it now).
        // PoW miners are not sequencers — admission uses the inner path.
        let batch_id = block.batch.id();
        if !self.batches.contains_key(&batch_id) {
            self.submit_batch_inner(block.batch.clone())?;
        } else if self
            .batches
            .get(&batch_id)
            .map(|t| t.batch != block.batch)
            .unwrap_or(true)
        {
            return Err(RollupError::Execution(
                "OVL block batch does not match sequenced batch".into(),
            ));
        }

        if expected_reward > 0 {
            self.ovl
                .mint(block.header.miner, Amount::from_base_units(expected_reward))?;
        }

        let id = block.id();
        self.pow_blocks.insert(block.header.height, id);
        self.tip_hash = id;
        self.tip_height = block.header.height.saturating_add(1);
        Ok(id)
    }

    /// Sequence a batch, mine a native OVL PoW block, and admit coinbase issuance.
    pub fn mine_and_admit(
        &mut self,
        batch: Batch,
        miner: Address,
        timestamp_ms: u64,
        max_nonces: u64,
    ) -> Result<OvlBlock, RollupError> {
        let batch_id = self.submit_batch_inner(batch.clone())?;
        let height = self.tip_height;
        let reward = self.emission.reward_at_height(height);
        let header = OvlBlockHeader {
            height,
            prev_block_hash: self.tip_hash,
            batch_id,
            timestamp_ms,
            bits: self.pow_bits,
            nonce: 0,
            miner,
            reward,
        };
        let block = mine_ovl_block(header, batch, max_nonces)?;
        self.admit_mined_block(block.clone())?;
        Ok(block)
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
                sequencer_min_bond: 0,
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
                sequencer_min_bond: 0,
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
                sequencer_min_bond: 0,
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

    #[test]
    fn mine_and_admit_mints_native_coinbase() {
        let miner = Address([9u8; 20]);
        let mut rollup = OvolosRollup::new(RollupConfig::default(), StubEvmExecutor, Hash::ZERO);
        rollup.pow_bits = 0; // deterministic in unit tests
        let batch = sample_batch(0, Hash::ZERO, &StubEvmExecutor, 0);
        let before = rollup.ovl().balance(miner).as_base_units();
        let block = rollup.mine_and_admit(batch, miner, 1, 8).unwrap();
        verify_pow(&block.header).unwrap();
        assert_eq!(rollup.tip_height(), 1);
        assert_eq!(rollup.tip_hash(), block.id());
        let after = rollup.ovl().balance(miner).as_base_units();
        assert_eq!(after - before, rollup.emission().reward_at_height(0));
    }

    #[test]
    fn bonded_sequencer_required_once_set_nonempty() {
        let seq = Address([0x51; 20]);
        let mut rollup = OvolosRollup::new(
            RollupConfig {
                challenge_window_ms: 1000,
                gas_payer: None,
                sequencer_min_bond: 100,
            },
            StubEvmExecutor,
            Hash::ZERO,
        );
        rollup
            .ovl_mut()
            .mint(seq, Amount::from_base_units(500))
            .unwrap();
        let batch = sample_batch(0, Hash::ZERO, &StubEvmExecutor, 0);
        // Still permissionless before any bond.
        rollup.submit_batch(batch.clone()).unwrap();
        rollup
            .bond_sequencer(seq, Amount::from_base_units(100))
            .unwrap();
        let batch1 = sample_batch(1, rollup.head_state_root(), &StubEvmExecutor, 1);
        assert!(matches!(
            rollup.submit_batch(batch1.clone()),
            Err(RollupError::UnauthorizedSequencer)
        ));
        let id = rollup.submit_batch_as(seq, batch1).unwrap();
        assert_eq!(rollup.batch_status(&id), Some(BatchStatus::Pending));
        assert!(matches!(
            rollup.finalize_due(10_000),
            Err(RollupError::UnauthorizedSequencer)
        ));
        let done = rollup.finalize_due_as(seq, 10_000).unwrap();
        assert!(done.contains(&id));
    }
}
