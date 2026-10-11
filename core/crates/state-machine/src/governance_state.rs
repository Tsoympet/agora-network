//! Canonical Trident governance policy and asset-isolated protocol treasuries.
//!
//! This module commits immutable v1 authorization policy and treasury balances
//! into consensus state. The existing unsigned node-local civic RPC snapshot is
//! intentionally stored under a different key and excluded from this root.

use agora_crypto::{verify_treasury_disbursement_bound, verify_vesting_unlock_bound};
use agora_governance::{
    authorization_for_class, hash_constitution_body, trident_policy_catalog, ProposalAuthorization,
    ProposalClass, CONSTITUTION_V1_BODY, CONSTITUTION_V1_ID,
};
use agora_types::{
    vested_amount_at, vesting_schedule_id, Address, Amount, Hash, NativeAssetId, OutPoint,
    TreasuryBalance, TreasuryDisbursement, TreasuryId, TxOut, VestingUnlock,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    accounts::credit_account_into, apply::TxAuthContext, columns::ColumnFamily, store::WriteBatch,
    utxo::outpoint_key, StateError, StateStore,
};

const POLICY_KEY: &[u8] = b"governance/consensus/policy";
const TREASURY_PREFIX: &[u8] = b"governance/treasury/";
const TREASURY_CONTROL_PREFIX: &[u8] = b"governance/treasury_control/";
const TREASURY_CONTROLLER_PREFIX: &[u8] = b"governance/treasury_controller/";
const TREASURY_NONCE_PREFIX: &[u8] = b"governance/treasury_nonce/";
const TREASURY_DISBURSEMENT_PREFIX: &[u8] = b"governance/treasury_disbursement/";
const EMERGENCY_POLICY_KEY: &[u8] = b"governance/consensus/emergency_policy_hash";
const VESTING_KEY: &[u8] = b"governance/vesting/schedules-v1";
const VESTING_UNLOCKED_PREFIX: &[u8] = b"governance/vesting/unlocked/";
const VESTING_NONCE_PREFIX: &[u8] = b"governance/vesting/nonce/";
const VESTING_CLAIM_PREFIX: &[u8] = b"governance/vesting/claim/";
pub const CANONICAL_GOVERNANCE_VERSION: u32 = 1;
const GOVERNANCE_TREASURY_ROOT_DOMAIN: &[u8] = b"agora-governance-treasury-root-v3";

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CanonicalGovernancePolicy {
    pub version: u32,
    pub constitution_id: String,
    pub constitution_hash: Hash,
    pub authorization_root: Hash,
}

pub fn authorization_policy_root() -> Hash {
    let policies: Vec<(ProposalClass, ProposalAuthorization)> = ProposalClass::ALL
        .iter()
        .copied()
        .map(|class| (class, authorization_for_class(class)))
        .collect();
    Hash::hash_borsh(&(
        b"agora-governance-authorization-v2",
        policies,
        trident_policy_catalog(),
    ))
}

impl Default for CanonicalGovernancePolicy {
    fn default() -> Self {
        Self {
            version: CANONICAL_GOVERNANCE_VERSION,
            constitution_id: CONSTITUTION_V1_ID.into(),
            constitution_hash: Hash(hash_constitution_body(CONSTITUTION_V1_BODY)),
            authorization_root: authorization_policy_root(),
        }
    }
}

fn treasury_key(treasury: TreasuryId) -> Vec<u8> {
    let mut key = Vec::with_capacity(TREASURY_PREFIX.len() + 1);
    key.extend_from_slice(TREASURY_PREFIX);
    key.push(treasury.wire_byte());
    key
}

pub(crate) fn put_treasury_into(
    batch: &mut WriteBatch,
    treasury: &TreasuryBalance,
) -> Result<(), StateError> {
    if treasury.treasury.asset() != treasury.asset {
        return Err(StateError::InvalidTx(
            "protocol treasury asset mismatch".into(),
        ));
    }
    let bytes = borsh::to_vec(treasury).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, &treasury_key(treasury.treasury), &bytes);
    Ok(())
}

