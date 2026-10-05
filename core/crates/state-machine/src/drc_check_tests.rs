//! Native DRC check behavior tests (exact-value authorization; no create-time lock).

#[cfg(test)]
mod tests {
    use agora_types::{Amount, Hash, NativeAssetId};

    use crate::accounts::load_account;
    use crate::drc_check::{load_drc_check_live, load_drc_check_receipt, lookup_drc_check_point};
    use crate::drc_check_test_harness::support::{
        apply_check_block, auth, coinbase, create_live_check, fund, key, signed_cancel,
        signed_cash, snapshot_check_state,
    };
    use crate::StateStore;

    #[test]
    fn create_charges_fee_only_not_amount() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(1);
        let dest = key(2);
        fund(&store, &owner, 100);
        let before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let (_, _create) = create_live_check(&store, &owner, &dest, 40, None, 1, &ctx);
        let after = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(after.balance, before.balance - 1);
        assert_eq!(after.nonce, before.nonce + 1);
    }

    #[test]
    fn exact_cash_moves_owner_to_destination() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(3);
        let dest = key(4);
        fund(&store, &owner, 100);
        fund(&store, &dest, 6);
        let (check_id, _create) = create_live_check(&store, &owner, &dest, 40, None, 1, &ctx);
        let snap_before = snapshot_check_state(&store, &owner, &dest);
        let cash = signed_cash(&dest, check_id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_check_cashes.push(cash);
        apply_check_block(&store, block, 2, &ctx);
        let snap_after = snapshot_check_state(&store, &owner, &dest);
        assert_eq!(snap_before.owner_balance - snap_after.owner_balance, 40);
        assert_eq!(
            snap_after.destination_balance - snap_before.destination_balance,
            40
        );
        assert_eq!(
            lookup_drc_check_point(&store, &check_id).unwrap(),
            "unknown"
        );
        let receipt = load_drc_check_receipt(&store, &check_id).unwrap().unwrap();
        assert_eq!(receipt.amount, Amount::from_base_units(40));
    }

    #[test]
    fn insufficient_owner_balance_leaves_check_live() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(5);
        let dest = key(6);
        fund(&store, &owner, 10);
        fund(&store, &dest, 5);
        let (check_id, _create) = create_live_check(&store, &owner, &dest, 40, None, 1, &ctx);
        let cash = signed_cash(&dest, check_id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_check_cashes.push(cash);
        let result = crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block,
            50,
            Some(&ctx),
            2,
        );
        assert!(result.is_err());
        assert_eq!(lookup_drc_check_point(&store, &check_id).unwrap(), "live");
        assert!(load_drc_check_live(&store, &check_id).unwrap().is_some());
    }

    #[test]
    fn expiration_rejects_cash_at_boundary() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(7);
        let dest = key(8);
        fund(&store, &owner, 100);
        fund(&store, &dest, 5);
        let (check_id, _create) = create_live_check(&store, &owner, &dest, 10, Some(5), 1, &ctx);
        let cash = signed_cash(&dest, check_id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_check_cashes.push(cash);
        let result = crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block,
            50,
            Some(&ctx),
            5,
        );
        assert!(result.is_err());
        assert_eq!(lookup_drc_check_point(&store, &check_id).unwrap(), "live");
    }

    #[test]
    fn owner_may_cancel_before_expiration() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(9);
        let dest = key(10);
        fund(&store, &owner, 100);
        let (check_id, _create) = create_live_check(&store, &owner, &dest, 10, Some(50), 1, &ctx);
        let cancel = signed_cancel(&owner, check_id, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_cancels.push(cancel);
        apply_check_block(&store, block, 2, &ctx);
        assert_eq!(
            lookup_drc_check_point(&store, &check_id).unwrap(),
            "unknown"
        );
    }
}
