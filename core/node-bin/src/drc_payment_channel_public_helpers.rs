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
    credit_account_into, load_drc_payment_channel_live, put_issued_supply_into, GenesisBuilder,
    StateStore, WriteBatch,
};
use agora_types::{
    Address, Amount, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseKind,
    DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, Hash,
    NativeAssetId, DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION, DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION,
    DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION, DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
};

fn coinbase_commitment_nonce(parents: &[Hash], timestamp_ms: u64, extranonce: u32) -> u64 {
    let mut sorted = parents.to_vec();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let tag = Hash::hash_borsh(&(b"agora-cb-parents-v2", sorted));
    let parent_tag = u32::from_le_bytes(tag.as_bytes()[..4].try_into().unwrap());
    let low = parent_tag ^ (timestamp_ms as u32);
    ((extranonce as u64) << 32) | u64::from(low)
}

use agora_rpc::RpcBackend;

use super::{NodeBackend, NodeBackendConfig};
use crate::admit::{BlockTemplateLanes, ChainState};
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
    put_issued_supply_into(&mut funding, NativeAssetId::DRC, 2_200_000);
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

pub fn payment_channel_mutation_reserved(backend: &NodeBackend, channel_id: &Hash) -> bool {
    backend
        .test_mempool()
        .lock()
        .unwrap()
        .payment_channel_mutation_reserved(channel_id)
}

pub fn ticket_consumer_reserved(backend: &NodeBackend, account: &Address, sequence: u64) -> bool {
    backend
        .test_mempool()
        .lock()
        .unwrap()
        .ticket_consumer_reserved(account, sequence)
}

/// Mine a block with explicit parents and lane payloads through live `submit_block`.
pub fn submit_lanes_at_parents(
    backend: &mut NodeBackend,
    parents: &[Hash],
    timestamp_ms: u64,
    lanes: BlockTemplateLanes<'_>,
    coinbase_extranonce: u32,
) -> Hash {
    let miner = backend.test_miner();
    let chain = backend.test_chain().lock().unwrap();
    let parent_ts = parents
        .iter()
        .filter_map(|parent| chain.load_block(parent).ok().flatten())
        .map(|block| block.header.timestamp_ms)
        .max()
        .unwrap_or(0);
    // `timestamp_ms` is a minimum delta above the parent (not an absolute clock time).
    let timestamp_ms = parent_ts.saturating_add(timestamp_ms.max(1_000));
    let mut block = chain.block_template_lanes(miner, lanes).unwrap();
    block.header.parents = parents.to_vec();
    block.header.bits = chain.expected_bits_for_parents(parents).unwrap();
    block.header.timestamp_ms = timestamp_ms;
    if let Some(cb) = block
        .transactions
        .iter_mut()
        .find(|tx| tx.inputs.is_empty())
    {
        cb.nonce = coinbase_commitment_nonce(parents, timestamp_ms, coinbase_extranonce);
    }
    block.header.tx_root = block.compute_body_root();
    block.header.nonce = 1;
    let pow = RandomXPowHasher.pow_hash(&block.header);
    LeadingZeroPow::new(PowAlgorithm::RandomX)
        .verify(&block.header, &pow)
        .unwrap();
    drop(chain);
    backend.submit_block(block).unwrap()
}

pub fn virtual_tip(backend: &NodeBackend) -> Hash {
    backend.test_chain().lock().unwrap().virtual_tip().unwrap()
}

pub fn live_cumulative_claimed(store: &StateStore, channel_id: &Hash) -> u64 {
    load_drc_payment_channel_live(store, channel_id)
        .ok()
        .flatten()
        .map(|live| live.cumulative_claimed.as_base_units())
        .unwrap_or(0)
}

/// Apply a sibling claim block, then reorg it away by extending an alternate empty branch.
pub fn reorg_away_claim_on_fund_tip(
    backend: &mut NodeBackend,
    store: &StateStore,
    fund_tip: Hash,
    claim: &agora_types::DrcPaymentChannelClaimTx,
) -> Hash {
    if live_cumulative_claimed(store, &claim.channel_id) == 0 {
        let _claim_tip = submit_lanes_at_parents(
            backend,
            &[fund_tip],
            1_000,
            BlockTemplateLanes {
                drc_payment_channel_claims: std::slice::from_ref(claim),
                ..BlockTemplateLanes::default()
            },
            51,
        );
    }
    assert!(live_cumulative_claimed(store, &claim.channel_id) > 0);
    let mut branch_tip = submit_lanes_at_parents(
        backend,
        &[fund_tip],
        2_000,
        BlockTemplateLanes::default(),
        52,
    );
    let mut extranonce = 53u32;
    for _ in 0..8 {
        if live_cumulative_claimed(store, &claim.channel_id) == 0 {
            break;
        }
        branch_tip = submit_lanes_at_parents(
            backend,
            &[branch_tip],
            1_000,
            BlockTemplateLanes::default(),
            extranonce,
        );
        extranonce += 1;
    }
    assert_eq!(
        live_cumulative_claimed(store, &claim.channel_id),
        0,
        "virtual reorg must restore pre-claim cumulative"
    );
    branch_tip
}

pub fn reward_pool_balance(store: &StateStore) -> u64 {
    agora_state_machine::load_reward_pool(store, NativeAssetId::DRC).unwrap_or(0)
}

pub fn burned_supply_balance(store: &StateStore) -> u64 {
    agora_state_machine::load_burned_supply(store, NativeAssetId::DRC).unwrap()
}

pub fn channel_conservation_quad(
    store: &StateStore,
    owner: &Address,
    destination: &Address,
    channel_id: &Hash,
) -> (u64, u64, u64, u64) {
    (
        drc_balance(store, owner),
        drc_balance(store, destination),
        locked_remainder(store, channel_id),
        burned_supply_balance(store),
    )
}
