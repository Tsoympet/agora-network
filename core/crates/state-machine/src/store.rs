use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::{StateError, StateZone};

/// State backend used by the node.
///
/// - [`StateStore::open_in_memory`] — ephemeral map for isolated tests
/// - [`StateStore::open`] — RocksDB when built with `--features rocksdb` (node default);
///   otherwise a portable snapshot under `$path/portable/state.bin`
pub struct StateStore {
    inner: Inner,
    /// When set, Memory mutations flush an atomic snapshot to this file.
    persist: Option<PathBuf>,
}

/// In-memory backend map: `(cf, key) -> value`.
type MemStore = Arc<Mutex<BTreeMap<(u8, Vec<u8>), Vec<u8>>>>;

/// Overlay delta: `None` means deleted relative to the base store.
type CowDelta = Arc<Mutex<BTreeMap<(u8, Vec<u8>), Option<Vec<u8>>>>>;

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
#[derive(Clone)]
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
#[derive(Default, Clone)]
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
            inner: Inner::Memory(Arc::new(Mutex::new(BTreeMap::new()))),
            persist: None,
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
                delta: Arc::new(Mutex::new(BTreeMap::new())),
            },
            persist: None,
        }
    }

    /// COW overlay over this store's backend (cheap Arc clone of the inner handle).
    ///
    /// Used inside multi-lane block apply so sequential account/stake ops see prior
    /// Accepted mutations without committing the consensus [`WriteBatch`] early.
    pub fn cow_overlay(&self) -> Self {
        Self::open_cow_overlay(Arc::new(Self {
            inner: self.clone_inner(),
            persist: None,
        }))
    }

    fn clone_inner(&self) -> Inner {
        match &self.inner {
            Inner::Memory(map) => Inner::Memory(Arc::clone(map)),
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => Inner::Rocks(Arc::clone(db)),
            Inner::Cow { base, delta } => Inner::Cow {
                base: Arc::clone(base),
                delta: Arc::clone(delta),
            },
        }
    }

    /// Open durable storage at `path`.
    ///
    /// RocksDB when the `rocksdb` feature is on. Otherwise a portable
    /// snapshot file so `--no-default-features` nodes survive restart
    /// (orphans, headers, UTXO, Block 0) without libstdc++.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateError> {
        #[cfg(feature = "rocksdb")]
        {
            Self::open_rocks(path)
        }
        #[cfg(not(feature = "rocksdb"))]
        {
            Self::open_portable(path)
        }
    }

    /// File-backed Memory map. Always available so RocksDB builds still
    /// test the portable crash path used by Experimental mesh hosts.
    pub fn open_portable(path: impl AsRef<Path>) -> Result<Self, StateError> {
        let dir = path.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| StateError::Storage(e.to_string()))?;
        let file = portable_state_file(dir);
        let map = load_portable_snapshot(&file)?;
        Ok(Self {
            inner: Inner::Memory(Arc::new(Mutex::new(map))),
            persist: Some(file),
        })
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
            persist: None,
        })
    }

    pub fn put_cf(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<(), StateError> {
        match &self.inner {
            Inner::Memory(map) => {
                let mut guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                guard.insert((cf as u8, key.to_vec()), value.to_vec());
                drop(guard);
                self.flush_portable()
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
                drop(guard);
                self.flush_portable()
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
                let mut map: BTreeMap<Vec<u8>, Vec<u8>> =
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
                Ok(map.into_iter().collect())
            }
        }
    }

    /// Return at most `limit` prefix-matching entries strictly after `start_after`.
    ///
    /// RocksDB seeks directly to the prefix/cursor boundary. In-memory stores use
    /// ordered ranges, and COW overlays only request enough base rows to account
    /// for overlay deletions. This keeps owner pagination bounded without a
    /// database-wide materialization.
    pub fn scan_prefix_after_limit(
        &self,
        cf: ColumnFamily,
        prefix: &[u8],
        start_after: Option<&[u8]>,
        limit: usize,
    ) -> Result<Vec<KvPair>, StateError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let starts_after = |key: &[u8]| start_after.is_none_or(|cursor| key > cursor);
        match &self.inner {
            Inner::Memory(map) => {
                let guard = map
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                Ok(guard
                    .iter()
                    .filter(|((stored_cf, key), _)| {
                        *stored_cf == cf as u8
                            && key.starts_with(prefix)
                            && starts_after(key.as_slice())
                    })
                    .take(limit)
                    .map(|((_, key), value)| (key.clone(), value.clone()))
                    .collect())
            }
            #[cfg(feature = "rocksdb")]
            Inner::Rocks(db) => {
                use rocksdb::{Direction, IteratorMode};

                let handle = db.cf_handle(cf.name()).ok_or(StateError::UnknownZone)?;
                let seek = start_after.unwrap_or(prefix);
                let iter = db.iterator_cf(handle, IteratorMode::From(seek, Direction::Forward));
                let mut out = Vec::with_capacity(limit);
                for item in iter {
                    let (key, value) = item.map_err(|e| StateError::Storage(e.to_string()))?;
                    if !key.starts_with(prefix) {
                        break;
                    }
                    if !starts_after(key.as_ref()) {
                        continue;
                    }
                    out.push((key.to_vec(), value.to_vec()));
                    if out.len() == limit {
                        break;
                    }
                }
                Ok(out)
            }
            Inner::Cow { base, delta } => {
                let guard = delta
                    .lock()
                    .map_err(|_| StateError::Storage("lock poisoned".into()))?;
                let relevant: Vec<(Vec<u8>, Option<Vec<u8>>)> = guard
                    .iter()
                    .filter(|((stored_cf, key), _)| {
                        *stored_cf == cf as u8
                            && key.starts_with(prefix)
                            && starts_after(key.as_slice())
                    })
                    .map(|((_, key), value)| (key.clone(), value.clone()))
                    .collect();
                drop(guard);

                let deleted = relevant.iter().filter(|(_, value)| value.is_none()).count();
                let base_limit = limit.saturating_add(deleted);
                let mut merged: BTreeMap<Vec<u8>, Vec<u8>> = base
                    .scan_prefix_after_limit(cf, prefix, start_after, base_limit)?
                    .into_iter()
                    .collect();
                for (key, value) in relevant {
                    match value {
                        Some(bytes) => {
                            merged.insert(key, bytes);
                        }
                        None => {
                            merged.remove(&key);
                        }
                    }
                }
                Ok(merged.into_iter().take(limit).collect())
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
                drop(guard);
                self.flush_portable()
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
                let mut map: BTreeMap<Vec<u8>, Vec<u8>> =
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
                for (key, value) in map {
                    f(key.as_slice(), value.as_slice())?;
                }
                Ok(())
            }
        }
    }

    fn flush_portable(&self) -> Result<(), StateError> {
        let Some(path) = self.persist.as_ref() else {
            return Ok(());
        };
        let Inner::Memory(map) = &self.inner else {
            return Ok(());
        };
        let guard = map
            .lock()
            .map_err(|_| StateError::Storage("lock poisoned".into()))?;
        write_portable_snapshot(path, &guard)
    }
}

