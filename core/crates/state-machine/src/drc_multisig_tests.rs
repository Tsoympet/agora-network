//! Integration tests for DRC weighted signer lists and multisign authorization.

#[cfg(test)]
mod tests {
    use agora_crypto::{
        sign_drc_multisign_participant_bound, sign_drc_payment_bound, sign_drc_signer_list_bound,
        KeyPair,
    };
    use agora_types::{
        canonical_sorted_entries, Amount, DrcMultisignAuth, DrcMultisignEntry, DrcPaymentTx,
        DrcSignerListAction, DrcSignerListEntry, DrcSignerListTx, Hash, NativeAssetId,
        DRC_MULTISIGN_AUTH_VERSION,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::TxAuthContext;
    use crate::drc_signer_list::{apply_drc_signer_list, load_drc_account_signer_list};
    use crate::payments::apply_drc_payment;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    fn key(byte: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[byte; 32]).unwrap()
    }

    fn fund(store: &StateStore, account: &KeyPair, amount: u64) {
        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            store,
            NativeAssetId::DRC,
            &account.address(),
            Amount::from_base_units(amount),
        )
        .unwrap();
        store.write_batch(batch).unwrap();
    }

    fn apply_list(store: &StateStore, tx: &DrcSignerListTx) {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_signer_list(store, tx, &auth(), &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    fn multisign_bundle(
        owner: agora_types::Address,
        operation_signing_bytes: &[u8],
        signers: &[(&KeyPair, u16)],
        auth: &TxAuthContext,
    ) -> DrcMultisignAuth {
        let mut entries: Vec<DrcMultisignEntry> = Vec::new();
        for (kp, _) in signers {
            let (signer, public_key, signature) = sign_drc_multisign_participant_bound(
                owner,
                operation_signing_bytes,
                kp,
                &auth.chain_id,
                &auth.genesis,
            )
            .unwrap();
            entries.push(DrcMultisignEntry {
                signer,
                public_key,
                signature,
            });
        }
        entries.sort_by_key(|entry| entry.signer.0);
        DrcMultisignAuth {
            version: DRC_MULTISIGN_AUTH_VERSION,
            signing_for: owner,
            signatures: entries,
        }
    }

    #[test]
    fn master_installs_signer_list_and_multisign_pays() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let signer_a = key(2);
        let signer_b = key(3);
        let recipient = key(4);
        fund(&store, &master, 200);
        let auth = auth();

        let entries = canonical_sorted_entries(&[
            DrcSignerListEntry {
                signer: signer_a.address(),
                weight: 1,
            },
            DrcSignerListEntry {
                signer: signer_b.address(),
                weight: 2,
            },
        ]);
        let mut install = DrcSignerListTx::unsigned_set(
            master.address(),
            entries,
            2,
            Amount::from_base_units(1),
            0,
        );
        sign_drc_signer_list_bound(&mut install, &master, &auth.chain_id, &auth.genesis).unwrap();
        apply_list(&store, &install);

        let list = load_drc_account_signer_list(&store, &master.address())
            .unwrap()
            .expect("list installed");
        assert_eq!(list.quorum, 2);

        let mut payment = DrcPaymentTx::unsigned(
            master.address(),
            recipient.address(),
            Amount::from_base_units(10),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            1,
        );
        payment.public_key.clear();
        payment.signature.clear();
        payment.multisign = Some(multisign_bundle(
            master.address(),
            &payment.signing_bytes_bound(&auth.chain_id, &auth.genesis),
            &[(&signer_a, 1), (&signer_b, 2)],
            &auth,
        ));

        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &payment, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            10
        );
    }

    #[test]
    fn master_recovery_after_signer_list_delete() {
        let store = StateStore::open_in_memory();
        let master = key(10);
        let signer = key(11);
        fund(&store, &master, 50);
        let auth = auth();

        let entries = vec![DrcSignerListEntry {
            signer: signer.address(),
            weight: 1,
        }];
        let mut install =
            DrcSignerListTx::unsigned_set(master.address(), entries, 1, Amount::ZERO, 0);
        sign_drc_signer_list_bound(&mut install, &master, &auth.chain_id, &auth.genesis).unwrap();
        apply_list(&store, &install);

        let mut delete = DrcSignerListTx::unsigned_delete(master.address(), Amount::ZERO, 1);
        sign_drc_signer_list_bound(&mut delete, &master, &auth.chain_id, &auth.genesis).unwrap();
        apply_list(&store, &delete);
        assert!(load_drc_account_signer_list(&store, &master.address())
            .unwrap()
            .is_none());

        let mut payment = DrcPaymentTx::unsigned(
            master.address(),
            key(12).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            0,
            Hash::ZERO,
            2,
        );
        sign_drc_payment_bound(&mut payment, &master, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &payment, &auth, &mut batch, &mut journal).unwrap();
    }

    #[test]
    fn new_signer_list_cannot_authorize_itself_with_multisign() {
        let store = StateStore::open_in_memory();
        let master = key(20);
        fund(&store, &master, 10);
        let auth = auth();
        let new_a = key(21);
        let new_b = key(22);
        let entries = canonical_sorted_entries(&[
            DrcSignerListEntry {
                signer: new_a.address(),
                weight: 1,
            },
            DrcSignerListEntry {
                signer: new_b.address(),
                weight: 1,
            },
        ]);
        let mut install =
            DrcSignerListTx::unsigned_set(master.address(), entries.clone(), 2, Amount::ZERO, 0);
        install.public_key.clear();
        install.signature.clear();
        install.multisign = Some(multisign_bundle(
            master.address(),
            &install.signing_bytes_bound(&auth.chain_id, &auth.genesis),
            &[(&new_a, 1), (&new_b, 1)],
            &auth,
        ));
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_signer_list(&store, &install, &auth, &mut batch, &mut journal).is_err());
    }

    #[test]
    fn reject_duplicate_signer_entries() {
        let owner = key(30).address();
        let signer = key(31).address();
        let err = DrcSignerListTx::unsigned_set(
            owner,
            vec![
                DrcSignerListEntry { signer, weight: 1 },
                DrcSignerListEntry { signer, weight: 2 },
            ],
            2,
            Amount::ZERO,
            0,
        )
        .validate_structure();
        assert!(err.is_err());
    }
}
