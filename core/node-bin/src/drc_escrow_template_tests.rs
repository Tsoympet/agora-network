//! Block-template and node persistence integration for native DRC escrow.

use std::sync::{Arc, Mutex};

use agora_consensus::{LeadingZeroPow, PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher};
use agora_crypto::{
    sign_drc_escrow_cancel_bound, sign_drc_escrow_create_bound, sign_drc_escrow_finish_bound,
    KeyPair,
};
use agora_p2p::Mempool;
use agora_state_machine::{
    credit_account_into, lookup_drc_escrow_point, GenesisBuilder, StateStore, WriteBatch,
};
use agora_types::{
    materialize_drc_multisign_attachments, Block, BlockHeader, DrcEscrowCancelTx,
    DrcEscrowCreateTx, DrcEscrowFinishTx, Hash, NativeAssetId, DRC_ESCROW_CANCEL_TX_VERSION,
    DRC_ESCROW_CREATE_TX_VERSION, DRC_ESCROW_FINISH_TX_VERSION,
};
use borsh::BorshDeserialize;
use serde_json::json;

use agora_rpc::RpcBackend;

use crate::admit::ChainState;
use crate::backend::{NodeBackend, NodeBackendConfig};
use crate::storage_policy::StoragePolicy;

fn funded_backend() -> (NodeBackend, Hash, KeyPair, KeyPair) {
    let store = Arc::new(StateStore::open_in_memory());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let owner = KeyPair::from_secret_bytes(&[0x50; 32]).unwrap();
    let recipient = KeyPair::from_secret_bytes(&[0x51; 32]).unwrap();
    let mut funding = WriteBatch::new();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &owner.address(),
        Amount::from_base_units(50_000),
    )
    .unwrap();
    store.write_batch(funding).unwrap();
    let chain = Arc::new(Mutex::new(
        ChainState::bootstrap_with(
            store.clone(),
            genesis,
            crate::admit::ChainBootConfig {
                chain_id: "agora-dev".into(),
                ..crate::admit::ChainBootConfig::default()
            },
            StoragePolicy::default(),
        )
        .unwrap(),
    ));
    let backend = NodeBackend::new(
        chain,
        store,
        Arc::new(Mutex::new(Mempool::new(64))),
        NodeBackendConfig {
            miner_address: owner.address(),
            ..backend_config(genesis)
        },
    );
    (backend, genesis, owner, recipient)
}

use agora_types::Amount;

fn boot_chain(store: Arc<StateStore>, genesis: Hash) -> ChainState {
    ChainState::bootstrap_with(
        store,
        genesis,
        crate::admit::ChainBootConfig {
            chain_id: "agora-dev".into(),
            ..crate::admit::ChainBootConfig::default()
        },
        StoragePolicy::default(),
    )
    .unwrap()
}

fn backend_config(genesis_hash: Hash) -> NodeBackendConfig {
    NodeBackendConfig {
        net: None,
        allow_fund: false,
        miner_address: Address::ZERO,
        connected_peers: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        network: "dev".into(),
        genesis_hash,
    }
}

use agora_types::Address;

