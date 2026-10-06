//! Per-asset issued, burned, and net-supply accounting for Trident L1.

use agora_types::{Amount, Hash, NativeAssetId};

use crate::columns::{meta_keys, ColumnFamily, SCHEMA_VERSION};
use crate::monetary::{EmissionKind, TridentMonetaryPolicy, TLT_MAX_SUPPLY_BASE};
use crate::staking::init_staking_reserve_into;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

/// Schema that activates accepted-only DRC fee destruction.
pub const DRC_FEE_BURN_SCHEMA_VERSION: u32 = 20;

/// Canonical native-asset supply view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSupplyState {
    pub asset: NativeAssetId,
    pub maximum_supply: u64,
    pub issued_supply: u64,
    pub burned_supply: u64,
    pub net_supply: u64,
}

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

/// Meta key for one asset's lifetime burned supply (`u64` LE).
pub fn burned_supply_key(asset: NativeAssetId) -> Vec<u8> {
    let mut key = meta_keys::BURNED_SUPPLY_ASSET_PREFIX.to_vec();
    key.push(asset.wire_byte());
    key
}

pub fn load_burned_supply(store: &StateStore, asset: NativeAssetId) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &burned_supply_key(asset))? else {
        let schema = load_schema_version(store)?;
        if schema >= DRC_FEE_BURN_SCHEMA_VERSION {
            return Err(StateError::Storage(format!(
                "schema {schema} is missing the {} burned-supply counter",
                asset.ticker()
            )));
        }
        return Ok(0);
    };
    if bytes.len() != 8 {
        return Err(StateError::Storage(format!(
            "malformed {} burned-supply counter",
            asset.ticker()
        )));
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    Ok(u64::from_le_bytes(arr))
}

pub fn put_burned_supply_into(batch: &mut WriteBatch, asset: NativeAssetId, burned: u64) {
    batch.put_cf(
        ColumnFamily::Meta,
        &burned_supply_key(asset),
        &burned.to_le_bytes(),
    );
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
    batch.put_cf(
        ColumnFamily::Meta,
        &max_supply_key(asset),
        &max.to_le_bytes(),
    );
    if asset == NativeAssetId::TLT {
        batch.put_cf(
            ColumnFamily::Meta,
            meta_keys::MAX_SUPPLY,
            &max.to_le_bytes(),
        );
    }
}

pub fn load_native_supply_state(
    store: &StateStore,
    asset: NativeAssetId,
) -> Result<NativeSupplyState, StateError> {
    let issued_supply = load_issued_supply(store, asset)?;
    let burned_supply = load_burned_supply(store, asset)?;
    let net_supply = issued_supply
        .checked_sub(burned_supply)
        .ok_or(StateError::BurnedSupplyExceedsIssued)?;
    Ok(NativeSupplyState {
        asset,
        maximum_supply: load_max_supply(store, asset)?,
        issued_supply,
        burned_supply,
        net_supply,
    })
}

/// Destroy an accepted DRC transaction fee in the same batch as its state change.
pub fn burn_drc_fee_into(
    store: &StateStore,
    batch: &mut WriteBatch,
    fee: u64,
) -> Result<(), StateError> {
    let schema = load_schema_version(store)?;
    if schema != DRC_FEE_BURN_SCHEMA_VERSION {
        return Err(StateError::InvalidTx(format!(
            "DRC fee burning requires datadir schema 20, found {schema}"
        )));
    }
    if fee == 0 {
        return Ok(());
    }
    let issued = load_issued_supply(store, NativeAssetId::DRC)?;
    let burned = load_burned_supply(store, NativeAssetId::DRC)?;
    let next = burned
        .checked_add(fee)
        .ok_or_else(|| StateError::InvalidTx("DRC burned-supply overflow".into()))?;
    if next > issued {
        return Err(StateError::BurnedSupplyExceedsIssued);
    }
    put_burned_supply_into(batch, NativeAssetId::DRC, next);
    Ok(())
}

