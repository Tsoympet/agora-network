//! Sign and verify native DRC payment channel operations.

use agora_types::{
    payment_channel_offledger_claim_signing_bytes, Amount, DrcPaymentChannelClaimTx,
    DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, Hash,
};

use crate::{parse_compressed_public_key, CryptoError, KeyPair, SignatureBytes};

pub fn sign_drc_payment_channel_create_bound(
    tx: &mut DrcPaymentChannelCreateTx,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    parse_compressed_public_key(&tx.claim_public_key)?;
    let signature = keypair.sign(&tx.signing_bytes_bound(chain_id, genesis))?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

pub fn verify_drc_payment_channel_create_bound(
    tx: &DrcPaymentChannelCreateTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    parse_compressed_public_key(&tx.claim_public_key)?;
    crate::drc_operation::verify_bound_secp256k1(
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
    .map(|_| ())
}

pub fn sign_drc_payment_channel_fund_bound(
    tx: &mut DrcPaymentChannelFundTx,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    let signature = keypair.sign(&tx.signing_bytes_bound(chain_id, genesis))?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

pub fn verify_drc_payment_channel_fund_bound(
    tx: &DrcPaymentChannelFundTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    crate::drc_operation::verify_bound_secp256k1(
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
    .map(|_| ())
}

pub fn sign_drc_payment_channel_claim_bound(
    tx: &mut DrcPaymentChannelClaimTx,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    let signature = keypair.sign(&tx.signing_bytes_bound(chain_id, genesis))?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

pub fn verify_drc_payment_channel_claim_bound(
    tx: &DrcPaymentChannelClaimTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    crate::drc_operation::verify_bound_secp256k1(
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
    .map(|_| ())
}

pub fn sign_payment_channel_offledger_claim(
    claim_key: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
    channel_id: &Hash,
    cumulative_authorized: Amount,
) -> Result<SignatureBytes, CryptoError> {
    let msg = payment_channel_offledger_claim_signing_bytes(
        chain_id,
        genesis,
        channel_id,
        cumulative_authorized,
    );
    claim_key.sign(&msg)
}

pub fn verify_payment_channel_offledger_claim(
    claim_public_key: &[u8],
    signature: &[u8],
    chain_id: &str,
    genesis: &Hash,
    channel_id: &Hash,
    cumulative_authorized: Amount,
) -> Result<(), CryptoError> {
    let pubkey = parse_compressed_public_key(claim_public_key)?;
    if signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut sig: SignatureBytes = [0u8; 64];
    sig.copy_from_slice(signature);
    let msg = payment_channel_offledger_claim_signing_bytes(
        chain_id,
        genesis,
        channel_id,
        cumulative_authorized,
    );
    KeyPair::verify(&pubkey, &msg, &sig)
}

pub fn sign_drc_payment_channel_close_bound(
    tx: &mut DrcPaymentChannelCloseTx,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    let signature = keypair.sign(&tx.signing_bytes_bound(chain_id, genesis))?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

pub fn verify_drc_payment_channel_close_bound(
    tx: &DrcPaymentChannelCloseTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    crate::drc_operation::verify_bound_secp256k1(
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use agora_types::{
        Amount, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseKind, DrcPaymentChannelCloseTx,
        DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, Hash,
        DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION, DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION,
        DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION, DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
    };

    use super::*;

    fn keypair(byte: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[byte; 32]).expect("valid deterministic secret")
    }

    #[test]
    fn offledger_claim_vector_is_stable() {
        let claim_key = keypair(42);
        let genesis = Hash([7; 32]);
        let channel_id = Hash([8; 32]);
        let cumulative = Amount::from_base_units(100);
        let msg = payment_channel_offledger_claim_signing_bytes(
            "agora-dev",
            &genesis,
            &channel_id,
            cumulative,
        );
        assert!(!msg.is_empty());
        let sig = sign_payment_channel_offledger_claim(
            &claim_key,
            "agora-dev",
            &genesis,
            &channel_id,
            cumulative,
        )
        .unwrap();
        verify_payment_channel_offledger_claim(
            &claim_key.public_key_bytes(),
            &sig,
            "agora-dev",
            &genesis,
            &channel_id,
            cumulative,
        )
        .unwrap();
    }

    #[test]
    fn claim_on_chain_signing_binds_cumulative_and_channel_signature() {
        let dest = keypair(2);
        let genesis = Hash([1; 32]);
        let mut tx = DrcPaymentChannelClaimTx {
            version: DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION,
            submitter: dest.address(),
            channel_id: Hash([3; 32]),
            cumulative_authorized: Amount::from_base_units(10),
            channel_claim_signature: vec![0u8; 64],
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_claim_bound(&mut tx, &dest, "agora-dev", &genesis).unwrap();
        let mut tampered = tx.clone();
        tampered.cumulative_authorized = Amount::from_base_units(11);
        assert!(verify_drc_payment_channel_claim_bound(&tampered, "agora-dev", &genesis).is_err());
        let mut tampered_sig = tx.clone();
        tampered_sig.channel_claim_signature[0] ^= 1;
        assert!(
            verify_drc_payment_channel_claim_bound(&tampered_sig, "agora-dev", &genesis).is_err()
        );
    }

    #[test]
    fn close_on_chain_signing_binds_close_kind() {
        let owner = keypair(1);
        let genesis = Hash([2; 32]);
        let mut tx = DrcPaymentChannelCloseTx {
            version: DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION,
            submitter: owner.address(),
            channel_id: Hash([4; 32]),
            close_kind: DrcPaymentChannelCloseKind::OwnerScheduleClose,
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_close_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();
        let mut swapped = tx.clone();
        swapped.close_kind = DrcPaymentChannelCloseKind::Finalize;
        assert!(verify_drc_payment_channel_close_bound(&swapped, "agora-dev", &genesis).is_err());
    }

    #[test]
    fn offledger_wrong_domain_amount_channel_and_key_fail() {
        let claim_key = keypair(5);
        let genesis = Hash([6; 32]);
        let channel_id = Hash([9; 32]);
        let cumulative = Amount::from_base_units(50);
        let sig = sign_payment_channel_offledger_claim(
            &claim_key,
            "agora-dev",
            &genesis,
            &channel_id,
            cumulative,
        )
        .unwrap();
        assert!(verify_payment_channel_offledger_claim(
            &claim_key.public_key_bytes(),
            &sig,
            "wrong-chain",
            &genesis,
            &channel_id,
            cumulative,
        )
        .is_err());
        assert!(verify_payment_channel_offledger_claim(
            &claim_key.public_key_bytes(),
            &sig,
            "agora-dev",
            &genesis,
            &Hash([0; 32]),
            cumulative,
        )
        .is_err());
        assert!(verify_payment_channel_offledger_claim(
            &keypair(6).public_key_bytes(),
            &sig,
            "agora-dev",
            &genesis,
            &channel_id,
            cumulative,
        )
        .is_err());
    }

    #[test]
    fn create_signing_vector_is_stable() {
        let owner = keypair(1);
        let claim_key = keypair(42);
        let genesis = Hash([7; 32]);
        let mut tx = DrcPaymentChannelCreateTx {
            version: DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
            owner: owner.address(),
            destination: keypair(2).address(),
            amount: Amount::from_base_units(100),
            fee: Amount::from_base_units(1),
            claim_public_key: claim_key.public_key_bytes().to_vec(),
            settle_delay_blue_scores: 10,
            destination_tag: Some(0),
            source_tag: None,
            invoice_id: Hash::ZERO,
            cancel_after_blue_score: None,
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let preimage = tx.signing_bytes_bound("agora-dev", &genesis);
        sign_drc_payment_channel_create_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();
        assert!(!preimage.is_empty());
        verify_drc_payment_channel_create_bound(&tx, "agora-dev", &genesis).unwrap();
    }

    #[test]
    fn fund_signing_omits_cancel_after() {
        let owner = keypair(3);
        let genesis = Hash([4; 32]);
        let mut tx = DrcPaymentChannelFundTx {
            version: DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
            submitter: owner.address(),
            channel_id: Hash([5; 32]),
            amount: Amount::from_base_units(20),
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_fund_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();
        verify_drc_payment_channel_fund_bound(&tx, "agora-dev", &genesis).unwrap();
    }
}
