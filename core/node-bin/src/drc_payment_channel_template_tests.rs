//! Block-template, persistence, and public admission integration for DRC payment channels.

use std::sync::{Arc, Mutex};

use agora_consensus::{LeadingZeroPow, PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher};
use agora_crypto::{
    sign_drc_payment_channel_claim_bound, sign_drc_payment_channel_close_bound,
    sign_drc_payment_channel_create_bound, sign_drc_payment_channel_fund_bound,
    sign_payment_channel_offledger_claim, KeyPair,
};
use agora_p2p::Mempool;
use agora_state_machine::{
    credit_account_into, lookup_drc_payment_channel_point, GenesisBuilder, StateStore,
    TxAuthContext, WriteBatch,
};
use agora_types::{
    materialize_drc_multisign_attachments, Address, Amount, Block, DrcPaymentChannelClaimTx,
    DrcPaymentChannelCloseKind, DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx,
    DrcPaymentChannelFundTx, Hash, NativeAssetId, DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION,
    DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION, DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
    DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
};
use borsh::BorshDeserialize;
use serde_json::json;

use agora_rpc::RpcBackend;

use crate::admit::ChainState;
use crate::backend::{NodeBackend, NodeBackendConfig};
use crate::storage_policy::StoragePolicy;

const CHAIN: &str = "agora-dev";

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

fn boot_chain(store: Arc<StateStore>, genesis: Hash) -> ChainState {
    ChainState::bootstrap_with(
        store,
        genesis,
        crate::admit::ChainBootConfig {
            chain_id: CHAIN.into(),
            ..crate::admit::ChainBootConfig::default()
        },
        StoragePolicy::default(),
    )
    .unwrap()
}

fn funded_backend() -> (NodeBackend, Hash, KeyPair, KeyPair, KeyPair) {
    let store = Arc::new(StateStore::open_in_memory());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let owner = KeyPair::from_secret_bytes(&[0x70; 32]).unwrap();
    let destination = KeyPair::from_secret_bytes(&[0x71; 32]).unwrap();
    let claim_key = KeyPair::from_secret_bytes(&[0x72; 32]).unwrap();
    let mut funding = WriteBatch::new();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &owner.address(),
        Amount::from_base_units(500_000),
    )
    .unwrap();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &destination.address(),
        Amount::from_base_units(50_000),
    )
    .unwrap();
    store.write_batch(funding).unwrap();
    let chain = Arc::new(Mutex::new(boot_chain(store.clone(), genesis)));
    let backend = NodeBackend::new(
        chain,
        store,
        Arc::new(Mutex::new(Mempool::new(64))),
        NodeBackendConfig {
            miner_address: owner.address(),
            ..backend_config(genesis)
        },
    );
    (backend, genesis, owner, destination, claim_key)
}

