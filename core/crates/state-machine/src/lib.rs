//! Persistent state application for consensus-ordered blocks.
//!
//! Five column families (hot / warm / archival / meta / utxo) keep tip validation
//! off cold compaction paths while fixing genesis supply caps in `meta`.

mod apply;
mod columns;
mod error;
mod genesis;
mod store;
mod utxo;
mod zones;

pub use apply::{apply_block, balance_of, revert_journal, validate_mempool_tx, UtxoJournal};
pub use columns::{meta_keys, ColumnFamily};
pub use error::StateError;
pub use genesis::{GenesisBuilder, SupplyCaps};
pub use store::StateStore;
pub use utxo::{outpoint_key, outpoint_key_parts};
pub use zones::StateZone;
