//! Persistent state application for consensus-ordered blocks.
//!
//! Five column families (hot / warm / archival / meta / utxo) keep tip validation
//! off cold compaction paths while fixing genesis supply caps in `meta`.

mod apply;
mod columns;
mod error;
mod genesis;
mod ghostdag_store;
mod headers;
mod marks;
mod monetary;
mod network;
mod orphans;
mod store;
mod trident_genesis;
mod tx_index;
mod utxo;
mod utxo_diff;
mod zones;

pub use apply::{
    apply_block, apply_block_batched, apply_block_batched_virtual, apply_block_batched_with_auth,
    apply_block_with_auth, balance_of, revert_journal, revert_journal_batched, sum_transfer_fees,
    transfer_fee, validate_mempool_tx, validate_mempool_tx_with_auth, ApplyMode, TxAuthContext,
    UtxoJournal,
};
pub use columns::{meta_keys, ColumnFamily, SCHEMA_VERSION};
pub use error::StateError;
pub use genesis::{GenesisBuilder, SupplyCaps};
pub use ghostdag_store::{
    ghostdag_key, load_ghostdag_record, store_ghostdag_record, GhostdagRecord,
};
pub use headers::{header_key, load_header, store_header, store_header_into};
pub use marks::{default_token_marks, TokenMark};
pub use monetary::{
    issued_within_cap, AssetMonetaryPolicy, EmissionKind, TridentMonetaryPolicy,
    DRC_MAX_SUPPLY_BASE, OVL_MAX_SUPPLY_BASE, TLT_MAX_SUPPLY_BASE,
};
pub use network::{
    daa_config_mainnet, daa_config_testnet, ChainParams, GenesisArtifact, GenesisConsensusPolicy,
    GenesisWalletPolicy, NetworkId, DEFAULT_GHOSTDAG_K, TESTNET_GENESIS_BITS,
    TESTNET_GENESIS_HASH_HEX, TESTNET_GENESIS_TIMESTAMP_MS, TESTNET_PREMINE_ADDRESS_HEX,
};
pub use orphans::{delete_orphan, list_orphans, load_orphan, orphan_key, store_orphan};
pub use store::{StateStore, WriteBatch};
pub use trident_genesis::{
    TridentFinalityPolicy, TridentGenesisArtifact, TridentValidatorGenesis,
    TRIDENT_CONSENSUS_POLICY_VERSION, TRIDENT_GENESIS_SCHEMA, TRIDENT_NET_FP_DOMAIN,
    TRIDENT_PROTOCOL_VERSION, TRIDENT_STATE_TRANSITION_VERSION, TRIDENT_TX_SIGNING_VERSION,
};
pub use tx_index::{
    decode_tx_location, encode_tx_location, index_block_transactions,
    index_block_transactions_into, list_tx_inclusions, lookup_tx_location, set_primary_tx_location,
    tx_inclusion_key, tx_index_key,
};
pub use utxo::{outpoint_key, outpoint_key_parts};
pub use utxo_diff::{delete_utxo_journal, load_utxo_journal, store_utxo_journal, utxo_diff_key};
pub use zones::StateZone;