fn treasury_controller_key(treasury: TreasuryId) -> Vec<u8> {
    let mut key = Vec::with_capacity(TREASURY_CONTROLLER_PREFIX.len() + 1);
    key.extend_from_slice(TREASURY_CONTROLLER_PREFIX);
    key.push(treasury.wire_byte());
    key
}

fn treasury_nonce_key(treasury: TreasuryId) -> Vec<u8> {
    let mut key = Vec::with_capacity(TREASURY_NONCE_PREFIX.len() + 1);
    key.extend_from_slice(TREASURY_NONCE_PREFIX);
    key.push(treasury.wire_byte());
    key
}

pub fn treasury_balance_key(treasury: TreasuryId) -> Vec<u8> {
    treasury_key(treasury)
}

pub fn treasury_controller_record_key(treasury: TreasuryId) -> Vec<u8> {
    treasury_controller_key(treasury)
}

pub fn treasury_nonce_record_key(treasury: TreasuryId) -> Vec<u8> {
    treasury_nonce_key(treasury)
}

pub fn treasury_disbursement_record_key(id: &Hash) -> Vec<u8> {
    treasury_disbursement_key(id)
}

fn treasury_disbursement_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(TREASURY_DISBURSEMENT_PREFIX.len() + 32);
    key.extend_from_slice(TREASURY_DISBURSEMENT_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn load_treasury_disbursement(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<TreasuryDisbursement>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &treasury_disbursement_key(id))? else {
        return Ok(None);
    };
    TreasuryDisbursement::try_from_slice(&bytes)
        .map(Some)
        .map_err(|e| StateError::Storage(e.to_string()))
}

pub fn put_treasury_controller_into(
    batch: &mut WriteBatch,
    treasury: TreasuryId,
    controller: Address,
) -> Result<(), StateError> {
    if controller == Address::ZERO {
        return Err(StateError::InvalidTx(
            "treasury controller must be nonzero".into(),
        ));
    }
    let bytes = borsh::to_vec(&controller).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &treasury_controller_key(treasury),
        &bytes,
    );
    Ok(())
}

pub fn load_treasury_controller(
    store: &StateStore,
    treasury: TreasuryId,
) -> Result<Option<Address>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &treasury_controller_key(treasury))? else {
        return Ok(None);
    };
    Address::try_from_slice(&bytes)
        .map(Some)
        .map_err(|e| StateError::Storage(e.to_string()))
}

pub fn load_treasury_nonce(store: &StateStore, treasury: TreasuryId) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &treasury_nonce_key(treasury))? else {
        return Ok(0);
    };
    u64::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))
}

fn treasury_control_key(treasury: TreasuryId) -> Vec<u8> {
    let mut key = Vec::with_capacity(TREASURY_CONTROL_PREFIX.len() + 1);
    key.extend_from_slice(TREASURY_CONTROL_PREFIX);
    key.push(treasury.wire_byte());
    key
}

pub(crate) fn put_treasury_control_into(
    batch: &mut WriteBatch,
    treasury: TreasuryId,
    control: &str,
) -> Result<(), StateError> {
    if control.trim().is_empty() {
        return Err(StateError::InvalidTx(
            "protocol treasury control must be nonempty".into(),
        ));
    }
    let bytes =
        borsh::to_vec(&control.to_string()).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, &treasury_control_key(treasury), &bytes);
    Ok(())
}

pub(crate) fn put_emergency_policy_hash_into(batch: &mut WriteBatch, hash: Hash) {
    batch.put_cf(ColumnFamily::Meta, EMERGENCY_POLICY_KEY, hash.as_bytes());
}

pub(crate) fn put_vesting_schedules_into(
    batch: &mut WriteBatch,
    vesting: &[crate::block_zero::BlockZeroVesting],
) -> Result<(), StateError> {
    let bytes = borsh::to_vec(&vesting.to_vec()).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, VESTING_KEY, &bytes);
    Ok(())
}

