//! Borsh wire roundtrip for escrow gossip messages and block transport.

use agora_p2p::NetworkMessage;
use agora_types::{Block, BlockHeader, Hash};

#[test]
fn network_message_escrow_variants_borsh_roundtrip() {
    let create = agora_types::DrcEscrowCreateTx {
        version: agora_types::DRC_ESCROW_CREATE_TX_VERSION,
        owner: agora_types::Address([1; 20]),
        recipient: agora_types::Address([2; 20]),
        amount: agora_types::Amount::from_base_units(5),
        fee: agora_types::Amount::from_base_units(1),
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        finish_after_blue_score: None,
        cancel_after_blue_score: Some(50),
        nonce: 0,
        account_sequence: None,
        public_key: vec![3; 33],
        signature: vec![4; 64],
        multisign: None,
    };
    for msg in [
        NetworkMessage::DrcEscrowCreate(create.clone()),
        NetworkMessage::DrcEscrowFinish(agora_types::DrcEscrowFinishTx {
            version: agora_types::DRC_ESCROW_FINISH_TX_VERSION,
            submitter: create.owner,
            escrow_id: create.escrow_id(),
            fee: agora_types::Amount::from_base_units(1),
            nonce: 1,
            account_sequence: None,
            public_key: vec![3; 33],
            signature: vec![4; 64],
            multisign: None,
        }),
        NetworkMessage::DrcEscrowCancel(agora_types::DrcEscrowCancelTx {
            version: agora_types::DRC_ESCROW_CANCEL_TX_VERSION,
            submitter: create.owner,
            escrow_id: create.escrow_id(),
            fee: agora_types::Amount::from_base_units(1),
            nonce: 2,
            account_sequence: None,
            public_key: vec![3; 33],
            signature: vec![4; 64],
            multisign: None,
        }),
    ] {
        let bytes = msg.encode();
        let decoded = NetworkMessage::decode(&bytes).expect("decode escrow gossip");
        assert_eq!(std::mem::discriminant(&msg), std::mem::discriminant(&decoded));
    }
}

#[test]
fn full_block_with_escrow_lanes_borsh_roundtrip() {
    let mut block = Block {
        header: BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 42,
            bits: 1,
            nonce: 0,
            tx_root: Hash::ZERO,
        },
        transactions: vec![],
        account_transfers: vec![],
        stake_ops: vec![],
        ovl_executions: vec![],
        drc_payments: vec![],
        data_commitments: vec![],
        drc_account_policies: vec![],
        drc_deposit_preauths: vec![],
        drc_regular_keys: vec![],
        drc_signer_lists: vec![],
        drc_ticket_creates: vec![],
        drc_escrow_creates: vec![agora_types::DrcEscrowCreateTx {
            version: agora_types::DRC_ESCROW_CREATE_TX_VERSION,
            owner: agora_types::Address([5; 20]),
            recipient: agora_types::Address([6; 20]),
            amount: agora_types::Amount::from_base_units(1),
            fee: agora_types::Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(10),
            nonce: 0,
            account_sequence: None,
            public_key: vec![1; 33],
            signature: vec![2; 64],
            multisign: None,
        }],
        drc_escrow_finishes: vec![],
        drc_escrow_cancels: vec![],
        drc_multisign_attachments: vec![],
    };
    block.header.tx_root = block.compute_body_root();
    let bytes = borsh::to_vec(&block).unwrap();
    let decoded: Block = borsh::from_slice(&bytes).unwrap();
    assert_eq!(decoded.drc_escrow_creates.len(), 1);
    assert_eq!(decoded.header.tx_root, block.header.tx_root);
}
