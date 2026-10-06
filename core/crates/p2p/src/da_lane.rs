//! Mempool admission for provenance-bound data commitments.
//!
//! Consensus apply still requires a non-zero DA network fingerprint. This pool
//! only reserves `(source, sequence)` and operator replay slots so gossip can
//! carry signed authorizations without pretending lab `recordDa` is L1 finality.

use agora_types::{Address, DataCommitmentAuthorization, DataCommitmentSource, Hash};

use crate::P2pError;

use super::Mempool;

/// Consensus block cap; templates never select more than this from the pool.
pub const MEMPOOL_DATA_COMMITMENT_TEMPLATE_LIMIT: usize = 64;

fn sequence_key(source: DataCommitmentSource, sequence: u64) -> (u8, u64) {
    (source.wire_byte(), sequence)
}

impl Mempool {
    pub(super) fn data_commitment_maps_len(&self) -> usize {
        self.data_commitments.len()
    }

    pub(super) fn data_commitment_maps_contains(&self, id: &Hash) -> bool {
        self.data_commitments.contains_key(id)
    }

    pub fn get_data_commitment(&self, id: &Hash) -> Option<&DataCommitmentAuthorization> {
        self.data_commitments.get(id)
    }

    pub fn get_data_commitment_by_sequence(
        &self,
        source: DataCommitmentSource,
        sequence: u64,
    ) -> Option<&DataCommitmentAuthorization> {
        let id = self
            .reserved_da_sequences
            .get(&sequence_key(source, sequence))?;
        self.data_commitments.get(id)
    }

    pub fn pending_data_commitment_sequence(
        &self,
        source: DataCommitmentSource,
        sequence: u64,
    ) -> bool {
        self.reserved_da_sequences
            .contains_key(&sequence_key(source, sequence))
    }

    pub fn pending_data_commitment_operator(&self, operator: &Address) -> bool {
        self.reserved_da_operators.contains_key(operator)
    }

    pub fn admit_data_commitment(
        &mut self,
        authorization: DataCommitmentAuthorization,
    ) -> Result<Hash, P2pError> {
        authorization
            .validate()
            .map_err(|err| P2pError::MempoolRejected(format!("invalid data commitment: {err}")))?;
        if authorization.public_key.len() != 33 || authorization.signature.len() != 64 {
            return Err(P2pError::MempoolRejected(
                "data commitment missing secp256k1 auth".into(),
            ));
        }
        let id = authorization.authorization_id();
        if self.data_commitments.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let seq_key = sequence_key(
            authorization.commitment.source,
            authorization.commitment.sequence,
        );
        if self.reserved_da_sequences.contains_key(&seq_key) {
            return Err(P2pError::MempoolRejected(
                "data commitment source sequence already pending".into(),
            ));
        }
        if self
            .reserved_da_operators
            .contains_key(&authorization.operator)
        {
            return Err(P2pError::MempoolRejected(
                "operator already has a pending data commitment nonce".into(),
            ));
        }
        self.reserved_da_sequences.insert(seq_key, id);
        self.reserved_da_operators
            .insert(authorization.operator, id);
        self.data_commitments.insert(id, authorization);
        Ok(id)
    }

    pub fn remove_data_commitment(&mut self, id: &Hash) -> Option<DataCommitmentAuthorization> {
        let authorization = self.data_commitments.remove(id)?;
        self.reserved_da_sequences.remove(&sequence_key(
            authorization.commitment.source,
            authorization.commitment.sequence,
        ));
        self.reserved_da_operators.remove(&authorization.operator);
        Some(authorization)
    }

    /// Deterministic authorization-id order. Inclusion still burns the TLT fee at apply.
    pub fn select_data_commitments(&self, max: usize) -> Vec<DataCommitmentAuthorization> {
        let mut entries: Vec<_> = self.data_commitments.values().cloned().collect();
        entries.sort_by(|a, b| {
            a.authorization_id()
                .as_bytes()
                .cmp(b.authorization_id().as_bytes())
        });
        let cap = max.min(MEMPOOL_DATA_COMMITMENT_TEMPLATE_LIMIT);
        if entries.len() > cap {
            entries.truncate(cap);
        }
        entries
    }

