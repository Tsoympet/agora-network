use agora_types::Address;
use sha2::{Digest, Sha256};

use crate::PublicKeyBytes;

/// Derive a 20-byte Agora address from a compressed secp256k1 public key.
///
/// Uses the first 20 bytes of SHA-256(pubkey). String form is Bech32m via
/// [`Address::to_bech32`] (`agora1…`); consensus encoding stays raw bytes.
pub fn address_from_pubkey(pubkey: &PublicKeyBytes) -> Address {
    let digest = Sha256::digest(pubkey);
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest[..20]);
    Address(out)
}
