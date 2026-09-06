//! Independent OVL / DRC staking modules for Trident finality validators.
//!
//! Bonding debits liquid account balances into staking locks. OVL and DRC sets
//! never share stake or combine via prices.

use agora_consensus::{SlashPolicy, ValidatorEvidence};
use agora_crypto::{address_from_pubkey, parse_compressed_public_key, verify_stake_tx_bound};
use agora_types::{
    Address, CheckpointAttestation, Hash, NativeAssetId, SignedStakeTx, StakeOpKind,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::accounts::{load_account, put_account_into};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::supply::{load_issued_supply, load_max_supply, put_issued_supply_into};
use crate::{StateError, StateStore};

/// Consensus-wide upper bound for validator commission basis points.
pub const MAX_VALIDATOR_COMMISSION_BPS: u16 = 10_000;

/// Staking parameters for one validator set (OVL or DRC).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakingParams {
    pub asset: NativeAssetId,
    pub max_validators: u32,
    pub min_self_bond: u64,
    pub unbonding_period_epochs: u64,
    /// Max commission in basis points (e.g. 10000 = 100%).
    pub max_commission_bps: u16,
    /// Max share of active stake one validator may hold (bps of total).
    pub max_concentration_bps: u16,
    /// Per-epoch drip from staking reserve into the reward pool (working testnet default).
    pub epoch_reserve_drip: u64,
}

impl StakingParams {
    pub fn ovl_default() -> Self {
        Self {
            asset: NativeAssetId::OVL,
            max_validators: 100,
            min_self_bond: 1_000_000,
            unbonding_period_epochs: 7,
            max_commission_bps: 5_000,
            max_concentration_bps: 3_000, // 30%
            epoch_reserve_drip: crate::monetary::WORKING_EPOCH_RESERVE_DRIP,
        }
    }

    pub fn drc_default() -> Self {
        Self {
            asset: NativeAssetId::DRC,
            max_validators: 100,
            min_self_bond: 1_000_000,
            unbonding_period_epochs: 7,
            max_commission_bps: 5_000,
            max_concentration_bps: 3_000,
            epoch_reserve_drip: crate::monetary::WORKING_EPOCH_RESERVE_DRIP,
        }
    }

    pub fn validate(&self) -> Result<(), StateError> {
        self.validate_asset()?;
        if self.max_commission_bps > MAX_VALIDATOR_COMMISSION_BPS {
            return Err(StateError::InvalidTx(
                "maximum commission exceeds global basis-point limit".into(),
            ));
        }
        if self.max_concentration_bps == 0
            || self.max_concentration_bps > MAX_VALIDATOR_COMMISSION_BPS
        {
            return Err(StateError::InvalidTx(
                "validator concentration basis-point policy is invalid".into(),
            ));
        }
        Ok(())
    }

