//! Offline validation for the human-owned Agora Trident genesis artifact.
//!
//! This module deliberately has no conversion into [`crate::ChainParams`]. A v3
//! document can be inspected and prepared for a freeze ceremony without making
//! it possible for the current v2 node loader to boot it.

use std::collections::BTreeSet;

use agora_crypto::parse_compressed_public_key;
use agora_types::{Address, Hash};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::network::NetworkId;

pub const TRIDENT_GENESIS_SCHEMA: &str = "agora-trident-genesis-v3";
pub const TRIDENT_STATE_TRANSITION_VERSION: &str = "agora-trident-state-v1";
pub const TRIDENT_CONSENSUS_POLICY_VERSION: &str = "agora-trident-consensus-v1";
pub const TRIDENT_NET_FP_DOMAIN: &[u8] = b"agora-trident-net-fp-v1";
pub const TRIDENT_GENESIS_ID_DOMAIN: &[u8] = b"agora-trident-genesis-identity-v1";
pub const TRIDENT_PROTOCOL_VERSION: u32 = 1;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct TridentFinalityPolicy {
    pub model: String,
    pub pow_work_threshold_policy: String,
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
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Default,
)]
#[serde(deny_unknown_fields)]
pub struct TridentValidatorGenesis {
    pub max_validators: u32,
    pub min_self_bond: u64,
    pub unbonding_period_checkpoints: u64,
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
    ovl_validators: &'a TridentValidatorGenesis,
    drc_validators: &'a TridentValidatorGenesis,
    vesting_schedules: &'a [TridentVestingSchedule],
    initial_allocations: &'a [TridentInitialAllocation],
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
            ovl_validators: &self.ovl_validators,
            drc_validators: &self.drc_validators,
            vesting_schedules: &self.vesting_schedules,
            initial_allocations: &self.initial_allocations,
        })
    }

    pub fn compute_network_fingerprint(&self) -> Hash {
        let identity = self.consensus_identity_hash();
        Hash::hash_borsh(&(
            TRIDENT_NET_FP_DOMAIN,
            TRIDENT_PROTOCOL_VERSION,
            self.chain_id.as_str(),
            identity.as_bytes(),
            TRIDENT_TX_SIGNING_VERSION,
            TRIDENT_STATE_TRANSITION_VERSION,
            TRIDENT_CONSENSUS_POLICY_VERSION,
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
        Ok(())
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

fn is_placeholder(value: &str) -> bool {
    value.trim().is_empty()
        || value == UNFROZEN
        || value.to_ascii_lowercase().contains("placeholder")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAFT: &str = include_str!("../../../../docs/genesis/trident.testnet.genesis.draft.json");

    #[test]
    fn checked_in_draft_parses_strictly_and_has_stable_identity() {
        let artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        artifact.validate_draft().unwrap();
        assert_eq!(
            artifact.consensus_identity_hash(),
            artifact.consensus_identity_hash()
        );
        assert_ne!(artifact.compute_network_fingerprint(), Hash::ZERO);
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
    fn populated_validator_key_must_be_compressed_secp256k1() {
        let mut artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        artifact
            .ovl_validators
            .genesis_set
            .push(TridentGenesisValidator {
                consensus_public_key: "00".repeat(33),
                withdrawal_address: "agoratest1notvalidateduntil-loader-integration".into(),
                self_bond: 1,
            });
        assert!(artifact
            .validate_draft()
            .unwrap_err()
            .contains("validator key is invalid"));
    }

    #[test]
    fn hash_fields_must_match_computed_values_at_freeze() {
        let mut artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        artifact.timestamp_ms = 1;
        artifact.bits = Some(0);
        artifact.maturity = "Scaffold".into();
        artifact.notes.clear();
        artifact.wallet.coin_type_status = "registered".into();
        artifact.governance_constitution_hash = "11".repeat(32);
        artifact.emergency_policy_hash = "22".repeat(32);
        artifact.finality.pow_work_threshold_policy = "fixed-cumulative-work-v1".into();
        artifact.assets.tlt.treasury_allocation = 1;
        artifact.assets.ovl.genesis_allocation = 1;
        artifact.assets.ovl.staking_reward_reserve = 1;
        artifact.assets.ovl.treasury_allocation = 1;
        artifact.assets.drc.genesis_allocation = 1;
        artifact.assets.drc.staking_reward_reserve = 1;
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
        };
        for set in [&mut artifact.ovl_validators, &mut artifact.drc_validators] {
            set.max_validators = 1;
            set.min_self_bond = 1;
            set.unbonding_period_checkpoints = 1;
            set.genesis_set.push(validator.clone());
        }
        artifact.genesis_hash = artifact.consensus_identity_hash().to_hex();
        artifact.network_fingerprint = "00".repeat(32);
        assert!(artifact
            .validate_freeze_ready()
            .unwrap_err()
            .contains("network_fingerprint mismatch"));

        artifact.network_fingerprint = artifact.compute_network_fingerprint().to_hex();
        artifact.validate_freeze_ready().unwrap();
    }
}
