//! Agora mark / token registry for genesis artifacts.
//!
//! Each mark is **native PoW money on its own layer**:
//! - **TLT** — L1 BlockDAG UTXO (RandomX)
//! - **OVL** — L2 Ovolos ledger (sha256_leading_zero)
//! - **DRC** — L3 Drachma / districts ledger (sha256_leading_zero)
//!
//! Only TLT is an L1 UTXO asset id. OVL and DRC each have a layer genesis
//! (`docs/genesis/ovolos.*.json`, `docs/genesis/drachma.*.json`).

use serde::{Deserialize, Serialize};

/// One named mark in the Agora economy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenMark {
    pub ticker: String,
    pub name: String,
    /// `L1` | `L2` | `L3`
    pub layer: String,
    /// Max supply in base units (8 decimals), when hard-capped.
    pub max_supply: u64,
    pub decimals: u8,
    pub role: String,
    /// Native money on the mark's declared layer (PoW + coinbase / emission).
    #[serde(default)]
    pub native: bool,
    /// PoW algorithm id for this mark's layer.
    #[serde(default)]
    pub pow_algorithm: String,
}

impl TokenMark {
    /// Talanton — scarce L1 native (100M whole @ 8 decimals).
    pub fn native_tlt(max_supply_base: u64) -> Self {
        Self {
            ticker: "TLT".into(),
            name: "Talanton".into(),
            layer: "L1".into(),
            max_supply: max_supply_base,
            decimals: 8,
            role: "native store of value / BlockDAG settlement".into(),
            native: true,
            pow_algorithm: "randomx".into(),
        }
    }

    /// Drachma — native L3 PoW money (6B whole = 60× TLT unit scale).
    pub fn drachma() -> Self {
        Self {
            ticker: "DRC".into(),
            name: "Drachma".into(),
            layer: "L3".into(),
            // 6_000_000_000 * 10^8
            max_supply: 600_000_000_000_000_000,
            decimals: 8,
            role: "native L3 PoW money / district & bridge settlements".into(),
            native: true,
            pow_algorithm: "sha256_leading_zero".into(),
        }
    }

    /// Ovolos — native L2 PoW money (21B whole, Bitcoin-shaped emission).
    pub fn ovolos() -> Self {
        Self {
            ticker: "OVL".into(),
            name: "Ovolos".into(),
            layer: "L2".into(),
            // 21_000_000_000 * 10^8
            max_supply: 2_100_000_000_000_000_000,
            decimals: 8,
            role: "native L2 PoW money / Ovolos rollup gas".into(),
            native: true,
            pow_algorithm: "sha256_leading_zero".into(),
        }
    }
}

/// Default Agora three-mark registry for a given L1 native max supply (base units).
pub fn default_token_marks(native_max_supply_base: u64) -> Vec<TokenMark> {
    vec![
        TokenMark::native_tlt(native_max_supply_base),
        TokenMark::drachma(),
        TokenMark::ovolos(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_marks_each_native_pow_on_own_layer() {
        let l1 = 10_000_000_000_000_000u64; // 100M TLT
        let marks = default_token_marks(l1);
        assert_eq!(marks.len(), 3);
        assert_eq!(marks[0].ticker, "TLT");
        assert_eq!(marks[0].layer, "L1");
        assert!(marks[0].native);
        assert_eq!(marks[0].pow_algorithm, "randomx");
        assert_eq!(marks[0].max_supply, l1);
        assert_eq!(marks[1].ticker, "DRC");
        assert_eq!(marks[1].layer, "L3");
        assert!(marks[1].native);
        assert_eq!(marks[1].pow_algorithm, "sha256_leading_zero");
        assert_eq!(marks[2].ticker, "OVL");
        assert_eq!(marks[2].layer, "L2");
        assert!(marks[2].native);
        assert_eq!(marks[2].pow_algorithm, "sha256_leading_zero");
        assert!(marks[1].max_supply > l1);
        assert!(marks[2].max_supply > marks[1].max_supply);
    }
}
