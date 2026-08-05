use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Hash;

/// Deterministic accepted/rejected bitmap for transactions in one blue block.
///
/// Bit `i` corresponds to `block.transactions[i]`. Bits are packed little-endian
/// within each byte (bit 0 of byte 0 = transaction index 0).
#[derive(
    Clone, PartialEq, Eq, Debug, Default, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct AcceptanceBitmap {
    /// Number of transaction bits represented.
    pub len: u32,
    /// Packed acceptance bits.
    pub bytes: Vec<u8>,
}

impl AcceptanceBitmap {
    pub fn from_bools(flags: &[bool]) -> Self {
        let len = flags.len() as u32;
        let mut bytes = vec![0u8; flags.len().div_ceil(8)];
        for (i, accepted) in flags.iter().enumerate() {
            if *accepted {
                bytes[i / 8] |= 1u8 << (i % 8);
            }
        }
        Self { len, bytes }
    }

    pub fn get(&self, index: usize) -> Option<bool> {
        if index >= self.len as usize {
            return None;
        }
        let byte = self.bytes.get(index / 8)?;
        Some((byte & (1u8 << (index % 8))) != 0)
    }

    pub fn is_accepted(&self, index: usize) -> bool {
        self.get(index).unwrap_or(false)
    }

    pub fn accepted_count(&self) -> usize {
        (0..self.len as usize)
            .filter(|i| self.is_accepted(*i))
            .count()
    }

    pub fn to_bools(&self) -> Vec<bool> {
        (0..self.len as usize)
            .map(|i| self.is_accepted(i))
            .collect()
    }
}

/// Lifecycle of a transaction relative to the acceptance layer.
///
/// Confirmations and explorer status must use this — never block color alone.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub enum TxAcceptanceStatus {
    /// Not yet evaluated against a blue accepting set.
    Pending,
    /// Included in a blue block and accepted after validation + conflict resolution.
    Accepted,
    /// Evaluated and rejected (invalid, duplicate, or input conflict).
    Rejected,
}

/// Confirmation depth derived from acceptance, not mere blue inclusion.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct TxConfirmation {
    pub status: TxAcceptanceStatus,
    /// Blue-score of the block that accepted this tx (`0` when not accepted).
    pub accepting_blue_score: u64,
    /// `tip_blue_score - accepting_blue_score` when accepted; otherwise `0`.
    pub confirmations: u64,
    pub accepting_block: Hash,
}

impl TxConfirmation {
    pub fn pending() -> Self {
        Self {
            status: TxAcceptanceStatus::Pending,
            accepting_blue_score: 0,
            confirmations: 0,
            accepting_block: Hash::ZERO,
        }
    }

    pub fn rejected() -> Self {
        Self {
            status: TxAcceptanceStatus::Rejected,
            accepting_blue_score: 0,
            confirmations: 0,
            accepting_block: Hash::ZERO,
        }
    }

    pub fn accepted(accepting_block: Hash, accepting_blue_score: u64, tip_blue_score: u64) -> Self {
        Self {
            status: TxAcceptanceStatus::Accepted,
            accepting_blue_score,
            confirmations: tip_blue_score.saturating_sub(accepting_blue_score),
            accepting_block,
        }
    }
}