fn load_emergency_policy_hash(store: &StateStore) -> Result<Hash, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, EMERGENCY_POLICY_KEY)? else {
        return Ok(Hash::ZERO);
    };
    if bytes.len() != 32 {
        return Err(StateError::Storage(
            "malformed emergency policy hash".into(),
        ));
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(Hash(hash))
}

fn load_treasury_controls(store: &StateStore) -> Result<Vec<(u8, String)>, StateError> {
    let mut controls = Vec::new();
    for treasury in TreasuryId::ALL {
        let Some(bytes) = store.get_cf(ColumnFamily::Meta, &treasury_control_key(treasury))? else {
            continue;
        };
        let control = String::try_from_slice(&bytes)
            .map_err(|error| StateError::Storage(error.to_string()))?;
        controls.push((treasury.wire_byte(), control));
    }
    Ok(controls)
}

pub fn load_vesting_schedules(
    store: &StateStore,
) -> Result<Vec<crate::block_zero::BlockZeroVesting>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, VESTING_KEY)? else {
        return Ok(Vec::new());
    };
    Vec::<crate::block_zero::BlockZeroVesting>::try_from_slice(&bytes)
        .map_err(|error| StateError::Storage(error.to_string()))
}

/// Write artifact-selected constitution, emergency policy, treasuries, controls, and vesting.
pub fn init_trident_governance_into(
    batch: &mut WriteBatch,
    constitution_hash: Hash,
    emergency_policy_hash: Hash,
    treasuries: &[TreasuryBalance],
    controls: &[(TreasuryId, String)],
    vesting: &[crate::block_zero::BlockZeroVesting],
) -> Result<(), StateError> {
    if constitution_hash == Hash::ZERO || emergency_policy_hash == Hash::ZERO {
        return Err(StateError::InvalidTx(
            "Trident governance hashes must be nonzero".into(),
        ));
    }
    let policy = CanonicalGovernancePolicy {
        version: CANONICAL_GOVERNANCE_VERSION,
        constitution_id: if constitution_hash == Hash(hash_constitution_body(CONSTITUTION_V1_BODY))
        {
            CONSTITUTION_V1_ID.into()
        } else {
            "trident-genesis-artifact".into()
        },
        constitution_hash,
        authorization_root: authorization_policy_root(),
    };
    let bytes = borsh::to_vec(&policy).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, POLICY_KEY, &bytes);
    put_emergency_policy_hash_into(batch, emergency_policy_hash);
    for treasury in treasuries {
        put_treasury_into(batch, treasury)?;
    }
    for (treasury, control) in controls {
        put_treasury_control_into(batch, *treasury, control)?;
    }
    put_vesting_schedules_into(batch, vesting)?;
    Ok(())
}

pub fn init_canonical_governance_into(batch: &mut WriteBatch) -> Result<(), StateError> {
    let policy = CanonicalGovernancePolicy::default();
    let bytes = borsh::to_vec(&policy).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, POLICY_KEY, &bytes);
    for treasury in TreasuryId::ALL {
        put_treasury_into(
            batch,
            &TreasuryBalance::new(treasury, treasury.asset(), Amount::ZERO)
                .map_err(StateError::InvalidTx)?,
        )?;
    }
    Ok(())
}

pub fn load_canonical_governance_policy(
    store: &StateStore,
) -> Result<CanonicalGovernancePolicy, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, POLICY_KEY)? else {
        return Ok(CanonicalGovernancePolicy::default());
    };
    CanonicalGovernancePolicy::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))
}

