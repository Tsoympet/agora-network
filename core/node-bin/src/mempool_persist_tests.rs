//! Pending TLT survives portable mempool reopen and stays RPC-queryable
//! without waiting for a block. This is a `--no-default-features` datadir
//! snapshot (`AGORAMP1`), not a public-testnet durability claim.

use std::sync::{Arc, Mutex};

use agora_crypto::{sign_transaction, KeyPair};
use agora_p2p::{mempool_persist_path, Mempool};
use agora_rpc::RpcBackend;
use agora_state_machine::{GenesisBuilder, StateStore};
use agora_types::{Amount, Hash, OutPoint, Transaction, TxIn, TxOut};

use crate::admit::ChainState;
use crate::backend::{NodeBackend, NodeBackendConfig};
use crate::storage_policy::StoragePolicy;

fn signed_spend() -> Transaction {
    let kp = KeyPair::from_secret_bytes(&[0x42; 32]).unwrap();
    let mut tx = Transaction::unsigned(
        1,
        vec![TxIn {
            previous_outpoint: OutPoint {
                tx_id: Hash::ZERO,
                index: 9,
            },
        }],
        vec![TxOut {
            value: Amount::from_base_units(1),
            address: kp.address(),
        }],
        3,
    );
    sign_transaction(&mut tx, &kp).unwrap();
    tx
}

fn backend_with_pool(mempool: Arc<Mutex<Mempool>>) -> NodeBackend {
    let store = Arc::new(StateStore::open_in_memory());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let chain = Arc::new(Mutex::new(
        ChainState::bootstrap(
            store.clone(),
            genesis,
            agora_consensus::PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap(),
    ));
    NodeBackend::new(
        chain,
        store,
        mempool,
        NodeBackendConfig {
            net: None,
            allow_fund: false,
            miner_address: agora_types::Address::ZERO,
            connected_peers: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            network: "dev".into(),
            genesis_hash: genesis,
        },
    )
}

#[test]
fn pending_tlt_survives_reopen_and_rpc_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let path = mempool_persist_path(dir.path());
    let tx = signed_spend();
    let id = tx.tx_id();

    {
        let mut pool = Mempool::open_persistent(&path, 16).unwrap();
        pool.admit_priced(tx.clone(), 4).unwrap();
    }

    let pool = Mempool::open_persistent(&path, 16).unwrap();
    assert!(pool.contains(&id));
    let backend = backend_with_pool(Arc::new(Mutex::new(pool)));

    let lookup = backend.get_transaction(&id).unwrap();
    assert_eq!(lookup.status.as_str(), "pending");
    assert_eq!(lookup.fee, Some(4));
    assert_eq!(
        lookup.transaction.as_ref().map(|pending| pending.tx_id()),
        Some(id)
    );

    let mempool = backend.get_mempool(8).unwrap();
    assert_eq!(mempool.len(), 1);
    assert_eq!(mempool[0].tx_id, id);
    assert_eq!(mempool[0].fee, Some(4));
}
