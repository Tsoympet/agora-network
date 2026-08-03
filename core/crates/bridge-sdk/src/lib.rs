//! Bridge-in-a-Box SDK for Agora District Chains (L3).
//!
//! Districts (gaming / privacy / general) connect to the hub via lock-mint and
//! burn-unlock messages. DRC balances are a layered mark ledger (not L1 UTXOs).
//! Light-client merkle proofs and [`MessageTransport`] cover messaging handoff.

mod bridge;
mod district;
mod drc;
mod error;
mod genesis;
mod messages;
mod proof;
mod transport;

pub use bridge::BridgeBox;
pub use district::{DistrictConfig, DistrictKind};
pub use drc::{DrcLedger, DRC_MAX_SUPPLY_BASE};
pub use error::BridgeError;
pub use genesis::{DrachmaGenesis, DrcPremine, GenesisDistrict};
pub use messages::{BridgeDirection, BridgeMessage, MessageStatus};
pub use proof::{merkle_root, prove_inclusion, prove_message, verify_inclusion, LightClientProof};
pub use transport::{InMemoryTransport, MessageTransport};
