use std::collections::{HashMap, HashSet};

use agora_types::{BlockHeader, Hash};

use crate::error::StratumError;
use crate::job::{share_id, MiningJob};

/// In-memory stratum pool state for kHeavyHash ASIC aggregation.
#[derive(Debug, Default)]
pub struct StratumPool {
    workers: HashSet<String>,
    jobs: HashMap<String, MiningJob>,
    accepted_shares: HashSet<String>,
    next_job: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedShare {
    pub worker: String,
    pub job_id: String,
    pub nonce: u64,
    pub pow_hash: Hash,
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

    pub fn create_job(&mut self, header: BlockHeader, difficulty_bits: u32) -> MiningJob {
        let job_id = format!("job-{}", self.next_job);
        self.next_job += 1;
        let job = MiningJob::new(job_id, header, difficulty_bits);
        self.jobs.insert(job.job_id.clone(), job.clone());
        job
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::Hash;

    #[test]
    fn accepts_easy_share_and_rejects_duplicate() {
        let mut pool = StratumPool::new();
        pool.authorize("asic-1");
        // bits=0 accepts any hash.
        let job = pool.create_job(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            0,
        );
        let share = pool.submit_share("asic-1", &job.job_id, 7).unwrap();
        assert_eq!(share.nonce, 7);
        assert!(matches!(
            pool.submit_share("asic-1", &job.job_id, 7),
            Err(StratumError::DuplicateShare)
        ));
    }
}
