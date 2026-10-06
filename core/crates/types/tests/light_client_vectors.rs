//! Locked signing preimages for shared light-client typed-lane builders.

use agora_types::{
    script_p2pkh, AccountTransfer, Address, Amount, DrcBookAsset, DrcOfferCancelTx,
    DrcOfferCreateTx, DrcPaymentTx, Hash, IssuedAssetId, IssuedCurrencyCode, NativeAssetId,
    OutPoint, OvlExecutionTx, TltCovenantInput, TltCovenantOutput, TltCovenantTx,
    DRC_OFFER_CANCEL_TX_VERSION, DRC_OFFER_CREATE_TX_VERSION, TLT_COVENANT_TX_VERSION,
    TLT_SEQUENCE_FINAL,
};

fn genesis() -> Hash {
    Hash::from_hex("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef").unwrap()
}

fn addr(fill: u8) -> Address {
    Address([fill; 20])
}

#[test]
fn account_transfer_v2_signing_bytes_are_locked() {
    let tx = AccountTransfer::unsigned_with_fee(
        NativeAssetId::OVL,
        addr(1),
        addr(2),
        Amount::from_base_units(10),
        Amount::from_base_units(1),
        7,
    );
    let bytes = tx.signing_bytes_bound("agora-testnet-1", &genesis());
    assert_eq!(
        hex::encode(&bytes),
        "1b00000061676f72612d74726964656e742d6163636f756e742d74782d76320f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0200000001010101010101010101010101010101010101010102020202020202020202020202020202020202020a0000000000000001000000000000000700000000000000"
    );
    let json = serde_json::to_value(&tx).unwrap();
    assert_eq!(json["asset"], "OVL");
    assert_eq!(json["amount"], 10);
    assert_eq!(json["from"].as_array().unwrap().len(), 20);
}

#[test]
fn ovl_execution_v1_signing_bytes_are_locked() {
    let tx = OvlExecutionTx::unsigned(
        addr(1),
        addr(2),
        Amount::from_base_units(3),
        21_000,
        1,
        4,
        vec![],
    );
    let bytes = tx.signing_bytes_bound("agora-testnet-1", &genesis());
    assert_eq!(
        hex::encode(&bytes),
        "1e00000061676f72612d74726964656e742d6f766c2d657865637574696f6e2d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0100000001010101010101010101010101010101010101010202020202020202020202020202020202020202030000000000000008520000000000000100000000000000040000000000000000000000"
    );
}

#[test]
fn drc_payment_v4_signing_bytes_are_locked() {
    let tx = DrcPaymentTx::unsigned_v4(
        addr(1),
        addr(2),
        Amount::from_base_units(9),
        Amount::from_base_units(1),
        None,
        None,
        Hash::ZERO,
        5,
        None,
    );
    let bytes = tx.signing_bytes_bound("agora-testnet-1", &genesis());
    assert_eq!(
        hex::encode(&bytes),
        "1c00000061676f72612d74726964656e742d6472632d7061796d656e742d76340f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef04000000010101010101010101010101010101010101010102020202020202020202020202020202020202020900000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000050000000000000000"
    );
}

