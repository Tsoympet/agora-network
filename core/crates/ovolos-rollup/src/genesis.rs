//! Ovolos (L2) genesis artifact — freezes native OVL PoW money + rollup boot params.
//!
//! OVL is **native money on L2**, sealed by layer PoW (`sha256_leading_zero`).
//! Separate from L1 Talanton genesis. Does not mint L1 UTXOs.

use std::path::Path;

use agora_types::{Address, Amount, Hash};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::ovl::{OvlLedger, DEFAULT_GAS_PER_TX, OVL_MAX_SUPPLY_BASE};
use crate::pow::{OvlEmission, OVOLOS_POW_ALGORITHM};
use crate::rollup::RollupConfig;
use crate::RollupError;

/// Default L2 PoW difficulty (leading-zero bits on SHA-256).
pub const DEFAULT_OVL_POW_BITS: u32 = 8;

/// Default emission: 50 OVL per block, halvings every 210_000 heights.
pub const DEFAULT_OVL_INITIAL_REWARD: u64 = 5_000_000_000;
pub const DEFAULT_OVL_HALVING_INTERVAL: u64 = 210_000;

/// One premine allocation at L2 ignite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct OvlPremine {
    pub address_hex: String,
    pub amount: u64,
}

/// Canonical fields hashed into [`OvolosGenesis::compute_hash`].
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
struct OvolosGenesisBody {
    network: String,
    version: u32,
    chain_id: String,
    parent_l1_network: String,
    parent_l1_genesis_hash: String,
    timestamp_ms: u64,
    max_supply: u64,
    decimals: u8,
    gas_per_tx: u64,
    challenge_window_ms: u64,
    genesis_state_root: String,
    premine: Vec<(String, u64)>,
    native: bool,
    pow_algorithm: String,
    pow_bits: u32,
    initial_block_reward: u64,
    halving_interval: u64,
}

/// Frozen Ovolos L2 genesis document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvolosGenesis {
    pub layer: String,
    pub mark: String,
    pub network: String,
    pub version: u32,
    pub chain_name: String,
    pub chain_id: String,
    pub parent_l1_network: String,
    pub parent_l1_genesis_hash: String,
    pub timestamp_ms: u64,
    pub max_supply: u64,
    pub decimals: u8,
    pub gas_per_tx: u64,
    pub challenge_window_ms: u64,
    /// Hex state root at sequence 0 (usually 32 zero bytes).
    pub genesis_state_root: String,
    pub premine: Vec<OvlPremine>,
    /// OVL is native money on this layer (not an L1 UTXO asset).
    pub native: bool,
    pub pow_algorithm: String,
    pub pow_bits: u32,
    pub initial_block_reward: u64,
    pub halving_interval: u64,
    /// Hex of [`Self::compute_hash`]; verified on load when non-empty / not `TBD`.
    pub genesis_hash: String,
}

impl Default for OvolosGenesis {
    fn default() -> Self {
        Self::testnet()
    }
}

impl OvolosGenesis {
    /// Embedded testnet defaults matching `docs/genesis/ovolos.testnet.genesis.json`.
    pub fn testnet() -> Self {
        Self {
            layer: "L2".into(),
            mark: "OVL".into(),
            network: "testnet".into(),
            version: 2,
            chain_name: "Ovolos Rollup".into(),
            chain_id: "agora-ovolos-testnet-1".into(),
            parent_l1_network: "testnet".into(),
            parent_l1_genesis_hash:
                "afe59232cd20a16bd56948044149d2b8013e63f3694c113074fef75ab0cb9b98".into(),
            timestamp_ms: 1785715200000,
            max_supply: OVL_MAX_SUPPLY_BASE,
            decimals: 8,
            gas_per_tx: DEFAULT_GAS_PER_TX,
            challenge_window_ms: 7 * 24 * 60 * 60 * 1000,
            genesis_state_root: Hash::ZERO.to_hex(),
            premine: vec![OvlPremine {
                // Same treasury address bytes as L1 testnet premine (HRP differs by layer UX).
                address_hex: "ff9ec96f09eb154d038a552ecae59c50204ea9a9".into(),
                // 10% of 21B OVL @ 8 decimals
                amount: 210_000_000_000_000_000,
            }],
            native: true,
            pow_algorithm: OVOLOS_POW_ALGORITHM.into(),
            pow_bits: DEFAULT_OVL_POW_BITS,
            initial_block_reward: DEFAULT_OVL_INITIAL_REWARD,
            halving_interval: DEFAULT_OVL_HALVING_INTERVAL,
            genesis_hash: String::new(),
        }
        .with_computed_hash()
    }

    pub fn mainnet_draft() -> Self {
        Self {
            layer: "L2".into(),
            mark: "OVL".into(),
            network: "mainnet".into(),
            version: 2,
            chain_name: "Ovolos Rollup".into(),
            chain_id: "agora-ovolos-mainnet-1".into(),
            parent_l1_network: "mainnet".into(),
            parent_l1_genesis_hash: "TBD".into(),
            timestamp_ms: 0,
            max_supply: OVL_MAX_SUPPLY_BASE,
            decimals: 8,
            gas_per_tx: DEFAULT_GAS_PER_TX,
            challenge_window_ms: 7 * 24 * 60 * 60 * 1000,
            genesis_state_root: Hash::ZERO.to_hex(),
            premine: vec![],
            native: true,
            pow_algorithm: OVOLOS_POW_ALGORITHM.into(),
            pow_bits: DEFAULT_OVL_POW_BITS,
            initial_block_reward: DEFAULT_OVL_INITIAL_REWARD,
            halving_interval: DEFAULT_OVL_HALVING_INTERVAL,
            genesis_hash: "TBD".into(),
        }
    }

