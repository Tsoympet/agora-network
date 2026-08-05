//! Shared Agora BlockDAG primitives.
//!
//! Consensus-critical encoding uses `borsh`. Client bindings are generated with `ts-rs`.

mod acceptance;
mod amount;
mod block;
mod hash;
mod network;
mod transaction;

pub use acceptance::{AcceptanceBitmap, TxAcceptanceStatus, TxConfirmation};
pub use amount::Amount;
pub use block::{Block, BlockHeader};
pub use hash::Hash;
pub use network::NetworkFingerprint;
pub use transaction::{Address, OutPoint, Transaction, TransactionBody, TxIn, TxOut};

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshDeserialize;

    fn test_fingerprint() -> NetworkFingerprint {
        NetworkFingerprint {
            network_name: "agora-test".into(),
            network_id: 1,
            genesis_hash: Hash::ZERO,
            ghostdag_k: 18,
            max_supply: 1,
            premine: 0,
            initial_reward: 50,
            halving_interval: 210_000,
        }
    }

    #[test]
    fn amount_whole_conversion() {
        let one = Amount::from_whole(1).expect("1 AGORA");
        assert_eq!(one.as_base_units(), 100_000_000);
        assert_eq!(one.checked_add(one).unwrap().as_base_units(), 200_000_000);
    }

    #[test]
    fn transaction_borsh_roundtrip_and_id_stable() {
        let fp = test_fingerprint();
        let tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(42),
                address: Address::ZERO,
            }],
            7,
        );
        let bytes = borsh::to_vec(&tx).unwrap();
        let decoded = Transaction::try_from_slice(&bytes).unwrap();
        assert_eq!(tx, decoded);
        assert_eq!(tx.tx_id(), decoded.tx_id());
        let signing = tx.signing_bytes(&fp);
        assert!(signing.len() > 32);
        assert_eq!(&signing[..32], fp.digest().as_bytes());
    }

    #[test]
    fn acceptance_bitmap_roundtrip_bits() {
        let flags = vec![true, false, true, true, false, false, false, true, true];
        let bitmap = AcceptanceBitmap::from_bools(&flags);
        assert_eq!(bitmap.len, flags.len() as u32);
        assert_eq!(bitmap.to_bools(), flags);
        assert_eq!(bitmap.accepted_count(), 5);
        let bytes = borsh::to_vec(&bitmap).unwrap();
        let decoded = AcceptanceBitmap::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded, bitmap);
    }

    #[test]
    fn network_fingerprint_digest_stable() {
        let a = test_fingerprint();
        let b = test_fingerprint();
        assert_eq!(a.digest(), b.digest());
        let mut c = test_fingerprint();
        c.network_id = 2;
        assert_ne!(a.digest(), c.digest());
    }

    #[test]
    fn block_tx_root_deterministic() {
        let tx = Transaction::unsigned(1, vec![], vec![], 1);
        let root = Block::compute_tx_root(std::slice::from_ref(&tx));
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 1,
            bits: 1,
            nonce: 0,
            tx_root: root,
        };
        let block = Block {
            header: header.clone(),
            transactions: vec![tx],
        };
        assert_eq!(block.id(), header.hash());
        assert_eq!(Block::compute_tx_root(&block.transactions), root);
        assert!(block.verify_tx_root());
    }

    #[test]
    fn tx_root_rejects_odd_length_duplicate_collision() {
        let a = Transaction::unsigned(1, vec![], vec![], 1);
        let b = Transaction::unsigned(1, vec![], vec![], 2);
        let c = Transaction::unsigned(1, vec![], vec![], 3);
        let root_abc = Block::compute_tx_root(&[a.clone(), b.clone(), c.clone()]);
        let root_abcc = Block::compute_tx_root(&[a, b, c.clone(), c]);
        assert_ne!(
            root_abc, root_abcc,
            "duplicate-last-leaf collision must not be possible"
        );
    }
}

/// Regenerates TypeScript bindings into `bindings/` when tests run.
#[cfg(test)]
mod ts_export {
    use super::*;
    use ts_rs::TS;

    #[test]
    fn export_shared_types() {
        Amount::export_all().expect("export Amount");
        Hash::export_all().expect("export Hash");
        Address::export_all().expect("export Address");
        OutPoint::export_all().expect("export OutPoint");
        TxIn::export_all().expect("export TxIn");
        TxOut::export_all().expect("export TxOut");
        Transaction::export_all().expect("export Transaction");
        BlockHeader::export_all().expect("export BlockHeader");
        Block::export_all().expect("export Block");
        NetworkFingerprint::export_all().expect("export NetworkFingerprint");
        AcceptanceBitmap::export_all().expect("export AcceptanceBitmap");
        TxAcceptanceStatus::export_all().expect("export TxAcceptanceStatus");
        TxConfirmation::export_all().expect("export TxConfirmation");
    }
}
