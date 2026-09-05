use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AccountTransfer, DrcPaymentTx, Hash, OvlExecutionTx, SignedStakeTx, Transaction};

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

/// Full Trident body: TLT UTXO plus native account, stake, execution, and payment lanes.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
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
    /// Signed native DRC payments with routing metadata.
    #[serde(default)]
    pub drc_payments: Vec<DrcPaymentTx>,
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
            drc_payments: Vec::new(),
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
    /// retain v2; OVL execution uses v3; DRC payments use v4.
    pub fn compute_body_root(&self) -> Hash {
        if !self.drc_payments.is_empty() {
            let payment_ids: Vec<Hash> = self
                .drc_payments
                .iter()
                .map(DrcPaymentTx::payment_id)
                .collect();
            return Hash::hash_borsh(&(
                b"agora-block-body-v4",
                self.compute_body_root_v3(),
                payment_ids,
            ));
        }
        self.compute_body_root_v3()
    }

    fn compute_body_root_v3(&self) -> Hash {
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

#[cfg(test)]
mod tests {
    use crate::{Address, Amount, DrcPaymentTx};

    use super::*;

    const FROZEN_V2_HEADER_BORSH_HEX: &str = concat!(
        "0100000000000090ebc49f010000000000000000000000000000",
        "6069a3710cacc620ebb4bdb9d3d3107c85371c17782d94cf6b6fb76fb05111bd"
    );
    const FROZEN_V2_BLOCK_BORSH_HEX: &str = concat!(
        "0100000000000090ebc49f0100000000000000000000000000006069a3710cacc620ebb4",
        "bdb9d3d3107c85371c17782d94cf6b6fb76fb05111bd0100000001000000000000000100",
        "00000080c6a47e8d0300ff9ec96f09eb154d038a552ecae59c50204ea9a9000000000000",
        "0000000000000000000000000000000000000000000000000000"
    );
    const FROZEN_V2_GENESIS_HASH: &str =
        "afe59232cd20a16bd56948044149d2b8013e63f3694c113074fef75ab0cb9b98";

    fn frozen_v2_testnet_block() -> Block {
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![crate::TxOut {
                value: Amount::from_base_units(1_000_000_000_000_000),
                address: Address::from_hex("ff9ec96f09eb154d038a552ecae59c50204ea9a9").unwrap(),
            }],
            0,
        );
        Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 1_785_715_200_000,
                bits: 0,
                nonce: 0,
                tx_root: Block::compute_tx_root(std::slice::from_ref(&coinbase)),
            },
            vec![coinbase],
        )
    }

    #[test]
    fn frozen_v2_header_and_block_bytes_are_unchanged() {
        let block = frozen_v2_testnet_block();
        let header_bytes = borsh::to_vec(&block.header).unwrap();
        let block_bytes = borsh::to_vec(&block).unwrap();

        assert_eq!(hex::encode(&header_bytes), FROZEN_V2_HEADER_BORSH_HEX);
        assert_eq!(hex::encode(&block_bytes), FROZEN_V2_BLOCK_BORSH_HEX);
        assert_eq!(block.header.hash().to_hex(), FROZEN_V2_GENESIS_HASH);
        assert_eq!(
            borsh::from_slice::<BlockHeader>(&header_bytes).unwrap(),
            block.header
        );
        assert_eq!(borsh::from_slice::<Block>(&block_bytes).unwrap(), block);
    }

    #[test]
    fn drc_payment_activates_body_root_v4() {
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        let legacy = block.compute_body_root();
        block.drc_payments.push(DrcPaymentTx::unsigned(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            9,
            Hash([3; 32]),
            0,
        ));
        assert_ne!(block.compute_body_root(), legacy);
        assert_eq!(
            block.compute_body_root(),
            Hash::hash_borsh(&(
                b"agora-block-body-v4",
                legacy,
                vec![block.drc_payments[0].payment_id()]
            ))
        );
    }
}
