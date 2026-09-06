use agora_consensus::EmissionSchedule;
use agora_types::{Address, Amount, Block, BlockHeader, Hash, Transaction, TxOut};

use crate::columns::{meta_keys, ColumnFamily};
use crate::{StateError, StateStore, WriteBatch};

/// Fixed monetary parameters written at genesis and never mutated ad hoc.
#[derive(Debug, Clone)]
pub struct SupplyCaps {
    /// Absolute max base units ever creatable (including premine + emission).
    pub max_supply: Amount,
    /// Premine / treasury allocation created in the genesis coinbase.
    pub premine: Amount,
    pub premine_address: Address,
}

impl Default for SupplyCaps {
    fn default() -> Self {
        Self {
            // 100 million AGORA with 8 decimals.
            max_supply: Amount::from_whole(100_000_000).expect("max supply"),
            // 10% premine reserved for treasury / ecosystem.
            premine: Amount::from_whole(10_000_000).expect("premine"),
            premine_address: Address::ZERO,
        }
    }
}

/// Builds Block 0 and persists caps + genesis hash into the state store.
#[derive(Debug, Clone)]
pub struct GenesisBuilder {
    pub supply: SupplyCaps,
    pub emission: EmissionSchedule,
    pub bits: u32,
    pub timestamp_ms: u64,
    /// When false, skip writing genesis payload into `cf_archival` (pruned node).
    pub write_archival: bool,
}

impl Default for GenesisBuilder {
    fn default() -> Self {
        Self {
            supply: SupplyCaps::default(),
            emission: EmissionSchedule::default(),
            bits: 0,
            timestamp_ms: 0,
            write_archival: true,
        }
    }
}

impl GenesisBuilder {
    pub fn with_premine_address(mut self, address: Address) -> Self {
        self.supply.premine_address = address;
        self
    }

    pub fn with_archival(mut self, write_archival: bool) -> Self {
        self.write_archival = write_archival;
        self
    }

