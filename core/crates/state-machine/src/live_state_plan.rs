//! Offline-only Trident Block 0 live-state materialization plan.
//!
//! The plan projects every committed manifest leaf onto exactly one primary
//! versioned record, derives the runtime indexes from those records, stages the
//! complete set in a copy-on-write overlay, and recomputes the composed root.
//! It exposes no raw batch; the separate fail-closed consumer is the only
//! durable path.

use std::collections::{BTreeMap, BTreeSet};

use agora_types::{
    Address, Amount, Hash, NativeAssetId, OutPoint, TransactionAcceptance, TreasuryBalance,
    TridentHeader, TxOut,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::accounts::{account_key, AccountState};
use crate::block_zero::{
    BlockZeroAllocation, BlockZeroFinality, BlockZeroSupply, BlockZeroTreasury, BlockZeroVesting,
    TridentBlockZeroState, TridentBlockZeroStorageRecord, BLOCK_ZERO_PREFIX,
    DATADIR_IDENTITY_PREFIX,
};
use crate::columns::{meta_keys, ColumnFamily};
use crate::community_state::{CanonicalCommunitySummary, SUMMARY_KEY};
use crate::governance_state::treasury_key;
use crate::payments::{empty_drc_payment_root, PAYMENT_ROOT_KEY};
use crate::staking::{
    epoch_key, reward_pool_meta_key, snapshot_key, staking_reserve_key, validator_meta_key,
    ValidatorRecord, ValidatorStatus,
};
use crate::supply::{issued_supply_key, max_supply_key};
use crate::tx_index::{encode_tx_location, tx_inclusion_key, tx_index_key};
use crate::utxo::outpoint_key;
use crate::{StateError, StateStore, WriteBatch, TRIDENT_STATE_TRANSITION_VERSION};

/// Version of the offline plan and every new primary live-state record.
pub const TRIDENT_LIVE_STATE_PLAN_VERSION: u32 = 1;
/// Domain for the canonical Block 0 body.
pub const TRIDENT_BLOCK_ZERO_BODY_DOMAIN: &[u8] = b"agora-trident-block-zero-body-v1";
/// Domain for the composed root of exact planned store records.
pub const TRIDENT_LIVE_STATE_ROOT_DOMAIN: &[u8] = b"agora-trident-live-state-root-v1";
const TRIDENT_LIVE_STATE_COMPONENT_DOMAIN: &[u8] = b"agora-trident-live-state-component-v1";
const TRIDENT_TLT_ISSUANCE_DOMAIN: &[u8] = b"agora-trident-tlt-issuance-v1";

const IDENTITY_KEY: &[u8] = b"trident/live/v1/identity";
const ALLOCATION_PREFIX: &[u8] = b"trident/live/v1/allocation/";
const SUPPLY_PREFIX: &[u8] = b"trident/live/v1/supply/";
const TREASURY_PREFIX: &[u8] = b"trident/live/v1/treasury/";
const VESTING_PREFIX: &[u8] = b"trident/live/v1/vesting/";
const STAKING_POLICY_PREFIX: &[u8] = b"trident/live/v1/staking_policy/";
const INITIAL_FINALITY_KEY: &[u8] = b"trident/live/v1/initial_finality";
const INITIAL_ACCEPTANCE_KEY: &[u8] = b"trident/live/v1/initial_acceptance";
const HEADER_PREFIX: &[u8] = b"trident/header/v1/";
const BODY_PREFIX: &[u8] = b"trident/body/v1/";

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentBlockZeroTltIssuance {
    pub version: u32,
    /// Output order follows canonical `(asset, address)` allocation order.
    pub outputs: Vec<TxOut>,
}

impl TridentBlockZeroTltIssuance {
    pub fn issuance_id(&self) -> Hash {
        Hash::hash_borsh(&(TRIDENT_TLT_ISSUANCE_DOMAIN, self))
    }
}

/// Concrete offline Block 0 body. Account, stake, and policy initialization are
/// state transitions, not unsigned transaction lanes.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentBlockZeroBody {
    pub version: u32,
    pub tlt_issuance: TridentBlockZeroTltIssuance,
}