pub fn load_protocol_treasury(
    store: &StateStore,
    treasury: TreasuryId,
) -> Result<TreasuryBalance, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &treasury_key(treasury))? else {
        return TreasuryBalance::new(treasury, treasury.asset(), Amount::ZERO)
            .map_err(StateError::InvalidTx);
    };
    let balance =
        TreasuryBalance::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    if balance.treasury != treasury || balance.asset != treasury.asset() {
        return Err(StateError::Storage(
            "corrupt protocol treasury asset identity".into(),
        ));
    }
    Ok(balance)
}

pub fn apply_treasury_disbursement_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    spend: &TreasuryDisbursement,
    auth: &TxAuthContext,
) -> Result<Option<OutPoint>, StateError> {
    if spend.version != 1 {
        return Err(StateError::InvalidTx(
            "unsupported treasury disbursement version".into(),
        ));
    }
    if spend.beneficiary == Address::ZERO {
        return Err(StateError::InvalidTx(
            "treasury beneficiary must be nonzero".into(),
        ));
    }
    if spend.amount == Amount::ZERO {
        return Err(StateError::InvalidTx(
            "treasury amount must be nonzero".into(),
        ));
    }
    if spend.reason_hash == Hash::ZERO {
        return Err(StateError::InvalidTx(
            "treasury reason hash must be nonzero".into(),
        ));
    }
    let signer = verify_treasury_disbursement_bound(spend, &auth.chain_id, &auth.genesis)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    let policy = load_canonical_governance_policy(store)?;
    if spend.authorization_root != policy.authorization_root {
        return Err(StateError::InvalidTx(
            "treasury authorization root mismatch".into(),
        ));
    }
    let controller = load_treasury_controller(store, spend.treasury)?
        .ok_or_else(|| StateError::InvalidTx("treasury has no consensus controller".into()))?;
    if signer != controller {
        return Err(StateError::InvalidTx(
            "treasury disbursement signer is not the controller".into(),
        ));
    }
    let current_nonce = load_treasury_nonce(store, spend.treasury)?;
    if spend.nonce != current_nonce {
        return Err(StateError::InvalidTx("treasury nonce mismatch".into()));
    }
    let id = spend.disbursement_id();
    if store
        .get_cf(ColumnFamily::Meta, &treasury_disbursement_key(&id))?
        .is_some()
    {
        return Err(StateError::InvalidTx(
            "duplicate treasury disbursement".into(),
        ));
    }
    let mut balance = load_protocol_treasury(store, spend.treasury)?;
    let next_balance = balance
        .balance
        .checked_sub(spend.amount)
        .ok_or_else(|| StateError::InvalidTx("treasury balance insufficient".into()))?;
    balance.balance = next_balance;
    let next_nonce = current_nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("treasury nonce overflow".into()))?;

    let mut pending = WriteBatch::new();
    put_treasury_into(&mut pending, &balance)?;
    let nonce_bytes = borsh::to_vec(&next_nonce).map_err(|e| StateError::Storage(e.to_string()))?;
    pending.put_cf(
        ColumnFamily::Meta,
        &treasury_nonce_key(spend.treasury),
        &nonce_bytes,
    );
    let rec = borsh::to_vec(spend).map_err(|e| StateError::Storage(e.to_string()))?;
    pending.put_cf(ColumnFamily::Meta, &treasury_disbursement_key(&id), &rec);

    let created = match spend.treasury.asset() {
        NativeAssetId::TLT => {
            let op = OutPoint {
                tx_id: spend.disbursement_id(),
                index: 0,
            };
            let key = outpoint_key(&op);
            if store.get_cf(ColumnFamily::Utxo, &key)?.is_some() {
                return Err(StateError::DuplicateOutpoint(format!(
                    "{}:0",
                    op.tx_id.to_hex()
                )));
            }
            let out = TxOut {
                value: spend.amount,
                address: spend.beneficiary,
            };
            let bytes = borsh::to_vec(&out).map_err(|e| StateError::Storage(e.to_string()))?;
            pending.put_cf(ColumnFamily::Utxo, &key, &bytes);
            Some(op)
        }
        NativeAssetId::OVL | NativeAssetId::DRC => {
            credit_account_into(
                &mut pending,
                store,
                spend.treasury.asset(),
                &spend.beneficiary,
                spend.amount,
            )?;
            None
        }
    };
    batch.append(pending);
    Ok(created)
}

