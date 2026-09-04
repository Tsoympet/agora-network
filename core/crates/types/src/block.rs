use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AccountTransfer, Hash, OvlExecutionTx, SignedStakeTx, Transaction};

/// Block header for Agora's BlockDAG tips.
///
/// Multiple parents enable parallel block production; GHOSTDAG later imposes order.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct BlockHeader {
    pub version: u16,
    pub parents: Vec<Hash>,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub nonce: u64,
    pub tx_root: Hash,
}

impl BlockHeader {
    pub fn hash(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

/// Full block: header + multi-lane body (TLT UTXO + OVL/DRC account/stake + OVL execution).
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct Block {
    pub header: BlockHeader,
    /// TLT UTXO lane (coinbase + transfers).
    pub transactions: Vec<Transaction>,
    /// OVL/DRC liquid account transfers (Trident).
    #[serde(default)]
    pub account_transfers: Vec<AccountTransfer>,
    /// OVL/DRC staking ops (Trident).
    #[serde(default)]
    pub stake_ops: Vec<SignedStakeTx>,
    /// Signed, gas-metered OVL execution envelopes.
    #[serde(default)]
    pub ovl_executions: Vec<OvlExecutionTx>,
}

impl Block {
    /// Construct a UTXO-only block (empty account/stake lanes).
    pub fn utxo(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
        Self {
            header,
            transactions,
            account_transfers: Vec::new(),
            stake_ops: Vec::new(),
            ovl_executions: Vec::new(),
        }
    }

    pub fn id(&self) -> Hash {
        self.header.hash()
    }

    /// Compute a simple pairwise tx merkle root (duplicate last leaf when odd).
    ///
    /// Kept intentionally minimal for Phase 1 ID stability; consensus may harden this later.
    pub fn compute_tx_root(transactions: &[Transaction]) -> Hash {
        if transactions.is_empty() {
            return Hash::ZERO;
        }
        let mut level: Vec<Hash> = transactions.iter().map(Transaction::tx_id).collect();
        while level.len() > 1 {
            if level.len() % 2 == 1 {
                level.push(*level.last().expect("non-empty"));
            }
            level = level
                .chunks(2)
                .map(|pair| {
                    let mut buf = [0u8; 64];
                    buf[..32].copy_from_slice(pair[0].as_bytes());
                    buf[32..].copy_from_slice(pair[1].as_bytes());
                    Hash::hash_bytes(&buf)
                })
                .collect();
        }
        level[0]
    }

    /// Body commitment for `header.tx_root`.
    ///
    /// UTXO-only blocks keep the legacy merkle root; account/stake-only bodies
    /// retain v2; OVL execution bodies use the v3 domain.
    pub fn compute_body_root(&self) -> Hash {
        if !self.ovl_executions.is_empty() {
            let execution_ids: Vec<Hash> = self
                .ovl_executions
                .iter()
                .map(OvlExecutionTx::tx_id)
                .collect();
            return Hash::hash_borsh(&(
                b"agora-block-body-v3",
                self.compute_body_root_v2(),
                execution_ids,
            ));
        }
        self.compute_body_root_v2()
    }

    fn compute_body_root_v2(&self) -> Hash {
        if self.account_transfers.is_empty() && self.stake_ops.is_empty() {
            return Self::compute_tx_root(&self.transactions);
        }
        let account_ids: Vec<Hash> = self
            .account_transfers
            .iter()
            .map(AccountTransfer::transfer_id)
            .collect();
        let stake_ids: Vec<Hash> = self
            .stake_ops
            .iter()
            .map(SignedStakeTx::stake_tx_id)
            .collect();
        Hash::hash_borsh(&(
            b"agora-block-body-v2",
            Self::compute_tx_root(&self.transactions),
            account_ids,
            stake_ids,
        ))
    }
}
