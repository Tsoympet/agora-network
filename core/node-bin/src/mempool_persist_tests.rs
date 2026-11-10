//! Pending TLT survives portable mempool reopen and stays RPC-queryable
//! without waiting for a block. Default-features nodes use RocksDB
//! `cf_mempool` instead of `AGORAMP1`. Neither path is a public-testnet
//! durability claim.

use std::sync::{Arc, Mutex};

use agora_crypto::{sign_transaction, KeyPair};
use agora_p2p::{mempool_persist_path, mempool_wal_path, Mempool, MempoolKv};
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

#[test]
fn pending_tlt_survives_store_mempool_cf_reopen() {
    use crate::mempool_kv::StoreMempoolKv;

    let tx = signed_spend();
    let id = tx.tx_id();
    let store = Arc::new(StateStore::open_in_memory());
    {
        let kv: Arc<dyn MempoolKv> = Arc::new(StoreMempoolKv(store.clone()));
        let mut pool = Mempool::open_persistent_kv(kv, 16).unwrap();
        pool.admit_priced(tx.clone(), 4).unwrap();
    }
    let kv: Arc<dyn MempoolKv> = Arc::new(StoreMempoolKv(store.clone()));
    let pool = Mempool::open_persistent_kv(kv, 16).unwrap();
    assert!(pool.contains(&id));
    assert_eq!(pool.fee_of(&id), Some(4));
    assert_eq!(
        store
            .scan_prefix(agora_state_machine::ColumnFamily::Mempool, &[])
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn successful_cf_migration_deletes_portable_files() {
    use crate::mempool_kv::{migrate_portable_into_kv, StoreMempoolKv};

    let dir = tempfile::tempdir().unwrap();
    let path = mempool_persist_path(dir.path());
    let wal = mempool_wal_path(dir.path());
    let tx = signed_spend();
    let id = tx.tx_id();
    {
        let mut pool = Mempool::open_persistent(&path, 16).unwrap();
        pool.admit_priced(tx, 4).unwrap();
    }
    assert!(path.exists() || wal.exists());

    let store = Arc::new(StateStore::open_in_memory());
    let kv: Arc<dyn MempoolKv> = Arc::new(StoreMempoolKv(store.clone()));
    let empty = Mempool::open_persistent_kv(kv.clone(), 16).unwrap();
    let pool = migrate_portable_into_kv(empty, kv, dir.path());
    assert!(pool.contains(&id));
    assert!(
        !path.exists(),
        "snapshot must be removed after CF migration"
    );
    assert!(!wal.exists(), "WAL must be removed after CF migration");
    assert!(!path.with_extension("bin.tmp").exists());

    let restored =
        Mempool::open_persistent_kv(Arc::new(StoreMempoolKv(store.clone())), 16).unwrap();
    assert!(restored.contains(&id));
    assert_eq!(
        store
            .scan_prefix(agora_state_machine::ColumnFamily::Mempool, &[])
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn rejected_portable_migration_keeps_files() {
    use crate::mempool_kv::{migrate_portable_into_kv, StoreMempoolKv};

    let dir = tempfile::tempdir().unwrap();
    let path = mempool_persist_path(dir.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"NOPE").unwrap();

    let store = Arc::new(StateStore::open_in_memory());
    let kv: Arc<dyn MempoolKv> = Arc::new(StoreMempoolKv(store));
    let empty = Mempool::open_persistent_kv(kv.clone(), 16).unwrap();
    let pool = migrate_portable_into_kv(empty, kv, dir.path());
    assert!(pool.is_empty());
    assert!(
        path.exists(),
        "corrupt portable snapshot must remain for retry"
    );
}

#[test]
fn portable_store_and_mempool_survive_process_exit_reopen() {
    use agora_p2p::{short_ids_for_block, NetworkMessage};
    use agora_state_machine::ColumnFamily;
    use agora_types::BlockHeader;

    let dir = tempfile::tempdir().unwrap();
    let tx = signed_spend();
    let id = tx.tx_id();
    {
        let store = StateStore::open_portable(dir.path()).unwrap();
        store.put_cf(ColumnFamily::Meta, b"keep", b"yes").unwrap();
        let mut pool = Mempool::open_persistent(mempool_persist_path(dir.path()), 16).unwrap();
        pool.admit_priced(tx.clone(), 4).unwrap();
    }

    let store = StateStore::open_portable(dir.path()).unwrap();
    assert_eq!(
        store.get_cf(ColumnFamily::Meta, b"keep").unwrap(),
        Some(b"yes".to_vec())
    );
    let pool = Mempool::open_persistent(mempool_persist_path(dir.path()), 16).unwrap();
    assert!(pool.contains(&id));
    assert_eq!(pool.fee_of(&id), Some(4));

    let mut utxo = agora_types::Block::utxo(
        BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 9,
            bits: 1,
            nonce: 0,
            tx_root: Hash::ZERO,
        },
        vec![tx.clone()],
    );
    utxo.header.tx_root = agora_types::Block::compute_tx_root(&utxo.transactions);
    match NetworkMessage::compact_from_block(&utxo) {
        NetworkMessage::CompactBlock { header, short_ids } => {
            assert_eq!(short_ids, short_ids_for_block(&utxo));
            let rebuilt = pool.inflate_compact_block(header, &short_ids).unwrap();
            assert_eq!(rebuilt.transactions, vec![tx]);
        }
        other => panic!("expected UTXO CompactBlock, got {other:?}"),
    }
}

#[cfg(feature = "rocksdb")]
#[test]
fn pending_tlt_survives_rocksdb_cf_reopen_and_inflates_compact() {
    use agora_p2p::{short_ids_for_block, NetworkMessage};
    use agora_types::BlockHeader;

    use crate::mempool_kv::StoreMempoolKv;

    let dir = tempfile::tempdir().unwrap();
    let tx = signed_spend();
    let id = tx.tx_id();
    {
        let store = Arc::new(StateStore::open(dir.path()).unwrap());
        let kv: Arc<dyn MempoolKv> = Arc::new(StoreMempoolKv(store));
        let mut pool = Mempool::open_persistent_kv(kv, 16).unwrap();
        pool.admit_priced(tx.clone(), 4).unwrap();
        assert!(
            !mempool_persist_path(dir.path()).exists(),
            "rocksdb path must not write AGORAMP1"
        );
    }

    let store = Arc::new(StateStore::open(dir.path()).unwrap());
    let kv: Arc<dyn MempoolKv> = Arc::new(StoreMempoolKv(store));
    let pool = Mempool::open_persistent_kv(kv, 16).unwrap();
    assert!(pool.contains(&id));
    assert_eq!(pool.fee_of(&id), Some(4));

    let mut utxo = agora_types::Block::utxo(
        BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 4,
            bits: 1,
            nonce: 0,
            tx_root: Hash::ZERO,
        },
        vec![tx.clone()],
    );
    utxo.header.tx_root = agora_types::Block::compute_tx_root(&utxo.transactions);
    match NetworkMessage::compact_from_block(&utxo) {
        NetworkMessage::CompactBlock { header, short_ids } => {
            assert_eq!(short_ids, short_ids_for_block(&utxo));
            let rebuilt = pool.inflate_compact_block(header, &short_ids).unwrap();
            assert_eq!(rebuilt.transactions, vec![tx.clone()]);
        }
        other => panic!("expected UTXO CompactBlock, got {other:?}"),
    }

    let backend = backend_with_pool(Arc::new(Mutex::new(pool)));
    let lookup = backend.get_transaction(&id).unwrap();
    assert_eq!(lookup.status.as_str(), "pending");
    assert_eq!(lookup.fee, Some(4));
}

#[cfg(feature = "rocksdb")]
#[test]
fn rocksdb_boot_migrates_portable_then_uses_cf() {
    use crate::mempool_kv::open_boot_mempool;

    let dir = tempfile::tempdir().unwrap();
    let path = mempool_persist_path(dir.path());
    let tx = signed_spend();
    let id = tx.tx_id();
    {
        let mut pool = Mempool::open_persistent(&path, 16).unwrap();
        pool.admit_priced(tx, 4).unwrap();
    }
    {
        let store = Arc::new(StateStore::open(dir.path()).unwrap());
        let pool = open_boot_mempool(&store, dir.path());
        assert!(pool.contains(&id));
        assert!(
            !path.exists(),
            "portable snapshot deleted after CF migration"
        );
        assert!(
            !mempool_wal_path(dir.path()).exists(),
            "portable WAL deleted after CF migration"
        );
    }
    let store = Arc::new(StateStore::open(dir.path()).unwrap());
    let pool = open_boot_mempool(&store, dir.path());
    assert!(pool.contains(&id));
    assert_eq!(
        store
            .scan_prefix(agora_state_machine::ColumnFamily::Mempool, &[])
            .unwrap()
            .len(),
        1
    );
}
