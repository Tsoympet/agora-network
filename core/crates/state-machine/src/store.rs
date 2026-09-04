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

/// In-memory backend map: `(cf, key) -> value`.
type MemStore = Arc<Mutex<HashMap<(u8, Vec<u8>), Vec<u8>>>>;

/// Overlay delta: `None` means deleted relative to the base store.
type CowDelta = Arc<Mutex<HashMap<(u8, Vec<u8>), Option<Vec<u8>>>>>;

/// A key/value pair returned by prefix scans.
pub type KvPair = (Vec<u8>, Vec<u8>);

enum Inner {
    Memory(MemStore),
    #[cfg(feature = "rocksdb")]
    Rocks(Arc<rocksdb::DB>),
    /// Copy-on-write view over another store: reads fall through; writes stay in `delta`.
    Cow {
        base: Arc<StateStore>,
        delta: CowDelta,
    },
}

/// A single mutation in a [`WriteBatch`].
enum WriteOp {
    Put(ColumnFamily, Vec<u8>, Vec<u8>),
    Delete(ColumnFamily, Vec<u8>),
}

/// An ordered set of column-family mutations committed atomically by
/// [`StateStore::write_batch`].
///
/// On RocksDB this maps to a native `WriteBatch` (all-or-nothing on `db.write`). On the
/// in-memory backend all ops are applied under one lock. Use this to commit consensus
/// state transitions (UTXO changes + journal + issued supply, etc.) as a single unit so a
/// crash cannot leave the store half-updated.
#[derive(Default)]
pub struct WriteBatch {
    ops: Vec<WriteOp>,
}

