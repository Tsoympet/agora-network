//! Historical Bridge-in-a-Box lab SDK for pre-Trident District Chains.
//!
//! This crate preserves the former DRC L3 prototype for tests and migration
//! evidence. It is non-canonical and non-routable from the Trident node. Its
//! DRC functionality is a closed, typed payment ledger with no VM, bytecode,
//! deploy, contract-call, Hook, or user-program path. Canonical DRC is native
//! Trident L1 account state; OVL is the sole programmable execution domain.

// Tagged payment / path-pay APIs take hub, district, parties, amounts, tags, and
// nonces as distinct parameters; bundling them into option-structs would obscure
// the typed payment surface without improving safety.
#![allow(clippy::too_many_arguments)]

mod attestor;
mod bridge;
mod district;
mod drc;
mod error;
mod genesis;
mod messages;
mod pow;
mod proof;
mod transport;

pub use attestor::{
    AttestorSet, ATTESTOR_BOND_ESCROW, DEFAULT_ATTESTOR_MIN_BOND, DEFAULT_QUORUM_DENOMINATOR,
    DEFAULT_QUORUM_NUMERATOR,
};
pub use bridge::{BridgeBox, BridgeCheckpoint, DEFAULT_PAYMENT_FEE_BASE};
pub use district::{DistrictConfig, DistrictKind};
pub use drc::{DrcLedger, DRC_MAX_SUPPLY_BASE};
pub use error::BridgeError;
pub use genesis::{
    DrachmaGenesis, DrcPremine, GenesisDistrict, DEFAULT_DRC_HALVING_INTERVAL,
    DEFAULT_DRC_INITIAL_REWARD, DEFAULT_DRC_POW_BITS,
};
pub use messages::{BridgeDirection, BridgeMessage, MessageStatus};
pub use pow::{
    leading_zero_bits, messages_root, mine_drc_block, verify_pow, DrcBlock, DrcBlockHeader,
    DrcEmission, DRACHMA_POW_ALGORITHM,
};
pub use proof::{merkle_root, prove_inclusion, prove_message, verify_inclusion, LightClientProof};
pub use transport::{InMemoryTransport, MessageTransport};
