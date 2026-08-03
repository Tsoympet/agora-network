//! Canonical network identity and genesis parameters.
//!
//! `dev` remains free-form for local experiments. `testnet` freezes Block 0
//! (premine, timestamp, bits, supply) so every peer agrees on the same root.

use agora_consensus::{DaaConfig, EmissionSchedule, GhostdagConfig, PowAlgorithm};
use agora_types::{Address, Amount, Hash};
use serde::{Deserialize, Serialize};

use crate::genesis::{GenesisBuilder, SupplyCaps};

/// Testnet / local DAA: 1s target, wide window, may start at bits=0.
pub fn daa_config_testnet() -> DaaConfig {
    DaaConfig {
        target_block_time_ms: 1_000,
        window_size: 90,
        max_adjustment_factor: 2.0,
        min_level: 0,
    }
}

/// Mainnet-oriented DAA floor (genesis bits still frozen separately).
pub fn daa_config_mainnet() -> DaaConfig {
    DaaConfig {
        target_block_time_ms: 1_000,
        window_size: 90,
        max_adjustment_factor: 2.0,
        min_level: 8,
    }
}

/// Default GHOSTDAG `k` for Agora networks.
pub const DEFAULT_GHOSTDAG_K: u32 = 18;

/// Well-known Agora network identifiers (`AGORA_NETWORK`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkId {
    /// Local / CI — premine and timestamp may vary per datadir.
    Dev,
    /// Shared public testnet with a frozen genesis artifact.
    Testnet,
    /// Reserved; genesis not frozen yet.
    Mainnet,
}

impl NetworkId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Testnet => "testnet",
            Self::Mainnet => "mainnet",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dev" | "development" | "local" => Some(Self::Dev),
            "testnet" | "test" => Some(Self::Testnet),
            "mainnet" | "main" => Some(Self::Mainnet),
            _ => None,
        }
    }
}

impl std::fmt::Display for NetworkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// BIP-39 `abandon…about` external(0) — local testnet / faucet premine.
pub const TESTNET_PREMINE_ADDRESS_HEX: &str = "ff9ec96f09eb154d038a552ecae59c50204ea9a9";

/// Frozen testnet genesis timestamp: 2026-08-03T00:00:00.000Z.
pub const TESTNET_GENESIS_TIMESTAMP_MS: u64 = 1_785_715_200_000;

/// Frozen testnet genesis `header.bits` (easy PoW for public test mining).
pub const TESTNET_GENESIS_BITS: u32 = 0;

/// Hex id of the frozen testnet Block 0 (`ChainParams::testnet().builder().build_block().id()`).
///
/// Changing supply / premine / timestamp / bits requires a network reset and a new constant.
pub const TESTNET_GENESIS_HASH_HEX: &str =
    "afe59232cd20a16bd56948044149d2b8013e63f3694c113074fef75ab0cb9b98";

/// Monetary + genesis + consensus policy parameters for one network.
#[derive(Debug, Clone)]
pub struct ChainParams {
    pub network: NetworkId,
    pub supply: SupplyCaps,
    pub emission: EmissionSchedule,
    pub bits: u32,
    pub timestamp_ms: u64,
    /// When set, boot must produce / load this genesis hash.
    pub expected_genesis: Option<Hash>,
    /// Work-weighted DAA policy (not part of the genesis hash).
    pub daa: DaaConfig,
    /// GHOSTDAG anticone bound `k`.
    pub ghostdag_k: u32,
    /// Canonical PoW algorithm for this network.
    pub pow_algorithm: PowAlgorithm,
}

impl ChainParams {
    /// Free-form local network (default node behavior).
    pub fn dev() -> Self {
        Self {
            network: NetworkId::Dev,
            supply: SupplyCaps::default(),
            emission: EmissionSchedule::default(),
            bits: 0,
            timestamp_ms: 0,
            expected_genesis: None,
            daa: daa_config_testnet(),
            ghostdag_k: DEFAULT_GHOSTDAG_K,
            pow_algorithm: PowAlgorithm::RandomX,
        }
    }

