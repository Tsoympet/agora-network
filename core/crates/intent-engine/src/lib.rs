//! Intent-Engine (L4) — asset orchestration over District bridges and AMMs.
//!
//! Users declare desired outcomes; solvers propose routes; settlement uses
//! [`agora_bridge_sdk::BridgeBox`] for cross-district moves or a constant-product
//! AMM for same-district swaps.

mod amm;
mod engine;
mod error;
mod intent;

pub use amm::ConstantProductPool;
pub use engine::{AmmSolver, CompositeSolver, IntentEngine, IntentSolver, NaiveSolver};
pub use error::IntentError;
pub use intent::{Intent, IntentStatus, Solution};
