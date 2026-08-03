//! Runnable Agora layer stack: Ovolos (L2) + Bridge-in-a-Box (L3) + Intent-Engine (L4).
//!
//! This is an in-process operator runtime — not a claim of a public multi-chain
//! network. L1 money remains TLT; OVL/DRC are layered mark ledgers.

mod error;
mod runtime;

pub use error::LayersError;
pub use runtime::{LayerInfo, LayersRuntime, LayersRuntimeConfig};
