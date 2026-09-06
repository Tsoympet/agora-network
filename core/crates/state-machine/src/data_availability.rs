//! Consensus state for authenticated, provenance-bound data commitments.
//!
//! This module owns deterministic `(source, sequence)` acceptance, operator
//! replay nonces, ceremony-gated activation, and TLT fee attribution. Transport
//! remains a separate concern.

use agora_crypto::verify_data_commitment_bound;
use agora_types::{
    Address, DataCommitmentAuthorization, DataCommitmentSource, Hash, TransactionAcceptance,
    TRIDENT_BLOCK_BODY_VERSION,
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

pub const ACCEPTED_DATA_COMMITMENT_VERSION: u32 = 1;
pub const DATA_AVAILABILITY_POLICY_VERSION: u32 = 1;
pub const DATA_AVAILABILITY_POLICY_DOMAIN: &[u8] = b"agora-trident-da-policy-v1";
pub const DATA_AVAILABILITY_ROOT_DOMAIN: &[u8] = b"agora-trident-da-state-root-v2";

const DATA_AVAILABILITY_PREFIX: &[u8] = b"da/v1/";
const COMMITMENT_PREFIX: &[u8] = b"da/v1/commitment/";
const OPERATOR_NONCE_PREFIX: &[u8] = b"da/v1/operator_nonce/";
const SOURCE_SEQUENCE_PREFIX: &[u8] = b"da/v1/source_sequence/";

/// Ceremony-owned activation, capacity, source, and TLT fee policy.
///
/// Disabled policy is intentionally explicit and carries no latent activation
/// values. Enabled values remain synthetic until a future ceremony freezes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentDataAvailabilityPolicy {
    pub version: u32,
    pub enabled: bool,
    #[serde(default)]
    pub activation_checkpoint: Option<u64>,
    pub activation_block_body_version: u16,
    pub max_commitments_per_block: u32,
    pub max_authorization_bytes_per_block: u32,
    pub base_fee_tlt: u64,
    pub fee_per_authorization_byte_tlt: u64,
    pub fee_per_state_byte_tlt: u64,
    pub allowed_sources: Vec<DataCommitmentSource>,
    pub max_sequence_advance: u64,
}

impl Default for TridentDataAvailabilityPolicy {
    fn default() -> Self {
        Self::disabled()
    }
}

impl TridentDataAvailabilityPolicy {
    pub const fn disabled() -> Self {
        Self {
            version: DATA_AVAILABILITY_POLICY_VERSION,
            enabled: false,
            activation_checkpoint: None,
            activation_block_body_version: TRIDENT_BLOCK_BODY_VERSION,
            max_commitments_per_block: 0,
            max_authorization_bytes_per_block: 0,
            base_fee_tlt: 0,
            fee_per_authorization_byte_tlt: 0,
            fee_per_state_byte_tlt: 0,
            allowed_sources: Vec::new(),
            max_sequence_advance: 0,
        }
    }

    /// Validate stable representation while permitting incomplete enabled drafts.
    pub fn validate_draft(&self) -> Result<(), String> {
        if self.version != DATA_AVAILABILITY_POLICY_VERSION {
            return Err(format!(
                "unsupported data availability policy version {}",
                self.version
            ));
        }
        if self.activation_block_body_version != TRIDENT_BLOCK_BODY_VERSION {
            return Err(format!(
                "DA activation block body version must be {TRIDENT_BLOCK_BODY_VERSION}"
            ));
        }
        if !self
            .allowed_sources
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        {
            return Err("DA allowed_sources must be sorted and unique".into());
        }
        if !self.enabled
            && (self.activation_checkpoint.is_some()
                || self.max_commitments_per_block != 0
                || self.max_authorization_bytes_per_block != 0
                || self.base_fee_tlt != 0
                || self.fee_per_authorization_byte_tlt != 0
                || self.fee_per_state_byte_tlt != 0
                || !self.allowed_sources.is_empty()
                || self.max_sequence_advance != 0)
        {
            return Err("disabled DA policy must not contain latent activation values".into());
        }
        Ok(())
    }

