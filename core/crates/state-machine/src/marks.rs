//! Agora mark / token registry for genesis artifacts.
//!
//! **Trident L1:** all three marks are protocol-native on one Layer 1.
//! - **TLT** — UTXO settlement; only mineable asset (RandomX)
//! - **OVL** — account module; never mined; execution + OVL validators
//! - **DRC** — account module; never mined; payments + DRC validators

use serde::{Deserialize, Serialize};

/// One named mark in the Agora economy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenMark {
    pub ticker: String,
    pub name: String,
    /// Always `L1` under Trident. Historical artifacts may still say `L2`/`L3`.
    pub layer: String,
    /// Max supply in base units (8 decimals), when hard-capped.
    pub max_supply: u64,
    pub decimals: u8,
    pub role: String,
    /// Native money on the mark's declared layer.
    #[serde(default)]
    pub native: bool,
    /// PoW algorithm id, or empty/`none` when not mineable.
    #[serde(default)]
    pub pow_algorithm: String,
    /// When false, protocol forbids PoW issuance for this mark.
    #[serde(default = "default_true_mineable_compat")]
    pub mineable: bool,
}

fn default_true_mineable_compat() -> bool {
    // Historical JSON without `mineable` keeps prior PoW marks readable;
    // Trident constructors set the field explicitly.
    true
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
            role: "L1 settlement / PoW security / base network fees".into(),
            native: true,
            pow_algorithm: "randomx".into(),
            mineable: true,
        }
    }

    /// Drachma — native L1 payment / community validator asset (never mined).
    pub fn drachma() -> Self {
        Self {
            ticker: "DRC".into(),
            name: "Drachma".into(),
            layer: "L1".into(),
            // 6_000_000_000 * 10^8
            max_supply: 600_000_000_000_000_000,
            decimals: 8,
            role: "L1 payments / DRC validators / community economy".into(),
            native: true,
            pow_algorithm: "none".into(),
            mineable: false,
        }
    }

    /// Ovolos — native L1 execution / technical validator asset (never mined).
    pub fn ovolos() -> Self {
        Self {
            ticker: "OVL".into(),
            name: "Ovolos".into(),
            layer: "L1".into(),
            // 21_000_000_000 * 10^8
            max_supply: 2_100_000_000_000_000_000,
            decimals: 8,
            role: "L1 execution gas / OVL validators / builder economy".into(),
            native: true,
            pow_algorithm: "none".into(),
            mineable: false,
        }
    }
}

/// Default Agora three-mark registry for a given L1 native max supply (base units).
pub fn default_token_marks(native_max_supply_base: u64) -> Vec<TokenMark> {
    vec![
        TokenMark::native_tlt(native_max_supply_base),
        TokenMark::ovolos(),
        TokenMark::drachma(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_marks_native_on_l1_only_tlt_mineable() {
        let l1 = 10_000_000_000_000_000u64; // 100M TLT
        let marks = default_token_marks(l1);
        assert_eq!(marks.len(), 3);
        assert_eq!(marks[0].ticker, "TLT");
        assert_eq!(marks[0].layer, "L1");
        assert!(marks[0].native);
        assert!(marks[0].mineable);
        assert_eq!(marks[0].pow_algorithm, "randomx");
        assert_eq!(marks[0].max_supply, l1);

        assert_eq!(marks[1].ticker, "OVL");
        assert_eq!(marks[1].layer, "L1");
        assert!(marks[1].native);
        assert!(!marks[1].mineable);
        assert_eq!(marks[1].pow_algorithm, "none");

        assert_eq!(marks[2].ticker, "DRC");
        assert_eq!(marks[2].layer, "L1");
        assert!(marks[2].native);
        assert!(!marks[2].mineable);
        assert_eq!(marks[2].pow_algorithm, "none");

        assert!(marks[2].max_supply > l1);
        assert!(marks[1].max_supply > marks[2].max_supply);
    }
}
