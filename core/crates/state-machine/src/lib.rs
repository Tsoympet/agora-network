//! Persistent state application for consensus-ordered blocks.
//!
//! Five column families (hot / warm / archival / meta / utxo) keep tip validation
//! off cold compaction paths while fixing genesis supply caps in `meta`.

mod columns;
mod error;
mod genesis;
mod store;
mod zones;

pub use columns::{meta_keys, ColumnFamily};
pub use error::StateError;
pub use genesis::{GenesisBuilder, SupplyCaps};
pub use store::StateStore;
pub use zones::StateZone;
