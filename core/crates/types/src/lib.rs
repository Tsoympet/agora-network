//! Shared Agora BlockDAG primitives.
//!
//! Consensus-critical encoding uses `borsh`. Client bindings are generated with `ts-rs`.

mod amount;
mod block;
mod hash;
mod hrp;
mod transaction;

pub use amount::Amount;
pub use block::{Block, BlockHeader};
pub use hash::Hash;
pub use hrp::{
    address_hrp_for_network, is_known_address_hrp, ADDRESS_HRP, ADDRESS_HRP_DEV,
    ADDRESS_HRP_MAINNET, ADDRESS_HRP_TESTNET,
};
pub use transaction::{Address, OutPoint, Transaction, TransactionBody, TxIn, TxOut};

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshDeserialize;

    #[test]
    fn amount_whole_conversion() {
        let one = Amount::from_whole(1).expect("1 AGORA");
        assert_eq!(one.as_base_units(), 100_000_000);
        assert_eq!(one.checked_add(one).unwrap().as_base_units(), 200_000_000);
    }

    #[test]
    fn address_bech32m_roundtrip_and_parse() {
        let addr = Address::from_hex("ff9ec96f09eb154d038a552ecae59c50204ea9a9").unwrap();
        let encoded = addr.to_bech32();
        // Locked against apps/shared/light-client `encodeAddress` (@scure/base bech32m).
        assert_eq!(encoded, "agora1l70vjmcfav256qu225hv4evu2qsya2dfajrcqc");
        assert_eq!(Address::from_bech32(&encoded), Some(addr));
        assert_eq!(Address::from_bech32(&encoded.to_uppercase()), Some(addr));
        let testnet = addr.to_bech32_hrp(ADDRESS_HRP_TESTNET);
        assert!(testnet.starts_with("agoratest1"));
        assert_eq!(Address::from_bech32(&testnet), Some(addr));
        assert_eq!(Address::parse(&encoded), Some(addr));
        assert_eq!(Address::parse(&testnet), Some(addr));
        assert_eq!(Address::parse(&addr.to_hex()), Some(addr));
        assert_eq!(Address::parse(&format!("0x{}", addr.to_hex())), Some(addr));
        assert_eq!(format!("{addr}"), encoded);
        assert!(Address::from_bech32("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4").is_none());
    }

    #[test]
    fn transaction_borsh_roundtrip_and_id_stable() {
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
        assert!(!tx.signing_bytes().is_empty());
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
    }
}
