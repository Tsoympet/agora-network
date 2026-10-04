//! Validate, canonicalize, and merge the consensus `drc_multisign_attachments` lane.

use std::collections::BTreeMap;

use crate::{
    attachment_key_for_account_transfer, attachment_key_for_deposit_preauth,
    attachment_key_for_payment, attachment_key_for_policy, attachment_key_for_regular_key,
    attachment_key_for_signer_list, attachment_key_for_stake, Address, Block,
    DrcMultisignAttachmentError, DrcMultisignAttachmentKey, DrcMultisignAuth,
    DrcMultisignBlockAttachment, Hash, NativeAssetId, DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
};

fn single_sig_present(public_key: &[u8], signature: &[u8]) -> bool {
    !public_key.is_empty() || !signature.is_empty()
}

fn needs_attachment(
    public_key: &[u8],
    signature: &[u8],
    multisign: &Option<DrcMultisignAuth>,
) -> bool {
    if multisign.is_some() {
        return true;
    }
    public_key.is_empty() && signature.is_empty()
}

/// Upper bound: one attachment per in-scope DRC operation in the body.
pub fn drc_multisign_attachment_capacity(block: &Block) -> usize {
    block
        .account_transfers
        .iter()
        .filter(|tx| tx.asset == NativeAssetId::DRC)
        .count()
        + block
            .stake_ops
            .iter()
            .filter(|tx| tx.asset == NativeAssetId::DRC)
            .count()
        + block.drc_regular_keys.len()
        + block.drc_signer_lists.len()
        + block.drc_account_policies.len()
        + block.drc_deposit_preauths.len()
        + block.drc_payments.len()
}

fn ensure_sorted(keys: &[DrcMultisignAttachmentKey]) -> Result<(), DrcMultisignAttachmentError> {
    for pair in keys.windows(2) {
        if pair[0] >= pair[1] {
            return Err(DrcMultisignAttachmentError::UnsortedAttachments);
        }
    }
    Ok(())
}

fn reject_inline_multisign(
    multisign: &Option<DrcMultisignAuth>,
) -> Result<(), DrcMultisignAttachmentError> {
    if multisign.is_some() {
        return Err(DrcMultisignAttachmentError::InlineMultisignForbidden);
    }
    Ok(())
}

fn check_owner_signing_for(
    owner: Address,
    auth: &DrcMultisignAuth,
) -> Result<(), DrcMultisignAttachmentError> {
    if auth.signing_for != owner {
        return Err(DrcMultisignAttachmentError::SigningForMismatch);
    }
    Ok(())
}

fn push_materialized(
    attachments: &mut Vec<DrcMultisignBlockAttachment>,
    key: DrcMultisignAttachmentKey,
    owner: Address,
    auth: DrcMultisignAuth,
) -> Result<(), DrcMultisignAttachmentError> {
    check_owner_signing_for(owner, &auth)?;
    attachments.push(DrcMultisignBlockAttachment {
        version: DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
        key,
        auth,
    });
    Ok(())
}

