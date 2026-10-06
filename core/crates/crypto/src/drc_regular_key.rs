//! Sign and verify DRC regular-key rotation operations.

use agora_types::{DrcRegularKeyTx, Hash};

use crate::address::address_from_pubkey;
use crate::drc_operation::verify_bound_secp256k1;
use crate::{CryptoError, KeyPair};

pub fn sign_drc_regular_key_bound(
    tx: &mut DrcRegularKeyTx,
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

pub fn verify_drc_regular_key_bound(
    tx: &DrcRegularKeyTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    verify_bound_secp256k1(
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(chain_id, genesis),
    )?;
    if tx.action == agora_types::DrcRegularKeyAction::Set {
        let mut pubkey = [0u8; 33];
        pubkey.copy_from_slice(&tx.regular_key_public_key);
        if address_from_pubkey(&pubkey) != tx.regular_key {
            return Err(CryptoError::InvalidTransactionAuth);
        }
    }
    Ok(())
}
