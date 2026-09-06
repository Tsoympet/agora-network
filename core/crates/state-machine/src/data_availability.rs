//! Consensus state for authenticated, provenance-bound data commitments.
//!
//! This module owns deterministic `(source, sequence)` acceptance and operator
//! replay nonces. Transport and fee-policy activation remain separate concerns.

use agora_crypto::verify_data_commitment_bound;
use agora_types::{
    Address, DataCommitmentAuthorization, DataCommitmentSource, Hash, TransactionAcceptance,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

pub const ACCEPTED_DATA_COMMITMENT_VERSION: u32 = 1;
pub const DATA_AVAILABILITY_ROOT_DOMAIN: &[u8] = b"agora-trident-da-state-root-v1";

const DATA_AVAILABILITY_PREFIX: &[u8] = b"da/v1/";
const COMMITMENT_PREFIX: &[u8] = b"da/v1/commitment/";
const OPERATOR_NONCE_PREFIX: &[u8] = b"da/v1/operator_nonce/";

/// Canonical state record retained for provenance and future status queries.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AcceptedDataCommitment {
    pub version: u32,
    pub accepted_in: Hash,
    pub authorization: DataCommitmentAuthorization,
}

impl AcceptedDataCommitment {
    fn validate_version(&self) -> Result<(), StateError> {
        if self.version != ACCEPTED_DATA_COMMITMENT_VERSION {
            return Err(StateError::Storage(format!(
                "unsupported accepted DA commitment version {}",
                self.version
            )));
        }
        Ok(())
    }
}

/// Stable state key for the sole accepted authorization at `(source, sequence)`.
pub fn data_commitment_key(source: DataCommitmentSource, sequence: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(COMMITMENT_PREFIX.len() + 1 + 8);
    key.extend_from_slice(COMMITMENT_PREFIX);
    key.push(source.wire_byte());
    key.extend_from_slice(&sequence.to_be_bytes());
    key
}

/// Stable replay cursor key for one secp256k1-derived operator address.
pub fn data_commitment_nonce_key(operator: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(OPERATOR_NONCE_PREFIX.len() + operator.0.len());
    key.extend_from_slice(OPERATOR_NONCE_PREFIX);
    key.extend_from_slice(&operator.0);
    key
}

pub fn load_data_commitment(
    store: &StateStore,
    source: DataCommitmentSource,
    sequence: u64,
) -> Result<Option<AcceptedDataCommitment>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &data_commitment_key(source, sequence))?
    else {
        return Ok(None);
    };
    let record = AcceptedDataCommitment::try_from_slice(&bytes)
        .map_err(|err| StateError::Storage(err.to_string()))?;
    record.validate_version()?;
    Ok(Some(record))
}

/// The next replay nonce accepted for `operator`; absent state starts at zero.
pub fn load_data_commitment_nonce(
    store: &StateStore,
    operator: &Address,
) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &data_commitment_nonce_key(operator))?
    else {
        return Ok(0);
    };
    if bytes.len() != 8 {
        return Err(StateError::Storage(
            "invalid DA operator nonce length".into(),
        ));
    }
    Ok(u64::from_le_bytes(
        bytes.try_into().expect("length checked"),
    ))
}