/// Move mempool/JSON inline multisign into the detached lane and strip inline fields.
pub fn materialize_drc_multisign_attachments(
    block: &mut Block,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), DrcMultisignAttachmentError> {
    let mut attachments = Vec::new();

    for tx in &mut block.account_transfers {
        if tx.asset != NativeAssetId::DRC {
            if tx.multisign.is_some() {
                return Err(DrcMultisignAttachmentError::WrongOperationKind);
            }
            continue;
        }
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_account_transfer(tx, chain_id, genesis)?;
            push_materialized(&mut attachments, key, tx.from, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.stake_ops {
        if tx.asset != NativeAssetId::DRC {
            if tx.multisign.is_some() {
                return Err(DrcMultisignAttachmentError::WrongOperationKind);
            }
            continue;
        }
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_stake(tx, chain_id, genesis)?;
            push_materialized(&mut attachments, key, tx.actor, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_regular_keys {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_regular_key(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.owner, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_signer_lists {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_signer_list(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.owner, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_account_policies {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_policy(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.account, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_deposit_preauths {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_deposit_preauth(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.owner, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_payments {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_payment(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.from, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    attachments.sort_by_key(|a| a.key);
    ensure_sorted(&attachments.iter().map(|a| a.key).collect::<Vec<_>>())?;
    block.drc_multisign_attachments = attachments;
    Ok(())
}

fn collect_expected_keys(
    block: &Block,
    chain_id: &str,
    genesis: &Hash,
) -> Result<Vec<DrcMultisignAttachmentKey>, DrcMultisignAttachmentError> {
    let mut expected = Vec::new();

    for tx in &block.account_transfers {
        if tx.asset != NativeAssetId::DRC {
            reject_inline_multisign(&tx.multisign)?;
            continue;
        }
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_account_transfer(tx, chain_id, genesis)?);
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.stake_ops {
        if tx.asset != NativeAssetId::DRC {
            reject_inline_multisign(&tx.multisign)?;
            continue;
        }
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_stake(tx, chain_id, genesis)?);
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_regular_keys {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_regular_key(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_signer_lists {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_signer_list(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_account_policies {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_policy(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_deposit_preauths {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_deposit_preauth(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_payments {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_payment(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    expected.sort();
    Ok(expected)
}

/// Validate lane shape and 1:1 correspondence before any state mutation.
pub fn validate_drc_multisign_attachment_lane(
    block: &Block,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), DrcMultisignAttachmentError> {
    let capacity = drc_multisign_attachment_capacity(block);
    if block.drc_multisign_attachments.len() > capacity {
        return Err(DrcMultisignAttachmentError::TooManyAttachments);
    }

    let keys: Vec<DrcMultisignAttachmentKey> = block
        .drc_multisign_attachments
        .iter()
        .map(|a| a.key)
        .collect();
    ensure_sorted(&keys)?;
    for pair in keys.windows(2) {
        if pair[0] == pair[1] {
            return Err(DrcMultisignAttachmentError::DuplicateAttachmentKey);
        }
    }

    let expected = collect_expected_keys(block, chain_id, genesis)?;
    if expected.len() != block.drc_multisign_attachments.len() {
        return Err(if block.drc_multisign_attachments.len() > expected.len() {
            DrcMultisignAttachmentError::OrphanAttachment
        } else {
            DrcMultisignAttachmentError::MissingAttachment
        });
    }

    for (attachment, key) in block.drc_multisign_attachments.iter().zip(expected.iter()) {
        attachment.validate()?;
        if attachment.key != *key {
            return Err(DrcMultisignAttachmentError::WrongOperationKind);
        }
        let owner = owner_for_key(block, *key, chain_id, genesis)?;
        check_owner_signing_for(owner, &attachment.auth)?;
    }

    Ok(())
}

fn owner_for_key(
    block: &Block,
    key: DrcMultisignAttachmentKey,
    chain_id: &str,
    genesis: &Hash,
) -> Result<Address, DrcMultisignAttachmentError> {
    for tx in &block.account_transfers {
        if tx.asset == NativeAssetId::DRC
            && attachment_key_for_account_transfer(tx, chain_id, genesis)? == key
        {
            return Ok(tx.from);
        }
    }
    for tx in &block.stake_ops {
        if tx.asset == NativeAssetId::DRC && attachment_key_for_stake(tx, chain_id, genesis)? == key
        {
            return Ok(tx.actor);
        }
    }
    for tx in &block.drc_regular_keys {
        if attachment_key_for_regular_key(tx, chain_id, genesis) == key {
            return Ok(tx.owner);
        }
    }
    for tx in &block.drc_signer_lists {
        if attachment_key_for_signer_list(tx, chain_id, genesis) == key {
            return Ok(tx.owner);
        }
    }
    for tx in &block.drc_account_policies {
        if attachment_key_for_policy(tx, chain_id, genesis) == key {
            return Ok(tx.account);
        }
    }
    for tx in &block.drc_deposit_preauths {
        if attachment_key_for_deposit_preauth(tx, chain_id, genesis) == key {
            return Ok(tx.owner);
        }
    }
    for tx in &block.drc_payments {
        if attachment_key_for_payment(tx, chain_id, genesis) == key {
            return Ok(tx.from);
        }
    }
    Err(DrcMultisignAttachmentError::OrphanAttachment)
}

/// Apply validated attachments onto operation envelopes for authorization verification.
pub fn merge_drc_multisign_attachments(
    mut block: Block,
    chain_id: &str,
    genesis: &Hash,
) -> Result<Block, DrcMultisignAttachmentError> {
    validate_drc_multisign_attachment_lane(&block, chain_id, genesis)?;
    if block.drc_multisign_attachments.is_empty() {
        return Ok(block);
    }

    let map: BTreeMap<DrcMultisignAttachmentKey, DrcMultisignAuth> = block
        .drc_multisign_attachments
        .iter()
        .map(|a| (a.key, a.auth.clone()))
        .collect();

    for tx in &mut block.account_transfers {
        if tx.asset != NativeAssetId::DRC || !needs_attachment(&tx.public_key, &tx.signature, &None)
        {
            continue;
        }
        let key = attachment_key_for_account_transfer(tx, chain_id, genesis)?;
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.stake_ops {
        if tx.asset != NativeAssetId::DRC || !needs_attachment(&tx.public_key, &tx.signature, &None)
        {
            continue;
        }
        let key = attachment_key_for_stake(tx, chain_id, genesis)?;
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_regular_keys {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_regular_key(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_signer_lists {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_signer_list(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_account_policies {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_policy(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_deposit_preauths {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_deposit_preauth(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_payments {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_payment(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    Ok(block)
}
