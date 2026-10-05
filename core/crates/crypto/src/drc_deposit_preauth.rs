//! Sign and verify recipient-controlled DRC deposit preauthorizations.

use agora_types::{DrcDepositPreauthTx, Hash};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

pub fn sign_drc_deposit_preauth_bound(
    tx: &mut DrcDepositPreauthTx,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
        .map_err(|_| CryptoError::InvalidTransactionAuth)?;
    if keypair.address() != tx.owner {
        return Err(CryptoError::InvalidTransactionAuth);
    }

    let signature = keypair.sign(&tx.signing_bytes_bound(chain_id, genesis))?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

pub fn verify_drc_deposit_preauth_bound(
    tx: &DrcDepositPreauthTx,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    tx.validate_structure()
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
    if address_from_pubkey(&public_key) != tx.owner {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agora_types::{Amount, DrcDepositPreauthAction};

    use super::*;

    fn keypair(byte: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[byte; 32]).expect("valid deterministic secret")
    }

    #[test]
    fn owner_signature_binds_network_type_action_source_nonce_and_fee() {
        let owner = keypair(1);
        let source = keypair(2);
        let genesis = Hash([7; 32]);
        let mut tx = DrcDepositPreauthTx::authorize(
            owner.address(),
            source.address(),
            Amount::from_base_units(3),
            4,
        );
        sign_drc_deposit_preauth_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();
        verify_drc_deposit_preauth_bound(&tx, "agora-dev", &genesis).unwrap();

        for tampered in [
            {
                let mut value = tx.clone();
                value.action = DrcDepositPreauthAction::Unauthorize;
                value
            },
            {
                let mut value = tx.clone();
                value.authorized_source = keypair(3).address();
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
                value.owner = keypair(4).address();
                value
            },
        ] {
            assert!(verify_drc_deposit_preauth_bound(&tampered, "agora-dev", &genesis).is_err());
        }
        assert!(verify_drc_deposit_preauth_bound(&tx, "agora-testnet-1", &genesis).is_err());
        assert!(verify_drc_deposit_preauth_bound(&tx, "agora-dev", &Hash([8; 32])).is_err());
    }

    #[test]
    fn deposit_preauth_signing_vector_is_stable() {
        let owner = keypair(1);
        let source = keypair(2);
        let genesis = Hash([7; 32]);
        let mut tx = DrcDepositPreauthTx::authorize(
            owner.address(),
            source.address(),
            Amount::from_base_units(3),
            4,
        );
        let preimage = tx.signing_bytes_bound("agora-dev", &genesis);
        sign_drc_deposit_preauth_bound(&mut tx, &owner, "agora-dev", &genesis).unwrap();

        assert_eq!(
            (
                hex::encode(preimage),
                hex::encode(&tx.public_key),
                hex::encode(&tx.signature),
            ),
            (
                "2400000061676f72612d74726964656e742d6472632d6465706f7369742d707265617574682d76310900000061676f72612d6465760707070707070707070707070707070707070707070707070707070707070707130000006472635f6465706f7369745f7072656175746801000000f1d12012406b87afb27f6dd16ac0a76fcdaa55ed0080a9f99957b29af20338037cf06360bc55422e5b04000000000000000300000000000000".to_owned(),
                "031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f".to_owned(),
                "8353ed9dbbf3321a0da49bf2f10abbb17a87a7ff2e4ce487df55fa373d75d5ce607faa9645ea21426e2bed87de207efe2ae26aee81337e6b564d94a07cbba66c".to_owned(),
            )
        );
    }

    #[test]
    fn wrong_owner_self_zero_and_future_versions_are_rejected() {
        let owner = keypair(1);
        let other = keypair(2);
        let mut wrong_owner =
            DrcDepositPreauthTx::authorize(owner.address(), other.address(), Amount::ZERO, 0);
        assert!(
            sign_drc_deposit_preauth_bound(&mut wrong_owner, &other, "agora-dev", &Hash::ZERO)
                .is_err()
        );
        assert!(wrong_owner.public_key.is_empty());

        for mut malformed in [
            DrcDepositPreauthTx {
                version: 0,
                ..wrong_owner.clone()
            },
            DrcDepositPreauthTx {
                version: agora_types::DRC_DEPOSIT_PREAUTH_TX_VERSION + 1,
                ..wrong_owner.clone()
            },
            DrcDepositPreauthTx {
                authorized_source: owner.address(),
                ..wrong_owner.clone()
            },
            DrcDepositPreauthTx {
                authorized_source: agora_types::Address::ZERO,
                ..wrong_owner
            },
        ] {
            assert!(sign_drc_deposit_preauth_bound(
                &mut malformed,
                &owner,
                "agora-dev",
                &Hash::ZERO
            )
            .is_err());
        }
    }
}
