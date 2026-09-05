//! Sign and verify network-bound passport attestations.

use agora_types::{Hash, PassportAttestation};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_passport_attestation_bound(
    attestation: &mut PassportAttestation,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    if keypair.address() != attestation.issuer {
        return Err(CryptoError::InvalidTransactionAuth);
    }

    let signature = keypair.sign(&attestation.signing_bytes_bound(chain_id, genesis))?;
    attestation.public_key = keypair.public_key_bytes().to_vec();
    attestation.signature = signature.to_vec();
    Ok(())
}

pub fn verify_passport_attestation_bound(
    attestation: &PassportAttestation,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    if attestation.public_key.len() != 33 || attestation.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }

    let mut public_key: PublicKeyBytes = [0; 33];
    public_key.copy_from_slice(&attestation.public_key);
    let mut signature: SignatureBytes = [0; 64];
    signature.copy_from_slice(&attestation.signature);

    KeyPair::verify(
        &public_key,
        &attestation.signing_bytes_bound(chain_id, genesis),
        &signature,
    )?;
    if address_from_pubkey(&public_key) != attestation.issuer {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agora_types::{Address, PassportCategory};

    use super::*;

    fn signed_attestation() -> (PassportAttestation, Hash) {
        let keypair = KeyPair::from_secret_bytes(&[1; 32]).unwrap();
        let genesis = Hash([7; 32]);
        let mut attestation = PassportAttestation::unsigned(
            keypair.address(),
            Address([2; 20]),
            PassportCategory::Code,
            Hash([3; 32]),
            Hash([4; 32]),
            5,
            Some(10),
            6,
        );
        sign_passport_attestation_bound(&mut attestation, &keypair, "agora-dev", &genesis).unwrap();
        (attestation, genesis)
    }

    #[test]
    fn passport_signature_is_non_transferable_and_covers_claim() {
        let (attestation, genesis) = signed_attestation();
        verify_passport_attestation_bound(&attestation, "agora-dev", &genesis).unwrap();

        let mut changed = attestation.clone();
        changed.subject = Address([9; 20]);
        assert!(verify_passport_attestation_bound(&changed, "agora-dev", &genesis).is_err());

        let mut changed = attestation.clone();
        changed.category = PassportCategory::Documentation;
        assert!(verify_passport_attestation_bound(&changed, "agora-dev", &genesis).is_err());

        let mut changed = attestation.clone();
        changed.evidence_hash = Hash([9; 32]);
        assert!(verify_passport_attestation_bound(&changed, "agora-dev", &genesis).is_err());
    }

    #[test]
    fn passport_signature_rejects_cross_chain_replay() {
        let (attestation, genesis) = signed_attestation();
        assert!(
            verify_passport_attestation_bound(&attestation, "agora-testnet", &genesis).is_err()
        );
        assert!(
            verify_passport_attestation_bound(&attestation, "agora-dev", &Hash([8; 32])).is_err()
        );
    }

    #[test]
    fn passport_signing_rejects_non_issuer_key() {
        let keypair = KeyPair::from_secret_bytes(&[1; 32]).unwrap();
        let mut attestation = PassportAttestation::unsigned(
            Address::ZERO,
            Address([2; 20]),
            PassportCategory::Code,
            Hash([3; 32]),
            Hash([4; 32]),
            5,
            None,
            6,
        );

        assert!(sign_passport_attestation_bound(
            &mut attestation,
            &keypair,
            "agora-dev",
            &Hash::ZERO
        )
        .is_err());
        assert!(attestation.public_key.is_empty());
        assert!(attestation.signature.is_empty());
    }
}
