//! Sign and verify network-bound vesting unlock claims.

use agora_types::{Hash, VestingUnlock};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_vesting_unlock_bound(
    claim: &mut VestingUnlock,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    let signature = keypair.sign(&claim.signing_bytes_bound(chain_id, genesis))?;
    claim.public_key = keypair.public_key_bytes().to_vec();
    claim.signature = signature.to_vec();
    Ok(())
}

pub fn verify_vesting_unlock_bound(
    claim: &VestingUnlock,
    chain_id: &str,
    genesis: &Hash,
) -> Result<agora_types::Address, CryptoError> {
    if claim.public_key.len() != 33 || claim.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut public_key: PublicKeyBytes = [0; 33];
    public_key.copy_from_slice(&claim.public_key);
    let mut signature: SignatureBytes = [0; 64];
    signature.copy_from_slice(&claim.signature);
    KeyPair::verify(
        &public_key,
        &claim.signing_bytes_bound(chain_id, genesis),
        &signature,
    )?;
    Ok(address_from_pubkey(&public_key))
}

#[cfg(test)]
mod tests {
    use agora_types::{Address, Amount, Hash, NativeAssetId};

    use super::*;

    #[test]
    fn vesting_unlock_signature_rejects_cross_chain_and_amount_edits() {
        let keypair = KeyPair::from_secret_bytes(&[1; 32]).unwrap();
        let genesis = Hash([7; 32]);
        let mut claim = VestingUnlock::unsigned(
            NativeAssetId::OVL,
            Address([2; 20]),
            Hash([3; 32]),
            Amount::from_base_units(10),
            0,
        );
        sign_vesting_unlock_bound(&mut claim, &keypair, "agora-dev", &genesis).unwrap();
        verify_vesting_unlock_bound(&claim, "agora-dev", &genesis).unwrap();
        assert!(verify_vesting_unlock_bound(&claim, "agora-testnet", &genesis).is_err());
        let mut changed = claim.clone();
        changed.amount = Amount::from_base_units(11);
        assert!(verify_vesting_unlock_bound(&changed, "agora-dev", &genesis).is_err());
    }
}
