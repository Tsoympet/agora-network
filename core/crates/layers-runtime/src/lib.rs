//! Historical Agora layer lab: Ovolos + Bridge-in-a-Box + Intent-Engine.
//!
//! This in-process runtime is retained for tests, migration evidence, and code
//! reuse. It is not a public multi-chain network or a canonical monetary state.
//! Under Trident, OVL and DRC are native L1 account assets and are never mined.

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
pub use runtime::{
    DrcPathPayment, LayerInfo, LayersRuntime, LayersRuntimeConfig, LockAndMintRequest,
};
