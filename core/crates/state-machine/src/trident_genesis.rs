//! Agora Trident genesis artifact (schema v3).
//!
//! Deterministic serialization uses fixed-field borsh hashing for consensus-relevant
//! identity. The JSON draft under `docs/genesis/trident.testnet.genesis.draft.json`
//! is human-editable until freeze; `genesis_hash` / fingerprint stay `UNFROZEN`
//! until a ceremony fills allocations and computes digests.

use agora_types::Hash;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::monetary::TridentMonetaryPolicy;
use crate::network::{GenesisConsensusPolicy, GenesisWalletPolicy, NetworkId};

/// Schema identifier embedded in Trident genesis JSON.
pub const TRIDENT_GENESIS_SCHEMA: &str = "agora-trident-genesis-v3";
/// State-transition version committed into the Trident network fingerprint.
pub const TRIDENT_STATE_TRANSITION_VERSION: &str = "agora-trident-state-v5";
/// Consensus-policy version string for Trident.
pub const TRIDENT_CONSENSUS_POLICY_VERSION: &str = "agora-trident-consensus-v1";
/// Fingerprint domain for Trident meshes (distinct from v2 `agora-net-fp-v1`).
pub const TRIDENT_NET_FP_DOMAIN: &[u8] = b"agora-trident-net-fp-v1";
/// v4 adds native DRC payment gossip and block-lane settlement.
pub const TRIDENT_PROTOCOL_VERSION: u32 = 4;
pub const TRIDENT_TX_SIGNING_VERSION: &str = "agora-trident-tx-v1";

/// Finality parameters (independent PoS quorums; no price oracle).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TridentFinalityPolicy {
    pub model: String,
    pub ovl_quorum_numerator: u32,
    pub ovl_quorum_denominator: u32,
    pub drc_quorum_numerator: u32,
    pub drc_quorum_denominator: u32,
    pub combine_stakes_by_price: bool,
    pub admin_bypass: bool,
}

impl Default for TridentFinalityPolicy {
    fn default() -> Self {
        Self {
            model: "pow_plus_dual_pos".into(),
            ovl_quorum_numerator: 2,
            ovl_quorum_denominator: 3,
            drc_quorum_numerator: 2,
            drc_quorum_denominator: 3,
            combine_stakes_by_price: false,
            admin_bypass: false,
        }
    }
}

impl TridentFinalityPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.combine_stakes_by_price {
            return Err("combine_stakes_by_price must be false".into());
        }
        if self.admin_bypass {
            return Err("admin_bypass must be false".into());
        }
        for (n, d, label) in [
            (
                self.ovl_quorum_numerator,
                self.ovl_quorum_denominator,
                "OVL",
            ),
            (
                self.drc_quorum_numerator,
                self.drc_quorum_denominator,
                "DRC",
            ),
        ] {
            if d == 0 || n == 0 || n > d {
                return Err(format!("invalid {label} quorum {n}/{d}"));
            }
        }
        Ok(())
    }
}

/// Validator bootstrap parameters (empty until ceremony / Phase 3).
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Default,
)]
pub struct TridentValidatorGenesis {
    pub max_validators: u32,
    pub min_self_bond: u64,
    pub unbonding_period_checkpoints: u64,
}

/// Portable Trident genesis document (version 3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TridentGenesisArtifact {
    pub network: NetworkId,
    pub version: u32,
    pub schema: String,
    pub chain_name: String,
    pub chain_id: String,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub decimals: u8,
    pub state_transition_version: String,
    pub consensus_policy_version: String,
    pub monetary: TridentMonetaryPolicy,
    pub finality: TridentFinalityPolicy,
    pub ovl_validators: TridentValidatorGenesis,
    pub drc_validators: TridentValidatorGenesis,
    pub consensus: GenesisConsensusPolicy,
    pub wallet: GenesisWalletPolicy,
    /// Hex digest once frozen; `UNFROZEN` in drafts.
    pub genesis_hash: String,
    /// Hex network fingerprint once frozen; `UNFROZEN` in drafts.
    pub network_fingerprint: String,
    #[serde(default)]
    pub governance_constitution_hash: String,
    #[serde(default)]
    pub emergency_policy_hash: String,
}

