//! Canonical, artifact-only Trident Block 0 state commitment.
//!
//! Preparation can stage a verified Meta envelope for a future atomic loader, but it
//! does not write the caller's datadir, materialize live balances, construct a
//! [`agora_types::Block`], or participate in node boot. The current header has no
//! state-root field, and several committed records still lack lossless runtime-store
//! representations. Keeping those mappings out of this batch prevents a partially
//! seeded Trident node from reaching networking.

use std::collections::{BTreeMap, BTreeSet};

use agora_crypto::{address_from_pubkey, parse_compressed_public_key};
use agora_types::{
    Address, CheckpointState, Hash, NativeAssetId, TreasuryId, TridentHeader, TridentHeaderIdentity,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::{meta_keys, ColumnFamily};
use crate::error::StateError;
use crate::staking::{
    validator_meta_key, StakingParams, ValidatorRecord, ValidatorStatus,
    MAX_VALIDATOR_COMMISSION_BPS,
};
use crate::store::{StateStore, WriteBatch};
use crate::trident_genesis::{
    TridentGenesisArtifact, TridentGenesisValidator, TridentInitialAllocation, TridentTreasury,
    TridentValidatorGenesis, TridentVestingSchedule, TRIDENT_CONSENSUS_POLICY_VERSION,
    TRIDENT_NET_FP_DOMAIN, TRIDENT_PROTOCOL_VERSION, TRIDENT_STATE_TRANSITION_VERSION,
    TRIDENT_TX_SIGNING_VERSION,
};
use crate::TridentDataAvailabilityPolicy;

/// Version of the canonical Block 0 native-state manifest.
///
/// Version 4 adds the ceremony-owned DA activation and TLT fee policy.
pub const TRIDENT_BLOCK_ZERO_STATE_VERSION: u32 = 4;
/// Domain for the complete native-state root.
pub const TRIDENT_BLOCK_ZERO_STATE_DOMAIN: &[u8] = b"agora-trident-block-zero-state-v4";
/// Domain for the value a future Block 0 header must commit.
pub const TRIDENT_BLOCK_ZERO_COMMITMENT_DOMAIN: &[u8] = b"agora-trident-block-zero-commitment-v4";
/// Version of the lossless Meta-CF storage envelope.
///
/// This is independent of [`crate::SCHEMA_VERSION`]: no current boot path
/// writes or consumes these records.
pub const TRIDENT_BLOCK_ZERO_STORAGE_VERSION: u32 = 4;
/// Version of the Borsh identity that binds one datadir to one Trident chain.
pub const TRIDENT_DATADIR_IDENTITY_VERSION: u32 = 1;

const TRIDENT_BLOCK_ZERO_META_KEYS: [&[u8]; 10] = [
    meta_keys::TRIDENT_BLOCK_ZERO_RECORD_VERSION,
    meta_keys::TRIDENT_BLOCK_ZERO_RECORD,
    meta_keys::TRIDENT_BLOCK_ZERO_STATE_PAYLOAD,
    meta_keys::TRIDENT_BLOCK_ZERO_STATE_ROOT,
    meta_keys::TRIDENT_BLOCK_ZERO_COMMITMENT,
    meta_keys::TRIDENT_BLOCK_ZERO_COMMITMENT_HASH,
    meta_keys::TRIDENT_BLOCK_ZERO_ARTIFACT_IDENTITY,
    meta_keys::TRIDENT_BLOCK_ZERO_CONSENSUS_POLICY_HASH,
    meta_keys::TRIDENT_BLOCK_ZERO_NETWORK_FINGERPRINT,
    meta_keys::TRIDENT_BLOCK_ZERO_CHAIN_ID,
];
const TRIDENT_DATADIR_IDENTITY_META_KEYS: [&[u8]; 2] = [
    meta_keys::TRIDENT_DATADIR_IDENTITY_VERSION,
    meta_keys::TRIDENT_DATADIR_IDENTITY,
];

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockZeroAllocation {
    pub asset: NativeAssetId,
    pub address: Address,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockZeroVesting {
    pub asset: NativeAssetId,
    pub address: Address,
    pub amount: u64,
    pub start_timestamp_ms: u64,
    pub cliff_timestamp_ms: u64,
    pub end_timestamp_ms: u64,
}

/// Supply buckets are mutually exclusive except that validator bonds and vesting
/// are locks inside `genesis_allocated`, not additional issuance.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockZeroSupply {
    pub asset: NativeAssetId,
    pub max_supply: u64,
    pub genesis_allocated: u64,
    pub treasury: u64,
    pub staking_reward_reserve: u64,
    pub unissued: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockZeroTreasury {
    pub treasury: TreasuryId,
    pub asset: NativeAssetId,
    pub control: String,
    pub balance: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockZeroValidator {
    /// Deterministically derived from the compressed secp256k1 consensus key.
    pub operator: Address,
    pub consensus_public_key: [u8; 33],
    pub withdrawal_address: Address,
    pub self_bond: u64,
    pub commission_bps: u16,
    pub metadata_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockZeroValidatorSet {
    pub asset: NativeAssetId,
    pub epoch: u64,
    pub max_validators: u32,
    pub min_self_bond: u64,
    pub unbonding_period_checkpoints: u64,
    pub max_commission_bps: u16,
    pub max_concentration_bps: u16,
    pub validators: Vec<BlockZeroValidator>,
    pub total_active_stake: u64,
}

impl BlockZeroValidator {
    /// Losslessly project a committed Block 0 validator into the existing
    /// staking record without writing it to live state.
    pub fn to_runtime_validator_record(
        &self,
        params: &StakingParams,
    ) -> Result<ValidatorRecord, String> {
        let record = ValidatorRecord {
            operator: self.operator,
            consensus_pubkey: self.consensus_public_key.to_vec(),
            withdrawal: self.withdrawal_address,
            self_bond: self.self_bond,
            delegated: 0,
            commission_bps: self.commission_bps,
            status: ValidatorStatus::Bonded,
            jailed_until_epoch: 0,
            metadata_hash: self.metadata_hash,
        };
        record
            .validate_genesis_registration(params)
            .map_err(|error| error.to_string())?;
        Ok(record)
    }
}

impl BlockZeroValidatorSet {
    /// Verify set policy equality and derive the exact runtime records and
    /// unchanged asset-scoped staking keys for a future atomic materializer.
    pub fn to_runtime_validator_entries(
        &self,
        params: &StakingParams,
    ) -> Result<Vec<(Vec<u8>, ValidatorRecord)>, String> {
        params.validate().map_err(|error| error.to_string())?;
        if self.epoch != 0
            || self.validators.is_empty()
            || self.validators.len() > self.max_validators as usize
            || !self
                .validators
                .windows(2)
                .all(|pair| pair[0].operator.0 < pair[1].operator.0)
            || params.asset != self.asset
            || params.max_validators != self.max_validators
            || params.min_self_bond != self.min_self_bond
            || params.unbonding_period_epochs != self.unbonding_period_checkpoints
            || params.max_commission_bps != self.max_commission_bps
            || params.max_concentration_bps != self.max_concentration_bps
        {
            return Err(format!(
                "{} Block 0 validator policy does not match runtime staking parameters",
                self.asset
            ));
        }

        let mut entries = Vec::with_capacity(self.validators.len());
        let mut keys = BTreeSet::new();
        let mut total_active_stake = 0u64;
        for validator in &self.validators {
            let record = validator.to_runtime_validator_record(params)?;
            total_active_stake = total_active_stake
                .checked_add(record.self_bond)
                .ok_or_else(|| format!("{} runtime validator stake overflow", self.asset))?;
            let (key, bytes) = record
                .canonical_genesis_storage_entry(params)
                .map_err(|error| error.to_string())?;
            if key != validator_meta_key(self.asset, &validator.operator)
                || !keys.insert(key.clone())
            {
                return Err(format!(
                    "{} Block 0 validator storage key is inconsistent",
                    self.asset
                ));
            }
            let decoded = ValidatorRecord::try_from_slice(&bytes)
                .map_err(|error| format!("invalid runtime validator bytes: {error}"))?;
            if decoded != record {
                return Err("runtime validator conversion changed its Borsh identity".into());
            }
            entries.push((key, record));
        }
        if total_active_stake != self.total_active_stake {
            return Err(format!(
                "{} runtime validator stake total mismatch",
                self.asset
            ));
        }
        Ok(entries)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockZeroFinality {
    pub state: CheckpointState,
    pub pow_work_met: bool,
    pub finalized_blue_score: Option<u64>,
    pub ovl_snapshot: Hash,
    pub ovl_active_stake: u64,
    pub ovl_signed_stake: u64,
    pub drc_snapshot: Hash,
    pub drc_active_stake: u64,
    pub drc_signed_stake: u64,
}

/// Complete canonical input to the future atomic Block 0 transition.
///
/// Vectors are sorted by stable wire identities before hashing. This means JSON
/// list order cannot change consensus identity while every value remains covered.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentBlockZeroState {
    pub version: u32,
    pub chain_id: String,
    pub network_fingerprint: Hash,
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
    pub data_availability: TridentDataAvailabilityPolicy,
    pub governance_constitution_hash: Hash,
    pub emergency_policy_hash: Hash,
    pub allocations: Vec<BlockZeroAllocation>,
    pub vesting: Vec<BlockZeroVesting>,
    pub supplies: Vec<BlockZeroSupply>,
    pub treasuries: Vec<BlockZeroTreasury>,
    pub validator_sets: Vec<BlockZeroValidatorSet>,
    pub finality: BlockZeroFinality,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentBlockZeroCommitment {
    pub version: u32,
    pub chain_id: String,
    pub network_fingerprint: Hash,
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
    pub state_root: Hash,
}

/// Header-level network identity available before ceremony-owned header fields.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentDatadirHeaderIdentity {
    pub protocol_version: u32,
    pub state_transition_version: String,
    pub block_zero_commitment: Hash,
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
}

/// Fail-closed identity binding for a Trident datadir.
///
/// The complete header hash is optional until the concrete body, timestamp,
/// difficulty, and nonce are ceremony-selected. The header's chain identity is
/// already available and mandatory.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentDatadirIdentity {
    pub version: u32,
    pub chain_id: String,
    pub network_fingerprint: Hash,
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
    pub block_zero_commitment: Hash,
    pub committed_state_root: Hash,
    pub header_identity: TridentDatadirHeaderIdentity,
    pub block_zero_header_hash: Option<Hash>,
}

/// Lossless storage envelope for a candidate Block 0 manifest.
///
/// Redundant identities are intentional: the checked reader verifies every
/// copy and the exact canonical payload before a future loader can append this
/// batch to live-state writes.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentBlockZeroStorageRecord {
    pub version: u32,
    pub manifest: TridentBlockZeroState,
    pub canonical_payload: Vec<u8>,
    pub commitment: TridentBlockZeroCommitment,
    pub commitment_hash: Hash,
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
    pub network_fingerprint: Hash,
    pub chain_id: String,
    pub datadir_identity: TridentDatadirIdentity,
}

impl TridentBlockZeroState {
    /// Derive a complete manifest only after the artifact passes freeze checks.
    pub fn from_artifact(artifact: &TridentGenesisArtifact) -> Result<Self, String> {
        artifact.validate_freeze_ready()?;

        let mut allocations = artifact
            .initial_allocations
            .iter()
            .map(|entry| allocation_from_artifact(artifact, entry))
            .collect::<Result<Vec<_>, _>>()?;
        allocations.sort_by_key(|entry| (entry.asset.wire_byte(), entry.address.0));
        reject_duplicate_allocations(&allocations)?;

        let mut vesting = artifact
            .vesting_schedules
            .iter()
            .map(|entry| vesting_from_artifact(artifact, entry))
            .collect::<Result<Vec<_>, _>>()?;
        vesting.sort_by_key(|entry| {
            (
                entry.asset.wire_byte(),
                entry.address.0,
                entry.start_timestamp_ms,
                entry.cliff_timestamp_ms,
                entry.end_timestamp_ms,
            )
        });
        verify_vesting_is_funded(&allocations, &vesting)?;

        let supplies = vec![
            supply_from_policy(NativeAssetId::TLT, &artifact.assets.tlt)?,
            supply_from_policy(NativeAssetId::OVL, &artifact.assets.ovl)?,
            supply_from_policy(NativeAssetId::DRC, &artifact.assets.drc)?,
        ];
        verify_allocation_supply(&allocations, &supplies)?;

        let treasuries = vec![
            treasury_from_artifact(TreasuryId::TltSecurity, &artifact.treasuries.tlt_security)?,
            treasury_from_artifact(TreasuryId::OvlBuilder, &artifact.treasuries.ovl_builder)?,
            treasury_from_artifact(TreasuryId::DrcCommunity, &artifact.treasuries.drc_community)?,
        ];
        verify_treasury_supply(&treasuries, &supplies)?;

        let ovl = validator_set_from_artifact(
            artifact,
            NativeAssetId::OVL,
            &artifact.ovl_validators,
            &allocations,
        )?;
        let drc = validator_set_from_artifact(
            artifact,
            NativeAssetId::DRC,
            &artifact.drc_validators,
            &allocations,
        )?;
        let finality = BlockZeroFinality {
            state: CheckpointState::Proposed,
            pow_work_met: false,
            finalized_blue_score: None,
            ovl_snapshot: Hash::hash_borsh(&ovl),
            ovl_active_stake: ovl.total_active_stake,
            ovl_signed_stake: 0,
            drc_snapshot: Hash::hash_borsh(&drc),
            drc_active_stake: drc.total_active_stake,
            drc_signed_stake: 0,
        };

        let state = Self {
            version: TRIDENT_BLOCK_ZERO_STATE_VERSION,
            chain_id: artifact.chain_id.clone(),
            network_fingerprint: parse_hash("network_fingerprint", &artifact.network_fingerprint)?,
            artifact_identity: artifact.consensus_identity_hash(),
            consensus_policy_hash: artifact.consensus_policy_hash(),
            data_availability: artifact.data_availability.clone(),
            governance_constitution_hash: parse_hash(
                "governance_constitution_hash",
                &artifact.governance_constitution_hash,
            )?,
            emergency_policy_hash: parse_hash(
                "emergency_policy_hash",
                &artifact.emergency_policy_hash,
            )?,
            allocations,
            vesting,
            supplies,
            treasuries,
            validator_sets: vec![ovl, drc],
            finality,
        };
        state.verify()?;
        Ok(state)
    }

    pub fn state_root(&self) -> Hash {
        Hash::hash_borsh(&(TRIDENT_BLOCK_ZERO_STATE_DOMAIN, self))
    }

    pub fn commitment(&self) -> TridentBlockZeroCommitment {
        TridentBlockZeroCommitment {
            version: self.version,
            chain_id: self.chain_id.clone(),
            network_fingerprint: self.network_fingerprint,
            artifact_identity: self.artifact_identity,
            consensus_policy_hash: self.consensus_policy_hash,
            state_root: self.state_root(),
        }
    }

    /// Verify canonical ordering, supply conservation, stake funding, and the
    /// initial fail-closed finality state without storage or network I/O.
    pub fn verify(&self) -> Result<(), String> {
        if self.version != TRIDENT_BLOCK_ZERO_STATE_VERSION {
            return Err("unsupported Trident Block 0 state version".into());
        }
        if self.chain_id.trim().is_empty() {
            return Err("Block 0 chain ID must be nonempty".into());
        }
        if self.artifact_identity == Hash::ZERO
            || self.consensus_policy_hash == Hash::ZERO
            || self.network_fingerprint == Hash::ZERO
        {
            return Err("Block 0 identities must be nonzero".into());
        }
        self.data_availability.validate_freeze_ready()?;
        if self.network_fingerprint
            != expected_network_fingerprint(
                &self.chain_id,
                &self.artifact_identity,
                &self.consensus_policy_hash,
            )
        {
            return Err("Block 0 network fingerprint is inconsistent".into());
        }
        if !is_sorted_allocations(&self.allocations) {
            return Err("Block 0 allocations are not canonically sorted".into());
        }
        reject_duplicate_allocations(&self.allocations)?;
        if !is_sorted_vesting(&self.vesting) {
            return Err("Block 0 vesting schedules are not canonically sorted".into());
        }
        verify_vesting_is_funded(&self.allocations, &self.vesting)?;
        verify_allocation_supply(&self.allocations, &self.supplies)?;
        verify_treasury_supply(&self.treasuries, &self.supplies)?;
        if self.validator_sets.len() != 2
            || self.validator_sets[0].asset != NativeAssetId::OVL
            || self.validator_sets[1].asset != NativeAssetId::DRC
        {
            return Err("Block 0 requires ordered independent OVL and DRC sets".into());
        }
        for set in &self.validator_sets {
            verify_validator_set(set, &self.allocations)?;
        }
        let ovl = &self.validator_sets[0];
        let drc = &self.validator_sets[1];
        if self.finality.state != CheckpointState::Proposed
            || self.finality.pow_work_met
            || self.finality.finalized_blue_score.is_some()
            || self.finality.ovl_signed_stake != 0
            || self.finality.drc_signed_stake != 0
            || self.finality.ovl_snapshot != Hash::hash_borsh(ovl)
            || self.finality.drc_snapshot != Hash::hash_borsh(drc)
            || self.finality.ovl_active_stake != ovl.total_active_stake
            || self.finality.drc_active_stake != drc.total_active_stake
        {
            return Err("Block 0 initial finality state is inconsistent".into());
        }
        self.commitment().verify()?;
        Ok(())
    }

    /// Round-trip the exact Borsh payload and verify it before any future writer
    /// is allowed to place it in an atomic storage batch.
    pub fn verified_borsh_payload(&self) -> Result<Vec<u8>, String> {
        self.verify()?;
        let bytes = borsh::to_vec(self).map_err(|error| error.to_string())?;
        let decoded = Self::try_from_slice(&bytes).map_err(|error| error.to_string())?;
        if decoded != *self || decoded.state_root() != self.state_root() {
            return Err("Block 0 Borsh round-trip changed the state commitment".into());
        }
        Ok(bytes)
    }

    /// Stage the lossless Meta envelope in one batch, apply it to a copy-on-write
    /// overlay, and reread the exact bytes before returning a future-loader batch.
    ///
    /// This does not write the caller's store, materialize live balances, or
    /// construct a Block 0 header. Current v2 ignition never calls it.
    pub fn stage_verified_store_batch(&self, store: &StateStore) -> Result<WriteBatch, StateError> {
        self.prepare_verified_store_batch(store, None)
            .map(|(_, batch)| batch)
    }

    /// Stage Block 0 with the hash of a fully specified offline header.
    pub fn stage_verified_store_batch_with_header(
        &self,
        store: &StateStore,
        header: &TridentHeader,
    ) -> Result<WriteBatch, StateError> {
        self.prepare_verified_store_batch(store, Some(header))
            .map(|(_, batch)| batch)
    }

    /// Atomically persist the verified envelope and datadir identity, then
    /// reread their exact bytes from the store.
    pub fn persist_verified_store_record(
        &self,
        store: &StateStore,
    ) -> Result<TridentBlockZeroStorageRecord, StateError> {
        self.persist_verified_store_record_inner(store, None)
    }

    /// Atomically persist the verified envelope with a concrete header hash.
    pub fn persist_verified_store_record_with_header(
        &self,
        store: &StateStore,
        header: &TridentHeader,
    ) -> Result<TridentBlockZeroStorageRecord, StateError> {
        self.persist_verified_store_record_inner(store, Some(header))
    }

    fn prepare_verified_store_batch(
        &self,
        store: &StateStore,
        header: Option<&TridentHeader>,
    ) -> Result<(TridentBlockZeroStorageRecord, WriteBatch), StateError> {
        ensure_block_zero_absent(store)?;
        let record = TridentBlockZeroStorageRecord::from_state_and_header(self, header)
            .map_err(StateError::Storage)?;
        let batch = encode_block_zero_batch(&record)?;
        let overlay = store.cow_overlay();
        overlay.write_batch(batch.clone())?;
        reread_staged_block_zero(&overlay, &record)?;
        Ok((record, batch))
    }

    fn persist_verified_store_record_inner(
        &self,
        store: &StateStore,
        header: Option<&TridentHeader>,
    ) -> Result<TridentBlockZeroStorageRecord, StateError> {
        let (expected, batch) = self.prepare_verified_store_batch(store, header)?;
        store.write_batch(batch)?;
        let loaded = load_verified_trident_block_zero(store)?;
        if loaded != expected {
            return Err(storage_err(
                "Block 0 durable reread changed the staged storage record",
            ));
        }
        verify_trident_datadir_identity(store, &expected.datadir_identity)?;
        Ok(loaded)
    }
}

impl TridentBlockZeroCommitment {
    pub fn hash(&self) -> Hash {
        Hash::hash_borsh(&(TRIDENT_BLOCK_ZERO_COMMITMENT_DOMAIN, self))
    }

    /// Verify the self-contained identities before deriving an offline header.
    ///
    /// Equality to a concrete manifest is additionally checked by
    /// [`TridentBlockZeroStorageRecord::verify`].
    pub fn verify(&self) -> Result<(), String> {
        if self.version != TRIDENT_BLOCK_ZERO_STATE_VERSION {
            return Err("unsupported Trident Block 0 commitment version".into());
        }
        if self.chain_id.trim().is_empty() {
            return Err("Block 0 commitment chain ID must be nonempty".into());
        }
        if self.network_fingerprint == Hash::ZERO
            || self.artifact_identity == Hash::ZERO
            || self.consensus_policy_hash == Hash::ZERO
            || self.state_root == Hash::ZERO
        {
            return Err("Block 0 commitment identities and state root must be nonzero".into());
        }
        if self.network_fingerprint
            != expected_network_fingerprint(
                &self.chain_id,
                &self.artifact_identity,
                &self.consensus_policy_hash,
            )
        {
            return Err("Block 0 commitment network fingerprint is inconsistent".into());
        }
        Ok(())
    }

    /// Identity fields a Trident header must repeat for this Block 0.
    pub fn trident_header_identity(&self) -> Result<TridentHeaderIdentity, String> {
        self.verify()?;
        let identity = TridentHeaderIdentity {
            protocol_version: TRIDENT_PROTOCOL_VERSION,
            state_transition_version: TRIDENT_STATE_TRANSITION_VERSION.into(),
            block_zero_commitment: self.hash(),
            artifact_identity: self.artifact_identity,
            consensus_policy_hash: self.consensus_policy_hash,
        };
        identity.validate().map_err(|error| error.to_string())?;
        Ok(identity)
    }

    /// Build an offline Block 0 header without defaulting ceremony-owned fields.
    ///
    /// The caller must supply timestamp, difficulty, nonce, and a nonzero root
    /// for the separately specified concrete Block 0 body.
    pub fn to_offline_trident_header(
        &self,
        timestamp_ms: u64,
        bits: u32,
        nonce: u64,
        body_root: Hash,
    ) -> Result<TridentHeader, String> {
        TridentHeader::new(
            self.trident_header_identity()?,
            Vec::new(),
            timestamp_ms,
            bits,
            nonce,
            body_root,
            self.state_root,
        )
        .map_err(|error| error.to_string())
    }

    /// Recheck a decoded offline Block 0 header against this commitment.
    pub fn verify_offline_trident_header(
        &self,
        header: &TridentHeader,
        expected_body_root: Hash,
    ) -> Result<(), String> {
        self.verify()?;
        if !header.parents.is_empty() {
            return Err("Trident Block 0 header must not have parents".into());
        }
        header
            .verify_against(
                &self.trident_header_identity()?,
                expected_body_root,
                self.state_root,
            )
            .map_err(|error| error.to_string())
    }
}

impl TridentDatadirHeaderIdentity {
    fn from_header_identity(identity: &TridentHeaderIdentity) -> Self {
        Self {
            protocol_version: identity.protocol_version,
            state_transition_version: identity.state_transition_version.clone(),
            block_zero_commitment: identity.block_zero_commitment,
            artifact_identity: identity.artifact_identity,
            consensus_policy_hash: identity.consensus_policy_hash,
        }
    }

    fn as_header_identity(&self) -> TridentHeaderIdentity {
        TridentHeaderIdentity {
            protocol_version: self.protocol_version,
            state_transition_version: self.state_transition_version.clone(),
            block_zero_commitment: self.block_zero_commitment,
            artifact_identity: self.artifact_identity,
            consensus_policy_hash: self.consensus_policy_hash,
        }
    }

    fn verify(&self) -> Result<(), String> {
        self.as_header_identity()
            .validate()
            .map_err(|error| error.to_string())?;
        if self.protocol_version != TRIDENT_PROTOCOL_VERSION {
            return Err("Trident datadir header protocol version mismatch".into());
        }
        if self.state_transition_version != TRIDENT_STATE_TRANSITION_VERSION {
            return Err("Trident datadir header state-transition version mismatch".into());
        }
        Ok(())
    }
}

impl TridentDatadirIdentity {
    /// Bind a verified Block 0 commitment to the header identity currently
    /// available. A complete header additionally binds its canonical hash.
    pub fn from_block_zero(
        commitment: &TridentBlockZeroCommitment,
        header: Option<&TridentHeader>,
    ) -> Result<Self, String> {
        commitment.verify()?;
        let header_identity = commitment.trident_header_identity()?;
        let block_zero_header_hash = if let Some(header) = header {
            commitment.verify_offline_trident_header(header, header.body_root)?;
            Some(
                header
                    .commitment_hash()
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        let identity = Self {
            version: TRIDENT_DATADIR_IDENTITY_VERSION,
            chain_id: commitment.chain_id.clone(),
            network_fingerprint: commitment.network_fingerprint,
            artifact_identity: commitment.artifact_identity,
            consensus_policy_hash: commitment.consensus_policy_hash,
            block_zero_commitment: commitment.hash(),
            committed_state_root: commitment.state_root,
            header_identity: TridentDatadirHeaderIdentity::from_header_identity(&header_identity),
            block_zero_header_hash,
        };
        identity.verify()?;
        Ok(identity)
    }

    /// Validate every redundant identity without consulting storage.
    pub fn verify(&self) -> Result<(), String> {
        if self.version != TRIDENT_DATADIR_IDENTITY_VERSION {
            return Err("unsupported Trident datadir identity version".into());
        }
        if self.chain_id.trim().is_empty() {
            return Err("Trident datadir chain ID must be nonempty".into());
        }
        if self.network_fingerprint == Hash::ZERO
            || self.artifact_identity == Hash::ZERO
            || self.consensus_policy_hash == Hash::ZERO
            || self.block_zero_commitment == Hash::ZERO
            || self.committed_state_root == Hash::ZERO
        {
            return Err("Trident datadir identities and state root must be nonzero".into());
        }
        if self.network_fingerprint
            != expected_network_fingerprint(
                &self.chain_id,
                &self.artifact_identity,
                &self.consensus_policy_hash,
            )
        {
            return Err("Trident datadir network fingerprint is inconsistent".into());
        }
        self.header_identity.verify()?;
        if self.header_identity.block_zero_commitment != self.block_zero_commitment
            || self.header_identity.artifact_identity != self.artifact_identity
            || self.header_identity.consensus_policy_hash != self.consensus_policy_hash
        {
            return Err("Trident datadir header identity is inconsistent".into());
        }
        if self.block_zero_header_hash == Some(Hash::ZERO) {
            return Err("Trident datadir Block 0 header hash must be nonzero".into());
        }
        Ok(())
    }

    /// Deterministic bytes used for expected/actual startup comparison.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.verify()?;
        let bytes = borsh::to_vec(self).map_err(|error| error.to_string())?;
        let decoded = Self::try_from_slice(&bytes).map_err(|error| error.to_string())?;
        if decoded != *self {
            return Err("Trident datadir identity Borsh round-trip changed the record".into());
        }
        Ok(bytes)
    }

    fn verify_against_block_zero(
        &self,
        state: &TridentBlockZeroState,
        commitment: &TridentBlockZeroCommitment,
    ) -> Result<(), String> {
        self.verify()?;
        if self.chain_id != state.chain_id
            || self.network_fingerprint != state.network_fingerprint
            || self.artifact_identity != state.artifact_identity
            || self.consensus_policy_hash != state.consensus_policy_hash
            || self.block_zero_commitment != commitment.hash()
            || self.committed_state_root != state.state_root()
        {
            return Err("Trident datadir identity does not match Block 0".into());
        }
        let expected_header = commitment.trident_header_identity()?;
        if self.header_identity
            != TridentDatadirHeaderIdentity::from_header_identity(&expected_header)
        {
            return Err("Trident datadir header identity does not match Block 0".into());
        }
        Ok(())
    }
}

impl TridentBlockZeroStorageRecord {
    pub fn from_state(state: &TridentBlockZeroState) -> Result<Self, String> {
        Self::from_state_and_header(state, None)
    }

    pub fn from_state_and_header(
        state: &TridentBlockZeroState,
        header: Option<&TridentHeader>,
    ) -> Result<Self, String> {
        let canonical_payload = state.verified_borsh_payload()?;
        let commitment = state.commitment();
        let commitment_hash = commitment.hash();
        let datadir_identity = TridentDatadirIdentity::from_block_zero(&commitment, header)?;
        let record = Self {
            version: TRIDENT_BLOCK_ZERO_STORAGE_VERSION,
            manifest: state.clone(),
            canonical_payload,
            commitment,
            commitment_hash,
            artifact_identity: state.artifact_identity,
            consensus_policy_hash: state.consensus_policy_hash,
            network_fingerprint: state.network_fingerprint,
            chain_id: state.chain_id.clone(),
            datadir_identity,
        };
        record.verify()?;
        Ok(record)
    }

    pub fn verify(&self) -> Result<(), String> {
        if self.version != TRIDENT_BLOCK_ZERO_STORAGE_VERSION {
            return Err("unsupported Trident Block 0 storage version".into());
        }
        self.manifest.verify()?;
        self.commitment.verify()?;
        let expected_payload = self.manifest.verified_borsh_payload()?;
        if self.canonical_payload != expected_payload {
            return Err("Block 0 canonical payload does not match the manifest".into());
        }
        let expected_commitment = self.manifest.commitment();
        if self.commitment != expected_commitment {
            return Err("Block 0 stored commitment does not match the manifest".into());
        }
        if self.commitment_hash != expected_commitment.hash() {
            return Err("Block 0 commitment hash mismatch".into());
        }
        if self.artifact_identity != self.manifest.artifact_identity
            || self.consensus_policy_hash != self.manifest.consensus_policy_hash
            || self.network_fingerprint != self.manifest.network_fingerprint
            || self.chain_id != self.manifest.chain_id
        {
            return Err("Block 0 stored identities are inconsistent".into());
        }
        if self.commitment.chain_id != self.chain_id
            || self.commitment.network_fingerprint != self.network_fingerprint
            || self.commitment.artifact_identity != self.artifact_identity
            || self.commitment.consensus_policy_hash != self.consensus_policy_hash
        {
            return Err("Block 0 commitment identities are inconsistent".into());
        }
        self.datadir_identity
            .verify_against_block_zero(&self.manifest, &self.commitment)?;
        Ok(())
    }
}

/// Checked reader for a future loader. Current boot never accepts this record.
pub fn load_verified_trident_block_zero(
    store: &StateStore,
) -> Result<TridentBlockZeroStorageRecord, StateError> {
    let values = require_block_zero_values(store)?;
    let identity_values = require_datadir_identity_values(store)?;
    decode_and_verify_block_zero(&values, &identity_values)
}

/// Future Trident startup preflight: require a fully verified Block 0 envelope
/// and compare the expected datadir identity to the stored bytes exactly.
pub fn verify_trident_datadir_identity(
    store: &StateStore,
    expected: &TridentDatadirIdentity,
) -> Result<TridentDatadirIdentity, StateError> {
    let expected_bytes = expected.canonical_bytes().map_err(storage_err)?;
    let record = load_verified_trident_block_zero(store)?;
    let identity_values = require_datadir_identity_values(store)?;
    require_exact_bytes(
        &identity_values,
        meta_keys::TRIDENT_DATADIR_IDENTITY,
        &expected_bytes,
        "expected Trident datadir identity",
    )?;
    if record.datadir_identity != *expected {
        return Err(storage_err(
            "Trident datadir identity does not match the expected chain",
        ));
    }
    Ok(record.datadir_identity)
}

/// Legacy/v2 startup must reject rather than ignore any complete or partial
/// Trident Block 0/datadir identity marker.
pub fn ensure_legacy_v2_datadir(store: &StateStore) -> Result<(), StateError> {
    let has_block_zero = !store
        .scan_prefix(ColumnFamily::Meta, BLOCK_ZERO_PREFIX)?
        .is_empty();
    let has_datadir_identity = !store
        .scan_prefix(ColumnFamily::Meta, DATADIR_IDENTITY_PREFIX)?
        .is_empty();
    if has_block_zero || has_datadir_identity {
        return Err(storage_err(
            "Trident datadir identity is present; legacy/v2 startup refuses this datadir",
        ));
    }
    Ok(())
}

const BLOCK_ZERO_PREFIX: &[u8] = b"meta/trident_block_zero/";
const DATADIR_IDENTITY_PREFIX: &[u8] = b"meta/trident_datadir_identity/";

type BlockZeroEncodedValues = Vec<(&'static [u8], Vec<u8>)>;

fn storage_err(message: impl Into<String>) -> StateError {
    StateError::Storage(message.into())
}

fn expected_network_fingerprint(
    chain_id: &str,
    artifact_identity: &Hash,
    consensus_policy_hash: &Hash,
) -> Hash {
    Hash::hash_borsh(&(
        TRIDENT_NET_FP_DOMAIN,
        TRIDENT_PROTOCOL_VERSION,
        chain_id,
        artifact_identity.as_bytes(),
        consensus_policy_hash.as_bytes(),
        TRIDENT_TX_SIGNING_VERSION,
        TRIDENT_STATE_TRANSITION_VERSION,
        TRIDENT_CONSENSUS_POLICY_VERSION,
    ))
}

fn ensure_block_zero_absent(store: &StateStore) -> Result<(), StateError> {
    if !store
        .scan_prefix(ColumnFamily::Meta, BLOCK_ZERO_PREFIX)?
        .is_empty()
        || !store
            .scan_prefix(ColumnFamily::Meta, DATADIR_IDENTITY_PREFIX)?
            .is_empty()
    {
        return Err(storage_err(
            "duplicate or partial Trident Block 0/datadir identity record",
        ));
    }
    Ok(())
}

fn encode_block_zero_values(
    record: &TridentBlockZeroStorageRecord,
) -> Result<BlockZeroEncodedValues, StateError> {
    record.verify().map_err(storage_err)?;
    let record_bytes = borsh::to_vec(record).map_err(|error| storage_err(error.to_string()))?;
    let commitment_bytes =
        borsh::to_vec(&record.commitment).map_err(|error| storage_err(error.to_string()))?;
    Ok(vec![
        (
            meta_keys::TRIDENT_BLOCK_ZERO_RECORD_VERSION,
            record.version.to_le_bytes().to_vec(),
        ),
        (meta_keys::TRIDENT_BLOCK_ZERO_RECORD, record_bytes),
        (
            meta_keys::TRIDENT_BLOCK_ZERO_STATE_PAYLOAD,
            record.canonical_payload.clone(),
        ),
        (
            meta_keys::TRIDENT_BLOCK_ZERO_STATE_ROOT,
            record.manifest.state_root().as_bytes().to_vec(),
        ),
        (meta_keys::TRIDENT_BLOCK_ZERO_COMMITMENT, commitment_bytes),
        (
            meta_keys::TRIDENT_BLOCK_ZERO_COMMITMENT_HASH,
            record.commitment_hash.as_bytes().to_vec(),
        ),
        (
            meta_keys::TRIDENT_BLOCK_ZERO_ARTIFACT_IDENTITY,
            record.artifact_identity.as_bytes().to_vec(),
        ),
        (
            meta_keys::TRIDENT_BLOCK_ZERO_CONSENSUS_POLICY_HASH,
            record.consensus_policy_hash.as_bytes().to_vec(),
        ),
        (
            meta_keys::TRIDENT_BLOCK_ZERO_NETWORK_FINGERPRINT,
            record.network_fingerprint.as_bytes().to_vec(),
        ),
        (
            meta_keys::TRIDENT_BLOCK_ZERO_CHAIN_ID,
            record.chain_id.as_bytes().to_vec(),
        ),
    ])
}

fn encode_datadir_identity_values(
    identity: &TridentDatadirIdentity,
) -> Result<BlockZeroEncodedValues, StateError> {
    let bytes = identity.canonical_bytes().map_err(storage_err)?;
    Ok(vec![
        (
            meta_keys::TRIDENT_DATADIR_IDENTITY_VERSION,
            identity.version.to_le_bytes().to_vec(),
        ),
        (meta_keys::TRIDENT_DATADIR_IDENTITY, bytes),
    ])
}

fn encode_block_zero_batch(
    record: &TridentBlockZeroStorageRecord,
) -> Result<WriteBatch, StateError> {
    let values = encode_block_zero_values(record)?;
    if values.len() != TRIDENT_BLOCK_ZERO_META_KEYS.len() {
        return Err(storage_err(
            "Block 0 staged batch is missing canonical Meta keys",
        ));
    }
    let identity_values = encode_datadir_identity_values(&record.datadir_identity)?;
    if identity_values.len() != TRIDENT_DATADIR_IDENTITY_META_KEYS.len() {
        return Err(storage_err(
            "staged batch is missing canonical Trident datadir identity keys",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut batch = WriteBatch::new();
    for (key, value) in values {
        if !TRIDENT_BLOCK_ZERO_META_KEYS.contains(&key) {
            return Err(storage_err("unexpected Block 0 Meta key in staged batch"));
        }
        if !seen.insert(key) {
            return Err(storage_err("duplicate Block 0 Meta key in staged batch"));
        }
        batch.put_cf(ColumnFamily::Meta, key, &value);
    }
    if seen.len() != TRIDENT_BLOCK_ZERO_META_KEYS.len() {
        return Err(storage_err(
            "Block 0 staged batch is missing canonical Meta keys",
        ));
    }
    let mut identity_seen = BTreeSet::new();
    for (key, value) in identity_values {
        if !TRIDENT_DATADIR_IDENTITY_META_KEYS.contains(&key) {
            return Err(storage_err(
                "unexpected Trident datadir identity Meta key in staged batch",
            ));
        }
        if !identity_seen.insert(key) {
            return Err(storage_err(
                "duplicate Trident datadir identity Meta key in staged batch",
            ));
        }
        batch.put_cf(ColumnFamily::Meta, key, &value);
    }
    if identity_seen.len() != TRIDENT_DATADIR_IDENTITY_META_KEYS.len() {
        return Err(storage_err(
            "staged batch is missing canonical Trident datadir identity keys",
        ));
    }
    Ok(batch)
}

fn require_block_zero_values(store: &StateStore) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, StateError> {
    let scanned = store.scan_prefix(ColumnFamily::Meta, BLOCK_ZERO_PREFIX)?;
    if scanned.is_empty() {
        return Err(storage_err("missing Trident Block 0 storage record"));
    }
    let values: BTreeMap<_, _> = scanned.into_iter().collect();
    if values.len() != TRIDENT_BLOCK_ZERO_META_KEYS.len()
        || TRIDENT_BLOCK_ZERO_META_KEYS
            .iter()
            .any(|key| !values.contains_key(*key))
    {
        return Err(storage_err(
            "incomplete, duplicate, or unexpected Trident Block 0 storage record",
        ));
    }
    Ok(values)
}

fn require_datadir_identity_values(
    store: &StateStore,
) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, StateError> {
    let scanned = store.scan_prefix(ColumnFamily::Meta, DATADIR_IDENTITY_PREFIX)?;
    if scanned.is_empty() {
        return Err(storage_err("missing Trident datadir identity"));
    }
    let values: BTreeMap<_, _> = scanned.into_iter().collect();
    if values.len() != TRIDENT_DATADIR_IDENTITY_META_KEYS.len()
        || TRIDENT_DATADIR_IDENTITY_META_KEYS
            .iter()
            .any(|key| !values.contains_key(*key))
    {
        return Err(storage_err(
            "incomplete, duplicate, or unexpected Trident datadir identity",
        ));
    }
    Ok(values)
}

fn require_exact_bytes(
    values: &BTreeMap<Vec<u8>, Vec<u8>>,
    key: &[u8],
    expected: &[u8],
    label: &str,
) -> Result<(), StateError> {
    let bytes = values
        .get(key)
        .map(Vec::as_slice)
        .ok_or_else(|| storage_err(format!("missing Block 0 {label}")))?;
    if bytes != expected {
        return Err(storage_err(format!(
            "Block 0 {label} does not match the canonical record"
        )));
    }
    Ok(())
}

fn require_hash(
    values: &BTreeMap<Vec<u8>, Vec<u8>>,
    key: &[u8],
    label: &str,
) -> Result<Hash, StateError> {
    let bytes = values
        .get(key)
        .ok_or_else(|| storage_err(format!("missing Block 0 {label}")))?;
    if bytes.len() != 32 {
        return Err(storage_err(format!("malformed Block 0 {label}")));
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(bytes);
    Ok(Hash(hash))
}

fn decode_and_verify_datadir_identity(
    values: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> Result<TridentDatadirIdentity, StateError> {
    let version_bytes = values
        .get(meta_keys::TRIDENT_DATADIR_IDENTITY_VERSION)
        .ok_or_else(|| storage_err("missing Trident datadir identity version"))?;
    let version_arr: [u8; 4] = version_bytes
        .as_slice()
        .try_into()
        .map_err(|_| storage_err("malformed Trident datadir identity version"))?;
    let version = u32::from_le_bytes(version_arr);
    if version != TRIDENT_DATADIR_IDENTITY_VERSION {
        return Err(storage_err("unsupported Trident datadir identity version"));
    }

    let identity_bytes = values
        .get(meta_keys::TRIDENT_DATADIR_IDENTITY)
        .ok_or_else(|| storage_err("missing Trident datadir identity record"))?;
    let identity = TridentDatadirIdentity::try_from_slice(identity_bytes)
        .map_err(|error| storage_err(format!("malformed Trident datadir identity: {error}")))?;
    if identity.version != version {
        return Err(storage_err(
            "Trident datadir identity version does not match its envelope",
        ));
    }
    let canonical = identity.canonical_bytes().map_err(storage_err)?;
    if canonical != *identity_bytes {
        return Err(storage_err(
            "Trident datadir identity Borsh bytes are not canonical",
        ));
    }
    Ok(identity)
}

fn decode_and_verify_block_zero(
    values: &BTreeMap<Vec<u8>, Vec<u8>>,
    identity_values: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> Result<TridentBlockZeroStorageRecord, StateError> {
    let version_bytes = values
        .get(meta_keys::TRIDENT_BLOCK_ZERO_RECORD_VERSION)
        .ok_or_else(|| storage_err("missing Block 0 record version"))?;
    let version_arr: [u8; 4] = version_bytes
        .as_slice()
        .try_into()
        .map_err(|_| storage_err("malformed Block 0 record version"))?;
    let version = u32::from_le_bytes(version_arr);
    if version != TRIDENT_BLOCK_ZERO_STORAGE_VERSION {
        return Err(storage_err("unsupported Trident Block 0 storage version"));
    }

    let record_bytes = values
        .get(meta_keys::TRIDENT_BLOCK_ZERO_RECORD)
        .ok_or_else(|| storage_err("missing Block 0 storage record"))?;
    let record = TridentBlockZeroStorageRecord::try_from_slice(record_bytes)
        .map_err(|error| storage_err(format!("malformed Block 0 storage record: {error}")))?;
    let round_tripped = borsh::to_vec(&record).map_err(|error| storage_err(error.to_string()))?;
    if round_tripped != *record_bytes {
        return Err(storage_err(
            "Block 0 storage record Borsh round-trip changed the envelope",
        ));
    }
    if record.version != version {
        return Err(storage_err(
            "Block 0 record version does not match the envelope",
        ));
    }
    record.verify().map_err(storage_err)?;
    let datadir_identity = decode_and_verify_datadir_identity(identity_values)?;
    if datadir_identity != record.datadir_identity {
        return Err(storage_err(
            "Trident datadir identity does not match the Block 0 envelope",
        ));
    }
    let identity_bytes = record
        .datadir_identity
        .canonical_bytes()
        .map_err(storage_err)?;
    require_exact_bytes(
        identity_values,
        meta_keys::TRIDENT_DATADIR_IDENTITY,
        &identity_bytes,
        "Trident datadir identity",
    )?;

    require_exact_bytes(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_STATE_PAYLOAD,
        &record.canonical_payload,
        "canonical payload",
    )?;
    let payload_state = TridentBlockZeroState::try_from_slice(&record.canonical_payload)
        .map_err(|error| storage_err(format!("malformed Block 0 canonical payload: {error}")))?;
    if payload_state != record.manifest
        || payload_state.state_root() != record.manifest.state_root()
    {
        return Err(storage_err(
            "Block 0 canonical payload is inconsistent with the manifest",
        ));
    }

    let commitment_bytes =
        borsh::to_vec(&record.commitment).map_err(|error| storage_err(error.to_string()))?;
    require_exact_bytes(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_COMMITMENT,
        &commitment_bytes,
        "commitment",
    )?;

    require_exact_bytes(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_STATE_ROOT,
        record.manifest.state_root().as_bytes(),
        "state root",
    )?;
    require_exact_bytes(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_COMMITMENT_HASH,
        record.commitment_hash.as_bytes(),
        "commitment hash",
    )?;
    require_exact_bytes(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_ARTIFACT_IDENTITY,
        record.artifact_identity.as_bytes(),
        "artifact identity",
    )?;
    require_exact_bytes(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_CONSENSUS_POLICY_HASH,
        record.consensus_policy_hash.as_bytes(),
        "consensus policy hash",
    )?;
    require_exact_bytes(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_NETWORK_FINGERPRINT,
        record.network_fingerprint.as_bytes(),
        "network fingerprint",
    )?;
    require_exact_bytes(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_CHAIN_ID,
        record.chain_id.as_bytes(),
        "chain ID",
    )?;

    // Independent raw-key copies must also decode to the same identities.
    let independent_root = require_hash(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_STATE_ROOT,
        "state root",
    )?;
    let independent_commitment = require_hash(
        values,
        meta_keys::TRIDENT_BLOCK_ZERO_COMMITMENT_HASH,
        "commitment hash",
    )?;
    if independent_root != record.manifest.state_root()
        || independent_commitment != record.commitment_hash
    {
        return Err(storage_err("Block 0 identity copies are mismatched"));
    }
    Ok(record)
}

fn reread_staged_block_zero(
    overlay: &StateStore,
    expected: &TridentBlockZeroStorageRecord,
) -> Result<(), StateError> {
    let loaded = load_verified_trident_block_zero(overlay)?;
    if loaded != *expected {
        return Err(storage_err(
            "Block 0 overlay reread changed the staged record",
        ));
    }
    let expected_values = encode_block_zero_values(expected)?;
    let stored = require_block_zero_values(overlay)?;
    for (key, value) in expected_values {
        require_exact_bytes(&stored, key, &value, "staged bytes")?;
    }
    let expected_identity_values = encode_datadir_identity_values(&expected.datadir_identity)?;
    let stored_identity = require_datadir_identity_values(overlay)?;
    for (key, value) in expected_identity_values {
        require_exact_bytes(
            &stored_identity,
            key,
            &value,
            "staged Trident datadir identity bytes",
        )?;
    }
    Ok(())
}

fn asset_from_ticker(ticker: &str) -> Result<NativeAssetId, String> {
    match ticker {
        "TLT" => Ok(NativeAssetId::TLT),
        "OVL" => Ok(NativeAssetId::OVL),
        "DRC" => Ok(NativeAssetId::DRC),
        _ => Err(format!("unknown native asset {ticker}")),
    }
}

fn parse_address(artifact: &TridentGenesisArtifact, value: &str) -> Result<Address, String> {
    let address = Address::parse(value).ok_or_else(|| "invalid Block 0 address".to_string())?;
    if !value.starts_with(&format!("{}1", artifact.wallet.address_hrp)) {
        return Err("Block 0 address uses the wrong network HRP".into());
    }
    Ok(address)
}

fn allocation_from_artifact(
    artifact: &TridentGenesisArtifact,
    entry: &TridentInitialAllocation,
) -> Result<BlockZeroAllocation, String> {
    Ok(BlockZeroAllocation {
        asset: asset_from_ticker(&entry.asset)?,
        address: parse_address(artifact, &entry.address)?,
        amount: entry.amount,
    })
}

fn vesting_from_artifact(
    artifact: &TridentGenesisArtifact,
    entry: &TridentVestingSchedule,
) -> Result<BlockZeroVesting, String> {
    Ok(BlockZeroVesting {
        asset: asset_from_ticker(&entry.asset)?,
        address: parse_address(artifact, &entry.address)?,
        amount: entry.amount,
        start_timestamp_ms: entry.start_timestamp_ms,
        cliff_timestamp_ms: entry.cliff_timestamp_ms,
        end_timestamp_ms: entry.end_timestamp_ms,
    })
}

fn supply_from_policy(
    asset: NativeAssetId,
    policy: &crate::trident_genesis::TridentAssetPolicy,
) -> Result<BlockZeroSupply, String> {
    let committed = policy
        .genesis_allocation
        .checked_add(policy.treasury_allocation)
        .and_then(|value| value.checked_add(policy.staking_reward_reserve))
        .ok_or_else(|| format!("{asset} Block 0 supply overflow"))?;
    let unissued = policy
        .max_supply
        .checked_sub(committed)
        .ok_or_else(|| format!("{asset} Block 0 supply exceeds cap"))?;
    Ok(BlockZeroSupply {
        asset,
        max_supply: policy.max_supply,
        genesis_allocated: policy.genesis_allocation,
        treasury: policy.treasury_allocation,
        staking_reward_reserve: policy.staking_reward_reserve,
        unissued,
    })
}

fn treasury_from_artifact(
    treasury: TreasuryId,
    entry: &TridentTreasury,
) -> Result<BlockZeroTreasury, String> {
    let asset = asset_from_ticker(&entry.asset)?;
    if asset != treasury.asset() {
        return Err(format!("{} treasury asset mismatch", treasury.as_str()));
    }
    Ok(BlockZeroTreasury {
        treasury,
        asset,
        control: entry.control.clone(),
        balance: entry.allocation,
    })
}

fn validator_set_from_artifact(
    artifact: &TridentGenesisArtifact,
    asset: NativeAssetId,
    source: &TridentValidatorGenesis,
    allocations: &[BlockZeroAllocation],
) -> Result<BlockZeroValidatorSet, String> {
    let mut validators = source
        .genesis_set
        .iter()
        .map(|entry| validator_from_artifact(artifact, entry))
        .collect::<Result<Vec<_>, _>>()?;
    validators.sort_by_key(|validator| validator.operator.0);
    let total_active_stake = validators.iter().try_fold(0u64, |sum, validator| {
        sum.checked_add(validator.self_bond)
            .ok_or_else(|| format!("{asset} active stake overflow"))
    })?;
    let set = BlockZeroValidatorSet {
        asset,
        epoch: 0,
        max_validators: source.max_validators,
        min_self_bond: source.min_self_bond,
        unbonding_period_checkpoints: source.unbonding_period_checkpoints,
        max_commission_bps: source
            .max_commission_bps
            .ok_or_else(|| format!("{asset} max commission is missing"))?,
        max_concentration_bps: source
            .max_concentration_bps
            .ok_or_else(|| format!("{asset} max concentration is missing"))?,
        validators,
        total_active_stake,
    };
    verify_validator_set(&set, allocations)?;
    Ok(set)
}

fn validator_from_artifact(
    artifact: &TridentGenesisArtifact,
    entry: &TridentGenesisValidator,
) -> Result<BlockZeroValidator, String> {
    let bytes = hex::decode(&entry.consensus_public_key)
        .map_err(|error| format!("validator key is not hex: {error}"))?;
    let consensus_public_key = parse_compressed_public_key(&bytes)
        .map_err(|error| format!("validator key is invalid: {error}"))?;
    Ok(BlockZeroValidator {
        operator: address_from_pubkey(&consensus_public_key),
        consensus_public_key,
        withdrawal_address: parse_address(artifact, &entry.withdrawal_address)?,
        self_bond: entry.self_bond,
        commission_bps: entry
            .commission_bps
            .ok_or_else(|| "validator commission is missing".to_string())?,
        metadata_hash: parse_nonzero_hash(
            "validator metadata_hash",
            entry
                .metadata_hash
                .as_deref()
                .ok_or_else(|| "validator metadata_hash is missing".to_string())?,
        )?,
    })
}

fn verify_validator_set(
    set: &BlockZeroValidatorSet,
    allocations: &[BlockZeroAllocation],
) -> Result<(), String> {
    if !matches!(set.asset, NativeAssetId::OVL | NativeAssetId::DRC)
        || set.epoch != 0
        || set.validators.is_empty()
        || set.validators.len() > set.max_validators as usize
        || set.max_commission_bps > MAX_VALIDATOR_COMMISSION_BPS
        || set.max_concentration_bps == 0
        || set.max_concentration_bps > MAX_VALIDATOR_COMMISSION_BPS
    {
        return Err(format!("{} Block 0 validator set is invalid", set.asset));
    }
    let mut operators = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut total = 0u64;
    for validator in &set.validators {
        let consensus_public_key = parse_compressed_public_key(&validator.consensus_public_key)
            .map_err(|error| {
                format!(
                    "{} Block 0 validator consensus key is invalid: {error}",
                    set.asset
                )
            })?;
        if !operators.insert(validator.operator)
            || !keys.insert(validator.consensus_public_key)
            || validator.self_bond < set.min_self_bond
            || validator.operator != address_from_pubkey(&consensus_public_key)
            || validator.commission_bps > set.max_commission_bps
            || validator.commission_bps > MAX_VALIDATOR_COMMISSION_BPS
            || validator.metadata_hash == Hash::ZERO
        {
            return Err(format!("{} Block 0 validator is invalid", set.asset));
        }
        let funded = allocations
            .iter()
            .find(|allocation| {
                allocation.asset == set.asset && allocation.address == validator.operator
            })
            .map(|allocation| allocation.amount)
            .unwrap_or(0);
        if funded < validator.self_bond {
            return Err(format!(
                "{} validator {} self bond is not funded by its artifact allocation",
                set.asset,
                validator.operator.to_bech32()
            ));
        }
        total = total
            .checked_add(validator.self_bond)
            .ok_or_else(|| format!("{} active stake overflow", set.asset))?;
    }
    if total != set.total_active_stake {
        return Err(format!("{} active stake total mismatch", set.asset));
    }
    Ok(())
}

fn reject_duplicate_allocations(allocations: &[BlockZeroAllocation]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    if allocations
        .iter()
        .any(|entry| !seen.insert((entry.asset.wire_byte(), entry.address.0)))
    {
        return Err("duplicate Block 0 allocation for asset/address".into());
    }
    Ok(())
}

fn verify_allocation_supply(
    allocations: &[BlockZeroAllocation],
    supplies: &[BlockZeroSupply],
) -> Result<(), String> {
    if supplies.len() != NativeAssetId::ALL.len()
        || supplies
            .iter()
            .zip(NativeAssetId::ALL)
            .any(|(supply, asset)| supply.asset != asset)
    {
        return Err("Block 0 supply buckets must be ordered TLT/OVL/DRC".into());
    }
    for supply in supplies {
        let allocated = allocations
            .iter()
            .filter(|entry| entry.asset == supply.asset)
            .try_fold(0u64, |sum, entry| sum.checked_add(entry.amount))
            .ok_or_else(|| format!("{} allocation overflow", supply.asset))?;
        if allocated != supply.genesis_allocated {
            return Err(format!(
                "{} allocated supply {allocated} does not match {}",
                supply.asset, supply.genesis_allocated
            ));
        }
        let total = supply
            .genesis_allocated
            .checked_add(supply.treasury)
            .and_then(|value| value.checked_add(supply.staking_reward_reserve))
            .and_then(|value| value.checked_add(supply.unissued))
            .ok_or_else(|| format!("{} supply bucket overflow", supply.asset))?;
        if total != supply.max_supply {
            return Err(format!(
                "{} supply buckets do not conserve supply",
                supply.asset
            ));
        }
    }
    Ok(())
}

fn verify_treasury_supply(
    treasuries: &[BlockZeroTreasury],
    supplies: &[BlockZeroSupply],
) -> Result<(), String> {
    if treasuries.len() != TreasuryId::ALL.len() {
        return Err("Block 0 requires all three protocol treasuries".into());
    }
    for (entry, id) in treasuries.iter().zip(TreasuryId::ALL) {
        let supply = supplies
            .iter()
            .find(|supply| supply.asset == id.asset())
            .ok_or_else(|| "treasury supply bucket is missing".to_string())?;
        if entry.treasury != id
            || entry.asset != id.asset()
            || entry.control.trim().is_empty()
            || entry.balance != supply.treasury
        {
            return Err(format!("{} treasury commitment mismatch", id.as_str()));
        }
    }
    Ok(())
}

fn verify_vesting_is_funded(
    allocations: &[BlockZeroAllocation],
    vesting: &[BlockZeroVesting],
) -> Result<(), String> {
    let mut locked = BTreeMap::<(u8, [u8; 20]), u64>::new();
    for schedule in vesting {
        let value = locked
            .entry((schedule.asset.wire_byte(), schedule.address.0))
            .or_default();
        *value = value
            .checked_add(schedule.amount)
            .ok_or_else(|| "Block 0 vesting amount overflow".to_string())?;
    }
    for ((asset, address), amount) in locked {
        let funded = allocations
            .iter()
            .find(|entry| entry.asset.wire_byte() == asset && entry.address.0 == address)
            .map(|entry| entry.amount)
            .unwrap_or(0);
        if amount > funded {
            return Err("Block 0 vesting exceeds the matching artifact allocation".into());
        }
    }
    Ok(())
}

fn is_sorted_allocations(entries: &[BlockZeroAllocation]) -> bool {
    entries.windows(2).all(|pair| {
        (pair[0].asset.wire_byte(), pair[0].address.0)
            < (pair[1].asset.wire_byte(), pair[1].address.0)
    })
}

fn is_sorted_vesting(entries: &[BlockZeroVesting]) -> bool {
    entries.windows(2).all(|pair| {
        (
            pair[0].asset.wire_byte(),
            pair[0].address.0,
            pair[0].start_timestamp_ms,
            pair[0].cliff_timestamp_ms,
            pair[0].end_timestamp_ms,
        ) <= (
            pair[1].asset.wire_byte(),
            pair[1].address.0,
            pair[1].start_timestamp_ms,
            pair[1].cliff_timestamp_ms,
            pair[1].end_timestamp_ms,
        )
    })
}

fn parse_hash(label: &str, value: &str) -> Result<Hash, String> {
    Hash::from_hex(value).ok_or_else(|| format!("{label} is not a 32-byte hash"))
}

fn parse_nonzero_hash(label: &str, value: &str) -> Result<Hash, String> {
    let hash = parse_hash(label, value)?;
    if hash == Hash::ZERO {
        return Err(format!("{label} must be nonzero"));
    }
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trident_genesis::TridentInitialAllocation;

    const DRAFT: &str = include_str!("../../../../docs/genesis/trident.testnet.genesis.draft.json");

    fn synthetic_freeze_ready_artifact() -> TridentGenesisArtifact {
        let mut artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        artifact.timestamp_ms = 1;
        artifact.bits = Some(0);
        artifact.maturity = "Scaffold".into();
        artifact.notes.clear();
        artifact.wallet.coin_type_status = "registered".into();
        artifact.governance_constitution_hash = "11".repeat(32);
        artifact.emergency_policy_hash = "22".repeat(32);
        artifact.finality.pow_work_threshold_policy = "minimum-blue-score-depth-v1".into();
        artifact.finality.pow_work_threshold = Some(12);

        let ovl = agora_crypto::KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let drc = agora_crypto::KeyPair::from_secret_bytes(&[8; 32]).unwrap();
        let tlt_address = Address([9; 20]).to_bech32_hrp("agoratest");
        let ovl_address = ovl.address().to_bech32_hrp("agoratest");
        let drc_address = drc.address().to_bech32_hrp("agoratest");
        artifact.initial_allocations = vec![
            TridentInitialAllocation {
                asset: "TLT".into(),
                address: tlt_address,
                amount: artifact.assets.tlt.genesis_allocation,
            },
            TridentInitialAllocation {
                asset: "OVL".into(),
                address: ovl_address.clone(),
                amount: 10,
            },
            TridentInitialAllocation {
                asset: "DRC".into(),
                address: drc_address.clone(),
                amount: 20,
            },
        ];
        artifact.assets.ovl.genesis_allocation = 10;
        artifact.assets.drc.genesis_allocation = 20;
        for (asset, treasury) in [
            (
                &mut artifact.assets.tlt,
                &mut artifact.treasuries.tlt_security,
            ),
            (
                &mut artifact.assets.ovl,
                &mut artifact.treasuries.ovl_builder,
            ),
            (
                &mut artifact.assets.drc,
                &mut artifact.treasuries.drc_community,
            ),
        ] {
            asset.treasury_allocation = 1;
            treasury.allocation = 1;
            treasury.control = "synthetic-governance-v1".into();
        }
        for (set, key, address, bond, metadata_byte) in [
            (&mut artifact.ovl_validators, &ovl, ovl_address, 5, "33"),
            (&mut artifact.drc_validators, &drc, drc_address, 7, "44"),
        ] {
            set.max_validators = 1;
            set.min_self_bond = 1;
            set.unbonding_period_checkpoints = 1;
            set.max_commission_bps = Some(2_000);
            set.max_concentration_bps = Some(10_000);
            set.genesis_set.push(TridentGenesisValidator {
                consensus_public_key: hex::encode(key.public_key_bytes()),
                withdrawal_address: address,
                self_bond: bond,
                commission_bps: Some(100),
                metadata_hash: Some(metadata_byte.repeat(32)),
            });
        }
        artifact.genesis_hash = artifact.consensus_identity_hash().to_hex();
        artifact.network_fingerprint = artifact.compute_network_fingerprint().to_hex();
        artifact
    }

    #[test]
    fn synthetic_artifact_produces_deterministic_complete_commitment() {
        let artifact = synthetic_freeze_ready_artifact();
        let state = TridentBlockZeroState::from_artifact(&artifact).unwrap();
        assert_eq!(
            state,
            TridentBlockZeroState::from_artifact(&artifact).unwrap()
        );
        assert_eq!(state.supplies.len(), 3);
        assert_eq!(state.treasuries.len(), 3);
        assert_eq!(state.validator_sets.len(), 2);
        assert_eq!(state.validator_sets[0].validators[0].commission_bps, 100);
        assert_eq!(
            state.validator_sets[0].validators[0].metadata_hash,
            Hash([0x33; 32])
        );
        assert_eq!(state.finality.state, CheckpointState::Proposed);
        assert!(!state.verified_borsh_payload().unwrap().is_empty());
        assert_eq!(state.commitment().hash(), state.commitment().hash());
        state.commitment().verify().unwrap();
    }

    #[test]
    fn validator_fields_change_state_root_and_block_zero_commitment() {
        let baseline =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();

        let mut changed_commission = baseline.clone();
        changed_commission.validator_sets[0].validators[0].commission_bps += 1;
        changed_commission.finality.ovl_snapshot =
            Hash::hash_borsh(&changed_commission.validator_sets[0]);
        changed_commission.verify().unwrap();
        assert_ne!(changed_commission.state_root(), baseline.state_root());
        assert_ne!(
            changed_commission.commitment().hash(),
            baseline.commitment().hash()
        );

        let mut changed_metadata = baseline.clone();
        changed_metadata.validator_sets[0].validators[0].metadata_hash = Hash([0x55; 32]);
        changed_metadata.finality.ovl_snapshot =
            Hash::hash_borsh(&changed_metadata.validator_sets[0]);
        changed_metadata.verify().unwrap();
        assert_ne!(changed_metadata.state_root(), baseline.state_root());
        assert_ne!(
            changed_metadata.commitment().hash(),
            baseline.commitment().hash()
        );
    }

    #[test]
    fn every_da_policy_field_reaches_block_zero_header_and_datadir_identity() {
        let baseline =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        let project = |state: &TridentBlockZeroState| {
            let commitment = state.commitment();
            let header = commitment
                .to_offline_trident_header(1, 0, 0, Hash([0x66; 32]))
                .unwrap();
            let datadir =
                TridentDatadirIdentity::from_block_zero(&commitment, Some(&header)).unwrap();
            (
                state.state_root(),
                commitment.hash(),
                header.commitment_hash().unwrap(),
                datadir.canonical_bytes().unwrap(),
            )
        };
        let baseline_projection = project(&baseline);

        let mut mutations = Vec::new();
        let mut changed = baseline.clone();
        changed.data_availability.version += 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.data_availability.enabled = true;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.data_availability.activation_checkpoint = Some(1);
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.data_availability.activation_block_body_version += 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.data_availability.max_commitments_per_block = 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.data_availability.max_authorization_bytes_per_block = 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.data_availability.base_fee_tlt = 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.data_availability.fee_per_authorization_byte_tlt = 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.data_availability.fee_per_state_byte_tlt = 1;
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed
            .data_availability
            .allowed_sources
            .push(agora_types::DataCommitmentSource::AgoraLayersOvolosBatchLab);
        mutations.push(changed);
        let mut changed = baseline.clone();
        changed.data_availability.max_sequence_advance = 1;
        mutations.push(changed);

        for changed in mutations {
            assert!(changed.verify().is_err());
            let projection = project(&changed);
            assert_ne!(projection.0, baseline_projection.0);
            assert_ne!(projection.1, baseline_projection.1);
            assert_ne!(projection.2, baseline_projection.2);
            assert_ne!(projection.3, baseline_projection.3);
        }
    }

    #[test]
    fn validator_manifest_converts_losslessly_to_runtime_records_and_keys() {
        let artifact = synthetic_freeze_ready_artifact();
        let runtime = artifact.to_runtime_policy().unwrap();
        let state = TridentBlockZeroState::from_artifact(&artifact).unwrap();

        for (set, params) in [
            (&state.validator_sets[0], &runtime.ovl_staking),
            (&state.validator_sets[1], &runtime.drc_staking),
        ] {
            let entries = set.to_runtime_validator_entries(params).unwrap();
            assert_eq!(entries.len(), set.validators.len());
            for ((key, record), source) in entries.iter().zip(&set.validators) {
                assert_eq!(*key, validator_meta_key(set.asset, &source.operator));
                assert_eq!(record.operator, source.operator);
                assert_eq!(
                    record.consensus_pubkey.as_slice(),
                    source.consensus_public_key
                );
                assert_eq!(record.withdrawal, source.withdrawal_address);
                assert_eq!(record.self_bond, source.self_bond);
                assert_eq!(record.delegated, 0);
                assert_eq!(record.commission_bps, source.commission_bps);
                assert_eq!(record.status, ValidatorStatus::Bonded);
                assert_eq!(record.jailed_until_epoch, 0);
                assert_eq!(record.metadata_hash, source.metadata_hash);

                let (_, bytes) = record.canonical_genesis_storage_entry(params).unwrap();
                assert_eq!(ValidatorRecord::try_from_slice(&bytes).unwrap(), *record);
            }
        }
    }

    #[test]
    fn validator_runtime_conversion_rejects_policy_and_identity_mismatch() {
        let artifact = synthetic_freeze_ready_artifact();
        let runtime = artifact.to_runtime_policy().unwrap();
        let state = TridentBlockZeroState::from_artifact(&artifact).unwrap();
        let set = &state.validator_sets[0];

        let mut wrong_params = runtime.ovl_staking.clone();
        wrong_params.max_commission_bps += 1;
        assert!(set
            .to_runtime_validator_entries(&wrong_params)
            .unwrap_err()
            .contains("does not match"));

        let mut changed = set.clone();
        changed.validators[0].metadata_hash = Hash::ZERO;
        assert!(changed
            .to_runtime_validator_entries(&runtime.ovl_staking)
            .unwrap_err()
            .contains("metadata hash must be nonzero"));

        let mut changed = set.clone();
        changed.validators[0].operator = Address([0x77; 20]);
        assert!(changed
            .to_runtime_validator_entries(&runtime.ovl_staking)
            .unwrap_err()
            .contains("operator does not match"));
    }

    #[test]
    fn verified_commitment_converts_to_an_offline_versioned_header() {
        let state =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        let commitment = state.commitment();
        let body_root = Hash([0x33; 32]);
        let header = commitment
            .to_offline_trident_header(1, 0, 7, body_root)
            .unwrap();

        assert!(header.parents.is_empty());
        assert_eq!(header.state_root, commitment.state_root);
        assert_eq!(header.identity.block_zero_commitment, commitment.hash());
        commitment
            .verify_offline_trident_header(&header, body_root)
            .unwrap();

        let bytes = header.canonical_bytes().unwrap();
        let decoded = TridentHeader::from_canonical_bytes(&bytes).unwrap();
        assert_eq!(decoded, header);
        commitment
            .verify_offline_trident_header(&decoded, body_root)
            .unwrap();

        let datadir_identity =
            TridentDatadirIdentity::from_block_zero(&commitment, Some(&decoded)).unwrap();
        assert_eq!(
            datadir_identity.header_identity.block_zero_commitment,
            commitment.hash()
        );
        assert_eq!(
            datadir_identity.block_zero_header_hash,
            Some(decoded.commitment_hash().unwrap())
        );
        let identity_bytes = datadir_identity.canonical_bytes().unwrap();
        assert_eq!(
            TridentDatadirIdentity::try_from_slice(&identity_bytes).unwrap(),
            datadir_identity
        );
    }

    #[test]
    fn block_zero_header_rejects_zero_and_mismatched_commitments() {
        let state =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        let commitment = state.commitment();
        let body_root = Hash([0x44; 32]);
        let header = commitment
            .to_offline_trident_header(1, 0, 0, body_root)
            .unwrap();

        let mut changed = header.clone();
        changed.state_root = Hash([0x45; 32]);
        assert!(commitment
            .verify_offline_trident_header(&changed, body_root)
            .unwrap_err()
            .contains("state root mismatch"));

        let mut changed = header.clone();
        changed.identity.consensus_policy_hash = Hash([0x46; 32]);
        assert!(commitment
            .verify_offline_trident_header(&changed, body_root)
            .unwrap_err()
            .contains("consensus policy hash mismatch"));

        let mut changed = commitment.clone();
        changed.state_root = Hash::ZERO;
        assert!(changed
            .to_offline_trident_header(1, 0, 0, body_root)
            .unwrap_err()
            .contains("must be nonzero"));

        let mut changed = commitment;
        changed.consensus_policy_hash = Hash::ZERO;
        assert!(changed
            .to_offline_trident_header(1, 0, 0, body_root)
            .unwrap_err()
            .contains("must be nonzero"));
    }

    #[test]
    fn allocation_order_is_canonical_but_values_are_committed() {
        let artifact = synthetic_freeze_ready_artifact();
        let baseline = TridentBlockZeroState::from_artifact(&artifact).unwrap();

        let mut reordered = artifact.clone();
        reordered.initial_allocations.reverse();
        reordered.genesis_hash = reordered.consensus_identity_hash().to_hex();
        reordered.network_fingerprint = reordered.compute_network_fingerprint().to_hex();
        let reordered = TridentBlockZeroState::from_artifact(&reordered).unwrap();
        assert_eq!(baseline.allocations, reordered.allocations);

        let mut changed = baseline.clone();
        changed.treasuries[0].control.push_str("-changed");
        changed.verify().unwrap();
        assert_ne!(changed.state_root(), baseline.state_root());
    }

    #[test]
    fn tampering_fails_closed_before_storage_staging() {
        let artifact = synthetic_freeze_ready_artifact();
        let mut state = TridentBlockZeroState::from_artifact(&artifact).unwrap();
        state.supplies[1].unissued -= 1;
        assert!(state.verify().unwrap_err().contains("conserve supply"));

        let mut state = TridentBlockZeroState::from_artifact(&artifact).unwrap();
        state.finality.pow_work_met = true;
        assert!(state
            .verified_borsh_payload()
            .unwrap_err()
            .contains("initial finality"));
    }

    #[test]
    fn validator_self_bond_requires_artifact_allocation() {
        let mut artifact = synthetic_freeze_ready_artifact();
        artifact.ovl_validators.genesis_set[0].self_bond = 11;
        artifact.genesis_hash = artifact.consensus_identity_hash().to_hex();
        artifact.network_fingerprint = artifact.compute_network_fingerprint().to_hex();
        assert!(TridentBlockZeroState::from_artifact(&artifact)
            .unwrap_err()
            .contains("self bond is not funded"));
    }

    #[test]
    fn checked_in_draft_cannot_prepare_block_zero() {
        let artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        assert!(TridentBlockZeroState::from_artifact(&artifact).is_err());
    }

    #[test]
    fn chain_id_and_network_fingerprint_are_identity_sensitive() {
        let artifact = synthetic_freeze_ready_artifact();
        let baseline = TridentBlockZeroState::from_artifact(&artifact).unwrap();
        assert_eq!(baseline.version, TRIDENT_BLOCK_ZERO_STATE_VERSION);
        assert_eq!(baseline.chain_id, artifact.chain_id);
        assert_eq!(
            baseline.network_fingerprint,
            expected_network_fingerprint(
                &baseline.chain_id,
                &baseline.artifact_identity,
                &baseline.consensus_policy_hash,
            )
        );

        let mut other_artifact = artifact.clone();
        other_artifact.chain_id = "agora-trident-testnet-other".into();
        other_artifact.genesis_hash = other_artifact.consensus_identity_hash().to_hex();
        other_artifact.network_fingerprint = other_artifact.compute_network_fingerprint().to_hex();
        let other = TridentBlockZeroState::from_artifact(&other_artifact).unwrap();
        assert_ne!(other.chain_id, baseline.chain_id);
        assert_ne!(other.network_fingerprint, baseline.network_fingerprint);
        assert_ne!(other.state_root(), baseline.state_root());
        assert_ne!(other.commitment().hash(), baseline.commitment().hash());

        let mut broken = baseline.clone();
        broken.chain_id = other.chain_id.clone();
        assert!(broken
            .verify()
            .unwrap_err()
            .contains("network fingerprint is inconsistent"));

        let mut broken = baseline.clone();
        broken.network_fingerprint = other.network_fingerprint;
        assert!(broken
            .verify()
            .unwrap_err()
            .contains("network fingerprint is inconsistent"));
    }

    fn stage_and_commit(store: &StateStore, state: &TridentBlockZeroState) -> WriteBatch {
        let batch = state.stage_verified_store_batch(store).unwrap();
        assert!(load_verified_trident_block_zero(store)
            .unwrap_err()
            .to_string()
            .contains("missing Trident Block 0"));
        store.write_batch(batch.clone()).unwrap();
        batch
    }

    fn stage_error(state: &TridentBlockZeroState, store: &StateStore) -> StateError {
        match state.stage_verified_store_batch(store) {
            Ok(_) => panic!("expected Block 0 staging to fail closed"),
            Err(error) => error,
        }
    }

    #[test]
    fn staged_record_is_verified_before_any_durable_write() {
        let store = StateStore::open_in_memory();
        let state =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        let expected = TridentBlockZeroStorageRecord::from_state(&state).unwrap();

        let batch = state.stage_verified_store_batch(&store).unwrap();
        assert!(!batch.is_empty());
        assert!(store
            .scan_prefix(ColumnFamily::Meta, BLOCK_ZERO_PREFIX)
            .unwrap()
            .is_empty());
        assert!(store
            .scan_prefix(ColumnFamily::Utxo, &[])
            .unwrap()
            .is_empty());

        store.write_batch(batch).unwrap();
        let loaded = load_verified_trident_block_zero(&store).unwrap();
        assert_eq!(loaded, expected);
        assert_eq!(
            loaded.canonical_payload,
            state.verified_borsh_payload().unwrap()
        );
        assert_eq!(loaded.commitment_hash, state.commitment().hash());
        assert_eq!(
            verify_trident_datadir_identity(&store, &expected.datadir_identity).unwrap(),
            expected.datadir_identity
        );
        assert!(store
            .scan_prefix(ColumnFamily::Utxo, &[])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn persisted_identity_is_exact_and_expected_mismatch_is_read_only() {
        let store = StateStore::open_in_memory();
        let state =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        let persisted = state.persist_verified_store_record(&store).unwrap();
        let identity_bytes = persisted.datadir_identity.canonical_bytes().unwrap();
        assert_eq!(
            store
                .get_cf(ColumnFamily::Meta, meta_keys::TRIDENT_DATADIR_IDENTITY)
                .unwrap(),
            Some(identity_bytes.clone())
        );

        let mut wrong_expected = persisted.datadir_identity.clone();
        wrong_expected.block_zero_header_hash = Some(Hash([0x77; 32]));
        wrong_expected.verify().unwrap();
        let error = verify_trident_datadir_identity(&store, &wrong_expected).unwrap_err();
        assert!(error
            .to_string()
            .contains("expected Trident datadir identity"));
        assert_eq!(
            store
                .get_cf(ColumnFamily::Meta, meta_keys::TRIDENT_DATADIR_IDENTITY)
                .unwrap(),
            Some(identity_bytes)
        );
        assert_eq!(load_verified_trident_block_zero(&store).unwrap(), persisted);
    }

    #[test]
    fn duplicate_or_partial_records_are_rejected_without_partial_writes() {
        let store = StateStore::open_in_memory();
        let state =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        stage_and_commit(&store, &state);
        let before = store
            .scan_prefix(ColumnFamily::Meta, BLOCK_ZERO_PREFIX)
            .unwrap();

        let error = stage_error(&state, &store);
        assert!(error.to_string().contains("duplicate"));
        assert_eq!(
            store
                .scan_prefix(ColumnFamily::Meta, BLOCK_ZERO_PREFIX)
                .unwrap(),
            before
        );

        store
            .delete_cf(ColumnFamily::Meta, meta_keys::TRIDENT_BLOCK_ZERO_CHAIN_ID)
            .unwrap();
        assert!(load_verified_trident_block_zero(&store)
            .unwrap_err()
            .to_string()
            .contains("incomplete"));
        assert!(stage_error(&state, &store)
            .to_string()
            .contains("duplicate"));
        assert!(store
            .get_cf(ColumnFamily::Meta, meta_keys::TRIDENT_BLOCK_ZERO_CHAIN_ID)
            .unwrap()
            .is_none());

        stage_fresh_record_into(&store, &state);
        store
            .delete_cf(
                ColumnFamily::Meta,
                meta_keys::TRIDENT_DATADIR_IDENTITY_VERSION,
            )
            .unwrap();
        assert!(load_verified_trident_block_zero(&store)
            .unwrap_err()
            .to_string()
            .contains("incomplete"));
        assert!(stage_error(&state, &store).to_string().contains("partial"));
    }

    #[test]
    fn malformed_inconsistent_and_mismatched_records_fail_closed() {
        let store = StateStore::open_in_memory();
        let state =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        stage_and_commit(&store, &state);

        store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::TRIDENT_BLOCK_ZERO_RECORD,
                b"not-borsh",
            )
            .unwrap();
        assert!(load_verified_trident_block_zero(&store)
            .unwrap_err()
            .to_string()
            .contains("malformed"));

        stage_fresh_record_into(&store, &state);
        store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::TRIDENT_BLOCK_ZERO_CHAIN_ID,
                b"agora-trident-testnet-other",
            )
            .unwrap();
        assert!(load_verified_trident_block_zero(&store)
            .unwrap_err()
            .to_string()
            .contains("does not match"));

        stage_fresh_record_into(&store, &state);
        store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::TRIDENT_BLOCK_ZERO_COMMITMENT_HASH,
                &[0x11; 32],
            )
            .unwrap();
        assert!(load_verified_trident_block_zero(&store)
            .unwrap_err()
            .to_string()
            .contains("does not match"));

        store
            .put_cf(
                ColumnFamily::Meta,
                b"meta/trident_block_zero/extra",
                b"unexpected",
            )
            .unwrap();
        assert!(load_verified_trident_block_zero(&store)
            .unwrap_err()
            .to_string()
            .contains("unexpected"));

        stage_fresh_record_into(&store, &state);
        store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::TRIDENT_DATADIR_IDENTITY,
                b"not-borsh",
            )
            .unwrap();
        assert!(load_verified_trident_block_zero(&store)
            .unwrap_err()
            .to_string()
            .contains("malformed Trident datadir identity"));
    }

    fn stage_fresh_record_into(store: &StateStore, state: &TridentBlockZeroState) {
        for key in TRIDENT_BLOCK_ZERO_META_KEYS {
            store.delete_cf(ColumnFamily::Meta, key).unwrap();
        }
        for key in TRIDENT_DATADIR_IDENTITY_META_KEYS {
            store.delete_cf(ColumnFamily::Meta, key).unwrap();
        }
        store
            .delete_cf(ColumnFamily::Meta, b"meta/trident_block_zero/extra")
            .unwrap();
        store
            .write_batch(state.stage_verified_store_batch(store).unwrap())
            .unwrap();
    }

    #[test]
    fn v2_ignition_does_not_write_block_zero_records() {
        let store = StateStore::open_in_memory();
        crate::GenesisBuilder::default().ignite(&store).unwrap();
        assert!(store
            .scan_prefix(ColumnFamily::Meta, BLOCK_ZERO_PREFIX)
            .unwrap()
            .is_empty());
        assert!(store
            .scan_prefix(ColumnFamily::Meta, DATADIR_IDENTITY_PREFIX)
            .unwrap()
            .is_empty());
        assert!(load_verified_trident_block_zero(&store)
            .unwrap_err()
            .to_string()
            .contains("missing"));
    }

    #[cfg(feature = "rocksdb")]
    fn temp_rocks_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agora-block-zero-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_reopen_preserves_verified_record_and_rejects_tamper() {
        let dir = temp_rocks_dir("reopen");
        let state =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        let expected = TridentBlockZeroStorageRecord::from_state(&state).unwrap();
        let identity_bytes = expected.datadir_identity.canonical_bytes().unwrap();
        {
            let store = StateStore::open(&dir).unwrap();
            assert!(store
                .scan_prefix(ColumnFamily::Meta, BLOCK_ZERO_PREFIX)
                .unwrap()
                .is_empty());
            assert_eq!(
                state.persist_verified_store_record(&store).unwrap(),
                expected
            );
        }
        {
            let store = StateStore::open(&dir).unwrap();
            assert_eq!(load_verified_trident_block_zero(&store).unwrap(), expected);
            assert_eq!(
                verify_trident_datadir_identity(&store, &expected.datadir_identity).unwrap(),
                expected.datadir_identity
            );
            assert_eq!(
                store
                    .get_cf(ColumnFamily::Meta, meta_keys::TRIDENT_DATADIR_IDENTITY)
                    .unwrap(),
                Some(identity_bytes.clone())
            );
            let mut tampered = identity_bytes;
            let last = tampered.len() - 1;
            tampered[last] ^= 1;
            store
                .put_cf(
                    ColumnFamily::Meta,
                    meta_keys::TRIDENT_DATADIR_IDENTITY,
                    &tampered,
                )
                .unwrap();
        }
        {
            let store = StateStore::open(&dir).unwrap();
            assert!(load_verified_trident_block_zero(&store)
                .unwrap_err()
                .to_string()
                .contains("Trident datadir"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_reopen_rejects_partial_datadir_identity() {
        let dir = temp_rocks_dir("partial-identity");
        let state =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        {
            let store = StateStore::open(&dir).unwrap();
            state.persist_verified_store_record(&store).unwrap();
            store
                .delete_cf(
                    ColumnFamily::Meta,
                    meta_keys::TRIDENT_DATADIR_IDENTITY_VERSION,
                )
                .unwrap();
        }
        {
            let store = StateStore::open(&dir).unwrap();
            assert!(load_verified_trident_block_zero(&store)
                .unwrap_err()
                .to_string()
                .contains("incomplete"));
            assert!(ensure_legacy_v2_datadir(&store)
                .unwrap_err()
                .to_string()
                .contains("legacy/v2 startup refuses"));
            assert!(store
                .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
                .unwrap()
                .is_none());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_rejected_stage_leaves_no_partial_write() {
        let dir = temp_rocks_dir("no-partial");
        let state =
            TridentBlockZeroState::from_artifact(&synthetic_freeze_ready_artifact()).unwrap();
        {
            let store = StateStore::open(&dir).unwrap();
            let _batch = state.stage_verified_store_batch(&store).unwrap();
            store
                .put_cf(
                    ColumnFamily::Meta,
                    meta_keys::TRIDENT_BLOCK_ZERO_CHAIN_ID,
                    b"stale",
                )
                .unwrap();
            assert!(stage_error(&state, &store)
                .to_string()
                .contains("duplicate"));
            assert_eq!(
                store
                    .get_cf(ColumnFamily::Meta, meta_keys::TRIDENT_BLOCK_ZERO_CHAIN_ID)
                    .unwrap(),
                Some(b"stale".to_vec())
            );
            assert!(store
                .get_cf(ColumnFamily::Meta, meta_keys::TRIDENT_BLOCK_ZERO_RECORD)
                .unwrap()
                .is_none());
            assert!(store
                .scan_prefix(ColumnFamily::Utxo, &[])
                .unwrap()
                .is_empty());
        }
        {
            let store = StateStore::open(&dir).unwrap();
            assert_eq!(
                store
                    .get_cf(ColumnFamily::Meta, meta_keys::TRIDENT_BLOCK_ZERO_CHAIN_ID)
                    .unwrap(),
                Some(b"stale".to_vec())
            );
            assert!(store
                .get_cf(ColumnFamily::Meta, meta_keys::TRIDENT_BLOCK_ZERO_RECORD)
                .unwrap()
                .is_none());
            assert!(load_verified_trident_block_zero(&store).is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