    /// Reject placeholder, unsafe, or arithmetic-overflowing enabled policy.
    pub fn validate_freeze_ready(&self) -> Result<(), String> {
        self.validate_draft()?;
        if !self.enabled {
            return Ok(());
        }

        if self.activation_checkpoint.unwrap_or(0) == 0 {
            return Err("enabled DA activation checkpoint is not selected".into());
        }
        if self.max_commitments_per_block == 0
            || usize::try_from(self.max_commitments_per_block)
                .map_err(|_| "DA commitment limit is not representable".to_string())?
                > agora_consensus::MAX_DATA_COMMITMENTS_PER_BLOCK
        {
            return Err("enabled DA commitment limit is zero or exceeds the hard cap".into());
        }
        if self.max_authorization_bytes_per_block == 0
            || usize::try_from(self.max_authorization_bytes_per_block)
                .map_err(|_| "DA byte limit is not representable".to_string())?
                > agora_consensus::MAX_DATA_COMMITMENT_BYTES_PER_BLOCK
        {
            return Err("enabled DA byte limit is zero or exceeds the hard cap".into());
        }
        if self.base_fee_tlt == 0
            || self.fee_per_authorization_byte_tlt == 0
            || self.fee_per_state_byte_tlt == 0
        {
            return Err("enabled DA TLT fee components must be nonzero".into());
        }
        if self.allowed_sources.is_empty() {
            return Err("enabled DA policy requires an explicit source allowlist".into());
        }
        if self.max_sequence_advance == 0 {
            return Err("enabled DA sequence window must be nonzero".into());
        }

        let commitments = u64::from(self.max_commitments_per_block);
        let authorization_bytes = u64::from(self.max_authorization_bytes_per_block);
        let fixed_growth = max_state_growth_fixed_bytes()?;
        let state_growth_bytes = authorization_bytes
            .checked_add(
                commitments
                    .checked_mul(fixed_growth)
                    .ok_or_else(|| "DA maximum state-growth bytes overflow".to_string())?,
            )
            .ok_or_else(|| "DA maximum state-growth bytes overflow".to_string())?;
        self.fee_for_totals(commitments, authorization_bytes, state_growth_bytes)?;
        Ok(())
    }

    pub fn canonical_hash(&self) -> Hash {
        Hash::hash_borsh(&(DATA_AVAILABILITY_POLICY_DOMAIN, self))
    }

    pub fn is_active_at(&self, blue_score: u64) -> bool {
        self.enabled
            && self
                .activation_checkpoint
                .is_some_and(|activation| blue_score >= activation)
    }

    pub fn allows_source(&self, source: DataCommitmentSource) -> bool {
        self.allowed_sources.binary_search(&source).is_ok()
    }

    pub fn validate_lane_bounds(
        &self,
        authorizations: &[DataCommitmentAuthorization],
    ) -> Result<u64, String> {
        let count = u32::try_from(authorizations.len())
            .map_err(|_| "DA commitment count is not representable".to_string())?;
        if count > self.max_commitments_per_block {
            return Err(format!(
                "too many data commitments for activated policy: {count} > {}",
                self.max_commitments_per_block
            ));
        }
        let mut bytes = 0u64;
        for authorization in authorizations {
            let encoded = borsh::to_vec(authorization).map_err(|error| error.to_string())?;
            bytes = bytes
                .checked_add(
                    u64::try_from(encoded.len())
                        .map_err(|_| "DA authorization length is not representable".to_string())?,
                )
                .ok_or_else(|| "DA authorization byte total overflow".to_string())?;
        }
        if bytes > u64::from(self.max_authorization_bytes_per_block) {
            return Err(format!(
                "data commitment authorizations too large: {bytes} > {}",
                self.max_authorization_bytes_per_block
            ));
        }
        Ok(bytes)
    }

    pub fn minimum_fee_tlt_for_sizes(
        &self,
        authorization_bytes: u64,
        state_growth_bytes: u64,
    ) -> Result<u64, String> {
        self.fee_for_totals(1, authorization_bytes, state_growth_bytes)
    }

    fn fee_for_totals(
        &self,
        commitments: u64,
        authorization_bytes: u64,
        state_growth_bytes: u64,
    ) -> Result<u64, String> {
        self.base_fee_tlt
            .checked_mul(commitments)
            .and_then(|base| {
                self.fee_per_authorization_byte_tlt
                    .checked_mul(authorization_bytes)
                    .and_then(|bytes| base.checked_add(bytes))
            })
            .and_then(|subtotal| {
                self.fee_per_state_byte_tlt
                    .checked_mul(state_growth_bytes)
                    .and_then(|growth| subtotal.checked_add(growth))
            })
            .ok_or_else(|| "DA minimum TLT fee overflow".into())
    }
}

/// Validated policy plus the exact network fingerprint used by authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataAvailabilityRuntimeConfig {
    pub(crate) network_fingerprint: Hash,
    pub(crate) policy: TridentDataAvailabilityPolicy,
}

