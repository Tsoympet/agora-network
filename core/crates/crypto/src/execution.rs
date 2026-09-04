//! Sign and verify network-bound OVL execution envelopes.

use agora_types::{Hash, OvlExecutionTx};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_ovl_execution_bound(
    tx: &mut OvlExecutionTx,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    if keypair.address() != tx.from {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let signature = keypair.sign(&tx.signing_bytes_bound(chain_id, genesis))?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

pub fn verify_ovl_execution_bound(
    tx: &OvlExecutionTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    if tx.public_key.len() != 33 || tx.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }

    let mut public_key: PublicKeyBytes = [0u8; 33];
    public_key.copy_from_slice(&tx.public_key);
    let mut signature: SignatureBytes = [0u8; 64];
    signature.copy_from_slice(&tx.signature);

    KeyPair::verify(
        &public_key,
        &tx.signing_bytes_bound(chain_id, genesis),
        &signature,
    )?;
    if address_from_pubkey(&public_key) != tx.from {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agora_types::{Address, Amount};

    use super::*;

    fn keypair() -> KeyPair {
        KeyPair::from_secret_bytes(&[1u8; 32]).expect("valid deterministic secret")
    }

    #[test]
    fn ovl_execution_bound_sign_verify_and_replay_rejection() {
        let keypair = keypair();
        let genesis = Hash([7u8; 32]);
        let mut tx = OvlExecutionTx::unsigned(
            keypair.address(),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            40_000,
            5,
            6,
            vec![0xaa, 0xbb],
        );

        sign_ovl_execution_bound(&mut tx, &keypair, "agora-dev", &genesis).unwrap();
        verify_ovl_execution_bound(&tx, "agora-dev", &genesis).unwrap();

        assert!(verify_ovl_execution_bound(&tx, "agora-testnet-1", &genesis).is_err());
        assert!(verify_ovl_execution_bound(&tx, "agora-dev", &Hash([8u8; 32])).is_err());
    }

    #[test]
    fn ovl_execution_signing_rejects_wrong_sender() {
        let keypair = keypair();
        let mut tx = OvlExecutionTx::unsigned(
            Address::ZERO,
            Address([2u8; 20]),
            Amount::ZERO,
            21_000,
            1,
            0,
            Vec::new(),
        );

        assert!(sign_ovl_execution_bound(&mut tx, &keypair, "agora-dev", &Hash::ZERO).is_err());
        assert!(tx.public_key.is_empty());
        assert!(tx.signature.is_empty());
    }
}
