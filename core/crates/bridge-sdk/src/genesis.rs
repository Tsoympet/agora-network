//! Drachma (L3 / Bridge-in-a-Box) genesis artifact — freezes DRC monetary + hub params.
//!
//! Separate from L1 Talanton genesis and L2 Ovolos genesis. Does not mint L1 UTXOs.

use std::path::Path;

use agora_types::{Address, Amount, Hash};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::district::{DistrictConfig, DistrictKind};
use crate::drc::{DrcLedger, DRC_MAX_SUPPLY_BASE};
use crate::BridgeError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct DrcPremine {
    /// Hub or district id that receives the allocation.
    pub district_id: String,
    pub address_hex: String,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct GenesisDistrict {
    pub district_id: String,
    pub kind: String,
    pub chain_id: u64,
    #[serde(default)]
    pub rpc_hint: String,
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
struct DrachmaGenesisBody {
    network: String,
    version: u32,
    chain_id: String,
    parent_l1_network: String,
    parent_l1_genesis_hash: String,
    hub_id: String,
    timestamp_ms: u64,
    max_supply: u64,
    decimals: u8,
    premine: Vec<(String, String, u64)>,
    districts: Vec<(String, String, u64, String)>,
}

/// Frozen Drachma L3 genesis document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrachmaGenesis {
    pub layer: String,
    pub mark: String,
    pub network: String,
    pub version: u32,
    pub chain_name: String,
    pub chain_id: String,
    pub parent_l1_network: String,
    pub parent_l1_genesis_hash: String,
    pub hub_id: String,
    pub timestamp_ms: u64,
    pub max_supply: u64,
    pub decimals: u8,
    pub premine: Vec<DrcPremine>,
    pub districts: Vec<GenesisDistrict>,
    pub genesis_hash: String,
}

impl Default for DrachmaGenesis {
    fn default() -> Self {
        Self::testnet()
    }
}

impl DrachmaGenesis {
    pub fn testnet() -> Self {
        Self {
            layer: "L3".into(),
            mark: "DRC".into(),
            network: "testnet".into(),
            version: 1,
            chain_name: "Drachma Bridge Hub".into(),
            chain_id: "agora-drachma-testnet-1".into(),
            parent_l1_network: "testnet".into(),
            parent_l1_genesis_hash:
                "afe59232cd20a16bd56948044149d2b8013e63f3694c113074fef75ab0cb9b98".into(),
            hub_id: "agora-hub".into(),
            timestamp_ms: 1785715200000,
            max_supply: DRC_MAX_SUPPLY_BASE,
            decimals: 8,
            premine: vec![DrcPremine {
                district_id: "agora-hub".into(),
                address_hex: "ff9ec96f09eb154d038a552ecae59c50204ea9a9".into(),
                // 10% of 6B DRC @ 8 decimals
                amount: 60_000_000_000_000_000,
            }],
            districts: vec![
                GenesisDistrict {
                    district_id: "agora-hub".into(),
                    kind: "general".into(),
                    chain_id: 1,
                    rpc_hint: String::new(),
                },
                GenesisDistrict {
                    district_id: "arena".into(),
                    kind: "gaming".into(),
                    chain_id: 9001,
                    rpc_hint: String::new(),
                },
                GenesisDistrict {
                    district_id: "veil".into(),
                    kind: "privacy".into(),
                    chain_id: 9002,
                    rpc_hint: String::new(),
                },
            ],
            genesis_hash: String::new(),
        }
        .with_computed_hash()
    }

    pub fn mainnet_draft() -> Self {
        Self {
            layer: "L3".into(),
            mark: "DRC".into(),
            network: "mainnet".into(),
            version: 1,
            chain_name: "Drachma Bridge Hub".into(),
            chain_id: "agora-drachma-mainnet-1".into(),
            parent_l1_network: "mainnet".into(),
            parent_l1_genesis_hash: "TBD".into(),
            hub_id: "agora-hub".into(),
            timestamp_ms: 0,
            max_supply: DRC_MAX_SUPPLY_BASE,
            decimals: 8,
            premine: vec![],
            districts: vec![GenesisDistrict {
                district_id: "agora-hub".into(),
                kind: "general".into(),
                chain_id: 1,
                rpc_hint: String::new(),
            }],
            genesis_hash: "TBD".into(),
        }
    }

    fn body(&self) -> DrachmaGenesisBody {
        DrachmaGenesisBody {
            network: self.network.clone(),
            version: self.version,
            chain_id: self.chain_id.clone(),
            parent_l1_network: self.parent_l1_network.clone(),
            parent_l1_genesis_hash: self.parent_l1_genesis_hash.clone(),
            hub_id: self.hub_id.clone(),
            timestamp_ms: self.timestamp_ms,
            max_supply: self.max_supply,
            decimals: self.decimals,
            premine: self
                .premine
                .iter()
                .map(|p| (p.district_id.clone(), p.address_hex.clone(), p.amount))
                .collect(),
            districts: self
                .districts
                .iter()
                .map(|d| {
                    (
                        d.district_id.clone(),
                        d.kind.clone(),
                        d.chain_id,
                        d.rpc_hint.clone(),
                    )
                })
                .collect(),
        }
    }

    pub fn compute_hash(&self) -> Hash {
        Hash::hash_borsh(&self.body())
    }

    pub fn with_computed_hash(mut self) -> Self {
        self.genesis_hash = self.compute_hash().to_hex();
        self
    }

    pub fn from_json(json: &str) -> Result<Self, BridgeError> {
        let g: Self = serde_json::from_str(json)
            .map_err(|e| BridgeError::Constraint(format!("drachma genesis json: {e}")))?;
        g.validate()?;
        Ok(g)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, BridgeError> {
        let raw = std::fs::read_to_string(path.as_ref())
            .map_err(|e| BridgeError::Constraint(format!("read drachma genesis: {e}")))?;
        Self::from_json(&raw)
    }

    pub fn validate(&self) -> Result<(), BridgeError> {
        if self.layer != "L3" || self.mark != "DRC" {
            return Err(BridgeError::Constraint(
                "drachma genesis must be layer=L3 mark=DRC".into(),
            ));
        }
        if self.max_supply == 0 {
            return Err(BridgeError::Constraint(
                "drachma max_supply must be > 0".into(),
            ));
        }
        let premine_sum: u64 = self.premine.iter().map(|p| p.amount).sum();
        if premine_sum > self.max_supply {
            return Err(BridgeError::Constraint(
                "drachma premine exceeds max_supply".into(),
            ));
        }
        let expected = self.compute_hash().to_hex();
        if !self.genesis_hash.is_empty()
            && self.genesis_hash != "TBD"
            && self.genesis_hash != expected
        {
            return Err(BridgeError::Constraint(format!(
                "drachma genesis_hash mismatch: file {} computed {}",
                self.genesis_hash, expected
            )));
        }
        Ok(())
    }

    pub fn district_configs(&self) -> Result<Vec<DistrictConfig>, BridgeError> {
        self.districts
            .iter()
            .map(|d| {
                let kind = match d.kind.to_ascii_lowercase().as_str() {
                    "gaming" => DistrictKind::Gaming,
                    "privacy" => DistrictKind::Privacy,
                    "general" => DistrictKind::General,
                    other => {
                        return Err(BridgeError::Constraint(format!(
                            "unknown district kind {other}"
                        )));
                    }
                };
                Ok(DistrictConfig {
                    district_id: d.district_id.clone(),
                    kind,
                    chain_id: d.chain_id,
                    rpc_hint: d.rpc_hint.clone(),
                })
            })
            .collect()
    }

    /// Build DRC ledger and apply genesis premine (hub locks for hub allocations).
    pub fn ignite_ledger(&self) -> Result<DrcLedger, BridgeError> {
        let mut ledger = DrcLedger::new(self.max_supply);
        for p in &self.premine {
            let addr = Address::from_hex(&p.address_hex)
                .ok_or_else(|| BridgeError::Constraint("bad drachma premine address_hex".into()))?;
            ledger.mint(&p.district_id, addr, Amount::from_base_units(p.amount))?;
        }
        Ok(ledger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testnet_genesis_stable_hash() {
        let g = DrachmaGenesis::testnet();
        assert_eq!(g.mark, "DRC");
        assert_eq!(g.layer, "L3");
        g.validate().unwrap();
        let ledger = g.ignite_ledger().unwrap();
        assert_eq!(ledger.minted(), 60_000_000_000_000_000);
        assert_eq!(g.compute_hash().to_hex(), g.genesis_hash);
    }

    #[test]
    fn docs_testnet_file_matches_embedded() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/genesis/drachma.testnet.genesis.json");
        if !path.exists() {
            return;
        }
        let file = DrachmaGenesis::from_path(&path).unwrap();
        let embedded = DrachmaGenesis::testnet();
        assert_eq!(file.genesis_hash, embedded.genesis_hash);
        assert_eq!(file.max_supply, embedded.max_supply);
        assert_eq!(file.premine, embedded.premine);
    }
}
