//! Signed staking operations for OVL / DRC validator sets (Trident L1).
//!
//! Distinct from [`crate::AccountTransfer`]: stake ops lock/unlock stake and
//! register consensus keys — they are not peer liquid transfers.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Address, Hash, NativeAssetId};

/// Domain separator for stake transaction signatures.
pub const STAKE_TX_SIGNING_DOMAIN: &[u8] = b"agora-trident-stake-tx-v1";

/// Kind of staking mutation.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub enum StakeOpKind {
    Bond,
    Delegate,
    UnbondSelf,
    Withdraw,
}

impl StakeOpKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bond => "Bond",
            Self::Delegate => "Delegate",
            Self::UnbondSelf => "UnbondSelf",
            Self::Withdraw => "Withdraw",
        }
    }
}

/// Network-bound signed stake transaction.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct SignedStakeTx {
    pub version: u32,
    pub asset: NativeAssetId,
    pub kind: StakeOpKind,
    /// Signer / liquid debit account (must match pubkey).
    pub actor: Address,
    /// Bond: operator (= actor). Delegate: target validator. Unbond/Withdraw: operator/owner.
    pub validator: Address,
    /// Bond / Delegate amount; ignored for UnbondSelf / Withdraw (full self-bond / matured).
    pub amount: u64,
    /// Bond only: 33-byte compressed consensus pubkey.
    pub consensus_pubkey: Vec<u8>,
    /// Bond only: withdrawal address (receives rewards / unbond credits).
    pub withdrawal: Address,
    /// Bond only: commission in basis points.
    pub commission_bps: u16,
    pub metadata_hash: Hash,
    /// Actor account nonce (must match current on-chain nonce).
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl SignedStakeTx {
    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let body = (
            STAKE_TX_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.asset,
            self.kind,
            self.actor,
            self.validator,
            self.amount,
            &self.consensus_pubkey,
            self.withdrawal,
            self.commission_bps,
            self.metadata_hash,
            self.nonce,
        );
        borsh::to_vec(&body).expect("borsh serialize stake tx body")
    }

    pub fn stake_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn unsigned_bond(
        asset: NativeAssetId,
        actor: Address,
        amount: u64,
        consensus_pubkey: Vec<u8>,
        withdrawal: Address,
        commission_bps: u16,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            asset,
            kind: StakeOpKind::Bond,
            actor,
            validator: actor,
            amount,
            consensus_pubkey,
            withdrawal,
            commission_bps,
            metadata_hash: Hash::ZERO,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn unsigned_delegate(
        asset: NativeAssetId,
        actor: Address,
        validator: Address,
        amount: u64,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            asset,
            kind: StakeOpKind::Delegate,
            actor,
            validator,
            amount,
            consensus_pubkey: Vec::new(),
            withdrawal: Address::ZERO,
            commission_bps: 0,
            metadata_hash: Hash::ZERO,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn unsigned_unbond_self(asset: NativeAssetId, actor: Address, nonce: u64) -> Self {
        Self {
            version: 1,
            asset,
            kind: StakeOpKind::UnbondSelf,
            actor,
            validator: actor,
            amount: 0,
            consensus_pubkey: Vec::new(),
            withdrawal: Address::ZERO,
            commission_bps: 0,
            metadata_hash: Hash::ZERO,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn unsigned_withdraw(asset: NativeAssetId, actor: Address, nonce: u64) -> Self {
        Self {
            version: 1,
            asset,
            kind: StakeOpKind::Withdraw,
            actor,
            validator: actor,
            amount: 0,
            consensus_pubkey: Vec::new(),
            withdrawal: Address::ZERO,
            commission_bps: 0,
            metadata_hash: Hash::ZERO,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stake_tx_id_stable() {
        let tx = SignedStakeTx::unsigned_bond(
            NativeAssetId::OVL,
            Address::ZERO,
            100,
            vec![2; 33],
            Address::ZERO,
            0,
            0,
        );
        assert_eq!(tx.stake_tx_id(), tx.stake_tx_id());
        assert_eq!(tx.kind.as_str(), "Bond");
    }
}
