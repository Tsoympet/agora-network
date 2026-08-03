use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::columns::ColumnFamily;
use crate::{StateError, StateZone};

/// State backend used by the node.
///
/// - [`StateStore::open_in_memory`] — portable tests / CI without a C++ toolchain
/// - [`StateStore::open`] — RocksDB when built with `--features rocksdb` (node default);
///   otherwise falls back to an in-memory map (path ignored)
pub struct StateStore {
    inner: Inner,
}

enum Inner {
    Memory(Arc<Mutex<HashMap<(u8, Vec<u8>), Vec<u8>>>>),
    #[cfg(feature = "rocksdb")]
    Rocks(Arc<rocksdb::DB>),
}

impl StateStore {
    /// Ephemeral map — isolated unit tests and feature-less CI builds.
    pub fn open_in_memory() -> Self {
        Self {
            inner: Inner::Memory(Arc::new(Mutex::new(HashMap::new()))),
        }
    }

    /// Open durable storage at `path` when the `rocksdb` feature is enabled.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateError> {
        #[cfg(feature = "rocksdb")]
        {
            return Self::open_rocks(path);
        }
        #[cfg(not(feature = "rocksdb"))]
        {
            let _ = path;
            Ok(Self::open_in_memory())
        }
    }

    #[cfg(feature = "rocksdb")]
    fn open_rocks(path: impl AsRef<Path>) -> Result<Self, StateError> {
        use rocksdb::{ColumnFamilyDescriptor, Options, DB};

        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|e| StateError::Storage(e.to_string()))?;
        }

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cfs =
            ColumnFamily::ALL.map(|cf| ColumnFamilyDescriptor::new(cf.name(), Options::default()));

        let db = DB::open_cf_descriptors(&opts, path, cfs)
            .map_err(|e| StateError::Storage(e.to_string()))?;

        Ok(Self {
            inner: Inner::Rocks(Arc::new(db)),
        })
    }

    pub fn put_cf(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<(), StateError> {
        match &self.inner {
            Inner::Memory(map) => {
                let mut guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                guard.insert((cf as u8, key.to_vec()), value.to_vec());
                Ok(())
            }
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => {
                let handle = db.cf_handle(cf.name()).ok_or(StateError::UnknownZone)?;
                db.put_cf(handle, key, value)
                    .map_err(|e| StateError::Storage(e.to_string()))
            }
        }
    }

    pub fn get_cf(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        match &self.inner {
            Inner::Memory(map) => {
                let guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                Ok(guard.get(&(cf as u8, key.to_vec())).cloned())
            }
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => {
                let handle = db.cf_handle(cf.name()).ok_or(StateError::UnknownZone)?;
                db.get_cf(handle, key)
                    .map_err(|e| StateError::Storage(e.to_string()))
            }
        }
    }

    pub fn delete_cf(&self, cf: ColumnFamily, key: &[u8]) -> Result<(), StateError> {
        match &self.inner {
            Inner::Memory(map) => {
                let mut guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                guard.remove(&(cf as u8, key.to_vec()));
                Ok(())
            }
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => {
                let handle = db.cf_handle(cf.name()).ok_or(StateError::UnknownZone)?;
                db.delete_cf(handle, key)
                    .map_err(|e| StateError::Storage(e.to_string()))
            }
        }
    }

    pub fn put(&self, zone: StateZone, key: &[u8], value: &[u8]) -> Result<(), StateError> {
        self.put_cf(zone.column_family(), key, value)
    }

    pub fn get(&self, zone: StateZone, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        self.get_cf(zone.column_family(), key)
    }

    /// Iterate all key/value pairs in a column family.
    ///
    /// Used by RPC balance scans and other index walks. Callback errors abort iteration.
    pub fn for_each_cf<F>(&self, cf: ColumnFamily, mut f: F) -> Result<(), StateError>
    where
        F: FnMut(&[u8], &[u8]) -> Result<(), StateError>,
    {
        match &self.inner {
            Inner::Memory(map) => {
                let guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                for ((stored_cf, key), value) in guard.iter() {
                    if *stored_cf == cf as u8 {
                        f(key.as_slice(), value.as_slice())?;
                    }
                }
                Ok(())
            }
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => {
                let handle = db.cf_handle(cf.name()).ok_or(StateError::UnknownZone)?;
                let iter = db.iterator_cf(handle, rocksdb::IteratorMode::Start);
                for item in iter {
                    let (key, value) = item.map_err(|e| StateError::Storage(e.to_string()))?;
                    f(key.as_ref(), value.as_ref())?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::meta_keys;

    #[test]
    fn five_column_families_roundtrip() {
        let store = StateStore::open_in_memory();
        for cf in ColumnFamily::ALL {
            let key = format!("k-{}", cf.name());
            store.put_cf(cf, key.as_bytes(), b"v").expect("put");
            assert_eq!(
                store.get_cf(cf, key.as_bytes()).expect("get"),
                Some(b"v".to_vec())
            );
        }
        store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::MAX_SUPPLY,
                &1_000u64.to_le_bytes(),
            )
            .unwrap();
        assert!(store
            .get_cf(ColumnFamily::Meta, meta_keys::MAX_SUPPLY)
            .unwrap()
            .is_some());
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_persists_across_reopen() {
        let dir = std::env::temp_dir().join(format!(
            "agora-rocksdb-reopen-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let store = StateStore::open(&dir).unwrap();
            store
                .put_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH, &[7u8; 32])
                .unwrap();
        }
        {
            let store = StateStore::open(&dir).unwrap();
            assert_eq!(
                store
                    .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
                    .unwrap(),
                Some(vec![7u8; 32])
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