    fn validate_asset(&self) -> Result<(), StateError> {
        if !matches!(self.asset, NativeAssetId::OVL | NativeAssetId::DRC) {
            return Err(StateError::InvalidTx("staking only for OVL or DRC".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ValidatorStatus {
    Bonded,
    Unbonding,
    Jailed,
    Tombstoned,
}

/// On-disk validator record.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorRecord {
    pub operator: Address,
    pub consensus_pubkey: Vec<u8>,
    pub withdrawal: Address,
    pub self_bond: u64,
    pub delegated: u64,
    pub commission_bps: u16,
    pub status: ValidatorStatus,
    pub jailed_until_epoch: u64,
    pub metadata_hash: Hash,
}

impl ValidatorRecord {
    pub fn voting_power(&self) -> u64 {
        if matches!(self.status, ValidatorStatus::Bonded) {
            self.self_bond.saturating_add(self.delegated)
        } else {
            0
        }
    }

    /// Validate the exact epoch-zero representation before a future genesis
    /// writer can derive its Meta key and Borsh value.
    pub fn validate_genesis_registration(&self, params: &StakingParams) -> Result<(), StateError> {
        params.validate()?;
        let key = parse_compressed_public_key(&self.consensus_pubkey)
            .map_err(|error| StateError::InvalidTx(format!("invalid consensus pubkey: {error}")))?;
        if address_from_pubkey(&key) != self.operator {
            return Err(StateError::InvalidTx(
                "validator operator does not match consensus pubkey".into(),
            ));
        }
        if self.self_bond < params.min_self_bond {
            return Err(StateError::InvalidTx("self-bond below minimum".into()));
        }
        if self.delegated != 0
            || self.status != ValidatorStatus::Bonded
            || self.jailed_until_epoch != 0
        {
            return Err(StateError::InvalidTx(
                "genesis validator must begin bonded without delegated or jailed state".into(),
            ));
        }
        if self.commission_bps > params.max_commission_bps
            || self.commission_bps > MAX_VALIDATOR_COMMISSION_BPS
        {
            return Err(StateError::InvalidTx("commission too high".into()));
        }
        if self.metadata_hash == Hash::ZERO {
            return Err(StateError::InvalidTx(
                "genesis validator metadata hash must be nonzero".into(),
            ));
        }
        Ok(())
    }

    /// Return the unchanged staking key format and canonical record bytes
    /// without writing live state.
    pub fn canonical_genesis_storage_entry(
        &self,
        params: &StakingParams,
    ) -> Result<(Vec<u8>, Vec<u8>), StateError> {
        self.validate_genesis_registration(params)?;
        let value = borsh::to_vec(self).map_err(|error| StateError::Storage(error.to_string()))?;
        let decoded =
            Self::try_from_slice(&value).map_err(|error| StateError::Storage(error.to_string()))?;
        if decoded != *self {
            return Err(StateError::Storage(
                "genesis validator Borsh round-trip changed the record".into(),
            ));
        }
        Ok((validator_meta_key(params.asset, &self.operator), value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Default)]
pub struct DelegationRecord {
    pub delegator: Address,
    pub validator: Address,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct UnbondingEntry {
    pub owner: Address,
    pub amount: u64,
    pub release_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorSetSnapshot {
    pub epoch: u64,
    pub asset: NativeAssetId,
    /// (operator, voting_power) sorted by operator bytes.
    pub validators: Vec<(Address, u64)>,
    pub total_active_stake: u64,
}

impl ValidatorSetSnapshot {
    pub fn commitment(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn power_of(&self, op: &Address) -> u64 {
        self.validators
            .iter()
            .find(|(a, _)| a == op)
            .map(|(_, p)| *p)
            .unwrap_or(0)
    }
}

pub fn validator_meta_key(asset: NativeAssetId, op: &Address) -> Vec<u8> {
    let mut k = b"stake/val/".to_vec();
    k.push(asset.wire_byte());
    k.push(b'/');
    k.extend_from_slice(&op.0);
    k
}

fn delegation_key(asset: NativeAssetId, delegator: &Address, validator: &Address) -> Vec<u8> {
    let mut k = b"stake/del/".to_vec();
    k.push(asset.wire_byte());
    k.push(b'/');
    k.extend_from_slice(&delegator.0);
    k.push(b'/');
    k.extend_from_slice(&validator.0);
    k
}

fn unbonding_prefix(asset: NativeAssetId) -> Vec<u8> {
    let mut k = b"stake/unbond/".to_vec();
    k.push(asset.wire_byte());
    k.push(b'/');
    k
}

fn unbonding_key(asset: NativeAssetId, op: &Address) -> Vec<u8> {
    let mut key = unbonding_prefix(asset);
    key.extend_from_slice(&op.0);
    key
}

/// Meta keys a signed stake op may mutate (for reorg journals).
///
/// Actor liquid balances are journaled separately via [`crate::UtxoJournal::account_before`]
/// so interleaved account-transfer + stake restores stay well-ordered.
pub fn stake_meta_keys_touched(tx: &SignedStakeTx) -> Vec<Vec<u8>> {
    let mut keys = vec![
        validator_meta_key(tx.asset, &tx.validator),
        delegation_key(tx.asset, &tx.actor, &tx.validator),
        unbonding_key(tx.asset, &tx.actor),
        reward_pool_key(tx.asset),
    ];
    if tx.actor != tx.validator {
        keys.push(validator_meta_key(tx.asset, &tx.actor));
        keys.push(unbonding_key(tx.asset, &tx.validator));
    }
    keys
}

/// Meta key for the OVL/DRC staking reward pool (fee-share / slash sink).
pub fn reward_pool_meta_key(asset: NativeAssetId) -> Vec<u8> {
    reward_pool_key(asset)
}

pub type MetaValueSnapshot = Vec<(Vec<u8>, Option<Vec<u8>>)>;

/// Snapshot current Meta values for `keys` (`None` = absent).
pub fn snapshot_meta_keys(
    store: &StateStore,
    keys: &[Vec<u8>],
) -> Result<MetaValueSnapshot, StateError> {
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        out.push((key.clone(), store.get_cf(ColumnFamily::Meta, key)?));
    }
    Ok(out)
}

pub(crate) fn epoch_key(asset: NativeAssetId) -> Vec<u8> {
    let mut k = b"stake/epoch/".to_vec();
    k.push(asset.wire_byte());
    k
}

pub(crate) fn snapshot_key(asset: NativeAssetId, epoch: u64) -> Vec<u8> {
    let mut k = b"stake/snap/".to_vec();
    k.push(asset.wire_byte());
    k.push(b'/');
    k.extend_from_slice(&epoch.to_le_bytes());
    k
}

fn reward_pool_key(asset: NativeAssetId) -> Vec<u8> {
    let mut k = b"stake/reward_pool/".to_vec();
    k.push(asset.wire_byte());
    k
}

pub(crate) fn staking_reserve_key(asset: NativeAssetId) -> Vec<u8> {
    let mut k = b"stake/reserve_remaining/".to_vec();
    k.push(asset.wire_byte());
    k
}

pub fn load_staking_reserve_remaining(
    store: &StateStore,
    asset: NativeAssetId,
) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &staking_reserve_key(asset))? else {
        return Ok(0);
    };
    if bytes.len() != 8 {
        return Ok(0);
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    Ok(u64::from_le_bytes(arr))
}

pub fn put_staking_reserve_remaining_into(
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    amount: u64,
) {
    batch.put_cf(
        ColumnFamily::Meta,
        &staking_reserve_key(asset),
        &amount.to_le_bytes(),
    );
}

/// Initialize remaining staking reserve from genesis monetary policy (OVL/DRC only).
pub fn init_staking_reserve_into(
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    reserve_base_units: u64,
) -> Result<(), StateError> {
    if !matches!(asset, NativeAssetId::OVL | NativeAssetId::DRC) {
        return Err(StateError::InvalidTx(
            "staking reserve only for OVL/DRC".into(),
        ));
    }
    put_staking_reserve_remaining_into(batch, asset, reserve_base_units);
    Ok(())
}

/// Move `amount` from staking reserve → reward pool and bump issued supply.
///
/// Never touches TLT. No-op when amount is 0 or reserve is empty.
pub fn drip_staking_reserve(
    store: &StateStore,
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    amount: u64,
) -> Result<u64, StateError> {
    if amount == 0 {
        return Ok(0);
    }
    if !matches!(asset, NativeAssetId::OVL | NativeAssetId::DRC) {
        return Err(StateError::InvalidTx(
            "cannot drip TLT staking reserve".into(),
        ));
    }
    let remaining = load_staking_reserve_remaining(store, asset)?;
    let drip = amount.min(remaining);
    if drip == 0 {
        return Ok(0);
    }
    let issued = load_issued_supply(store, asset)?;
    let max = load_max_supply(store, asset)?;
    let next_issued = issued
        .checked_add(drip)
        .ok_or_else(|| StateError::InvalidTx("issued overflow".into()))?;
    if next_issued > max {
        return Err(StateError::SupplyCapExceeded);
    }
    put_staking_reserve_remaining_into(batch, asset, remaining - drip);
    put_issued_supply_into(batch, asset, next_issued);
    credit_reward_pool_into(store, batch, asset, drip)?;
    Ok(drip)
}

/// Fee-share sink for future OVL execution / DRC payment modules.
///
/// Call only for Accepted fee attribution in the same asset — never divert TLT miner fees.
pub fn credit_fee_share_to_reward_pool(
    store: &StateStore,
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    amount: u64,
) -> Result<u64, StateError> {
    credit_reward_pool_into(store, batch, asset, amount)
}

/// Slash / fee proceeds held for epoch distribution (never TLT).
pub fn load_reward_pool(store: &StateStore, asset: NativeAssetId) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &reward_pool_key(asset))? else {
        return Ok(0);
    };
    if bytes.len() != 8 {
        return Ok(0);
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    Ok(u64::from_le_bytes(arr))
}

pub fn put_reward_pool_into(batch: &mut WriteBatch, asset: NativeAssetId, amount: u64) {
    batch.put_cf(
        ColumnFamily::Meta,
        &reward_pool_key(asset),
        &amount.to_le_bytes(),
    );
}

pub fn credit_reward_pool_into(
    store: &StateStore,
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    amount: u64,
) -> Result<u64, StateError> {
    if !matches!(asset, NativeAssetId::OVL | NativeAssetId::DRC) {
        return Err(StateError::InvalidTx("reward pool only for OVL/DRC".into()));
    }
    let next = load_reward_pool(store, asset)?
        .checked_add(amount)
        .ok_or_else(|| StateError::InvalidTx("reward pool overflow".into()))?;
    put_reward_pool_into(batch, asset, next);
    Ok(next)
}

pub fn load_epoch(store: &StateStore, asset: NativeAssetId) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &epoch_key(asset))? else {
        return Ok(0);
    };
    if bytes.len() != 8 {
        return Ok(0);
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    Ok(u64::from_le_bytes(arr))
}

pub fn put_epoch_into(batch: &mut WriteBatch, asset: NativeAssetId, epoch: u64) {
    batch.put_cf(ColumnFamily::Meta, &epoch_key(asset), &epoch.to_le_bytes());
}

pub fn load_validator(
    store: &StateStore,
    asset: NativeAssetId,
    op: &Address,
) -> Result<Option<ValidatorRecord>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &validator_meta_key(asset, op))? else {
        return Ok(None);
    };
    Ok(Some(
        ValidatorRecord::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?,
    ))
}

pub fn put_validator_into(
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    record: &ValidatorRecord,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(record).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &validator_meta_key(asset, &record.operator),
        &bytes,
    );
    Ok(())
}

fn debit_liquid(
    store: &StateStore,
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    owner: &Address,
    amount: u64,
) -> Result<(), StateError> {
    let mut acct = load_account(store, asset, owner)?;
    if acct.balance < amount {
        return Err(StateError::InvalidTx(
            "insufficient liquid stake funds".into(),
        ));
    }
    acct.balance -= amount;
    put_account_into(batch, asset, owner, &acct)
}

fn credit_liquid(
    store: &StateStore,
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    owner: &Address,
    amount: u64,
) -> Result<(), StateError> {
    let mut acct = load_account(store, asset, owner)?;
    acct.balance = acct
        .balance
        .checked_add(amount)
        .ok_or_else(|| StateError::InvalidTx("balance overflow".into()))?;
    put_account_into(batch, asset, owner, &acct)
}

/// Register / self-bond a validator. Debits liquid account balance.
#[allow(clippy::too_many_arguments)]
pub fn bond_validator(
    store: &StateStore,
    batch: &mut WriteBatch,
    params: &StakingParams,
    operator: Address,
    consensus_pubkey: Vec<u8>,
    withdrawal: Address,
    self_bond: u64,
    commission_bps: u16,
    metadata_hash: Hash,
) -> Result<(), StateError> {
    params.validate()?;
    if self_bond < params.min_self_bond {
        return Err(StateError::InvalidTx("self-bond below minimum".into()));
    }
    if commission_bps > params.max_commission_bps {
        return Err(StateError::InvalidTx("commission too high".into()));
    }
    if consensus_pubkey.len() != 33 {
        return Err(StateError::InvalidTx(
            "consensus pubkey must be 33 bytes".into(),
        ));
    }
    if let Some(existing) = load_validator(store, params.asset, &operator)? {
        if matches!(existing.status, ValidatorStatus::Tombstoned) {
            return Err(StateError::InvalidTx("tombstoned validator".into()));
        }
    }
    // Count validators for max set size (bonded only).
    let snap = build_snapshot(store, params.asset, load_epoch(store, params.asset)?)?;
    if snap.validators.len() as u32 >= params.max_validators
        && load_validator(store, params.asset, &operator)?.is_none()
    {
        return Err(StateError::InvalidTx("validator set full".into()));
    }

    debit_liquid(store, batch, params.asset, &operator, self_bond)?;
    let record = ValidatorRecord {
        operator,
        consensus_pubkey,
        withdrawal,
        self_bond,
        delegated: 0,
        commission_bps,
        status: ValidatorStatus::Bonded,
        jailed_until_epoch: 0,
        metadata_hash,
    };
    put_validator_into(batch, params.asset, &record)
}

/// Delegate liquid stake to a bonded validator.
pub fn delegate(
    store: &StateStore,
    batch: &mut WriteBatch,
    params: &StakingParams,
    delegator: Address,
    validator: Address,
    amount: u64,
) -> Result<(), StateError> {
    params.validate_asset()?;
    if amount == 0 {
        return Err(StateError::InvalidTx("zero delegation".into()));
    }
    let mut val = load_validator(store, params.asset, &validator)?
        .ok_or_else(|| StateError::InvalidTx("unknown validator".into()))?;
    if !matches!(val.status, ValidatorStatus::Bonded) {
        return Err(StateError::InvalidTx("validator not active".into()));
    }

    // Concentration only applies when other active stake exists; a sole validator
    // necessarily holds 100% and must still accept delegation.
    let snap = build_snapshot(store, params.asset, load_epoch(store, params.asset)?)?;
    let new_power = val.voting_power().saturating_add(amount);
    let new_total = snap.total_active_stake.saturating_add(amount);
    let other_stake = snap.total_active_stake.saturating_sub(val.voting_power());
    if other_stake > 0 && new_total > 0 {
        let share_bps = (u128::from(new_power) * 10_000 / u128::from(new_total)) as u16;
        if share_bps > params.max_concentration_bps {
            return Err(StateError::InvalidTx(
                "delegation would exceed concentration limit".into(),
            ));
        }
    }

    debit_liquid(store, batch, params.asset, &delegator, amount)?;
    val.delegated = val
        .delegated
        .checked_add(amount)
        .ok_or_else(|| StateError::InvalidTx("delegated overflow".into()))?;
    put_validator_into(batch, params.asset, &val)?;

    let mut del =
        load_delegation(store, params.asset, &delegator, &validator)?.unwrap_or(DelegationRecord {
            delegator,
            validator,
            amount: 0,
        });
    del.amount = del
        .amount
        .checked_add(amount)
        .ok_or_else(|| StateError::InvalidTx("delegation overflow".into()))?;
    put_delegation_into(batch, params.asset, &del)
}

fn load_delegation(
    store: &StateStore,
    asset: NativeAssetId,
    delegator: &Address,
    validator: &Address,
) -> Result<Option<DelegationRecord>, StateError> {
    let Some(bytes) = store.get_cf(
        ColumnFamily::Meta,
        &delegation_key(asset, delegator, validator),
    )?
    else {
        return Ok(None);
    };
    Ok(Some(
        DelegationRecord::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?,
    ))
}

fn put_delegation_into(
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    del: &DelegationRecord,
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(del).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &delegation_key(asset, &del.delegator, &del.validator),
        &bytes,
    );
    Ok(())
}

/// Begin unbonding self-bond (simplified: full self-bond).
pub fn begin_unbond_self(
    store: &StateStore,
    batch: &mut WriteBatch,
    params: &StakingParams,
    operator: Address,
) -> Result<(), StateError> {
    params.validate_asset()?;
    let mut val = load_validator(store, params.asset, &operator)?
        .ok_or_else(|| StateError::InvalidTx("unknown validator".into()))?;
    if matches!(val.status, ValidatorStatus::Tombstoned) {
        return Err(StateError::InvalidTx("tombstoned".into()));
    }
    let epoch = load_epoch(store, params.asset)?;
    let amount = val.self_bond;
    val.self_bond = 0;
    val.status = ValidatorStatus::Unbonding;
    put_validator_into(batch, params.asset, &val)?;
    let entry = UnbondingEntry {
        owner: operator,
        amount,
        release_epoch: epoch.saturating_add(params.unbonding_period_epochs),
    };
    let key = unbonding_key(params.asset, &operator);
    let bytes = borsh::to_vec(&entry).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, &key, &bytes);
    Ok(())
}

