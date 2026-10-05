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
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_payment_channel_creates: vec![],
            drc_payment_channel_funds: vec![],
            drc_payment_channel_claims: vec![],
            drc_payment_channel_closes: vec![],
            drc_multisign_attachments: vec![],
        }
    }

    fn escrow_attachment_block(tx_root: Hash) -> Block {
        let mut block = empty_block(1, tx_root);
        block
            .drc_escrow_creates
            .push(agora_types::DrcEscrowCreateTx {
                version: agora_types::DRC_ESCROW_CREATE_TX_VERSION,
                owner: agora_types::Address([3; 20]),
                recipient: agora_types::Address([4; 20]),
                amount: agora_types::Amount::from_base_units(1),
                fee: agora_types::Amount::from_base_units(1),
                destination_tag: None,
                source_tag: None,
                invoice_id: Hash::ZERO,
                finish_after_blue_score: None,
                cancel_after_blue_score: Some(10),
                nonce: 0,
                account_sequence: None,
                public_key: vec![],
                signature: vec![],
                multisign: None,
            });
        block
            .drc_multisign_attachments
            .push(agora_types::DrcMultisignBlockAttachment {
                version: agora_types::DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
                key: agora_types::DrcMultisignAttachmentKey {
                    version: agora_types::DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
                    kind: agora_types::DrcMultisignOperationKind::DrcEscrowCreate,
                    signing_commitment: Hash([5; 32]),
                },
                auth: agora_types::DrcMultisignAuth {
                    version: agora_types::DRC_MULTISIGN_AUTH_VERSION,
                    signing_for: agora_types::Address([3; 20]),
                    signatures: vec![],
                },
            });
        block
    }

    #[test]
    fn stratum_job_preserves_check_lane_body_root() {
        let mut pool = StratumPool::new();
        let mut block = check_attachment_block(Hash::ZERO);
        block.header.tx_root = block.compute_body_root();
        let job = pool.create_job(block.clone(), 1);
        assert_eq!(job.block.drc_check_creates.len(), 1);
        assert_eq!(job.block.drc_multisign_attachments.len(), 1);
        assert_eq!(job.block.header.tx_root, block.header.tx_root);
        let solved = job.with_nonce(3);
        assert_eq!(solved.header.tx_root, block.header.tx_root);
        assert_eq!(solved.drc_check_creates.len(), 1);
    }

    fn check_attachment_block(parent: Hash) -> Block {
        let mut block = empty_block(1, parent);
        block.drc_check_creates.push(agora_types::DrcCheckCreateTx {
            version: agora_types::DRC_CHECK_CREATE_TX_VERSION,
            owner: agora_types::Address([2; 20]),
            destination: agora_types::Address([3; 20]),
            amount: agora_types::Amount::from_base_units(5),
            fee: agora_types::Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            expires_after_blue_score: Some(10),
            nonce: 0,
            account_sequence: None,
            public_key: vec![],
            signature: vec![],
            multisign: None,
        });
        block
            .drc_multisign_attachments
            .push(agora_types::DrcMultisignBlockAttachment {
                version: agora_types::DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
                key: agora_types::DrcMultisignAttachmentKey {
                    version: agora_types::DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
                    kind: agora_types::DrcMultisignOperationKind::DrcCheckCreate,
                    signing_commitment: Hash([5; 32]),
                },
                auth: agora_types::DrcMultisignAuth {
                    version: agora_types::DRC_MULTISIGN_AUTH_VERSION,
                    signing_for: agora_types::Address([3; 20]),
                    signatures: vec![],
                },
            });
        block
    }

    #[test]
    fn stratum_job_preserves_escrow_lane_body_root() {
        let mut pool = StratumPool::new();
        let mut block = escrow_attachment_block(Hash::ZERO);
        block.header.tx_root = block.compute_body_root();
        let job = pool.create_job(block.clone(), 1);
        assert_eq!(job.block.drc_escrow_creates.len(), 1);
        assert_eq!(job.block.drc_multisign_attachments.len(), 1);
        assert_eq!(job.block.header.tx_root, block.header.tx_root);
        let solved = job.with_nonce(3);
        assert_eq!(solved.header.tx_root, block.header.tx_root);
        assert_eq!(solved.drc_escrow_creates.len(), 1);
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
