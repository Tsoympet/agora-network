//! Header-binding witness for a TLT transaction Merkle root.
//!
//! `Block::compute_body_root` wraps the TLT Merkle root once per occupied body
//! lane. A light client can recompute `header.tx_root` from that Merkle root
//! plus these layers. Missing layers must fail closed: a Merkle root that is
//! not folded into the header is not an inclusion proof.

use crate::{
    AccountTransfer, Block, DataCommitmentAuthorization, DrcAccountPolicyTx, DrcCheckCancelTx,
    DrcCheckCashTx, DrcCheckCreateTx, DrcDepositPreauthTx, DrcEscrowCancelTx, DrcEscrowCreateTx,
    DrcEscrowFinishTx, DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx, DrcIssuedTransferTx,
    DrcMultisignBlockAttachment, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseTx,
    DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, DrcPaymentTx, DrcRegularKeyTx,
    DrcSignerListTx, DrcTicketCreateTx, DrcTrustLineIssuerControlTx, DrcTrustLineSetTx, Hash,
    OvlExecutionTx, SignedStakeTx, DRC_PAYMENT_VERSION,
};

/// One outer wrap of the TLT Merkle root inside `header.tx_root`.
///
/// Variants match the consensus body-root domains. `v2`/`v3`/`v4` hash a
/// fixed-width domain (no Borsh length prefix). Later lanes hash a
/// length-prefixed domain plus a `u16` version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyBindingStep {
    V2 {
        account_ids: Vec<Hash>,
        stake_ids: Vec<Hash>,
    },
    V3 {
        execution_ids: Vec<Hash>,
    },
    V4 {
        payment_ids: Vec<Hash>,
    },
    Versioned {
        domain: &'static str,
        version: u16,
        id_lists: Vec<Vec<Hash>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyBindingError {
    UnsupportedLayer,
}

/// Layers that fold a TLT Merkle root into `block.compute_body_root()`.
///
/// Order is innermost first. An empty list means the header root is the TLT
/// Merkle root itself (UTXO-only body).
pub fn tlt_body_binding(block: &Block) -> Vec<BodyBindingStep> {
    let mut steps = Vec::new();
    if !block.account_transfers.is_empty() || !block.stake_ops.is_empty() {
        steps.push(BodyBindingStep::V2 {
            account_ids: block
                .account_transfers
                .iter()
                .map(AccountTransfer::transfer_id)
                .collect(),
            stake_ids: block
                .stake_ops
                .iter()
                .map(SignedStakeTx::stake_tx_id)
                .collect(),
        });
    }
    if !block.ovl_executions.is_empty() {
        steps.push(BodyBindingStep::V3 {
            execution_ids: block
                .ovl_executions
                .iter()
                .map(OvlExecutionTx::tx_id)
                .collect(),
        });
    }
    if !block.drc_payments.is_empty() {
        steps.push(BodyBindingStep::V4 {
            payment_ids: block
                .drc_payments
                .iter()
                .map(DrcPaymentTx::payment_id)
                .collect(),
        });
    }
    if !block.data_commitments.is_empty() {
        steps.push(versioned(
            "agora-block-body-v5",
            5,
            vec![block
                .data_commitments
                .iter()
                .map(DataCommitmentAuthorization::authorization_id)
                .collect()],
        ));
    }
    if !block.drc_account_policies.is_empty() {
        steps.push(versioned(
            "agora-block-body-v6",
            6,
            vec![block
                .drc_account_policies
                .iter()
                .map(DrcAccountPolicyTx::policy_tx_id)
                .collect()],
        ));
    }
    if !block.drc_deposit_preauths.is_empty() {
        steps.push(versioned(
            "agora-block-body-v7",
            7,
            vec![block
                .drc_deposit_preauths
                .iter()
                .map(DrcDepositPreauthTx::preauth_tx_id)
                .collect()],
        ));
    }
    if block
        .drc_payments
        .iter()
        .any(|payment| payment.version >= DRC_PAYMENT_VERSION)
    {
        steps.push(versioned(
            "agora-block-body-v8",
            8,
            vec![block
                .drc_payments
                .iter()
                .map(DrcPaymentTx::payment_id)
                .collect()],
        ));
    }
    if !block.drc_regular_keys.is_empty() {
        steps.push(versioned(
            "agora-block-body-v9",
            9,
            vec![block
                .drc_regular_keys
                .iter()
                .map(DrcRegularKeyTx::regular_key_tx_id)
                .collect()],
        ));
    }
    if !block.drc_signer_lists.is_empty() {
        steps.push(versioned(
            "agora-block-body-v10",
            10,
            vec![block
                .drc_signer_lists
                .iter()
                .map(DrcSignerListTx::signer_list_tx_id)
                .collect()],
        ));
    }
    if !block.drc_multisign_attachments.is_empty() {
        steps.push(versioned(
            "agora-block-body-v11",
            11,
            vec![block
                .drc_multisign_attachments
                .iter()
                .map(DrcMultisignBlockAttachment::body_commitment_id)
                .collect()],
        ));
    }
    if !block.drc_ticket_creates.is_empty() {
        steps.push(versioned(
            "agora-block-body-v12",
            12,
            vec![block
                .drc_ticket_creates
                .iter()
                .map(DrcTicketCreateTx::ticket_create_tx_id)
                .collect()],
        ));
    }
    if !block.drc_escrow_creates.is_empty()
        || !block.drc_escrow_finishes.is_empty()
        || !block.drc_escrow_cancels.is_empty()
    {
        steps.push(versioned(
            "agora-block-body-v13",
            13,
            vec![
                block
                    .drc_escrow_creates
                    .iter()
                    .map(DrcEscrowCreateTx::escrow_id)
                    .collect(),
                block
                    .drc_escrow_finishes
                    .iter()
                    .map(DrcEscrowFinishTx::finish_tx_id)
                    .collect(),
                block
                    .drc_escrow_cancels
                    .iter()
                    .map(DrcEscrowCancelTx::cancel_tx_id)
                    .collect(),
            ],
        ));
    }
    if !block.drc_check_creates.is_empty()
        || !block.drc_check_cashes.is_empty()
        || !block.drc_check_cancels.is_empty()
    {
        steps.push(versioned(
            "agora-block-body-v14",
            14,
            vec![
                block
                    .drc_check_creates
                    .iter()
                    .map(DrcCheckCreateTx::check_id)
                    .collect(),
                block
                    .drc_check_cashes
                    .iter()
                    .map(DrcCheckCashTx::cash_tx_id)
                    .collect(),
                block
                    .drc_check_cancels
                    .iter()
                    .map(DrcCheckCancelTx::cancel_tx_id)
                    .collect(),
            ],
        ));
    }
    if !block.drc_payment_channel_creates.is_empty()
        || !block.drc_payment_channel_funds.is_empty()
        || !block.drc_payment_channel_claims.is_empty()
        || !block.drc_payment_channel_closes.is_empty()
    {
        steps.push(versioned(
            "agora-block-body-v15",
            15,
            vec![
                block
                    .drc_payment_channel_creates
                    .iter()
                    .map(DrcPaymentChannelCreateTx::channel_id)
                    .collect(),
                block
                    .drc_payment_channel_funds
                    .iter()
                    .map(DrcPaymentChannelFundTx::fund_tx_id)
                    .collect(),
                block
                    .drc_payment_channel_claims
                    .iter()
                    .map(DrcPaymentChannelClaimTx::claim_tx_id)
                    .collect(),
                block
                    .drc_payment_channel_closes
                    .iter()
                    .map(DrcPaymentChannelCloseTx::close_tx_id)
                    .collect(),
            ],
        ));
    }
    if !block.drc_trust_line_sets.is_empty() || !block.drc_issued_transfers.is_empty() {
        steps.push(versioned(
            "agora-block-body-v16",
            16,
            vec![
                block
                    .drc_trust_line_sets
                    .iter()
                    .map(DrcTrustLineSetTx::trust_line_set_tx_id)
                    .collect(),
                block
                    .drc_issued_transfers
                    .iter()
                    .map(DrcIssuedTransferTx::issued_transfer_tx_id)
                    .collect(),
            ],
        ));
    }
    if !block.drc_issued_asset_policy_sets.is_empty()
        || !block.drc_trust_line_issuer_controls.is_empty()
        || !block.drc_issued_clawbacks.is_empty()
    {
        steps.push(versioned(
            "agora-block-body-v17",
            17,
            vec![
                block
                    .drc_issued_asset_policy_sets
                    .iter()
                    .map(DrcIssuedAssetPolicySetTx::policy_set_tx_id)
                    .collect(),
                block
                    .drc_trust_line_issuer_controls
                    .iter()
                    .map(DrcTrustLineIssuerControlTx::issuer_control_tx_id)
                    .collect(),
                block
                    .drc_issued_clawbacks
                    .iter()
                    .map(DrcIssuedClawbackTx::clawback_tx_id)
                    .collect(),
            ],
        ));
    }
    steps
}

fn versioned(domain: &'static str, version: u16, id_lists: Vec<Vec<Hash>>) -> BodyBindingStep {
    BodyBindingStep::Versioned {
        domain,
        version,
        id_lists,
    }
}

/// Recompute the header body root from a TLT Merkle root and binding layers.
pub fn fold_body_binding(
    tx_merkle_root: Hash,
    steps: &[BodyBindingStep],
) -> Result<Hash, BodyBindingError> {
    let mut inner = tx_merkle_root;
    for step in steps {
        inner = fold_step(inner, step)?;
    }
    Ok(inner)
}

fn fold_step(inner: Hash, step: &BodyBindingStep) -> Result<Hash, BodyBindingError> {
    match step {
        BodyBindingStep::V2 {
            account_ids,
            stake_ids,
        } => Ok(Hash::hash_borsh(&(
            b"agora-block-body-v2",
            inner,
            account_ids,
            stake_ids,
        ))),
        BodyBindingStep::V3 { execution_ids } => Ok(Hash::hash_borsh(&(
            b"agora-block-body-v3",
            inner,
            execution_ids,
        ))),
        BodyBindingStep::V4 { payment_ids } => Ok(Hash::hash_borsh(&(
            b"agora-block-body-v4",
            inner,
            payment_ids,
        ))),
        BodyBindingStep::Versioned {
            domain,
            version,
            id_lists,
        } => fold_versioned(domain.as_bytes(), *version, inner, id_lists),
    }
}

fn fold_versioned(
    domain: &[u8],
    version: u16,
    inner: Hash,
    lists: &[Vec<Hash>],
) -> Result<Hash, BodyBindingError> {
    // Tuple arity is part of the consensus encoding. Unknown shapes fail closed.
    match lists {
        [a] => Ok(Hash::hash_borsh(&(domain, version, inner, a))),
        [a, b] => Ok(Hash::hash_borsh(&(domain, version, inner, a, b))),
        [a, b, c] => Ok(Hash::hash_borsh(&(domain, version, inner, a, b, c))),
        [a, b, c, d] => Ok(Hash::hash_borsh(&(domain, version, inner, a, b, c, d))),
        _ => Err(BodyBindingError::UnsupportedLayer),
    }
}

/// True when `id` is one of the lane ids committed by a binding step.
pub fn binding_contains_id(steps: &[BodyBindingStep], id: &Hash) -> bool {
    steps.iter().any(|step| match step {
        BodyBindingStep::V2 {
            account_ids,
            stake_ids,
        } => account_ids.contains(id) || stake_ids.contains(id),
        BodyBindingStep::V3 { execution_ids } => execution_ids.contains(id),
        BodyBindingStep::V4 { payment_ids } => payment_ids.contains(id),
        BodyBindingStep::Versioned { id_lists, .. } => {
            id_lists.iter().any(|list| list.contains(id))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Address, Amount, BlockHeader, NativeAssetId, OvlExecutionTx, Transaction, TxOut,
        DRC_PAYMENT_VERSION,
    };

    fn header() -> BlockHeader {
        BlockHeader {
            version: 1,
            parents: vec![],
            timestamp_ms: 1_700_000_000_000,
            bits: 1,
            nonce: 7,
            tx_root: Hash::ZERO,
        }
    }

    fn tlt_tx() -> Transaction {
        Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(5),
                address: Address([9u8; 20]),
            }],
            3,
        )
    }

    #[test]
    fn utxo_only_binding_is_the_merkle_root() {
        let tx = tlt_tx();
        let mut block = Block::utxo(header(), vec![tx]);
        block.header.tx_root = block.compute_body_root();
        let steps = tlt_body_binding(&block);
        assert!(steps.is_empty());
        let merkle = Block::compute_tx_root(&block.transactions);
        assert_eq!(fold_body_binding(merkle, &steps).unwrap(), merkle);
        assert_eq!(merkle, block.header.tx_root);
    }

    #[test]
    fn multi_lane_binding_matches_header_body_root() {
        let mut block = Block::utxo(header(), vec![tlt_tx()]);
        block.account_transfers = vec![AccountTransfer::unsigned(
            NativeAssetId::OVL,
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(4),
            1,
        )];
        block.ovl_executions = vec![OvlExecutionTx::unsigned(
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            21_000,
            1,
            0,
            vec![0xab],
        )];
        let mut payment = DrcPaymentTx::unsigned_v2(
            Address([3u8; 20]),
            Address([4u8; 20]),
            Amount::from_base_units(8),
            Amount::from_base_units(1),
            0,
            None,
            Hash([6u8; 32]),
            2,
        );
        payment.version = DRC_PAYMENT_VERSION;
        block.drc_payments = vec![payment];
        block.header.tx_root = block.compute_body_root();

        let steps = tlt_body_binding(&block);
        assert!(steps
            .iter()
            .any(|step| matches!(step, BodyBindingStep::V3 { .. })));
        assert!(steps
            .iter()
            .any(|step| matches!(step, BodyBindingStep::V4 { .. })));
        let merkle = Block::compute_tx_root(&block.transactions);
        assert_eq!(
            fold_body_binding(merkle, &steps).unwrap(),
            block.header.tx_root
        );
        let exec_id = block.ovl_executions[0].tx_id();
        assert!(binding_contains_id(&steps, &exec_id));
        assert!(!binding_contains_id(&steps, &Hash([9u8; 32])));
        let mut dropped = steps.clone();
        dropped.pop();
        assert_ne!(
            fold_body_binding(merkle, &dropped).unwrap(),
            block.header.tx_root
        );
    }

    #[test]
    fn locked_client_vectors() {
        let tx = tlt_tx();
        let mut block = Block::utxo(header(), vec![tx]);
        block.header.tx_root = block.compute_body_root();
        let merkle = Block::compute_tx_root(&block.transactions);
        let proof = crate::prove_tlt_tx_merkle(
            &block
                .transactions
                .iter()
                .map(Transaction::tx_id)
                .collect::<Vec<_>>(),
            0,
        )
        .unwrap();
        assert_eq!(
            block.header.hash().to_hex(),
            "0687bb9723213df52b4bc9868c70c99c0e6bc21758a81ec158cd35338ae582be"
        );
        assert_eq!(
            merkle.to_hex(),
            "1864d005427479e7b35c49f49342edc1d152d7c9924e88b74ee63d400a8156f4"
        );
        assert_eq!(proof.tx_id, merkle);
        assert!(proof.siblings.is_empty());

        let mut wrapped = Block::utxo(header(), vec![tlt_tx()]);
        wrapped.account_transfers = vec![AccountTransfer::unsigned(
            NativeAssetId::DRC,
            Address([3u8; 20]),
            Address([4u8; 20]),
            Amount::from_base_units(9),
            4,
        )];
        let steps = tlt_body_binding(&wrapped);
        let wrapped_merkle = Block::compute_tx_root(&wrapped.transactions);
        let folded = fold_body_binding(wrapped_merkle, &steps).unwrap();
        let BodyBindingStep::V2 {
            account_ids,
            stake_ids,
        } = &steps[0]
        else {
            panic!("expected v2");
        };
        assert!(stake_ids.is_empty());
        assert_eq!(folded, wrapped.compute_body_root());
        assert_eq!(
            account_ids[0].to_hex(),
            "633dea78d31691edd0c36d6dfe67ee8c0eaef738a86a71e6fd9ef698a9aadf91"
        );
        assert_eq!(
            folded.to_hex(),
            "4d339b0ad0130a65e06ce1434ccad358db4c50e92eb952ab5d0a0aad008da375"
        );
        let versioned = fold_body_binding(
            merkle,
            &[BodyBindingStep::Versioned {
                domain: "agora-block-body-v5",
                version: 5,
                id_lists: vec![vec![Hash([2u8; 32])]],
            }],
        )
        .unwrap();
        assert_eq!(
            versioned.to_hex(),
            "22d7f391c609b5798f27122b4eab779d2416098b8fd25d740d18f4f2180d7585"
        );
    }

    #[test]
    fn versioned_layer_rejects_unknown_arity() {
        let step = BodyBindingStep::Versioned {
            domain: "agora-block-body-v5",
            version: 5,
            id_lists: vec![],
        };
        assert_eq!(
            fold_body_binding(Hash([1u8; 32]), &[step]),
            Err(BodyBindingError::UnsupportedLayer)
        );
    }
}