/// Withdraw matured unbonding entries.
pub fn withdraw_unbonded(
    store: &StateStore,
    batch: &mut WriteBatch,
    params: &StakingParams,
    owner: Address,
) -> Result<u64, StateError> {
    params.validate_asset()?;
    let epoch = load_epoch(store, params.asset)?;
    let key = unbonding_key(params.asset, &owner);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? else {
        return Ok(0);
    };
    let entry =
        UnbondingEntry::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    if epoch < entry.release_epoch {
        return Err(StateError::InvalidTx("unbonding period not elapsed".into()));
    }
    credit_liquid(store, batch, params.asset, &owner, entry.amount)?;
    batch.delete_cf(ColumnFamily::Meta, &key);
    Ok(entry.amount)
}

/// Build a deterministic validator-set snapshot for `epoch`.
pub fn build_snapshot(
    store: &StateStore,
    asset: NativeAssetId,
    epoch: u64,
) -> Result<ValidatorSetSnapshot, StateError> {
    let prefix = {
        let mut p = b"stake/val/".to_vec();
        p.push(asset.wire_byte());
        p.push(b'/');
        p
    };
    let mut validators = Vec::new();
    store.for_each_cf(ColumnFamily::Meta, |key, value| {
        if key.starts_with(&prefix) && key.len() == prefix.len() + 20 {
            let record = ValidatorRecord::try_from_slice(value)
                .map_err(|e| StateError::Storage(e.to_string()))?;
            // Unjail if epoch advanced.
            let mut rec = record;
            if matches!(rec.status, ValidatorStatus::Jailed) && epoch >= rec.jailed_until_epoch {
                rec.status = ValidatorStatus::Bonded;
            }
            let power = rec.voting_power();
            if power > 0 {
                validators.push((rec.operator, power));
            }
        }
        Ok(())
    })?;
    validators.sort_by_key(|entry| entry.0 .0);
    let total_active_stake = validators.iter().map(|(_, p)| *p).sum();
    Ok(ValidatorSetSnapshot {
        epoch,
        asset,
        validators,
        total_active_stake,
    })
}

