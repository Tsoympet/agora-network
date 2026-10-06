//! Schema-22 OVL wei interpretation and the dev-gated execution world.
//!
//! Stored Agora OVL balances, stake weights, treasuries, and supply counters
//! remain `u64`. At schema 22 that integer is the exact quotient
//! `wei / 10^10`, not a second currency. Sub-quotient dust exists only in the
//! EVM world, under Ethereum addresses that are not aliases of Agora
//! SHA-256/Bech32 keys. Moving a quotient into an EVM account is not
//! implemented.

use agora_ovl_evm::OvlEvmWorld;
use agora_types::{Hash, OvlWei};

use crate::columns::{ColumnFamily, OVL_EVM_SCHEMA_VERSION};
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

/// Meta key for the borsh `OvlEvmWorld`. Distinct from `account/<asset>/<addr>`.
pub const OVL_EVM_WORLD_KEY: &[u8] = b"meta/ovl/evm/v1/world";

pub fn load_ovl_evm_world(store: &StateStore) -> Result<OvlEvmWorld, StateError> {
    match store.get_cf(ColumnFamily::Meta, OVL_EVM_WORLD_KEY)? {
        Some(bytes) => OvlEvmWorld::from_bytes(&bytes)
            .map_err(|err| StateError::Storage(format!("OVL EVM world: {err}"))),
        None => Ok(OvlEvmWorld::inactive()),
    }
}

pub fn put_ovl_evm_world_into(batch: &mut WriteBatch, world: &OvlEvmWorld) {
    batch.put_cf(ColumnFamily::Meta, OVL_EVM_WORLD_KEY, &world.to_bytes());
}

/// Domain-separated commitment of the execution subroot.
///
/// This is not an Ethereum MPT state root and it does not include
/// contract-verification notes.
pub fn ovl_evm_state_commitment(store: &StateStore) -> Result<Hash, StateError> {
    let world = load_ovl_evm_world(store)?;
    Ok(Hash::hash_borsh(&(
        b"agora-ovl-evm-commitment-v1",
        world.execution_subroot(),
    )))
}

pub fn ovl_evm_schema_active(schema: u32) -> bool {
    schema >= OVL_EVM_SCHEMA_VERSION
}

/// Interpret one stored OVL quotient as wei.
pub fn legacy_ovl_quotient_to_wei(quotient: u64) -> Option<OvlWei> {
    OvlWei::from_legacy_base(quotient)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supply::{put_burned_supply_into, put_issued_supply_into, put_max_supply_into};
    use agora_types::NativeAssetId;

    use crate::{put_schema_version_into, verify_supply_invariants, SCHEMA_VERSION};

    #[test]
    fn one_legacy_unit_is_ten_billion_wei_and_not_a_second_balance() {
        let wei = legacy_ovl_quotient_to_wei(1).unwrap();
        assert_eq!(wei.to_decimal(), "10000000000");
        assert_eq!(wei.exact_legacy_quotient(), Some(1));
    }

    #[test]
    fn schema_22_does_not_enforce_the_historical_ovl_cap() {
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        put_schema_version_into(&mut batch, SCHEMA_VERSION);
        for asset in NativeAssetId::ALL {
            put_max_supply_into(&mut batch, asset, 10);
            put_burned_supply_into(&mut batch, asset, 0);
        }
        put_issued_supply_into(&mut batch, NativeAssetId::OVL, 11);
        put_issued_supply_into(&mut batch, NativeAssetId::DRC, 10);
        put_issued_supply_into(&mut batch, NativeAssetId::TLT, 10);
        store.write_batch(batch).unwrap();
        verify_supply_invariants(&store).unwrap();

        let mut over_drc = WriteBatch::new();
        put_issued_supply_into(&mut over_drc, NativeAssetId::DRC, 11);
        store.write_batch(over_drc).unwrap();
        assert!(matches!(
            verify_supply_invariants(&store),
            Err(StateError::SupplyCapExceeded)
        ));
    }
}
