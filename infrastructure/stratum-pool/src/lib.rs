//! kHeavyHash ASIC stratum pool for Agora Network.
//!
//! Aggregates miner shares against job templates. PoW hashing currently uses a
//! SHA-256 stand-in until the audited kHeavyHash library is linked.

mod error;
mod job;
mod pool;
mod protocol;

pub use error::StratumError;
pub use job::{leading_zero_bits, share_id, MiningJob};
pub use pool::{AcceptedShare, StratumPool};
pub use protocol::{StratumRequest, StratumResponse};
