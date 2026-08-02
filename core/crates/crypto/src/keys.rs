use secp256k1::ecdsa::Signature;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

use crate::CryptoError;

pub type PublicKeyBytes = [u8; 33];
pub type SignatureBytes = [u8; 64];

/// secp256k1 keypair used for transaction authorization.
pub struct KeyPair {
    secret: SecretKey,
    public: PublicKey,
}

impl KeyPair {
    /// Derive a keypair from the first 32 bytes of a BIP-39 seed.
    ///
    /// Full BIP-44 path hardening lands in a follow-up; this keeps Phase 1 signing usable.
    pub fn from_seed(seed: &[u8; 64]) -> Result<Self, CryptoError> {
        let secp = Secp256k1::new();
        let secret = SecretKey::from_slice(&seed[..32])
            .map_err(|e| CryptoError::Secp256k1(e.to_string()))?;
        let public = PublicKey::from_secret_key(&secp, &secret);
        Ok(Self { secret, public })
    }

    pub fn public_key_bytes(&self) -> PublicKeyBytes {
        self.public.serialize()
    }

    pub fn sign(&self, message: &[u8]) -> Result<SignatureBytes, CryptoError> {
        let secp = Secp256k1::new();
        let digest = Sha256::digest(message);
        let msg = Message::from_digest_slice(&digest)
            .map_err(|e| CryptoError::Secp256k1(e.to_string()))?;
        let sig = secp.sign_ecdsa(&msg, &self.secret);
        Ok(sig.serialize_compact())
    }

    pub fn verify(pubkey: &PublicKeyBytes, message: &[u8], signature: &SignatureBytes) -> Result<(), CryptoError> {
        let secp = Secp256k1::verification_only();
        let public = PublicKey::from_slice(pubkey)
            .map_err(|e| CryptoError::Secp256k1(e.to_string()))?;
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
    }
}
