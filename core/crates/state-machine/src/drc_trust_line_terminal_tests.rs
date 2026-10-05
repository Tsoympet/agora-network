//! Journal revert/reapply and RocksDB reopen parity for trust lines.

#[cfg(test)]
mod journal {
    use agora_types::Hash;

    use crate::drc_trust_line::load_drc_trust_line_live;
    use crate::drc_trust_line_test_harness::support::{
        apply_block, apply_block_journal, asset, auth, coinbase, fund, issuer_outstanding, key,
        line_balance, revert_journal, signed_issued_transfer, signed_trust_line_set, std_code,
        trust_root,
    };
    use crate::StateStore;

    #[test]
    fn revert_trust_line_create_restores_counts_and_root() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(1);
        let holder = key(2);
        fund(&store, &issuer, 50);
        fund(&store, &holder, 50);
        let root0 = trust_root(&store);
        let cur = std_code(b"JRN");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 20, 1, 0, &ctx));
        block.header.tx_root = block.compute_body_root();
        let journal = apply_block_journal(&store, block, 1, &ctx);
        assert_ne!(trust_root(&store), root0);
        revert_journal(&store, &journal);
        assert_eq!(trust_root(&store), root0);
        assert!(
            load_drc_trust_line_live(&store, &holder.address(), &asset(&issuer, cur))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn reapply_after_revert_is_deterministic() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(3);
        let holder = key(4);
        fund(&store, &issuer, 80);
        fund(&store, &holder, 80);
        let cur = std_code(b"RAP");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 30, 1, 0, &ctx));
        block.header.tx_root = block.compute_body_root();
        let journal = apply_block_journal(&store, block.clone(), 1, &ctx);
        let root1 = trust_root(&store);
        revert_journal(&store, &journal);
        apply_block(&store, block, 1, &ctx);
        assert_eq!(trust_root(&store), root1);
    }

    #[test]
    fn revert_issued_transfer_restores_liability_and_balances() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(5);
        let holder = key(6);
        fund(&store, &issuer, 80);
        fund(&store, &holder, 80);
        let cur = std_code(b"XFR");
        let ast = asset(&issuer, cur);
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 50, 1, 0, &ctx));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            12,
            1,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        let journal = apply_block_journal(&store, block, 2, &ctx);
        assert_eq!(line_balance(&store, holder.address(), &ast), 12);
        revert_journal(&store, &journal);
        assert_eq!(line_balance(&store, holder.address(), &ast), 0);
        assert_eq!(issuer_outstanding(&store, &ast), 0);
    }
}

#[cfg(all(test, feature = "rocksdb"))]
mod rocksdb_reopen_parity {
    use agora_types::Hash;

    use crate::drc_trust_line::count_live_trust_lines_for_holder;
    use crate::drc_trust_line_test_harness::support::{
        apply_block, auth, coinbase, fund, key, signed_trust_line_set, std_code, trust_root,
    };
    use crate::StateStore;

    #[test]
    fn reopen_after_trust_line_create() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = auth();
        let issuer = key(7);
        let holder = key(8);
        {
            let store = StateStore::open(dir.path()).unwrap();
            fund(&store, &issuer, 40);
            fund(&store, &holder, 40);
            let mut block = coinbase(vec![Hash::ZERO], &holder);
            block.drc_trust_line_sets.push(signed_trust_line_set(
                &holder,
                &issuer,
                std_code(b"RDB"),
                15,
                1,
                0,
                &ctx,
            ));
            block.header.tx_root = block.compute_body_root();
            apply_block(&store, block, 1, &ctx);
        }
        let reopened = StateStore::open(dir.path()).unwrap();
        assert_eq!(
            count_live_trust_lines_for_holder(&reopened, &holder.address()).unwrap(),
            1
        );
        assert_ne!(trust_root(&reopened), Hash::ZERO);
    }
}
