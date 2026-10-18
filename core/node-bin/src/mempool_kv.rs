//! Default-features mempool persistence through RocksDB `cf_mempool`.
//!
//! `--no-default-features` keeps the portable WAL/snapshot files. This is not
//! a public-testnet durability claim.

use std::path::Path;
use std::sync::Arc;

use agora_p2p::{mempool_persist_path, Mempool};
#[cfg(any(feature = "rocksdb", test))]
use agora_p2p::{MempoolKv, P2pError};
#[cfg(any(feature = "rocksdb", test))]
use agora_state_machine::ColumnFamily;
use agora_state_machine::StateStore;
#[cfg(any(feature = "rocksdb", test))]
use agora_types::Hash;
use tracing::warn;

#[cfg(any(feature = "rocksdb", test))]
pub(crate) struct StoreMempoolKv(pub Arc<StateStore>);

#[cfg(any(feature = "rocksdb", test))]
impl MempoolKv for StoreMempoolKv {
    fn put(&self, id: &Hash, payload: &[u8]) -> Result<(), P2pError> {
        self.0
            .put_cf(ColumnFamily::Mempool, id.as_bytes(), payload)
            .map_err(|error| P2pError::Decode(error.to_string()))
    }

    fn delete(&self, id: &Hash) -> Result<(), P2pError> {
        self.0
            .delete_cf(ColumnFamily::Mempool, id.as_bytes())
            .map_err(|error| P2pError::Decode(error.to_string()))
    }

    fn load_all(&self) -> Result<Vec<Vec<u8>>, P2pError> {
        Ok(self
            .0
            .scan_prefix(ColumnFamily::Mempool, &[])
            .map_err(|error| P2pError::Decode(error.to_string()))?
            .into_iter()
            .map(|(_, value)| value)
            .collect())
    }
}

pub(crate) fn open_boot_mempool(store: &Arc<StateStore>, data_dir: &Path) -> Mempool {
    #[cfg(feature = "rocksdb")]
    {
        open_rocks_mempool(store, data_dir)
    }
    #[cfg(not(feature = "rocksdb"))]
    {
        let _ = store;
        open_portable_mempool(data_dir)
    }
}

fn open_portable_mempool(data_dir: &Path) -> Mempool {
    let mempool_path = mempool_persist_path(data_dir);
    Mempool::open_persistent(&mempool_path, 10_000).unwrap_or_else(|err| {
        warn!(
            error = %err,
            path = %mempool_path.display(),
            "portable mempool snapshot rejected; starting empty"
        );
        let mut pool = Mempool::new(10_000);
        pool.set_persist_path(&mempool_path);
        pool
    })
}

#[cfg(feature = "rocksdb")]
fn open_rocks_mempool(store: &Arc<StateStore>, data_dir: &Path) -> Mempool {
    let kv: Arc<dyn MempoolKv> = Arc::new(StoreMempoolKv(Arc::clone(store)));
    match Mempool::open_persistent_kv(kv.clone(), 10_000) {
        Ok(pool) if !pool.is_empty() => pool,
        Ok(empty) => migrate_portable_into_kv(empty, kv, data_dir),
        Err(err) => {
            warn!(error = %err, "mempool CF restore failed; starting empty");
            let mut pool = Mempool::new(10_000);
            pool.set_persist_kv(kv);
            pool
        }
    }
}

#[cfg(feature = "rocksdb")]
fn migrate_portable_into_kv(empty_cf: Mempool, kv: Arc<dyn MempoolKv>, data_dir: &Path) -> Mempool {
    let path = mempool_persist_path(data_dir);
    if !path.exists() && !path.with_extension("wal").exists() {
        return empty_cf;
    }
    match Mempool::open_persistent(&path, 10_000) {
        Ok(mut migrated) => {
            migrated.set_persist_kv(kv);
            migrated.flush_persist();
            migrated
        }
        Err(err) => {
            warn!(
                error = %err,
                path = %path.display(),
                "portable mempool migration skipped; using empty CF"
            );
            empty_cf
        }
    }
}