/// Advance epoch, optional reserve drip, persist snapshot, distribute rewards.
pub fn advance_epoch(
    store: &StateStore,
    batch: &mut WriteBatch,
    asset: NativeAssetId,
) -> Result<ValidatorSetSnapshot, StateError> {
    advance_epoch_with_params(store, batch, &params_for_asset(asset))
}

fn params_for_asset(asset: NativeAssetId) -> StakingParams {
    match asset {
        NativeAssetId::OVL => StakingParams::ovl_default(),
        NativeAssetId::DRC => StakingParams::drc_default(),
        NativeAssetId::TLT => StakingParams::ovl_default(), // unused; validate fails
    }
}

pub fn advance_epoch_with_params(
    store: &StateStore,
    batch: &mut WriteBatch,
    params: &StakingParams,
) -> Result<ValidatorSetSnapshot, StateError> {
    params.validate_asset()?;
    let asset = params.asset;
    let dripped = if params.epoch_reserve_drip > 0 {
        drip_staking_reserve(store, batch, asset, params.epoch_reserve_drip)?
    } else {
        0
    };
    let next = load_epoch(store, asset)?.saturating_add(1);
    let snap = build_snapshot(store, asset, next)?;
    put_epoch_into(batch, asset, next);
    let bytes = borsh::to_vec(&snap).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, &snapshot_key(asset, next), &bytes);
    let pool = load_reward_pool(store, asset)?.saturating_add(dripped);
    let _distributed = distribute_reward_pool_amount(store, batch, asset, &snap, pool)?;
    Ok(snap)
}