#[test]
fn drc_offer_create_v1_signing_bytes_are_locked() {
    let tx = DrcOfferCreateTx {
        version: DRC_OFFER_CREATE_TX_VERSION,
        owner: addr(1),
        taker_pays: DrcBookAsset::NativeDrc,
        taker_pays_amount: 8,
        taker_gets: DrcBookAsset::Issued(IssuedAssetId {
            issuer: addr(2),
            currency: IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"),
        }),
        taker_gets_amount: 4,
        fill_mode: 1,
        time_in_force: 1,
        fee: Amount::from_base_units(1),
        expires_after_blue_score: None,
        nonce: 3,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    let bytes = tx.signing_bytes_bound("agora-testnet-1", &genesis());
    assert_eq!(
        hex::encode(&bytes),
        "2100000061676f72612d74726964656e742d6472632d6f666665722d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef100000006472635f6f666665725f6372656174650100000001010101010101010101010101010101010101010108000000000000000202020202020202020202020202020202020202025553440000000000000000000000000000000000040000000000000001010100000000000000000300000000000000"
    );
}

#[test]
fn drc_offer_cancel_v1_signing_bytes_are_locked() {
    let tx = DrcOfferCancelTx {
        version: DRC_OFFER_CANCEL_TX_VERSION,
        submitter: addr(1),
        offer_id: Hash([9; 32]),
        fee: Amount::from_base_units(1),
        nonce: 6,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    let bytes = tx.signing_bytes_bound("agora-testnet-1", &genesis());
    assert_eq!(
        hex::encode(&bytes),
        "2100000061676f72612d74726964656e742d6472632d6f666665722d63616e63656c2d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef100000006472635f6f666665725f63616e63656c010000000101010101010101010101010101010101010101090909090909090909090909090909090909090909090909090909090909090901000000000000000600000000000000"
    );
}

#[test]
fn remaining_drc_family_signing_bytes_are_locked() {
    use agora_types::*;
    let g = genesis();
    let chain = "agora-testnet-1";
    let currency = IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    let escrow_create = DrcEscrowCreateTx {
        version: DRC_ESCROW_CREATE_TX_VERSION,
        owner: addr(1),
        recipient: addr(2),
        amount: Amount::from_base_units(9),
        fee: Amount::from_base_units(1),
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        finish_after_blue_score: None,
        cancel_after_blue_score: None,
        nonce: 5,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    assert_eq!(
        hex::encode(escrow_create.signing_bytes_bound(chain, &g)),
        "2200000061676f72612d74726964656e742d6472632d657363726f772d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef110000006472635f657363726f775f6372656174650100000001010101010101010101010101010101010101010202020202020202020202020202020202020202090000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000000000"
    );

    let escrow_finish = DrcEscrowFinishTx {
        version: DRC_ESCROW_FINISH_TX_VERSION,
        submitter: addr(1),
        escrow_id: Hash([9; 32]),
        fee: Amount::from_base_units(1),
        nonce: 6,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    assert_eq!(
        hex::encode(escrow_finish.signing_bytes_bound(chain, &g)),
        "2200000061676f72612d74726964656e742d6472632d657363726f772d66696e6973682d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef110000006472635f657363726f775f66696e697368010000000101010101010101010101010101010101010101090909090909090909090909090909090909090909090909090909090909090901000000000000000600000000000000"
    );

    let ticket = DrcTicketCreateTx::unsigned(addr(1), Amount::from_base_units(1), 4);
    assert_eq!(
        hex::encode(ticket.signing_bytes_bound(chain, &g)),
        "2200000061676f72612d74726964656e742d6472632d7469636b65742d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef110000006472635f7469636b65745f63726561746501000000010101010101010101010101010101010101010104000000000000000100000000000000"
    );

    let policy =
        DrcAccountPolicyTx::set_require_destination_tag(addr(1), Amount::from_base_units(1), 3);
    assert_eq!(
        hex::encode(policy.signing_bytes_bound(chain, &g)),
        "2300000061676f72612d74726964656e742d6472632d6163636f756e742d706f6c6963792d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0100000001010101010101010101010101010101010101010001000000000000000300000000000000"
    );

    let preauth = DrcDepositPreauthTx::authorize(addr(1), addr(2), Amount::from_base_units(1), 2);
    assert_eq!(
        hex::encode(preauth.signing_bytes_bound(chain, &g)),
        "2400000061676f72612d74726964656e742d6472632d6465706f7369742d707265617574682d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef130000006472635f6465706f7369745f7072656175746801000000010101010101010101010101010101010101010100020202020202020202020202020202020202020202000000000000000100000000000000"
    );

    let regular = DrcRegularKeyTx::unsigned(
        addr(1),
        DrcRegularKeyAction::Clear,
        Address::ZERO,
        Vec::new(),
        Amount::from_base_units(1),
        8,
    );
    assert_eq!(
        hex::encode(regular.signing_bytes_bound(chain, &g)),
        "2000000061676f72612d74726964656e742d6472632d726567756c61722d6b65792d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0f0000006472635f726567756c61725f6b65790100000001010101010101010101010101010101010101010100000000000000000000000000000000000000000000000008000000000000000100000000000000"
    );

    let signer_list = DrcSignerListTx::unsigned_set(
        addr(1),
        vec![DrcSignerListEntry {
            signer: addr(2),
            weight: 1,
        }],
        1,
        Amount::from_base_units(1),
        7,
    );
    assert_eq!(
        hex::encode(signer_list.signing_bytes_bound(chain, &g)),
        "2000000061676f72612d74726964656e742d6472632d7369676e65722d6c6973742d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0f0000006472635f7369676e65725f6c6973740100000001010101010101010101010101010101010101010001000000010000000202020202020202020202020202020202020202010007000000000000000100000000000000"
    );

    let check_create = DrcCheckCreateTx {
        version: DRC_CHECK_CREATE_TX_VERSION,
        owner: addr(1),
        destination: addr(2),
        amount: Amount::from_base_units(9),
        fee: Amount::from_base_units(1),
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        expires_after_blue_score: None,
        nonce: 5,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    assert_eq!(
        hex::encode(check_create.signing_bytes_bound(chain, &g)),
        "2100000061676f72612d74726964656e742d6472632d636865636b2d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef100000006472635f636865636b5f63726561746501000000010101010101010101010101010101010101010102020202020202020202020202020202020202020900000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000000000"
    );

    let check_cash = DrcCheckCashTx {
        version: DRC_CHECK_CASH_TX_VERSION,
        submitter: addr(1),
        check_id: Hash([9; 32]),
        fee: Amount::from_base_units(1),
        nonce: 6,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    assert_eq!(
        hex::encode(check_cash.signing_bytes_bound(chain, &g)),
        "1f00000061676f72612d74726964656e742d6472632d636865636b2d636173682d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0e0000006472635f636865636b5f63617368010000000101010101010101010101010101010101010101090909090909090909090909090909090909090909090909090909090909090901000000000000000600000000000000"
    );

    let channel_create = DrcPaymentChannelCreateTx {
        version: DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
        owner: addr(1),
        destination: addr(2),
        amount: Amount::from_base_units(9),
        fee: Amount::from_base_units(1),
        claim_public_key: vec![2; 33],
        settle_delay_blue_scores: 4,
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        cancel_after_blue_score: None,
        nonce: 5,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    assert_eq!(
        hex::encode(channel_create.signing_bytes_bound(chain, &g)),
        "2b00000061676f72612d74726964656e742d6472632d7061796d656e742d6368616e6e656c2d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef1a0000006472635f7061796d656e745f6368616e6e656c5f63726561746501000000010101010101010101010101010101010101010102020202020202020202020202020202020202020900000000000000010000000000000021000000020202020202020202020202020202020202020202020202020202020202020202040000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000000000"
    );

    let channel_fund = DrcPaymentChannelFundTx {
        version: DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
        submitter: addr(1),
        channel_id: Hash([9; 32]),
        amount: Amount::from_base_units(3),
        fee: Amount::from_base_units(1),
        nonce: 6,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    assert_eq!(
        hex::encode(channel_fund.signing_bytes_bound(chain, &g)),
        "2900000061676f72612d74726964656e742d6472632d7061796d656e742d6368616e6e656c2d66756e642d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef180000006472635f7061796d656e745f6368616e6e656c5f66756e640100000001010101010101010101010101010101010101010909090909090909090909090909090909090909090909090909090909090909030000000000000001000000000000000600000000000000"
    );

    let trust = DrcTrustLineSetTx {
        version: DRC_TRUST_LINE_SET_TX_VERSION,
        holder: addr(1),
        issuer: addr(2),
        currency,
        limit: IssuedAmount::from_units(100),
        fee: Amount::from_base_units(1),
        nonce: 4,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    assert_eq!(
        hex::encode(trust.signing_bytes_bound(chain, &g)),
        "2300000061676f72612d74726964656e742d6472632d74727573742d6c696e652d7365742d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0100000001010101010101010101010101010101010101010202020202020202020202020202020202020202555344000000000000000000000000000000000064000000000000000100000000000000040000000000000000"
    );

    let xfer = DrcIssuedTransferTx {
        version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
        sender: addr(1),
        recipient: addr(2),
        issuer: addr(3),
        currency,
        amount: IssuedAmount::from_units(7),
        fee: Amount::from_base_units(1),
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        nonce: 5,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    assert_eq!(
        hex::encode(xfer.signing_bytes_bound(chain, &g)),
        "2400000061676f72612d74726964656e742d6472632d6973737565642d7472616e736665722d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0100000001010101010101010101010101010101010101010202020202020202020202020202020202020202030303030303030303030303030303030303030355534400000000000000000000000000000000000700000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000050000000000000000"
    );

    let policy_set = DrcIssuedAssetPolicySetTx {
        version: DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
        issuer: addr(1),
        currency,
        action: DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
        fee: Amount::from_base_units(1),
        nonce: 4,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    assert_eq!(
        hex::encode(policy_set.signing_bytes_bound(chain, &g)),
        "d2a02cf60231a28a6d807b91089da97d09d595a6af4bfb76c78b9155aadd1d21"
    );
}

#[test]
fn tlt_covenant_bound_sighash_is_locked() {
    let tx = TltCovenantTx {
        version: TLT_COVENANT_TX_VERSION,
        inputs: vec![TltCovenantInput {
            previous_outpoint: OutPoint {
                tx_id: Hash([4; 32]),
                index: 0,
            },
            sequence: TLT_SEQUENCE_FINAL,
            script_sig: Vec::new(),
        }],
        outputs: vec![TltCovenantOutput {
            value: Amount::from_base_units(1),
            script_pubkey: script_p2pkh(&addr(1)),
        }],
        lock_time: 0,
        nonce: 9,
    };
    let bytes = tx.sighash_preimage_bound("agora-testnet-1", &genesis());
    assert_eq!(
        hex::encode(&bytes),
        "61676f72612d746c742d636f76656e616e742d736967686173682d626f756e642d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef8b0000001d00000061676f72612d746c742d636f76656e616e742d736967686173682d76310200000001000000040404040404040404040404040404040404040404040404040404040404040400000000ffffffff0100000001000000000000001a00000076a90114010101010101010101010101010101010101010188ac00000000000000000900000000000000"
    );
}