/// Verify and stage one commitment without committing the store.
///
/// Invalid structure/auth is always a hard error. Exact signed retries and
/// deterministic source/sequence or nonce conflicts return typed soft outcomes;
/// the caller decides whether its apply mode permits soft skipping.
#[allow(clippy::too_many_arguments)]
pub fn apply_data_commitment(
    store: &StateStore,
    authorization: &DataCommitmentAuthorization,
    l1_chain_id: &str,
    l1_genesis: &Hash,
    l1_network_fingerprint: &Hash,
    accepted_in: Hash,
    batch: &mut WriteBatch,
    meta_before: &mut Vec<(Vec<u8>, Option<Vec<u8>>)>,
) -> Result<TransactionAcceptance, StateError> {
    verify_data_commitment_bound(
        authorization,
        l1_chain_id,
        l1_genesis,
        l1_network_fingerprint,
    )
    .map_err(|err| StateError::InvalidTx(format!("invalid DA authorization: {err}")))?;

    let source = authorization.commitment.source;
    let sequence = authorization.commitment.sequence;
    let commitment_key = data_commitment_key(source, sequence);
    if let Some(bytes) = store.get_cf(ColumnFamily::Meta, &commitment_key)? {
        let existing = AcceptedDataCommitment::try_from_slice(&bytes)
            .map_err(|err| StateError::Storage(err.to_string()))?;
        existing.validate_version()?;
        return if existing.authorization.authorization_id() == authorization.authorization_id() {
            Ok(TransactionAcceptance::ExactDuplicate)
        } else {
            Ok(TransactionAcceptance::ConflictLost)
        };
    }

    let expected_nonce = load_data_commitment_nonce(store, &authorization.operator)?;
    if authorization.replay_nonce != expected_nonce {
        return Ok(TransactionAcceptance::ConflictLost);
    }
    let next_nonce = authorization
        .replay_nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("DA replay nonce exhausted".into()))?;

    let record = AcceptedDataCommitment {
        version: ACCEPTED_DATA_COMMITMENT_VERSION,
        accepted_in,
        authorization: authorization.clone(),
    };
    let record_bytes =
        borsh::to_vec(&record).map_err(|err| StateError::Storage(err.to_string()))?;
    let nonce_key = data_commitment_nonce_key(&authorization.operator);
    let prior_nonce = store.get_cf(ColumnFamily::Meta, &nonce_key)?;

    meta_before.push((commitment_key.clone(), None));
    meta_before.push((nonce_key.clone(), prior_nonce));
    batch.put_cf(ColumnFamily::Meta, &commitment_key, &record_bytes);
    batch.put_cf(ColumnFamily::Meta, &nonce_key, &next_nonce.to_le_bytes());
    Ok(TransactionAcceptance::Accepted)
}

/// Restore DA keys captured before accepted operations, in reverse write order.
pub fn revert_data_commitment_meta_into(
    batch: &mut WriteBatch,
    meta_before: &[(Vec<u8>, Option<Vec<u8>>)],
) {
    for (key, prior) in meta_before.iter().rev() {
        match prior {
            Some(value) => batch.put_cf(ColumnFamily::Meta, key, value),
            None => batch.delete_cf(ColumnFamily::Meta, key),
        }
    }
}

/// Commitment over accepted records and replay cursors, sorted by state key.
pub fn data_availability_root(store: &StateStore) -> Result<Hash, StateError> {
    let entries = store.scan_prefix(ColumnFamily::Meta, DATA_AVAILABILITY_PREFIX)?;
    Ok(Hash::hash_borsh(&(
        DATA_AVAILABILITY_ROOT_DOMAIN,
        ACCEPTED_DATA_COMMITMENT_VERSION,
        entries,
    )))
}

#[cfg(test)]
mod tests {
    use agora_crypto::{sign_data_commitment_bound, KeyPair};
    use agora_types::DataAvailabilityCommitment;

    use super::*;

    const CHAIN_ID: &str = "agora-trident-testnet-1";

    fn signed_authorization(
        keypair: &KeyPair,
        sequence: u64,
        replay_nonce: u64,
        marker: u8,
        genesis: &Hash,
        fingerprint: &Hash,
    ) -> DataCommitmentAuthorization {
        let commitment = DataAvailabilityCommitment::agora_layers_ovolos_batch(
            "agora-ovolos-testnet-1".into(),
            Hash([1; 32]),
            Hash([marker; 32]),
            sequence,
            Hash([3; 32]),
            Hash([marker.wrapping_add(1); 32]),
            Hash([5; 32]),
            6,
            7,
        );
        let mut authorization =
            DataCommitmentAuthorization::unsigned(keypair.address(), replay_nonce, commitment);
        sign_data_commitment_bound(&mut authorization, keypair, CHAIN_ID, genesis, fingerprint)
            .unwrap();
        authorization
    }

