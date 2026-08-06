//! Persist Trident finality certificates and the finalized tip marker.

use agora_types::{FinalityCertificate, Hash};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const CERT_PREFIX: &[u8] = b"finality/cert/";
const FINALIZED_TIP: &[u8] = b"finality/tip_blue_score";

pub fn certificate_key(block_hash: &Hash) -> Vec<u8> {
    let mut k = CERT_PREFIX.to_vec();
    k.extend_from_slice(block_hash.as_bytes());
    k
}

pub fn put_certificate_into(
    batch: &mut WriteBatch,
    cert: &FinalityCertificate,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(cert).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &certificate_key(&cert.body.block_hash),
        &bytes,
    );
    if cert.state.is_finalized() {
        batch.put_cf(
            ColumnFamily::Meta,
            FINALIZED_TIP,
            &cert.body.blue_score.to_le_bytes(),
        );
    }
    Ok(())
}

pub fn load_certificate(
    store: &StateStore,
    block_hash: &Hash,
) -> Result<Option<FinalityCertificate>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &certificate_key(block_hash))? else {
        return Ok(None);
    };
    Ok(Some(
        FinalityCertificate::try_from_slice(&bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?,
    ))
}

pub fn load_finalized_blue_score(store: &StateStore) -> Result<Option<u64>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, FINALIZED_TIP)? else {
        return Ok(None);
    };
    if bytes.len() != 8 {
        return Ok(None);
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    Ok(Some(u64::from_le_bytes(arr)))
}

/// Aggregate signed stake from distinct validator addresses (idempotent set).
#[derive(Debug, Default, Clone, BorshSerialize, BorshDeserialize)]
pub struct AttestationIndex {
    pub ovl_signers: Vec<agora_types::Address>,
    pub drc_signers: Vec<agora_types::Address>,
}

impl AttestationIndex {
    pub fn insert_ovl(&mut self, addr: agora_types::Address) {
        if !self.ovl_signers.contains(&addr) {
            self.ovl_signers.push(addr);
            self.ovl_signers.sort_by(|a, b| a.0.cmp(&b.0));
        }
    }

    pub fn insert_drc(&mut self, addr: agora_types::Address) {
        if !self.drc_signers.contains(&addr) {
            self.drc_signers.push(addr);
            self.drc_signers.sort_by(|a, b| a.0.cmp(&b.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use agora_consensus::{evaluate_checkpoint_state, note_pow_progress, FinalityPowPolicy};
    use agora_types::{CheckpointBody, CheckpointState, FinalityCertificate, Hash};

    use super::*;

    #[test]
    fn persist_finalized_tip() {
        let store = StateStore::open_in_memory();
        let body = CheckpointBody {
            chain_id: "c".into(),
            genesis_hash: Hash::ZERO,
            consensus_policy_hash: Hash::ZERO,
            state_transition_version: "v".into(),
            blue_score: 42,
            block_hash: Hash([7u8; 32]),
            state_root: Hash::ZERO,
            validator_epoch: 1,
        };
        let mut cert = FinalityCertificate::new(body);
        note_pow_progress(&mut cert, 1, &FinalityPowPolicy::default());
        cert.ovl_active_stake = 100;
        cert.ovl_signed_stake = 70;
        cert.drc_active_stake = 100;
        cert.drc_signed_stake = 70;
        cert.state = evaluate_checkpoint_state(true, 70, 100, 70, 100);
        assert_eq!(cert.state, CheckpointState::Finalized);
        let mut batch = WriteBatch::new();
        put_certificate_into(&mut batch, &cert).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(load_finalized_blue_score(&store).unwrap(), Some(42));
        let loaded = load_certificate(&store, &Hash([7u8; 32])).unwrap().unwrap();
        assert!(loaded.state.is_finalized());
    }
}
