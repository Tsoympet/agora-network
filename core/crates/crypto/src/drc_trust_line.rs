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
