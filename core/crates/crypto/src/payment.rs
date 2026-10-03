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
    tx.validate_envelope_version()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
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
    tx.validate_envelope_version()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
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

    fn payment_v2(from: Address, source_tag: Option<u32>) -> DrcPaymentTx {
        DrcPaymentTx::unsigned_v2(
            from,
            Address([2u8; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(1),
            42,
            source_tag,
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

    #[test]
    fn drc_payment_v2_source_tag_is_authenticated() {
        let keypair = keypair();
        let genesis = Hash([7u8; 32]);
        let mut tx = payment_v2(keypair.address(), Some(u32::MAX));

        sign_drc_payment_bound(&mut tx, &keypair, "agora-dev", &genesis).unwrap();
        verify_drc_payment_bound(&tx, "agora-dev", &genesis).unwrap();

        tx.source_tag = Some(u32::MAX - 1);
        assert!(verify_drc_payment_bound(&tx, "agora-dev", &genesis).is_err());
        tx.source_tag = None;
        assert!(verify_drc_payment_bound(&tx, "agora-dev", &genesis).is_err());
    }

    #[test]
    fn drc_payment_rejects_unsupported_or_legacy_source_tag_versions() {
        let keypair = keypair();
        let mut legacy = payment(keypair.address());
        legacy.source_tag = Some(0);
        assert!(sign_drc_payment_bound(&mut legacy, &keypair, "agora-dev", &Hash::ZERO).is_err());

        let mut unsupported = payment_v2(keypair.address(), None);
        unsupported.version = agora_types::DRC_PAYMENT_VERSION + 1;
        assert!(
            sign_drc_payment_bound(&mut unsupported, &keypair, "agora-dev", &Hash::ZERO).is_err()
        );
    }

    #[test]
    fn drc_payment_v3_authenticates_destination_tag_presence_and_zero() {
        let keypair = keypair();
        let genesis = Hash([7; 32]);
        let mut tx = DrcPaymentTx::unsigned_v3(
            keypair.address(),
            Address([2; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(1),
            Some(0),
            None,
            Hash::ZERO,
            0,
        );
        sign_drc_payment_bound(&mut tx, &keypair, "agora-dev", &genesis).unwrap();
        verify_drc_payment_bound(&tx, "agora-dev", &genesis).unwrap();

        tx.destination_tag = None;
        assert!(verify_drc_payment_bound(&tx, "agora-dev", &genesis).is_err());
    }

    #[test]
    fn drc_payment_v4_authenticates_last_valid_blue_score() {
        let keypair = keypair();
        let genesis = Hash([7; 32]);
        let mut tx = DrcPaymentTx::unsigned_v4(
            keypair.address(),
            Address([2; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(1),
            Some(0),
            None,
            Hash::ZERO,
            0,
            Some(42),
        );
        sign_drc_payment_bound(&mut tx, &keypair, "agora-dev", &genesis).unwrap();
        verify_drc_payment_bound(&tx, "agora-dev", &genesis).unwrap();

        tx.last_valid_blue_score = Some(43);
        assert!(verify_drc_payment_bound(&tx, "agora-dev", &genesis).is_err());
        tx.last_valid_blue_score = None;
        assert!(verify_drc_payment_bound(&tx, "agora-dev", &genesis).is_err());
    }
}
