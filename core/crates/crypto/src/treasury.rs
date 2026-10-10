//! Sign and verify network-bound protocol treasury disbursements.

use agora_types::{Hash, TreasuryDisbursement};

use crate::{address::address_from_pubkey, CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_treasury_disbursement_bound(
    spend: &mut TreasuryDisbursement,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    let signature = keypair.sign(&spend.signing_bytes_bound(chain_id, genesis))?;
    spend.public_key = keypair.public_key_bytes().to_vec();
    spend.signature = signature.to_vec();
    Ok(())
}

pub fn verify_treasury_disbursement_bound(
    spend: &TreasuryDisbursement,
    chain_id: &str,
    genesis: &Hash,
) -> Result<agora_types::Address, CryptoError> {
    if spend.public_key.len() != 33 || spend.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut public_key: PublicKeyBytes = [0; 33];
    public_key.copy_from_slice(&spend.public_key);
    let mut signature: SignatureBytes = [0; 64];
    signature.copy_from_slice(&spend.signature);
    KeyPair::verify(
        &public_key,
        &spend.signing_bytes_bound(chain_id, genesis),
        &signature,
    )?;
    Ok(address_from_pubkey(&public_key))
}

#[cfg(test)]
mod tests {
    use agora_types::{Address, Amount, Hash, TreasuryId};

    use super::*;

    #[test]
    fn treasury_signature_rejects_cross_chain_and_amount_edits() {
        let keypair = KeyPair::from_secret_bytes(&[1; 32]).unwrap();
        let genesis = Hash([7; 32]);
        let mut spend = TreasuryDisbursement::unsigned(
            TreasuryId::OvlBuilder,
            Address([2; 20]),
            Amount::from_base_units(10),
            Hash([3; 32]),
            Hash([4; 32]),
            0,
        );
        sign_treasury_disbursement_bound(&mut spend, &keypair, "agora-dev", &genesis).unwrap();
        verify_treasury_disbursement_bound(&spend, "agora-dev", &genesis).unwrap();
        assert!(verify_treasury_disbursement_bound(&spend, "agora-testnet", &genesis).is_err());
        let mut changed = spend.clone();
        changed.amount = Amount::from_base_units(11);
        assert!(verify_treasury_disbursement_bound(&changed, "agora-dev", &genesis).is_err());
    }
}
