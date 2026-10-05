//! Sign and verify native DRC check operations.

use agora_types::{DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx, Hash};

use crate::{CryptoError, KeyPair};

pub fn sign_drc_check_create_bound(
    tx: &mut DrcCheckCreateTx,
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

pub fn verify_drc_check_create_bound(
    tx: &DrcCheckCreateTx,
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

pub fn sign_drc_check_cash_bound(
    tx: &mut DrcCheckCashTx,
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

pub fn verify_drc_check_cash_bound(
    tx: &DrcCheckCashTx,
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

pub fn sign_drc_check_cancel_bound(
    tx: &mut DrcCheckCancelTx,
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

pub fn verify_drc_check_cancel_bound(
    tx: &DrcCheckCancelTx,
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
    use agora_types::{Amount, DrcCheckCreateTx, DRC_CHECK_CREATE_TX_VERSION};

    use super::*;

    fn keypair(byte: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[byte; 32]).expect("valid deterministic secret")
    }

    #[test]
    fn drc_check_create_signing_vector_is_stable() {
        let owner = keypair(1);
        let genesis = Hash([7; 32]);
        let mut tx = DrcCheckCreateTx {
            version: DRC_CHECK_CREATE_TX_VERSION,
            owner: owner.address(),
            destination: agora_types::Address([2; 20]),
            amount: Amount::from_base_units(5),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            expires_after_blue_score: Some(100),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let preimage = tx.signing_bytes_bound("agora-dev", &genesis);
        sign_drc_check_create_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();
        let got = (
            hex::encode(preimage),
            hex::encode(&tx.public_key),
            hex::encode(&tx.signature),
        );
        assert_eq!(
            got,
            (
                "2100000061676f72612d74726964656e742d6472632d636865636b2d6372656174652d76310900000061676f72612d6465760707070707070707070707070707070707070707070707070707070707070707100000006472635f636865636b5f63726561746501000000f1d12012406b87afb27f6dd16ac0a76fcdaa55ed020202020202020202020202020202020202020205000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000164000000000000000000000000000000".to_owned(),
                "031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f".to_owned(),
                "3ca6450a60971ce123bf699018dd4d674fd99cc6e355bb588c6f825ad12123b84e9eaf5377b557d2cc1e991255e4917108823476f96537a580d73f40c57b7033".to_owned(),
            )
        );
    }
}
