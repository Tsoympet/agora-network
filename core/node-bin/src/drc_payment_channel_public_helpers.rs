//! Shared helpers for NodeBackend payment-channel public integration tests.

use std::sync::{Arc, Mutex};

use agora_consensus::{LeadingZeroPow, PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher};
use agora_crypto::{
    sign_drc_payment_channel_claim_bound, sign_drc_payment_channel_close_bound,
    sign_drc_payment_channel_create_bound, sign_drc_payment_channel_fund_bound,
    sign_payment_channel_offledger_claim, KeyPair,
};
use agora_p2p::Mempool;
use agora_state_machine::{
    credit_account_into, load_drc_payment_channel_live, GenesisBuilder, StateStore, WriteBatch,
};
use agora_types::{
    Address, Amount, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseKind,
    DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, Hash,
    NativeAssetId, DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION, DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION,
    DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION, DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
};

use agora_rpc::RpcBackend;

use super::{NodeBackend, NodeBackendConfig};
use crate::admit::ChainState;
use crate::storage_policy::StoragePolicy;

pub const CHAIN: &str = "agora-dev";

pub fn backend_config(genesis_hash: Hash, miner: Address) -> NodeBackendConfig {
    NodeBackendConfig {
        net: None,
        allow_fund: false,
        miner_address: miner,
        connected_peers: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        network: "dev".into(),
        genesis_hash,
    }
}

pub fn boot_chain(store: Arc<StateStore>, genesis: Hash) -> ChainState {
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

pub fn drc_balance(store: &StateStore, addr: &Address) -> u64 {
    agora_state_machine::load_account(store, NativeAssetId::DRC, addr)
        .unwrap()
        .balance
}

pub fn locked_remainder(store: &StateStore, channel_id: &Hash) -> u64 {
    load_drc_payment_channel_live(store, channel_id)
        .ok()
        .flatten()
        .map(|live| {
            live.total_funded
                .as_base_units()
                .saturating_sub(live.cumulative_claimed.as_base_units())
        })
        .unwrap_or(0)
}

/// Owner + destination spendable DRC plus locked remainder for one channel (if live).
pub fn spendable_plus_locked(
    store: &StateStore,
    owner: &Address,
    destination: &Address,
    channel_id: &Hash,
) -> u64 {
    drc_balance(store, owner)
        + drc_balance(store, destination)
        + locked_remainder(store, channel_id)
}

pub fn assert_conservation_after_fee(previous_total: u64, current_total: u64, explicit_fee: u64) {
    assert_eq!(
        previous_total,
        current_total + explicit_fee,
        "DRC conservation: expected total to drop by explicit fee only"
    );
}

pub struct FundedChannelFixture {
    pub backend: NodeBackend,
    pub store: Arc<StateStore>,
    pub genesis: Hash,
    pub owner: KeyPair,
    pub destination: KeyPair,
    pub claim_key: KeyPair,
}

pub fn funded_fixture() -> FundedChannelFixture {
    let store = Arc::new(StateStore::open_in_memory());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let owner = KeyPair::from_secret_bytes(&[0x70; 32]).unwrap();
    let destination = KeyPair::from_secret_bytes(&[0x71; 32]).unwrap();
    let claim_key = KeyPair::from_secret_bytes(&[0x72; 32]).unwrap();
    let miner = KeyPair::from_secret_bytes(&[0x99; 32]).unwrap();
    let mut funding = WriteBatch::new();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &owner.address(),
        Amount::from_base_units(2_000_000),
    )
    .unwrap();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &destination.address(),
        Amount::from_base_units(200_000),
    )
    .unwrap();
    store.write_batch(funding).unwrap();
    let backend = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store.clone(),
        Arc::new(Mutex::new(Mempool::new(64))),
        backend_config(genesis, miner.address()),
    );
    FundedChannelFixture {
        backend,
        store,
        genesis,
        owner,
        destination,
        claim_key,
    }
}

pub fn signed_create(
    owner: &KeyPair,
    claim_key: &KeyPair,
    destination: Address,
    genesis: Hash,
    nonce: u64,
    amount: u64,
    cancel_after: Option<u64>,
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
        cancel_after_blue_score: cancel_after,
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_payment_channel_create_bound(&mut tx, owner, CHAIN, &genesis).unwrap();
    tx
}

pub fn signed_fund(
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

pub fn signed_claim(
    dest: &KeyPair,
    claim_key: &KeyPair,
    channel_id: Hash,
    _genesis: Hash,
    cumulative: u64,
    nonce: u64,
    chain_id: &str,
    genesis_hash: &Hash,
) -> DrcPaymentChannelClaimTx {
    let offledger = sign_payment_channel_offledger_claim(
        claim_key,
        chain_id,
        genesis_hash,
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
    sign_drc_payment_channel_claim_bound(&mut tx, dest, CHAIN, genesis_hash).unwrap();
    tx
}

pub fn signed_close(
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

pub fn mine_template(backend: &mut NodeBackend) -> Hash {
    let mut block = backend.get_block_template().unwrap();
    assert_eq!(block.header.tx_root, block.compute_body_root());
    assert!(assert_template_has_no_invalid_payment_channel_lanes(&block));
    block.header.nonce = 1;
    let pow = RandomXPowHasher.pow_hash(&block.header);
    LeadingZeroPow::new(PowAlgorithm::RandomX)
        .verify(&block.header, &pow)
        .unwrap();
    backend.submit_block(block).unwrap()
}

pub fn assert_template_has_no_invalid_payment_channel_lanes(block: &agora_types::Block) -> bool {
    for tx in &block.drc_payment_channel_creates {
        assert!(tx.validate_structure().is_ok());
    }
    for tx in &block.drc_payment_channel_funds {
        assert!(tx.validate_structure().is_ok());
    }
    for tx in &block.drc_payment_channel_claims {
        assert!(tx.validate_structure().is_ok());
        assert!(!tx.channel_claim_signature.is_empty());
    }
    for tx in &block.drc_payment_channel_closes {
        assert!(tx.validate_structure().is_ok());
    }
    true
}

pub fn mempool_len(backend: &NodeBackend) -> usize {
    backend.test_mempool().lock().unwrap().len()
}

pub fn account_reserved(backend: &NodeBackend, account: &Address) -> bool {
    backend
        .test_mempool()
        .lock()
        .unwrap()
        .account_reserved(NativeAssetId::DRC, account)
}