    pub fn build_block(&self) -> Block {
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: self.supply.premine,
                address: self.supply.premine_address,
            }],
            0,
        );
        let tx_root = Block::compute_tx_root(std::slice::from_ref(&coinbase));
        Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: self.timestamp_ms,
                bits: self.bits,
                nonce: 0,
                tx_root,
            },
            transactions: vec![coinbase],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
        }
    }

    /// Prepare every genesis mutation without touching storage.
    ///
    /// Keeping the complete ignition in one batch is a prerequisite for a future
    /// Trident artifact loader: validation can finish before RocksDB receives an
    /// all-or-nothing commit.
    fn ignition_batch(&self) -> Result<(Hash, WriteBatch), StateError> {
        if self.supply.premine.as_base_units() > self.supply.max_supply.as_base_units() {
            return Err(StateError::Storage("premine exceeds max supply".into()));
        }

        let block = self.build_block();
        let genesis_hash = block.id();
        let block_bytes = borsh::to_vec(&block).map_err(|e| StateError::Storage(e.to_string()))?;
        let mut batch = WriteBatch::new();

        batch.put_cf(ColumnFamily::Hot, genesis_hash.as_bytes(), &block_bytes);
        if self.write_archival {
            batch.put_cf(
                ColumnFamily::Archival,
                genesis_hash.as_bytes(),
                &block_bytes,
            );
        }
        crate::headers::store_header_into(&mut batch, &genesis_hash, &block.header)?;
        crate::tx_index::index_block_transactions_into(&mut batch, &block);
        batch.put_cf(
            ColumnFamily::Meta,
            meta_keys::GENESIS_HASH,
            genesis_hash.as_bytes(),
        );
        batch.put_cf(
            ColumnFamily::Meta,
            meta_keys::MAX_SUPPLY,
            &self.supply.max_supply.as_base_units().to_le_bytes(),
        );
        batch.put_cf(
            ColumnFamily::Meta,
            meta_keys::PREMINE,
            &self.supply.premine.as_base_units().to_le_bytes(),
        );
        batch.put_cf(
            ColumnFamily::Meta,
            meta_keys::ISSUED_SUPPLY,
            &self.supply.premine.as_base_units().to_le_bytes(),
        );
        // Trident schema: per-asset supply keys + schema version (TLT issued = premine).
        crate::supply::put_max_supply_into(
            &mut batch,
            agora_types::NativeAssetId::TLT,
            self.supply.max_supply.as_base_units(),
        );
        crate::supply::put_issued_supply_into(
            &mut batch,
            agora_types::NativeAssetId::TLT,
            self.supply.premine.as_base_units(),
        );
        // Trident multi-asset caps, issued counters, staking reserves, schema.
        // Re-apply TLT issued=premine after ignite (ignite sets genesis_allocation).
        let policy = crate::monetary::TridentMonetaryPolicy::default();
        crate::supply::ignite_trident_supply(&mut batch, &policy)?;
        crate::governance_state::init_canonical_governance_into(&mut batch)?;
        crate::community_state::init_canonical_community_into(&mut batch)?;
        crate::supply::put_issued_supply_into(
            &mut batch,
            agora_types::NativeAssetId::TLT,
            self.supply.premine.as_base_units(),
        );
        crate::supply::put_max_supply_into(
            &mut batch,
            agora_types::NativeAssetId::TLT,
            self.supply.max_supply.as_base_units(),
        );

        let tips = vec![genesis_hash];
        let tips_bytes = borsh::to_vec(&tips).map_err(|e| StateError::Storage(e.to_string()))?;
        batch.put_cf(ColumnFamily::Meta, meta_keys::TIPS, &tips_bytes);
        batch.put_cf(
            ColumnFamily::Meta,
            meta_keys::VIRTUAL_TIP,
            genesis_hash.as_bytes(),
        );

        // Genesis UTXO: coinbase output 0 (baseline for virtual chain; no journal).
        let mut utxo_key = Vec::with_capacity(36);
        utxo_key.extend_from_slice(block.transactions[0].tx_id().as_bytes());
        utxo_key.extend_from_slice(&0u32.to_le_bytes());
        let utxo_val = borsh::to_vec(&block.transactions[0].outputs[0])
            .map_err(|e| StateError::Storage(e.to_string()))?;
        batch.put_cf(ColumnFamily::Utxo, &utxo_key, &utxo_val);

        let _ = &self.emission; // schedule is consulted by consensus; retained for API completeness.
        Ok((genesis_hash, batch))
    }

    /// Persist the complete genesis state in one atomic storage commit.
    pub fn ignite(&self, store: &StateStore) -> Result<Hash, StateError> {
        let (genesis_hash, batch) = self.ignition_batch()?;
        store.write_batch(batch)?;
        Ok(genesis_hash)
    }

    fn existing_genesis(store: &StateStore) -> Result<Option<Hash>, StateError> {
        let Some(bytes) = store.get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)? else {
            return Ok(None);
        };
        if bytes.len() != 32 {
            return Err(StateError::Storage(
                "malformed genesis hash in datadir; refusing to overwrite existing state".into(),
            ));
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Ok(Some(Hash(hash)))
    }

    fn require_empty_datadir(store: &StateStore) -> Result<(), StateError> {
        for cf in ColumnFamily::ALL {
            if !store.scan_prefix(cf, &[])?.is_empty() {
                return Err(StateError::Storage(format!(
                    "datadir contains {} state without a genesis identity; refusing initialization",
                    cf.name()
                )));
            }
        }
        Ok(())
    }

    /// Return the existing genesis hash from meta, or [`ignite`] a fresh chain.
    pub fn load_or_ignite(&self, store: &StateStore) -> Result<Hash, StateError> {
        if let Some(hash) = Self::existing_genesis(store)? {
            return Ok(hash);
        }
        Self::require_empty_datadir(store)?;
        self.ignite(store)
    }

    /// Like [`load_or_ignite`], but validate identity before any fresh write.
    pub fn load_or_ignite_checked(
        &self,
        store: &StateStore,
        expected: Option<Hash>,
    ) -> Result<Hash, StateError> {
        if let Some(hash) = Self::existing_genesis(store)? {
            if let Some(want) = expected {
                if hash != want {
                    return Err(StateError::Storage(format!(
                        "genesis hash mismatch: datadir {} expected {} (wipe AGORA_DATA or change AGORA_NETWORK)",
                        hash.to_hex(),
                        want.to_hex()
                    )));
                }
            }
            return Ok(hash);
        }

        Self::require_empty_datadir(store)?;
        let hash = self.build_block().id();
        if let Some(want) = expected {
            if hash != want {
                return Err(StateError::Storage(format!(
                    "genesis hash mismatch before initialization: computed {} expected {}",
                    hash.to_hex(),
                    want.to_hex()
                )));
            }
        }
        self.ignite(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_ignition_writes_caps_and_utxo() {
        let store = StateStore::open_in_memory();
        let premine = Address([7u8; 20]);
        let hash = GenesisBuilder::default()
            .with_premine_address(premine)
            .ignite(&store)
            .unwrap();

        let stored = store
            .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
            .unwrap()
            .unwrap();
        assert_eq!(stored.as_slice(), hash.as_bytes());

        let max = store
            .get_cf(ColumnFamily::Meta, meta_keys::MAX_SUPPLY)
            .unwrap()
            .unwrap();
        assert_eq!(
            u64::from_le_bytes(max.try_into().unwrap()),
            Amount::from_whole(100_000_000).unwrap().as_base_units()
        );

        let tips = store
            .get_cf(ColumnFamily::Meta, meta_keys::TIPS)
            .unwrap()
            .unwrap();
        let tips: Vec<Hash> = borsh::from_slice(&tips).unwrap();
        assert_eq!(tips, vec![hash]);
        assert!(store
            .get_cf(ColumnFamily::Archival, hash.as_bytes())
            .unwrap()
            .is_some());
    }

    #[test]
    fn genesis_writes_issued_supply_eq_premine() {
        let store = StateStore::open_in_memory();
        GenesisBuilder::default().ignite(&store).unwrap();
        let issued = store
            .get_cf(ColumnFamily::Meta, meta_keys::ISSUED_SUPPLY)
            .unwrap()
            .unwrap();
        let premine = store
            .get_cf(ColumnFamily::Meta, meta_keys::PREMINE)
            .unwrap()
            .unwrap();
        assert_eq!(issued, premine);
    }

    #[test]
    fn genesis_ignites_working_staking_reserves() {
        use crate::{
            load_schema_version, load_staking_reserve_remaining, DRC_WORKING_RESERVE_BASE,
            OVL_WORKING_RESERVE_BASE, SCHEMA_VERSION,
        };
        use agora_types::NativeAssetId;

        let store = StateStore::open_in_memory();
        GenesisBuilder::default().ignite(&store).unwrap();
        assert_eq!(load_schema_version(&store).unwrap(), SCHEMA_VERSION);
        assert_eq!(
            load_staking_reserve_remaining(&store, NativeAssetId::OVL).unwrap(),
            OVL_WORKING_RESERVE_BASE
        );
        assert_eq!(
            load_staking_reserve_remaining(&store, NativeAssetId::DRC).unwrap(),
            DRC_WORKING_RESERVE_BASE
        );
    }

    #[test]
    fn genesis_can_skip_archival() {
        let store = StateStore::open_in_memory();
        let hash = GenesisBuilder::default()
            .with_archival(false)
            .ignite(&store)
            .unwrap();
        assert!(store
            .get_cf(ColumnFamily::Hot, hash.as_bytes())
            .unwrap()
            .is_some());
        assert!(store
            .get_cf(ColumnFamily::Archival, hash.as_bytes())
            .unwrap()
            .is_none());
    }

    #[test]
    fn checked_identity_mismatch_leaves_fresh_store_empty() {
        let store = StateStore::open_in_memory();
        let error = GenesisBuilder::default()
            .load_or_ignite_checked(&store, Some(Hash([0x55; 32])))
            .unwrap_err();
        assert!(error.to_string().contains("before initialization"));
        for cf in ColumnFamily::ALL {
            assert!(store.scan_prefix(cf, &[]).unwrap().is_empty());
        }
    }

    #[test]
    fn malformed_existing_identity_is_never_overwritten() {
        let store = StateStore::open_in_memory();
        store
            .put_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH, b"broken")
            .unwrap();

        let error = GenesisBuilder::default()
            .load_or_ignite_checked(&store, None)
            .unwrap_err();
        assert!(error.to_string().contains("malformed genesis hash"));
        assert_eq!(
            store
                .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
                .unwrap(),
            Some(b"broken".to_vec())
        );
    }

    #[test]
    fn state_without_genesis_identity_is_never_reinitialized() {
        let store = StateStore::open_in_memory();
        store
            .put_cf(ColumnFamily::Utxo, b"existing", b"value")
            .unwrap();

        let error = GenesisBuilder::default()
            .load_or_ignite(&store)
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("state without a genesis identity"));
        assert_eq!(
            store.get_cf(ColumnFamily::Utxo, b"existing").unwrap(),
            Some(b"value".to_vec())
        );
        assert!(store
            .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
            .unwrap()
            .is_none());
    }
}
