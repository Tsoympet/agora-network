//! Shared helpers for issuer-scoped trust line / issued-transfer tests.

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub mod support {
    use std::sync::atomic::{AtomicU64, Ordering};

    use agora_crypto::{sign_drc_issued_transfer_bound, sign_drc_trust_line_set_bound, KeyPair};
    use agora_types::{
        Amount, Block, BlockHeader, DrcIssuedTransferTx, DrcTrustLineSetTx, Hash, IssuedAmount,
        IssuedAssetId, IssuedCurrencyCode, NativeAssetId, Transaction, TxOut,
        DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION, DRC_TRUST_LINE_SET_TX_VERSION,
    };

    use crate::accounts::credit_account_into;
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_trust_line::{
        drc_trust_line_root, load_drc_issued_transfer_receipt, load_drc_issuer_liability,
        load_drc_trust_line_live,
    };
    use crate::store::WriteBatch;
    use crate::StateStore;

    static COINBASE_SEQ: AtomicU64 = AtomicU64::new(8_000_000);

    pub fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    pub fn key(b: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[b; 32]).unwrap()
    }

    pub fn fund(store: &StateStore, kp: &KeyPair, amount: u64) {
        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            store,
            NativeAssetId::DRC,
            &kp.address(),
            Amount::from_base_units(amount),
        )
        .unwrap();
        store.write_batch(batch).unwrap();
    }

    pub fn std_code(tag: &[u8; 3]) -> IssuedCurrencyCode {
        let mut c = [0u8; 20];
        c[..3].copy_from_slice(tag);
        IssuedCurrencyCode(c)
    }

    pub fn asset(issuer: &KeyPair, currency: IssuedCurrencyCode) -> IssuedAssetId {
        IssuedAssetId {
            issuer: issuer.address(),
            currency,
        }
    }

    pub fn coinbase(parents: Vec<Hash>, payout: &KeyPair) -> Block {
        let seq = COINBASE_SEQ.fetch_add(1, Ordering::Relaxed);
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents,
                timestamp_ms: seq,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![Transaction::unsigned(
                1,
                vec![],
                vec![TxOut {
                    value: Amount::from_base_units(50),
                    address: payout.address(),
                }],
                seq,
            )],
        );
        block.header.tx_root = block.compute_body_root();
        block
    }

    pub fn signed_trust_line_set(
        holder: &KeyPair,
        issuer: &KeyPair,
        currency: IssuedCurrencyCode,
        limit: u64,
        fee: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcTrustLineSetTx {
        let mut tx = DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TX_VERSION,
            holder: holder.address(),
            issuer: issuer.address(),
            currency,
            limit: IssuedAmount::from_units(limit),
            fee: Amount::from_base_units(fee),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_trust_line_set_bound(&mut tx, holder, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn signed_issued_transfer(
        sender: &KeyPair,
        recipient: agora_types::Address,
        issuer: &KeyPair,
        currency: IssuedCurrencyCode,
        amount: u64,
        fee: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcIssuedTransferTx {
        let mut tx = DrcIssuedTransferTx {
            version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            sender: sender.address(),
            recipient,
            issuer: issuer.address(),
            currency,
            amount: IssuedAmount::from_units(amount),
            fee: Amount::from_base_units(fee),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_issued_transfer_bound(&mut tx, sender, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn apply_block(store: &StateStore, block: Block, blue: u64, ctx: &TxAuthContext) {
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), blue)
                .unwrap();
        store.write_batch(result.batch).unwrap();
    }

    pub fn line_balance(
        store: &StateStore,
        holder: agora_types::Address,
        asset: &IssuedAssetId,
    ) -> u64 {
        load_drc_trust_line_live(store, &holder, asset)
            .unwrap()
            .unwrap()
            .balance
            .as_units()
    }

    pub fn issuer_outstanding(store: &StateStore, asset: &IssuedAssetId) -> u64 {
        load_drc_issuer_liability(store, asset).unwrap().as_units()
    }

    pub fn receipt_exists(store: &StateStore, tx_id: &Hash) -> bool {
        load_drc_issued_transfer_receipt(store, tx_id)
            .unwrap()
            .is_some()
    }

    pub fn trust_root(store: &StateStore) -> Hash {
        drc_trust_line_root(store).unwrap()
    }

    pub fn apply_block_journal(
        store: &StateStore,
        block: Block,
        blue: u64,
        ctx: &TxAuthContext,
    ) -> crate::apply::UtxoJournal {
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), blue)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        result.journal
    }

    pub fn revert_journal(store: &StateStore, journal: &crate::apply::UtxoJournal) {
        crate::apply::revert_journal(store, journal).unwrap();
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub mod multisign {
    use agora_crypto::{sign_drc_multisign_participant_bound, sign_drc_signer_list_bound, KeyPair};
    use agora_types::{
        materialize_drc_multisign_attachments, Amount, Block, DrcMultisignAuth, DrcMultisignEntry,
        DrcSignerListEntry, DrcSignerListTx, DrcTrustLineSetTx, NativeAssetId,
        DRC_MULTISIGN_AUTH_VERSION, DRC_TRUST_LINE_SET_TX_VERSION,
    };

    use super::support::{coinbase, std_code};
    use crate::accounts::load_account;
    use crate::apply::TxAuthContext;
    use crate::drc_signer_list::apply_drc_signer_list;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    pub fn install_signer_list(
        store: &StateStore,
        master: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let nonce = load_account(store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        install_signer_list_at_nonce(store, master, signers, ctx, nonce);
    }

    pub fn install_signer_list_at_nonce(
        store: &StateStore,
        master: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
        nonce: u64,
    ) {
        let total: u16 = signers.iter().map(|(_, w)| w).sum();
        let mut install = DrcSignerListTx::unsigned_set(
            master.address(),
            agora_types::canonical_sorted_entries(
                &signers
                    .iter()
                    .map(|(kp, weight)| DrcSignerListEntry {
                        signer: kp.address(),
                        weight: *weight,
                    })
                    .collect::<Vec<_>>(),
            ),
            u32::from(total),
            Amount::ZERO,
            nonce,
        );
        sign_drc_signer_list_bound(&mut install, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_signer_list(store, &install, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    pub fn multisign_bundle(
        owner: agora_types::Address,
        signing_bytes: &[u8],
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> DrcMultisignAuth {
        let mut entries = Vec::new();
        for (kp, _) in signers {
            let (signer, public_key, signature) = sign_drc_multisign_participant_bound(
                owner,
                signing_bytes,
                kp,
                &ctx.chain_id,
                &ctx.genesis,
            )
            .unwrap();
            entries.push(DrcMultisignEntry {
                signer,
                public_key,
                signature,
            });
        }
        entries.sort_by_key(|e| e.signer.0);
        DrcMultisignAuth {
            version: DRC_MULTISIGN_AUTH_VERSION,
            signing_for: owner,
            signatures: entries,
        }
    }

    pub fn base_multisign_trust_line_set_block(
        store: &StateStore,
        holder: &KeyPair,
        issuer: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, holder, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &holder.address())
            .unwrap()
            .nonce;
        let mut tx = DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TX_VERSION,
            holder: holder.address(),
            issuer: issuer.address(),
            currency: std_code(b"USD"),
            limit: agora_types::IssuedAmount::from_units(100),
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let signing_bytes = tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        tx.multisign = Some(multisign_bundle(
            holder.address(),
            &signing_bytes,
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![agora_types::Hash::ZERO], holder);
        block.drc_trust_line_sets.push(tx);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();
        block
    }
}
