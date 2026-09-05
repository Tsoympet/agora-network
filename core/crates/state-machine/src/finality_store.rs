//! Persist Trident finality certificates, attestation indexes, and tip marker.

use agora_types::{Address, CheckpointAttestation, FinalityCertificate, Hash, NativeAssetId};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const CERT_PREFIX: &[u8] = b"finality/cert/";
const IDX_PREFIX: &[u8] = b"finality/idx/";
const LAST_ATT_PREFIX: &[u8] = b"finality/last_att/";
const FINALIZED_TIP: &[u8] = b"finality/tip_blue_score";

pub fn certificate_key(block_hash: &Hash) -> Vec<u8> {
    let mut k = CERT_PREFIX.to_vec();
    k.extend_from_slice(block_hash.as_bytes());
    k
}

fn index_key(block_hash: &Hash) -> Vec<u8> {
    let mut k = IDX_PREFIX.to_vec();
    k.extend_from_slice(block_hash.as_bytes());
    k
}

fn last_attestation_key(set: NativeAssetId, validator: &Address, epoch: u64) -> Vec<u8> {
    let mut k = LAST_ATT_PREFIX.to_vec();
    k.push(set.wire_byte());
    k.push(b'/');
    k.extend_from_slice(&validator.0);
    k.push(b'/');
    k.extend_from_slice(&epoch.to_le_bytes());
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
    pub ovl_signers: Vec<Address>,
    pub drc_signers: Vec<Address>,
}

impl AttestationIndex {
    pub fn insert_ovl(&mut self, addr: Address) {
        if !self.ovl_signers.contains(&addr) {
            self.ovl_signers.push(addr);
            self.ovl_signers.sort_by_key(|address| address.0);
        }
    }

    pub fn insert_drc(&mut self, addr: Address) {
        if !self.drc_signers.contains(&addr) {
            self.drc_signers.push(addr);
            self.drc_signers.sort_by_key(|address| address.0);
        }
    }

    pub fn insert(&mut self, set: NativeAssetId, addr: Address) {
        match set {
            NativeAssetId::OVL => self.insert_ovl(addr),
            NativeAssetId::DRC => self.insert_drc(addr),
            NativeAssetId::TLT => {}
        }
    }

    pub fn signers(&self, set: NativeAssetId) -> &[Address] {
        match set {
            NativeAssetId::OVL => &self.ovl_signers,
            NativeAssetId::DRC => &self.drc_signers,
            NativeAssetId::TLT => &[],
        }
    }
}

pub fn put_attestation_index_into(
    batch: &mut WriteBatch,
    block_hash: &Hash,
    index: &AttestationIndex,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(index).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, &index_key(block_hash), &bytes);
    Ok(())
}

pub fn load_attestation_index(
    store: &StateStore,
    block_hash: &Hash,
) -> Result<AttestationIndex, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &index_key(block_hash))? else {
        return Ok(AttestationIndex::default());
    };
    AttestationIndex::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))
}

pub fn put_last_attestation_into(
    batch: &mut WriteBatch,
    att: &CheckpointAttestation,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(att).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &last_attestation_key(att.set, &att.validator, att.body.validator_epoch),
        &bytes,
    );
    Ok(())
}

pub fn load_last_attestation(
    store: &StateStore,
    set: NativeAssetId,
    validator: &Address,
    epoch: u64,
) -> Result<Option<CheckpointAttestation>, StateError> {
    let Some(bytes) = store.get_cf(
        ColumnFamily::Meta,
        &last_attestation_key(set, validator, epoch),
    )?
    else {
        return Ok(None);
    };
    Ok(Some(
        CheckpointAttestation::try_from_slice(&bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?,
    ))
}

#[cfg(test)]
mod tests {
    use agora_consensus::{evaluate_checkpoint_state, note_pow_progress, FinalityPowPolicy};
    use agora_types::{CheckpointBody, CheckpointState, FinalityCertificate, Hash};

    use super::*;

    #[test]
    fn persist_finalized_tip_and_index() {
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
        let mut idx = AttestationIndex::default();
        idx.insert_ovl(Address([1u8; 20]));
        let mut batch = WriteBatch::new();
        put_certificate_into(&mut batch, &cert).unwrap();
        put_attestation_index_into(&mut batch, &Hash([7u8; 32]), &idx).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(load_finalized_blue_score(&store).unwrap(), Some(42));
        let loaded = load_certificate(&store, &Hash([7u8; 32])).unwrap().unwrap();
        assert!(loaded.state.is_finalized());
        assert_eq!(
            load_attestation_index(&store, &Hash([7u8; 32]))
                .unwrap()
                .ovl_signers
                .len(),
            1
        );
    }
}