impl DataAvailabilityRuntimeConfig {
    pub fn new(
        network_fingerprint: Hash,
        policy: TridentDataAvailabilityPolicy,
    ) -> Result<Self, String> {
        let config = Self {
            network_fingerprint,
            policy,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.network_fingerprint == Hash::ZERO {
            return Err("DA runtime network fingerprint must be nonzero".into());
        }
        self.policy.validate_freeze_ready()
    }

    pub const fn network_fingerprint(&self) -> Hash {
        self.network_fingerprint
    }

    pub const fn policy(&self) -> &TridentDataAvailabilityPolicy {
        &self.policy
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataAvailabilityFeeQuote {
    pub authorization_bytes: u64,
    pub state_growth_bytes: u64,
    pub minimum_fee_tlt: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataCommitmentApplyOutcome {
    pub acceptance: TransactionAcceptance,
    pub fee: Option<DataAvailabilityFeeQuote>,
}

impl DataCommitmentApplyOutcome {
    const fn without_fee(acceptance: TransactionAcceptance) -> Self {
        Self {
            acceptance,
            fee: None,
        }
    }
}

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

/// Stable high-water key used to bound forward sequence gaps for one source.
pub fn data_commitment_source_sequence_key(source: DataCommitmentSource) -> Vec<u8> {
    let mut key = Vec::with_capacity(SOURCE_SEQUENCE_PREFIX.len() + 1);
    key.extend_from_slice(SOURCE_SEQUENCE_PREFIX);
    key.push(source.wire_byte());
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

/// Highest accepted sequence for `source`; absence anchors the initial window at zero.
pub fn load_data_commitment_source_sequence(
    store: &StateStore,
    source: DataCommitmentSource,
) -> Result<Option<u64>, StateError> {
    let Some(bytes) = store.get_cf(
        ColumnFamily::Meta,
        &data_commitment_source_sequence_key(source),
    )?
    else {
        return Ok(None);
    };
    if bytes.len() != 8 {
        return Err(StateError::Storage(
            "invalid DA source sequence length".into(),
        ));
    }
    Ok(Some(u64::from_le_bytes(
        bytes.try_into().expect("length checked"),
    )))
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
    runtime: &DataAvailabilityRuntimeConfig,
    accepted_in: Hash,
    batch: &mut WriteBatch,
    meta_before: &mut Vec<(Vec<u8>, Option<Vec<u8>>)>,
) -> Result<DataCommitmentApplyOutcome, StateError> {
    verify_data_commitment_bound(
        authorization,
        l1_chain_id,
        l1_genesis,
        &runtime.network_fingerprint,
    )
    .map_err(|err| StateError::InvalidTx(format!("invalid DA authorization: {err}")))?;

    let source = authorization.commitment.source;
    if !runtime.policy.allows_source(source) {
        return Err(StateError::InvalidTx(format!(
            "DA source {} is not allowed by consensus policy",
            source.wire_byte()
        )));
    }
    let sequence = authorization.commitment.sequence;
    let commitment_key = data_commitment_key(source, sequence);
    if let Some(bytes) = store.get_cf(ColumnFamily::Meta, &commitment_key)? {
        let existing = AcceptedDataCommitment::try_from_slice(&bytes)
            .map_err(|err| StateError::Storage(err.to_string()))?;
        existing.validate_version()?;
        return if existing.authorization.authorization_id() == authorization.authorization_id() {
            Ok(DataCommitmentApplyOutcome::without_fee(
                TransactionAcceptance::ExactDuplicate,
            ))
        } else {
            Ok(DataCommitmentApplyOutcome::without_fee(
                TransactionAcceptance::ConflictLost,
            ))
        };
    }

    let source_sequence = load_data_commitment_source_sequence(store, source)?;
    if !sequence_within_window(
        sequence,
        source_sequence,
        runtime.policy.max_sequence_advance,
    ) {
        return Ok(DataCommitmentApplyOutcome::without_fee(
            TransactionAcceptance::ConflictLost,
        ));
    }

    let expected_nonce = load_data_commitment_nonce(store, &authorization.operator)?;
    if authorization.replay_nonce != expected_nonce {
        return Ok(DataCommitmentApplyOutcome::without_fee(
            TransactionAcceptance::ConflictLost,
        ));
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
    let source_sequence_key = data_commitment_source_sequence_key(source);
    let prior_source_sequence = store.get_cf(ColumnFamily::Meta, &source_sequence_key)?;
    let advances_source_sequence = source_sequence.is_none_or(|high_water| sequence > high_water);

    let authorization_bytes = u64::try_from(
        borsh::to_vec(authorization)
            .map_err(|err| StateError::Storage(err.to_string()))?
            .len(),
    )
    .map_err(|_| StateError::InvalidTx("DA authorization length is not representable".into()))?;
    let state_growth_bytes = accepted_state_growth_bytes(
        commitment_key.len(),
        record_bytes.len(),
        prior_nonce.is_none(),
        advances_source_sequence && prior_source_sequence.is_none(),
    )
    .map_err(StateError::InvalidTx)?;
    let minimum_fee_tlt = runtime
        .policy
        .minimum_fee_tlt_for_sizes(authorization_bytes, state_growth_bytes)
        .map_err(StateError::InvalidTx)?;

    meta_before.push((commitment_key.clone(), None));
    meta_before.push((nonce_key.clone(), prior_nonce));
    batch.put_cf(ColumnFamily::Meta, &commitment_key, &record_bytes);
    batch.put_cf(ColumnFamily::Meta, &nonce_key, &next_nonce.to_le_bytes());
    if advances_source_sequence {
        meta_before.push((source_sequence_key.clone(), prior_source_sequence));
        batch.put_cf(
            ColumnFamily::Meta,
            &source_sequence_key,
            &sequence.to_le_bytes(),
        );
    }
    Ok(DataCommitmentApplyOutcome {
        acceptance: TransactionAcceptance::Accepted,
        fee: Some(DataAvailabilityFeeQuote {
            authorization_bytes,
            state_growth_bytes,
            minimum_fee_tlt,
        }),
    })
}

fn sequence_within_window(
    sequence: u64,
    source_high_water: Option<u64>,
    max_sequence_advance: u64,
) -> bool {
    match source_high_water {
        None => sequence <= max_sequence_advance,
        Some(high_water) if sequence <= high_water => true,
        Some(high_water) => sequence
            .checked_sub(high_water)
            .is_some_and(|advance| advance <= max_sequence_advance),
    }
}

fn accepted_state_growth_bytes(
    commitment_key_bytes: usize,
    record_bytes: usize,
    new_operator_cursor: bool,
    new_source_cursor: bool,
) -> Result<u64, String> {
    let mut bytes = commitment_key_bytes
        .checked_add(record_bytes)
        .ok_or_else(|| "DA state-growth byte total overflow".to_string())?;
    if new_operator_cursor {
        bytes = bytes
            .checked_add(OPERATOR_NONCE_PREFIX.len())
            .and_then(|value| value.checked_add(Address::ZERO.0.len()))
            .and_then(|value| value.checked_add(std::mem::size_of::<u64>()))
            .ok_or_else(|| "DA state-growth byte total overflow".to_string())?;
    }
    if new_source_cursor {
        bytes = bytes
            .checked_add(SOURCE_SEQUENCE_PREFIX.len())
            .and_then(|value| value.checked_add(1))
            .and_then(|value| value.checked_add(std::mem::size_of::<u64>()))
            .ok_or_else(|| "DA state-growth byte total overflow".to_string())?;
    }
    u64::try_from(bytes).map_err(|_| "DA state-growth bytes are not representable".into())
}

fn max_state_growth_fixed_bytes() -> Result<u64, String> {
    [
        COMMITMENT_PREFIX.len(),
        1,
        std::mem::size_of::<u64>(),
        std::mem::size_of::<u32>(),
        Hash::ZERO.as_bytes().len(),
        OPERATOR_NONCE_PREFIX.len(),
        Address::ZERO.0.len(),
        std::mem::size_of::<u64>(),
        SOURCE_SEQUENCE_PREFIX.len(),
        1,
        std::mem::size_of::<u64>(),
    ]
    .into_iter()
    .try_fold(0u64, |total, value| {
        let value = u64::try_from(value)
            .map_err(|_| "DA state-growth bound is not representable".to_string())?;
        total
            .checked_add(value)
            .ok_or_else(|| "DA state-growth bound overflow".to_string())
    })
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
    use agora_types::{DataAvailabilityCommitment, DataCommitmentSource};

    use super::*;

    const CHAIN_ID: &str = "agora-trident-testnet-1";

    fn enabled_runtime(fingerprint: Hash) -> DataAvailabilityRuntimeConfig {
        DataAvailabilityRuntimeConfig::new(
            fingerprint,
            TridentDataAvailabilityPolicy {
                version: DATA_AVAILABILITY_POLICY_VERSION,
                enabled: true,
                activation_checkpoint: Some(10),
                activation_block_body_version: TRIDENT_BLOCK_BODY_VERSION,
                max_commitments_per_block: 8,
                max_authorization_bytes_per_block: 16_384,
                base_fee_tlt: 10,
                fee_per_authorization_byte_tlt: 2,
                fee_per_state_byte_tlt: 3,
                allowed_sources: vec![DataCommitmentSource::AgoraLayersOvolosBatchLab],
                max_sequence_advance: 64,
            },
        )
        .unwrap()
    }

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
        let runtime = enabled_runtime(fingerprint);
        let initial_root = data_availability_root(&store).unwrap();
        let first = signed_authorization(&keypair, 4, 0, 11, &genesis, &fingerprint);
        let mut batch = WriteBatch::new();
        let mut journal = Vec::new();

        let outcome = apply_data_commitment(
            &store,
            &first,
            CHAIN_ID,
            &genesis,
            &runtime,
            Hash([10; 32]),
            &mut batch,
            &mut journal,
        )
        .unwrap();
        assert_eq!(outcome.acceptance, TransactionAcceptance::Accepted);
        let quote = outcome.fee.unwrap();
        assert_eq!(
            quote.minimum_fee_tlt,
            runtime
                .policy
                .minimum_fee_tlt_for_sizes(quote.authorization_bytes, quote.state_growth_bytes)
                .unwrap()
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
            load_data_commitment_source_sequence(&store, first.commitment.source).unwrap(),
            Some(4)
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
                &runtime,
                Hash([12; 32]),
                &mut retry_batch,
                &mut retry_journal,
            )
            .unwrap(),
            DataCommitmentApplyOutcome::without_fee(TransactionAcceptance::ExactDuplicate)
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
                    &runtime,
                    Hash([13; 32]),
                    &mut rejected,
                    &mut rejected_journal,
                )
                .unwrap(),
                DataCommitmentApplyOutcome::without_fee(TransactionAcceptance::ConflictLost)
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
        assert_eq!(
            load_data_commitment_source_sequence(&store, first.commitment.source).unwrap(),
            None
        );
    }

    #[test]
    fn enabled_policy_rejects_fee_overflow_and_sequence_leaps() {
        let fingerprint = Hash([9; 32]);
        let mut runtime = enabled_runtime(fingerprint);
        runtime.policy.base_fee_tlt = u64::MAX;
        assert!(runtime
            .policy
            .minimum_fee_tlt_for_sizes(1, 1)
            .unwrap_err()
            .contains("overflow"));
        assert!(runtime.policy.validate_freeze_ready().is_err());

        let store = StateStore::open_in_memory();
        let keypair = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let genesis = Hash([8; 32]);
        let runtime = enabled_runtime(fingerprint);
        let too_far = signed_authorization(
            &keypair,
            runtime.policy.max_sequence_advance + 1,
            0,
            11,
            &genesis,
            &fingerprint,
        );
        let mut batch = WriteBatch::new();
        let mut journal = Vec::new();
        let outcome = apply_data_commitment(
            &store,
            &too_far,
            CHAIN_ID,
            &genesis,
            &runtime,
            Hash([10; 32]),
            &mut batch,
            &mut journal,
        )
        .unwrap();
        assert_eq!(outcome.acceptance, TransactionAcceptance::ConflictLost);
        assert!(outcome.fee.is_none());
        assert!(batch.is_empty());
        assert!(journal.is_empty());
    }

    #[test]
    fn wrong_network_or_tampering_is_a_hard_error_without_staged_state() {
        let store = StateStore::open_in_memory();
        let keypair = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let genesis = Hash([8; 32]);
        let fingerprint = Hash([9; 32]);
        let runtime = enabled_runtime(fingerprint);
        let authorization = signed_authorization(&keypair, 4, 0, 11, &genesis, &fingerprint);
        let mut batch = WriteBatch::new();
        let mut journal = Vec::new();
        assert!(apply_data_commitment(
            &store,
            &authorization,
            CHAIN_ID,
            &genesis,
            &DataAvailabilityRuntimeConfig {
                network_fingerprint: Hash([10; 32]),
                policy: runtime.policy,
            },
            Hash([11; 32]),
            &mut batch,
            &mut journal,
        )
        .is_err());
        assert!(batch.is_empty());
        assert!(journal.is_empty());
    }
}
