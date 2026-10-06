//! Validate, canonicalize, and merge the consensus `drc_multisign_attachments` lane.

use std::collections::BTreeMap;

use crate::{
    attachment_key_for_account_transfer, attachment_key_for_check_cancel,
    attachment_key_for_check_cash, attachment_key_for_check_create,
    attachment_key_for_deposit_preauth, attachment_key_for_escrow_cancel,
    attachment_key_for_escrow_create, attachment_key_for_escrow_finish,
    attachment_key_for_issued_asset_policy_set, attachment_key_for_issued_clawback,
    attachment_key_for_issued_transfer, attachment_key_for_offer_cancel,
    attachment_key_for_offer_create, attachment_key_for_payment,
    attachment_key_for_payment_channel_claim, attachment_key_for_payment_channel_close,
    attachment_key_for_payment_channel_create, attachment_key_for_payment_channel_fund,
    attachment_key_for_policy, attachment_key_for_regular_key, attachment_key_for_signer_list,
    attachment_key_for_stake, attachment_key_for_ticket_create,
    attachment_key_for_trust_line_issuer_control, attachment_key_for_trust_line_set, Address,
    Block, DrcMultisignAttachmentError, DrcMultisignAttachmentKey, DrcMultisignAuth,
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
        + block.drc_ticket_creates.len()
        + block.drc_escrow_creates.len()
        + block.drc_escrow_finishes.len()
        + block.drc_escrow_cancels.len()
        + block.drc_check_creates.len()
        + block.drc_check_cashes.len()
        + block.drc_check_cancels.len()
        + block.drc_payment_channel_creates.len()
        + block.drc_payment_channel_funds.len()
        + block.drc_payment_channel_claims.len()
        + block.drc_payment_channel_closes.len()
        + block.drc_trust_line_sets.len()
        + block.drc_issued_transfers.len()
        + block.drc_issued_asset_policy_sets.len()
        + block.drc_trust_line_issuer_controls.len()
        + block.drc_issued_clawbacks.len()
        + block.drc_offer_creates.len()
        + block.drc_offer_cancels.len()
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

    for tx in &mut block.drc_ticket_creates {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_ticket_create(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.owner, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_escrow_creates {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_escrow_create(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.owner, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_escrow_finishes {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_escrow_finish(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.submitter, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_escrow_cancels {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_escrow_cancel(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.submitter, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_check_creates {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_check_create(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.owner, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_check_cashes {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_check_cash(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.submitter, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_check_cancels {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_check_cancel(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.submitter, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_payment_channel_creates {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_payment_channel_create(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.owner, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_payment_channel_funds {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_payment_channel_fund(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.submitter, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_payment_channel_claims {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_payment_channel_claim(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.submitter, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_payment_channel_closes {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_payment_channel_close(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.submitter, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_trust_line_sets {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_trust_line_set(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.holder, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_issued_transfers {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_issued_transfer(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.sender, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_issued_asset_policy_sets {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_issued_asset_policy_set(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.issuer, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_trust_line_issuer_controls {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_trust_line_issuer_control(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.issuer, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_issued_clawbacks {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_issued_clawback(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.issuer, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_offer_creates {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_offer_create(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.owner, auth)?;
        } else if needs_attachment(&tx.public_key, &tx.signature, &None) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &mut block.drc_offer_cancels {
        if let Some(auth) = tx.multisign.take() {
            if single_sig_present(&tx.public_key, &tx.signature) {
                return Err(DrcMultisignAttachmentError::MixedAuthorization);
            }
            tx.public_key.clear();
            tx.signature.clear();
            let key = attachment_key_for_offer_cancel(tx, chain_id, genesis);
            push_materialized(&mut attachments, key, tx.submitter, auth)?;
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

    for tx in &block.drc_ticket_creates {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_ticket_create(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_escrow_creates {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_escrow_create(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_escrow_finishes {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_escrow_finish(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_escrow_cancels {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_escrow_cancel(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_check_creates {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_check_create(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_check_cashes {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_check_cash(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_check_cancels {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_check_cancel(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_payment_channel_creates {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_payment_channel_create(
                tx, chain_id, genesis,
            ));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_payment_channel_funds {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_payment_channel_fund(
                tx, chain_id, genesis,
            ));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_payment_channel_claims {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_payment_channel_claim(
                tx, chain_id, genesis,
            ));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_payment_channel_closes {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_payment_channel_close(
                tx, chain_id, genesis,
            ));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_trust_line_sets {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_trust_line_set(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_issued_transfers {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_issued_transfer(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_issued_asset_policy_sets {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_issued_asset_policy_set(
                tx, chain_id, genesis,
            ));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_trust_line_issuer_controls {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_trust_line_issuer_control(
                tx, chain_id, genesis,
            ));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_issued_clawbacks {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_issued_clawback(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_offer_creates {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_offer_create(tx, chain_id, genesis));
        } else if !single_sig_present(&tx.public_key, &tx.signature) {
            return Err(DrcMultisignAttachmentError::MissingAttachment);
        }
    }

    for tx in &block.drc_offer_cancels {
        reject_inline_multisign(&tx.multisign)?;
        if needs_attachment(&tx.public_key, &tx.signature, &None) {
            expected.push(attachment_key_for_offer_cancel(tx, chain_id, genesis));
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
    for tx in &block.drc_ticket_creates {
        if attachment_key_for_ticket_create(tx, chain_id, genesis) == key {
            return Ok(tx.owner);
        }
    }
    for tx in &block.drc_escrow_creates {
        if attachment_key_for_escrow_create(tx, chain_id, genesis) == key {
            return Ok(tx.owner);
        }
    }
    for tx in &block.drc_escrow_finishes {
        if attachment_key_for_escrow_finish(tx, chain_id, genesis) == key {
            return Ok(tx.submitter);
        }
    }
    for tx in &block.drc_escrow_cancels {
        if attachment_key_for_escrow_cancel(tx, chain_id, genesis) == key {
            return Ok(tx.submitter);
        }
    }
    for tx in &block.drc_check_creates {
        if attachment_key_for_check_create(tx, chain_id, genesis) == key {
            return Ok(tx.owner);
        }
    }
    for tx in &block.drc_check_cashes {
        if attachment_key_for_check_cash(tx, chain_id, genesis) == key {
            return Ok(tx.submitter);
        }
    }
    for tx in &block.drc_check_cancels {
        if attachment_key_for_check_cancel(tx, chain_id, genesis) == key {
            return Ok(tx.submitter);
        }
    }
    for tx in &block.drc_payment_channel_creates {
        if attachment_key_for_payment_channel_create(tx, chain_id, genesis) == key {
            return Ok(tx.owner);
        }
    }
    for tx in &block.drc_payment_channel_funds {
        if attachment_key_for_payment_channel_fund(tx, chain_id, genesis) == key {
            return Ok(tx.submitter);
        }
    }
    for tx in &block.drc_payment_channel_claims {
        if attachment_key_for_payment_channel_claim(tx, chain_id, genesis) == key {
            return Ok(tx.submitter);
        }
    }
    for tx in &block.drc_payment_channel_closes {
        if attachment_key_for_payment_channel_close(tx, chain_id, genesis) == key {
            return Ok(tx.submitter);
        }
    }
    for tx in &block.drc_trust_line_sets {
        if attachment_key_for_trust_line_set(tx, chain_id, genesis) == key {
            return Ok(tx.holder);
        }
    }
    for tx in &block.drc_issued_transfers {
        if attachment_key_for_issued_transfer(tx, chain_id, genesis) == key {
            return Ok(tx.sender);
        }
    }
    for tx in &block.drc_issued_asset_policy_sets {
        if attachment_key_for_issued_asset_policy_set(tx, chain_id, genesis) == key {
            return Ok(tx.issuer);
        }
    }
    for tx in &block.drc_trust_line_issuer_controls {
        if attachment_key_for_trust_line_issuer_control(tx, chain_id, genesis) == key {
            return Ok(tx.issuer);
        }
    }
    for tx in &block.drc_issued_clawbacks {
        if attachment_key_for_issued_clawback(tx, chain_id, genesis) == key {
            return Ok(tx.issuer);
        }
    }
    for tx in &block.drc_offer_creates {
        if attachment_key_for_offer_create(tx, chain_id, genesis) == key {
            return Ok(tx.owner);
        }
    }
    for tx in &block.drc_offer_cancels {
        if attachment_key_for_offer_cancel(tx, chain_id, genesis) == key {
            return Ok(tx.submitter);
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

    for tx in &mut block.drc_ticket_creates {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_ticket_create(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_escrow_creates {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_escrow_create(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_escrow_finishes {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_escrow_finish(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_escrow_cancels {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_escrow_cancel(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_check_creates {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_check_create(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_check_cashes {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_check_cash(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_check_cancels {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_check_cancel(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_payment_channel_creates {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_payment_channel_create(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_payment_channel_funds {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_payment_channel_fund(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_payment_channel_claims {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_payment_channel_claim(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_payment_channel_closes {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_payment_channel_close(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_trust_line_sets {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_trust_line_set(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_issued_transfers {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_issued_transfer(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_issued_asset_policy_sets {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_issued_asset_policy_set(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_trust_line_issuer_controls {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_trust_line_issuer_control(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_issued_clawbacks {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_issued_clawback(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_offer_creates {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_offer_create(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    for tx in &mut block.drc_offer_cancels {
        if !needs_attachment(&tx.public_key, &tx.signature, &None) {
            continue;
        }
        let key = attachment_key_for_offer_cancel(tx, chain_id, genesis);
        tx.multisign = Some(
            map.get(&key)
                .cloned()
                .ok_or(DrcMultisignAttachmentError::MissingAttachment)?,
        );
    }

    Ok(block)
}
