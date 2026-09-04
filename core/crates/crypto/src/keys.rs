use secp256k1::ecdsa::Signature;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

use crate::address::address_from_pubkey;
use crate::CryptoError;
use agora_types::Address;

pub type PublicKeyBytes = [u8; 33];
pub type SignatureBytes = [u8; 64];

/// Validate and canonicalize a compressed secp256k1 public key.
pub fn parse_compressed_public_key(bytes: &[u8]) -> Result<PublicKeyBytes, CryptoError> {
    let public =
        PublicKey::from_slice(bytes).map_err(|e| CryptoError::Secp256k1(e.to_string()))?;
    let compressed = public.serialize();
    if bytes != compressed {
        return Err(CryptoError::Secp256k1(
            "public key must use compressed secp256k1 encoding".into(),
        ));
    }
    Ok(compressed)
}

/// secp256k1 keypair used for transaction authorization.
pub struct KeyPair {
    secret: SecretKey,
    public: PublicKey,
}

impl KeyPair {
    pub fn from_secret_bytes(secret: &[u8]) -> Result<Self, CryptoError> {
        let secp = Secp256k1::new();
        let secret =
            SecretKey::from_slice(secret).map_err(|e| CryptoError::Secp256k1(e.to_string()))?;
        let public = PublicKey::from_secret_key(&secp, &secret);
        Ok(Self { secret, public })
    }

    /// Derive a keypair from the first 32 bytes of a BIP-39 seed (account-less shortcut).
    ///
    /// Prefer [`crate::derive_bip44`] for wallet accounts.
    pub fn from_seed(seed: &[u8; 64]) -> Result<Self, CryptoError> {
        Self::from_secret_bytes(&seed[..32])
    }

    pub fn public_key_bytes(&self) -> PublicKeyBytes {
        self.public.serialize()
    }

    pub fn address(&self) -> Address {
        address_from_pubkey(&self.public_key_bytes())
    }

    pub fn sign(&self, message: &[u8]) -> Result<SignatureBytes, CryptoError> {
        let secp = Secp256k1::new();
        let digest = Sha256::digest(message);
        let msg = Message::from_digest_slice(&digest)
            .map_err(|e| CryptoError::Secp256k1(e.to_string()))?;
        let sig = secp.sign_ecdsa(&msg, &self.secret);
        Ok(sig.serialize_compact())
    }

    pub fn verify(
        pubkey: &PublicKeyBytes,
        message: &[u8],
        signature: &SignatureBytes,
    ) -> Result<(), CryptoError> {
        let secp = Secp256k1::verification_only();
        let public =
            PublicKey::from_slice(pubkey).map_err(|e| CryptoError::Secp256k1(e.to_string()))?;
        let digest = Sha256::digest(message);
        let msg = Message::from_digest_slice(&digest)
            .map_err(|e| CryptoError::Secp256k1(e.to_string()))?;
        let sig = Signature::from_compact(signature)
            .map_err(|e| CryptoError::Secp256k1(e.to_string()))?;
        secp.verify_ecdsa(&msg, &sig, &public)
            .map_err(|e| CryptoError::Secp256k1(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mnemonic::{generate_mnemonic, seed_from_mnemonic};

    #[test]
    fn sign_and_verify_roundtrip() {
        let phrase = generate_mnemonic().expect("mnemonic");
        let seed = seed_from_mnemonic(&phrase, "").expect("seed");
        let kp = KeyPair::from_seed(&seed).expect("keypair");
        let msg = b"agora-network";
        let sig = kp.sign(msg).expect("sign");
        KeyPair::verify(&kp.public_key_bytes(), msg, &sig).expect("verify");
        assert_ne!(kp.address(), Address::ZERO);
    }

    #[test]
    fn compressed_public_key_parser_rejects_uncompressed_encoding() {
        let keypair = KeyPair::from_secret_bytes(&[7; 32]).expect("keypair");
        let compressed = keypair.public_key_bytes();
        assert_eq!(
            parse_compressed_public_key(&compressed).unwrap(),
            compressed
        );

        let public = PublicKey::from_slice(&compressed).unwrap();
        assert!(parse_compressed_public_key(&public.serialize_uncompressed()).is_err());
    }
}
