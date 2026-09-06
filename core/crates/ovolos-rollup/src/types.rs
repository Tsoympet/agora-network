use agora_types::Hash;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Opaque EVM transaction bytes (RLP / typed-tx payload).
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct EvmTx(pub Vec<u8>);

/// Historical lab sequencer batch; construction does not imply L1 submission.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct Batch {
    pub sequence: u64,
    pub prev_state_root: Hash,
    pub post_state_root: Hash,
    pub transactions: Vec<EvmTx>,
    pub posted_at_ms: u64,
}

impl Batch {
    pub fn id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn tx_count(&self) -> usize {
        self.transactions.len()
    }
}

/// Fraud proof challenging a batch's claimed post-state root.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct FraudProof {
    pub batch_id: Hash,
    pub claimed_post_state_root: Hash,
    pub computed_post_state_root: Hash,
    /// Index of the first diverging transaction within the batch.
    pub diverging_tx_index: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BatchStatus {
    Pending,
    Challenged,
    Finalized,
    Reverted,
}