/// Apply a network-bound signed stake transaction (bond/delegate/unbond/withdraw).
///
/// Final account put (balance + bumped nonce) wins over helper writes in the same batch.
pub fn apply_signed_stake_tx(
    store: &StateStore,
    batch: &mut WriteBatch,
    tx: &SignedStakeTx,
    auth: &TxAuthContext,
    params: &StakingParams,
) -> Result<(), StateError> {
    params.validate_asset()?;
    if tx.asset != params.asset {
        return Err(StateError::InvalidTx("stake asset mismatch".into()));
    }
    verify_stake_tx_bound(tx, &auth.chain_id, &auth.genesis)
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;

    let acct = load_account(store, tx.asset, &tx.actor)?;
    if acct.nonce != tx.nonce {
        return Err(StateError::InvalidTx(format!(
            "bad stake nonce: got {} expected {}",
            tx.nonce, acct.nonce
        )));
    }

    let mut after = acct.clone();
    match tx.kind {
        StakeOpKind::Bond => {
            if tx.actor != tx.validator {
                return Err(StateError::InvalidTx(
                    "bond actor must equal validator".into(),
                ));
            }
            bond_validator(
                store,
                batch,
                params,
                tx.actor,
                tx.consensus_pubkey.clone(),
                tx.withdrawal,
                tx.amount,
                tx.commission_bps,
                tx.metadata_hash,
            )?;
            after.balance = after
                .balance
                .checked_sub(tx.amount)
                .ok_or_else(|| StateError::InvalidTx("insufficient liquid stake funds".into()))?;
        }
        StakeOpKind::Delegate => {
            delegate(store, batch, params, tx.actor, tx.validator, tx.amount)?;
            after.balance = after
                .balance
                .checked_sub(tx.amount)
                .ok_or_else(|| StateError::InvalidTx("insufficient liquid stake funds".into()))?;
        }
        StakeOpKind::UnbondSelf => {
            begin_unbond_self(store, batch, params, tx.actor)?;
        }
        StakeOpKind::Withdraw => {
            let credited = withdraw_unbonded(store, batch, params, tx.actor)?;
            after.balance = after
                .balance
                .checked_add(credited)
                .ok_or_else(|| StateError::InvalidTx("balance overflow".into()))?;
        }
    }
    after.nonce = tx
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("nonce overflow".into()))?;
    put_account_into(batch, tx.asset, &tx.actor, &after)?;
    Ok(())
}

/// Pro-rata reward-pool drip to bonded validators (commission + self/delegator split).
///
/// Source is slash proceeds (and later staking-reserve drip) — never TLT mint.
pub fn distribute_reward_pool(
    store: &StateStore,
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    snap: &ValidatorSetSnapshot,
) -> Result<u64, StateError> {
    let pool = load_reward_pool(store, asset)?;
    distribute_reward_pool_amount(store, batch, asset, snap, pool)
}

/// Like [`distribute_reward_pool`] but with an explicit pool total (for pending batch drips).
pub fn distribute_reward_pool_amount(
    store: &StateStore,
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    snap: &ValidatorSetSnapshot,
    pool: u64,
) -> Result<u64, StateError> {
    use std::collections::BTreeMap;

    if pool == 0 || snap.total_active_stake == 0 {
        return Ok(0);
    }
    let mut remaining = pool;
    let mut credits: BTreeMap<[u8; 20], u64> = BTreeMap::new();

    for (op, power) in &snap.validators {
        if *power == 0 {
            continue;
        }
        let share =
            ((u128::from(pool) * u128::from(*power)) / u128::from(snap.total_active_stake)) as u64;
        if share == 0 {
            continue;
        }
        let Some(val) = load_validator(store, asset, op)? else {
            continue;
        };
        let commission = ((u128::from(share) * u128::from(val.commission_bps))
            / u128::from(MAX_VALIDATOR_COMMISSION_BPS)) as u64;
        let after_commission = share.saturating_sub(commission);
        let bonded = val.self_bond.saturating_add(val.delegated).max(1);
        let op_stake_share = ((u128::from(after_commission) * u128::from(val.self_bond))
            / u128::from(bonded)) as u64;
        let mut to_operator = commission.saturating_add(op_stake_share);
        let mut del_paid = 0u64;
        let mut del_credits: Vec<(Address, u64)> = Vec::new();
        let prefix = {
            let mut p = b"stake/del/".to_vec();
            p.push(asset.wire_byte());
            p.push(b'/');
            p
        };
        store.for_each_cf(ColumnFamily::Meta, |key, value| {
            if !key.starts_with(&prefix) {
                return Ok(());
            }
            // stake/del/<asset>/<delegator20>/<validator20>
            let expected_len = prefix.len() + 20 + 1 + 20;
            if key.len() != expected_len || key[prefix.len() + 20] != b'/' {
                return Ok(());
            }
            if &key[prefix.len() + 21..] != op.0.as_slice() {
                return Ok(());
            }
            let del = DelegationRecord::try_from_slice(value)
                .map_err(|e| StateError::Storage(e.to_string()))?;
            if del.amount == 0 {
                return Ok(());
            }
            let piece = ((u128::from(after_commission) * u128::from(del.amount))
                / u128::from(bonded)) as u64;
            if piece > 0 {
                del_credits.push((del.delegator, piece));
                del_paid = del_paid.saturating_add(piece);
            }
            Ok(())
        })?;
        for (addr, amt) in del_credits {
            credits
                .entry(addr.0)
                .and_modify(|v| *v = v.saturating_add(amt))
                .or_insert(amt);
        }
        let unpaid = after_commission.saturating_sub(op_stake_share + del_paid);
        to_operator = to_operator.saturating_add(unpaid);
        if to_operator > 0 {
            credits
                .entry(val.withdrawal.0)
                .and_modify(|v| *v = v.saturating_add(to_operator))
                .or_insert(to_operator);
        }
        remaining = remaining.saturating_sub(share);
    }
    for (bytes, amt) in credits {
        credit_liquid(store, batch, asset, &Address(bytes), amt)?;
    }
    put_reward_pool_into(batch, asset, remaining);
    Ok(pool.saturating_sub(remaining))
}

