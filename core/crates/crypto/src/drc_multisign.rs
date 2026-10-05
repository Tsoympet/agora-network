//! Multisign participant signing and bundle verification.

use agora_types::{
    validate_exclusive_authorization, Address, DrcMultisignAuth, DrcSignerListEntry, Hash,
};

use crate::drc_operation::verify_bound_secp256k1;
use crate::{CryptoError, KeyPair};

pub fn sign_drc_multisign_participant_bound(
    signing_for: Address,
    operation_signing_bytes: &[u8],
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(Address, Vec<u8>, Vec<u8>), CryptoError> {
    let bytes = DrcMultisignAuth::participant_signing_bytes_bound(
        signing_for,
        operation_signing_bytes,
        chain_id,
        genesis,
    );
    let signature = keypair.sign(&bytes)?;
    Ok((
        keypair.address(),
        keypair.public_key_bytes().to_vec(),
        signature.to_vec(),
    ))
}

pub fn verify_drc_multisign_against_list(
    auth: &DrcMultisignAuth,
    signing_for: Address,
    operation_signing_bytes: &[u8],
    chain_id: &str,
    genesis: &Hash,
    entries: &[DrcSignerListEntry],
    quorum: u32,
) -> Result<(), CryptoError> {
    auth.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    if auth.signing_for != signing_for {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut weight: u64 = 0;
    for entry in &auth.signatures {
        let Some(listed) = entries.iter().find(|e| e.signer == entry.signer) else {
            return Err(CryptoError::InvalidTransactionAuth);
        };
        let participant_bytes = DrcMultisignAuth::participant_signing_bytes_bound(
            signing_for,
            operation_signing_bytes,
            chain_id,
            genesis,
        );
        let derived =
            verify_bound_secp256k1(&entry.public_key, &entry.signature, &participant_bytes)?;
        if derived != entry.signer {
            return Err(CryptoError::InvalidTransactionAuth);
        }
        weight = weight
            .checked_add(u64::from(listed.weight))
            .ok_or(CryptoError::InvalidTransactionAuth)?;
        if weight >= u64::from(quorum) {
            return Ok(());
        }
    }
    Err(CryptoError::InvalidTransactionAuth)
}

pub fn validate_drc_operation_authorization_fields(
    public_key: &[u8],
    signature: &[u8],
    multisign: &Option<DrcMultisignAuth>,
) -> Result<(), CryptoError> {
    validate_exclusive_authorization(public_key, signature, multisign)
        .map_err(|_| CryptoError::InvalidTransactionAuth)
}
