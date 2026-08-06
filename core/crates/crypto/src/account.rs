//! Sign / verify Trident native account transfers (OVL / DRC).

use agora_types::{AccountTransfer, Hash};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_account_transfer_bound(
    tx: &mut AccountTransfer,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    let signing = tx.signing_bytes_bound(chain_id, genesis);
    let signature = keypair.sign(&signing)?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

pub fn verify_account_transfer_bound(
    tx: &AccountTransfer,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    if tx.public_key.len() != 33 || tx.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut pubkey = [0u8; 33];
    pubkey.copy_from_slice(&tx.public_key);
    let mut signature: SignatureBytes = [0u8; 64];
    signature.copy_from_slice(&tx.signature);
    KeyPair::verify(
        &pubkey,
        &tx.signing_bytes_bound(chain_id, genesis),
        &signature,
    )?;
    let signer = address_from_pubkey(&pubkey);
    if signer != tx.from {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

pub fn account_signer_address(tx: &AccountTransfer) -> Result<agora_types::Address, CryptoError> {
    if tx.public_key.len() != 33 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut pubkey: PublicKeyBytes = [0u8; 33];
    pubkey.copy_from_slice(&tx.public_key);
    Ok(address_from_pubkey(&pubkey))
}
