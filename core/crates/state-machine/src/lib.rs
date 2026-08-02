//! Persistent state application for consensus-ordered blocks.
//!
//! Triple-zone layout (hot / warm / archival) keeps sub-second tips off cold storage I/O.

mod error;
mod store;
mod zones;

pub use error::StateError;
pub use store::StateStore;
pub use zones::StateZone;
