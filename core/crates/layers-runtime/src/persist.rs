//! Durable L2/L3 checkpoints for `agora-layers`.
//!
//! State roots remain SHA-256 digests over the account+storage cache (not full
//! Ethereum MPT). Persistence survives process restarts via a JSON checkpoint.

use std::fs;
use std::path::{Path, PathBuf};

use agora_bridge_sdk::BridgeCheckpoint;
use agora_ovolos_rollup::AccountSnapDto;
use agora_types::{Address, Hash};
use serde::{Deserialize, Serialize};

use crate::LayersError;

const CHECKPOINT_FILE: &str = "layers-checkpoint.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayersCheckpoint {
    pub version: u32,
    pub head_state_root: String,
    pub next_sequence: u64,
    pub ovl_tip_hash: String,
    pub ovl_tip_height: u64,
    pub ovl_balances: Vec<(Address, u64)>,
    pub ovl_minted: u64,
    pub sequencer_bonds: Vec<(Address, u64)>,
    pub revm_snapshots: Vec<(String, Vec<AccountSnapDto>)>,
    pub bridge: BridgeCheckpoint,
    pub l2_mempool: Vec<Vec<u8>>,
}

impl LayersCheckpoint {
    pub fn path_in(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join(CHECKPOINT_FILE)
    }

    pub fn save(&self, dir: impl AsRef<Path>) -> Result<(), LayersError> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir).map_err(|e| LayersError::Rollup(format!("mkdir: {e}")))?;
        let path = Self::path_in(dir);
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| LayersError::Rollup(format!("checkpoint encode: {e}")))?;
        fs::write(&tmp, bytes)
            .map_err(|e| LayersError::Rollup(format!("checkpoint write: {e}")))?;
        fs::rename(&tmp, &path)
            .map_err(|e| LayersError::Rollup(format!("checkpoint rename: {e}")))?;
        Ok(())
    }

    pub fn load(dir: impl AsRef<Path>) -> Result<Option<Self>, LayersError> {
        let path = Self::path_in(dir);
        if !path.exists() {
            return Ok(None);
        }
        let bytes =
            fs::read(&path).map_err(|e| LayersError::Rollup(format!("checkpoint read: {e}")))?;
        let cp: Self = serde_json::from_slice(&bytes)
            .map_err(|e| LayersError::Rollup(format!("checkpoint decode: {e}")))?;
        Ok(Some(cp))
    }
}

pub fn parse_hash(hex: &str) -> Result<Hash, LayersError> {
    Hash::from_hex(hex).ok_or_else(|| LayersError::Rollup(format!("bad hash {hex}")))
}
