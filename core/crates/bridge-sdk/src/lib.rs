//! Bridge-in-a-Box SDK for Agora District Chains (L3).
//!
//! Districts (gaming / privacy / general) connect to the hub via lock-mint and
//! burn-unlock messages without forking consensus crates. Light-client merkle
//! proofs and a [`MessageTransport`] cover production messaging handoff.

mod bridge;
mod district;
mod error;
mod messages;
mod proof;
mod transport;

pub use bridge::BridgeBox;
pub use district::{DistrictConfig, DistrictKind};
pub use error::BridgeError;
pub use messages::{BridgeDirection, BridgeMessage, MessageStatus};
pub use proof::{
    merkle_root, prove_inclusion, prove_message, verify_inclusion, LightClientProof,
};
pub use transport::{InMemoryTransport, MessageTransport};