    /// Canonical Agora testnet — frozen Block 0.
    pub fn testnet() -> Self {
        let premine_address = Address::from_hex(TESTNET_PREMINE_ADDRESS_HEX)
            .expect("static testnet premine hex");
        let mut supply = SupplyCaps::default();
        supply.premine_address = premine_address;
        let expected = Hash::from_hex(TESTNET_GENESIS_HASH_HEX).expect("static testnet genesis hex");
        Self {
            network: NetworkId::Testnet,
            supply,
            emission: EmissionSchedule::default(),
            bits: TESTNET_GENESIS_BITS,
            timestamp_ms: TESTNET_GENESIS_TIMESTAMP_MS,
            expected_genesis: Some(expected),
            daa: daa_config_testnet(),
            ghostdag_k: DEFAULT_GHOSTDAG_K,
            pow_algorithm: PowAlgorithm::RandomX,
        }
    }

    /// Mainnet placeholder — refuses boot until a genesis is frozen.
    pub fn mainnet() -> Self {
        Self {
            network: NetworkId::Mainnet,
            supply: SupplyCaps::default(),
            emission: EmissionSchedule::default(),
            // Placeholder initial leading-zero requirement once genesis is frozen.
            bits: 16,
            timestamp_ms: 0,
            expected_genesis: None,
            daa: daa_config_mainnet(),
            ghostdag_k: DEFAULT_GHOSTDAG_K,
            pow_algorithm: PowAlgorithm::RandomX,
        }
    }

    pub fn ghostdag_config(&self) -> GhostdagConfig {
        GhostdagConfig {
            k: self.ghostdag_k,
        }
    }

    pub fn for_network(id: NetworkId) -> Result<Self, String> {
        match id {
            NetworkId::Dev => Ok(Self::dev()),
            NetworkId::Testnet => Ok(Self::testnet()),
            NetworkId::Mainnet => Err(
                "mainnet genesis is not frozen yet — use AGORA_NETWORK=testnet or dev".into(),
            ),
        }
    }

    pub fn with_premine_address(mut self, address: Address) -> Self {
        self.supply.premine_address = address;
        // Dev may retarget expected genesis via env; clear frozen expectation on override.
        if self.network == NetworkId::Dev {
            self.expected_genesis = None;
        }
        self
    }

    pub fn with_timestamp_ms(mut self, timestamp_ms: u64) -> Self {
        self.timestamp_ms = timestamp_ms;
        if self.network == NetworkId::Dev {
            self.expected_genesis = None;
        }
        self
    }

    pub fn with_bits(mut self, bits: u32) -> Self {
        self.bits = bits;
        if self.network == NetworkId::Dev {
            self.expected_genesis = None;
        }
        self
    }

    pub fn with_expected_genesis(mut self, hash: Hash) -> Self {
        self.expected_genesis = Some(hash);
        self
    }

    pub fn builder(&self) -> GenesisBuilder {
        GenesisBuilder {
            supply: self.supply.clone(),
            emission: self.emission.clone(),
            bits: self.bits,
            timestamp_ms: self.timestamp_ms,
            write_archival: true,
        }
    }

    /// Compute Block 0 id from these params (does not touch storage).
    pub fn compute_genesis_hash(&self) -> Hash {
        self.builder().build_block().id()
    }
}

/// Portable genesis document committed under `docs/genesis/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisArtifact {
    pub network: NetworkId,
    pub version: u32,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub max_supply: u64,
    pub premine: u64,
    pub premine_address: String,
    pub premine_address_hex: String,
    pub genesis_hash: String,
    pub emission_initial_reward: u64,
    pub emission_halving_interval: u64,
}

