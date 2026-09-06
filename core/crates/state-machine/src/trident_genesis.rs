//! Offline validation for the human-owned Agora Trident genesis artifact.
//!
//! This module deliberately has no conversion into [`crate::ChainParams`]. A v3
//! document can be inspected and prepared for a freeze ceremony without making
//! it possible for the current v2 node loader to boot it.

use std::collections::BTreeSet;

use agora_consensus::{DaaConfig, EmissionSchedule, GhostdagConfig, PowAlgorithm};
use agora_crypto::parse_compressed_public_key;
use agora_types::{Address, Hash, NativeAssetId};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::data_availability::{DataAvailabilityRuntimeConfig, TridentDataAvailabilityPolicy};
use crate::monetary::{AssetMonetaryPolicy, EmissionKind, TridentMonetaryPolicy};
use crate::network::NetworkId;
use crate::staking::{StakingParams, MAX_VALIDATOR_COMMISSION_BPS};

pub const TRIDENT_GENESIS_SCHEMA: &str = "agora-trident-genesis-v3";
/// State-transition version committed into the Trident network fingerprint.
pub const TRIDENT_STATE_TRANSITION_VERSION: &str = "agora-trident-state-v7";
/// Consensus-policy version string for Trident.
pub const TRIDENT_CONSENSUS_POLICY_VERSION: &str = "agora-trident-consensus-v3";
pub const TRIDENT_NET_FP_DOMAIN: &[u8] = b"agora-trident-net-fp-v1";
pub const TRIDENT_GENESIS_ID_DOMAIN: &[u8] = b"agora-trident-genesis-identity-v3";
pub const TRIDENT_CONSENSUS_POLICY_DOMAIN: &[u8] = b"agora-trident-consensus-policy-v3";
/// v5 adds the authenticated block-only DA commitment lane.
pub const TRIDENT_PROTOCOL_VERSION: u32 = 5;
pub const TRIDENT_TX_SIGNING_VERSION: &str = "agora-trident-tx-v1";
const UNFROZEN: &str = "UNFROZEN";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentWalletPolicy {
    pub address_hrp: String,
    pub coin_type: u32,
    pub coin_type_status: String,
    pub bip44_path_account0: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentEmissionPolicy {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub initial_reward: Option<u64>,
    #[serde(default)]
    pub halving_interval: Option<u64>,
    #[serde(default)]
    pub reserve_base_units: Option<u64>,
    #[serde(default)]
    pub epoch_reserve_drip: Option<u64>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentAssetPolicy {
    pub id: String,
    pub name: String,
    pub max_supply: u64,
    pub decimals: u8,
    pub mineable: bool,
    pub pow_algorithm: Option<String>,
    pub genesis_allocation: u64,
    pub staking_reward_reserve: u64,
    pub treasury_allocation: u64,
    pub emission: TridentEmissionPolicy,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentAssets {
    #[serde(rename = "TLT")]
    pub tlt: TridentAssetPolicy,
    #[serde(rename = "OVL")]
    pub ovl: TridentAssetPolicy,
    #[serde(rename = "DRC")]
    pub drc: TridentAssetPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentTreasury {
    pub asset: String,
    pub control: String,
    pub allocation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentTreasuries {
    pub tlt_security: TridentTreasury,
    pub ovl_builder: TridentTreasury,
    pub drc_community: TridentTreasury,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentPowPolicy {
    pub algorithm: String,
    pub ghostdag_k: u32,
    pub target_block_time_ms: u64,
    pub daa_window_size: u64,
    pub daa_max_adjustment_bits: u32,
    pub daa_min_level: u32,
    pub daa_max_level: u32,
}

/// Finality parameters (independent PoS quorums; no price oracle).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentFinalityPolicy {
    pub model: String,
    pub pow_work_threshold_policy: String,
    /// Ceremony-selected threshold interpreted according to `pow_work_threshold_policy`.
    #[serde(default)]
    pub pow_work_threshold: Option<u64>,
    pub ovl_quorum_numerator: u32,
    pub ovl_quorum_denominator: u32,
    pub drc_quorum_numerator: u32,
    pub drc_quorum_denominator: u32,
    pub combine_stakes_by_price: bool,
    pub admin_bypass: bool,
}

impl TridentFinalityPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.model != "pow_plus_dual_pos" {
            return Err("finality model must be pow_plus_dual_pos".into());
        }
        if self.combine_stakes_by_price {
            return Err("combine_stakes_by_price must be false".into());
        }
        if self.admin_bypass {
            return Err("admin_bypass must be false".into());
        }
        for (numerator, denominator, label) in [
            (
                self.ovl_quorum_numerator,
                self.ovl_quorum_denominator,
                "OVL",
            ),
            (
                self.drc_quorum_numerator,
                self.drc_quorum_denominator,
                "DRC",
            ),
        ] {
            if denominator == 0 || numerator == 0 || numerator > denominator {
                return Err(format!("invalid {label} quorum {numerator}/{denominator}"));
            }
            if u64::from(numerator) * 3 < u64::from(denominator) * 2 {
                return Err(format!("{label} quorum must be at least two thirds"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentGenesisValidator {
    pub consensus_public_key: String,
    pub withdrawal_address: String,
    pub self_bond: u64,
    /// Ceremony-selected commission. `None` is accepted only in draft artifacts.
    #[serde(default)]
    pub commission_bps: Option<u16>,
    /// Ceremony-selected 32-byte metadata commitment encoded as lowercase hex.
    #[serde(default)]
    pub metadata_hash: Option<String>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Default,
)]
#[serde(deny_unknown_fields)]
pub struct TridentValidatorGenesis {
    pub max_validators: u32,
    pub min_self_bond: u64,
    pub unbonding_period_checkpoints: u64,
    #[serde(default)]
    pub max_commission_bps: Option<u16>,
    #[serde(default)]
    pub max_concentration_bps: Option<u16>,
    pub genesis_set: Vec<TridentGenesisValidator>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentVestingSchedule {
    pub asset: String,
    pub address: String,
    pub amount: u64,
    pub start_timestamp_ms: u64,
    pub cliff_timestamp_ms: u64,
    pub end_timestamp_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentInitialAllocation {
    pub asset: String,
    pub address: String,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentGenesisArtifact {
    pub network: NetworkId,
    pub version: u32,
    pub schema: String,
    pub maturity: String,
    pub chain_name: String,
    pub chain_id: String,
    pub timestamp_ms: u64,
    /// Optional in drafts because the human ceremony has not selected it yet.
    #[serde(default)]
    pub bits: Option<u32>,
    pub genesis_hash: String,
    pub network_fingerprint: String,
    pub state_transition_version: String,
    pub consensus_policy_version: String,
    pub governance_constitution_hash: String,
    pub emergency_policy_hash: String,
    pub decimals: u8,
    pub wallet: TridentWalletPolicy,
    pub assets: TridentAssets,
    pub treasuries: TridentTreasuries,
    pub pow: TridentPowPolicy,
    pub finality: TridentFinalityPolicy,
    pub data_availability: TridentDataAvailabilityPolicy,
    pub ovl_validators: TridentValidatorGenesis,
    pub drc_validators: TridentValidatorGenesis,
    pub vesting_schedules: Vec<TridentVestingSchedule>,
    pub initial_allocations: Vec<TridentInitialAllocation>,
    pub notes: Vec<String>,
}

#[derive(BorshSerialize)]
struct ConsensusIdentity<'a> {
    domain: &'a [u8],
    network: &'a str,
    version: u32,
    schema: &'a str,
    chain_name: &'a str,
    chain_id: &'a str,
    timestamp_ms: u64,
    bits: Option<u32>,
    state_transition_version: &'a str,
    consensus_policy_version: &'a str,
    governance_constitution_hash: &'a str,
    emergency_policy_hash: &'a str,
    decimals: u8,
    wallet: &'a TridentWalletPolicy,
    assets: &'a TridentAssets,
    treasuries: &'a TridentTreasuries,
    pow: &'a TridentPowPolicy,
    finality: &'a TridentFinalityPolicy,
    data_availability: &'a TridentDataAvailabilityPolicy,
    ovl_validators: &'a TridentValidatorGenesis,
    drc_validators: &'a TridentValidatorGenesis,
    vesting_schedules: &'a [TridentVestingSchedule],
    initial_allocations: &'a [TridentInitialAllocation],
}

/// Fully typed policy derived only from a freeze-ready Trident artifact.
///
/// This is intentionally not a node boot configuration: canonical Block 0 can
/// be derived and stored as a verified Meta envelope, but live balances and
/// header identity are still incomplete.
#[derive(Debug, Clone)]
pub struct TridentRuntimePolicy {
    pub network: NetworkId,
    pub chain_id: String,
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
    pub network_fingerprint: Hash,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub pow_algorithm: PowAlgorithm,
    pub daa: DaaConfig,
    pub ghostdag: GhostdagConfig,
    pub tlt_emission: EmissionSchedule,
    pub monetary: TridentMonetaryPolicy,
    pub ovl_staking: StakingParams,
    pub drc_staking: StakingParams,
    pub finality: TridentRuntimeFinalityPolicy,
    pub data_availability: TridentDataAvailabilityPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TridentRuntimeFinalityPolicy {
    pub min_pow_depth: u64,
    pub ovl_quorum_numerator: u32,
    pub ovl_quorum_denominator: u32,
    pub drc_quorum_numerator: u32,
    pub drc_quorum_denominator: u32,
}

impl TridentRuntimePolicy {
    /// Build the exact state-machine context committed by this runtime policy.
    pub fn data_availability_runtime_config(
        &self,
    ) -> Result<DataAvailabilityRuntimeConfig, String> {
        DataAvailabilityRuntimeConfig::new(self.network_fingerprint, self.data_availability.clone())
    }
}

impl TridentGenesisArtifact {
    pub fn validate_draft(&self) -> Result<(), String> {
        if self.version != 3 {
            return Err(format!("expected version 3, got {}", self.version));
        }
        if self.schema != TRIDENT_GENESIS_SCHEMA {
            return Err(format!("unexpected schema {}", self.schema));
        }
        if self.chain_name.trim().is_empty() || self.chain_id.trim().is_empty() {
            return Err("chain_name and chain_id are required".into());
        }
        if self.state_transition_version != TRIDENT_STATE_TRANSITION_VERSION {
            return Err("state_transition_version mismatch".into());
        }
        if self.consensus_policy_version != TRIDENT_CONSENSUS_POLICY_VERSION {
            return Err("consensus_policy_version mismatch".into());
        }
        if self.decimals != 8 {
            return Err("Trident v3 requires 8 decimals".into());
        }
        if self.wallet.address_hrp != self.network.address_hrp() {
            return Err(format!(
                "wallet HRP {} does not match {}",
                self.wallet.address_hrp, self.network
            ));
        }
        if self.wallet.coin_type != self.network.coin_type() {
            return Err("wallet coin type does not match network policy".into());
        }
        self.validate_assets()?;
        self.validate_treasuries()?;
        self.validate_pow()?;
        self.finality.validate()?;
        self.data_availability.validate_draft()?;
        self.validate_populated_validator_set("OVL", &self.ovl_validators)?;
        self.validate_populated_validator_set("DRC", &self.drc_validators)?;
        self.validate_populated_allocations()?;
        Ok(())
    }

    pub fn validate_freeze_ready(&self) -> Result<(), String> {
        self.validate_draft()?;
        if self.timestamp_ms == 0 {
            return Err("timestamp_ms is still zero".into());
        }
        if self.bits.is_none() {
            return Err("bits is not selected".into());
        }
        for (label, value) in [
            (
                "governance_constitution_hash",
                &self.governance_constitution_hash,
            ),
            ("emergency_policy_hash", &self.emergency_policy_hash),
        ] {
            validate_frozen_hash(label, value)?;
        }
        if is_placeholder(&self.finality.pow_work_threshold_policy) {
            return Err("pow_work_threshold_policy is still a placeholder".into());
        }
        if self.finality.pow_work_threshold_policy != "minimum-blue-score-depth-v1" {
            return Err("unsupported pow_work_threshold_policy".into());
        }
        if self.finality.pow_work_threshold.unwrap_or(0) == 0 {
            return Err("pow_work_threshold is not selected".into());
        }
        if is_placeholder(&self.maturity) || self.maturity.contains("draft") {
            return Err("maturity still marks this artifact as a draft".into());
        }
        if self.notes.iter().any(|note| {
            let note = note.to_ascii_lowercase();
            note.contains("draft") || note.contains("unfrozen") || note.contains("placeholder")
        }) {
            return Err("artifact notes still describe draft placeholders".into());
        }
        if is_placeholder(&self.wallet.coin_type_status)
            || self.wallet.coin_type_status.contains("pending")
            || self.wallet.coin_type_status.contains("provisional")
        {
            return Err("wallet coin_type_status is not frozen".into());
        }
        self.require_freeze_ready_assets()?;
        self.require_freeze_ready_treasuries()?;
        self.data_availability.validate_freeze_ready()?;
        self.require_freeze_ready_validator_set("OVL", &self.ovl_validators)?;
        self.require_freeze_ready_validator_set("DRC", &self.drc_validators)?;
        self.require_allocation_totals()?;

        let expected_genesis = self.consensus_identity_hash().to_hex();
        if self.genesis_hash != expected_genesis {
            return Err(format!(
                "genesis_hash mismatch: expected {expected_genesis}, got {}",
                self.genesis_hash
            ));
        }
        let expected_fingerprint = self.compute_network_fingerprint().to_hex();
        if self.network_fingerprint != expected_fingerprint {
            return Err(format!(
                "network_fingerprint mismatch: expected {expected_fingerprint}, got {}",
                self.network_fingerprint
            ));
        }
        Ok(())
    }

    /// Convert a strictly validated artifact into all currently representable
    /// runtime policy in one place. This performs no storage or network I/O.
    pub fn to_runtime_policy(&self) -> Result<TridentRuntimePolicy, String> {
        self.validate_freeze_ready()?;

        let initial_reward = self
            .assets
            .tlt
            .emission
            .initial_reward
            .ok_or_else(|| "TLT emission initial_reward is not selected".to_string())?;
        let halving_interval = self
            .assets
            .tlt
            .emission
            .halving_interval
            .ok_or_else(|| "TLT emission halving_interval is not selected".to_string())?;
        if initial_reward == 0 || halving_interval == 0 {
            return Err("TLT emission policy contains zero".into());
        }

        let monetary = TridentMonetaryPolicy {
            tlt: self.asset_runtime_policy(NativeAssetId::TLT, &self.assets.tlt)?,
            ovl: self.asset_runtime_policy(NativeAssetId::OVL, &self.assets.ovl)?,
            drc: self.asset_runtime_policy(NativeAssetId::DRC, &self.assets.drc)?,
        };
        monetary.validate()?;

        let artifact_identity = self.consensus_identity_hash();
        let consensus_policy_hash = self.consensus_policy_hash();
        let network_fingerprint = self.compute_network_fingerprint();
        Ok(TridentRuntimePolicy {
            network: self.network,
            chain_id: self.chain_id.clone(),
            artifact_identity,
            consensus_policy_hash,
            network_fingerprint,
            timestamp_ms: self.timestamp_ms,
            bits: self.bits.expect("freeze-ready validation requires bits"),
            pow_algorithm: PowAlgorithm::RandomX,
            daa: DaaConfig {
                target_block_time_ms: self.pow.target_block_time_ms,
                window_size: self.pow.daa_window_size,
                max_adjustment_bits: self.pow.daa_max_adjustment_bits,
                min_level: self.pow.daa_min_level,
                max_level: self.pow.daa_max_level,
            },
            ghostdag: GhostdagConfig {
                k: self.pow.ghostdag_k,
            },
            tlt_emission: EmissionSchedule {
                initial_reward,
                halving_interval,
            },
            monetary,
            ovl_staking: self.staking_runtime_policy(
                NativeAssetId::OVL,
                &self.ovl_validators,
                &self.assets.ovl,
            )?,
            drc_staking: self.staking_runtime_policy(
                NativeAssetId::DRC,
                &self.drc_validators,
                &self.assets.drc,
            )?,
            finality: TridentRuntimeFinalityPolicy {
                min_pow_depth: self
                    .finality
                    .pow_work_threshold
                    .expect("freeze-ready validation requires finality threshold"),
                ovl_quorum_numerator: self.finality.ovl_quorum_numerator,
                ovl_quorum_denominator: self.finality.ovl_quorum_denominator,
                drc_quorum_numerator: self.finality.drc_quorum_numerator,
                drc_quorum_denominator: self.finality.drc_quorum_denominator,
            },
            data_availability: self.data_availability.clone(),
        })
    }

    pub fn consensus_identity_hash(&self) -> Hash {
        Hash::hash_borsh(&ConsensusIdentity {
            domain: TRIDENT_GENESIS_ID_DOMAIN,
            network: self.network.as_str(),
            version: self.version,
            schema: &self.schema,
            chain_name: &self.chain_name,
            chain_id: &self.chain_id,
            timestamp_ms: self.timestamp_ms,
            bits: self.bits,
            state_transition_version: &self.state_transition_version,
            consensus_policy_version: &self.consensus_policy_version,
            governance_constitution_hash: &self.governance_constitution_hash,
            emergency_policy_hash: &self.emergency_policy_hash,
            decimals: self.decimals,
            wallet: &self.wallet,
            assets: &self.assets,
            treasuries: &self.treasuries,
            pow: &self.pow,
            finality: &self.finality,
            data_availability: &self.data_availability,
            ovl_validators: &self.ovl_validators,
            drc_validators: &self.drc_validators,
            vesting_schedules: &self.vesting_schedules,
            initial_allocations: &self.initial_allocations,
        })
    }

    pub fn compute_network_fingerprint(&self) -> Hash {
        let identity = self.consensus_identity_hash();
        let policy = self.consensus_policy_hash();
        Hash::hash_borsh(&(
            TRIDENT_NET_FP_DOMAIN,
            TRIDENT_PROTOCOL_VERSION,
            self.chain_id.as_str(),
            identity.as_bytes(),
            policy.as_bytes(),
            TRIDENT_TX_SIGNING_VERSION,
            TRIDENT_STATE_TRANSITION_VERSION,
            TRIDENT_CONSENSUS_POLICY_VERSION,
        ))
    }

    /// Canonical digest of policy that runtime consensus must consume.
    pub fn consensus_policy_hash(&self) -> Hash {
        Hash::hash_borsh(&(
            TRIDENT_CONSENSUS_POLICY_DOMAIN,
            self.consensus_policy_version.as_str(),
            &self.assets,
            &self.pow,
            &self.finality,
            &self.data_availability,
            &self.ovl_validators,
            &self.drc_validators,
        ))
    }

    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| error.to_string())
    }

    fn validate_assets(&self) -> Result<(), String> {
        for (ticker, id, policy) in [
            ("TLT", "0x00", &self.assets.tlt),
            ("OVL", "0x01", &self.assets.ovl),
            ("DRC", "0x02", &self.assets.drc),
        ] {
            if policy.id != id {
                return Err(format!("{ticker} id must be {id}"));
            }
            if policy.max_supply == 0 || policy.decimals != self.decimals {
                return Err(format!("{ticker} supply/decimals policy is invalid"));
            }
            let committed = policy
                .genesis_allocation
                .checked_add(policy.staking_reward_reserve)
                .and_then(|value| value.checked_add(policy.treasury_allocation))
                .ok_or_else(|| format!("{ticker} allocation overflow"))?;
            if committed > policy.max_supply {
                return Err(format!("{ticker} allocations exceed max_supply"));
            }
        }
        if !self.assets.tlt.mineable || self.assets.tlt.pow_algorithm.as_deref() != Some("randomx")
        {
            return Err("TLT must be mineable only with randomx".into());
        }
        for (ticker, policy) in [("OVL", &self.assets.ovl), ("DRC", &self.assets.drc)] {
            if policy.mineable || policy.pow_algorithm.is_some() {
                return Err(format!("{ticker} must never be mineable"));
            }
        }
        if self.assets.tlt.staking_reward_reserve != 0 {
            return Err("TLT cannot have a staking reward reserve".into());
        }
        if self.assets.tlt.emission.reserve_base_units.is_some()
            || self.assets.tlt.emission.epoch_reserve_drip.is_some()
        {
            return Err("TLT cannot have staking reserve emission fields".into());
        }
        if self.assets.tlt.emission.initial_reward == Some(0)
            || self.assets.tlt.emission.halving_interval == Some(0)
        {
            return Err("TLT emission policy contains zero".into());
        }
        for (ticker, policy) in [("OVL", &self.assets.ovl), ("DRC", &self.assets.drc)] {
            if policy.emission.reserve_base_units != Some(policy.staking_reward_reserve) {
                return Err(format!(
                    "{ticker} emission reserve must match staking_reward_reserve"
                ));
            }
            if policy.emission.epoch_reserve_drip == Some(0) {
                return Err(format!("{ticker} epoch reserve drip must be nonzero"));
            }
        }
        Ok(())
    }

    fn validate_treasuries(&self) -> Result<(), String> {
        for (ticker, treasury) in [
            ("TLT", &self.treasuries.tlt_security),
            ("OVL", &self.treasuries.ovl_builder),
            ("DRC", &self.treasuries.drc_community),
        ] {
            if treasury.asset != ticker {
                return Err(format!("{ticker} treasury asset mismatch"));
            }
            if treasury.control.trim().is_empty() {
                return Err(format!("{ticker} treasury control is required"));
            }
        }
        Ok(())
    }

    fn validate_pow(&self) -> Result<(), String> {
        if self.pow.algorithm != "randomx" {
            return Err("Trident public networks require randomx".into());
        }
        if self.pow.ghostdag_k == 0
            || self.pow.target_block_time_ms == 0
            || self.pow.daa_window_size == 0
            || self.pow.daa_max_adjustment_bits == 0
            || self.pow.daa_min_level == 0
            || self.pow.daa_max_level < self.pow.daa_min_level
        {
            return Err("PoW policy contains an invalid zero or range".into());
        }
        Ok(())
    }

    fn validate_populated_validator_set(
        &self,
        label: &str,
        validators: &TridentValidatorGenesis,
    ) -> Result<(), String> {
        if validators.genesis_set.is_empty() {
            return Ok(());
        }
        self.validate_validator_policy(label, validators)?;
        let mut keys = BTreeSet::new();
        for validator in &validators.genesis_set {
            let bytes = hex::decode(&validator.consensus_public_key)
                .map_err(|error| format!("{label} validator key is not hex: {error}"))?;
            let key = parse_compressed_public_key(&bytes)
                .map_err(|error| format!("{label} validator key is invalid: {error}"))?;
            if !keys.insert(key) {
                return Err(format!("{label} validator key is duplicated"));
            }
            if validator.self_bond == 0 {
                return Err(format!(
                    "{label} validator withdrawal address/self bond is invalid"
                ));
            }
            self.validate_network_address(&validator.withdrawal_address)?;
            if let Some(commission_bps) = validator.commission_bps {
                let set_max = validators
                    .max_commission_bps
                    .expect("populated validator policy was validated");
                if commission_bps > set_max || commission_bps > MAX_VALIDATOR_COMMISSION_BPS {
                    return Err(format!(
                        "{label} validator commission exceeds the selected maximum"
                    ));
                }
            }
            if let Some(metadata_hash) = validator.metadata_hash.as_deref() {
                if !is_placeholder(metadata_hash) && !is_zero_hash(metadata_hash) {
                    parse_nonzero_hash(&format!("{label} validator metadata_hash"), metadata_hash)?;
                }
            }
        }
        Ok(())
    }

    fn validate_populated_allocations(&self) -> Result<(), String> {
        for allocation in &self.initial_allocations {
            validate_asset_ticker(&allocation.asset)?;
            if allocation.amount == 0 {
                return Err("initial allocation has an empty address or zero amount".into());
            }
            self.validate_network_address(&allocation.address)?;
        }
        for schedule in &self.vesting_schedules {
            validate_asset_ticker(&schedule.asset)?;
            if schedule.amount == 0
                || schedule.start_timestamp_ms > schedule.cliff_timestamp_ms
                || schedule.cliff_timestamp_ms > schedule.end_timestamp_ms
            {
                return Err("vesting schedule is invalid".into());
            }
            self.validate_network_address(&schedule.address)?;
        }
        Ok(())
    }

    fn require_freeze_ready_assets(&self) -> Result<(), String> {
        for (ticker, policy) in [
            ("TLT", &self.assets.tlt),
            ("OVL", &self.assets.ovl),
            ("DRC", &self.assets.drc),
        ] {
            let committed = policy.genesis_allocation
                + policy.staking_reward_reserve
                + policy.treasury_allocation;
            if committed == 0 {
                return Err(format!("{ticker} has no committed allocation"));
            }
        }
        if self.assets.ovl.staking_reward_reserve == 0
            || self.assets.drc.staking_reward_reserve == 0
        {
            return Err("OVL and DRC staking reward reserves must be nonzero".into());
        }
        if self.assets.tlt.emission.initial_reward.unwrap_or(0) == 0
            || self.assets.tlt.emission.halving_interval.unwrap_or(0) == 0
        {
            return Err("TLT emission policy is not frozen".into());
        }
        for (ticker, policy) in [("OVL", &self.assets.ovl), ("DRC", &self.assets.drc)] {
            if policy.emission.epoch_reserve_drip.unwrap_or(0) == 0 {
                return Err(format!("{ticker} epoch reserve drip is not frozen"));
            }
        }
        Ok(())
    }

    fn require_freeze_ready_treasuries(&self) -> Result<(), String> {
        for (ticker, treasury) in [
            ("TLT", &self.treasuries.tlt_security),
            ("OVL", &self.treasuries.ovl_builder),
            ("DRC", &self.treasuries.drc_community),
        ] {
            if treasury.allocation == 0
                || is_placeholder(&treasury.control)
                || treasury.control == "multisig_or_governance"
            {
                return Err(format!(
                    "{ticker} treasury allocation/control is not frozen"
                ));
            }
        }
        Ok(())
    }

    fn require_freeze_ready_validator_set(
        &self,
        label: &str,
        validators: &TridentValidatorGenesis,
    ) -> Result<(), String> {
        if validators.max_validators == 0
            || validators.min_self_bond == 0
            || validators.unbonding_period_checkpoints == 0
            || validators.genesis_set.is_empty()
        {
            return Err(format!("{label} validator policy/set is empty"));
        }
        self.validate_validator_policy(label, validators)?;
        if validators.genesis_set.len() > validators.max_validators as usize {
            return Err(format!("{label} genesis set exceeds max_validators"));
        }
        if validators
            .genesis_set
            .iter()
            .any(|validator| validator.self_bond < validators.min_self_bond)
        {
            return Err(format!("{label} validator self bond is below minimum"));
        }
        for validator in &validators.genesis_set {
            let commission_bps = validator
                .commission_bps
                .ok_or_else(|| format!("{label} validator commission_bps is not selected"))?;
            let set_max = validators
                .max_commission_bps
                .expect("freeze-ready validator policy was validated");
            if commission_bps > set_max || commission_bps > MAX_VALIDATOR_COMMISSION_BPS {
                return Err(format!(
                    "{label} validator commission exceeds the selected maximum"
                ));
            }
            let metadata_hash = validator
                .metadata_hash
                .as_deref()
                .ok_or_else(|| format!("{label} validator metadata_hash is not selected"))?;
            parse_nonzero_hash(&format!("{label} validator metadata_hash"), metadata_hash)?;
        }
        Ok(())
    }

    fn validate_validator_policy(
        &self,
        label: &str,
        validators: &TridentValidatorGenesis,
    ) -> Result<(), String> {
        let commission = validators
            .max_commission_bps
            .ok_or_else(|| format!("{label} max_commission_bps is not selected"))?;
        let concentration = validators
            .max_concentration_bps
            .ok_or_else(|| format!("{label} max_concentration_bps is not selected"))?;
        if commission > MAX_VALIDATOR_COMMISSION_BPS
            || concentration == 0
            || concentration > MAX_VALIDATOR_COMMISSION_BPS
        {
            return Err(format!("{label} validator basis-point policy is invalid"));
        }
        Ok(())
    }

    fn asset_runtime_policy(
        &self,
        asset: NativeAssetId,
        policy: &TridentAssetPolicy,
    ) -> Result<AssetMonetaryPolicy, String> {
        let emission = if asset == NativeAssetId::TLT {
            EmissionKind::PowHalving {
                initial_reward: policy
                    .emission
                    .initial_reward
                    .ok_or_else(|| "TLT initial_reward is not selected".to_string())?,
                halving_interval: policy
                    .emission
                    .halving_interval
                    .ok_or_else(|| "TLT halving_interval is not selected".to_string())?,
            }
        } else {
            EmissionKind::StakingReserve {
                reserve_base_units: policy
                    .emission
                    .reserve_base_units
                    .ok_or_else(|| format!("{asset} staking reserve is not selected"))?,
            }
        };
        Ok(AssetMonetaryPolicy {
            asset,
            max_supply: policy.max_supply,
            decimals: policy.decimals,
            mineable: policy.mineable,
            genesis_allocation: policy.genesis_allocation,
            treasury_allocation: policy.treasury_allocation,
            emission,
        })
    }

    fn staking_runtime_policy(
        &self,
        asset: NativeAssetId,
        validators: &TridentValidatorGenesis,
        policy: &TridentAssetPolicy,
    ) -> Result<StakingParams, String> {
        Ok(StakingParams {
            asset,
            max_validators: validators.max_validators,
            min_self_bond: validators.min_self_bond,
            unbonding_period_epochs: validators.unbonding_period_checkpoints,
            max_commission_bps: validators
                .max_commission_bps
                .ok_or_else(|| format!("{asset} max_commission_bps is not selected"))?,
            max_concentration_bps: validators
                .max_concentration_bps
                .ok_or_else(|| format!("{asset} max_concentration_bps is not selected"))?,
            epoch_reserve_drip: policy
                .emission
                .epoch_reserve_drip
                .ok_or_else(|| format!("{asset} epoch reserve drip is not selected"))?,
        })
    }

    fn require_allocation_totals(&self) -> Result<(), String> {
        if self.initial_allocations.is_empty() {
            return Err("initial_allocations is empty".into());
        }
        for (ticker, expected) in [
            ("TLT", self.assets.tlt.genesis_allocation),
            ("OVL", self.assets.ovl.genesis_allocation),
            ("DRC", self.assets.drc.genesis_allocation),
        ] {
            let actual = self
                .initial_allocations
                .iter()
                .filter(|allocation| allocation.asset == ticker)
                .try_fold(0u64, |sum, allocation| sum.checked_add(allocation.amount))
                .ok_or_else(|| format!("{ticker} initial allocation overflow"))?;
            if actual != expected {
                return Err(format!(
                    "{ticker} initial allocations total {actual}, expected {expected}"
                ));
            }
        }
        for (ticker, document, policy) in [
            (
                "TLT",
                self.treasuries.tlt_security.allocation,
                self.assets.tlt.treasury_allocation,
            ),
            (
                "OVL",
                self.treasuries.ovl_builder.allocation,
                self.assets.ovl.treasury_allocation,
            ),
            (
                "DRC",
                self.treasuries.drc_community.allocation,
                self.assets.drc.treasury_allocation,
            ),
        ] {
            if document != policy {
                return Err(format!(
                    "{ticker} treasury allocation {document} does not match asset policy {policy}"
                ));
            }
        }
        Ok(())
    }

    fn validate_network_address(&self, address: &str) -> Result<(), String> {
        let prefix = format!("{}1", self.wallet.address_hrp);
        if !address.starts_with(&prefix) || Address::parse(address).is_none() {
            return Err(format!(
                "address must be a valid {} Bech32m address",
                self.wallet.address_hrp
            ));
        }
        Ok(())
    }
}

fn validate_asset_ticker(ticker: &str) -> Result<(), String> {
    if matches!(ticker, "TLT" | "OVL" | "DRC") {
        Ok(())
    } else {
        Err(format!("unknown native asset {ticker}"))
    }
}

fn validate_frozen_hash(label: &str, value: &str) -> Result<(), String> {
    if is_placeholder(value) {
        return Err(format!("{label} is still a placeholder"));
    }
    if value.len() != 64
        || value.starts_with("0x")
        || value
            .chars()
            .any(|character| !character.is_ascii_hexdigit() || character.is_ascii_uppercase())
    {
        return Err(format!(
            "{label} must be 64 lowercase hexadecimal characters"
        ));
    }
    Ok(())
}

fn parse_nonzero_hash(label: &str, value: &str) -> Result<Hash, String> {
    validate_frozen_hash(label, value)?;
    let hash = Hash::from_hex(value).ok_or_else(|| format!("{label} is not a 32-byte hash"))?;
    if hash == Hash::ZERO {
        return Err(format!("{label} must be nonzero"));
    }
    Ok(hash)
}

fn is_zero_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte == b'0')
}

fn is_placeholder(value: &str) -> bool {
    value.trim().is_empty()
        || value == UNFROZEN
        || value.to_ascii_lowercase().contains("placeholder")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        artifact.assets.tlt.treasury_allocation = 1;
        artifact.assets.ovl.genesis_allocation = 1;
        artifact.assets.ovl.staking_reward_reserve = 1;
        artifact.assets.ovl.emission.reserve_base_units = Some(1);
        artifact.assets.ovl.treasury_allocation = 1;
        artifact.assets.drc.genesis_allocation = 1;
        artifact.assets.drc.staking_reward_reserve = 1;
        artifact.assets.drc.emission.reserve_base_units = Some(1);
        artifact.assets.drc.treasury_allocation = 1;
        artifact.treasuries.tlt_security.allocation = 1;
        artifact.treasuries.ovl_builder.allocation = 1;
        artifact.treasuries.drc_community.allocation = 1;
        artifact.treasuries.tlt_security.control = "governance-v1".into();
        artifact.treasuries.ovl_builder.control = "governance-v1".into();
        artifact.treasuries.drc_community.control = "governance-v1".into();
        let address = Address([9; 20]).to_bech32_hrp("agoratest");
        for (ticker, amount) in [
            ("TLT", artifact.assets.tlt.genesis_allocation),
            ("OVL", 1),
            ("DRC", 1),
        ] {
            artifact.initial_allocations.push(TridentInitialAllocation {
                asset: ticker.into(),
                address: address.clone(),
                amount,
            });
        }
        let key = agora_crypto::KeyPair::from_secret_bytes(&[7; 32])
            .unwrap()
            .public_key_bytes();
        let validator = TridentGenesisValidator {
            consensus_public_key: hex::encode(key),
            withdrawal_address: address,
            self_bond: 1,
            commission_bps: Some(100),
            metadata_hash: Some("33".repeat(32)),
        };
        for set in [&mut artifact.ovl_validators, &mut artifact.drc_validators] {
            set.max_validators = 1;
            set.min_self_bond = 1;
            set.unbonding_period_checkpoints = 1;
            set.max_commission_bps = Some(2_000);
            set.max_concentration_bps = Some(10_000);
            set.genesis_set.push(validator.clone());
        }
        artifact.genesis_hash = artifact.consensus_identity_hash().to_hex();
        artifact.network_fingerprint = artifact.compute_network_fingerprint().to_hex();
        artifact
    }

    fn enable_synthetic_data_availability(artifact: &mut TridentGenesisArtifact) {
        artifact.data_availability = TridentDataAvailabilityPolicy {
            version: crate::DATA_AVAILABILITY_POLICY_VERSION,
            enabled: true,
            activation_checkpoint: Some(100),
            activation_block_body_version: agora_types::TRIDENT_BLOCK_BODY_VERSION,
            max_commitments_per_block: 8,
            max_authorization_bytes_per_block: 16_384,
            base_fee_tlt: 10,
            fee_per_authorization_byte_tlt: 2,
            fee_per_state_byte_tlt: 3,
            allowed_sources: vec![agora_types::DataCommitmentSource::AgoraLayersOvolosBatchLab],
            max_sequence_advance: 64,
        };
        artifact.genesis_hash = artifact.consensus_identity_hash().to_hex();
        artifact.network_fingerprint = artifact.compute_network_fingerprint().to_hex();
    }

    #[test]
    fn checked_in_draft_parses_strictly_and_has_stable_identity() {
        let artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        artifact.validate_draft().unwrap();
        assert_eq!(
            artifact.data_availability,
            TridentDataAvailabilityPolicy::disabled()
        );
        assert_eq!(
            artifact.consensus_identity_hash(),
            artifact.consensus_identity_hash()
        );
        assert_ne!(artifact.compute_network_fingerprint(), Hash::ZERO);
    }

    #[test]
    fn enabled_data_availability_requires_complete_safe_policy() {
        let disabled = synthetic_freeze_ready_artifact();
        disabled.validate_freeze_ready().unwrap();
        assert!(
            !disabled
                .to_runtime_policy()
                .unwrap()
                .data_availability
                .enabled
        );

        let mut placeholder = disabled.clone();
        placeholder.data_availability.enabled = true;
        assert!(placeholder.data_availability.validate_draft().is_ok());
        assert!(placeholder
            .data_availability
            .validate_freeze_ready()
            .unwrap_err()
            .contains("activation checkpoint"));

        let mut enabled = disabled;
        enable_synthetic_data_availability(&mut enabled);
        enabled.validate_freeze_ready().unwrap();
        let runtime = enabled.to_runtime_policy().unwrap();
        assert_eq!(runtime.data_availability, enabled.data_availability);
        assert!(
            runtime
                .data_availability_runtime_config()
                .unwrap()
                .policy
                .enabled
        );

        let mut unsafe_limit = enabled.clone();
        unsafe_limit.data_availability.max_commitments_per_block =
            u32::try_from(agora_consensus::MAX_DATA_COMMITMENTS_PER_BLOCK)
                .unwrap()
                .checked_add(1)
                .unwrap();
        assert!(unsafe_limit
            .data_availability
            .validate_freeze_ready()
            .unwrap_err()
            .contains("hard cap"));
    }

    #[test]
    fn every_data_availability_policy_field_changes_all_artifact_identities() {
        let mut artifact = synthetic_freeze_ready_artifact();
        enable_synthetic_data_availability(&mut artifact);
        let identity = artifact.consensus_identity_hash();
        let policy = artifact.consensus_policy_hash();
        let fingerprint = artifact.compute_network_fingerprint();

        let mut mutations = Vec::new();
        let mut changed = artifact.clone();
        changed.data_availability.version += 1;
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.enabled = false;
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.activation_checkpoint = Some(101);
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.activation_block_body_version += 1;
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.max_commitments_per_block += 1;
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.max_authorization_bytes_per_block += 1;
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.base_fee_tlt += 1;
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.fee_per_authorization_byte_tlt += 1;
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.fee_per_state_byte_tlt += 1;
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.allowed_sources.clear();
        mutations.push(changed);
        let mut changed = artifact.clone();
        changed.data_availability.max_sequence_advance += 1;
        mutations.push(changed);

        for changed in mutations {
            assert_ne!(changed.consensus_identity_hash(), identity);
            assert_ne!(changed.consensus_policy_hash(), policy);
            assert_ne!(changed.compute_network_fingerprint(), fingerprint);
        }
    }

    #[test]
    fn checked_in_unfrozen_draft_is_not_freeze_ready() {
        let artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        let error = artifact.validate_freeze_ready().unwrap_err();
        assert!(
            error.contains("timestamp_ms")
                || error.contains("UNFROZEN")
                || error.contains("placeholder")
        );
    }

    #[test]
    fn strict_parser_rejects_unknown_consensus_fields() {
        let json = DRAFT.replace(
            "\"ghostdag_k\": 18",
            "\"ghostdag_k\": 18, \"silent_override\": 1",
        );
        assert!(TridentGenesisArtifact::from_json(&json)
            .unwrap_err()
            .contains("unknown field"));
    }

    #[test]
    fn identity_and_fingerprint_change_with_allocation() {
        let mut artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        let identity = artifact.consensus_identity_hash();
        let fingerprint = artifact.compute_network_fingerprint();
        artifact.assets.ovl.genesis_allocation = 1;
        assert_ne!(artifact.consensus_identity_hash(), identity);
        assert_ne!(artifact.compute_network_fingerprint(), fingerprint);
    }

    #[test]
    fn staking_emission_reserve_must_match_asset_policy() {
        let mut artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        artifact.assets.ovl.emission.reserve_base_units = Some(1);
        assert!(artifact
            .validate_draft()
            .unwrap_err()
            .contains("emission reserve must match"));
    }

    #[test]
    fn populated_validator_key_must_be_compressed_secp256k1() {
        let mut artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        artifact.ovl_validators.max_commission_bps = Some(2_000);
        artifact.ovl_validators.max_concentration_bps = Some(10_000);
        artifact
            .ovl_validators
            .genesis_set
            .push(TridentGenesisValidator {
                consensus_public_key: "00".repeat(33),
                withdrawal_address: "agoratest1notvalidateduntil-loader-integration".into(),
                self_bond: 1,
                commission_bps: Some(100),
                metadata_hash: Some("11".repeat(32)),
            });
        assert!(artifact
            .validate_draft()
            .unwrap_err()
            .contains("validator key is invalid"));
    }

    #[test]
    fn validator_metadata_round_trips_json_and_borsh() {
        let validator = synthetic_freeze_ready_artifact()
            .ovl_validators
            .genesis_set
            .remove(0);
        let json = serde_json::to_string(&validator).unwrap();
        assert_eq!(
            serde_json::from_str::<TridentGenesisValidator>(&json).unwrap(),
            validator
        );
        let bytes = borsh::to_vec(&validator).unwrap();
        assert_eq!(
            TridentGenesisValidator::try_from_slice(&bytes).unwrap(),
            validator
        );
    }

    #[test]
    fn validator_commission_and_metadata_are_identity_sensitive() {
        let artifact = synthetic_freeze_ready_artifact();
        let identity = artifact.consensus_identity_hash();
        let policy = artifact.consensus_policy_hash();
        let fingerprint = artifact.compute_network_fingerprint();

        let mut changed = artifact.clone();
        changed.ovl_validators.genesis_set[0].commission_bps = Some(101);
        assert_ne!(changed.consensus_identity_hash(), identity);
        assert_ne!(changed.consensus_policy_hash(), policy);
        assert_ne!(changed.compute_network_fingerprint(), fingerprint);

        let mut changed = artifact;
        changed.ovl_validators.genesis_set[0].metadata_hash = Some("44".repeat(32));
        assert_ne!(changed.consensus_identity_hash(), identity);
        assert_ne!(changed.consensus_policy_hash(), policy);
        assert_ne!(changed.compute_network_fingerprint(), fingerprint);
    }

    #[test]
    fn validator_commission_obeys_set_and_global_limits() {
        let mut artifact = synthetic_freeze_ready_artifact();
        artifact.ovl_validators.genesis_set[0].commission_bps = Some(2_001);
        assert!(artifact
            .validate_draft()
            .unwrap_err()
            .contains("commission exceeds"));

        let mut artifact = synthetic_freeze_ready_artifact();
        artifact.ovl_validators.max_commission_bps =
            Some(MAX_VALIDATOR_COMMISSION_BPS.saturating_add(1));
        assert!(artifact
            .validate_draft()
            .unwrap_err()
            .contains("basis-point policy"));

        let json =
            serde_json::to_string(&synthetic_freeze_ready_artifact().ovl_validators.genesis_set[0])
                .unwrap()
                .replace("\"commission_bps\":100", "\"commission_bps\":65536");
        assert!(serde_json::from_str::<TridentGenesisValidator>(&json).is_err());
    }

    #[test]
    fn freeze_rejects_missing_placeholder_and_zero_metadata_fields() {
        let mut missing_commission = synthetic_freeze_ready_artifact();
        missing_commission.ovl_validators.genesis_set[0].commission_bps = None;
        missing_commission.validate_draft().unwrap();
        assert!(missing_commission
            .validate_freeze_ready()
            .unwrap_err()
            .contains("commission_bps is not selected"));

        let mut missing_metadata = synthetic_freeze_ready_artifact();
        missing_metadata.ovl_validators.genesis_set[0].metadata_hash = None;
        missing_metadata.validate_draft().unwrap();
        assert!(missing_metadata
            .validate_freeze_ready()
            .unwrap_err()
            .contains("metadata_hash is not selected"));

        for placeholder in ["UNFROZEN".to_string(), "00".repeat(32)] {
            let mut artifact = synthetic_freeze_ready_artifact();
            artifact.ovl_validators.genesis_set[0].metadata_hash = Some(placeholder);
            artifact.validate_draft().unwrap();
            assert!(artifact
                .validate_freeze_ready()
                .unwrap_err()
                .contains("metadata_hash"));
        }
    }

    #[test]
    fn legacy_validator_entries_are_draft_only_and_cannot_freeze() {
        let artifact = synthetic_freeze_ready_artifact();
        let mut legacy_json = serde_json::to_value(&artifact).unwrap();
        for set_name in ["ovl_validators", "drc_validators"] {
            let entry = legacy_json[set_name]["genesis_set"][0]
                .as_object_mut()
                .unwrap();
            entry.remove("commission_bps");
            entry.remove("metadata_hash");
        }
        let legacy: TridentGenesisArtifact = serde_json::from_value(legacy_json).unwrap();
        legacy.validate_draft().unwrap();
        assert!(legacy.validate_freeze_ready().is_err());
        assert!(legacy
            .ovl_validators
            .genesis_set
            .iter()
            .all(
                |validator| validator.commission_bps.is_none() && validator.metadata_hash.is_none()
            ));
    }

    #[test]
    fn hash_fields_must_match_computed_values_at_freeze() {
        let mut artifact = synthetic_freeze_ready_artifact();
        artifact.network_fingerprint = "00".repeat(32);
        assert!(artifact
            .validate_freeze_ready()
            .unwrap_err()
            .contains("network_fingerprint mismatch"));

        artifact.network_fingerprint = artifact.compute_network_fingerprint().to_hex();
        artifact.validate_freeze_ready().unwrap();

        let runtime = artifact.to_runtime_policy().unwrap();
        assert_eq!(
            runtime.artifact_identity,
            artifact.consensus_identity_hash()
        );
        assert_eq!(
            runtime.consensus_policy_hash,
            artifact.consensus_policy_hash()
        );
        assert_eq!(
            runtime.network_fingerprint,
            artifact.compute_network_fingerprint()
        );
        assert_eq!(runtime.chain_id, artifact.chain_id);
        assert_eq!(runtime.bits, 0);
        assert_eq!(runtime.daa.min_level, artifact.pow.daa_min_level);
        assert_eq!(runtime.ghostdag.k, artifact.pow.ghostdag_k);
        assert_eq!(runtime.finality.min_pow_depth, 12);
        assert_eq!(runtime.ovl_staking.min_self_bond, 1);
        assert_eq!(runtime.ovl_staking.max_commission_bps, 2_000);
        assert_eq!(runtime.drc_staking.epoch_reserve_drip, 1_000_000_000);
        assert_eq!(runtime.data_availability, artifact.data_availability);
        let da_runtime = runtime.data_availability_runtime_config().unwrap();
        assert_eq!(da_runtime.network_fingerprint, runtime.network_fingerprint);
        assert!(!da_runtime.policy.enabled);

        artifact.pow.ghostdag_k += 1;
        let error = artifact.to_runtime_policy().unwrap_err();
        assert!(error.contains("genesis_hash mismatch"));
    }
}
