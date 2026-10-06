//! Historical Ovolos rollup lab with OVL-only EVM execution.
//!
//! This pre-Trident prototype is retained for tests, code reuse, and migration
//! evidence; it is not canonical money or a public L2. Its VM and `eth_*`
//! compatibility are strictly OVL-scoped and never accept DRC. Canonical smart
//! contract execution belongs only to the Trident OVL domain.

mod da;
mod error;
mod executor;
mod genesis;
mod ovl;
mod pow;
mod rollup;
mod sequencer;
mod types;

#[cfg(feature = "revm")]
mod revm_exec;
#[cfg(feature = "revm")]
mod signed_tx;

pub use da::{tx_merkle_root, BatchCommitment};
pub use error::RollupError;
pub use executor::{reexecute_batch, EvmExecutor, StubEvmExecutor};
pub use genesis::{
    OvlPremine, OvolosGenesis, DEFAULT_OVL_HALVING_INTERVAL, DEFAULT_OVL_INITIAL_REWARD,
    DEFAULT_OVL_POW_BITS,
};
pub use ovl::{OvlLedger, DEFAULT_GAS_PER_TX, OVL_MAX_SUPPLY_BASE};
pub use pow::{
    leading_zero_bits, mine_ovl_block, verify_pow, OvlBlock, OvlBlockHeader, OvlEmission,
    OVOLOS_POW_ALGORITHM,
};
pub use rollup::{OvolosRollup, RollupCheckpoint, RollupConfig};
pub use sequencer::{SequencerSet, DEFAULT_SEQUENCER_MIN_BOND, SEQUENCER_BOND_ESCROW};
pub use types::{Batch, BatchStatus, EvmTx, FraudProof};

#[cfg(feature = "revm")]
pub use revm_exec::{
    encode_create, encode_transfer, encode_value_transfer, AccountSnapDto, RevmExecutor,
};
#[cfg(feature = "revm")]
pub use signed_tx::{decode_evm_tx, DecodedEvmTx};
