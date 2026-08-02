//! kHeavyHash ASIC stratum pool for Agora Network.
//!
//! Aggregates miner shares against job templates pulled from `agora-node`
//! (`agora_getBlockTemplate`). Network-difficulty shares are forwarded via
//! `agora_submitBlock`. Share PoW uses the audited Kaspa kHeavyHash digest via
//! `agora_consensus::KHeavyHashPowHasher`.

mod error;
mod job;
pub mod node_rpc;
mod pool;
mod protocol;

pub use error::StratumError;
pub use job::{leading_zero_bits, share_id, MiningJob};
pub use pool::{AcceptedShare, StratumPool};
pub use protocol::{StratumRequest, StratumResponse};