fn signed_create(
    owner: &KeyPair,
    claim_key: &KeyPair,
    destination: Address,
    genesis: Hash,
    nonce: u64,
    amount: u64,
) -> DrcPaymentChannelCreateTx {
    let mut tx = DrcPaymentChannelCreateTx {
        version: DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
        owner: owner.address(),
        destination,
        amount: Amount::from_base_units(amount),
        fee: Amount::from_base_units(1),
        claim_public_key: claim_key.public_key_bytes().to_vec(),
        settle_delay_blue_scores: 5,
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        cancel_after_blue_score: Some(500),
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_payment_channel_create_bound(&mut tx, owner, CHAIN, &genesis).unwrap();
    tx
}

fn signed_fund(
    owner: &KeyPair,
    channel_id: Hash,
    genesis: Hash,
    nonce: u64,
    amount: u64,
) -> DrcPaymentChannelFundTx {
    let mut tx = DrcPaymentChannelFundTx {
        version: DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
        submitter: owner.address(),
        channel_id,
        amount: Amount::from_base_units(amount),
        fee: Amount::from_base_units(1),
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_payment_channel_fund_bound(&mut tx, owner, CHAIN, &genesis).unwrap();
    tx
}

fn signed_claim(
    dest: &KeyPair,
    claim_key: &KeyPair,
    channel_id: Hash,
    genesis: Hash,
    cumulative: u64,
    nonce: u64,
) -> DrcPaymentChannelClaimTx {
    let offledger = sign_payment_channel_offledger_claim(
        claim_key,
        CHAIN,
        &genesis,
        &channel_id,
        Amount::from_base_units(cumulative),
    )
    .unwrap();
    let mut tx = DrcPaymentChannelClaimTx {
        version: DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION,
        submitter: dest.address(),
        channel_id,
        cumulative_authorized: Amount::from_base_units(cumulative),
        channel_claim_signature: offledger.to_vec(),
        fee: Amount::from_base_units(1),
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_payment_channel_claim_bound(&mut tx, dest, CHAIN, &genesis).unwrap();
    tx
}

fn signed_close(
    submitter: &KeyPair,
    channel_id: Hash,
    genesis: Hash,
    kind: DrcPaymentChannelCloseKind,
    nonce: u64,
) -> DrcPaymentChannelCloseTx {
    let mut tx = DrcPaymentChannelCloseTx {
        version: DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION,
        submitter: submitter.address(),
        channel_id,
        close_kind: kind,
        fee: Amount::from_base_units(1),
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_payment_channel_close_bound(&mut tx, submitter, CHAIN, &genesis).unwrap();
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

fn drc_balance(store: &StateStore, addr: &Address) -> u64 {
    agora_state_machine::load_account(store, NativeAssetId::DRC, addr)
        .unwrap()
        .balance
}

#[test]
fn pending_create_blocks_public_fund_claim_and_close_admission() {
    let (mut backend, genesis, owner, destination, claim_key) = funded_backend();
    let create = signed_create(&owner, &claim_key, destination.address(), genesis, 0, 100);
    let channel_id = create.channel_id();
    backend
        .submit_drc_payment_channel_create(create.clone())
        .unwrap();
    assert_eq!(
        backend.get_drc_payment_channel(&channel_id).unwrap()["status"],
        json!("unknown")
    );

    let fund = signed_fund(&owner, channel_id, genesis, 1, 10);
    assert!(backend.submit_drc_payment_channel_fund(fund).is_err());

    let claim = signed_claim(&destination, &claim_key, channel_id, genesis, 5, 0);
    assert!(backend.submit_drc_payment_channel_claim(claim).is_err());

    let close = signed_close(
        &owner,
        channel_id,
        genesis,
        DrcPaymentChannelCloseKind::OwnerScheduleClose,
        2,
    );
    assert!(backend.submit_drc_payment_channel_close(close).is_err());

    mine_template(&mut backend);
    assert_eq!(
        backend.get_drc_payment_channel(&channel_id).unwrap()["status"],
        json!("live")
    );
}

#[test]
fn template_selects_payment_channel_lanes_in_order_with_valid_body_root() {
    let (mut backend, genesis, owner, destination, claim_key) = funded_backend();
    let create = signed_create(&owner, &claim_key, destination.address(), genesis, 0, 50);
    let channel_id = create.channel_id();
    backend.submit_drc_payment_channel_create(create).unwrap();
    mine_template(&mut backend);

    let fund = signed_fund(&owner, channel_id, genesis, 1, 20);
    backend.submit_drc_payment_channel_fund(fund).unwrap();

    let template = backend.get_block_template().unwrap();
    assert_eq!(template.drc_payment_channel_creates.len(), 0);
    assert_eq!(template.drc_payment_channel_funds.len(), 1);
    assert_eq!(template.header.tx_root, template.compute_body_root());
    let bytes = borsh::to_vec(&template).unwrap();
    let decoded: Block = BorshDeserialize::try_from_slice(&bytes).unwrap();
    assert_eq!(decoded.drc_payment_channel_funds.len(), 1);
    assert_eq!(decoded.header.tx_root, decoded.compute_body_root());
    let auth = TxAuthContext {
        chain_id: CHAIN.into(),
        genesis,
        data_availability_network_fingerprint: None,
    };
    let mut materialized = template.clone();
    materialize_drc_multisign_attachments(&mut materialized, &auth.chain_id, &auth.genesis)
        .expect("materialize");
    assert_eq!(
        materialized.header.tx_root,
        materialized.compute_body_root()
    );
}

#[test]
fn public_e2e_invariant_create_fund_two_claims_schedule_finalize() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(StateStore::open(dir.path()).unwrap());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let owner = KeyPair::from_secret_bytes(&[0x80; 32]).unwrap();
    let destination = KeyPair::from_secret_bytes(&[0x81; 32]).unwrap();
    let claim_key = KeyPair::from_secret_bytes(&[0x82; 32]).unwrap();
    let mut funding = WriteBatch::new();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &owner.address(),
        Amount::from_base_units(1_000_000),
    )
    .unwrap();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &destination.address(),
        Amount::from_base_units(100_000),
    )
    .unwrap();
    store.write_batch(funding).unwrap();

    let miner = KeyPair::from_secret_bytes(&[0x99; 32]).unwrap();
    let mut backend = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store.clone(),
        Arc::new(Mutex::new(Mempool::new(64))),
        NodeBackendConfig {
            miner_address: miner.address(),
            ..backend_config(genesis)
        },
    );

    let owner_before = drc_balance(store.as_ref(), &owner.address());
    let dest_before = drc_balance(store.as_ref(), &destination.address());

    let create = signed_create(&owner, &claim_key, destination.address(), genesis, 0, 200);
    let channel_id = create.channel_id();
    backend.submit_drc_payment_channel_create(create).unwrap();
    mine_template(&mut backend);

    backend
        .submit_drc_payment_channel_fund(signed_fund(&owner, channel_id, genesis, 1, 50))
        .unwrap();
    mine_template(&mut backend);

    backend
        .submit_drc_payment_channel_claim(signed_claim(
            &destination,
            &claim_key,
            channel_id,
            genesis,
            30,
            0,
        ))
        .unwrap();
    mine_template(&mut backend);

    backend
        .submit_drc_payment_channel_claim(signed_claim(
            &destination,
            &claim_key,
            channel_id,
            genesis,
            60,
            1,
        ))
        .unwrap();
    mine_template(&mut backend);

    backend
        .submit_drc_payment_channel_close(signed_close(
            &owner,
            channel_id,
            genesis,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            2,
        ))
        .unwrap();
    mine_template(&mut backend);

    for _ in 0..8 {
        mine_template(&mut backend);
    }

    backend
        .submit_drc_payment_channel_close(signed_close(
            &owner,
            channel_id,
            genesis,
            DrcPaymentChannelCloseKind::Finalize,
            3,
        ))
        .unwrap();
    mine_template(&mut backend);

    assert_eq!(
        backend.get_drc_payment_channel(&channel_id).unwrap()["status"],
        json!("receipt")
    );
    assert_eq!(
        backend
            .get_drc_payment_channel_receipt(&channel_id)
            .unwrap()["status"],
        json!("known")
    );

    let owner_after = drc_balance(store.as_ref(), &owner.address());
    let dest_after = drc_balance(store.as_ref(), &destination.address());
    assert!(
        dest_after > dest_before,
        "destination must receive claimed value net of fees"
    );
    assert!(
        owner_before + dest_before >= owner_after + dest_after,
        "only lane fees may destroy DRC supply in this flow"
    );

    drop(backend);
    drop(store);

    let reopened_store = Arc::new(StateStore::open(dir.path()).unwrap());
    let _reopened = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(reopened_store.clone(), genesis))),
        reopened_store.clone(),
        Arc::new(Mutex::new(Mempool::new(8))),
        backend_config(genesis),
    );
    assert_eq!(
        lookup_drc_payment_channel_point(reopened_store.as_ref(), &channel_id).unwrap(),
        "receipt"
    );
}