/// Root-commit max, issued, burned, and net supply for all native assets.
pub fn native_supply_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries = Vec::with_capacity(NativeAssetId::ALL.len());
    for asset in NativeAssetId::ALL {
        let state = load_native_supply_state(store, asset)?;
        entries.push((
            state.asset,
            state.maximum_supply,
            state.issued_supply,
            state.burned_supply,
            state.net_supply,
        ));
    }
    Ok(Hash::hash_borsh(&(b"agora-native-supply-root-v1", entries)))
}

/// Invariant: burned ≤ issued ≤ max for every native asset.
pub fn verify_supply_invariants(store: &StateStore) -> Result<(), StateError> {
    for asset in NativeAssetId::ALL {
        let state = load_native_supply_state(store, asset)?;
        if state.issued_supply > state.maximum_supply {
            return Err(StateError::SupplyCapExceeded);
        }
        let _ = Amount::from_base_units(state.net_supply);
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

/// Atomically activate fee-burn accounting on a schema-19 Experimental datadir.
///
/// Historical DRC reward-pool funds have mixed provenance, so migration starts
/// every lifetime counter at zero and never infers prior fees.
pub fn migrate_drc_fee_burn_schema(store: &StateStore) -> Result<(), StateError> {
    let version = load_schema_version(store)?;
    if version == DRC_FEE_BURN_SCHEMA_VERSION {
        for asset in NativeAssetId::ALL {
            if store
                .get_cf(ColumnFamily::Meta, &burned_supply_key(asset))?
                .is_none()
            {
                return Err(StateError::Storage(format!(
                    "schema 20 is missing the {} burned-supply counter",
                    asset.ticker()
                )));
            }
        }
        return verify_supply_invariants(store);
    }
    if version != DRC_FEE_BURN_SCHEMA_VERSION - 1 {
        return Err(StateError::Storage(format!(
            "DRC fee-burn migration requires schema 19, found {version}"
        )));
    }
    for asset in NativeAssetId::ALL {
        if store
            .get_cf(ColumnFamily::Meta, &burned_supply_key(asset))?
            .is_some()
        {
            return Err(StateError::Storage(format!(
                "schema 19 unexpectedly contains a {} burned-supply counter",
                asset.ticker()
            )));
        }
    }
    verify_supply_invariants(store)?;
    let mut batch = WriteBatch::new();
    for asset in NativeAssetId::ALL {
        put_burned_supply_into(&mut batch, asset, 0);
    }
    put_schema_version_into(&mut batch, DRC_FEE_BURN_SCHEMA_VERSION);
    store.write_batch(batch)
}

/// Write Trident monetary caps, issued/burned counters, and schema version.
pub fn ignite_trident_supply(
    batch: &mut WriteBatch,
    policy: &TridentMonetaryPolicy,
) -> Result<(), StateError> {
    policy.validate().map_err(StateError::InvalidTx)?;
    for asset in NativeAssetId::ALL {
        let p = policy.policy(asset);
        put_max_supply_into(batch, asset, p.max_supply);
        let issued = p.genesis_allocation.saturating_add(p.treasury_allocation);
        if issued > p.max_supply {
            return Err(StateError::SupplyCapExceeded);
        }
        put_issued_supply_into(batch, asset, issued);
        put_burned_supply_into(batch, asset, 0);
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

    #[test]
    fn drc_fee_burn_tracks_net_supply_and_rejects_overburn() {
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        put_schema_version_into(&mut batch, DRC_FEE_BURN_SCHEMA_VERSION);
        put_max_supply_into(&mut batch, NativeAssetId::DRC, 100);
        put_issued_supply_into(&mut batch, NativeAssetId::DRC, 10);
        put_burned_supply_into(&mut batch, NativeAssetId::DRC, 0);
        store.write_batch(batch).unwrap();

        let mut burn = WriteBatch::new();
        burn_drc_fee_into(&store, &mut burn, 3).unwrap();
        store.write_batch(burn).unwrap();
        assert_eq!(
            load_native_supply_state(&store, NativeAssetId::DRC).unwrap(),
            NativeSupplyState {
                asset: NativeAssetId::DRC,
                maximum_supply: 100,
                issued_supply: 10,
                burned_supply: 3,
                net_supply: 7,
            }
        );

        let mut overburn = WriteBatch::new();
        assert!(matches!(
            burn_drc_fee_into(&store, &mut overburn, 8),
            Err(StateError::BurnedSupplyExceedsIssued)
        ));
        assert_eq!(load_burned_supply(&store, NativeAssetId::DRC).unwrap(), 3);

        let mut counters = WriteBatch::new();
        put_issued_supply_into(&mut counters, NativeAssetId::DRC, u64::MAX);
        put_burned_supply_into(&mut counters, NativeAssetId::DRC, u64::MAX);
        store.write_batch(counters).unwrap();
        let mut overflow = WriteBatch::new();
        assert!(matches!(
            burn_drc_fee_into(&store, &mut overflow, 1),
            Err(StateError::InvalidTx(message)) if message.contains("overflow")
        ));
    }

    #[test]
    fn fee_burn_migration_starts_at_zero_without_pool_inference() {
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        put_schema_version_into(&mut batch, DRC_FEE_BURN_SCHEMA_VERSION - 1);
        for asset in NativeAssetId::ALL {
            put_max_supply_into(&mut batch, asset, 100);
            put_issued_supply_into(&mut batch, asset, 10);
        }
        store.write_batch(batch).unwrap();

        migrate_drc_fee_burn_schema(&store).unwrap();
        assert_eq!(
            load_schema_version(&store).unwrap(),
            DRC_FEE_BURN_SCHEMA_VERSION
        );
        for asset in NativeAssetId::ALL {
            assert_eq!(load_burned_supply(&store, asset).unwrap(), 0);
            assert_eq!(
                load_native_supply_state(&store, asset).unwrap().net_supply,
                10
            );
        }
        migrate_drc_fee_burn_schema(&store).unwrap();
    }

    #[test]
    fn malformed_and_excess_burn_counters_fail_closed() {
        let store = StateStore::open_in_memory();
        store
            .put_cf(
                ColumnFamily::Meta,
                &burned_supply_key(NativeAssetId::DRC),
                &[1, 2, 3],
            )
            .unwrap();
        assert!(load_burned_supply(&store, NativeAssetId::DRC).is_err());

        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        put_max_supply_into(&mut batch, NativeAssetId::DRC, 10);
        put_issued_supply_into(&mut batch, NativeAssetId::DRC, 2);
        put_burned_supply_into(&mut batch, NativeAssetId::DRC, 3);
        store.write_batch(batch).unwrap();
        assert!(matches!(
            verify_supply_invariants(&store),
            Err(StateError::BurnedSupplyExceedsIssued)
        ));
    }

    #[test]
    fn active_schema_requires_counters_and_exact_version() {
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        put_schema_version_into(&mut batch, DRC_FEE_BURN_SCHEMA_VERSION);
        store.write_batch(batch).unwrap();
        assert!(load_burned_supply(&store, NativeAssetId::DRC).is_err());

        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        put_schema_version_into(&mut batch, DRC_FEE_BURN_SCHEMA_VERSION + 1);
        put_issued_supply_into(&mut batch, NativeAssetId::DRC, 1);
        put_burned_supply_into(&mut batch, NativeAssetId::DRC, 0);
        store.write_batch(batch).unwrap();
        let mut burn = WriteBatch::new();
        assert!(matches!(
            burn_drc_fee_into(&store, &mut burn, 0),
            Err(StateError::InvalidTx(message)) if message.contains("found 21")
        ));
    }
}