impl TridentGenesisArtifact {
    pub fn draft_testnet() -> Self {
        let monetary = TridentMonetaryPolicy::default();
        let consensus = GenesisConsensusPolicy {
            pow_algorithm: "randomx".into(),
            ghostdag_k: 18,
            target_block_time_ms: 1_000,
            daa_window_size: 90,
            daa_max_adjustment_factor: 2.0,
            daa_min_level: 8,
            daa_max_level: 128,
        };
        Self {
            network: NetworkId::Testnet,
            version: 3,
            schema: TRIDENT_GENESIS_SCHEMA.into(),
            chain_name: "Agora Trident".into(),
            chain_id: "agora-trident-testnet-1".into(),
            timestamp_ms: 0,
            bits: 0,
            decimals: 8,
            state_transition_version: TRIDENT_STATE_TRANSITION_VERSION.into(),
            consensus_policy_version: TRIDENT_CONSENSUS_POLICY_VERSION.into(),
            monetary,
            finality: TridentFinalityPolicy::default(),
            ovl_validators: TridentValidatorGenesis::default(),
            drc_validators: TridentValidatorGenesis::default(),
            consensus,
            wallet: GenesisWalletPolicy {
                address_hrp: "agoratest".into(),
                coin_type: 8888,
                coin_type_status: "provisional-slip44-pending".into(),
                bip44_path_account0: "m/44'/8888'/0'/0/0".into(),
            },
            genesis_hash: "UNFROZEN".into(),
            network_fingerprint: "UNFROZEN".into(),
            governance_constitution_hash: "UNFROZEN".into(),
            emergency_policy_hash: "UNFROZEN".into(),
        }
    }

    pub fn validate_draft(&self) -> Result<(), String> {
        if self.version != 3 {
            return Err(format!("expected version 3, got {}", self.version));
        }
        if self.schema != TRIDENT_GENESIS_SCHEMA {
            return Err(format!("unexpected schema {}", self.schema));
        }
        if self.chain_id.is_empty() {
            return Err("chain_id required".into());
        }
        if self.state_transition_version != TRIDENT_STATE_TRANSITION_VERSION {
            return Err("state_transition_version mismatch".into());
        }
        if self.consensus_policy_version != TRIDENT_CONSENSUS_POLICY_VERSION {
            return Err("consensus_policy_version mismatch".into());
        }
        self.monetary.validate()?;
        self.finality.validate()?;
        if self.consensus.pow_algorithm != "randomx" {
            return Err("Trident public nets require randomx".into());
        }
        Ok(())
    }

    /// Deterministic digest over consensus-relevant fields (not the mutable UNFROZEN strings).
    pub fn consensus_identity_hash(&self) -> Hash {
        let policy = self.consensus.canonical_hash();
        Hash::hash_borsh(&(
            TRIDENT_GENESIS_SCHEMA,
            self.version,
            self.chain_id.as_str(),
            self.timestamp_ms,
            self.bits,
            self.decimals,
            self.state_transition_version.as_str(),
            self.consensus_policy_version.as_str(),
            &self.monetary,
            &self.finality,
            &self.ovl_validators,
            &self.drc_validators,
            policy.as_bytes(),
            self.wallet.address_hrp.as_str(),
            self.wallet.coin_type,
        ))
    }

    /// Trident network fingerprint — any consensus-relevant change must alter this.
    pub fn compute_network_fingerprint(&self) -> Hash {
        let identity = self.consensus_identity_hash();
        let policy = self.consensus.canonical_hash();
        Hash::hash_borsh(&(
            TRIDENT_NET_FP_DOMAIN,
            TRIDENT_PROTOCOL_VERSION,
            self.chain_id.as_str(),
            identity.as_bytes(),
            policy.as_bytes(),
            TRIDENT_TX_SIGNING_VERSION,
            TRIDENT_STATE_TRANSITION_VERSION,
            TRIDENT_CONSENSUS_POLICY_VERSION,
        ))
    }

    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_validates_and_fingerprint_stable_for_same_doc() {
        let a = TridentGenesisArtifact::draft_testnet();
        a.validate_draft().unwrap();
        let fp1 = a.compute_network_fingerprint();
        let fp2 = a.compute_network_fingerprint();
        assert_eq!(fp1, fp2);
        assert_ne!(fp1, Hash::ZERO);
    }

    #[test]
    fn fingerprint_changes_when_chain_id_changes() {
        let mut a = TridentGenesisArtifact::draft_testnet();
        let fp1 = a.compute_network_fingerprint();
        a.chain_id = "agora-trident-testnet-2".into();
        let fp2 = a.compute_network_fingerprint();
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn fingerprint_changes_when_ovl_cap_changes() {
        let mut a = TridentGenesisArtifact::draft_testnet();
        let fp1 = a.compute_network_fingerprint();
        a.monetary.ovl.max_supply -= 1;
        let fp2 = a.compute_network_fingerprint();
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn finality_rejects_admin_bypass_and_price_combine() {
        let mut f = TridentFinalityPolicy::default();
        f.admin_bypass = true;
        assert!(f.validate().is_err());
        f.admin_bypass = false;
        f.combine_stakes_by_price = true;
        assert!(f.validate().is_err());
    }

    #[test]
    fn json_roundtrip_draft() {
        let a = TridentGenesisArtifact::draft_testnet();
        let json = a.to_json_pretty().unwrap();
        let b = TridentGenesisArtifact::from_json(&json).unwrap();
        assert_eq!(a.chain_id, b.chain_id);
        assert_eq!(
            a.compute_network_fingerprint(),
            b.compute_network_fingerprint()
        );
    }
}