pub fn load_snapshot(
    store: &StateStore,
    asset: NativeAssetId,
    epoch: u64,
) -> Result<Option<ValidatorSetSnapshot>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &snapshot_key(asset, epoch))? else {
        return Ok(None);
    };
    Ok(Some(
        ValidatorSetSnapshot::try_from_slice(&bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?,
    ))
}

/// Apply slash / jail / tombstone from evidence.
pub fn apply_evidence(
    store: &StateStore,
    batch: &mut WriteBatch,
    evidence: &ValidatorEvidence,
    policy: &SlashPolicy,
) -> Result<u64, StateError> {
    if !matches!(evidence.set, NativeAssetId::OVL | NativeAssetId::DRC) {
        return Err(StateError::InvalidTx("bad evidence set".into()));
    }
    let mut val = load_validator(store, evidence.set, &evidence.validator)?
        .ok_or_else(|| StateError::InvalidTx("unknown validator".into()))?;
    let bonded = val.self_bond.saturating_add(val.delegated);
    let (slash, tombstone, jail_epochs) = policy.penalty_for(&evidence.kind, bonded);
    // Slash from self_bond first, then delegated.
    let mut remaining = slash;
    let from_self = remaining.min(val.self_bond);
    val.self_bond -= from_self;
    remaining -= from_self;
    let from_del = remaining.min(val.delegated);
    val.delegated -= from_del;
    if tombstone {
        val.status = ValidatorStatus::Tombstoned;
    } else if jail_epochs > 0 {
        let epoch = load_epoch(store, evidence.set)?;
        val.status = ValidatorStatus::Jailed;
        val.jailed_until_epoch = epoch.saturating_add(jail_epochs);
    }
    put_validator_into(batch, evidence.set, &val)?;
    if slash > 0 {
        credit_reward_pool_into(store, batch, evidence.set, slash)?;
    }
    Ok(slash)
}

/// Sum voting power of validators that signed (by address) within a snapshot.
pub fn signed_stake_for(snapshot: &ValidatorSetSnapshot, signers: &[Address]) -> u64 {
    let mut total = 0u64;
    for s in signers {
        total = total.saturating_add(snapshot.power_of(s));
    }
    total
}

/// Verify attestation pubkey matches registered consensus key.
pub fn validator_key_matches(
    store: &StateStore,
    att: &CheckpointAttestation,
) -> Result<bool, StateError> {
    let Some(val) = load_validator(store, att.set, &att.validator)? else {
        return Ok(false);
    };
    Ok(val.consensus_pubkey == att.public_key && matches!(val.status, ValidatorStatus::Bonded))
}

#[cfg(test)]
mod tests {
    use agora_consensus::{detect_double_checkpoint, SlashPolicy};
    use agora_crypto::{
        derive_bip44, seed_from_mnemonic, sign_checkpoint_attestation, sign_stake_tx_bound,
        Bip44Path,
    };
    use agora_types::{Amount, CheckpointBody, Hash, NativeAssetId, SignedStakeTx};

    use super::*;
    use crate::accounts::credit_account_into;
    use crate::apply::TxAuthContext;
    use crate::supply::{load_issued_supply, put_issued_supply_into, put_max_supply_into};
    use crate::StateStore;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn fund(store: &StateStore, asset: NativeAssetId, who: &Address, amt: u64) {
        let mut batch = WriteBatch::new();
        credit_account_into(&mut batch, store, asset, who, Amount::from_base_units(amt)).unwrap();
        store.write_batch(batch).unwrap();
    }

    #[test]
    fn genesis_validator_record_has_canonical_key_and_borsh_value() {
        let key = agora_crypto::KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let params = StakingParams {
            min_self_bond: 1,
            max_commission_bps: 2_000,
            ..StakingParams::ovl_default()
        };
        let record = ValidatorRecord {
            operator: key.address(),
            consensus_pubkey: key.public_key_bytes().to_vec(),
            withdrawal: Address([0x22; 20]),
            self_bond: 5,
            delegated: 0,
            commission_bps: 100,
            status: ValidatorStatus::Bonded,
            jailed_until_epoch: 0,
            metadata_hash: Hash([0x33; 32]),
        };

        let (storage_key, value) = record.canonical_genesis_storage_entry(&params).unwrap();
        assert_eq!(
            storage_key,
            validator_meta_key(NativeAssetId::OVL, &record.operator)
        );
        assert_eq!(ValidatorRecord::try_from_slice(&value).unwrap(), record);
    }

