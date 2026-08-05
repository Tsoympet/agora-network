//! Persistent state application for consensus-ordered blocks.
//!
//! Five column families (hot / warm / archival / meta / utxo) keep tip validation
//! off cold compaction paths while fixing genesis supply caps in `meta`.
//!
//! Transaction acceptance bitmaps and UTXO journals are committed atomically via
//! [`journal::commit_acceptance`]. Datadirs are bound to a
//! [`agora_types::NetworkFingerprint`].

mod columns;
mod error;
mod genesis;
mod journal;
mod store;
mod zones;

pub use columns::{acceptance_keys, meta_keys, utxo_key, utxo_key_outpoint, ColumnFamily};
pub use error::StateError;
pub use genesis::{GenesisBuilder, SupplyCaps};
pub use journal::{
    acceptance_status, acceptance_store_ops, assert_datadir_fingerprint, commit_acceptance,
    is_tx_accepted_in_block, load_acceptance_bitmap, load_accepted_fees, load_network_fingerprint,
    tx_confirmation, write_network_fingerprint, AcceptedTxRecord, StoreUtxoView,
};
pub use store::{StateStore, StoreOp};
pub use zones::StateZone;
