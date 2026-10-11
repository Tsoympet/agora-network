//! Mempool admission for signed Hub-coordinator passport attestations.

use agora_types::{Address, Hash, PassportAttestation};

use crate::P2pError;

use super::Mempool;

impl Mempool {
    pub fn get_passport_attestation(&self, id: &Hash) -> Option<&PassportAttestation> {
        self.passport_attestations.get(id)
    }

    pub fn admit_passport_attestation(
        &mut self,
        attestation: PassportAttestation,
    ) -> Result<Hash, P2pError> {
        if attestation.version != 1 {
            return Err(P2pError::MempoolRejected(
                "unsupported passport attestation version".into(),
            ));
        }
        if attestation.public_key.len() != 33 || attestation.signature.len() != 64 {
            return Err(P2pError::MempoolRejected(
                "passport attestation missing secp256k1 auth".into(),
            ));
        }
        let id = attestation.attestation_id();
        if self.passport_attestations.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self
            .reserved_passport_issuers
            .contains_key(&attestation.issuer)
        {
            return Err(P2pError::MempoolRejected(
                "issuer already has a pending passport attestation nonce".into(),
            ));
        }
        self.reserved_passport_issuers
            .insert(attestation.issuer, id);
        self.passport_attestations.insert(id, attestation);
        Ok(id)
    }

    pub fn remove_passport_attestation(&mut self, id: &Hash) -> Option<PassportAttestation> {
        let attestation = self.passport_attestations.remove(id)?;
        self.reserved_passport_issuers.remove(&attestation.issuer);
        Some(attestation)
    }

    pub fn select_passport_attestations(&self, max: usize) -> Vec<PassportAttestation> {
        let mut entries: Vec<_> = self.passport_attestations.values().cloned().collect();
        entries.sort_by(|a, b| {
            a.attestation_id()
                .as_bytes()
                .cmp(b.attestation_id().as_bytes())
        });
        if entries.len() > max {
            entries.truncate(max);
        }
        entries
    }

    pub(super) fn evict_passport_attestations_from_block(&mut self, block: &agora_types::Block) {
        let mut consumed: std::collections::HashSet<Address> = std::collections::HashSet::new();
        for attestation in &block.passport_attestations {
            let id = attestation.attestation_id();
            self.passport_attestations.remove(&id);
            consumed.insert(attestation.issuer);
        }
        self.reserved_passport_issuers
            .retain(|issuer, _| !consumed.contains(issuer));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{Address, Hash, PassportAttestation, PassportCategory};

    fn att(nonce: u64) -> PassportAttestation {
        let mut a = PassportAttestation::unsigned(
            Address([1; 20]),
            Address([2; 20]),
            PassportCategory::Code,
            Hash([3; 32]),
            Hash([4; 32]),
            5,
            None,
            nonce,
        );
        a.public_key = vec![1; 33];
        a.signature = vec![2; 64];
        a
    }

    #[test]
    fn admits_and_evicts_passport_attestations() {
        let mut pool = Mempool::new(8);
        let first = att(0);
        let id = pool.admit_passport_attestation(first.clone()).unwrap();
        assert_eq!(pool.get_passport_attestation(&id).unwrap().nonce, 0);
        assert!(pool.admit_passport_attestation(att(1)).is_err());
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
        block.passport_attestations.push(first);
        pool.evict_passport_attestations_from_block(&block);
        assert!(pool.get_passport_attestation(&id).is_none());
        assert!(pool.admit_passport_attestation(att(1)).is_ok());
    }
}
