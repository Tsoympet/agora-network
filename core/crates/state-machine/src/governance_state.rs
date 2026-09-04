//! Canonical Trident governance policy and asset-isolated protocol treasuries.
//!
//! This module commits immutable v1 authorization policy and treasury balances
//! into consensus state. The existing unsigned node-local civic RPC snapshot is
//! intentionally stored under a different key and excluded from this root.

use agora_governance::{
    authorization_for_class, hash_constitution_body, ProposalAuthorization, ProposalClass,
    CONSTITUTION_V1_BODY, CONSTITUTION_V1_ID,
};
use agora_types::{Amount, Hash, TreasuryBalance, TreasuryId};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const POLICY_KEY: &[u8] = b"governance/consensus/policy";
const TREASURY_PREFIX: &[u8] = b"governance/treasury/";
pub const CANONICAL_GOVERNANCE_VERSION: u32 = 1;

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
    Hash::hash_borsh(&(b"agora-governance-authorization-v1", policies))
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

fn put_treasury_into(
    batch: &mut WriteBatch,
    treasury: &TreasuryBalance,
) -> Result<(), StateError> {
    if treasury.treasury.asset() != treasury.asset {
        return Err(StateError::InvalidTx(
            "protocol treasury asset mismatch".into(),
        ));
    }
    let bytes = borsh::to_vec(treasury).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &treasury_key(treasury.treasury),
        &bytes,
    );
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
    let balance = TreasuryBalance::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if balance.treasury != treasury || balance.asset != treasury.asset() {
        return Err(StateError::Storage(
            "corrupt protocol treasury asset identity".into(),
        ));
    }
    Ok(balance)
}

pub fn load_protocol_treasuries(
    store: &StateStore,
) -> Result<Vec<TreasuryBalance>, StateError> {
    TreasuryId::ALL
        .iter()
        .copied()
        .map(|id| load_protocol_treasury(store, id))
        .collect()
}

pub fn governance_treasury_root(store: &StateStore) -> Result<Hash, StateError> {
    let policy = load_canonical_governance_policy(store)?;
    let treasuries = load_protocol_treasuries(store)?;
    Ok(Hash::hash_borsh(&(
        b"agora-governance-treasury-root-v1",
        policy,
        treasuries,
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
}