    #[test]
    fn acceptance_is_atomic_idempotent_and_replay_bound() {
        let store = StateStore::open_in_memory();
        let keypair = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let genesis = Hash([8; 32]);
        let fingerprint = Hash([9; 32]);
        let initial_root = data_availability_root(&store).unwrap();
        let first = signed_authorization(&keypair, 4, 0, 11, &genesis, &fingerprint);
        let mut batch = WriteBatch::new();
        let mut journal = Vec::new();

        assert_eq!(
            apply_data_commitment(
                &store,
                &first,
                CHAIN_ID,
                &genesis,
                &fingerprint,
                Hash([10; 32]),
                &mut batch,
                &mut journal,
            )
            .unwrap(),
            TransactionAcceptance::Accepted
        );
        // Staging alone cannot expose a half-applied record/nonce.
        assert!(load_data_commitment(&store, first.commitment.source, 4)
            .unwrap()
            .is_none());
        assert_eq!(
            load_data_commitment_nonce(&store, &keypair.address()).unwrap(),
            0
        );
        store.write_batch(batch).unwrap();
        assert_eq!(
            load_data_commitment_nonce(&store, &keypair.address()).unwrap(),
            1
        );
        assert_eq!(
            load_data_commitment(&store, first.commitment.source, 4)
                .unwrap()
                .unwrap()
                .authorization,
            first
        );
        assert_ne!(data_availability_root(&store).unwrap(), initial_root);

        let mut retry_batch = WriteBatch::new();
        let mut retry_journal = Vec::new();
        assert_eq!(
            apply_data_commitment(
                &store,
                &first,
                CHAIN_ID,
                &genesis,
                &fingerprint,
                Hash([12; 32]),
                &mut retry_batch,
                &mut retry_journal,
            )
            .unwrap(),
            TransactionAcceptance::ExactDuplicate
        );
        assert!(retry_batch.is_empty());
        assert!(retry_journal.is_empty());

        let conflict = signed_authorization(&keypair, 4, 1, 12, &genesis, &fingerprint);
        let replay = signed_authorization(&keypair, 5, 0, 13, &genesis, &fingerprint);
        for candidate in [&conflict, &replay] {
            let mut rejected = WriteBatch::new();
            let mut rejected_journal = Vec::new();
            assert_eq!(
                apply_data_commitment(
                    &store,
                    candidate,
                    CHAIN_ID,
                    &genesis,
                    &fingerprint,
                    Hash([13; 32]),
                    &mut rejected,
                    &mut rejected_journal,
                )
                .unwrap(),
                TransactionAcceptance::ConflictLost
            );
            assert!(rejected.is_empty());
            assert!(rejected_journal.is_empty());
        }

        let mut revert = WriteBatch::new();
        revert_data_commitment_meta_into(&mut revert, &journal);
        store.write_batch(revert).unwrap();
        assert_eq!(data_availability_root(&store).unwrap(), initial_root);
        assert!(load_data_commitment(&store, first.commitment.source, 4)
            .unwrap()
            .is_none());
        assert_eq!(
            load_data_commitment_nonce(&store, &keypair.address()).unwrap(),
            0
        );
    }

    #[test]
    fn wrong_network_or_tampering_is_a_hard_error_without_staged_state() {
        let store = StateStore::open_in_memory();
        let keypair = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let genesis = Hash([8; 32]);
        let fingerprint = Hash([9; 32]);
        let authorization = signed_authorization(&keypair, 4, 0, 11, &genesis, &fingerprint);
        let mut batch = WriteBatch::new();
        let mut journal = Vec::new();
        assert!(apply_data_commitment(
            &store,
            &authorization,
            CHAIN_ID,
            &genesis,
            &Hash([10; 32]),
            Hash([11; 32]),
            &mut batch,
            &mut journal,
        )
        .is_err());
        assert!(batch.is_empty());
        assert!(journal.is_empty());
    }
}
