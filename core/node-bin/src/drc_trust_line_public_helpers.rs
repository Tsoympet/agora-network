//! Shared helpers for NodeBackend trust-line public integration tests.

use std::sync::{Arc, Mutex};

use agora_consensus::{LeadingZeroPow, PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher};
use agora_crypto::{
    sign_drc_issued_transfer_bound, sign_drc_trust_line_set_bound, KeyPair,
};
use agora_p2p::Mempool;
use agora_state_machine::{
    credit_account_into, load_drc_issuer_liability, load_drc_trust_line_live, GenesisBuilder,
    StateStore, WriteBatch,
};
use agora_types::{
    drc_trust_line_live_meta_key, Address, Amount, DrcIssuedTransferTx, DrcTrustLineSetTx, Hash,
    IssuedAmount, IssuedAssetId, IssuedCurrencyCode, NativeAssetId,
    DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION, DRC_TRUST_LINE_SET_TX_VERSION,
};

use super::{NodeBackend, NodeBackendConfig};
use agora_rpc::RpcBackend;
use crate::admit::{BlockTemplateLanes, ChainState};
use crate::storage_policy::StoragePolicy;

pub use super::drc_payment_channel_public_helpers::{
    account_reserved, backend_config, boot_chain, mine_template as mine_template_inner,
    submit_lanes_at_parents, ticket_consumer_reserved, virtual_tip, CHAIN,
};

fn coinbase_commitment_nonce(parents: &[Hash], timestamp_ms: u64, extranonce: u32) -> u64 {
    let mut sorted = parents.to_vec();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let tag = Hash::hash_borsh(&(b"agora-cb-parents-v2", sorted));
    let parent_tag = u32::from_le_bytes(tag.as_bytes()[..4].try_into().unwrap());
    let low = parent_tag ^ (timestamp_ms as u32);
    ((extranonce as u64) << 32) | u64::from(low)
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

pub fn drc_balance(store: &StateStore, addr: &Address) -> u64 {
    agora_state_machine::load_account(store, NativeAssetId::DRC, addr)
        .unwrap()
        .balance
}

pub fn line_balance(store: &StateStore, holder: &Address, ast: &IssuedAssetId) -> u64 {
    load_drc_trust_line_live(store, holder, ast)
        .ok()
        .flatten()
        .map(|l| l.balance.as_units())
        .unwrap_or(0)
}

pub fn issuer_outstanding(store: &StateStore, ast: &IssuedAssetId) -> u64 {
    load_drc_issuer_liability(store, ast).unwrap().as_units()
}

pub fn sum_holder_balances_for_asset(
    store: &StateStore,
    issuer: &Address,
    currency: &IssuedCurrencyCode,
) -> u64 {
    let ast = IssuedAssetId {
        issuer: *issuer,
        currency: *currency,
    };
    agora_state_machine::sum_holder_balances_for_asset(store, &ast)
        .unwrap()
        .as_units()
}

pub fn reward_pool_balance(store: &StateStore) -> u64 {
    agora_state_machine::load_reward_pool(store, NativeAssetId::DRC).unwrap_or(0)
}

pub struct TrustLineFixture {
    pub backend: NodeBackend,
    pub store: Arc<StateStore>,
    pub genesis: Hash,
    pub holder: KeyPair,
    pub issuer: KeyPair,
    pub holder_b: KeyPair,
    pub cur_usd: IssuedCurrencyCode,
    pub cur_eur: IssuedCurrencyCode,
}

pub fn funded_trust_line_fixture() -> TrustLineFixture {
    let store = Arc::new(StateStore::open_in_memory());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let holder = KeyPair::from_secret_bytes(&[0xa0; 32]).unwrap();
    let holder_b = KeyPair::from_secret_bytes(&[0xa1; 32]).unwrap();
    let issuer = KeyPair::from_secret_bytes(&[0xa2; 32]).unwrap();
    let miner = KeyPair::from_secret_bytes(&[0x99; 32]).unwrap();
    let mut funding = WriteBatch::new();
    for kp in [&holder, &holder_b, &issuer] {
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &kp.address(),
            Amount::from_base_units(5_000_000),
        )
        .unwrap();
    }
    store.write_batch(funding).unwrap();
    let backend = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store.clone(),
        Arc::new(Mutex::new(Mempool::new(128))),
        backend_config(genesis, miner.address()),
    );
    TrustLineFixture {
        backend,
        store,
        genesis,
        holder,
        issuer,
        holder_b,
        cur_usd: std_code(b"USD"),
        cur_eur: std_code(b"EUR"),
    }
}

