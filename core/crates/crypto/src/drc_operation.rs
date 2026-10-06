//! Shared secp256k1 verification helpers for DRC account-bound operations.

use agora_types::Address;

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

/// Verify a bound secp256k1 signature and return the signer-derived address.
pub fn verify_bound_secp256k1(
    public_key: &[u8],
    signature: &[u8],
    signing_bytes: &[u8],
) -> Result<Address, CryptoError> {
    if public_key.len() != 33 || signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut pubkey: PublicKeyBytes = [0u8; 33];
    pubkey.copy_from_slice(public_key);
    let mut sig: SignatureBytes = [0u8; 64];
    sig.copy_from_slice(signature);
    KeyPair::verify(&pubkey, signing_bytes, &sig)?;
    Ok(address_from_pubkey(&pubkey))
}