impl WriteBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put_cf(&mut self, cf: ColumnFamily, key: &[u8], value: &[u8]) {
        self.ops
            .push(WriteOp::Put(cf, key.to_vec(), value.to_vec()));
    }

    pub fn delete_cf(&mut self, cf: ColumnFamily, key: &[u8]) {
        self.ops.push(WriteOp::Delete(cf, key.to_vec()));
    }

    /// Append all ops from `other` after this batch's ops (preserves order).
    pub fn append(&mut self, mut other: WriteBatch) {
        self.ops.append(&mut other.ops);
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

impl StateStore {
    /// Ephemeral map — isolated unit tests and feature-less CI builds.
    pub fn open_in_memory() -> Self {
        Self {
            inner: Inner::Memory(Arc::new(Mutex::new(HashMap::new()))),
        }
    }

    /// Copy-on-write overlay: shares the base store for reads; mutations stay local.
    ///
    /// Used by blue_order UTXO pre-validation so tip extensions do not clone the
    /// entire UTXO set / journal corpus into a new map.
    pub fn open_cow_overlay(base: Arc<StateStore>) -> Self {
        Self {
            inner: Inner::Cow {
                base,
                delta: Arc::new(Mutex::new(HashMap::new())),
            },
        }
    }

    /// Open durable storage at `path` when the `rocksdb` feature is enabled.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateError> {
        #[cfg(feature = "rocksdb")]
        {
            Self::open_rocks(path)
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
            Inner::Cow { delta, .. } => {
                let mut guard = delta
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                guard.insert((cf as u8, key.to_vec()), Some(value.to_vec()));
                Ok(())
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
            Inner::Cow { base, delta } => {
                let guard = delta
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                if let Some(entry) = guard.get(&(cf as u8, key.to_vec())) {
                    return Ok(entry.clone());
                }
                drop(guard);
                base.get_cf(cf, key)
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
            Inner::Cow { delta, .. } => {
                let mut guard = delta
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                guard.insert((cf as u8, key.to_vec()), None);
                Ok(())
            }
        }
    }

    /// Scan keys in `cf` that start with `prefix` (inclusive).
    pub fn scan_prefix(&self, cf: ColumnFamily, prefix: &[u8]) -> Result<Vec<KvPair>, StateError> {
        match &self.inner {
            Inner::Memory(map) => {
                let guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                let mut out: Vec<(Vec<u8>, Vec<u8>)> = guard
                    .iter()
                    .filter(|((c, k), _)| *c == cf as u8 && k.starts_with(prefix))
                    .map(|((_, k), v)| (k.clone(), v.clone()))
                    .collect();
                out.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(out)
            }
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => {
                use rocksdb::{Direction, IteratorMode};

                let handle = db.cf_handle(cf.name()).ok_or(StateError::UnknownZone)?;
                let mut out = Vec::new();
                let iter = db.iterator_cf(handle, IteratorMode::From(prefix, Direction::Forward));
                for item in iter {
                    let (k, v) = item.map_err(|e| StateError::Storage(e.to_string()))?;
                    if !k.starts_with(prefix) {
                        break;
                    }
                    out.push((k.to_vec(), v.to_vec()));
                }
                Ok(out)
            }
            Inner::Cow { base, delta } => {
                let mut map: HashMap<Vec<u8>, Vec<u8>> =
                    base.scan_prefix(cf, prefix)?.into_iter().collect();
                let guard = delta
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                for ((c, k), v) in guard.iter() {
                    if *c == cf as u8 && k.starts_with(prefix) {
                        match v {
                            Some(bytes) => {
                                map.insert(k.clone(), bytes.clone());
                            }
                            None => {
                                map.remove(k);
                            }
                        }
                    }
                }
                let mut out: Vec<_> = map.into_iter().collect();
                out.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(out)
            }
        }
    }

    /// Commit a [`WriteBatch`] atomically (all ops apply, or none on error).
    pub fn write_batch(&self, batch: WriteBatch) -> Result<(), StateError> {
        match &self.inner {
            Inner::Memory(map) => {
                let mut guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                for op in batch.ops {
                    match op {
                        WriteOp::Put(cf, key, value) => {
                            guard.insert((cf as u8, key), value);
                        }
                        WriteOp::Delete(cf, key) => {
                            guard.remove(&(cf as u8, key));
                        }
                    }
                }
                Ok(())
            }
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => {
                let mut wb = rocksdb::WriteBatch::default();
                for op in batch.ops {
                    match op {
                        WriteOp::Put(cf, key, value) => {
                            let handle = db.cf_handle(cf.name()).ok_or(StateError::UnknownZone)?;
                            wb.put_cf(handle, key, value);
                        }
                        WriteOp::Delete(cf, key) => {
                            let handle = db.cf_handle(cf.name()).ok_or(StateError::UnknownZone)?;
                            wb.delete_cf(handle, key);
                        }
                    }
                }
                db.write(wb).map_err(|e| StateError::Storage(e.to_string()))
            }
            Inner::Cow { delta, .. } => {
                let mut guard = delta
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                for op in batch.ops {
                    match op {
                        WriteOp::Put(cf, key, value) => {
                            guard.insert((cf as u8, key), Some(value));
                        }
                        WriteOp::Delete(cf, key) => {
                            guard.insert((cf as u8, key), None);
                        }
                    }
                }
                Ok(())
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
            Inner::Cow { base, delta } => {
                // Use scan_prefix (not for_each_cf) to avoid monomorphization recursion
                // when overlays are stacked.
                let mut map: HashMap<Vec<u8>, Vec<u8>> =
                    base.scan_prefix(cf, &[])?.into_iter().collect();
                let guard = delta
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                for ((c, k), v) in guard.iter() {
                    if *c == cf as u8 {
                        match v {
                            Some(bytes) => {
                                map.insert(k.clone(), bytes.clone());
                            }
                            None => {
                                map.remove(k);
                            }
                        }
                    }
                }
                let mut keys: Vec<_> = map.keys().cloned().collect();
                keys.sort();
                for k in keys {
                    let v = &map[&k];
                    f(k.as_slice(), v.as_slice())?;
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
    use std::sync::Arc;

    #[test]
    fn cow_overlay_isolates_writes() {
        let base = Arc::new(StateStore::open_in_memory());
        base.put_cf(ColumnFamily::Utxo, b"a", b"1").unwrap();
        let overlay = StateStore::open_cow_overlay(base.clone());
        assert_eq!(
            overlay.get_cf(ColumnFamily::Utxo, b"a").unwrap(),
            Some(b"1".to_vec())
        );
        overlay.put_cf(ColumnFamily::Utxo, b"a", b"2").unwrap();
        overlay.put_cf(ColumnFamily::Utxo, b"b", b"3").unwrap();
        overlay.delete_cf(ColumnFamily::Utxo, b"a").unwrap();
        assert_eq!(overlay.get_cf(ColumnFamily::Utxo, b"a").unwrap(), None);
        assert_eq!(
            overlay.get_cf(ColumnFamily::Utxo, b"b").unwrap(),
            Some(b"3".to_vec())
        );
        // Base unchanged.
        assert_eq!(
            base.get_cf(ColumnFamily::Utxo, b"a").unwrap(),
            Some(b"1".to_vec())
        );
        assert_eq!(base.get_cf(ColumnFamily::Utxo, b"b").unwrap(), None);
    }

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
