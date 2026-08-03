//! Bridge-in-a-Box SDK for Agora District Chains (L3).
//!
//! **DRC is native PoW money on L3** (`sha256_leading_zero` hub/district blocks +
//! coinbase emission). It is not an L1 UTXO asset. Districts connect to the hub
//! via lock-mint and burn-unlock messages. Light-client merkle proofs and
//! [`MessageTransport`] cover messaging handoff.

mod bridge;
mod district;
mod drc;
mod error;
mod genesis;
mod messages;
mod pow;
mod proof;
mod transport;

pub use bridge::BridgeBox;
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
