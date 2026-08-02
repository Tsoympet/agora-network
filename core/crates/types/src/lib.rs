//! Shared Agora BlockDAG primitives.
//!
//! Consensus-critical encoding uses `borsh`. Client bindings are generated with `ts-rs`.

mod hash;
mod transaction;
mod block;

pub use block::{Block, BlockHeader};
pub use hash::Hash;
pub use transaction::{Address, Transaction, TxOut};

/// Regenerates TypeScript bindings into `bindings/` when tests run with the export feature path.
#[cfg(test)]
mod ts_export {
    use super::*;
    use ts_rs::TS;

    #[test]
    fn export_shared_types() {
        // Export path keeps apps/ and core/ aligned after type changes.
        Hash::export_all().expect("export Hash");
        Address::export_all().expect("export Address");
        Transaction::export_all().expect("export Transaction");
        BlockHeader::export_all().expect("export BlockHeader");
        Block::export_all().expect("export Block");
    }
}
