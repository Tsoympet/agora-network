//! secp256k1 authorization for provenance-bound data commitments.

use agora_types::{DataCommitmentAuthorization, Hash};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_data_commitment_bound(
    authorization: &mut DataCommitmentAuthorization,
    keypair: &KeyPair,
    l1_chain_id: &str,
    l1_genesis: &Hash,
    l1_network_fingerprint: &Hash,
) -> Result<(), CryptoError> {
    validate_context(
        authorization,
        l1_chain_id,
        l1_genesis,
        l1_network_fingerprint,
    )?;
    if authorization.operator != keypair.address() {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let signing =
        authorization.signing_bytes_bound(l1_chain_id, l1_genesis, l1_network_fingerprint);
    let signature = keypair.sign(&signing)?;
    authorization.public_key = keypair.public_key_bytes().to_vec();
    authorization.signature = signature.to_vec();
    Ok(())
}

pub fn verify_data_commitment_bound(
    authorization: &DataCommitmentAuthorization,
    l1_chain_id: &str,
    l1_genesis: &Hash,
    l1_network_fingerprint: &Hash,
) -> Result<(), CryptoError> {
    validate_context(
        authorization,
        l1_chain_id,
        l1_genesis,
        l1_network_fingerprint,
    )?;
    if authorization.public_key.len() != 33 || authorization.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut public_key: PublicKeyBytes = [0; 33];
    public_key.copy_from_slice(&authorization.public_key);
    let mut signature: SignatureBytes = [0; 64];
    signature.copy_from_slice(&authorization.signature);
    KeyPair::verify(
        &public_key,
        &authorization.signing_bytes_bound(l1_chain_id, l1_genesis, l1_network_fingerprint),
        &signature,
    )?;
    if address_from_pubkey(&public_key) != authorization.operator {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

fn validate_context(
    authorization: &DataCommitmentAuthorization,
    l1_chain_id: &str,
    l1_genesis: &Hash,
    l1_network_fingerprint: &Hash,
) -> Result<(), CryptoError> {
    authorization
        .validate()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    let chain_id = l1_chain_id.as_bytes();
    if chain_id.is_empty()
        || chain_id.len() > agora_types::MAX_DA_CHAIN_ID_BYTES
        || !chain_id
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-._".contains(byte))
        || *l1_genesis == Hash::ZERO
        || *l1_network_fingerprint == Hash::ZERO
    {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agora_types::{Address, DataAvailabilityCommitment};

    use super::*;

    fn authorization(operator: Address) -> DataCommitmentAuthorization {
        let commitment = DataAvailabilityCommitment::agora_layers_ovolos_batch(
            "agora-ovolos-testnet-1".into(),
            Hash([1; 32]),
            Hash([2; 32]),
            3,
            Hash([4; 32]),
            Hash([5; 32]),
            Hash([6; 32]),
            7,
            8,
        );
        DataCommitmentAuthorization::unsigned(operator, 9, commitment)
    }

    #[test]
    fn authorization_binds_operator_chain_genesis_fingerprint_and_nonce() {
        let keypair = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let genesis = Hash([8; 32]);
        let fingerprint = Hash([9; 32]);
        let mut authorization = authorization(keypair.address());
        let unsigned_id = authorization.authorization_id();

        sign_data_commitment_bound(
            &mut authorization,
            &keypair,
            "agora-trident-testnet-1",
            &genesis,
            &fingerprint,
        )
        .unwrap();
        assert_ne!(authorization.authorization_id(), unsigned_id);
        verify_data_commitment_bound(
            &authorization,
            "agora-trident-testnet-1",
            &genesis,
            &fingerprint,
        )
        .unwrap();

        assert!(verify_data_commitment_bound(
            &authorization,
            "agora-trident-dev-1",
            &genesis,
            &fingerprint
        )
        .is_err());
        assert!(verify_data_commitment_bound(
            &authorization,
            "agora-trident-testnet-1",
            &Hash([10; 32]),
            &fingerprint
        )
        .is_err());
        assert!(verify_data_commitment_bound(
            &authorization,
            "agora-trident-testnet-1",
            &genesis,
            &Hash([11; 32])
        )
        .is_err());

        let mut replay = authorization.clone();
        replay.replay_nonce += 1;
        assert!(verify_data_commitment_bound(
            &replay,
            "agora-trident-testnet-1",
            &genesis,
            &fingerprint
        )
        .is_err());
    }

    #[test]
    fn signing_rejects_wrong_operator_and_missing_network_identity() {
        let keypair = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let mut wrong_operator = authorization(Address::ZERO);
        assert!(sign_data_commitment_bound(
            &mut wrong_operator,
            &keypair,
            "agora-trident-testnet-1",
            &Hash([8; 32]),
            &Hash([9; 32])
        )
        .is_err());
        assert!(wrong_operator.public_key.is_empty());
        assert!(wrong_operator.signature.is_empty());

        let mut missing_identity = authorization(keypair.address());
        assert!(sign_data_commitment_bound(
            &mut missing_identity,
            &keypair,
            "agora-trident-testnet-1",
            &Hash::ZERO,
            &Hash([9; 32])
        )
        .is_err());
    }
}
