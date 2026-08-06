//! Sign / verify Trident signed stake transactions.

use agora_types::{Hash, SignedStakeTx};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, SignatureBytes};

pub fn sign_stake_tx_bound(
    tx: &mut SignedStakeTx,
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

pub fn verify_stake_tx_bound(
    tx: &SignedStakeTx,
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
    if signer != tx.actor {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agora_types::{NativeAssetId, SignedStakeTx};

    use super::*;
    use crate::mnemonic::{generate_mnemonic, seed_from_mnemonic};
    use crate::KeyPair;

    #[test]
    fn stake_tx_sign_verify_roundtrip() {
        let phrase = generate_mnemonic().unwrap();
        let seed = seed_from_mnemonic(&phrase, "").unwrap();
        let kp = KeyPair::from_seed(&seed).unwrap();
        let genesis = Hash([7u8; 32]);
        let mut tx = SignedStakeTx::unsigned_bond(
            NativeAssetId::OVL,
            kp.address(),
            1_000,
            kp.public_key_bytes().to_vec(),
            kp.address(),
            100,
            0,
        );
        sign_stake_tx_bound(&mut tx, &kp, "agora-dev", &genesis).unwrap();
        verify_stake_tx_bound(&tx, "agora-dev", &genesis).unwrap();
        assert!(verify_stake_tx_bound(&tx, "agora-testnet-1", &genesis).is_err());
    }
}
