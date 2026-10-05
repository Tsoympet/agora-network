//! Sign and verify owner-authorized DRC account-policy operations.

use agora_types::{DrcAccountPolicyTx, Hash};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_drc_account_policy_bound(
    tx: &mut DrcAccountPolicyTx,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_version()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    if keypair.address() != tx.account {
        return Err(CryptoError::InvalidTransactionAuth);
    }

    let signature = keypair.sign(&tx.signing_bytes_bound(chain_id, genesis))?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

pub fn verify_drc_account_policy_bound(
    tx: &DrcAccountPolicyTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_version()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    if tx.public_key.len() != 33 || tx.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }

    let mut public_key: PublicKeyBytes = [0; 33];
    public_key.copy_from_slice(&tx.public_key);
    let mut signature: SignatureBytes = [0; 64];
    signature.copy_from_slice(&tx.signature);
    KeyPair::verify(
        &public_key,
        &tx.signing_bytes_bound(chain_id, genesis),
        &signature,
    )?;
    if address_from_pubkey(&public_key) != tx.account {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agora_types::{Amount, DrcAccountPolicyAction};

    use super::*;

    fn keypair(byte: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[byte; 32]).expect("valid deterministic secret")
    }

    #[test]
    fn owner_signature_binds_network_action_nonce_and_fee() {
        let owner = keypair(1);
        let genesis = Hash([7; 32]);
        let mut tx = DrcAccountPolicyTx::set_require_destination_tag(
            owner.address(),
            Amount::from_base_units(3),
            4,
        );
        sign_drc_account_policy_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();
        verify_drc_account_policy_bound(&tx, "agora-dev", &genesis).unwrap();

        for tampered in [
            {
                let mut value = tx.clone();
                value.action = DrcAccountPolicyAction::ClearRequireDestinationTag;
                value
            },
            {
                let mut value = tx.clone();
                value.nonce += 1;
                value
            },
            {
                let mut value = tx.clone();
                value.fee = Amount::from_base_units(4);
                value
            },
            {
                let mut value = tx.clone();
                value.account = keypair(2).address();
                value
            },
        ] {
            assert!(verify_drc_account_policy_bound(&tampered, "agora-dev", &genesis).is_err());
        }
        assert!(verify_drc_account_policy_bound(&tx, "agora-testnet-1", &genesis).is_err());
        assert!(verify_drc_account_policy_bound(&tx, "agora-dev", &Hash([8; 32])).is_err());
    }

    #[test]
    fn wrong_owner_and_unknown_version_are_rejected_before_signing() {
        let owner = keypair(1);
        let other = keypair(2);
        let mut wrong_owner =
            DrcAccountPolicyTx::set_require_destination_tag(owner.address(), Amount::ZERO, 0);
        assert!(
            sign_drc_account_policy_bound(&mut wrong_owner, &other, "agora-dev", &Hash::ZERO)
                .is_err()
        );
        assert!(wrong_owner.public_key.is_empty());

        let mut unsupported = wrong_owner;
        unsupported.version += 1;
        assert!(
            sign_drc_account_policy_bound(&mut unsupported, &owner, "agora-dev", &Hash::ZERO)
                .is_err()
        );
    }

    #[test]
    fn deposit_auth_v2_signature_is_domain_separated_and_action_bound() {
        let owner = keypair(1);
        let genesis = Hash([3; 32]);
        let mut tx = DrcAccountPolicyTx::set_deposit_auth_required(
            owner.address(),
            Amount::from_base_units(2),
            5,
        );
        sign_drc_account_policy_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();
        verify_drc_account_policy_bound(&tx, "agora-dev", &genesis).unwrap();

        tx.action = DrcAccountPolicyAction::ClearDepositAuthRequired;
        assert!(verify_drc_account_policy_bound(&tx, "agora-dev", &genesis).is_err());
        tx.action = DrcAccountPolicyAction::SetRequireDestinationTag;
        assert!(verify_drc_account_policy_bound(&tx, "agora-dev", &genesis).is_err());
    }
}
