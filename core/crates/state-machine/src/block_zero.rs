//! Canonical, artifact-only Trident Block 0 state commitment.
//!
//! This module intentionally does not write a datadir or construct a [`agora_types::Block`].
//! The current header has no state-root field, and several committed records do not yet
//! have lossless runtime-store representations. Keeping preparation pure prevents a
//! partially seeded Trident node from reaching networking.

use std::collections::{BTreeMap, BTreeSet};

use agora_crypto::{address_from_pubkey, parse_compressed_public_key};
use agora_types::{Address, CheckpointState, Hash, NativeAssetId, TreasuryId};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::trident_genesis::{
    TridentGenesisArtifact, TridentGenesisValidator, TridentInitialAllocation, TridentTreasury,
    TridentValidatorGenesis, TridentVestingSchedule,
};

/// Version of the canonical Block 0 native-state manifest.
pub const TRIDENT_BLOCK_ZERO_STATE_VERSION: u32 = 1;
/// Domain for the complete native-state root.
pub const TRIDENT_BLOCK_ZERO_STATE_DOMAIN: &[u8] = b"agora-trident-block-zero-state-v1";
/// Domain for the value a future Block 0 header must commit.
pub const TRIDENT_BLOCK_ZERO_COMMITMENT_DOMAIN: &[u8] = b"agora-trident-block-zero-commitment-v1";

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
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
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
    pub artifact_identity: Hash,
    pub consensus_policy_hash: Hash,
    pub state_root: Hash,
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
            artifact_identity: artifact.consensus_identity_hash(),
            consensus_policy_hash: artifact.consensus_policy_hash(),
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
        if self.artifact_identity == Hash::ZERO || self.consensus_policy_hash == Hash::ZERO {
            return Err("Block 0 identities must be nonzero".into());
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
}

impl TridentBlockZeroCommitment {
    pub fn hash(&self) -> Hash {
        Hash::hash_borsh(&(TRIDENT_BLOCK_ZERO_COMMITMENT_DOMAIN, self))
    }
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
    {
        return Err(format!("{} Block 0 validator set is invalid", set.asset));
    }
    let mut operators = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut total = 0u64;
    for validator in &set.validators {
        if !operators.insert(validator.operator)
            || !keys.insert(validator.consensus_public_key)
            || validator.self_bond < set.min_self_bond
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
        for (set, key, address, bond) in [
            (&mut artifact.ovl_validators, &ovl, ovl_address, 5),
            (&mut artifact.drc_validators, &drc, drc_address, 7),
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
        assert_eq!(state.finality.state, CheckpointState::Proposed);
        assert!(!state.verified_borsh_payload().unwrap().is_empty());
        assert_eq!(state.commitment().hash(), state.commitment().hash());
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
    fn tampering_fails_closed_before_any_storage_api_exists() {
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
}
