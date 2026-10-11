//! Beneficiary-signed vesting unlock claims.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Address, Amount, Hash, NativeAssetId};

/// Domain separator for network-bound vesting unlocks.
pub const VESTING_UNLOCK_DOMAIN: &[u8] = b"agora-vesting-unlock-v1";
/// Domain separator for the consensus schedule identity.
pub const VESTING_SCHEDULE_ID_DOMAIN: &[u8] = b"agora-vesting-schedule-v1";

/// Beneficiary-signed claim of vested coins. Does not mint.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct VestingUnlock {
    pub version: u32,
    pub asset: NativeAssetId,
    pub beneficiary: Address,
    pub schedule_id: Hash,
    pub amount: Amount,
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl VestingUnlock {
    pub fn unsigned(
        asset: NativeAssetId,
        beneficiary: Address,
        schedule_id: Hash,
        amount: Amount,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            asset,
            beneficiary,
            schedule_id,
            amount,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let body = (
            VESTING_UNLOCK_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.asset,
            self.beneficiary,
            self.schedule_id,
            self.amount,
            self.nonce,
        );
        borsh::to_vec(&body).expect("borsh serialize vesting unlock body")
    }

    pub fn unlock_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

/// Canonical identity of one genesis vesting schedule.
pub fn vesting_schedule_id(
    asset: NativeAssetId,
    address: &Address,
    amount: u64,
    start_timestamp_ms: u64,
    cliff_timestamp_ms: u64,
    end_timestamp_ms: u64,
) -> Hash {
    Hash::hash_borsh(&(
        VESTING_SCHEDULE_ID_DOMAIN,
        asset,
        address,
        amount,
        start_timestamp_ms,
        cliff_timestamp_ms,
        end_timestamp_ms,
    ))
}

/// Linear vest after `start`; nothing is claimable before `cliff`.
pub fn vested_amount_at(
    amount: u64,
    start_timestamp_ms: u64,
    cliff_timestamp_ms: u64,
    end_timestamp_ms: u64,
    now_ms: u64,
) -> u64 {
    if now_ms < cliff_timestamp_ms || end_timestamp_ms <= start_timestamp_ms {
        return 0;
    }
    if now_ms >= end_timestamp_ms {
        return amount;
    }
    let elapsed = now_ms.saturating_sub(start_timestamp_ms);
    let span = end_timestamp_ms - start_timestamp_ms;
    ((amount as u128) * (elapsed as u128) / (span as u128)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vested_amount_respects_cliff_and_linear_end() {
        assert_eq!(vested_amount_at(100, 10, 20, 30, 19), 0);
        assert_eq!(vested_amount_at(100, 10, 20, 30, 20), 50);
        assert_eq!(vested_amount_at(100, 10, 20, 30, 30), 100);
        assert_eq!(vested_amount_at(100, 10, 20, 30, 40), 100);
    }

    #[test]
    fn unlock_id_covers_signature_and_chain() {
        let mut claim = VestingUnlock::unsigned(
            NativeAssetId::OVL,
            Address([1; 20]),
            Hash([2; 32]),
            Amount::from_base_units(10),
            0,
        );
        let unsigned = claim.unlock_id();
        claim.signature = vec![9; 64];
        assert_ne!(claim.unlock_id(), unsigned);
        assert_ne!(
            claim.signing_bytes_bound("agora-dev", &Hash::ZERO),
            claim.signing_bytes_bound("agora-testnet", &Hash::ZERO)
        );
    }
}