#[test]
fn verify_rpc_checks_offledger_claim_against_live_channel() {
    let (mut backend, genesis, owner, destination, claim_key) = funded_backend();
    let create = signed_create(&owner, &claim_key, destination.address(), genesis, 0, 80);
    let channel_id = create.channel_id();
    backend.submit_drc_payment_channel_create(create).unwrap();
    mine_template(&mut backend);

    let sig = sign_payment_channel_offledger_claim(
        &claim_key,
        CHAIN,
        &genesis,
        &channel_id,
        Amount::from_base_units(25),
    )
    .unwrap();
    let ok = backend
        .verify_drc_payment_channel_claim(&channel_id, Amount::from_base_units(25), &sig.to_vec())
        .unwrap();
    assert_eq!(ok["valid"], json!(true));

    assert!(backend
        .verify_drc_payment_channel_claim(&channel_id, Amount::from_base_units(999), &sig.to_vec(),)
        .is_err());
}

#[test]
fn destination_immediate_close_path_closes_channel() {
    let (mut backend, genesis, owner, destination, claim_key) = funded_backend();
    let create = signed_create(
        &owner,
        &claim_key,
        destination.address(),
        genesis,
        0,
        40,
    );
    let channel_id = create.channel_id();
    backend.submit_drc_payment_channel_create(create).unwrap();
    mine_template(&mut backend);
    backend
        .submit_drc_payment_channel_close(signed_close(
            &destination,
            channel_id,
            genesis,
            DrcPaymentChannelCloseKind::DestinationClose,
            0,
        ))
        .unwrap();
    mine_template(&mut backend);
    assert_eq!(
        backend.get_drc_payment_channel(&channel_id).unwrap()["status"],
        json!("receipt")
    );
}
