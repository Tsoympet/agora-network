//! Sign and verify network-bound Hub / Grant / Mission registrations.

use agora_types::{GrantRegistration, Hash, HubRegistration, MissionRegistration};

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

fn verify_bound_envelope(
    public_key: &[u8],
    signature: &[u8],
    expected_address: agora_types::Address,
    signing_bytes: &[u8],
) -> Result<(), CryptoError> {
    if public_key.len() != 33 || signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut pk: PublicKeyBytes = [0; 33];
    pk.copy_from_slice(public_key);
    let mut sig: SignatureBytes = [0; 64];
    sig.copy_from_slice(signature);
    KeyPair::verify(&pk, signing_bytes, &sig)?;
    if address_from_pubkey(&pk) != expected_address {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    Ok(())
}

pub fn sign_hub_registration_bound(
    registration: &mut HubRegistration,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    let signer = registration
        .first_coordinator()
        .ok_or(CryptoError::InvalidTransactionAuth)?;
    if keypair.address() != signer {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let signature = keypair.sign(&registration.signing_bytes_bound(chain_id, genesis))?;
    registration.public_key = keypair.public_key_bytes().to_vec();
    registration.signature = signature.to_vec();
    Ok(())
}

pub fn verify_hub_registration_bound(
    registration: &HubRegistration,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    let signer = registration
        .first_coordinator()
        .ok_or(CryptoError::InvalidTransactionAuth)?;
    verify_bound_envelope(
        &registration.public_key,
        &registration.signature,
        signer,
        &registration.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn sign_grant_registration_bound(
    registration: &mut GrantRegistration,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    if keypair.address() != registration.registrar {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let signature = keypair.sign(&registration.signing_bytes_bound(chain_id, genesis))?;
    registration.public_key = keypair.public_key_bytes().to_vec();
    registration.signature = signature.to_vec();
    Ok(())
}

pub fn verify_grant_registration_bound(
    registration: &GrantRegistration,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    verify_bound_envelope(
        &registration.public_key,
        &registration.signature,
        registration.registrar,
        &registration.signing_bytes_bound(chain_id, genesis),
    )
}

pub fn sign_mission_registration_bound(
    registration: &mut MissionRegistration,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    if keypair.address() != registration.sponsor {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let signature = keypair.sign(&registration.signing_bytes_bound(chain_id, genesis))?;
    registration.public_key = keypair.public_key_bytes().to_vec();
    registration.signature = signature.to_vec();
    Ok(())
}

pub fn verify_mission_registration_bound(
    registration: &MissionRegistration,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    verify_bound_envelope(
        &registration.public_key,
        &registration.signature,
        registration.sponsor,
        &registration.signing_bytes_bound(chain_id, genesis),
    )
}

#[cfg(test)]
mod tests {
    use agora_types::{Address, Amount, CommunityGrantKind, Hash, TreasuryId};

    use super::*;

    #[test]
    fn community_signatures_reject_cross_chain_and_field_edits() {
        let keypair = KeyPair::from_secret_bytes(&[1; 32]).unwrap();
        let genesis = Hash([7; 32]);
        let mut hub = HubRegistration::unsigned(
            "Agora Hub".into(),
            "Geographic".into(),
            Hash([2; 32]),
            vec![keypair.address()],
            Address([4; 20]),
            12,
            3,
            Hash([5; 32]),
            Hash([6; 32]),
            1,
            0,
        );
        sign_hub_registration_bound(&mut hub, &keypair, "agora-dev", &genesis).unwrap();
        verify_hub_registration_bound(&hub, "agora-dev", &genesis).unwrap();
        assert!(verify_hub_registration_bound(&hub, "agora-testnet", &genesis).is_err());

        let mut grant = GrantRegistration::unsigned(
            keypair.address(),
            7,
            TreasuryId::OvlBuilder,
            Address([2; 20]),
            Amount::from_base_units(10),
            CommunityGrantKind::Micro,
            vec![],
            Hash::ZERO,
            0,
        );
        sign_grant_registration_bound(&mut grant, &keypair, "agora-dev", &genesis).unwrap();
        verify_grant_registration_bound(&grant, "agora-dev", &genesis).unwrap();
        let mut changed = grant.clone();
        changed.beneficiary = Address([9; 20]);
        assert!(verify_grant_registration_bound(&changed, "agora-dev", &genesis).is_err());

        let mut mission = MissionRegistration::unsigned(
            keypair.address(),
            TreasuryId::DrcCommunity,
            Amount::from_base_units(5),
            Hash([3; 32]),
            0,
        );
        sign_mission_registration_bound(&mut mission, &keypair, "agora-dev", &genesis).unwrap();
        verify_mission_registration_bound(&mission, "agora-dev", &genesis).unwrap();
        assert!(verify_mission_registration_bound(&mission, "agora-dev", &Hash([8; 32])).is_err());
    }

    #[test]
    fn hub_signing_rejects_non_coordinator_key() {
        let keypair = KeyPair::from_secret_bytes(&[1; 32]).unwrap();
        let mut hub = HubRegistration::unsigned(
            "Agora Hub".into(),
            "Geographic".into(),
            Hash([2; 32]),
            vec![Address::ZERO],
            Address([4; 20]),
            12,
            3,
            Hash([5; 32]),
            Hash([6; 32]),
            1,
            0,
        );
        assert!(sign_hub_registration_bound(&mut hub, &keypair, "agora-dev", &Hash::ZERO).is_err());
        assert!(hub.public_key.is_empty());
        assert!(hub.signature.is_empty());
    }
}