    fn body(&self) -> OvolosGenesisBody {
        OvolosGenesisBody {
            network: self.network.clone(),
            version: self.version,
            chain_id: self.chain_id.clone(),
            parent_l1_network: self.parent_l1_network.clone(),
            parent_l1_genesis_hash: self.parent_l1_genesis_hash.clone(),
            timestamp_ms: self.timestamp_ms,
            max_supply: self.max_supply,
            decimals: self.decimals,
            gas_per_tx: self.gas_per_tx,
            challenge_window_ms: self.challenge_window_ms,
            genesis_state_root: self.genesis_state_root.clone(),
            premine: self
                .premine
                .iter()
                .map(|p| (p.address_hex.clone(), p.amount))
                .collect(),
            native: self.native,
            pow_algorithm: self.pow_algorithm.clone(),
            pow_bits: self.pow_bits,
            initial_block_reward: self.initial_block_reward,
            halving_interval: self.halving_interval,
        }
    }

    pub fn compute_hash(&self) -> Hash {
        Hash::hash_borsh(&self.body())
    }

    pub fn with_computed_hash(mut self) -> Self {
        self.genesis_hash = self.compute_hash().to_hex();
        self
    }

    pub fn from_json(json: &str) -> Result<Self, RollupError> {
        let g: Self = serde_json::from_str(json)
            .map_err(|e| RollupError::Execution(format!("ovolos genesis json: {e}")))?;
        g.validate()?;
        Ok(g)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, RollupError> {
        let raw = std::fs::read_to_string(path.as_ref())
            .map_err(|e| RollupError::Execution(format!("read ovolos genesis: {e}")))?;
        Self::from_json(&raw)
    }

    pub fn validate(&self) -> Result<(), RollupError> {
        if self.layer != "L2" || self.mark != "OVL" {
            return Err(RollupError::Execution(
                "ovolos genesis must be layer=L2 mark=OVL".into(),
            ));
        }
        if !self.native {
            return Err(RollupError::Execution(
                "ovolos genesis must set native=true (OVL is L2 native money)".into(),
            ));
        }
        if self.pow_algorithm != OVOLOS_POW_ALGORITHM {
            return Err(RollupError::Execution(format!(
                "ovolos pow_algorithm must be {OVOLOS_POW_ALGORITHM}"
            )));
        }
        if self.max_supply == 0 {
            return Err(RollupError::Execution(
                "ovolos max_supply must be > 0".into(),
            ));
        }
        let premine_sum: u64 = self.premine.iter().map(|p| p.amount).sum();
        if premine_sum > self.max_supply {
            return Err(RollupError::Execution(
                "ovolos premine exceeds max_supply".into(),
            ));
        }
        let expected = self.compute_hash().to_hex();
        if !self.genesis_hash.is_empty()
            && self.genesis_hash != "TBD"
            && self.genesis_hash != expected
        {
            return Err(RollupError::Execution(format!(
                "ovolos genesis_hash mismatch: file {} computed {}",
                self.genesis_hash, expected
            )));
        }
        Ok(())
    }

    pub fn genesis_state_root_hash(&self) -> Result<Hash, RollupError> {
        Hash::from_hex(&self.genesis_state_root)
            .ok_or_else(|| RollupError::Execution("invalid genesis_state_root".into()))
    }

    pub fn rollup_config(&self, gas_payer: Option<Address>) -> RollupConfig {
        RollupConfig {
            challenge_window_ms: self.challenge_window_ms,
            gas_payer,
            sequencer_min_bond: crate::sequencer::DEFAULT_SEQUENCER_MIN_BOND,
        }
    }

    pub fn emission(&self) -> OvlEmission {
        OvlEmission {
            initial_reward: self.initial_block_reward,
            halving_interval: self.halving_interval,
        }
    }

    /// Build ledger and apply premine allocations.
    pub fn ignite_ledger(&self) -> Result<OvlLedger, RollupError> {
        let mut ledger = OvlLedger::new(self.max_supply, self.gas_per_tx);
        for p in &self.premine {
            let addr = Address::from_hex(&p.address_hex)
                .ok_or_else(|| RollupError::Execution("bad premine address_hex".into()))?;
            ledger.mint(addr, Amount::from_base_units(p.amount))?;
        }
        Ok(ledger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testnet_genesis_stable_hash() {
        let g = OvolosGenesis::testnet();
        assert_eq!(g.mark, "OVL");
        assert_eq!(g.layer, "L2");
        assert!(g.native);
        assert_eq!(g.pow_algorithm, OVOLOS_POW_ALGORITHM);
        assert_eq!(g.version, 2);
        assert!(!g.genesis_hash.is_empty());
        g.validate().unwrap();
        let ledger = g.ignite_ledger().unwrap();
        assert_eq!(ledger.minted(), 210_000_000_000_000_000);
        assert_eq!(g.compute_hash().to_hex(), g.genesis_hash);
    }

    #[test]
    fn docs_testnet_file_matches_embedded() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/genesis/ovolos.testnet.genesis.json");
        if !path.exists() {
            return;
        }
        let file = OvolosGenesis::from_path(&path).unwrap();
        let embedded = OvolosGenesis::testnet();
        assert_eq!(file.genesis_hash, embedded.genesis_hash);
        assert_eq!(file.max_supply, embedded.max_supply);
        assert_eq!(file.premine, embedded.premine);
        assert!(file.native);
        assert_eq!(file.pow_algorithm, OVOLOS_POW_ALGORITHM);
    }
}
