//! Sign and verify network-bound DRC payment envelopes.

use agora_types::{DrcPaymentTx, Hash};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_drc_payment_bound(
    tx: &mut DrcPaymentTx,
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

pub fn verify_drc_payment_bound(
    tx: &DrcPaymentTx,
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

    fn payment(from: Address) -> DrcPaymentTx {
        DrcPaymentTx::unsigned(
            from,
            Address([2u8; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(1),
            42,
            Hash([6u8; 32]),
            7,
        )
    }

    #[test]
    fn drc_payment_bound_sign_verify_and_replay_rejection() {
        let keypair = keypair();
        let genesis = Hash([7u8; 32]);
        let mut tx = payment(keypair.address());

        sign_drc_payment_bound(&mut tx, &keypair, "agora-dev", &genesis).unwrap();
        verify_drc_payment_bound(&tx, "agora-dev", &genesis).unwrap();

        assert!(verify_drc_payment_bound(&tx, "agora-testnet-1", &genesis).is_err());
        assert!(verify_drc_payment_bound(&tx, "agora-dev", &Hash([8u8; 32])).is_err());
    }

    #[test]
    fn drc_payment_rejects_wrong_sender() {
        let keypair = keypair();
        let mut tx = payment(Address::ZERO);

        assert!(sign_drc_payment_bound(&mut tx, &keypair, "agora-dev", &Hash::ZERO).is_err());
        assert!(tx.public_key.is_empty());
        assert!(tx.signature.is_empty());

        let mut signed = payment(keypair.address());
        sign_drc_payment_bound(&mut signed, &keypair, "agora-dev", &Hash::ZERO).unwrap();
        signed.from = Address::ZERO;
        assert!(verify_drc_payment_bound(&signed, "agora-dev", &Hash::ZERO).is_err());
    }
}
