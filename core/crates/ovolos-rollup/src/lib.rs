//! Ovolos — Agora's optimistic rollup (L2) for EVM smart-contract scaling.
//!
//! Batches are sequenced optimistically, open to fraud proofs during a challenge
//! window, then finalized. Successful challenges rewind the head. Operators post
//! [`BatchCommitment`] blobs to L1 / Agora DA. OVL gas is a layered mark ledger
//! (not an L1 UTXO asset).

mod da;
mod error;
mod executor;
mod genesis;
mod ovl;
mod rollup;
mod types;

#[cfg(feature = "revm")]
mod revm_exec;

pub use da::{tx_merkle_root, BatchCommitment};
pub use error::RollupError;
pub use executor::{reexecute_batch, EvmExecutor, StubEvmExecutor};
pub use genesis::{OvlPremine, OvolosGenesis};
pub use ovl::{OvlLedger, DEFAULT_GAS_PER_TX, OVL_MAX_SUPPLY_BASE};
pub use rollup::{OvolosRollup, RollupConfig};
pub use types::{Batch, BatchStatus, EvmTx, FraudProof};

#[cfg(feature = "revm")]
pub use revm_exec::{encode_transfer, RevmExecutor};
