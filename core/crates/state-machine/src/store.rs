use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

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

        let cfs = [StateZone::Hot, StateZone::Warm, StateZone::Archival]
            .map(|z| ColumnFamilyDescriptor::new(z.column_family(), Options::default()));

        let db = DB::open_cf_descriptors(&opts, path, cfs)
            .map_err(|e| StateError::Storage(e.to_string()))?;

        Ok(Self {
            inner: Inner::Rocks(Arc::new(db)),
        })
    }

    pub fn put(&self, zone: StateZone, key: &[u8], value: &[u8]) -> Result<(), StateError> {
        match &self.inner {
            Inner::Memory(map) => {
                let mut guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                guard.insert((zone as u8, key.to_vec()), value.to_vec());
                Ok(())
            }
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => {
                let cf = db
                    .cf_handle(zone.column_family())
                    .ok_or(StateError::UnknownZone)?;
                db.put_cf(cf, key, value)
                    .map_err(|e| StateError::Storage(e.to_string()))
            }
        }
    }

    pub fn get(&self, zone: StateZone, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        match &self.inner {
            Inner::Memory(map) => {
                let guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                Ok(guard.get(&(zone as u8, key.to_vec())).cloned())
            }
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => {
                let cf = db
                    .cf_handle(zone.column_family())
                    .ok_or(StateError::UnknownZone)?;
                db.get_cf(cf, key)
                    .map_err(|e| StateError::Storage(e.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_roundtrip() {
        let store = StateStore::open("/tmp/agora-state-test").expect("open");
        store.put(StateZone::Hot, b"k", b"v").expect("put");
        assert_eq!(store.get(StateZone::Hot, b"k").expect("get"), Some(b"v".to_vec()));
    }
}
