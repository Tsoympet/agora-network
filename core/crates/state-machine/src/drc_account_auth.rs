//! State-aware authorization for DRC account-bound operations (master + regular key).

use agora_crypto::verify_bound_secp256k1;
use agora_types::{
    AccountTransfer, Address, DrcAccountPolicyTx, DrcDepositPreauthTx, DrcPaymentTx,
    DrcRegularKeyTx, NativeAssetId, SignedStakeTx,
};

use crate::apply::TxAuthContext;
use crate::drc_regular_key::load_drc_account_regular_key;
use crate::{StateError, StateStore};

/// Whether `signer` may act for `owner` on DRC account lanes.
pub fn authorize_drc_account_operator(
    store: &StateStore,
    owner: &Address,
    signer: &Address,
) -> Result<(), StateError> {
    if signer == owner {
        return Ok(());
    }
    let Some(regular_key) = load_drc_account_regular_key(store, owner)? else {
        return Err(StateError::InvalidTx(
            "DRC operation signer is not master or regular key".into(),
        ));
    };
    if signer == &regular_key {
        Ok(())
    } else {
        Err(StateError::InvalidTx(
            "DRC operation signer is not master or regular key".into(),
        ))
    }
}

fn authorize_after_bound_signature(
    store: &StateStore,
    owner: &Address,
    public_key: &[u8],
    signature: &[u8],
    signing_bytes: &[u8],
) -> Result<(), StateError> {
    let signer = verify_bound_secp256k1(public_key, signature, signing_bytes)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    authorize_drc_account_operator(store, owner, &signer)
}

/// Verify payment envelope crypto + master/regular authorization.
pub fn verify_drc_payment_operation(
    store: &StateStore,
    tx: &DrcPaymentTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_envelope_version()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    authorize_after_bound_signature(
        store,
        &tx.from,
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
    )
}

pub fn verify_drc_account_policy_operation(
    store: &StateStore,
    tx: &DrcAccountPolicyTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_version()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    authorize_after_bound_signature(
        store,
        &tx.account,
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
    )
}

pub fn verify_drc_deposit_preauth_operation(
    store: &StateStore,
    tx: &DrcDepositPreauthTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    authorize_after_bound_signature(
        store,
        &tx.owner,
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
    )
}

/// Regular-key rotation may be signed by the master key or the currently installed regular key.
pub fn verify_drc_regular_key_operation(
    store: &StateStore,
    tx: &DrcRegularKeyTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    agora_crypto::verify_drc_regular_key_bound(tx, &auth.chain_id, &auth.genesis)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    let signer = verify_bound_secp256k1(
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
    )
    .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    if signer == tx.owner {
        return Ok(());
    }
    let Some(regular_key) = load_drc_account_regular_key(store, &tx.owner)? else {
        return Err(StateError::InvalidTx(
            "regular-key operation requires master or installed regular key".into(),
        ));
    };
    if signer == regular_key {
        Ok(())
    } else {
        Err(StateError::InvalidTx(
            "regular-key operation requires master or installed regular key".into(),
        ))
    }
}

/// DRC account transfers accept master or regular-key signatures; OVL remains master-only.
pub fn verify_drc_account_transfer_operation(
    store: &StateStore,
    tx: &AccountTransfer,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    if tx.asset != NativeAssetId::DRC {
        agora_crypto::verify_account_transfer_bound(tx, &auth.chain_id, &auth.genesis)
            .map_err(|error| StateError::InvalidTx(error.to_string()))?;
        return Ok(());
    }
    authorize_after_bound_signature(
        store,
        &tx.from,
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
    )
}

pub fn verify_drc_stake_operation(
    store: &StateStore,
    tx: &SignedStakeTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    if tx.asset != NativeAssetId::DRC {
        agora_crypto::verify_stake_tx_bound(tx, &auth.chain_id, &auth.genesis)
            .map_err(|error| StateError::InvalidTx(error.to_string()))?;
        return Ok(());
    }
    authorize_after_bound_signature(
        store,
        &tx.actor,
        &tx.public_key,
        &tx.signature,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
    )
}
