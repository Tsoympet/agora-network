//! Ovolos — Agora's optimistic rollup (L2) for EVM smart-contract scaling.
//!
//! Batches are sequenced optimistically, open to fraud proofs during a challenge
//! window, then finalized. EVM execution is abstracted behind [`EvmExecutor`].

mod error;
mod executor;
mod rollup;
mod types;

pub use error::RollupError;
pub use executor::{reexecute_batch, EvmExecutor, StubEvmExecutor};
pub use rollup::{OvolosRollup, RollupConfig};
pub use types::{Batch, BatchStatus, EvmTx, FraudProof};