pub fn vesting_unlocked_record_key(schedule_id: &Hash) -> Vec<u8> {
    vesting_unlocked_key(schedule_id)
}

pub fn vesting_nonce_record_key(beneficiary: &Address) -> Vec<u8> {
    vesting_nonce_key(beneficiary)
}

pub fn vesting_claim_record_key(id: &Hash) -> Vec<u8> {
    vesting_claim_key(id)
}

fn vesting_unlocked_key(schedule_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(VESTING_UNLOCKED_PREFIX.len() + 32);
    key.extend_from_slice(VESTING_UNLOCKED_PREFIX);
    key.extend_from_slice(schedule_id.as_bytes());
    key
}

fn vesting_nonce_key(beneficiary: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(VESTING_NONCE_PREFIX.len() + 20);
    key.extend_from_slice(VESTING_NONCE_PREFIX);
    key.extend_from_slice(&beneficiary.0);
    key
}

fn vesting_claim_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(VESTING_CLAIM_PREFIX.len() + 32);
    key.extend_from_slice(VESTING_CLAIM_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn load_vesting_unlocked(store: &StateStore, schedule_id: &Hash) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &vesting_unlocked_key(schedule_id))? else {
        return Ok(0);
    };
    u64::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))
}

pub fn load_vesting_nonce(store: &StateStore, beneficiary: &Address) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &vesting_nonce_key(beneficiary))? else {
        return Ok(0);
    };
    u64::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))
}

pub fn load_vesting_unlock(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<VestingUnlock>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &vesting_claim_key(id))? else {
        return Ok(None);
    };
    VestingUnlock::try_from_slice(&bytes)
        .map(Some)
        .map_err(|e| StateError::Storage(e.to_string()))
}

pub fn load_vesting_progress(store: &StateStore) -> Result<Vec<(Hash, u64)>, StateError> {
    let mut progress = Vec::new();
    for schedule in load_vesting_schedules(store)? {
        let id = vesting_schedule_id(
            schedule.asset,
            &schedule.address,
            schedule.amount,
            schedule.start_timestamp_ms,
            schedule.cliff_timestamp_ms,
            schedule.end_timestamp_ms,
        );
        let unlocked = load_vesting_unlocked(store, &id)?;
        if unlocked > 0 {
            progress.push((id, unlocked));
        }
    }
    progress.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    Ok(progress)
}