impl GenesisArtifact {
    pub fn from_params(params: &ChainParams) -> Self {
        let hash = params.compute_genesis_hash();
        Self {
            network: params.network,
            version: 1,
            timestamp_ms: params.timestamp_ms,
            bits: params.bits,
            max_supply: params.supply.max_supply.as_base_units(),
            premine: params.supply.premine.as_base_units(),
            premine_address: params.supply.premine_address.to_bech32(),
            premine_address_hex: params.supply.premine_address.to_hex(),
            genesis_hash: hash.to_hex(),
            emission_initial_reward: params.emission.initial_reward,
            emission_halving_interval: params.emission.halving_interval,
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }

    /// Rebuild [`ChainParams`] from a dumped artifact (hash must match recompute).
    pub fn to_params(&self) -> Result<ChainParams, String> {
        let address = Address::parse(&self.premine_address)
            .or_else(|| Address::from_hex(&self.premine_address_hex))
            .ok_or_else(|| "invalid premine address in genesis artifact".to_string())?;
        let expected = Hash::from_hex(&self.genesis_hash)
            .ok_or_else(|| "invalid genesis_hash in artifact".to_string())?;
        let max_supply = Amount::from_base_units(self.max_supply);
        let premine = Amount::from_base_units(self.premine);
        let base = match self.network {
            NetworkId::Mainnet => ChainParams::mainnet(),
            NetworkId::Testnet => ChainParams::testnet(),
            NetworkId::Dev => ChainParams::dev(),
        };
        let params = ChainParams {
            network: self.network,
            supply: SupplyCaps {
                max_supply,
                premine,
                premine_address: address,
            },
            emission: EmissionSchedule {
                initial_reward: self.emission_initial_reward,
                halving_interval: self.emission_halving_interval,
            },
            bits: self.bits,
            timestamp_ms: self.timestamp_ms,
            expected_genesis: Some(expected),
            daa: base.daa,
            ghostdag_k: base.ghostdag_k,
            pow_algorithm: base.pow_algorithm,
        };
        let computed = params.compute_genesis_hash();
        if computed != expected {
            return Err(format!(
                "genesis artifact hash mismatch: artifact {} recomputed {}",
                expected.to_hex(),
                computed.to_hex()
            ));
        }
        Ok(params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testnet_genesis_hash_is_frozen() {
        let params = ChainParams::testnet();
        let hash = params.compute_genesis_hash();
        // Print once when regenerating the constant after a deliberate freeze change.
        eprintln!("testnet genesis hash = {}", hash.to_hex());
        assert_eq!(hash.to_hex(), TESTNET_GENESIS_HASH_HEX);
        assert_eq!(params.expected_genesis, Some(hash));
        let artifact = GenesisArtifact::from_params(&params);
        assert_eq!(artifact.network, NetworkId::Testnet);
        let roundtrip = artifact.to_params().unwrap();
        assert_eq!(roundtrip.compute_genesis_hash(), hash);
    }

    #[test]
    fn network_id_parse() {
        assert_eq!(NetworkId::parse("TESTNET"), Some(NetworkId::Testnet));
        assert_eq!(NetworkId::parse("dev"), Some(NetworkId::Dev));
        assert!(ChainParams::for_network(NetworkId::Mainnet).is_err());
    }

    #[test]
    fn consensus_policy_locked_per_network() {
        let testnet = ChainParams::testnet();
        assert_eq!(testnet.bits, TESTNET_GENESIS_BITS);
        assert_eq!(testnet.daa.min_level, 0);
        assert_eq!(testnet.ghostdag_k, DEFAULT_GHOSTDAG_K);
        assert_eq!(testnet.pow_algorithm, PowAlgorithm::RandomX);

        let mainnet = ChainParams::mainnet();
        assert!(mainnet.daa.min_level >= 8);
        assert_eq!(mainnet.bits, 16);
        assert_eq!(mainnet.pow_algorithm, PowAlgorithm::RandomX);
    }
}
