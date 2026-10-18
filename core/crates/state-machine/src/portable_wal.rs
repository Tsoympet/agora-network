//! Append-only WAL for portable `--no-default-features` datadirs.
//!
//! Frames are `u32le` length + Borsh payload + FNV-1a trailer. A torn tail is
//! skipped on replay. Periodic tmp+rename checkpoints still compact into
//! `state.bin`. This is not RocksDB and not a public-testnet durability claim.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use borsh::{BorshDeserialize, BorshSerialize};

use crate::StateError;

pub const WAL_MAGIC: &[u8; 8] = b"AGORAW01";
pub const DEFAULT_CHECKPOINT_EVERY: u32 = 64;

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub enum PortableWalOp {
    Put {
        cf: u8,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        cf: u8,
        key: Vec<u8>,
    },
    Batch {
        ops: Vec<PortableWalOp>,
    },
}

pub struct PortableWalWriter {
    file: File,
    pub ops_since_checkpoint: u32,
}

impl PortableWalWriter {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StateError::Storage(e.to_string()))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        let len = file
            .metadata()
            .map_err(|e| StateError::Storage(e.to_string()))?
            .len();
        if len == 0 {
            file.write_all(WAL_MAGIC)
                .map_err(|e| StateError::Storage(e.to_string()))?;
            let _ = file.sync_all();
        } else {
            file.seek(SeekFrom::End(0))
                .map_err(|e| StateError::Storage(e.to_string()))?;
        }
        Ok(Self {
            file,
            ops_since_checkpoint: 0,
        })
    }

    pub fn append(&mut self, op: &PortableWalOp) -> Result<(), StateError> {
        let payload = borsh::to_vec(op).map_err(|e| StateError::Storage(e.to_string()))?;
        let len = u32::try_from(payload.len())
            .map_err(|_| StateError::Storage("portable WAL record too large".into()))?;
        self.file
            .write_all(&len.to_le_bytes())
            .map_err(|e| StateError::Storage(e.to_string()))?;
        self.file
            .write_all(&payload)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        self.file
            .write_all(&fnv1a32(&payload).to_le_bytes())
            .map_err(|e| StateError::Storage(e.to_string()))?;
        self.file
            .flush()
            .map_err(|e| StateError::Storage(e.to_string()))?;
        let _ = self.file.sync_data();
        self.ops_since_checkpoint = self.ops_since_checkpoint.saturating_add(1);
        Ok(())
    }

    pub fn truncate(&mut self) -> Result<(), StateError> {
        self.file
            .set_len(0)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| StateError::Storage(e.to_string()))?;
        self.file
            .write_all(WAL_MAGIC)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        let _ = self.file.sync_all();
        self.ops_since_checkpoint = 0;
        Ok(())
    }
}

pub fn wal_path_for_snapshot(snapshot: &Path) -> PathBuf {
    snapshot.with_extension("wal")
}

/// Replay complete frames. A truncated or checksum-failed tail is ignored.
pub fn replay_wal(path: &Path) -> Result<Vec<PortableWalOp>, StateError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut file = File::open(path).map_err(|e| StateError::Storage(e.to_string()))?;
    let mut magic = [0u8; 8];
    match file.read_exact(&mut magic) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(Vec::new()),
        Err(err) => return Err(StateError::Storage(err.to_string())),
    }
    if &magic != WAL_MAGIC {
        return Err(StateError::Storage(
            "portable WAL is missing AGORAW01 magic".into(),
        ));
    }
    let mut ops = Vec::new();
    loop {
        let mut len_buf = [0u8; 4];
        match file.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(StateError::Storage(err.to_string())),
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        if file.read_exact(&mut payload).is_err() {
            break;
        }
        let mut crc_buf = [0u8; 4];
        if file.read_exact(&mut crc_buf).is_err() {
            break;
        }
        if u32::from_le_bytes(crc_buf) != fnv1a32(&payload) {
            break;
        }
        match PortableWalOp::try_from_slice(&payload) {
            Ok(op) => ops.push(op),
            Err(_) => break,
        }
    }
    Ok(ops)
}

pub fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}
