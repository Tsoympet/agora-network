//! Agora mark / token registry for genesis artifacts.
//!
//! L1 consensus settles a single native asset ([`TokenMark::native_tlt`]).
//! Drachma (DRC) and Ovolos (OVL) are frozen in the genesis document for
//! wallets, explorers, and future L2 / bridge issuance — they are **not**
//! separate L1 UTXO asset ids today.

use serde::{Deserialize, Serialize};

/// One named mark in the Agora economy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenMark {
    pub ticker: String,
    pub name: String,
    /// `L1` | `L2` | `L2+`
    pub layer: String,
    /// Max supply in base units (8 decimals), when hard-capped.
    pub max_supply: u64,
    pub decimals: u8,
    pub role: String,
    /// When true, this mark is the L1 `Amount` / `SupplyCaps` native asset.
    #[serde(default)]
    pub native: bool,
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
        }
    }

    /// Drachma — circulating medium (6B whole = 60× TLT unit scale).
    pub fn drachma() -> Self {
        Self {
            ticker: "DRC".into(),
            name: "Drachma".into(),
            layer: "L2+".into(),
            // 6_000_000_000 * 10^8
            max_supply: 600_000_000_000_000_000,
            decimals: 8,
            role: "medium of exchange / district & bridge settlements".into(),
            native: false,
        }
    }

    /// Ovolos — rollup / micro unit (21B whole, Bitcoin-shaped L2 gas brand).
    pub fn ovolos() -> Self {
        Self {
            ticker: "OVL".into(),
            name: "Ovolos".into(),
            layer: "L2".into(),
            // 21_000_000_000 * 10^8
            max_supply: 2_100_000_000_000_000_000,
            decimals: 8,
            role: "Ovolos rollup gas / micro-unit brand".into(),
            native: false,
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
    fn three_marks_and_native_matches_l1_cap() {
        let l1 = 10_000_000_000_000_000u64; // 100M TLT
        let marks = default_token_marks(l1);
        assert_eq!(marks.len(), 3);
        assert_eq!(marks[0].ticker, "TLT");
        assert!(marks[0].native);
        assert_eq!(marks[0].max_supply, l1);
        assert_eq!(marks[1].ticker, "DRC");
        assert_eq!(marks[2].ticker, "OVL");
        assert!(marks[1].max_supply > l1);
        assert!(marks[2].max_supply > marks[1].max_supply);
    }
}
