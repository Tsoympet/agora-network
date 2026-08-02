//! Bridge-in-a-Box SDK for Agora District Chains (L3).
//!
//! Districts (gaming / privacy / general) connect to the hub via lock-mint and
//! burn-unlock messages without forking consensus crates.

mod bridge;
mod district;
mod error;
mod messages;

pub use bridge::BridgeBox;
pub use district::{DistrictConfig, DistrictKind};
pub use error::BridgeError;
pub use messages::{BridgeDirection, BridgeMessage, MessageStatus};
