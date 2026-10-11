//! Mempool admission for beneficiary-signed vesting unlock claims.

use agora_types::{Address, Hash, VestingUnlock};

use crate::P2pError;

use super::Mempool;

impl Mempool {
    pub fn get_vesting_unlock(&self, id: &Hash) -> Option<&VestingUnlock> {
        self.vesting_unlocks.get(id)
    }

    pub fn admit_vesting_unlock(&mut self, claim: VestingUnlock) -> Result<Hash, P2pError> {
        if claim.version != 1 {
            return Err(P2pError::MempoolRejected(
                "unsupported vesting unlock version".into(),
            ));
        }
        if claim.public_key.len() != 33 || claim.signature.len() != 64 {
            return Err(P2pError::MempoolRejected(
                "vesting unlock missing secp256k1 auth".into(),
            ));
        }
        if claim.amount.as_base_units() == 0 {
            return Err(P2pError::MempoolRejected(
                "vesting amount must be nonzero".into(),
            ));
        }
        let id = claim.unlock_id();
        if self.vesting_unlocks.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self
            .reserved_vesting_beneficiaries
            .contains_key(&claim.beneficiary)
        {
            return Err(P2pError::MempoolRejected(
                "beneficiary already has a pending vesting nonce".into(),
            ));
        }
        self.reserved_vesting_beneficiaries
            .insert(claim.beneficiary, id);
        self.vesting_unlocks.insert(id, claim);
        Ok(id)
    }

    pub fn select_vesting_unlocks(&self, max: usize) -> Vec<VestingUnlock> {
        let mut entries: Vec<_> = self.vesting_unlocks.values().cloned().collect();
        entries.sort_by(|a, b| a.unlock_id().as_bytes().cmp(b.unlock_id().as_bytes()));
        if entries.len() > max {
            entries.truncate(max);
        }
        entries
    }

    pub(super) fn evict_vesting_unlocks_from_block(&mut self, block: &agora_types::Block) {
        let mut spent = std::collections::HashSet::<Address>::new();
        for claim in &block.vesting_unlocks {
            self.vesting_unlocks.remove(&claim.unlock_id());
            spent.insert(claim.beneficiary);
        }
        self.reserved_vesting_beneficiaries
            .retain(|beneficiary, _| !spent.contains(beneficiary));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{Address, Amount, Hash, NativeAssetId};

    fn claim(nonce: u64) -> VestingUnlock {
        let mut claim = VestingUnlock::unsigned(
            NativeAssetId::OVL,
            Address([1; 20]),
            Hash([2; 32]),
            Amount::from_base_units(10),
            nonce,
        );
        claim.public_key = vec![1; 33];
        claim.signature = vec![2; 64];
        claim
    }

    #[test]
    fn admits_and_evicts_vesting_unlocks() {
        let mut pool = Mempool::new(8);
        let first = claim(0);
        let id = pool.admit_vesting_unlock(first.clone()).unwrap();
        assert!(pool.get_vesting_unlock(&id).is_some());
        assert!(pool.admit_vesting_unlock(claim(1)).is_err());
        let mut block = agora_types::Block::utxo(
            agora_types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.vesting_unlocks.push(first);
        pool.evict_vesting_unlocks_from_block(&block);
        assert!(pool.get_vesting_unlock(&id).is_none());
        assert!(pool.admit_vesting_unlock(claim(1)).is_ok());
    }
}
