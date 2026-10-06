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
