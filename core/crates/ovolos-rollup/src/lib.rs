//! Ovolos — Agora's optimistic rollup (L2) for EVM smart-contract scaling.
//!
//! **OVL is native PoW money on L2** (`sha256_leading_zero` block seals + coinbase
//! emission). It is not an L1 UTXO asset. Batches are sequenced optimistically,
//! open to fraud proofs during a challenge window, then finalized. Operators post
//! [`BatchCommitment`] blobs to L1 / Agora DA.

mod da;
mod error;
mod executor;
mod genesis;
mod ovl;
mod pow;
mod rollup;
mod types;

#[cfg(feature = "revm")]
mod revm_exec;

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
pub use rollup::{OvolosRollup, RollupConfig};
pub use types::{Batch, BatchStatus, EvmTx, FraudProof};

#[cfg(feature = "revm")]
pub use revm_exec::{encode_create, encode_transfer, encode_value_transfer, RevmExecutor};
