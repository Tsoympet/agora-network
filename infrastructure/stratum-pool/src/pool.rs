use std::collections::{HashMap, HashSet};

use agora_types::{Block, Hash};

use crate::error::StratumError;
use crate::job::{share_id, MiningJob};

/// In-memory stratum pool state for kHeavyHash ASIC aggregation.
#[derive(Debug, Default)]
pub struct StratumPool {
    workers: HashSet<String>,
    jobs: HashMap<String, MiningJob>,
    accepted_shares: HashSet<String>,
    next_job: u64,
    current_job_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedShare {
    pub worker: String,
    pub job_id: String,
    pub nonce: u64,
    pub pow_hash: Hash,
    /// Solved block ready for `agora_submitBlock`.
    pub block: Block,
}

impl StratumPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn authorize(&mut self, worker: impl Into<String>) -> bool {
        self.workers.insert(worker.into()) || true
    }

    pub fn is_authorized(&self, worker: &str) -> bool {
        self.workers.contains(worker)
    }

    pub fn current_job(&self) -> Option<&MiningJob> {
        self.current_job_id
            .as_ref()
            .and_then(|id| self.jobs.get(id))
    }

    pub fn create_job(&mut self, block: Block, difficulty_bits: u32) -> MiningJob {
        let job_id = format!("job-{}", self.next_job);
        self.next_job += 1;
        let job = MiningJob::new(job_id, block, difficulty_bits);
        self.current_job_id = Some(job.job_id.clone());
        self.jobs.insert(job.job_id.clone(), job.clone());
        job
    }

    /// Install a live node template when parents / tx_root / bits change.
    pub fn upsert_template(&mut self, block: Block) -> Option<MiningJob> {
        if let Some(cur) = self.current_job() {
            if cur.block.header.parents == block.header.parents
                && cur.block.header.tx_root == block.header.tx_root
                && cur.block.header.bits == block.header.bits
            {
                return None;
            }
        }
        let bits = block.header.bits;
        Some(self.create_job(block, bits))
    }

    pub fn submit_share(
        &mut self,
        worker: &str,
        job_id: &str,
        nonce: u64,
    ) -> Result<AcceptedShare, StratumError> {
        if !self.is_authorized(worker) {
            return Err(StratumError::Unauthorized);
        }
        let job = self
            .jobs
            .get(job_id)
            .cloned()
            .ok_or_else(|| StratumError::UnknownJob(job_id.into()))?;
        let pow_hash = job.pow_hash(nonce);
        if !job.meets_target(&pow_hash) {
            return Err(StratumError::LowDifficulty);
        }
        let id = share_id(job_id, nonce, worker);
        if !self.accepted_shares.insert(id) {
            return Err(StratumError::DuplicateShare);
        }
        Ok(AcceptedShare {
            worker: worker.into(),
            job_id: job_id.into(),
            nonce,
            pow_hash,
            block: job.with_nonce(nonce),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{BlockHeader, Hash};

    fn empty_block(bits: u32, tx_root: Hash) -> Block {
        Block {
            header: BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits,
                nonce: 0,
                tx_root,
            },
            transactions: vec![],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
        }
    }

    #[test]
    fn accepts_easy_share_and_rejects_duplicate() {
        let mut pool = StratumPool::new();
        pool.authorize("asic-1");
        // bits=0 accepts any hash.
        let job = pool.create_job(empty_block(0, Hash::ZERO), 0);
        let share = pool.submit_share("asic-1", &job.job_id, 7).unwrap();
        assert_eq!(share.nonce, 7);
        assert_eq!(share.block.header.nonce, 7);
        assert!(matches!(
            pool.submit_share("asic-1", &job.job_id, 7),
            Err(StratumError::DuplicateShare)
        ));
    }

    #[test]
    fn upsert_template_skips_identical_work() {
        let mut pool = StratumPool::new();
        let block = empty_block(1, Hash::ZERO);
        assert!(pool.upsert_template(block.clone()).is_some());
        assert!(pool.upsert_template(block).is_none());
        let mut next = empty_block(1, Hash::ZERO);
        next.header.parents = vec![Hash::hash_borsh(&1u64)];
        assert!(pool.upsert_template(next).is_some());
        assert_eq!(pool.current_job().unwrap().job_id, "job-1");
    }
}
