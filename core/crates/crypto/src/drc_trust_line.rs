//! Sign and verify trust line set and issued transfer operations.

use agora_types::{DrcIssuedTransferTx, DrcTrustLineSetTx, Hash};

use crate::{CryptoError, KeyPair};

pub fn sign_drc_trust_line_set_bound(
    tx: &mut DrcTrustLineSetTx,
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

pub fn verify_drc_trust_line_set_bound(
    tx: &DrcTrustLineSetTx,
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

pub fn sign_drc_issued_transfer_bound(
    tx: &mut DrcIssuedTransferTx,
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

pub fn verify_drc_issued_transfer_bound(
    tx: &DrcIssuedTransferTx,
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
mod signing_vectors {
    use agora_types::{DrcIssuedTransferTx, DrcTrustLineSetTx, Hash, IssuedAmount};

    use super::{
        sign_drc_issued_transfer_bound, sign_drc_trust_line_set_bound,
        verify_drc_issued_transfer_bound, verify_drc_trust_line_set_bound, KeyPair,
    };

    const CHAIN: &str = "vector-chain";
    const GENESIS: Hash = Hash([0x11; 32]);

    #[test]
    fn trust_line_set_and_issued_transfer_signing_domains_distinct() {
        let holder = KeyPair::from_secret_bytes(&[0x01; 32]).unwrap();
        let issuer = KeyPair::from_secret_bytes(&[0x02; 32]).unwrap();
        let currency = agora_types::IssuedCurrencyCode(*b"VEC\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        let mut set = DrcTrustLineSetTx {
            version: agora_types::DRC_TRUST_LINE_SET_TX_VERSION,
            holder: holder.address(),
            issuer: issuer.address(),
            currency,
            limit: IssuedAmount::from_units(1),
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let set_bytes = set.signing_bytes_bound(CHAIN, &GENESIS);
        sign_drc_trust_line_set_bound(&mut set, &holder, CHAIN, &GENESIS).unwrap();
        verify_drc_trust_line_set_bound(&set, CHAIN, &GENESIS).unwrap();

        let mut xfer = DrcIssuedTransferTx {
            version: agora_types::DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            sender: issuer.address(),
            recipient: holder.address(),
            issuer: issuer.address(),
            currency,
            amount: IssuedAmount::from_units(1),
            fee: agora_types::Amount::from_base_units(1),
            destination_tag: Some(0),
            source_tag: Some(1),
            invoice_id: Hash::ZERO,
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let xfer_bytes = xfer.signing_bytes_bound(CHAIN, &GENESIS);
        assert_ne!(set_bytes, xfer_bytes);
        sign_drc_issued_transfer_bound(&mut xfer, &issuer, CHAIN, &GENESIS).unwrap();
        verify_drc_issued_transfer_bound(&xfer, CHAIN, &GENESIS).unwrap();
    }
}
