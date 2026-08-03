use std::str::FromStr;

use bip32::{DerivationPath, XPrv};

use crate::{CryptoError, KeyPair};

/// Provisional BIP-44 / SLIP-0044 coin type for Agora.
///
/// **Not yet registered** with SLIP-0044 — replace before mainnet freeze.
/// Until then every Agora network (including testnet) derives with this type so
/// existing premine / faucet vectors stay stable.
pub const AGORA_COIN_TYPE: u32 = 8888;

/// Alias documenting the SLIP-0044 application target.
pub const AGORA_COIN_TYPE_PROVISIONAL: u32 = AGORA_COIN_TYPE;

/// BIP-44 coin type used for public testnet wallets (same as provisional until SLIP assign).
pub const AGORA_COIN_TYPE_TESTNET: u32 = AGORA_COIN_TYPE;

/// BIP-44 account path builder: `m/44'/coin_type'/account'/change/index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bip44Path {
    pub coin_type: u32,
    pub account: u32,
    pub change: u32,
    pub index: u32,
}

impl Default for Bip44Path {
    fn default() -> Self {
        Self {
            coin_type: AGORA_COIN_TYPE,
            account: 0,
            change: 0,
            index: 0,
        }
    }
}

impl Bip44Path {
    pub fn new(account: u32, change: u32, index: u32) -> Self {
        Self {
            coin_type: AGORA_COIN_TYPE,
            account,
            change,
            index,
        }
    }

    pub fn external(index: u32) -> Self {
        Self::new(0, 0, index)
    }

    pub fn to_string_path(&self) -> String {
        // Hardened purpose / coin / account per BIP-44.
        format!(
            "m/44'/{}'/{}'/{}/{}",
            self.coin_type, self.account, self.change, self.index
        )
    }
}

/// Derive a secp256k1 keypair at a BIP-44 path from a BIP-39 seed.
pub fn derive_bip44(seed: &[u8; 64], path: &Bip44Path) -> Result<KeyPair, CryptoError> {
    let derivation = DerivationPath::from_str(&path.to_string_path())
        .map_err(|e| CryptoError::Bip32(e.to_string()))?;
    let xprv = XPrv::derive_from_path(seed, &derivation)
        .map_err(|e| CryptoError::Bip32(e.to_string()))?;
    // bip32 uses k256 under the hood; we only take the 32-byte secret into secp256k1.
    KeyPair::from_secret_bytes(xprv.private_key().to_bytes().as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mnemonic::seed_from_mnemonic;

    // Fixed vector mnemonic (BIP-39 test phrase) for deterministic path checks.
    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn bip44_paths_are_deterministic_and_distinct() {
        let seed = seed_from_mnemonic(PHRASE, "").expect("seed");
        let a = derive_bip44(&seed, &Bip44Path::external(0)).expect("idx0");
        let b = derive_bip44(&seed, &Bip44Path::external(1)).expect("idx1");
        let a2 = derive_bip44(&seed, &Bip44Path::external(0)).expect("idx0 again");
        assert_eq!(a.public_key_bytes(), a2.public_key_bytes());
        assert_ne!(a.public_key_bytes(), b.public_key_bytes());
    }
}
