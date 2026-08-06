//! Explicit transaction-acceptance statuses for Agora Trident L1.
//!
//! Block color alone must never imply confirmation. Acceptance is the sole
//! authority for mutation, fee attribution, confirmations, mempool eviction,
//! and explorer display (wired in Phase 2 onto the hardened Virtual soft-skip path).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Deterministic outcome of evaluating a transaction against the acceptance layer.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
pub enum TransactionAcceptance {
    /// Inputs available; fully valid; wins conflict resolution; mutates state.
    Accepted,
    /// Exact same `tx_id` already accepted earlier in consensus order.
    ExactDuplicate,
    /// Structurally/cryptographically validated but lost deterministic conflict.
    ConflictLost,
    /// Failed structural or cryptographic validation.
    Invalid,
}

impl TransactionAcceptance {
    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted)
    }

    /// Fees and coinbase entitlement credit only for accepted txs.
    pub const fn credits_fees(self) -> bool {
        self.is_accepted()
    }
}

/// Packed accepted/rejected bits for transactions in one blue block.
///
/// Bit `i` corresponds to `block.transactions[i]`. Bits are packed little-endian
/// within each byte (bit 0 of byte 0 = transaction index 0).
#[derive(
    Clone,
    PartialEq,
    Eq,
    Debug,
    Default,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
pub struct AcceptanceBitmap {
    pub len: u32,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceptance_fee_credit_only_when_accepted() {
        assert!(TransactionAcceptance::Accepted.credits_fees());
        assert!(!TransactionAcceptance::ExactDuplicate.credits_fees());
        assert!(!TransactionAcceptance::ConflictLost.credits_fees());
        assert!(!TransactionAcceptance::Invalid.credits_fees());
    }

    #[test]
    fn bitmap_roundtrip_flags() {
        let flags = [true, false, true, true, false];
        let bm = AcceptanceBitmap::from_bools(&flags);
        assert_eq!(bm.len, 5);
        assert_eq!(bm.accepted_count(), 3);
        for (i, flag) in flags.iter().enumerate() {
            assert_eq!(bm.get(i), Some(*flag));
        }
    }
}
