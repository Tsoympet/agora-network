//! Peer-to-peer networking surface for Agora.
//!
//! All transports must go through libp2p (Phase 4). This crate currently exposes
//! topic names and a network config so higher layers can wire without inventing APIs.

mod config;
mod error;
mod topics;

pub use config::NetworkConfig;
pub use error::P2pError;
pub use topics::{TOPIC_BLOCKS, TOPIC_TRANSACTIONS};