impl TridentBlockZeroBody {
    pub fn root(&self) -> Hash {
        Hash::hash_borsh(&(TRIDENT_BLOCK_ZERO_BODY_DOMAIN, self))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentLiveIdentityRecord {
    pub version: u32,
    pub manifest_version: u32,
    pub chain_id: String,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub network_fingerprint: Hash,
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
    pub governance_constitution_hash: Hash,
    pub emergency_policy_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentLiveAllocationRecord {
    pub version: u32,
    pub allocation: BlockZeroAllocation,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentLiveSupplyRecord {
    pub version: u32,
    pub supply: BlockZeroSupply,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentLiveTreasuryRecord {
    pub version: u32,
    pub treasury: BlockZeroTreasury,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentLiveStakingPolicyRecord {
    pub version: u32,
    pub asset: NativeAssetId,
    pub epoch: u64,
    pub max_validators: u32,
    pub min_self_bond: u64,
    pub unbonding_period_checkpoints: u64,
    pub max_commission_bps: u16,
    pub max_concentration_bps: u16,
    pub epoch_reserve_drip: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TridentVestingFunding {
    TltOutPoint(OutPoint),
    Account {
        asset: NativeAssetId,
        address: Address,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentLiveVestingLockRecord {
    pub version: u32,
    pub schedule: BlockZeroVesting,
    pub funding: TridentVestingFunding,
}

impl TridentLiveVestingLockRecord {
    pub fn locked_amount_at(&self, timestamp_ms: u64) -> Result<u64, String> {
        self.schedule.locked_amount_at(timestamp_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentLiveInitialFinalityRecord {
    pub version: u32,
    pub finality: BlockZeroFinality,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentBlockZeroAcceptanceRecord {
    pub version: u32,
    pub body_root: Hash,
    pub tlt_issuance_id: Hash,
    pub status: TransactionAcceptance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum TridentLiveStateComponent {
    Identity = 0,
    Allocations = 1,
    Utxo = 2,
    OvlAccounts = 3,
    DrcAccounts = 4,
    Supply = 5,
    Treasuries = 6,
    Vesting = 7,
    OvlStaking = 8,
    DrcStaking = 9,
    Acceptance = 10,
    Finality = 11,
    DrcPayments = 12,
    Community = 13,
}

impl TridentLiveStateComponent {
    const ALL: [Self; 14] = [
        Self::Identity,
        Self::Allocations,
        Self::Utxo,
        Self::OvlAccounts,
        Self::DrcAccounts,
        Self::Supply,
        Self::Treasuries,
        Self::Vesting,
        Self::OvlStaking,
        Self::DrcStaking,
        Self::Acceptance,
        Self::Finality,
        Self::DrcPayments,
        Self::Community,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentLiveStateRoots {
    pub identity: Hash,
    pub allocations: Hash,
    pub utxo: Hash,
    pub ovl_accounts: Hash,
    pub drc_accounts: Hash,
    pub supply: Hash,
    pub treasuries: Hash,
    pub vesting: Hash,
    pub ovl_staking: Hash,
    pub drc_staking: Hash,
    pub acceptance: Hash,
    pub finality: Hash,
    pub drc_payments: Hash,
    pub community: Hash,
}

impl TridentLiveStateRoots {
    pub fn state_root(&self) -> Hash {
        Hash::hash_borsh(&(
            TRIDENT_LIVE_STATE_ROOT_DOMAIN,
            TRIDENT_LIVE_STATE_PLAN_VERSION,
            TRIDENT_STATE_TRANSITION_VERSION,
            self,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentManifestFieldMapping {
    pub field: String,
    pub column_family: ColumnFamily,
    pub key: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentPlannedStoreRecord {
    column_family: ColumnFamily,
    key: Vec<u8>,
    value: Vec<u8>,
    component: Option<TridentLiveStateComponent>,
    manifest_fields: Vec<String>,
}

impl TridentPlannedStoreRecord {
    pub fn column_family(&self) -> ColumnFamily {
        self.column_family
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    pub fn value(&self) -> &[u8] {
        &self.value
    }

    pub fn component(&self) -> Option<TridentLiveStateComponent> {
        self.component
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentAssetConservation {
    pub asset: NativeAssetId,
    pub genesis_allocated: u64,
    pub liquid: u64,
    pub validator_bonds: u64,
    pub treasury: u64,
    pub issued: u64,
    pub staking_reward_reserve: u64,
    pub unissued: u64,
    pub max_supply: u64,
}

/// Fully verified output of the offline planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentLiveStatePlan {
    pub version: u32,
    pub body: TridentBlockZeroBody,
    pub body_root: Hash,
    pub state_roots: TridentLiveStateRoots,
    pub state_root: Hash,
    pub header: TridentHeader,
    pub header_hash: Hash,
    records: Vec<TridentPlannedStoreRecord>,
    manifest_mappings: Vec<TridentManifestFieldMapping>,
    conservation: Vec<TridentAssetConservation>,
}

impl TridentBlockZeroState {
    pub fn canonical_body(&self) -> TridentBlockZeroBody {
        projected_records(self).body
    }

    pub fn canonical_body_root(&self) -> Hash {
        self.canonical_body().root()
    }

    /// Build and COW-verify a complete plan without mutating `base`.
    pub fn plan_live_state(
        &self,
        base: &StateStore,
        header: &TridentHeader,
    ) -> Result<TridentLiveStatePlan, StateError> {
        TridentLiveStatePlan::build(self, base, header)
    }
}

impl TridentLiveStatePlan {
    fn build(
        state: &TridentBlockZeroState,
        base: &StateStore,
        header: &TridentHeader,
    ) -> Result<Self, StateError> {
        let plan = Self::derive_verified(state, header)?;
        validate_plan_base(base, state, header)?;

        let before = snapshot_store(base)?;
        let overlay = plan.staged_overlay(base)?;
        plan.verify_overlay(&overlay)?;
        let after = snapshot_store(base)?;
        if after != before {
            return Err(storage_error(
                "Block 0 planning mutated the caller's base store",
            ));
        }
        Ok(plan)
    }

    pub(crate) fn derive_verified(
        state: &TridentBlockZeroState,
        header: &TridentHeader,
    ) -> Result<Self, StateError> {
        state.verify().map_err(storage_error)?;
        validate_header(state, header)?;

        let projection = projected_records(state);
        let body_root = projection.body.root();
        let state_roots = roots_from_records(&projection.records);
        let state_root = state_roots.state_root();
        if state_root != state.state_root() || body_root != state.canonical_body_root() {
            return Err(storage_error(
                "Block 0 projection disagrees with its canonical roots",
            ));
        }

        let header_hash = header
            .commitment_hash()
            .map_err(|error| storage_error(error.to_string()))?;
        let mut records = projection.records;
        append_ancillary_records(
            &mut records,
            state,
            header,
            header_hash,
            &projection.body,
            body_root,
            state_root,
        );
        canonicalize_records(&mut records)?;
        let manifest_mappings = verify_coverage(state, &records)?;
        let conservation = conservation_for(state)?;

        let plan = Self {
            version: TRIDENT_LIVE_STATE_PLAN_VERSION,
            body: projection.body,
            body_root,
            state_roots,
            state_root,
            header: header.clone(),
            header_hash,
            records,
            manifest_mappings,
            conservation,
        };
        plan.verify(state, header)?;
        Ok(plan)
    }

    pub fn records(&self) -> &[TridentPlannedStoreRecord] {
        &self.records
    }

    pub fn manifest_mappings(&self) -> &[TridentManifestFieldMapping] {
        &self.manifest_mappings
    }

    pub fn conservation(&self) -> &[TridentAssetConservation] {
        &self.conservation
    }

    /// Recheck a plan against independently verified state and header inputs.
    pub fn verify(
        &self,
        state: &TridentBlockZeroState,
        header: &TridentHeader,
    ) -> Result<(), StateError> {
        if self.version != TRIDENT_LIVE_STATE_PLAN_VERSION {
            return Err(storage_error("unsupported Trident live-state plan version"));
        }
        state.verify().map_err(storage_error)?;
        validate_header(state, header)?;
        if self.header != *header {
            return Err(storage_error("live-state plan header mismatch"));
        }

        let projection = projected_records(state);
        let body_root = projection.body.root();
        let state_roots = roots_from_records(&projection.records);
        let state_root = state_roots.state_root();
        let header_hash = header
            .commitment_hash()
            .map_err(|error| storage_error(error.to_string()))?;
        if self.body != projection.body
            || self.body_root != body_root
            || self.state_roots != state_roots
            || self.state_root != state_root
            || self.header_hash != header_hash
        {
            return Err(storage_error(
                "live-state plan roots or header identity were tampered",
            ));
        }

        let mut expected_records = projection.records;
        append_ancillary_records(
            &mut expected_records,
            state,
            header,
            header_hash,
            &self.body,
            body_root,
            state_root,
        );
        canonicalize_records(&mut expected_records)?;
        if self.records != expected_records {
            return Err(storage_error("live-state plan record set was tampered"));
        }
        let mappings = verify_coverage(state, &self.records)?;
        if self.manifest_mappings != mappings {
            return Err(storage_error(
                "live-state manifest field mapping was tampered",
            ));
        }
        let conservation = conservation_for(state)?;
        if self.conservation != conservation {
            return Err(storage_error(
                "live-state supply conservation proof was tampered",
            ));
        }
        Ok(())
    }

    /// Apply the plan only to a new COW view. Raw batch access remains private
    /// so durable writes can only pass the atomic consumer's full verification.
    pub fn staged_overlay(&self, base: &StateStore) -> Result<StateStore, StateError> {
        let before = snapshot_store(base)?;
        let overlay = base.cow_overlay();
        overlay.write_batch(batch_from_records(&self.records))?;
        let after = snapshot_store(base)?;
        if after != before {
            return Err(storage_error(
                "COW staging mutated the live-state plan base store",
            ));
        }
        Ok(overlay)
    }

    pub(crate) fn verify_overlay(&self, overlay: &StateStore) -> Result<(), StateError> {
        let mut reread = Vec::with_capacity(self.records.len());
        for record in &self.records {
            let value = overlay
                .get_cf(record.column_family, &record.key)?
                .ok_or_else(|| storage_error("staged live-state record is missing"))?;
            if value != record.value {
                return Err(storage_error(
                    "staged live-state record does not match planned bytes",
                ));
            }
            let mut actual = record.clone();
            actual.value = value;
            reread.push(actual);
        }
        let state_records: Vec<_> = reread
            .iter()
            .filter(|record| record.component.is_some())
            .cloned()
            .collect();
        let reread_roots = roots_from_records(&state_records);
        if reread_roots != self.state_roots || reread_roots.state_root() != self.state_root {
            return Err(storage_error(
                "staged live-state composed root does not match the plan",
            ));
        }

        let body_key = body_key(&self.header_hash);
        let body_bytes = overlay
            .get_cf(ColumnFamily::Hot, &body_key)?
            .ok_or_else(|| storage_error("staged Block 0 body is missing"))?;
        let body = TridentBlockZeroBody::try_from_slice(&body_bytes)
            .map_err(|error| storage_error(error.to_string()))?;
        if body != self.body || body.root() != self.body_root {
            return Err(storage_error("staged Block 0 body root mismatch"));
        }
        let header_bytes = overlay
            .get_cf(ColumnFamily::Warm, &header_key(&self.header_hash))?
            .ok_or_else(|| storage_error("staged Trident header is missing"))?;
        let header = TridentHeader::from_canonical_bytes(&header_bytes)
            .map_err(|error| storage_error(error.to_string()))?;
        if header != self.header
            || header
                .commitment_hash()
                .map_err(|error| storage_error(error.to_string()))?
                != self.header_hash
        {
            return Err(storage_error("staged Trident header mismatch"));
        }
        Ok(())
    }
}

struct ProjectedRecords {
    body: TridentBlockZeroBody,
    records: Vec<TridentPlannedStoreRecord>,
}

pub(crate) fn projected_state_root(state: &TridentBlockZeroState) -> Hash {
    roots_from_records(&projected_records(state).records).state_root()
}

pub(crate) fn verify_manifest_coverage(state: &TridentBlockZeroState) -> Result<(), String> {
    if state
        .allocations
        .iter()
        .filter(|allocation| allocation.asset == NativeAssetId::TLT)
        .count()
        > u32::MAX as usize
    {
        return Err("Block 0 has too many TLT outputs".into());
    }
    verify_coverage(state, &projected_records(state).records)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn projected_records(state: &TridentBlockZeroState) -> ProjectedRecords {
    let outputs = state
        .allocations
        .iter()
        .filter(|allocation| allocation.asset == NativeAssetId::TLT)
        .map(|allocation| TxOut {
            value: Amount::from_base_units(allocation.amount),
            address: allocation.address,
        })
        .collect();
    let body = TridentBlockZeroBody {
        version: TRIDENT_LIVE_STATE_PLAN_VERSION,
        tlt_issuance: TridentBlockZeroTltIssuance {
            version: TRIDENT_LIVE_STATE_PLAN_VERSION,
            outputs,
        },
    };
    let issuance_id = body.tlt_issuance.issuance_id();
    let body_root = body.root();
    let bonds = validator_bonds(state);
    let mut tlt_outpoints = BTreeMap::new();
    let mut records = Vec::new();

    push_encoded(
        &mut records,
        ColumnFamily::Meta,
        IDENTITY_KEY.to_vec(),
        &TridentLiveIdentityRecord {
            version: TRIDENT_LIVE_STATE_PLAN_VERSION,
            manifest_version: state.version,
            chain_id: state.chain_id.clone(),
            timestamp_ms: state.timestamp_ms,
            bits: state.bits,
            network_fingerprint: state.network_fingerprint,
            artifact_identity: state.artifact_identity,
            consensus_policy_hash: state.consensus_policy_hash,
            governance_constitution_hash: state.governance_constitution_hash,
            emergency_policy_hash: state.emergency_policy_hash,
        },
        Some(TridentLiveStateComponent::Identity),
        fields(
            "",
            &[
                "version",
                "chain_id",
                "timestamp_ms",
                "bits",
                "network_fingerprint",
                "artifact_identity",
                "consensus_policy_hash",
                "governance_constitution_hash",
                "emergency_policy_hash",
            ],
        ),
    );

    let mut tlt_index = 0u32;
    for (index, allocation) in state.allocations.iter().enumerate() {
        let prefix = format!("allocations[{index}]");
        push_encoded(
            &mut records,
            ColumnFamily::Meta,
            allocation_record_key(allocation),
            &TridentLiveAllocationRecord {
                version: TRIDENT_LIVE_STATE_PLAN_VERSION,
                allocation: allocation.clone(),
            },
            Some(TridentLiveStateComponent::Allocations),
            fields(&prefix, &["asset", "address", "amount"]),
        );

        match allocation.asset {
            NativeAssetId::TLT => {
                let outpoint = OutPoint {
                    tx_id: issuance_id,
                    index: tlt_index,
                };
                tlt_index = tlt_index
                    .checked_add(1)
                    .expect("manifest coverage bounds TLT output count");
                tlt_outpoints.insert(allocation.address.0, outpoint);
                push_encoded(
                    &mut records,
                    ColumnFamily::Utxo,
                    outpoint_key(&outpoint).to_vec(),
                    &TxOut {
                        value: Amount::from_base_units(allocation.amount),
                        address: allocation.address,
                    },
                    Some(TridentLiveStateComponent::Utxo),
                    Vec::new(),
                );
            }
            NativeAssetId::OVL | NativeAssetId::DRC => {
                let bonded = bonds
                    .get(&(allocation.asset.wire_byte(), allocation.address.0))
                    .copied()
                    .unwrap_or(0);
                let account = AccountState {
                    balance: allocation
                        .amount
                        .checked_sub(bonded)
                        .expect("manifest validation funds every validator bond"),
                    nonce: 0,
                };
                let component = if allocation.asset == NativeAssetId::OVL {
                    TridentLiveStateComponent::OvlAccounts
                } else {
                    TridentLiveStateComponent::DrcAccounts
                };
                push_encoded(
                    &mut records,
                    ColumnFamily::Meta,
                    account_key(allocation.asset, &allocation.address),
                    &account,
                    Some(component),
                    Vec::new(),
                );
            }
        }
    }

    for (index, supply) in state.supplies.iter().enumerate() {
        let prefix = format!("supplies[{index}]");
        push_encoded(
            &mut records,
            ColumnFamily::Meta,
            supply_record_key(supply.asset),
            &TridentLiveSupplyRecord {
                version: TRIDENT_LIVE_STATE_PLAN_VERSION,
                supply: supply.clone(),
            },
            Some(TridentLiveStateComponent::Supply),
            fields(
                &prefix,
                &[
                    "asset",
                    "max_supply",
                    "genesis_allocated",
                    "treasury",
                    "staking_reward_reserve",
                    "unissued",
                ],
            ),
        );
        push_raw(
            &mut records,
            ColumnFamily::Meta,
            max_supply_key(supply.asset),
            supply.max_supply.to_le_bytes().to_vec(),
            Some(TridentLiveStateComponent::Supply),
        );
        let issued = supply
            .genesis_allocated
            .checked_add(supply.treasury)
            .expect("manifest validation excludes issued-supply overflow");
        push_raw(
            &mut records,
            ColumnFamily::Meta,
            issued_supply_key(supply.asset),
            issued.to_le_bytes().to_vec(),
            Some(TridentLiveStateComponent::Supply),
        );
        if matches!(supply.asset, NativeAssetId::OVL | NativeAssetId::DRC) {
            push_raw(
                &mut records,
                ColumnFamily::Meta,
                staking_reserve_key(supply.asset),
                supply.staking_reward_reserve.to_le_bytes().to_vec(),
                Some(TridentLiveStateComponent::Supply),
            );
        }
    }

    for (index, treasury) in state.treasuries.iter().enumerate() {
        let prefix = format!("treasuries[{index}]");
        push_encoded(
            &mut records,
            ColumnFamily::Meta,
            treasury_record_key(treasury.treasury.wire_byte()),
            &TridentLiveTreasuryRecord {
                version: TRIDENT_LIVE_STATE_PLAN_VERSION,
                treasury: treasury.clone(),
            },
            Some(TridentLiveStateComponent::Treasuries),
            fields(&prefix, &["treasury", "asset", "control", "balance"]),
        );
        let balance = TreasuryBalance {
            treasury: treasury.treasury,
            asset: treasury.asset,
            balance: Amount::from_base_units(treasury.balance),
        };
        push_encoded(
            &mut records,
            ColumnFamily::Meta,
            treasury_key(treasury.treasury),
            &balance,
            Some(TridentLiveStateComponent::Treasuries),
            Vec::new(),
        );
    }

    for (index, schedule) in state.vesting.iter().enumerate() {
        let funding = match schedule.asset {
            NativeAssetId::TLT => TridentVestingFunding::TltOutPoint(
                *tlt_outpoints
                    .get(&schedule.address.0)
                    .expect("manifest validation funds every TLT vesting lock"),
            ),
            NativeAssetId::OVL | NativeAssetId::DRC => TridentVestingFunding::Account {
                asset: schedule.asset,
                address: schedule.address,
            },
        };
        let prefix = format!("vesting[{index}]");
        push_encoded(
            &mut records,
            ColumnFamily::Meta,
            vesting_record_key(schedule),
            &TridentLiveVestingLockRecord {
                version: TRIDENT_LIVE_STATE_PLAN_VERSION,
                schedule: schedule.clone(),
                funding,
            },
            Some(TridentLiveStateComponent::Vesting),
            fields(
                &prefix,
                &[
                    "asset",
                    "address",
                    "amount",
                    "start_timestamp_ms",
                    "cliff_timestamp_ms",
                    "end_timestamp_ms",
                    "release_policy",
                ],
            ),
        );
    }

    for (set_index, set) in state.validator_sets.iter().enumerate() {
        let component = staking_component(set.asset);
        let prefix = format!("validator_sets[{set_index}]");
        push_encoded(
            &mut records,
            ColumnFamily::Meta,
            staking_policy_key(set.asset),
            &TridentLiveStakingPolicyRecord {
                version: TRIDENT_LIVE_STATE_PLAN_VERSION,
                asset: set.asset,
                epoch: set.epoch,
                max_validators: set.max_validators,
                min_self_bond: set.min_self_bond,
                unbonding_period_checkpoints: set.unbonding_period_checkpoints,
                max_commission_bps: set.max_commission_bps,
                max_concentration_bps: set.max_concentration_bps,
                epoch_reserve_drip: set.epoch_reserve_drip,
            },
            Some(component),
            fields(
                &prefix,
                &[
                    "asset",
                    "epoch",
                    "max_validators",
                    "min_self_bond",
                    "unbonding_period_checkpoints",
                    "max_commission_bps",
                    "max_concentration_bps",
                    "epoch_reserve_drip",
                ],
            ),
        );
        push_raw(
            &mut records,
            ColumnFamily::Meta,
            epoch_key(set.asset),
            set.epoch.to_le_bytes().to_vec(),
            Some(component),
        );
        for (validator_index, validator) in set.validators.iter().enumerate() {
            let record = ValidatorRecord {
                operator: validator.operator,
                consensus_pubkey: validator.consensus_public_key.to_vec(),
                withdrawal: validator.withdrawal_address,
                self_bond: validator.self_bond,
                delegated: 0,
                commission_bps: validator.commission_bps,
                status: ValidatorStatus::Bonded,
                jailed_until_epoch: 0,
                metadata_hash: validator.metadata_hash,
            };
            let validator_prefix = format!("{prefix}.validators[{validator_index}]");
            push_encoded(
                &mut records,
                ColumnFamily::Meta,
                validator_meta_key(set.asset, &validator.operator),
                &record,
                Some(component),
                fields(
                    &validator_prefix,
                    &[
                        "operator",
                        "consensus_public_key",
                        "withdrawal_address",
                        "self_bond",
                        "commission_bps",
                        "metadata_hash",
                    ],
                ),
            );
        }
        let snapshot = set.epoch_zero_snapshot();
        push_encoded(
            &mut records,
            ColumnFamily::Meta,
            snapshot_key(set.asset, set.epoch),
            &snapshot,
            Some(component),
            fields(&prefix, &["total_active_stake"]),
        );
        push_raw(
            &mut records,
            ColumnFamily::Meta,
            reward_pool_meta_key(set.asset),
            0u64.to_le_bytes().to_vec(),
            Some(component),
        );
    }

    push_encoded(
        &mut records,
        ColumnFamily::Warm,
        INITIAL_ACCEPTANCE_KEY.to_vec(),
        &TridentBlockZeroAcceptanceRecord {
            version: TRIDENT_LIVE_STATE_PLAN_VERSION,
            body_root,
            tlt_issuance_id: issuance_id,
            status: TransactionAcceptance::Accepted,
        },
        Some(TridentLiveStateComponent::Acceptance),
        Vec::new(),
    );
    push_encoded(
        &mut records,
        ColumnFamily::Meta,
        INITIAL_FINALITY_KEY.to_vec(),
        &TridentLiveInitialFinalityRecord {
            version: TRIDENT_LIVE_STATE_PLAN_VERSION,
            finality: state.finality.clone(),
        },
        Some(TridentLiveStateComponent::Finality),
        fields(
            "finality",
            &[
                "state",
                "pow_work_met",
                "finalized_blue_score",
                "ovl_snapshot",
                "ovl_active_stake",
                "ovl_signed_stake",
                "drc_snapshot",
                "drc_active_stake",
                "drc_signed_stake",
            ],
        ),
    );
    push_raw(
        &mut records,
        ColumnFamily::Meta,
        PAYMENT_ROOT_KEY.to_vec(),
        empty_drc_payment_root().as_bytes().to_vec(),
        Some(TridentLiveStateComponent::DrcPayments),
    );
    push_encoded(
        &mut records,
        ColumnFamily::Meta,
        SUMMARY_KEY.to_vec(),
        &CanonicalCommunitySummary::default(),
        Some(TridentLiveStateComponent::Community),
        Vec::new(),
    );

    records.sort_by(record_order);
    ProjectedRecords { body, records }
}

fn append_ancillary_records(
    records: &mut Vec<TridentPlannedStoreRecord>,
    state: &TridentBlockZeroState,
    header: &TridentHeader,
    header_hash: Hash,
    body: &TridentBlockZeroBody,
    body_root: Hash,
    state_root: Hash,
) {
    push_raw(
        records,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_PLAN_VERSION.to_vec(),
        TRIDENT_LIVE_STATE_PLAN_VERSION.to_le_bytes().to_vec(),
        None,
    );
    push_raw(
        records,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_MANIFEST_ROOT.to_vec(),
        state.manifest_root().as_bytes().to_vec(),
        None,
    );
    push_raw(
        records,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_BODY_ROOT.to_vec(),
        body_root.as_bytes().to_vec(),
        None,
    );
    push_raw(
        records,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_STATE_ROOT.to_vec(),
        state_root.as_bytes().to_vec(),
        None,
    );
    push_raw(
        records,
        ColumnFamily::Meta,
        meta_keys::TRIDENT_LIVE_STATE_HEADER_HASH.to_vec(),
        header_hash.as_bytes().to_vec(),
        None,
    );
    push_raw(
        records,
        ColumnFamily::Meta,
        meta_keys::GENESIS_HASH.to_vec(),
        header_hash.as_bytes().to_vec(),
        None,
    );
    let tips = borsh::to_vec(&vec![header_hash]).expect("hash vector Borsh serialization");
    push_raw(
        records,
        ColumnFamily::Meta,
        meta_keys::TIPS.to_vec(),
        tips,
        None,
    );
    push_raw(
        records,
        ColumnFamily::Meta,
        meta_keys::VIRTUAL_TIP.to_vec(),
        header_hash.as_bytes().to_vec(),
        None,
    );
    push_raw(
        records,
        ColumnFamily::Hot,
        body_key(&header_hash),
        borsh::to_vec(body).expect("Block 0 body Borsh serialization"),
        None,
    );
    push_raw(
        records,
        ColumnFamily::Archival,
        body_key(&header_hash),
        borsh::to_vec(body).expect("Block 0 body Borsh serialization"),
        None,
    );
    push_raw(
        records,
        ColumnFamily::Warm,
        header_key(&header_hash),
        header
            .canonical_bytes()
            .expect("verified Trident header canonical serialization"),
        None,
    );
    let issuance_id = body.tlt_issuance.issuance_id();
    let location = encode_tx_location(&header_hash, 0).to_vec();
    push_raw(
        records,
        ColumnFamily::Warm,
        tx_index_key(&issuance_id),
        location.clone(),
        None,
    );
    push_raw(
        records,
        ColumnFamily::Warm,
        tx_inclusion_key(&issuance_id, &header_hash),
        location,
        None,
    );
}

fn validate_header(
    state: &TridentBlockZeroState,
    header: &TridentHeader,
) -> Result<(), StateError> {
    let commitment = state.commitment();
    if commitment.manifest_root != state.manifest_root()
        || commitment.state_root != state.state_root()
    {
        return Err(storage_error("Block 0 commitment roots are inconsistent"));
    }
    commitment
        .verify_offline_trident_header(header, state.canonical_body_root())
        .map_err(storage_error)?;
    if header.timestamp_ms != state.timestamp_ms {
        return Err(storage_error(
            "Trident Block 0 header timestamp does not match the manifest",
        ));
    }
    if header.bits != state.bits {
        return Err(storage_error(
            "Trident Block 0 header difficulty does not match the manifest",
        ));
    }
    Ok(())
}

fn validate_plan_base(
    base: &StateStore,
    state: &TridentBlockZeroState,
    header: &TridentHeader,
) -> Result<(), StateError> {
    for cf in [
        ColumnFamily::Hot,
        ColumnFamily::Warm,
        ColumnFamily::Archival,
        ColumnFamily::Utxo,
    ] {
        if !base.scan_prefix(cf, &[])?.is_empty() {
            return Err(storage_error(format!(
                "live-state plan requires an empty {} base",
                cf.name()
            )));
        }
    }
    let meta = base.scan_prefix(ColumnFamily::Meta, &[])?;
    if meta.is_empty() {
        return Ok(());
    }
    if meta.iter().any(|(key, _)| {
        !key.starts_with(BLOCK_ZERO_PREFIX) && !key.starts_with(DATADIR_IDENTITY_PREFIX)
    }) {
        return Err(storage_error(
            "live-state plan base contains non-candidate Meta records",
        ));
    }
    let expected = TridentBlockZeroStorageRecord::from_state_and_header(state, Some(header))
        .map_err(storage_error)?;
    let loaded = crate::load_verified_trident_block_zero(base)?;
    if loaded != expected {
        return Err(storage_error(
            "live-state plan base candidate does not match the verified inputs",
        ));
    }
    Ok(())
}

fn conservation_for(
    state: &TridentBlockZeroState,
) -> Result<Vec<TridentAssetConservation>, StateError> {
    let bonds = validator_bonds_checked(state)?;
    let mut result = Vec::with_capacity(NativeAssetId::ALL.len());
    for asset in NativeAssetId::ALL {
        let supply = state
            .supplies
            .iter()
            .find(|supply| supply.asset == asset)
            .ok_or_else(|| storage_error(format!("{asset} supply record is missing")))?;
        let genesis_allocated = checked_sum(
            state
                .allocations
                .iter()
                .filter(|allocation| allocation.asset == asset)
                .map(|allocation| allocation.amount),
            "allocation",
        )?;
        let validator_bonds = bonds
            .iter()
            .filter(|((wire, _), _)| *wire == asset.wire_byte())
            .try_fold(0u64, |sum, (_, amount)| sum.checked_add(*amount))
            .ok_or_else(|| storage_error(format!("{asset} validator bond overflow")))?;
        let liquid = genesis_allocated
            .checked_sub(validator_bonds)
            .ok_or_else(|| storage_error(format!("{asset} validator bonds exceed allocations")))?;
        let issued = genesis_allocated
            .checked_add(supply.treasury)
            .ok_or_else(|| storage_error(format!("{asset} issued supply overflow")))?;
        let recomposed_max = issued
            .checked_add(supply.staking_reward_reserve)
            .and_then(|amount| amount.checked_add(supply.unissued))
            .ok_or_else(|| storage_error(format!("{asset} supply overflow")))?;
        if genesis_allocated != supply.genesis_allocated
            || liquid
                .checked_add(validator_bonds)
                .ok_or_else(|| storage_error(format!("{asset} holdings overflow")))?
                != genesis_allocated
            || recomposed_max != supply.max_supply
        {
            return Err(storage_error(format!(
                "{asset} live-state plan does not conserve supply"
            )));
        }
        result.push(TridentAssetConservation {
            asset,
            genesis_allocated,
            liquid,
            validator_bonds,
            treasury: supply.treasury,
            issued,
            staking_reward_reserve: supply.staking_reward_reserve,
            unissued: supply.unissued,
            max_supply: supply.max_supply,
        });
    }
    Ok(result)
}

fn validator_bonds(state: &TridentBlockZeroState) -> BTreeMap<(u8, [u8; 20]), u64> {
    let mut bonds = BTreeMap::new();
    for set in &state.validator_sets {
        for validator in &set.validators {
            let amount = bonds
                .entry((set.asset.wire_byte(), validator.operator.0))
                .or_insert(0u64);
            *amount = amount
                .checked_add(validator.self_bond)
                .expect("manifest validation excludes validator bond overflow");
        }
    }
    bonds
}

fn validator_bonds_checked(
    state: &TridentBlockZeroState,
) -> Result<BTreeMap<(u8, [u8; 20]), u64>, StateError> {
    let mut bonds = BTreeMap::new();
    for set in &state.validator_sets {
        for validator in &set.validators {
            let amount = bonds
                .entry((set.asset.wire_byte(), validator.operator.0))
                .or_insert(0u64);
            *amount = amount
                .checked_add(validator.self_bond)
                .ok_or_else(|| storage_error("validator bond overflow"))?;
        }
    }
    Ok(bonds)
}

fn checked_sum(mut values: impl Iterator<Item = u64>, label: &str) -> Result<u64, StateError> {
    values
        .try_fold(0u64, u64::checked_add)
        .ok_or_else(|| storage_error(format!("{label} sum overflow")))
}

fn roots_from_records(records: &[TridentPlannedStoreRecord]) -> TridentLiveStateRoots {
    let roots: BTreeMap<_, _> = TridentLiveStateComponent::ALL
        .iter()
        .copied()
        .map(|component| {
            let mut entries: Vec<(u8, Vec<u8>, Vec<u8>)> = records
                .iter()
                .filter(|record| record.component == Some(component))
                .map(|record| {
                    (
                        record.column_family as u8,
                        record.key.clone(),
                        record.value.clone(),
                    )
                })
                .collect();
            entries.sort();
            (
                component,
                Hash::hash_borsh(&(
                    TRIDENT_LIVE_STATE_COMPONENT_DOMAIN,
                    TRIDENT_LIVE_STATE_PLAN_VERSION,
                    component,
                    entries,
                )),
            )
        })
        .collect();
    let get = |component| {
        *roots
            .get(&component)
            .expect("all live-state components have roots")
    };
    TridentLiveStateRoots {
        identity: get(TridentLiveStateComponent::Identity),
        allocations: get(TridentLiveStateComponent::Allocations),
        utxo: get(TridentLiveStateComponent::Utxo),
        ovl_accounts: get(TridentLiveStateComponent::OvlAccounts),
        drc_accounts: get(TridentLiveStateComponent::DrcAccounts),
        supply: get(TridentLiveStateComponent::Supply),
        treasuries: get(TridentLiveStateComponent::Treasuries),
        vesting: get(TridentLiveStateComponent::Vesting),
        ovl_staking: get(TridentLiveStateComponent::OvlStaking),
        drc_staking: get(TridentLiveStateComponent::DrcStaking),
        acceptance: get(TridentLiveStateComponent::Acceptance),
        finality: get(TridentLiveStateComponent::Finality),
        drc_payments: get(TridentLiveStateComponent::DrcPayments),
        community: get(TridentLiveStateComponent::Community),
    }
}

fn verify_coverage(
    state: &TridentBlockZeroState,
    records: &[TridentPlannedStoreRecord],
) -> Result<Vec<TridentManifestFieldMapping>, StateError> {
    let expected = expected_manifest_fields(state);
    let mut mapped = BTreeMap::new();
    for record in records {
        for field in &record.manifest_fields {
            if mapped
                .insert(
                    field.clone(),
                    TridentManifestFieldMapping {
                        field: field.clone(),
                        column_family: record.column_family,
                        key: record.key.clone(),
                    },
                )
                .is_some()
            {
                return Err(storage_error(format!(
                    "manifest field {field} maps to more than one live-state record"
                )));
            }
        }
    }
    let actual: BTreeSet<_> = mapped.keys().cloned().collect();
    if actual != expected {
        let missing: Vec<_> = expected.difference(&actual).cloned().collect();
        let unexpected: Vec<_> = actual.difference(&expected).cloned().collect();
        return Err(storage_error(format!(
            "manifest coverage mismatch; missing={missing:?}, unexpected={unexpected:?}"
        )));
    }
    Ok(mapped.into_values().collect())
}

fn expected_manifest_fields(state: &TridentBlockZeroState) -> BTreeSet<String> {
    let mut expected: BTreeSet<_> = fields(
        "",
        &[
            "version",
            "chain_id",
            "timestamp_ms",
            "bits",
            "network_fingerprint",
            "artifact_identity",
            "consensus_policy_hash",
            "governance_constitution_hash",
            "emergency_policy_hash",
        ],
    )
    .into_iter()
    .collect();
    for (index, _) in state.allocations.iter().enumerate() {
        expected.extend(fields(
            &format!("allocations[{index}]"),
            &["asset", "address", "amount"],
        ));
    }
    for (index, _) in state.vesting.iter().enumerate() {
        expected.extend(fields(
            &format!("vesting[{index}]"),
            &[
                "asset",
                "address",
                "amount",
                "start_timestamp_ms",
                "cliff_timestamp_ms",
                "end_timestamp_ms",
                "release_policy",
            ],
        ));
    }
    for (index, _) in state.supplies.iter().enumerate() {
        expected.extend(fields(
            &format!("supplies[{index}]"),
            &[
                "asset",
                "max_supply",
                "genesis_allocated",
                "treasury",
                "staking_reward_reserve",
                "unissued",
            ],
        ));
    }
    for (index, _) in state.treasuries.iter().enumerate() {
        expected.extend(fields(
            &format!("treasuries[{index}]"),
            &["treasury", "asset", "control", "balance"],
        ));
    }
    for (set_index, set) in state.validator_sets.iter().enumerate() {
        let prefix = format!("validator_sets[{set_index}]");
        expected.extend(fields(
            &prefix,
            &[
                "asset",
                "epoch",
                "max_validators",
                "min_self_bond",
                "unbonding_period_checkpoints",
                "max_commission_bps",
                "max_concentration_bps",
                "epoch_reserve_drip",
                "total_active_stake",
            ],
        ));
        for (validator_index, _) in set.validators.iter().enumerate() {
            expected.extend(fields(
                &format!("{prefix}.validators[{validator_index}]"),
                &[
                    "operator",
                    "consensus_public_key",
                    "withdrawal_address",
                    "self_bond",
                    "commission_bps",
                    "metadata_hash",
                ],
            ));
        }
    }
    expected.extend(fields(
        "finality",
        &[
            "state",
            "pow_work_met",
            "finalized_blue_score",
            "ovl_snapshot",
            "ovl_active_stake",
            "ovl_signed_stake",
            "drc_snapshot",
            "drc_active_stake",
            "drc_signed_stake",
        ],
    ));
    expected
}

fn fields(prefix: &str, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .map(|name| {
            if prefix.is_empty() {
                (*name).to_string()
            } else {
                format!("{prefix}.{name}")
            }
        })
        .collect()
}

fn canonicalize_records(records: &mut [TridentPlannedStoreRecord]) -> Result<(), StateError> {
    records.sort_by(record_order);
    for pair in records.windows(2) {
        if pair[0].column_family == pair[1].column_family && pair[0].key == pair[1].key {
            return Err(storage_error("duplicate live-state plan store key"));
        }
    }
    Ok(())
}

fn record_order(
    left: &TridentPlannedStoreRecord,
    right: &TridentPlannedStoreRecord,
) -> std::cmp::Ordering {
    (left.column_family as u8, left.key.as_slice())
        .cmp(&(right.column_family as u8, right.key.as_slice()))
}

fn push_encoded<T: BorshSerialize>(
    records: &mut Vec<TridentPlannedStoreRecord>,
    column_family: ColumnFamily,
    key: Vec<u8>,
    value: &T,
    component: Option<TridentLiveStateComponent>,
    manifest_fields: Vec<String>,
) {
    push_record(
        records,
        column_family,
        key,
        borsh::to_vec(value).expect("in-memory Borsh serialization"),
        component,
        manifest_fields,
    );
}

fn push_raw(
    records: &mut Vec<TridentPlannedStoreRecord>,
    column_family: ColumnFamily,
    key: Vec<u8>,
    value: Vec<u8>,
    component: Option<TridentLiveStateComponent>,
) {
    push_record(records, column_family, key, value, component, Vec::new());
}

fn push_record(
    records: &mut Vec<TridentPlannedStoreRecord>,
    column_family: ColumnFamily,
    key: Vec<u8>,
    value: Vec<u8>,
    component: Option<TridentLiveStateComponent>,
    manifest_fields: Vec<String>,
) {
    records.push(TridentPlannedStoreRecord {
        column_family,
        key,
        value,
        component,
        manifest_fields,
    });
}

fn batch_from_records(records: &[TridentPlannedStoreRecord]) -> WriteBatch {
    let mut batch = WriteBatch::new();
    for record in records {
        batch.put_cf(record.column_family, &record.key, &record.value);
    }
    batch
}

type StoreSnapshot = Vec<(ColumnFamily, Vec<crate::store::KvPair>)>;

fn snapshot_store(store: &StateStore) -> Result<StoreSnapshot, StateError> {
    ColumnFamily::ALL
        .iter()
        .copied()
        .map(|cf| Ok((cf, store.scan_prefix(cf, &[])?)))
        .collect()
}

fn allocation_record_key(allocation: &BlockZeroAllocation) -> Vec<u8> {
    let mut key = Vec::with_capacity(ALLOCATION_PREFIX.len() + 1 + 1 + 20);
    key.extend_from_slice(ALLOCATION_PREFIX);
    key.push(allocation.asset.wire_byte());
    key.push(b'/');
    key.extend_from_slice(&allocation.address.0);
    key
}

fn supply_record_key(asset: NativeAssetId) -> Vec<u8> {
    let mut key = SUPPLY_PREFIX.to_vec();
    key.push(asset.wire_byte());
    key
}

fn treasury_record_key(treasury: u8) -> Vec<u8> {
    let mut key = TREASURY_PREFIX.to_vec();
    key.push(treasury);
    key
}

fn vesting_record_key(schedule: &BlockZeroVesting) -> Vec<u8> {
    let mut key = Vec::with_capacity(VESTING_PREFIX.len() + 1 + 1 + 20 + 1 + 32);
    key.extend_from_slice(VESTING_PREFIX);
    key.push(schedule.asset.wire_byte());
    key.push(b'/');
    key.extend_from_slice(&schedule.address.0);
    key.push(b'/');
    key.extend_from_slice(Hash::hash_borsh(schedule).as_bytes());
    key
}

fn staking_policy_key(asset: NativeAssetId) -> Vec<u8> {
    let mut key = STAKING_POLICY_PREFIX.to_vec();
    key.push(asset.wire_byte());
    key
}

fn header_key(header_hash: &Hash) -> Vec<u8> {
    let mut key = HEADER_PREFIX.to_vec();
    key.extend_from_slice(header_hash.as_bytes());
    key
}

fn body_key(header_hash: &Hash) -> Vec<u8> {
    let mut key = BODY_PREFIX.to_vec();
    key.extend_from_slice(header_hash.as_bytes());
    key
}

fn staking_component(asset: NativeAssetId) -> TridentLiveStateComponent {
    match asset {
        NativeAssetId::OVL => TridentLiveStateComponent::OvlStaking,
        NativeAssetId::DRC => TridentLiveStateComponent::DrcStaking,
        // Validation rejects this before a plan can be returned; keeping the
        // projection total avoids a panic when hashing an invalid candidate.
        NativeAssetId::TLT => TridentLiveStateComponent::Identity,
    }
}

fn storage_error(message: impl Into<String>) -> StateError {
    StateError::Storage(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_zero::{BlockZeroVestingPolicy, TridentBlockZeroState};
    use crate::load_verified_trident_block_zero;
    use crate::trident_genesis::{
        TridentGenesisArtifact, TridentGenesisValidator, TridentInitialAllocation,
        TridentVestingSchedule,
    };
    use agora_crypto::KeyPair;
    use agora_types::CheckpointState;

    const DRAFT: &str = include_str!("../../../../docs/genesis/trident.testnet.genesis.draft.json");

    fn synthetic_artifact() -> TridentGenesisArtifact {
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

        let ovl = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let drc = KeyPair::from_secret_bytes(&[8; 32]).unwrap();
        let tlt_address = Address([9; 20]).to_bech32_hrp("agoratest");
        let ovl_address = ovl.address().to_bech32_hrp("agoratest");
        let drc_address = drc.address().to_bech32_hrp("agoratest");
        artifact.assets.ovl.genesis_allocation = 100;
        artifact.assets.drc.genesis_allocation = 200;
        artifact.initial_allocations = vec![
            TridentInitialAllocation {
                asset: "TLT".into(),
                address: tlt_address.clone(),
                amount: artifact.assets.tlt.genesis_allocation,
            },
            TridentInitialAllocation {
                asset: "OVL".into(),
                address: ovl_address.clone(),
                amount: 100,
            },
            TridentInitialAllocation {
                asset: "DRC".into(),
                address: drc_address.clone(),
                amount: 200,
            },
        ];
        artifact.vesting_schedules = vec![
            TridentVestingSchedule {
                asset: "TLT".into(),
                address: tlt_address,
                amount: 100,
                start_timestamp_ms: 10,
                cliff_timestamp_ms: 20,
                end_timestamp_ms: 110,
                release_policy: "linear_from_start_with_cliff_v1".into(),
            },
            TridentVestingSchedule {
                asset: "OVL".into(),
                address: ovl_address.clone(),
                amount: 30,
                start_timestamp_ms: 10,
                cliff_timestamp_ms: 20,
                end_timestamp_ms: 110,
                release_policy: "linear_from_start_with_cliff_v1".into(),
            },
            TridentVestingSchedule {
                asset: "DRC".into(),
                address: drc_address.clone(),
                amount: 50,
                start_timestamp_ms: 10,
                cliff_timestamp_ms: 20,
                end_timestamp_ms: 110,
                release_policy: "linear_from_start_with_cliff_v1".into(),
            },
        ];

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
            asset.treasury_allocation = 10;
            treasury.allocation = 10;
            treasury.control = "ceremony-governance-timelock-v1".into();
        }
        for (set, key, address, bond, metadata_byte) in [
            (&mut artifact.ovl_validators, &ovl, ovl_address, 20, "33"),
            (&mut artifact.drc_validators, &drc, drc_address, 40, "44"),
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

    fn state_and_header() -> (TridentBlockZeroState, TridentHeader) {
        let state = TridentBlockZeroState::from_artifact(&synthetic_artifact()).unwrap();
        let header = state
            .commitment()
            .to_offline_trident_header(
                state.timestamp_ms,
                state.bits,
                7,
                state.canonical_body_root(),
            )
            .unwrap();
        (state, header)
    }

    #[test]
    fn plan_is_deterministic_complete_conserving_and_base_read_only() {
        let (state, header) = state_and_header();
        let base = StateStore::open_in_memory();
        let first = state.plan_live_state(&base, &header).unwrap();
        let second = state.plan_live_state(&base, &header).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.body_root, header.body_root);
        assert_eq!(first.state_root, header.state_root);
        assert_eq!(first.state_root, first.state_roots.state_root());
        assert_eq!(
            first.manifest_mappings.len(),
            expected_manifest_fields(&state).len()
        );
        let mapped: BTreeSet<_> = first
            .manifest_mappings
            .iter()
            .map(|mapping| mapping.field.as_str())
            .collect();
        assert_eq!(mapped.len(), first.manifest_mappings.len());
        assert!(first
            .records
            .windows(2)
            .all(|pair| { record_order(&pair[0], &pair[1]) == std::cmp::Ordering::Less }));

        for proof in &first.conservation {
            assert_eq!(
                proof.liquid + proof.validator_bonds,
                proof.genesis_allocated
            );
            assert_eq!(proof.genesis_allocated + proof.treasury, proof.issued);
            assert_eq!(
                proof.issued + proof.staking_reward_reserve + proof.unissued,
                proof.max_supply
            );
        }
        for cf in ColumnFamily::ALL {
            assert!(base.scan_prefix(cf, &[]).unwrap().is_empty());
        }

        let overlay = first.staged_overlay(&base).unwrap();
        assert!(!overlay
            .scan_prefix(ColumnFamily::Utxo, &[])
            .unwrap()
            .is_empty());
        assert!(base
            .scan_prefix(ColumnFamily::Utxo, &[])
            .unwrap()
            .is_empty());
        first.verify_overlay(&overlay).unwrap();
    }

    #[test]
    fn plan_rejects_duplicate_tampered_and_mismatched_inputs() {
        let (state, header) = state_and_header();
        let base = StateStore::open_in_memory();
        let plan = state.plan_live_state(&base, &header).unwrap();

        let mut tampered = plan.clone();
        let state_record = tampered
            .records
            .iter_mut()
            .find(|record| record.component.is_some())
            .unwrap();
        state_record.value[0] ^= 1;
        assert!(tampered
            .verify(&state, &header)
            .unwrap_err()
            .to_string()
            .contains("tampered"));

        let mut duplicate = plan.clone();
        duplicate.records.push(duplicate.records[0].clone());
        assert!(duplicate.verify(&state, &header).is_err());

        let mut duplicate_state = state.clone();
        duplicate_state
            .allocations
            .push(state.allocations[0].clone());
        assert!(duplicate_state.verify().is_err());

        let wrong_body_header = state
            .commitment()
            .to_offline_trident_header(state.timestamp_ms, state.bits, 7, Hash([0x77; 32]))
            .unwrap();
        assert!(state
            .plan_live_state(&base, &wrong_body_header)
            .unwrap_err()
            .to_string()
            .contains("body root mismatch"));

        let mut wrong_state_header = header;
        wrong_state_header.state_root = Hash([0x88; 32]);
        assert!(state
            .plan_live_state(&base, &wrong_state_header)
            .unwrap_err()
            .to_string()
            .contains("state root mismatch"));
    }

    #[test]
    fn accounts_stake_treasuries_and_finality_are_asset_isolated() {
        let (state, header) = state_and_header();
        let base = StateStore::open_in_memory();
        let plan = state.plan_live_state(&base, &header).unwrap();
        let overlay = plan.staged_overlay(&base).unwrap();

        let ovl_set = &state.validator_sets[0];
        let drc_set = &state.validator_sets[1];
        let ovl_operator = ovl_set.validators[0].operator;
        let drc_operator = drc_set.validators[0].operator;
        assert_eq!(
            crate::load_account(&overlay, NativeAssetId::OVL, &ovl_operator)
                .unwrap()
                .balance,
            80
        );
        assert_eq!(
            crate::load_account(&overlay, NativeAssetId::DRC, &drc_operator)
                .unwrap()
                .balance,
            160
        );
        assert!(overlay
            .get_cf(
                ColumnFamily::Meta,
                &account_key(NativeAssetId::TLT, &Address([9; 20]))
            )
            .unwrap()
            .is_none());

        assert_eq!(
            crate::load_validator(&overlay, NativeAssetId::OVL, &ovl_operator)
                .unwrap()
                .unwrap()
                .self_bond,
            20
        );
        assert_eq!(
            crate::load_validator(&overlay, NativeAssetId::DRC, &drc_operator)
                .unwrap()
                .unwrap()
                .self_bond,
            40
        );
        assert_eq!(
            crate::load_snapshot(&overlay, NativeAssetId::OVL, 0)
                .unwrap()
                .unwrap()
                .commitment(),
            state.finality.ovl_snapshot
        );
        assert_eq!(
            crate::load_snapshot(&overlay, NativeAssetId::DRC, 0)
                .unwrap()
                .unwrap()
                .commitment(),
            state.finality.drc_snapshot
        );

        let finality_bytes = overlay
            .get_cf(ColumnFamily::Meta, INITIAL_FINALITY_KEY)
            .unwrap()
            .unwrap();
        let finality = TridentLiveInitialFinalityRecord::try_from_slice(&finality_bytes).unwrap();
        assert_eq!(finality.finality.state, CheckpointState::Proposed);
        assert!(!finality.finality.pow_work_met);
        assert_eq!(finality.finality.ovl_signed_stake, 0);
        assert_eq!(finality.finality.drc_signed_stake, 0);

        for treasury in &state.treasuries {
            let bytes = overlay
                .get_cf(
                    ColumnFamily::Meta,
                    &treasury_record_key(treasury.treasury.wire_byte()),
                )
                .unwrap()
                .unwrap();
            let record = TridentLiveTreasuryRecord::try_from_slice(&bytes).unwrap();
            assert_eq!(record.treasury, *treasury);
            assert_eq!(record.treasury.asset, record.treasury.treasury.asset());
        }
    }

    #[test]
    fn vesting_policy_is_explicit_enforceable_and_disjoint_from_bonds() {
        let (state, header) = state_and_header();
        let base = StateStore::open_in_memory();
        let plan = state.plan_live_state(&base, &header).unwrap();
        let locks: Vec<_> = plan
            .records
            .iter()
            .filter(|record| record.key.starts_with(VESTING_PREFIX))
            .map(|record| TridentLiveVestingLockRecord::try_from_slice(&record.value).unwrap())
            .collect();
        assert_eq!(locks.len(), 3);
        for lock in &locks {
            assert_eq!(
                lock.schedule.release_policy,
                BlockZeroVestingPolicy::LinearFromStartWithCliffV1
            );
            assert_eq!(lock.locked_amount_at(19).unwrap(), lock.schedule.amount);
            assert!(lock.locked_amount_at(20).unwrap() < lock.schedule.amount);
            assert_eq!(lock.locked_amount_at(110).unwrap(), 0);
        }
        assert!(locks
            .iter()
            .any(|lock| matches!(lock.funding, TridentVestingFunding::TltOutPoint(_))));
        assert!(locks.iter().any(|lock| matches!(
            lock.funding,
            TridentVestingFunding::Account {
                asset: NativeAssetId::OVL,
                ..
            }
        )));

        let mut overlocked = synthetic_artifact();
        overlocked.vesting_schedules[1].amount = 81;
        overlocked.genesis_hash = overlocked.consensus_identity_hash().to_hex();
        overlocked.network_fingerprint = overlocked.compute_network_fingerprint().to_hex();
        assert!(TridentBlockZeroState::from_artifact(&overlocked)
            .unwrap_err()
            .contains("vesting and validator locks exceed"));

        let mut ambiguous = synthetic_artifact();
        ambiguous.vesting_schedules[0].release_policy = "UNFROZEN".into();
        ambiguous.genesis_hash = ambiguous.consensus_identity_hash().to_hex();
        ambiguous.network_fingerprint = ambiguous.compute_network_fingerprint().to_hex();
        assert!(TridentBlockZeroState::from_artifact(&ambiguous)
            .unwrap_err()
            .contains("release_policy"));
    }

    #[test]
    fn verified_candidate_base_can_be_planned_without_live_writes() {
        let (state, header) = state_and_header();
        let base = StateStore::open_in_memory();
        state
            .persist_verified_store_record_with_header(&base, &header)
            .unwrap();
        let before = snapshot_store(&base).unwrap();
        let plan = state.plan_live_state(&base, &header).unwrap();
        assert_eq!(snapshot_store(&base).unwrap(), before);
        assert_eq!(
            load_verified_trident_block_zero(&base).unwrap().manifest,
            state
        );
        let overlay = plan.staged_overlay(&base).unwrap();
        assert!(overlay
            .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
            .unwrap()
            .is_some());
        assert!(base
            .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
            .unwrap()
            .is_none());
    }

    #[test]
    fn atomic_commit_reopens_idempotently_and_uses_one_durable_batch() {
        let (state, header) = state_and_header();
        let store = StateStore::open_in_memory();
        let plan = state.plan_live_state(&store, &header).unwrap();

        let ready = plan.commit_atomically(&store, &state, &header).unwrap();
        assert_eq!(store.batch_write_calls_for_test(), 1);
        assert_eq!(ready.manifest_root(), state.manifest_root());
        assert_eq!(ready.body_root(), header.body_root);
        assert_eq!(ready.state_roots(), &plan.state_roots);
        assert_eq!(ready.state_root(), header.state_root);
        assert_eq!(
            ready.header_hash(),
            header.commitment_hash().expect("verified header")
        );
        assert_eq!(
            ready.datadir_identity().block_zero_header_hash,
            Some(ready.header_hash())
        );

        let committed = snapshot_store(&store).unwrap();
        let reopened = crate::reopen_verified_trident_live_state(&store, &state, &header).unwrap();
        assert_eq!(reopened, ready);
        let idempotent = plan.commit_atomically(&store, &state, &header).unwrap();
        assert_eq!(idempotent, ready);
        assert_eq!(store.batch_write_calls_for_test(), 1);
        assert_eq!(snapshot_store(&store).unwrap(), committed);
    }

    #[test]
    fn injected_atomic_write_failure_returns_no_readiness_and_leaves_store_empty() {
        let (state, header) = state_and_header();
        let store = StateStore::open_in_memory();
        let plan = state.plan_live_state(&store, &header).unwrap();
        store.fail_next_batch_write_for_test();

        let error = plan.commit_atomically(&store, &state, &header).unwrap_err();
        assert!(error.to_string().contains("injected batch write failure"));
        assert_eq!(store.batch_write_calls_for_test(), 0);
        for cf in ColumnFamily::ALL {
            assert!(store.scan_prefix(cf, &[]).unwrap().is_empty());
        }
        assert!(crate::reopen_verified_trident_live_state(&store, &state, &header).is_err());
    }

    #[test]
    fn partial_existing_state_is_rejected_without_overwrite() {
        let (state, header) = state_and_header();
        let planning_store = StateStore::open_in_memory();
        let plan = state
            .plan_live_state(&planning_store, &header)
            .expect("independent plan");
        let store = StateStore::open_in_memory();
        store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::TRIDENT_LIVE_STATE_STATE_ROOT,
                plan.state_root.as_bytes(),
            )
            .unwrap();
        let before = snapshot_store(&store).unwrap();

        let error = plan.commit_atomically(&store, &state, &header).unwrap_err();
        assert!(error.to_string().contains("partial, mismatched"));
        assert_eq!(store.batch_write_calls_for_test(), 0);
        assert_eq!(snapshot_store(&store).unwrap(), before);
    }

    #[test]
    fn durable_record_and_root_tamper_fail_closed_without_repair() {
        let (state, header) = state_and_header();
        let store = StateStore::open_in_memory();
        let plan = state.plan_live_state(&store, &header).unwrap();
        plan.commit_atomically(&store, &state, &header).unwrap();

        let utxo = plan
            .records()
            .iter()
            .find(|record| record.component() == Some(TridentLiveStateComponent::Utxo))
            .expect("planned UTXO");
        let mut changed = utxo.value().to_vec();
        changed[0] ^= 1;
        store
            .put_cf(utxo.column_family(), utxo.key(), &changed)
            .unwrap();
        let tampered = snapshot_store(&store).unwrap();
        assert!(crate::reopen_verified_trident_live_state(&store, &state, &header).is_err());
        assert!(plan.commit_atomically(&store, &state, &header).is_err());
        assert_eq!(snapshot_store(&store).unwrap(), tampered);
        assert_eq!(store.batch_write_calls_for_test(), 1);

        store
            .put_cf(utxo.column_family(), utxo.key(), utxo.value())
            .unwrap();
        store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::TRIDENT_LIVE_STATE_STATE_ROOT,
                &[0x99; 32],
            )
            .unwrap();
        let wrong_root = snapshot_store(&store).unwrap();
        assert!(crate::reopen_verified_trident_live_state(&store, &state, &header).is_err());
        assert_eq!(snapshot_store(&store).unwrap(), wrong_root);
        assert_eq!(store.batch_write_calls_for_test(), 1);
    }

    #[test]
    fn mismatched_verified_inputs_never_overwrite_an_exact_commit() {
        let (state, header) = state_and_header();
        let store = StateStore::open_in_memory();
        let plan = state.plan_live_state(&store, &header).unwrap();
        plan.commit_atomically(&store, &state, &header).unwrap();
        let before = snapshot_store(&store).unwrap();

        let mut wrong_header = header.clone();
        wrong_header.state_root = Hash([0x88; 32]);
        let error =
            crate::reopen_verified_trident_live_state(&store, &state, &wrong_header).unwrap_err();
        assert!(error.to_string().contains("state root mismatch"));
        assert_eq!(snapshot_store(&store).unwrap(), before);
        assert_eq!(store.batch_write_calls_for_test(), 1);
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_reopen_proves_overlay_plan_never_reaches_base() {
        let dir = std::env::temp_dir().join(format!(
            "agora-live-plan-cow-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let (state, header) = state_and_header();
        {
            let base = StateStore::open(&dir).unwrap();
            let plan = state.plan_live_state(&base, &header).unwrap();
            let overlay = plan.staged_overlay(&base).unwrap();
            assert!(!overlay
                .scan_prefix(ColumnFamily::Utxo, &[])
                .unwrap()
                .is_empty());
            assert!(base
                .scan_prefix(ColumnFamily::Utxo, &[])
                .unwrap()
                .is_empty());
        }
        {
            let reopened = StateStore::open(&dir).unwrap();
            for cf in ColumnFamily::ALL {
                assert!(reopened.scan_prefix(cf, &[]).unwrap().is_empty());
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_atomic_commit_reopens_exactly_without_network_side_effects() {
        let root = std::env::temp_dir().join(format!(
            "agora-live-commit-reopen-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let state_dir = root.join("state");
        let _ = std::fs::remove_dir_all(&root);
        let (state, header) = state_and_header();
        let plan;
        let expected;
        {
            let store = StateStore::open(&state_dir).unwrap();
            plan = state.plan_live_state(&store, &header).unwrap();
            expected = plan.commit_atomically(&store, &state, &header).unwrap();
            assert_eq!(store.batch_write_calls_for_test(), 1);
        }
        assert!(!root.join("p2p").exists());
        assert!(!root.join("rpc").exists());
        {
            let reopened_store = StateStore::open(&state_dir).unwrap();
            let reopened =
                crate::reopen_verified_trident_live_state(&reopened_store, &state, &header)
                    .unwrap();
            assert_eq!(reopened, expected);
            assert_eq!(
                plan.commit_atomically(&reopened_store, &state, &header)
                    .unwrap(),
                expected
            );
            assert_eq!(reopened_store.batch_write_calls_for_test(), 0);
        }
        assert!(!root.join("p2p").exists());
        assert!(!root.join("rpc").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_injected_write_failure_and_partial_reopen_fail_closed() {
        let root = std::env::temp_dir().join(format!(
            "agora-live-commit-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let (state, header) = state_and_header();
        let planning_store = StateStore::open_in_memory();
        let plan = state.plan_live_state(&planning_store, &header).unwrap();
        {
            let store = StateStore::open(&root).unwrap();
            store.fail_next_batch_write_for_test();
            assert!(plan.commit_atomically(&store, &state, &header).is_err());
            for cf in ColumnFamily::ALL {
                assert!(store.scan_prefix(cf, &[]).unwrap().is_empty());
            }
            store
                .put_cf(
                    ColumnFamily::Meta,
                    meta_keys::TRIDENT_LIVE_STATE_STATE_ROOT,
                    plan.state_root.as_bytes(),
                )
                .unwrap();
        }
        {
            let store = StateStore::open(&root).unwrap();
            let before = snapshot_store(&store).unwrap();
            assert!(plan.commit_atomically(&store, &state, &header).is_err());
            assert!(crate::reopen_verified_trident_live_state(&store, &state, &header).is_err());
            assert_eq!(snapshot_store(&store).unwrap(), before);
            assert_eq!(store.batch_write_calls_for_test(), 0);
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
