use agora_consensus::EmissionSchedule;
use agora_types::{Address, Amount, Block, BlockHeader, Hash, Transaction, TxOut};

use crate::columns::{meta_keys, ColumnFamily};
use crate::{StateError, StateStore};

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
        }
    }

    /// Persist genesis block bytes, tips, and supply caps.
    pub fn ignite(&self, store: &StateStore) -> Result<Hash, StateError> {
        if self.supply.premine.as_base_units() > self.supply.max_supply.as_base_units() {
            return Err(StateError::Storage(
                "premine exceeds max supply".into(),
            ));
        }

        let block = self.build_block();
        let genesis_hash = block.id();
        let block_bytes = borsh::to_vec(&block)
            .map_err(|e| StateError::Storage(e.to_string()))?;

        store.put_cf(ColumnFamily::Hot, genesis_hash.as_bytes(), &block_bytes)?;
        if self.write_archival {
            store.put_cf(ColumnFamily::Archival, genesis_hash.as_bytes(), &block_bytes)?;
        }
        crate::tx_index::index_block_transactions(store, &block)?;
        store.put_cf(
            ColumnFamily::Meta,
            meta_keys::GENESIS_HASH,
            genesis_hash.as_bytes(),
        )?;
        store.put_cf(
            ColumnFamily::Meta,
            meta_keys::MAX_SUPPLY,
            &self.supply.max_supply.as_base_units().to_le_bytes(),
        )?;
        store.put_cf(
            ColumnFamily::Meta,
            meta_keys::PREMINE,
            &self.supply.premine.as_base_units().to_le_bytes(),
        )?;

        let tips = vec![genesis_hash];
        let tips_bytes = borsh::to_vec(&tips).map_err(|e| StateError::Storage(e.to_string()))?;
        store.put_cf(ColumnFamily::Meta, meta_keys::TIPS, &tips_bytes)?;

        // Genesis UTXO: coinbase output 0.
        let mut utxo_key = Vec::with_capacity(36);
        utxo_key.extend_from_slice(block.transactions[0].tx_id().as_bytes());
        utxo_key.extend_from_slice(&0u32.to_le_bytes());
        let utxo_val = borsh::to_vec(&block.transactions[0].outputs[0])
            .map_err(|e| StateError::Storage(e.to_string()))?;
        store.put_cf(ColumnFamily::Utxo, &utxo_key, &utxo_val)?;

        let _ = &self.emission; // schedule is consulted by consensus; retained for API completeness.
        Ok(genesis_hash)
    }

    /// Return the existing genesis hash from meta, or [`ignite`] a fresh chain.
    pub fn load_or_ignite(&self, store: &StateStore) -> Result<Hash, StateError> {
        if let Some(bytes) = store.get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)? {
            if bytes.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                return Ok(Hash(arr));
            }
        }
        self.ignite(store)
    }

    /// Like [`load_or_ignite`], then reject datadirs whose genesis ≠ `expected`.
    pub fn load_or_ignite_checked(
        &self,
        store: &StateStore,
        expected: Option<Hash>,
    ) -> Result<Hash, StateError> {
        let hash = self.load_or_ignite(store)?;
        if let Some(want) = expected {
            if hash != want {
                return Err(StateError::Storage(format!(
                    "genesis hash mismatch: datadir {} expected {} (wipe AGORA_DATA or change AGORA_NETWORK)",
                    hash.to_hex(),
                    want.to_hex()
                )));
            }
        }
        Ok(hash)
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

        let tips = store.get_cf(ColumnFamily::Meta, meta_keys::TIPS).unwrap().unwrap();
        let tips: Vec<Hash> = borsh::from_slice(&tips).unwrap();
        assert_eq!(tips, vec![hash]);
        assert!(store
            .get_cf(ColumnFamily::Archival, hash.as_bytes())
            .unwrap()
            .is_some());
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
}