    #[test]
    fn genesis_validator_record_rejects_defaults_and_limits() {
        let key = agora_crypto::KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let params = StakingParams {
            min_self_bond: 1,
            max_commission_bps: 2_000,
            ..StakingParams::ovl_default()
        };
        let record = ValidatorRecord {
            operator: key.address(),
            consensus_pubkey: key.public_key_bytes().to_vec(),
            withdrawal: Address([0x22; 20]),
            self_bond: 5,
            delegated: 0,
            commission_bps: 100,
            status: ValidatorStatus::Bonded,
            jailed_until_epoch: 0,
            metadata_hash: Hash([0x33; 32]),
        };

        let mut changed = record.clone();
        changed.metadata_hash = Hash::ZERO;
        assert!(changed
            .validate_genesis_registration(&params)
            .unwrap_err()
            .to_string()
            .contains("metadata hash must be nonzero"));

        let mut changed = record.clone();
        changed.commission_bps = params.max_commission_bps + 1;
        assert!(changed
            .validate_genesis_registration(&params)
            .unwrap_err()
            .to_string()
            .contains("commission too high"));

        let mut changed = record;
        changed.operator = Address([0x44; 20]);
        assert!(changed
            .validate_genesis_registration(&params)
            .unwrap_err()
            .to_string()
            .contains("operator does not match"));

        let mut invalid_params = params;
        invalid_params.max_commission_bps = MAX_VALIDATOR_COMMISSION_BPS + 1;
        assert!(invalid_params.validate().is_err());
    }

    #[test]
    fn ovl_bond_delegate_epoch_and_quorum_power() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let op = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let del = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let params = StakingParams {
            min_self_bond: 100,
            ..StakingParams::ovl_default()
        };
        fund(&store, NativeAssetId::OVL, &op.address(), 1_000);
        fund(&store, NativeAssetId::OVL, &del.address(), 500);

