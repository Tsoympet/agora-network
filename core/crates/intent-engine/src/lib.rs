//! Intent-Engine (L4) — AI-driven asset orchestration over District bridges.
//!
//! Users declare desired outcomes; solvers propose routes; settlement can hand
//! off to [`agora_bridge_sdk::BridgeBox`].

mod engine;
mod error;
mod intent;

pub use engine::{IntentEngine, IntentSolver, NaiveSolver};
pub use error::IntentError;
pub use intent::{Intent, IntentStatus, Solution};
