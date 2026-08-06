//! Canonical network identity and genesis parameters.
//!
//! `dev` remains free-form for local experiments. `testnet` freezes Block 0
//! (premine, timestamp, bits, supply) so every peer agrees on the same root.

use agora_consensus::{DaaConfig, EmissionSchedule, GhostdagConfig, PowAlgorithm};
use agora_crypto::AGORA_COIN_TYPE;
use agora_types::{
    Address, Amount, Hash, ADDRESS_HRP_DEV, ADDRESS_HRP_MAINNET, ADDRESS_HRP_TESTNET,
};
use serde::{Deserialize, Serialize};

use crate::genesis::{GenesisBuilder, SupplyCaps};
use crate::marks::{default_token_marks, TokenMark};

/// Public testnet DAA: 1s target, wide window.
///
/// Genesis `header.bits` stays `0` (frozen hash), but `min_level` floors
/// post-genesis templates / retarget so public peers are not free-mined forever.
pub fn daa_config_testnet() -> DaaConfig {
    DaaConfig {
        target_block_time_ms: 1_000,
        window_size: 90,
        max_adjustment_bits: 1,
        min_level: 8,
        max_level: 128,
    }
}

/// Mainnet-oriented DAA floor (genesis bits still frozen separately).
pub fn daa_config_mainnet() -> DaaConfig {
    DaaConfig {
        target_block_time_ms: 1_000,
        window_size: 90,
        max_adjustment_bits: 1,
        min_level: 8,
        max_level: 128,
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

    /// Bech32m HRP for this network (`agora` / `agoratest` / `agoradev`).
    pub fn address_hrp(self) -> &'static str {
        match self {
            Self::Mainnet => ADDRESS_HRP_MAINNET,
            Self::Testnet => ADDRESS_HRP_TESTNET,
            Self::Dev => ADDRESS_HRP_DEV,
        }
    }

    /// BIP-44 coin type (provisional SLIP-0044 `8888` until registered).
    pub fn coin_type(self) -> u32 {
        let _ = self;
        AGORA_COIN_TYPE
    }

    /// Stable chain id string for wallets / explorers.
    pub fn chain_id(self) -> &'static str {
        match self {
            Self::Mainnet => "agora-mainnet-1",
            Self::Testnet => "agora-testnet-1",
            Self::Dev => "agora-dev",
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
        let premine_address =
            Address::from_hex(TESTNET_PREMINE_ADDRESS_HEX).expect("static testnet premine hex");
        let supply = SupplyCaps {
            premine_address,
            ..SupplyCaps::default()
        };
        let expected =
            Hash::from_hex(TESTNET_GENESIS_HASH_HEX).expect("static testnet genesis hex");
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
        GhostdagConfig { k: self.ghostdag_k }
    }

    pub fn for_network(id: NetworkId) -> Result<Self, String> {
        match id {
            NetworkId::Dev => Ok(Self::dev()),
            NetworkId::Testnet => Ok(Self::testnet()),
            NetworkId::Mainnet => {
                Err("mainnet genesis is not frozen yet — use AGORA_NETWORK=testnet or dev".into())
            }
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

/// Consensus policy snapshot embedded in genesis JSON (does not affect Block 0 hash).
///
/// Identity: peers must agree on [`Self::canonical_hash`] in addition to the genesis
/// block hash — otherwise two nodes can share Block 0 while enforcing different rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenesisConsensusPolicy {
    pub pow_algorithm: String,
    pub ghostdag_k: u32,
    pub target_block_time_ms: u64,
    pub daa_window_size: u64,
    pub daa_max_adjustment_factor: f64,
    pub daa_min_level: u32,
    #[serde(default = "default_daa_max_level")]
    pub daa_max_level: u32,
}

impl GenesisConsensusPolicy {
    /// Canonical SHA-256 over a stable encoding of consensus knobs.
    pub fn canonical_hash(&self) -> Hash {
        // Fixed field order — do not use serde_json (key order / float formatting).
        let factor_milli = (self.daa_max_adjustment_factor * 1000.0).round() as u64;
        let payload = (
            self.pow_algorithm.as_str(),
            self.ghostdag_k,
            self.target_block_time_ms,
            self.daa_window_size,
            factor_milli,
            self.daa_min_level,
            self.daa_max_level,
        );
        Hash::hash_borsh(&payload)
    }
}

fn default_daa_max_level() -> u32 {
    128
}

/// Wallet identity snapshot (HRP + BIP-44 / provisional SLIP-0044).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisWalletPolicy {
    pub address_hrp: String,
    pub coin_type: u32,
    pub coin_type_status: String,
    pub bip44_path_account0: String,
}

/// Portable genesis document committed under `docs/genesis/`.
///
/// Version 2 adds chain identity, three-mark token registry, consensus + wallet
/// policy. Hash-affecting monetary fields remain at the top level for v1 compat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisArtifact {
    pub network: NetworkId,
    pub version: u32,
    #[serde(default = "default_chain_name")]
    pub chain_name: String,
    #[serde(default)]
    pub chain_id: String,
    pub timestamp_ms: u64,
    pub bits: u32,
    /// L1 native max supply (TLT) in base units — hash-affecting.
    pub max_supply: u64,
    pub premine: u64,
    pub premine_address: String,
    pub premine_address_hex: String,
    pub genesis_hash: String,
    pub emission_initial_reward: u64,
    pub emission_halving_interval: u64,
    #[serde(default = "default_decimals")]
    pub decimals: u8,
    #[serde(default = "default_native_ticker")]
    pub native_ticker: String,
    #[serde(default)]
    pub tokens: Vec<TokenMark>,
    #[serde(default)]
    pub consensus: Option<GenesisConsensusPolicy>,
    /// Hash of [`GenesisConsensusPolicy`] — network identity beyond Block 0.
    #[serde(default)]
    pub consensus_policy_hash: String,
    #[serde(default)]
    pub wallet: Option<GenesisWalletPolicy>,
}