        let mut batch = WriteBatch::new();
        bond_validator(
            &store,
            &mut batch,
            &params,
            op.address(),
            op.public_key_bytes().to_vec(),
            op.address(),
            200,
            100,
            Hash::ZERO,
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let mut batch = WriteBatch::new();
        delegate(
            &store,
            &mut batch,
            &params,
            del.address(),
            op.address(),
            300,
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let mut batch = WriteBatch::new();
        let snap = advance_epoch(&store, &mut batch, NativeAssetId::OVL).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(snap.total_active_stake, 500);
        assert_eq!(snap.power_of(&op.address()), 500);
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &op.address())
                .unwrap()
                .balance,
            800
        );
    }

    #[test]
    fn independent_ovl_drc_sets() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let a = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        fund(&store, NativeAssetId::OVL, &a.address(), 10_000);
        fund(&store, NativeAssetId::DRC, &a.address(), 10_000);
        let ovl = StakingParams {
            min_self_bond: 100,
            ..StakingParams::ovl_default()
        };
        let drc = StakingParams {
            min_self_bond: 100,
            ..StakingParams::drc_default()
        };
        let mut batch = WriteBatch::new();
        bond_validator(
            &store,
            &mut batch,
            &ovl,
            a.address(),
            a.public_key_bytes().to_vec(),
            a.address(),
            100,
            0,
            Hash::ZERO,
        )
        .unwrap();
        bond_validator(
            &store,
            &mut batch,
            &drc,
            a.address(),
            a.public_key_bytes().to_vec(),
            a.address(),
            200,
            0,
            Hash::ZERO,
        )
        .unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            load_validator(&store, NativeAssetId::OVL, &a.address())
                .unwrap()
                .unwrap()
                .self_bond,
            100
        );
        assert_eq!(
            load_validator(&store, NativeAssetId::DRC, &a.address())
                .unwrap()
                .unwrap()
                .self_bond,
            200
        );
    }

    #[test]
    fn equivocation_tombstones_and_slashes() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let op = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let params = StakingParams {
            min_self_bond: 10_000,
            ..StakingParams::ovl_default()
        };
        fund(&store, NativeAssetId::OVL, &op.address(), 20_000);
        let mut batch = WriteBatch::new();
        bond_validator(
            &store,
            &mut batch,
            &params,
            op.address(),
            op.public_key_bytes().to_vec(),
            op.address(),
            10_000,
            0,
            Hash::ZERO,
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let body = |b| CheckpointBody {
            chain_id: "c".into(),
            genesis_hash: Hash::ZERO,
            consensus_policy_hash: Hash::ZERO,
            state_transition_version: "v".into(),
            blue_score: 1,
            block_hash: Hash([b; 32]),
            state_root: Hash::ZERO,
            validator_epoch: 1,
        };
        let a = sign_checkpoint_attestation(body(1), NativeAssetId::OVL, &op).unwrap();
        let b = sign_checkpoint_attestation(body(2), NativeAssetId::OVL, &op).unwrap();
        let ev = detect_double_checkpoint(&a, &b).unwrap();
        let mut batch = WriteBatch::new();
        let slashed = apply_evidence(&store, &mut batch, &ev, &SlashPolicy::default()).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(slashed, 500); // 5% of 10000
        let val = load_validator(&store, NativeAssetId::OVL, &op.address())
            .unwrap()
            .unwrap();
        assert_eq!(val.status, ValidatorStatus::Tombstoned);
        assert_eq!(val.self_bond, 9_500);
    }

    #[test]
    fn slash_credits_reward_pool_and_epoch_distributes() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let op = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let params = StakingParams {
            min_self_bond: 10_000,
            ..StakingParams::ovl_default()
        };
        fund(&store, NativeAssetId::OVL, &op.address(), 20_000);
        let mut batch = WriteBatch::new();
        bond_validator(
            &store,
            &mut batch,
            &params,
            op.address(),
            op.public_key_bytes().to_vec(),
            op.address(),
            10_000,
            0,
            Hash::ZERO,
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let body = |b| CheckpointBody {
            chain_id: "c".into(),
            genesis_hash: Hash::ZERO,
            consensus_policy_hash: Hash::ZERO,
            state_transition_version: "v".into(),
            blue_score: 1,
            block_hash: Hash([b; 32]),
            state_root: Hash::ZERO,
            validator_epoch: 1,
        };
        let a = sign_checkpoint_attestation(body(1), NativeAssetId::OVL, &op).unwrap();
        let b = sign_checkpoint_attestation(body(2), NativeAssetId::OVL, &op).unwrap();
        let ev = detect_double_checkpoint(&a, &b).unwrap();
        let mut batch = WriteBatch::new();
        let slashed = apply_evidence(&store, &mut batch, &ev, &SlashPolicy::default()).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(slashed, 500);
        assert_eq!(load_reward_pool(&store, NativeAssetId::OVL).unwrap(), 500);

        // Bond again via a fresh validator address so the pool can distribute to an active set.
        let op2 = derive_bip44(&seed, &Bip44Path::external(3)).unwrap();
        fund(&store, NativeAssetId::OVL, &op2.address(), 1_000);
        let mut batch = WriteBatch::new();
        bond_validator(
            &store,
            &mut batch,
            &StakingParams {
                min_self_bond: 100,
                ..StakingParams::ovl_default()
            },
            op2.address(),
            op2.public_key_bytes().to_vec(),
            op2.address(),
            100,
            0,
            Hash::ZERO,
        )
        .unwrap();
        store.write_batch(batch).unwrap();
        let mut batch = WriteBatch::new();
        let snap = advance_epoch(&store, &mut batch, NativeAssetId::OVL).unwrap();
        store.write_batch(batch).unwrap();
        assert!(snap.total_active_stake >= 100);
        // Tombstoned op has 0 power; op2 receives the pool.
        assert_eq!(load_reward_pool(&store, NativeAssetId::OVL).unwrap(), 0);
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &op2.address())
                .unwrap()
                .balance,
            900 + 500 // 1000-100 bond + 500 reward
        );
    }

    #[test]
    fn unbond_and_withdraw_after_period() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let op = derive_bip44(&seed, &Bip44Path::external(2)).unwrap();
        let params = StakingParams {
            min_self_bond: 50,
            unbonding_period_epochs: 2,
            ..StakingParams::drc_default()
        };
        fund(&store, NativeAssetId::DRC, &op.address(), 100);
        let mut batch = WriteBatch::new();
        bond_validator(
            &store,
            &mut batch,
            &params,
            op.address(),
            op.public_key_bytes().to_vec(),
            op.address(),
            50,
            0,
            Hash::ZERO,
        )
        .unwrap();
        store.write_batch(batch).unwrap();
        let mut batch = WriteBatch::new();
        begin_unbond_self(&store, &mut batch, &params, op.address()).unwrap();
        store.write_batch(batch).unwrap();
        // Too early.
        let mut batch = WriteBatch::new();
        assert!(withdraw_unbonded(&store, &mut batch, &params, op.address()).is_err());
        // Advance epochs.
        for _ in 0..2 {
            let mut batch = WriteBatch::new();
            advance_epoch(&store, &mut batch, NativeAssetId::DRC).unwrap();
            store.write_batch(batch).unwrap();
        }
        let mut batch = WriteBatch::new();
        let got = withdraw_unbonded(&store, &mut batch, &params, op.address()).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(got, 50);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &op.address())
                .unwrap()
                .balance,
            100
        );
    }

    #[test]
    fn signed_stake_tx_bond_bumps_nonce() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let op = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        fund(&store, NativeAssetId::OVL, &op.address(), 1_000);
        let genesis = Hash([3u8; 32]);
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis,
        };
        let params = StakingParams {
            min_self_bond: 100,
            ..StakingParams::ovl_default()
        };
        let mut tx = SignedStakeTx::unsigned_bond(
            NativeAssetId::OVL,
            op.address(),
            200,
            op.public_key_bytes().to_vec(),
            op.address(),
            50,
            0,
        );
        sign_stake_tx_bound(&mut tx, &op, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        apply_signed_stake_tx(&store, &mut batch, &tx, &auth, &params).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            load_validator(&store, NativeAssetId::OVL, &op.address())
                .unwrap()
                .unwrap()
                .self_bond,
            200
        );
        let acct = load_account(&store, NativeAssetId::OVL, &op.address()).unwrap();
        assert_eq!(acct.balance, 800);
        assert_eq!(acct.nonce, 1);
    }

    #[test]
    fn reserve_drip_credits_pool_and_issued() {
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        put_max_supply_into(&mut batch, NativeAssetId::OVL, 1_000_000);
        put_issued_supply_into(&mut batch, NativeAssetId::OVL, 0);
        init_staking_reserve_into(&mut batch, NativeAssetId::OVL, 1_000).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            load_staking_reserve_remaining(&store, NativeAssetId::OVL).unwrap(),
            1_000
        );
        let mut batch = WriteBatch::new();
        let dripped = drip_staking_reserve(&store, &mut batch, NativeAssetId::OVL, 250).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(dripped, 250);
        assert_eq!(
            load_staking_reserve_remaining(&store, NativeAssetId::OVL).unwrap(),
            750
        );
        assert_eq!(load_reward_pool(&store, NativeAssetId::OVL).unwrap(), 250);
        assert_eq!(load_issued_supply(&store, NativeAssetId::OVL).unwrap(), 250);
        // Fee-share alias.
        let mut batch = WriteBatch::new();
        credit_fee_share_to_reward_pool(&store, &mut batch, NativeAssetId::OVL, 10).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(load_reward_pool(&store, NativeAssetId::OVL).unwrap(), 260);
    }
}
