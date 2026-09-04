//! Runnable Agora layer stack: Ovolos (L2) + Bridge-in-a-Box (L3) + Intent-Engine (L4).
//!
//! This is an in-process operator runtime — not a claim of a public multi-chain
//! network. Each mark is native PoW money on its layer: TLT (L1 RandomX),
//! OVL (L2 sha256), DRC (L3 sha256). OVL/DRC are not L1 UTXO asset ids.

mod error;
mod migration;
mod persist;
mod runtime;

pub use error::LayersError;
pub use migration::{
    export_migration_snapshot, load_and_verify_migration_snapshot, verify_migration_snapshot,
    MigrationError, MigrationSnapshot,
};
pub use persist::LayersCheckpoint;
pub use runtime::{LayerInfo, LayersRuntime, LayersRuntimeConfig};
