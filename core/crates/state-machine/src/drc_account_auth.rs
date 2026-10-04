//! State-aware authorization for DRC account-bound operations (master, regular key, multisign).

use agora_crypto::{
    validate_drc_operation_authorization_fields, verify_bound_secp256k1,
    verify_drc_multisign_against_list, verify_drc_signer_list_single_signature_bound,
};
use agora_types::{
    validate_exclusive_authorization, AccountTransfer, Address, DrcAccountPolicyAction,
    DrcAccountPolicyTx, DrcDepositPreauthTx, DrcMultisignAuth, DrcPaymentTx, DrcRegularKeyTx,
    DrcSignerListTx, DrcTicketCreateTx, NativeAssetId, SignedStakeTx,
};

use crate::apply::TxAuthContext;
use crate::drc_master_key_recovery::{drc_master_key_disabled, has_alternate_recovery};
use crate::drc_regular_key::load_drc_account_regular_key;
use crate::drc_signer_list::load_drc_account_signer_list;
use crate::{StateError, StateStore};

/// Whether `signer` may act for `owner` on DRC account lanes via master or regular key.
pub fn authorize_drc_account_operator(
    store: &StateStore,
    owner: &Address,
    signer: &Address,
) -> Result<(), StateError> {
    if signer == owner {
        if drc_master_key_disabled(store, owner)? {
            return Err(StateError::InvalidTx(
                "DRC master key is disabled for this account".into(),
            ));
        }
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

#[allow(clippy::too_many_arguments)]
fn verify_multisign_or_single(
    store: &StateStore,
    owner: &Address,
    public_key: &[u8],
    signature: &[u8],
    multisign: &Option<DrcMultisignAuth>,
    operation_signing_bytes: &[u8],
    auth: &TxAuthContext,
    allow_current_signer_list: bool,
    forbidden_new_list_entries: Option<&[agora_types::DrcSignerListEntry]>,
) -> Result<(), StateError> {
    let _ = allow_current_signer_list;
    validate_exclusive_authorization(public_key, signature, multisign)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    if let Some(bundle) = multisign {
        bundle
            .validate_structure()
            .map_err(|error| StateError::InvalidTx(error.to_string()))?;
        if bundle.signing_for != *owner {
            return Err(StateError::InvalidTx(
                "multisign signing_for mismatch".into(),
            ));
        }
        let Some(list) = load_drc_account_signer_list(store, owner)? else {
            return Err(StateError::InvalidTx(
                "multisign requires an installed signer list".into(),
            ));
        };
        if let Some(new_entries) = forbidden_new_list_entries {
            for entry in &bundle.signatures {
                if new_entries.iter().any(|e| e.signer == entry.signer)
                    && !list.entries.iter().any(|e| e.signer == entry.signer)
                {
                    return Err(StateError::InvalidTx(
                        "new signer list cannot authorize its own installation".into(),
                    ));
                }
            }
        }
        verify_drc_multisign_against_list(
            bundle,
            *owner,
            operation_signing_bytes,
            &auth.chain_id,
            &auth.genesis,
            &list.entries,
            list.quorum,
        )
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
        return Ok(());
    }
    let signer = verify_bound_secp256k1(public_key, signature, operation_signing_bytes)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    if signer == *owner && drc_master_key_disabled(store, owner)? {
        return Err(StateError::InvalidTx(
            "DRC master key is disabled for this account".into(),
        ));
    }
    authorize_drc_account_operator(store, owner, &signer)
}

/// Verify payment envelope crypto + master/regular/multisign authorization.
pub fn verify_drc_payment_operation(
    store: &StateStore,
    tx: &DrcPaymentTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_envelope_version()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    verify_multisign_or_single(
        store,
        &tx.from,
        &tx.public_key,
        &tx.signature,
        &tx.multisign,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
        auth,
        true,
        None,
    )
}

pub fn verify_drc_account_policy_operation(
    store: &StateStore,
    tx: &DrcAccountPolicyTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_version()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    let signing_bytes = tx.signing_bytes_bound(&auth.chain_id, &auth.genesis);
    match tx.action {
        DrcAccountPolicyAction::SetMasterKeyDisabled => {
            validate_exclusive_authorization(&tx.public_key, &tx.signature, &tx.multisign)
                .map_err(|error| StateError::InvalidTx(error.to_string()))?;
            if tx.multisign.is_some() {
                return Err(StateError::InvalidTx(
                    "master-key disable must be authorized by the owner master key".into(),
                ));
            }
            let signer = verify_bound_secp256k1(&tx.public_key, &tx.signature, &signing_bytes)
                .map_err(|error| StateError::InvalidTx(error.to_string()))?;
            if signer != tx.account {
                return Err(StateError::InvalidTx(
                    "master-key disable must be authorized by the owner master key".into(),
                ));
            }
            if drc_master_key_disabled(store, &tx.account)? {
                return Err(StateError::InvalidTx(
                    "DRC master key is already disabled".into(),
                ));
            }
            if !has_alternate_recovery(store, &tx.account)? {
                return Err(StateError::InvalidTx(
                    "master-key disable requires a live regular key or signer list".into(),
                ));
            }
            Ok(())
        }
        DrcAccountPolicyAction::ClearMasterKeyDisabled => {
            if !drc_master_key_disabled(store, &tx.account)? {
                return Err(StateError::InvalidTx(
                    "DRC master key is not disabled".into(),
                ));
            }
            validate_exclusive_authorization(&tx.public_key, &tx.signature, &tx.multisign)
                .map_err(|error| StateError::InvalidTx(error.to_string()))?;
            if let Some(bundle) = &tx.multisign {
                return verify_multisign_or_single(
                    store,
                    &tx.account,
                    &tx.public_key,
                    &tx.signature,
                    &Some(bundle.clone()),
                    &signing_bytes,
                    auth,
                    true,
                    None,
                );
            }
            let signer = verify_bound_secp256k1(&tx.public_key, &tx.signature, &signing_bytes)
                .map_err(|error| StateError::InvalidTx(error.to_string()))?;
            if signer == tx.account {
                return Err(StateError::InvalidTx(
                    "disabled DRC master key cannot clear master-key disable".into(),
                ));
            }
            authorize_drc_account_operator(store, &tx.account, &signer)
        }
        _ => verify_multisign_or_single(
            store,
            &tx.account,
            &tx.public_key,
            &tx.signature,
            &tx.multisign,
            &signing_bytes,
            auth,
            true,
            None,
        ),
    }
}

pub fn verify_drc_ticket_create_operation(
    store: &StateStore,
    tx: &DrcTicketCreateTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    validate_drc_operation_authorization_fields(&tx.public_key, &tx.signature, &tx.multisign)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    if tx.multisign.is_none() {
        agora_crypto::verify_drc_ticket_create_bound(tx, &auth.chain_id, &auth.genesis)
            .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    }
    verify_multisign_or_single(
        store,
        &tx.owner,
        &tx.public_key,
        &tx.signature,
        &tx.multisign,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
        auth,
        true,
        None,
    )
}

pub fn verify_drc_deposit_preauth_operation(
    store: &StateStore,
    tx: &DrcDepositPreauthTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    verify_multisign_or_single(
        store,
        &tx.owner,
        &tx.public_key,
        &tx.signature,
        &tx.multisign,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
        auth,
        true,
        None,
    )
}

pub fn verify_drc_regular_key_operation(
    store: &StateStore,
    tx: &DrcRegularKeyTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    validate_drc_operation_authorization_fields(&tx.public_key, &tx.signature, &tx.multisign)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    if tx.multisign.is_none() {
        agora_crypto::verify_drc_regular_key_bound(tx, &auth.chain_id, &auth.genesis)
            .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    }
    verify_multisign_or_single(
        store,
        &tx.owner,
        &tx.public_key,
        &tx.signature,
        &tx.multisign,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
        auth,
        true,
        None,
    )
}

pub fn verify_drc_signer_list_operation(
    store: &StateStore,
    tx: &DrcSignerListTx,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    validate_drc_operation_authorization_fields(&tx.public_key, &tx.signature, &tx.multisign)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    let signing_bytes = tx.signing_bytes_bound(&auth.chain_id, &auth.genesis);
    let forbidden = match tx.action {
        agora_types::DrcSignerListAction::Set => Some(tx.entries.as_slice()),
        agora_types::DrcSignerListAction::Delete => None,
    };
    if tx.multisign.is_some() {
        return verify_multisign_or_single(
            store,
            &tx.owner,
            &tx.public_key,
            &tx.signature,
            &tx.multisign,
            &signing_bytes,
            auth,
            true,
            forbidden,
        );
    }
    let signer = verify_drc_signer_list_single_signature_bound(tx, &auth.chain_id, &auth.genesis)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    authorize_drc_account_operator(store, &tx.owner, &signer)
}

/// DRC account transfers accept master, regular-key, or multisign signatures; OVL remains master-only.
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
    verify_multisign_or_single(
        store,
        &tx.from,
        &tx.public_key,
        &tx.signature,
        &tx.multisign,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
        auth,
        true,
        None,
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
    verify_multisign_or_single(
        store,
        &tx.actor,
        &tx.public_key,
        &tx.signature,
        &tx.multisign,
        &tx.signing_bytes_bound(&auth.chain_id, &auth.genesis),
        auth,
        true,
        None,
    )
}
