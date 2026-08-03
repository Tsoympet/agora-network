//! Persistent state application for consensus-ordered blocks.
//!
//! Five column families (hot / warm / archival / meta / utxo) keep tip validation
//! off cold compaction paths while fixing genesis supply caps in `meta`.

mod apply;
mod columns;
mod error;
mod genesis;
mod marks;
mod network;
mod store;
mod tx_index;
mod utxo;
mod utxo_diff;
mod zones;

pub use apply::{
    apply_block, balance_of, revert_journal, sum_transfer_fees, transfer_fee, validate_mempool_tx,
    UtxoJournal,
};
pub use columns::{meta_keys, ColumnFamily};
pub use error::StateError;
pub use genesis::{GenesisBuilder, SupplyCaps};
pub use marks::{default_token_marks, TokenMark};
pub use network::{
    daa_config_mainnet, daa_config_testnet, ChainParams, GenesisArtifact, GenesisConsensusPolicy,
    GenesisWalletPolicy, NetworkId, DEFAULT_GHOSTDAG_K, TESTNET_GENESIS_BITS,
    TESTNET_GENESIS_HASH_HEX, TESTNET_GENESIS_TIMESTAMP_MS, TESTNET_PREMINE_ADDRESS_HEX,
};
pub use store::StateStore;
pub use tx_index::{
    decode_tx_location, encode_tx_location, index_block_transactions, lookup_tx_location,
    tx_index_key,
};
pub use utxo::{outpoint_key, outpoint_key_parts};
pub use utxo_diff::{delete_utxo_journal, load_utxo_journal, store_utxo_journal, utxo_diff_key};
pub use zones::StateZone;