pub fn apply_vesting_unlock_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    claim: &VestingUnlock,
    auth: &TxAuthContext,
    now_ms: u64,
) -> Result<Option<OutPoint>, StateError> {
    if claim.version != 1 {
        return Err(StateError::InvalidTx(
            "unsupported vesting unlock version".into(),
        ));
    }
    if claim.beneficiary == Address::ZERO {
        return Err(StateError::InvalidTx(
            "vesting beneficiary must be nonzero".into(),
        ));
    }
    if claim.amount == Amount::ZERO {
        return Err(StateError::InvalidTx(
            "vesting amount must be nonzero".into(),
        ));
    }
    let signer = verify_vesting_unlock_bound(claim, &auth.chain_id, &auth.genesis)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    if signer != claim.beneficiary {
        return Err(StateError::InvalidTx(
            "vesting unlock signer is not the beneficiary".into(),
        ));
    }
    let schedules = load_vesting_schedules(store)?;
    let schedule = schedules
        .iter()
        .find(|schedule| {
            vesting_schedule_id(
                schedule.asset,
                &schedule.address,
                schedule.amount,
                schedule.start_timestamp_ms,
                schedule.cliff_timestamp_ms,
                schedule.end_timestamp_ms,
            ) == claim.schedule_id
        })
        .ok_or_else(|| StateError::InvalidTx("unknown vesting schedule".into()))?;
    if schedule.asset != claim.asset || schedule.address != claim.beneficiary {
        return Err(StateError::InvalidTx(
            "vesting unlock does not match the schedule".into(),
        ));
    }
    let current_nonce = load_vesting_nonce(store, &claim.beneficiary)?;
    if claim.nonce != current_nonce {
        return Err(StateError::InvalidTx("vesting nonce mismatch".into()));
    }
    let id = claim.unlock_id();
    if store
        .get_cf(ColumnFamily::Meta, &vesting_claim_key(&id))?
        .is_some()
    {
        return Err(StateError::InvalidTx("duplicate vesting unlock".into()));
    }
    let vested = vested_amount_at(
        schedule.amount,
        schedule.start_timestamp_ms,
        schedule.cliff_timestamp_ms,
        schedule.end_timestamp_ms,
        now_ms,
    );
    let already = load_vesting_unlocked(store, &claim.schedule_id)?;
    let claimable = vested
        .checked_sub(already)
        .ok_or_else(|| StateError::InvalidTx("vesting unlock accounting overflow".into()))?;
    if claim.amount.as_base_units() > claimable {
        return Err(StateError::InvalidTx(
            "vesting unlock exceeds vested remainder".into(),
        ));
    }
    let next_unlocked = already
        .checked_add(claim.amount.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("vesting unlock overflow".into()))?;
    let next_nonce = current_nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("vesting nonce overflow".into()))?;

    let mut pending = WriteBatch::new();
    let unlocked_bytes =
        borsh::to_vec(&next_unlocked).map_err(|e| StateError::Storage(e.to_string()))?;
    pending.put_cf(
        ColumnFamily::Meta,
        &vesting_unlocked_key(&claim.schedule_id),
        &unlocked_bytes,
    );
    let nonce_bytes = borsh::to_vec(&next_nonce).map_err(|e| StateError::Storage(e.to_string()))?;
    pending.put_cf(
        ColumnFamily::Meta,
        &vesting_nonce_key(&claim.beneficiary),
        &nonce_bytes,
    );
    let rec = borsh::to_vec(claim).map_err(|e| StateError::Storage(e.to_string()))?;
    pending.put_cf(ColumnFamily::Meta, &vesting_claim_key(&id), &rec);

    let created = match claim.asset {
        NativeAssetId::TLT => {
            let op = OutPoint {
                tx_id: id,
                index: 0,
            };
            let key = outpoint_key(&op);
            if store.get_cf(ColumnFamily::Utxo, &key)?.is_some() {
                return Err(StateError::DuplicateOutpoint(format!(
                    "{}:0",
                    op.tx_id.to_hex()
                )));
            }
            let out = TxOut {
                value: claim.amount,
                address: claim.beneficiary,
            };
            let bytes = borsh::to_vec(&out).map_err(|e| StateError::Storage(e.to_string()))?;
            pending.put_cf(ColumnFamily::Utxo, &key, &bytes);
            Some(op)
        }
        NativeAssetId::OVL | NativeAssetId::DRC => {
            credit_account_into(
                &mut pending,
                store,
                claim.asset,
                &claim.beneficiary,
                claim.amount,
            )?;
            None
        }
    };
    batch.append(pending);
    Ok(created)
}

pub fn load_protocol_treasuries(store: &StateStore) -> Result<Vec<TreasuryBalance>, StateError> {
    TreasuryId::ALL
        .iter()
        .copied()
        .map(|id| load_protocol_treasury(store, id))
        .collect()
}

