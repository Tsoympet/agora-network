//! Bech32m human-readable parts for Agora networks.
//!
//! Mainnet uses `agora` (→ `agora1…`). Testnet / dev use distinct HRPs so
//! addresses cannot be confused across networks (Bitcoin `bc` / `tb` style).

/// Mainnet / default display HRP.
pub const ADDRESS_HRP_MAINNET: &str = "agora";

/// Public testnet HRP.
pub const ADDRESS_HRP_TESTNET: &str = "agoratest";

/// Local / CI HRP.
pub const ADDRESS_HRP_DEV: &str = "agoradev";

/// Default encode HRP ([`ADDRESS_HRP_MAINNET`]).
pub const ADDRESS_HRP: &str = ADDRESS_HRP_MAINNET;

/// True when `hrp` is a known Agora network HRP (case-insensitive).
pub fn is_known_address_hrp(hrp: &str) -> bool {
    hrp.eq_ignore_ascii_case(ADDRESS_HRP_MAINNET)
        || hrp.eq_ignore_ascii_case(ADDRESS_HRP_TESTNET)
        || hrp.eq_ignore_ascii_case(ADDRESS_HRP_DEV)
}

/// Resolve HRP for a network id string (`mainnet` / `testnet` / `dev`).
pub fn address_hrp_for_network(network: &str) -> &'static str {
    match network.trim().to_ascii_lowercase().as_str() {
        "mainnet" | "main" => ADDRESS_HRP_MAINNET,
        "testnet" | "test" => ADDRESS_HRP_TESTNET,
        _ => ADDRESS_HRP_DEV,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_hrps() {
        assert!(is_known_address_hrp("agora"));
        assert!(is_known_address_hrp("AGORATEST"));
        assert!(is_known_address_hrp("agoradev"));
        assert!(!is_known_address_hrp("bc"));
        assert_eq!(address_hrp_for_network("testnet"), ADDRESS_HRP_TESTNET);
        assert_eq!(address_hrp_for_network("mainnet"), ADDRESS_HRP_MAINNET);
    }
}