const PORTABLE_MAGIC: &[u8; 8] = b"AGORAP01";

fn portable_state_file(dir: &Path) -> PathBuf {
    dir.join("portable").join("state.bin")
}

fn write_portable_snapshot(path: &Path, map: &MemStoreMap) -> Result<(), StateError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StateError::Storage(e.to_string()))?;
    }
    let rows: Vec<PortableRow> = map
        .iter()
        .map(|((cf, key), value)| PortableRow {
            cf: *cf,
            key: key.clone(),
            value: value.clone(),
        })
        .collect();
    let payload = borsh::to_vec(&rows).map_err(|e| StateError::Storage(e.to_string()))?;
    let mut bytes = Vec::with_capacity(PORTABLE_MAGIC.len() + payload.len());
    bytes.extend_from_slice(PORTABLE_MAGIC);
    bytes.extend_from_slice(&payload);
    let tmp = path.with_extension("bin.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    if let Ok(file) = std::fs::File::open(&tmp) {
        let _ = file.sync_all();
    }
    std::fs::rename(&tmp, path).map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(())
}

fn load_portable_snapshot(path: &Path) -> Result<MemStoreMap, StateError> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = std::fs::read(path).map_err(|e| StateError::Storage(e.to_string()))?;
    if bytes.len() < PORTABLE_MAGIC.len() || !bytes.starts_with(PORTABLE_MAGIC) {
        return Err(StateError::Storage(
            "portable state snapshot is missing AGORAP01 magic".into(),
        ));
    }
    let rows = Vec::<PortableRow>::try_from_slice(&bytes[PORTABLE_MAGIC.len()..])
        .map_err(|e| StateError::Storage(e.to_string()))?;
    let mut map = BTreeMap::new();
    for row in rows {
        map.insert((row.cf, row.key), row.value);
    }
    Ok(map)
}