fn signed_create(
    owner: &KeyPair,
    recipient: agora_types::Address,
    genesis: Hash,
    nonce: u64,
    cancel_after: Option<u64>,
) -> DrcEscrowCreateTx {
    let mut tx = DrcEscrowCreateTx {
        version: DRC_ESCROW_CREATE_TX_VERSION,
        owner: owner.address(),
        recipient,
        amount: Amount::from_base_units(20),
        fee: Amount::from_base_units(1),
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        finish_after_blue_score: None,
        cancel_after_blue_score: cancel_after,
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_escrow_create_bound(&mut tx, owner, "agora-dev", &genesis).unwrap();
    tx
}

fn mine_template(backend: &mut NodeBackend) -> Hash {
    let mut block = backend.get_block_template().unwrap();
    assert_eq!(block.header.tx_root, block.compute_body_root());
    block.header.nonce = 1;
    let pow = RandomXPowHasher.pow_hash(&block.header);
    LeadingZeroPow::new(PowAlgorithm::RandomX)
        .verify(&block.header, &pow)
        .unwrap();
    backend.submit_block(block).unwrap()
}

#[test]
fn pending_create_blocks_public_finish_and_cancel_admission() {
    let (mut backend, genesis, owner, recipient) = funded_backend();
    let create = signed_create(&owner, recipient.address(), genesis, 0, Some(100));
    let id = backend.submit_drc_escrow_create(create.clone()).unwrap();
    assert_eq!(
        backend.get_drc_escrow(&id).unwrap()["status"],
        json!("unknown")
    );

    let mut finish = DrcEscrowFinishTx {
        version: DRC_ESCROW_FINISH_TX_VERSION,
        submitter: owner.address(),
        escrow_id: id,
        fee: Amount::from_base_units(1),
        nonce: 1,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_escrow_finish_bound(&mut finish, &owner, "agora-dev", &genesis).unwrap();
    assert!(backend.submit_drc_escrow_finish(finish).is_err());

    let mut cancel = DrcEscrowCancelTx {
        version: DRC_ESCROW_CANCEL_TX_VERSION,
        submitter: owner.address(),
        escrow_id: id,
        fee: Amount::from_base_units(1),
        nonce: 1,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_escrow_cancel_bound(&mut cancel, &owner, "agora-dev", &genesis).unwrap();
    assert!(backend.submit_drc_escrow_cancel(cancel).is_err());

    let template = backend.get_block_template().unwrap();
    assert_eq!(template.drc_escrow_creates, vec![create]);
    assert!(template.drc_escrow_finishes.is_empty());
    assert!(template.drc_escrow_cancels.is_empty());
    assert!(template.drc_multisign_attachments.is_empty());
}

#[test]
fn template_includes_single_settlement_and_rejects_finish_cancel_conflict() {
    let (mut backend, genesis, owner, recipient) = funded_backend();
    let create = signed_create(&owner, recipient.address(), genesis, 0, Some(100));
    let id = create.escrow_id();
    backend.submit_drc_escrow_create(create).unwrap();
    mine_template(&mut backend);
    assert_eq!(
        backend.get_drc_escrow(&id).unwrap()["status"],
        json!("live")
    );

    let mut finish = DrcEscrowFinishTx {
        version: DRC_ESCROW_FINISH_TX_VERSION,
        submitter: owner.address(),
        escrow_id: id,
        fee: Amount::from_base_units(1),
        nonce: 1,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_escrow_finish_bound(&mut finish, &owner, "agora-dev", &genesis).unwrap();
    backend.submit_drc_escrow_finish(finish.clone()).unwrap();

    let mut cancel = DrcEscrowCancelTx {
        version: DRC_ESCROW_CANCEL_TX_VERSION,
        submitter: owner.address(),
        escrow_id: id,
        fee: Amount::from_base_units(1),
        nonce: 2,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_escrow_cancel_bound(&mut cancel, &owner, "agora-dev", &genesis).unwrap();
    assert!(backend.submit_drc_escrow_cancel(cancel).is_err());

    let template = backend.get_block_template().unwrap();
    assert_eq!(template.drc_escrow_finishes.len(), 1);
    assert!(template.drc_escrow_cancels.is_empty());
    assert_eq!(template.drc_escrow_finishes[0].escrow_id, id);
}

#[test]
fn template_materializes_multisign_attachments_and_borsh_roundtrip() {
    use agora_state_machine::TxAuthContext;
    use borsh::BorshDeserialize;

    let store = Arc::new(StateStore::open_in_memory());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let owner = KeyPair::from_secret_bytes(&[0x52; 32]).unwrap();
    let recipient = KeyPair::from_secret_bytes(&[0x53; 32]).unwrap();
    let mut funding = WriteBatch::new();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &owner.address(),
        Amount::from_base_units(20_000),
    )
    .unwrap();
    store.write_batch(funding).unwrap();
    let auth = TxAuthContext {
        chain_id: "agora-dev".into(),
        genesis,
        data_availability_network_fingerprint: None,
    };
    let chain = Arc::new(Mutex::new(
        ChainState::bootstrap_with(
            store.clone(),
            genesis,
            crate::admit::ChainBootConfig {
                chain_id: "agora-dev".into(),
                ..crate::admit::ChainBootConfig::default()
            },
            StoragePolicy::default(),
        )
        .unwrap(),
    ));
    let mut backend = NodeBackend::new(
        chain,
        store,
        Arc::new(Mutex::new(Mempool::new(8))),
        backend_config(genesis),
    );
    backend
        .submit_drc_escrow_create(signed_create(
            &owner,
            recipient.address(),
            genesis,
            0,
            Some(100),
        ))
        .unwrap();
    let template = backend.get_block_template().unwrap();
    let bytes = borsh::to_vec(&template).unwrap();
    let decoded: Block = BorshDeserialize::try_from_slice(&bytes).unwrap();
    assert_eq!(decoded.header.tx_root, template.compute_body_root());
    assert_eq!(decoded.drc_escrow_creates.len(), 1);
    assert!(template.drc_multisign_attachments.is_empty());
    let mut materialized = template.clone();
    materialize_drc_multisign_attachments(&mut materialized, &auth.chain_id, &auth.genesis)
        .expect("single-sig template has no inline multisign to materialize");
    assert!(materialized.drc_multisign_attachments.is_empty());
}

#[test]
fn mined_escrow_survives_backend_reopen_and_rpc_queries() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(StateStore::open(dir.path()).unwrap());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let owner = KeyPair::from_secret_bytes(&[0x55; 32]).unwrap();
    let recipient = KeyPair::from_secret_bytes(&[0x56; 32]).unwrap();
    let mut funding = WriteBatch::new();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &owner.address(),
        Amount::from_base_units(20_000),
    )
    .unwrap();
    store.write_batch(funding).unwrap();
    let mut backend = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store.clone(),
        Arc::new(Mutex::new(Mempool::new(64))),
        NodeBackendConfig {
            miner_address: owner.address(),
            ..backend_config(genesis)
        },
    );
    let create = signed_create(&owner, recipient.address(), genesis, 0, Some(100));
    let id = create.escrow_id();
    backend.submit_drc_escrow_create(create).unwrap();
    mine_template(&mut backend);
    assert_eq!(
        backend.get_drc_escrow(&id).unwrap()["status"],
        json!("live")
    );
    drop(backend);
    drop(store);

    let reopened_store = Arc::new(StateStore::open(dir.path()).unwrap());
    let backend2 = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(reopened_store.clone(), genesis))),
        reopened_store.clone(),
        Arc::new(Mutex::new(Mempool::new(64))),
        NodeBackendConfig {
            miner_address: owner.address(),
            ..backend_config(genesis)
        },
    );
    assert_eq!(
        lookup_drc_escrow_point(reopened_store.as_ref(), &id).unwrap(),
        "live"
    );
    assert_eq!(
        backend2.get_drc_escrow(&id).unwrap()["status"],
        json!("live")
    );
    drop(backend2);
    drop(reopened_store);

    let mut finish = DrcEscrowFinishTx {
        version: DRC_ESCROW_FINISH_TX_VERSION,
        submitter: owner.address(),
        escrow_id: id,
        fee: Amount::from_base_units(1),
        nonce: 1,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_escrow_finish_bound(&mut finish, &owner, "agora-dev", &genesis).unwrap();
    let reopened_store = Arc::new(StateStore::open(dir.path()).unwrap());
    let mut backend3 = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(reopened_store.clone(), genesis))),
        reopened_store.clone(),
        Arc::new(Mutex::new(Mempool::new(64))),
        NodeBackendConfig {
            miner_address: owner.address(),
            ..backend_config(genesis)
        },
    );
    backend3.submit_drc_escrow_finish(finish).unwrap();
    mine_template(&mut backend3);
    assert!(backend3.get_drc_escrow_receipt(&id).unwrap().is_object());
    drop(backend3);
    drop(reopened_store);

    let finish_store = Arc::new(StateStore::open(dir.path()).unwrap());
    let after_finish = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(finish_store.clone(), genesis))),
        finish_store,
        Arc::new(Mutex::new(Mempool::new(64))),
        backend_config(genesis),
    );
    assert_eq!(
        after_finish.get_drc_escrow_receipt(&id).unwrap()["outcome"],
        json!("finished")
    );
    drop(after_finish);

    let create2 = signed_create(&owner, recipient.address(), genesis, 2, Some(1));
    let id2 = create2.escrow_id();
    let reopened_store = Arc::new(StateStore::open(dir.path()).unwrap());
    let mut backend_cancel = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(reopened_store.clone(), genesis))),
        reopened_store.clone(),
        Arc::new(Mutex::new(Mempool::new(64))),
        NodeBackendConfig {
            miner_address: owner.address(),
            ..backend_config(genesis)
        },
    );
    backend_cancel.submit_drc_escrow_create(create2).unwrap();
    mine_template(&mut backend_cancel);
    let mut cancel = DrcEscrowCancelTx {
        version: DRC_ESCROW_CANCEL_TX_VERSION,
        submitter: owner.address(),
        escrow_id: id2,
        fee: Amount::from_base_units(1),
        nonce: 3,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_escrow_cancel_bound(&mut cancel, &owner, "agora-dev", &genesis).unwrap();
    backend_cancel.submit_drc_escrow_cancel(cancel).unwrap();
    mine_template(&mut backend_cancel);
    drop(backend_cancel);
    drop(reopened_store);

    let cancel_store = Arc::new(StateStore::open(dir.path()).unwrap());
    let after_cancel = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(cancel_store.clone(), genesis))),
        cancel_store,
        Arc::new(Mutex::new(Mempool::new(64))),
        backend_config(genesis),
    );
    assert_eq!(
        after_cancel.get_drc_escrow_receipt(&id2).unwrap()["outcome"],
        json!("cancelled")
    );
    assert!(after_cancel
        .get_drc_escrow_receipt(&id2)
        .unwrap()
        .is_object());
}