fn default_chain_name() -> String {
    "Agora Network".into()
}
fn default_decimals() -> u8 {
    8
}
fn default_native_ticker() -> String {
    "TLT".into()
}

fn pow_algorithm_name(algo: PowAlgorithm) -> &'static str {
    match algo {
        PowAlgorithm::RandomX => "randomx",
        PowAlgorithm::KHeavyHash => "kheavyhash",
    }
}

fn parse_pow_algorithm(s: &str) -> Option<PowAlgorithm> {
    match s.trim().to_ascii_lowercase().as_str() {
        "randomx" | "rx" => Some(PowAlgorithm::RandomX),
        "kheavyhash" | "kheavy" | "asic" => Some(PowAlgorithm::KHeavyHash),
        _ => None,
    }
}

impl GenesisArtifact {
    pub fn from_params(params: &ChainParams) -> Self {
        let hash = params.compute_genesis_hash();
        let hrp = params.network.address_hrp();
        let coin_type = params.network.coin_type();
        let max_supply = params.supply.max_supply.as_base_units();
        let consensus = GenesisConsensusPolicy {
            pow_algorithm: pow_algorithm_name(params.pow_algorithm).into(),
            ghostdag_k: params.ghostdag_k,
            target_block_time_ms: params.daa.target_block_time_ms,
            daa_window_size: params.daa.window_size,
            daa_max_adjustment_factor: params.daa.max_adjustment_factor(),
            daa_min_level: params.daa.min_level,
            daa_max_level: params.daa.max_level,
        };
        let consensus_policy_hash = consensus.canonical_hash().to_hex();
        Self {
            network: params.network,
            version: 2,
            chain_name: default_chain_name(),
            chain_id: params.network.chain_id().into(),
            timestamp_ms: params.timestamp_ms,
            bits: params.bits,
            max_supply,
            premine: params.supply.premine.as_base_units(),
            premine_address: params.supply.premine_address.to_bech32_hrp(hrp),
            premine_address_hex: params.supply.premine_address.to_hex(),
            genesis_hash: hash.to_hex(),
            emission_initial_reward: params.emission.initial_reward,
            emission_halving_interval: params.emission.halving_interval,
            decimals: 8,
            native_ticker: "TLT".into(),
            tokens: default_token_marks(max_supply),
            consensus: Some(consensus),
            consensus_policy_hash,
            wallet: Some(GenesisWalletPolicy {
                address_hrp: hrp.into(),
                coin_type,
                coin_type_status: "provisional-slip44-pending".into(),
                bip44_path_account0: format!("m/44'/{coin_type}'/0'/0/0"),
            }),
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

        let (daa, ghostdag_k, pow_algorithm) = if let Some(c) = &self.consensus {
            let pow = parse_pow_algorithm(&c.pow_algorithm).ok_or_else(|| {
                format!("unsupported pow_algorithm in genesis: {}", c.pow_algorithm)
            })?;
            let policy_hash = c.canonical_hash();
            if !self.consensus_policy_hash.is_empty() {
                let expected = Hash::from_hex(&self.consensus_policy_hash).ok_or_else(|| {
                    "invalid consensus_policy_hash in genesis artifact".to_string()
                })?;
                if expected != policy_hash {
                    return Err(format!(
                        "consensus_policy_hash mismatch: artifact {} != computed {}",
                        self.consensus_policy_hash,
                        policy_hash.to_hex()
                    ));
                }
            }
            (
                DaaConfig {
                    target_block_time_ms: c.target_block_time_ms,
                    window_size: c.daa_window_size,
                    max_adjustment_bits: DaaConfig::max_adjustment_bits_from_factor(
                        c.daa_max_adjustment_factor,
                    ),
                    min_level: c.daa_min_level,
                    max_level: c.daa_max_level,
                },
                c.ghostdag_k,
                pow,
            )
        } else {
            (base.daa, base.ghostdag_k, base.pow_algorithm)
        };

        if let Some(w) = &self.wallet {
            let expected_hrp = self.network.address_hrp();
            if !w.address_hrp.eq_ignore_ascii_case(expected_hrp) {
                return Err(format!(
                    "wallet.address_hrp '{}' does not match network {} ({expected_hrp})",
                    w.address_hrp, self.network
                ));
            }
            if w.coin_type != self.network.coin_type() {
                return Err(format!(
                    "wallet.coin_type {} does not match network coin_type {}",
                    w.coin_type,
                    self.network.coin_type()
                ));
            }
        }

        // TLT max_supply in the registry must agree with top-level max_supply when present.
        // Under Trident, OVL/DRC are also L1-native (account modules); v2 artifacts may
        // still list historical L2/L3 layer strings — monetary locus is defined by
        // `TridentGenesisArtifact` (v3) going forward.
        if let Some(tlt) = self.tokens.iter().find(|t| t.ticker == "TLT") {
            if tlt.max_supply != self.max_supply {
                return Err(format!(
                    "TLT max_supply {} != artifact max_supply {}",
                    tlt.max_supply, self.max_supply
                ));
            }
        }

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
            daa,
            ghostdag_k,
            pow_algorithm,
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
        assert_eq!(artifact.version, 2);
        assert_eq!(artifact.native_ticker, "TLT");
        assert_eq!(artifact.tokens.len(), 3);
        assert!(artifact.premine_address.starts_with("agoratest1"));
        assert_eq!(
            artifact.wallet.as_ref().unwrap().address_hrp,
            ADDRESS_HRP_TESTNET
        );
        let roundtrip = artifact.to_params().unwrap();
        assert_eq!(roundtrip.compute_genesis_hash(), hash);
        assert_eq!(roundtrip.ghostdag_k, params.ghostdag_k);
        assert_eq!(roundtrip.daa.min_level, params.daa.min_level);
        let policy = artifact.consensus.as_ref().unwrap();
        assert_eq!(
            artifact.consensus_policy_hash,
            policy.canonical_hash().to_hex()
        );
        eprintln!(
            "testnet consensus_policy_hash = {}",
            artifact.consensus_policy_hash
        );
    }

    #[test]
    fn v1_genesis_json_still_loads() {
        let v1 = r#"{
          "network": "testnet",
          "version": 1,
          "timestamp_ms": 1785715200000,
          "bits": 0,
          "max_supply": 10000000000000000,
          "premine": 1000000000000000,
          "premine_address": "agora1l70vjmcfav256qu225hv4evu2qsya2dfajrcqc",
          "premine_address_hex": "ff9ec96f09eb154d038a552ecae59c50204ea9a9",
          "genesis_hash": "afe59232cd20a16bd56948044149d2b8013e63f3694c113074fef75ab0cb9b98",
          "emission_initial_reward": 5000000000,
          "emission_halving_interval": 210000
        }"#;
        let artifact = GenesisArtifact::from_json(v1).unwrap();
        assert_eq!(artifact.version, 1);
        assert!(artifact.tokens.is_empty());
        let params = artifact.to_params().unwrap();
        assert_eq!(
            params.compute_genesis_hash().to_hex(),
            TESTNET_GENESIS_HASH_HEX
        );
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
        assert_eq!(testnet.daa.min_level, 8);
        assert_eq!(testnet.ghostdag_k, DEFAULT_GHOSTDAG_K);
        assert_eq!(testnet.pow_algorithm, PowAlgorithm::RandomX);

        let mainnet = ChainParams::mainnet();
        assert!(mainnet.daa.min_level >= 8);
        assert_eq!(mainnet.bits, 16);
        assert_eq!(mainnet.pow_algorithm, PowAlgorithm::RandomX);
    }
}