pub fn governance_treasury_root(store: &StateStore) -> Result<Hash, StateError> {
    let policy = load_canonical_governance_policy(store)?;
    let treasuries = load_protocol_treasuries(store)?;
    let emergency = load_emergency_policy_hash(store)?;
    let controls = load_treasury_controls(store)?;
    let vesting = load_vesting_schedules(store)?;
    let vesting_progress = load_vesting_progress(store)?;
    Ok(Hash::hash_borsh(&(
        GOVERNANCE_TREASURY_ROOT_DOMAIN,
        policy,
        treasuries,
        emergency,
        controls,
        vesting,
        vesting_progress,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_policy_and_three_asset_isolated_treasuries_commit() {
        let store = StateStore::open_in_memory();
        let root_before = governance_treasury_root(&store).unwrap();
        let mut batch = WriteBatch::new();
        init_canonical_governance_into(&mut batch).unwrap();
        store.write_batch(batch).unwrap();

        let policy = load_canonical_governance_policy(&store).unwrap();
        assert_eq!(policy.authorization_root, authorization_policy_root());
        let treasuries = load_protocol_treasuries(&store).unwrap();
        assert_eq!(treasuries.len(), 3);
        for treasury in treasuries {
            assert_eq!(treasury.asset, treasury.treasury.asset());
            assert_eq!(treasury.balance, Amount::ZERO);
        }
        // Missing state deterministically means the same zero/default genesis state.
        assert_eq!(governance_treasury_root(&store).unwrap(), root_before);
    }

    #[test]
    fn treasury_root_changes_with_asset_correct_balance() {
        let store = StateStore::open_in_memory();
        let initial = governance_treasury_root(&store).unwrap();
        let mut batch = WriteBatch::new();
        put_treasury_into(
            &mut batch,
            &TreasuryBalance::new(
                TreasuryId::DrcCommunity,
                agora_types::NativeAssetId::DRC,
                Amount::from_base_units(7),
            )
            .unwrap(),
        )
        .unwrap();
        store.write_batch(batch).unwrap();
        assert_ne!(governance_treasury_root(&store).unwrap(), initial);
    }

    #[test]
    fn treasury_write_rejects_cross_asset_record() {
        let mut batch = WriteBatch::new();
        let corrupt = TreasuryBalance {
            treasury: TreasuryId::OvlBuilder,
            asset: agora_types::NativeAssetId::DRC,
            balance: Amount::from_base_units(1),
        };
        assert!(put_treasury_into(&mut batch, &corrupt).is_err());
        assert!(batch.is_empty());
    }

    #[test]
    fn disbursement_rejects_wrong_signer_root_and_insufficient_balance() {
        use agora_crypto::{sign_treasury_disbursement_bound, KeyPair};

        let store = StateStore::open_in_memory();
        let controller = KeyPair::from_secret_bytes(&[5; 32]).unwrap();
        let stranger = KeyPair::from_secret_bytes(&[6; 32]).unwrap();
        let mut batch = WriteBatch::new();
        init_canonical_governance_into(&mut batch).unwrap();
        put_treasury_into(
            &mut batch,
            &TreasuryBalance::new(
                TreasuryId::OvlBuilder,
                NativeAssetId::OVL,
                Amount::from_base_units(40),
            )
            .unwrap(),
        )
        .unwrap();
        put_treasury_controller_into(&mut batch, TreasuryId::OvlBuilder, controller.address())
            .unwrap();
        store.write_batch(batch).unwrap();

        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([7; 32]),
            data_availability_network_fingerprint: None,
        };
        let mut spend = TreasuryDisbursement::unsigned(
            TreasuryId::OvlBuilder,
            Address([9; 20]),
            Amount::from_base_units(10),
            Hash([3; 32]),
            authorization_policy_root(),
            0,
        );
        sign_treasury_disbursement_bound(&mut spend, &stranger, &auth.chain_id, &auth.genesis)
            .unwrap();
        let mut reject = WriteBatch::new();
        assert!(apply_treasury_disbursement_into(&mut reject, &store, &spend, &auth).is_err());

        sign_treasury_disbursement_bound(&mut spend, &controller, &auth.chain_id, &auth.genesis)
            .unwrap();
        spend.authorization_root = Hash([1; 32]);
        assert!(apply_treasury_disbursement_into(&mut reject, &store, &spend, &auth).is_err());

        spend.authorization_root = authorization_policy_root();
        spend.amount = Amount::from_base_units(41);
        sign_treasury_disbursement_bound(&mut spend, &controller, &auth.chain_id, &auth.genesis)
            .unwrap();
        assert!(apply_treasury_disbursement_into(&mut reject, &store, &spend, &auth).is_err());

        spend.amount = Amount::from_base_units(10);
        sign_treasury_disbursement_bound(&mut spend, &controller, &auth.chain_id, &auth.genesis)
            .unwrap();
        apply_treasury_disbursement_into(&mut reject, &store, &spend, &auth).unwrap();
        store.write_batch(reject).unwrap();
        assert_eq!(
            load_protocol_treasury(&store, TreasuryId::OvlBuilder)
                .unwrap()
                .balance,
            Amount::from_base_units(30)
        );
    }

    #[test]
    fn vesting_unlock_rejects_before_cliff_overclaim_and_wrong_signer() {
        use agora_crypto::{sign_vesting_unlock_bound, KeyPair};
        use agora_types::vesting_schedule_id;

        let store = StateStore::open_in_memory();
        let beneficiary = KeyPair::from_secret_bytes(&[8; 32]).unwrap();
        let stranger = KeyPair::from_secret_bytes(&[9; 32]).unwrap();
        let schedule = crate::block_zero::BlockZeroVesting {
            asset: NativeAssetId::OVL,
            address: beneficiary.address(),
            amount: 100,
            start_timestamp_ms: 10,
            cliff_timestamp_ms: 20,
            end_timestamp_ms: 30,
        };
        let schedule_id = vesting_schedule_id(
            schedule.asset,
            &schedule.address,
            schedule.amount,
            schedule.start_timestamp_ms,
            schedule.cliff_timestamp_ms,
            schedule.end_timestamp_ms,
        );
        let mut batch = WriteBatch::new();
        init_canonical_governance_into(&mut batch).unwrap();
        put_vesting_schedules_into(&mut batch, &[schedule]).unwrap();
        store.write_batch(batch).unwrap();

        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([7; 32]),
            data_availability_network_fingerprint: None,
        };
        let mut claim = VestingUnlock::unsigned(
            NativeAssetId::OVL,
            beneficiary.address(),
            schedule_id,
            Amount::from_base_units(50),
            0,
        );
        sign_vesting_unlock_bound(&mut claim, &stranger, &auth.chain_id, &auth.genesis).unwrap();
        let mut reject = WriteBatch::new();
        assert!(apply_vesting_unlock_into(&mut reject, &store, &claim, &auth, 20).is_err());

        sign_vesting_unlock_bound(&mut claim, &beneficiary, &auth.chain_id, &auth.genesis).unwrap();
        assert!(apply_vesting_unlock_into(&mut reject, &store, &claim, &auth, 19).is_err());

        claim.amount = Amount::from_base_units(51);
        sign_vesting_unlock_bound(&mut claim, &beneficiary, &auth.chain_id, &auth.genesis).unwrap();
        assert!(apply_vesting_unlock_into(&mut reject, &store, &claim, &auth, 20).is_err());

        claim.amount = Amount::from_base_units(50);
        sign_vesting_unlock_bound(&mut claim, &beneficiary, &auth.chain_id, &auth.genesis).unwrap();
        apply_vesting_unlock_into(&mut reject, &store, &claim, &auth, 20).unwrap();
        store.write_batch(reject).unwrap();
        assert_eq!(load_vesting_unlocked(&store, &schedule_id).unwrap(), 50);
        let root_after = governance_treasury_root(&store).unwrap();
        let empty = StateStore::open_in_memory();
        init_canonical_governance_into(&mut WriteBatch::new()).unwrap();
        assert_ne!(root_after, governance_treasury_root(&empty).unwrap());
    }
}
