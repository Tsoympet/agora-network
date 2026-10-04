//! Sign and verify DRC signer-list operations.

use agora_types::{DrcSignerListTx, Hash};

use crate::drc_operation::verify_bound_secp256k1;
use crate::{CryptoError, KeyPair};

pub fn sign_drc_signer_list_bound(
    tx: &mut DrcSignerListTx,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    let signature = keypair.sign(&tx.signing_bytes_bound(chain_id, genesis))?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    tx.multisign = None;
    Ok(())
}

pub fn verify_drc_signer_list_single_signature_bound(
    tx: &DrcSignerListTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<agora_types::Address, CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    verify_bound_secp256k1(
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(chain_id, genesis),
    )
}
