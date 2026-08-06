//! Per-asset issued-supply accounting for Trident L1.

use agora_types::{Amount, NativeAssetId};

use crate::columns::{meta_keys, ColumnFamily, SCHEMA_VERSION};
use crate::monetary::{EmissionKind, TridentMonetaryPolicy, TLT_MAX_SUPPLY_BASE};
use crate::staking::init_staking_reserve_into;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

/// Meta key for one asset's issued supply (`u64` LE).
pub fn issued_supply_key(asset: NativeAssetId) -> Vec<u8> {
    let mut key = meta_keys::ISSUED_SUPPLY_ASSET_PREFIX.to_vec();
    key.push(asset.wire_byte());
    key
}

pub fn load_issued_supply(store: &StateStore, asset: NativeAssetId) -> Result<u64, StateError> {
    if let Some(bytes) = store.get_cf(ColumnFamily::Meta, &issued_supply_key(asset))? {
        if bytes.len() == 8 {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&bytes);
            return Ok(u64::from_le_bytes(arr));
        }
    }
    // Legacy single-key path for TLT (pre-Trident).
    if asset == NativeAssetId::TLT {
        if let Some(bytes) = store.get_cf(ColumnFamily::Meta, meta_keys::ISSUED_SUPPLY)? {
            if bytes.len() == 8 {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&bytes);
                return Ok(u64::from_le_bytes(arr));
            }
        }
    }
    Ok(0)
}

pub fn put_issued_supply_into(batch: &mut WriteBatch, asset: NativeAssetId, issued: u64) {
    batch.put_cf(
        ColumnFamily::Meta,
        &issued_supply_key(asset),
        &issued.to_le_bytes(),
    );
    // Keep legacy TLT key mirrored for existing node readers during transition.
    if asset == NativeAssetId::TLT {
        batch.put_cf(
            ColumnFamily::Meta,
            meta_keys::ISSUED_SUPPLY,
            &issued.to_le_bytes(),
        );
    }
}

pub fn load_max_supply(store: &StateStore, asset: NativeAssetId) -> Result<u64, StateError> {
    match asset {
        NativeAssetId::TLT => {
            if let Some(bytes) = store.get_cf(ColumnFamily::Meta, meta_keys::MAX_SUPPLY)? {
                if bytes.len() == 8 {
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(&bytes);
                    return Ok(u64::from_le_bytes(arr));
                }
            }
            Ok(TLT_MAX_SUPPLY_BASE)
        }
        NativeAssetId::OVL | NativeAssetId::DRC => {
            let key = max_supply_key(asset);
            if let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? {
                if bytes.len() == 8 {
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(&bytes);
                    return Ok(u64::from_le_bytes(arr));
                }
            }
            Ok(TridentMonetaryPolicy::default().policy(asset).max_supply)
        }
    }
}

pub fn max_supply_key(asset: NativeAssetId) -> Vec<u8> {
    let mut key = b"meta/max_supply/".to_vec();
    key.push(asset.wire_byte());
    key
}

pub fn put_max_supply_into(batch: &mut WriteBatch, asset: NativeAssetId, max: u64) {
    batch.put_cf(ColumnFamily::Meta, &max_supply_key(asset), &max.to_le_bytes());
    if asset == NativeAssetId::TLT {
        batch.put_cf(
            ColumnFamily::Meta,
            meta_keys::MAX_SUPPLY,
            &max.to_le_bytes(),
        );
    }
}

/// Invariant: issued ≤ max for every native asset.
pub fn verify_supply_invariants(store: &StateStore) -> Result<(), StateError> {
    for asset in NativeAssetId::ALL {
        let issued = load_issued_supply(store, asset)?;
        let max = load_max_supply(store, asset)?;
        if issued > max {
            return Err(StateError::SupplyCapExceeded);
        }
        let _ = Amount::from_base_units(issued);
    }
    Ok(())
}

pub fn put_schema_version_into(batch: &mut WriteBatch, version: u32) {
    batch.put_cf(
        ColumnFamily::Meta,
        meta_keys::SCHEMA_VERSION,
        &version.to_le_bytes(),
    );
}

pub fn load_schema_version(store: &StateStore) -> Result<u32, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, meta_keys::SCHEMA_VERSION)? else {
        return Ok(1);
    };
    if bytes.len() != 4 {
        return Ok(1);
    }
    let mut arr = [0u8; 4];
    arr.copy_from_slice(&bytes);
    Ok(u32::from_le_bytes(arr))
}

/// Write Trident monetary caps + zero/non-zero issued counters and schema version.
pub fn ignite_trident_supply(
    batch: &mut WriteBatch,
    policy: &TridentMonetaryPolicy,
) -> Result<(), StateError> {
    policy
        .validate()
        .map_err(|e| StateError::InvalidTx(e))?;
    for asset in NativeAssetId::ALL {
        let p = policy.policy(asset);
        put_max_supply_into(batch, asset, p.max_supply);
        let issued = p.genesis_allocation.saturating_add(p.treasury_allocation);
        if issued > p.max_supply {
            return Err(StateError::SupplyCapExceeded);
        }
        put_issued_supply_into(batch, asset, issued);
        if let EmissionKind::StakingReserve { reserve_base_units } = p.emission {
            init_staking_reserve_into(batch, asset, reserve_base_units)?;
        }
    }
    put_schema_version_into(batch, SCHEMA_VERSION);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StateStore;

    #[test]
    fn issued_within_cap_and_legacy_tlt_mirror() {
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        put_issued_supply_into(&mut batch, NativeAssetId::TLT, 42);
        put_max_supply_into(&mut batch, NativeAssetId::TLT, 100);
        store.write_batch(batch).unwrap();
        assert_eq!(load_issued_supply(&store, NativeAssetId::TLT).unwrap(), 42);
        // Legacy key mirrored.
        let legacy = store
            .get_cf(ColumnFamily::Meta, meta_keys::ISSUED_SUPPLY)
            .unwrap()
            .unwrap();
        assert_eq!(u64::from_le_bytes(legacy.try_into().unwrap()), 42);
        verify_supply_invariants(&store).unwrap();
    }
}
