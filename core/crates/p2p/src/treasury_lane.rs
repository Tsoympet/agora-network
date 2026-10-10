//! Mempool admission for controller-signed protocol treasury spends.

use agora_types::{Hash, TreasuryDisbursement, TreasuryId};

use crate::P2pError;

use super::Mempool;

impl Mempool {
    pub fn get_treasury_disbursement(&self, id: &Hash) -> Option<&TreasuryDisbursement> {
        self.treasury_disbursements.get(id)
    }

    pub fn admit_treasury_disbursement(
        &mut self,
        spend: TreasuryDisbursement,
    ) -> Result<Hash, P2pError> {
        if spend.version != 1 {
            return Err(P2pError::MempoolRejected(
                "unsupported treasury disbursement version".into(),
            ));
        }
        if spend.public_key.len() != 33 || spend.signature.len() != 64 {
            return Err(P2pError::MempoolRejected(
                "treasury disbursement missing secp256k1 auth".into(),
            ));
        }
        if spend.amount.as_base_units() == 0 {
            return Err(P2pError::MempoolRejected(
                "treasury amount must be nonzero".into(),
            ));
        }
        let id = spend.disbursement_id();
        if self.treasury_disbursements.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self.reserved_treasury_ids.contains_key(&spend.treasury) {
            return Err(P2pError::MempoolRejected(
                "treasury already has a pending disbursement nonce".into(),
            ));
        }
        self.reserved_treasury_ids.insert(spend.treasury, id);
        self.treasury_disbursements.insert(id, spend);
        Ok(id)
    }

    pub fn select_treasury_disbursements(&self, max: usize) -> Vec<TreasuryDisbursement> {
        let mut entries: Vec<_> = self.treasury_disbursements.values().cloned().collect();
        entries.sort_by(|a, b| {
            a.disbursement_id()
                .as_bytes()
                .cmp(b.disbursement_id().as_bytes())
        });
        if entries.len() > max {
            entries.truncate(max);
        }
        entries
    }

    pub(super) fn evict_treasury_disbursements_from_block(&mut self, block: &agora_types::Block) {
        let mut spent = std::collections::HashSet::<TreasuryId>::new();
        for spend in &block.treasury_disbursements {
            self.treasury_disbursements.remove(&spend.disbursement_id());
            spent.insert(spend.treasury);
        }
        self.reserved_treasury_ids
            .retain(|treasury, _| !spent.contains(treasury));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{Address, Amount, Hash, TreasuryId};

    fn spend(nonce: u64) -> TreasuryDisbursement {
        let mut spend = TreasuryDisbursement::unsigned(
            TreasuryId::OvlBuilder,
            Address([1; 20]),
            Amount::from_base_units(10),
            Hash([2; 32]),
            Hash([3; 32]),
            nonce,
        );
        spend.public_key = vec![1; 33];
        spend.signature = vec![2; 64];
        spend
    }

    #[test]
    fn admits_and_evicts_treasury_disbursements() {
        let mut pool = Mempool::new(8);
        let first = spend(0);
        let id = pool.admit_treasury_disbursement(first.clone()).unwrap();
        assert!(pool.get_treasury_disbursement(&id).is_some());
        assert!(pool.admit_treasury_disbursement(spend(1)).is_err());
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
        block.treasury_disbursements.push(first);
        pool.evict_treasury_disbursements_from_block(&block);
        assert!(pool.get_treasury_disbursement(&id).is_none());
        assert!(pool.admit_treasury_disbursement(spend(1)).is_ok());
    }
}
