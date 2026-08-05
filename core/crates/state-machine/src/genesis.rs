use agora_consensus::{
    accept_blue_blocks, BlueBlockInput, EmissionSchedule, GhostdagConfig, MemoryUtxoView,
    OrderedBlock,
};
use agora_types::{
    Address, Amount, Block, BlockHeader, Hash, NetworkFingerprint, Transaction, TxOut,
};

use crate::columns::{meta_keys, ColumnFamily};
use crate::journal::acceptance_store_ops;
use crate::store::StoreOp;
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
    pub ghostdag: GhostdagConfig,
    pub network_name: String,
    pub network_id: u32,
    pub bits: u32,
    pub timestamp_ms: u64,
}

impl Default for GenesisBuilder {
    fn default() -> Self {
        Self {
            supply: SupplyCaps::default(),
            emission: EmissionSchedule::default(),
            ghostdag: GhostdagConfig::default(),
            network_name: "agora-devnet".into(),
            network_id: 1,
            bits: 0,
            timestamp_ms: 0,
        }
    }
}

impl GenesisBuilder {
    pub fn with_premine_address(mut self, address: Address) -> Self {
        self.supply.premine_address = address;
        self
    }

    pub fn with_network(mut self, name: impl Into<String>, network_id: u32) -> Self {
        self.network_name = name.into();
        self.network_id = network_id;
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

    /// Build the full network fingerprint for a genesis block hash.
    pub fn fingerprint_for(&self, genesis_hash: Hash) -> NetworkFingerprint {
        NetworkFingerprint {
            network_name: self.network_name.clone(),
            network_id: self.network_id,
            genesis_hash,
            ghostdag_k: self.ghostdag.k,
            max_supply: self.supply.max_supply.as_base_units(),
            premine: self.supply.premine.as_base_units(),
            initial_reward: self.emission.initial_reward,
            halving_interval: self.emission.halving_interval,
        }
    }

    /// Persist genesis block, supply caps, network fingerprint, and acceptance state
    /// in a **single atomic write batch**.
    ///
    /// Crash safety: either the datadir is fully bound (fingerprint + premine UTXO +
    /// acceptance bitmap) or it is unchanged — never half-initialized.
    pub fn ignite(&self, store: &StateStore) -> Result<(Hash, NetworkFingerprint), StateError> {
        if self.supply.premine.as_base_units() > self.supply.max_supply.as_base_units() {
            return Err(StateError::Storage("premine exceeds max supply".into()));
        }

        // Datadir must not already belong to another network.
        if let Some(existing) = crate::journal::load_network_fingerprint(store)? {
            return Err(StateError::FingerprintMismatch(format!(
                "datadir already bound to fingerprint {}",
                existing.digest_hex()
            )));
        }

        let block = self.build_block();
        let genesis_hash = block.id();
        let fingerprint = self.fingerprint_for(genesis_hash);
        let block_bytes = borsh::to_vec(&block).map_err(|e| StateError::Storage(e.to_string()))?;

        // Run acceptance against an empty in-memory UTXO view (not the store),
        // so we can commit meta + fingerprint + acceptance/UTXO in one batch.
        let empty = MemoryUtxoView::new();
        let acceptance = accept_blue_blocks(
            &[BlueBlockInput {
                ordered: OrderedBlock {
                    hash: genesis_hash,
                    blue_score: 1,
                    is_blue: true,
                },
                block,
                subsidy: self.supply.premine,
            }],
            &empty,
            &fingerprint,
        )
        .map_err(|e| StateError::Storage(e.to_string()))?;

        if !acceptance.blocks[0].bitmap.is_accepted(0) {
            return Err(StateError::Storage(
                "genesis coinbase was not accepted".into(),
            ));
        }

        let fp_bytes =
            borsh::to_vec(&fingerprint).map_err(|e| StateError::Storage(e.to_string()))?;
        let tips_bytes =
            borsh::to_vec(&vec![genesis_hash]).map_err(|e| StateError::Storage(e.to_string()))?;

        let mut ops = vec![
            StoreOp::Put {
                cf: ColumnFamily::Archival,
                key: genesis_hash.as_bytes().to_vec(),
                value: block_bytes.clone(),
            },
            StoreOp::Put {
                cf: ColumnFamily::Hot,
                key: genesis_hash.as_bytes().to_vec(),
                value: block_bytes,
            },
            StoreOp::Put {
                cf: ColumnFamily::Meta,
                key: meta_keys::GENESIS_HASH.to_vec(),
                value: genesis_hash.as_bytes().to_vec(),
            },
            StoreOp::Put {
                cf: ColumnFamily::Meta,
                key: meta_keys::MAX_SUPPLY.to_vec(),
                value: self
                    .supply
                    .max_supply
                    .as_base_units()
                    .to_le_bytes()
                    .to_vec(),
            },
            StoreOp::Put {
                cf: ColumnFamily::Meta,
                key: meta_keys::PREMINE.to_vec(),
                value: self.supply.premine.as_base_units().to_le_bytes().to_vec(),
            },
            StoreOp::Put {
                cf: ColumnFamily::Meta,
                key: meta_keys::TIPS.to_vec(),
                value: tips_bytes,
            },
            StoreOp::Put {
                cf: ColumnFamily::Meta,
                key: meta_keys::NETWORK_FINGERPRINT.to_vec(),
                value: fp_bytes,
            },
        ];
        ops.extend(acceptance_store_ops(&acceptance)?);

        store.write_batch(ops)?;
        Ok((genesis_hash, fingerprint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{assert_datadir_fingerprint, load_acceptance_bitmap, tx_confirmation};

    #[test]
    fn genesis_ignition_writes_caps_utxo_and_acceptance() {
        let store = StateStore::open("/tmp/agora-genesis-accept-test").unwrap();
        let premine = Address([7u8; 20]);
        let (hash, fp) = GenesisBuilder::default()
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

        assert_datadir_fingerprint(&store, &fp).unwrap();

        let bitmap = load_acceptance_bitmap(&store, &hash).unwrap().unwrap();
        assert!(bitmap.is_accepted(0));

        let block = GenesisBuilder::default()
            .with_premine_address(premine)
            .build_block();
        let tx_id = block.transactions[0].tx_id();
        let conf = tx_confirmation(&store, &tx_id, 5).unwrap();
        assert_eq!(conf.confirmations, 4);
        assert!(matches!(
            conf.status,
            agora_types::TxAcceptanceStatus::Accepted
        ));
    }

    #[test]
    fn datadir_rejects_second_ignition() {
        let store = StateStore::open("/tmp/agora-genesis-fp-twice").unwrap();
        GenesisBuilder::default().ignite(&store).unwrap();
        let err = GenesisBuilder::default().ignite(&store).unwrap_err();
        assert!(matches!(err, StateError::FingerprintMismatch(_)));
    }
}
