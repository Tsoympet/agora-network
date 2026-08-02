use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::columns::ColumnFamily;
use crate::{StateError, StateZone};

/// State backend used by the node.
///
/// Default build keeps an in-memory map so the workspace compiles without a C++
/// toolchain. Enable `--features rocksdb` for durable column-family storage.
pub struct StateStore {
    inner: Inner,
}

enum Inner {
    Memory(Arc<Mutex<HashMap<(u8, Vec<u8>), Vec<u8>>>>),
    #[cfg(feature = "rocksdb")]
    Rocks(Arc<rocksdb::DB>),
}

impl StateStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateError> {
        #[cfg(feature = "rocksdb")]
        {
            return Self::open_rocks(path);
        }
        #[cfg(not(feature = "rocksdb"))]
        {
            let _ = path;
            Ok(Self {
                inner: Inner::Memory(Arc::new(Mutex::new(HashMap::new()))),
            })
        }
    }

    #[cfg(feature = "rocksdb")]
    fn open_rocks(path: impl AsRef<Path>) -> Result<Self, StateError> {
        use rocksdb::{ColumnFamilyDescriptor, Options, DB};

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cfs = ColumnFamily::ALL
            .map(|cf| ColumnFamilyDescriptor::new(cf.name(), Options::default()));

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

    pub fn put(&self, zone: StateZone, key: &[u8], value: &[u8]) -> Result<(), StateError> {
        self.put_cf(zone.column_family(), key, value)
    }

    pub fn get(&self, zone: StateZone, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        self.get_cf(zone.column_family(), key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::meta_keys;

    #[test]
    fn five_column_families_roundtrip() {
        let store = StateStore::open("/tmp/agora-state-test").expect("open");
        for cf in ColumnFamily::ALL {
            let key = format!("k-{}", cf.name());
            store.put_cf(cf, key.as_bytes(), b"v").expect("put");
            assert_eq!(
                store.get_cf(cf, key.as_bytes()).expect("get"),
                Some(b"v".to_vec())
            );
        }
        store
            .put_cf(ColumnFamily::Meta, meta_keys::MAX_SUPPLY, &1_000u64.to_le_bytes())
            .unwrap();
        assert!(store
            .get_cf(ColumnFamily::Meta, meta_keys::MAX_SUPPLY)
            .unwrap()
            .is_some());
    }
}
