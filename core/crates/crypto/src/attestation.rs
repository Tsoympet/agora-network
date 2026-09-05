//! Sign / verify Trident checkpoint attestations.

use agora_types::{Address, CheckpointAttestation, CheckpointBody, NativeAssetId};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_checkpoint_attestation(
    body: CheckpointBody,
    set: NativeAssetId,
    keypair: &KeyPair,
) -> Result<CheckpointAttestation, CryptoError> {
    if set == NativeAssetId::TLT {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let signing = body.signing_bytes();
    let signature = keypair.sign(&signing)?;
    Ok(CheckpointAttestation {
        body,
        set,
        validator: keypair.address(),
        public_key: keypair.public_key_bytes().to_vec(),
        signature: signature.to_vec(),
    })
}

pub fn verify_checkpoint_attestation(att: &CheckpointAttestation) -> Result<(), CryptoError> {
    if att.set == NativeAssetId::TLT {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    if att.public_key.len() != 33 || att.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut pubkey: PublicKeyBytes = [0u8; 33];
    pubkey.copy_from_slice(&att.public_key);
    let mut signature: SignatureBytes = [0u8; 64];
    signature.copy_from_slice(&att.signature);
    KeyPair::verify(&pubkey, &att.body.signing_bytes(), &signature)?;
    let addr: Address = address_from_pubkey(&pubkey);
    if addr != att.validator {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agora_types::{CheckpointBody, Hash, NativeAssetId};

    use super::*;
    use crate::mnemonic::{generate_mnemonic, seed_from_mnemonic};

    fn sample_body() -> CheckpointBody {
        CheckpointBody {
            chain_id: "agora-trident-testnet-1".into(),
            genesis_hash: Hash::ZERO,
            consensus_policy_hash: Hash([1u8; 32]),
            state_transition_version: "agora-trident-state-v1".into(),
            blue_score: 9,
            block_hash: Hash([2u8; 32]),
            state_root: Hash([3u8; 32]),
            validator_epoch: 4,
        }
    }

    #[test]
    fn attestation_sign_verify_roundtrip() {
        let phrase = generate_mnemonic().unwrap();
        let seed = seed_from_mnemonic(&phrase, "").unwrap();
        let kp = KeyPair::from_seed(&seed).unwrap();
        let att = sign_checkpoint_attestation(sample_body(), NativeAssetId::OVL, &kp).unwrap();
        verify_checkpoint_attestation(&att).unwrap();
        assert!(sign_checkpoint_attestation(sample_body(), NativeAssetId::TLT, &kp).is_err());
    }
}