#[derive(BorshSerialize, BorshDeserialize)]
struct PortableRow {
    cf: u8,
    key: Vec<u8>,
    value: Vec<u8>,
}

type MemStoreMap = BTreeMap<(u8, Vec<u8>), Vec<u8>>;

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

    #[test]
    fn portable_store_persists_put_delete_and_batch_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = StateStore::open_portable(dir.path()).unwrap();
            store
                .put_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH, &[7u8; 32])
                .unwrap();
            store.put_cf(ColumnFamily::Utxo, b"spent", b"old").unwrap();
            store.delete_cf(ColumnFamily::Utxo, b"spent").unwrap();
            let mut batch = WriteBatch::new();
            batch.put_cf(ColumnFamily::Warm, b"orphan/a", b"body");
            batch.put_cf(ColumnFamily::Hot, b"tip", b"1");
            store.write_batch(batch).unwrap();
        }
        let store = StateStore::open_portable(dir.path()).unwrap();
        assert_eq!(
            store
                .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
                .unwrap(),
            Some(vec![7u8; 32])
        );
        assert_eq!(store.get_cf(ColumnFamily::Utxo, b"spent").unwrap(), None);
        assert_eq!(
            store.get_cf(ColumnFamily::Warm, b"orphan/a").unwrap(),
            Some(b"body".to_vec())
        );
        assert_eq!(
            store.get_cf(ColumnFamily::Hot, b"tip").unwrap(),
            Some(b"1".to_vec())
        );
        assert!(dir.path().join("portable").join("state.bin").is_file());
    }

    #[test]
    fn portable_store_cow_writes_do_not_flush() {
        let dir = tempfile::tempdir().unwrap();
        let base = Arc::new(StateStore::open_portable(dir.path()).unwrap());
        base.put_cf(ColumnFamily::Utxo, b"a", b"1").unwrap();
        let overlay = StateStore::open_cow_overlay(base);
        overlay.put_cf(ColumnFamily::Utxo, b"a", b"2").unwrap();
        overlay.put_cf(ColumnFamily::Utxo, b"b", b"3").unwrap();
        drop(overlay);
        let reopened = StateStore::open_portable(dir.path()).unwrap();
        assert_eq!(
            reopened.get_cf(ColumnFamily::Utxo, b"a").unwrap(),
            Some(b"1".to_vec())
        );
        assert_eq!(reopened.get_cf(ColumnFamily::Utxo, b"b").unwrap(), None);
    }

    #[test]
    fn portable_store_rejects_truncated_magic() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("portable").join("state.bin");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"NOPE").unwrap();
        match StateStore::open_portable(dir.path()) {
            Err(StateError::Storage(msg)) => assert!(msg.contains("AGORAP01")),
            Err(other) => panic!("unexpected {other}"),
            Ok(_) => panic!("expected truncated snapshot to fail"),
        }
    }
}
