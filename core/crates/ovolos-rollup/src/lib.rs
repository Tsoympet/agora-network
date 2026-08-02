//! Ovolos — Agora's optimistic rollup (L2) for EVM smart-contract scaling.
//!
//! Batches are sequenced optimistically, open to fraud proofs during a challenge
//! window, then finalized. EVM execution is abstracted behind [`EvmExecutor`].
//! Enable the `revm` feature (default) for the audited EVM binding.

mod error;
mod executor;
mod rollup;
mod types;

#[cfg(feature = "revm")]
mod revm_exec;

pub use error::RollupError;
pub use executor::{reexecute_batch, EvmExecutor, StubEvmExecutor};
pub use rollup::{OvolosRollup, RollupConfig};
pub use types::{Batch, BatchStatus, EvmTx, FraudProof};

#[cfg(feature = "revm")]
pub use revm_exec::{encode_transfer, RevmExecutor};