    pub(super) fn evict_data_commitment_lanes_from_block(&mut self, block: &agora_types::Block) {
        let mut consumed_sequences = std::collections::HashSet::new();
        let mut consumed_operators = std::collections::HashSet::new();
        for authorization in &block.data_commitments {
            let id = authorization.authorization_id();
            let _ = self.remove_data_commitment(&id);
            consumed_sequences.insert(sequence_key(
                authorization.commitment.source,
                authorization.commitment.sequence,
            ));
            consumed_operators.insert(authorization.operator);
        }
        let stale: Vec<Hash> = self
            .data_commitments
            .iter()
            .filter_map(|(id, authorization)| {
                let seq = sequence_key(
                    authorization.commitment.source,
                    authorization.commitment.sequence,
                );
                (consumed_sequences.contains(&seq)
                    || consumed_operators.contains(&authorization.operator))
                .then_some(*id)
            })
            .collect();
        for id in stale {
            let _ = self.remove_data_commitment(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use agora_crypto::{sign_data_commitment_bound, KeyPair};
    use agora_types::{Address, DataAvailabilityCommitment, Hash};

    use super::*;
    use crate::Mempool;

    fn signed_auth(marker: u8, sequence: u64, nonce: u64) -> DataCommitmentAuthorization {
        let keypair = KeyPair::from_secret_bytes(&[marker; 32]).unwrap();
        let commitment = DataAvailabilityCommitment::agora_layers_ovolos_batch(
            "agora-ovolos-testnet-1".into(),
            Hash([1; 32]),
            Hash([marker; 32]),
            sequence,
            Hash([3; 32]),
            Hash([4; 32]),
            Hash([5; 32]),
            6,
            7,
        );
        let mut authorization =
            DataCommitmentAuthorization::unsigned(keypair.address(), nonce, commitment);
        sign_data_commitment_bound(
            &mut authorization,
            &keypair,
            "agora-trident-testnet-1",
            &Hash([8; 32]),
            &Hash([9; 32]),
        )
        .unwrap();
        authorization
    }

    #[test]
    fn admits_selects_and_evicts_signed_commitments() {
        let first = signed_auth(11, 4, 0);
        let second = signed_auth(12, 5, 0);
        let mut pool = Mempool::new(8);
        let first_id = pool.admit_data_commitment(first.clone()).unwrap();
        let second_id = pool.admit_data_commitment(second.clone()).unwrap();
        assert_eq!(pool.len(), 2);
        assert!(pool.data_commitment_maps_contains(&first_id));
        assert!(pool.pending_data_commitment_sequence(first.commitment.source, 4));
        let selected = pool.select_data_commitments(8);
        assert_eq!(selected.len(), 2);
        assert!(
            selected[0].authorization_id().as_bytes() <= selected[1].authorization_id().as_bytes()
        );

        let mut block = agora_types::Block::utxo(
            agora_types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.data_commitments.push(first);
        pool.evict_data_commitment_lanes_from_block(&block);
        assert!(pool.get_data_commitment(&first_id).is_none());
        assert!(pool.get_data_commitment(&second_id).is_some());
    }

    #[test]
    fn rejects_unsigned_and_duplicate_slots() {
        let mut pool = Mempool::new(8);
        let unsigned = DataCommitmentAuthorization::unsigned(
            Address([1; 20]),
            0,
            DataAvailabilityCommitment::agora_layers_ovolos_batch(
                "agora-ovolos-testnet-1".into(),
                Hash([1; 32]),
                Hash([2; 32]),
                3,
                Hash([4; 32]),
                Hash([5; 32]),
                Hash([6; 32]),
                7,
                8,
            ),
        );
        assert!(pool.admit_data_commitment(unsigned).is_err());

        let first = signed_auth(13, 1, 0);
        pool.admit_data_commitment(first.clone()).unwrap();
        let conflict = signed_auth(14, 1, 0);
        assert!(pool.admit_data_commitment(conflict).is_err());
        let same_operator = signed_auth(13, 9, 1);
        assert!(pool.admit_data_commitment(same_operator).is_err());
    }
}