pub fn signed_trust_line_set(
    holder: &KeyPair,
    issuer: &KeyPair,
    currency: IssuedCurrencyCode,
    limit: u64,
    genesis: Hash,
    nonce: u64,
) -> DrcTrustLineSetTx {
    let mut tx = DrcTrustLineSetTx {
        version: DRC_TRUST_LINE_SET_TX_VERSION,
        holder: holder.address(),
        issuer: issuer.address(),
        currency,
        limit: IssuedAmount::from_units(limit),
        fee: Amount::from_base_units(1),
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_trust_line_set_bound(&mut tx, holder, CHAIN, &genesis).unwrap();
    tx
}

pub fn signed_issued_transfer(
    sender: &KeyPair,
    recipient: Address,
    issuer: &KeyPair,
    currency: IssuedCurrencyCode,
    amount: u64,
    genesis: Hash,
    nonce: u64,
) -> DrcIssuedTransferTx {
    let mut tx = DrcIssuedTransferTx {
        version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
        sender: sender.address(),
        recipient,
        issuer: issuer.address(),
        currency,
        amount: IssuedAmount::from_units(amount),
        fee: Amount::from_base_units(1),
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_issued_transfer_bound(&mut tx, sender, CHAIN, &genesis).unwrap();
    tx
}

pub fn mine_template(backend: &mut NodeBackend) -> Hash {
    let mut block = backend.get_block_template().unwrap();
    assert!(assert_template_has_no_invalid_trust_line_lanes(&block));
    assert_eq!(block.header.tx_root, block.compute_body_root());
    block.header.nonce = 1;
    let pow = RandomXPowHasher.pow_hash(&block.header);
    LeadingZeroPow::new(PowAlgorithm::RandomX)
        .verify(&block.header, &pow)
        .unwrap();
    backend.submit_block(block).unwrap()
}

pub fn assert_template_has_no_invalid_trust_line_lanes(block: &agora_types::Block) -> bool {
    for tx in &block.drc_trust_line_sets {
        assert!(tx.validate_structure().is_ok());
    }
    for tx in &block.drc_issued_transfers {
        assert!(tx.validate_structure().is_ok());
        assert_eq!(tx.invoice_id, Hash::ZERO);
    }
    true
}

pub fn mempool_len(backend: &NodeBackend) -> usize {
    backend.test_mempool().lock().unwrap().len()
}

pub fn trust_line_mutation_reserved(backend: &NodeBackend, holder: &Address, ast: &IssuedAssetId) -> bool {
    backend
        .test_mempool()
        .lock()
        .unwrap()
        .trust_line_mutation_reserved(holder, &ast.asset_key())
}

pub fn trust_line_meta_reserved(backend: &NodeBackend, holder: &Address, ast: &IssuedAssetId) -> bool {
    let key = drc_trust_line_live_meta_key(holder, ast);
    backend
        .test_mempool()
        .lock()
        .unwrap()
        .trust_line_meta_key_reserved(&key)
}

pub fn issuer_liability_mutation_reserved(backend: &NodeBackend, ast: &IssuedAssetId) -> bool {
    backend
        .test_mempool()
        .lock()
        .unwrap()
        .issuer_liability_mutation_reserved(&ast.asset_key())
}

pub fn setup_live_line(
    backend: &mut NodeBackend,
    genesis: Hash,
    holder: &KeyPair,
    issuer: &KeyPair,
    currency: IssuedCurrencyCode,
    limit: u64,
    holder_nonce: u64,
) {
    let set = signed_trust_line_set(
        holder,
        issuer,
        currency,
        limit,
        genesis,
        holder_nonce,
    );
    backend.submit_drc_trust_line_set(set).unwrap();
    mine_template(backend);
}

pub fn assert_liability_equals_sum_balances(
    store: &StateStore,
    issuer: &Address,
    currency: &IssuedCurrencyCode,
) {
    let ast = IssuedAssetId {
        issuer: *issuer,
        currency: *currency,
    };
    assert_eq!(
        issuer_outstanding(store, &ast),
        sum_holder_balances_for_asset(store, issuer, currency)
    );
}

pub fn reorg_away_transfer_on_tip(
    backend: &mut NodeBackend,
    store: &StateStore,
    fund_tip: Hash,
    transfer: &DrcIssuedTransferTx,
) -> Hash {
    let ast = transfer.asset_id();
    let before = line_balance(store, &transfer.recipient, &ast);
    if before == 0 {
        let _ = submit_lanes_at_parents(
            backend,
            &[fund_tip],
            1_000,
            BlockTemplateLanes {
                drc_issued_transfers: std::slice::from_ref(transfer),
                ..BlockTemplateLanes::default()
            },
            61,
        );
    }
    assert!(line_balance(store, &transfer.recipient, &ast) > 0);
    let mut branch_tip = submit_lanes_at_parents(
        backend,
        &[fund_tip],
        2_000,
        BlockTemplateLanes::default(),
        62,
    );
    let mut extranonce = 63u32;
    for _ in 0..8 {
        if line_balance(store, &transfer.recipient, &ast) == 0 {
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
    assert_eq!(line_balance(store, &transfer.recipient, &ast), 0);
    branch_tip
}
