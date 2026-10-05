//! Public-core parity: future versions and ticket selector rejection for trust line ops.

#[cfg(test)]
mod public_core_parity {
    use agora_types::{
        Amount, DrcAccountSequenceSelector, DrcIssuedTransferTx, DrcTrustLineSetTx, Hash,
        IssuedAmount, IssuedCurrencyCode, DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION,
        DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION, DRC_TRUST_LINE_SET_TICKET_VERSION,
        DRC_TRUST_LINE_SET_TX_VERSION,
    };

    use crate::drc_trust_line_test_harness::support::{auth, fund, key, mint_ticket};

    #[test]
    fn future_trust_line_set_version_rejects_ticket_selector() {
        let store = crate::StateStore::open_in_memory();
        let holder = key(40);
        fund(&store, &holder, 100);
        let seq = mint_ticket(&store, &holder);
        let currency = IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        let tx = DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TICKET_VERSION + 1,
            holder: holder.address(),
            issuer: key(41).address(),
            currency,
            limit: IssuedAmount::from_units(100),
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(tx.validate_structure().is_err());
        let _ = auth();
    }

    #[test]
    fn future_issued_transfer_version_rejects_ticket_selector() {
        let store = crate::StateStore::open_in_memory();
        let sender = key(42);
        fund(&store, &sender, 100);
        let seq = mint_ticket(&store, &sender);
        let currency = IssuedCurrencyCode(*b"EUR\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        let tx = DrcIssuedTransferTx {
            version: DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION + 1,
            sender: sender.address(),
            recipient: key(43).address(),
            issuer: key(44).address(),
            currency,
            amount: IssuedAmount::from_units(1),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(tx.validate_structure().is_err());
    }

    #[test]
    fn legacy_trust_line_set_rejects_ticket_selector() {
        let currency = IssuedCurrencyCode(*b"ABC\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        let tx = DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TX_VERSION,
            holder: key(45).address(),
            issuer: key(46).address(),
            currency,
            limit: IssuedAmount::from_units(1),
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(2)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(tx.validate_structure().is_ok());
        let selector = agora_types::resolve_drc_account_sequence(
            tx.version,
            DRC_TRUST_LINE_SET_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        );
        assert!(selector.is_err());
    }

    #[test]
    fn legacy_issued_transfer_rejects_ticket_selector() {
        let currency = IssuedCurrencyCode(*b"ABC\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        let tx = DrcIssuedTransferTx {
            version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            sender: key(47).address(),
            recipient: key(48).address(),
            issuer: key(49).address(),
            currency,
            amount: IssuedAmount::from_units(1),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(2)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(tx.validate_structure().is_ok());
        let selector = agora_types::resolve_drc_account_sequence(
            tx.version,
            DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        );
        assert!(selector.is_err());
    }
}
