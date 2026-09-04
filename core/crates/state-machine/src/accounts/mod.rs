//! Native OVL / DRC account modules (Trident L1).
//!
//! TLT remains UTXO. OVL and DRC balances + nonces live here and commit into the
//! same atomic [`WriteBatch`] as UTXO apply when callers include account ops.

use agora_crypto::verify_account_transfer_bound;
use agora_types::{AccountTransfer, Address, Amount, Hash, NativeAssetId};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::supply::{load_issued_supply, put_issued_supply_into};
use crate::{StateError, StateStore};

/// Persistent account record for one (asset, address).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Default)]
pub struct AccountState {
    pub balance: u64,
    pub nonce: u64,
}

pub fn account_key(asset: NativeAssetId, address: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(16 + 20);
    key.extend_from_slice(b"account/");
    key.push(asset.wire_byte());
    key.push(b'/');
    key.extend_from_slice(&address.0);
    key
}

pub fn load_account(
    store: &StateStore,
    asset: NativeAssetId,
    address: &Address,
) -> Result<AccountState, StateError> {
    if asset == NativeAssetId::TLT {
        return Err(StateError::InvalidTx(
            "TLT uses UTXO module, not account module".into(),
        ));
    }
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &account_key(asset, address))? else {
        return Ok(AccountState::default());
    };
    AccountState::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))
}

pub fn put_account_into(
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    address: &Address,
    state: &AccountState,
) -> Result<(), StateError> {
    if asset == NativeAssetId::TLT {
        return Err(StateError::InvalidTx(
            "TLT uses UTXO module, not account module".into(),
        ));
    }
    let bytes = borsh::to_vec(state).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, &account_key(asset, address), &bytes);
    Ok(())
}

/// Credit `amount` to `address` without nonce bump (genesis / treasury / rewards).
pub fn credit_account_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    asset: NativeAssetId,
    address: &Address,
    amount: Amount,
) -> Result<(), StateError> {
    if amount.as_base_units() == 0 {
        return Err(StateError::InvalidTx("zero credit".into()));
    }
    let mut acct = load_account(store, asset, address)?;
    // Overlay: prefer batch? For genesis, store is empty. For apply chains, caller
    // must pass a store that already reflects prior batch writes (COW overlay).
    acct.balance = acct
        .balance
        .checked_add(amount.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("balance overflow".into()))?;
    put_account_into(batch, asset, address, &acct)
}

/// Journal of account mutations for revert.
#[derive(Debug, Clone, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AccountJournal {
    pub before: Vec<(NativeAssetId, Address, AccountState)>,
}

/// Apply a signed account transfer. Validates auth, nonce, balances **before** mutation.
pub fn apply_account_transfer(
    store: &StateStore,
    tx: &AccountTransfer,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<(), StateError> {
    apply_account_transfer_checked(store, tx, Some(auth), batch, journal)
}

/// Like [`apply_account_transfer`] with optional auth (tests / unsigned fixtures).
pub(crate) fn apply_account_transfer_checked(
    store: &StateStore,
    tx: &AccountTransfer,
    auth: Option<&TxAuthContext>,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<(), StateError> {
    if tx.asset == NativeAssetId::TLT {
        return Err(StateError::InvalidTx("cannot account-transfer TLT".into()));
    }
    if !matches!(tx.asset, NativeAssetId::OVL | NativeAssetId::DRC) {
        return Err(StateError::InvalidTx("unknown native asset".into()));
    }
    if tx.amount.as_base_units() == 0 {
        return Err(StateError::InvalidTx("zero transfer".into()));
    }
    if tx.from == tx.to {
        return Err(StateError::InvalidTx("self-transfer forbidden".into()));
    }

    // Auth before any mutation.
    if let Some(ctx) = auth {
        verify_account_transfer_bound(tx, &ctx.chain_id, &ctx.genesis)
            .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    } else if tx.public_key.is_empty() && tx.signature.is_empty() {
        // Unsigned only allowed in tests that pass auth=None explicitly — still
        // require from address consistency via empty skip; production callers
        // must supply auth.
    } else {
        return Err(StateError::InvalidTx(
            "account transfer requires network-bound auth".into(),
        ));
    }

    let mut from = load_account(store, tx.asset, &tx.from)?;
    let mut to = load_account(store, tx.asset, &tx.to)?;

    if from.nonce != tx.nonce {
        return Err(StateError::InvalidTx(format!(
            "bad nonce: got {} expected {}",
            tx.nonce, from.nonce
        )));
    }
    // Recipient overflow before debit. Fee is same-asset and never credited to `to`.
    let new_to = to
        .balance
        .checked_add(tx.amount.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("recipient overflow".into()))?;
    let debit = tx
        .amount
        .as_base_units()
        .checked_add(tx.fee.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("amount+fee overflow".into()))?;
    if from.balance < debit {
        return Err(StateError::InvalidTx("insufficient account balance".into()));
    }

    journal.before.push((tx.asset, tx.from, from.clone()));
    journal.before.push((tx.asset, tx.to, to.clone()));

    from.balance -= debit;
    from.nonce = from
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("nonce overflow".into()))?;
    to.balance = new_to;

    put_account_into(batch, tx.asset, &tx.from, &from)?;
    put_account_into(batch, tx.asset, &tx.to, &to)?;
    Ok(())
}

pub fn revert_account_journal_into(
    batch: &mut WriteBatch,
    journal: &AccountJournal,
) -> Result<(), StateError> {
    // Restore in reverse so last-writer wins for duplicate addresses.
    for (asset, address, state) in journal.before.iter().rev() {
        put_account_into(batch, *asset, address, state)?;
    }
    Ok(())
}

/// Genesis credit for OVL/DRC allocations (increments issued supply).
pub fn genesis_credit(
    store: &StateStore,
    batch: &mut WriteBatch,
    asset: NativeAssetId,
    address: &Address,
    amount: Amount,
) -> Result<(), StateError> {
    credit_account_into(batch, store, asset, address, amount)?;
    let issued = load_issued_supply(store, asset)?;
    let next = issued
        .checked_add(amount.as_base_units())
        .ok_or(StateError::SupplyCapExceeded)?;
    put_issued_supply_into(batch, asset, next);
    Ok(())
}

/// Deterministic account-root commitment for one asset (sorted address keys).
pub fn account_root(store: &StateStore, asset: NativeAssetId) -> Result<Hash, StateError> {
    let prefix = {
        let mut p = b"account/".to_vec();
        p.push(asset.wire_byte());
        p.push(b'/');
        p
    };
    let mut entries: Vec<(Address, AccountState)> = Vec::new();
    store.for_each_cf(ColumnFamily::Meta, |key, value| {
        if key.starts_with(&prefix) && key.len() == prefix.len() + 20 {
            let mut addr = [0u8; 20];
            addr.copy_from_slice(&key[prefix.len()..]);
            let state = AccountState::try_from_slice(value)
                .map_err(|e| StateError::Storage(e.to_string()))?;
            if state.balance > 0 || state.nonce > 0 {
                entries.push((Address(addr), state));
            }
        }
        Ok(())
    })?;
    entries.sort_by(|a, b| a.0 .0.cmp(&b.0 .0));
    Ok(Hash::hash_borsh(&(asset.wire_byte(), &entries)))
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_account_transfer_bound, Bip44Path};
    use agora_types::{AccountTransfer, Address, Amount, Hash, NativeAssetId};

    use super::*;
    use crate::StateStore;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn ovl_transfer_atomic_and_nonce() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = Hash([9u8; 32]);
        let auth = TxAuthContext {
            chain_id: "agora-trident-testnet-1".into(),
            genesis,
        };

        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            &store,
            NativeAssetId::OVL,
            &alice.address(),
            Amount::from_base_units(1_000),
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let mut tx = AccountTransfer::unsigned(
            NativeAssetId::OVL,
            alice.address(),
            bob.address(),
            Amount::from_base_units(400),
            0,
        );
        sign_account_transfer_bound(&mut tx, &alice, &auth.chain_id, &auth.genesis).unwrap();

        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_account_transfer(&store, &tx, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &alice.address())
                .unwrap()
                .balance,
            600
        );
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &alice.address())
                .unwrap()
                .nonce,
            1
        );
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &bob.address())
                .unwrap()
                .balance,
            400
        );

        // Replay same nonce → reject before debit.
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_account_transfer(&store, &tx, &auth, &mut batch, &mut journal).is_err());
    }

    #[test]
    fn rejects_tlt_and_cross_asset_confusion() {
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let tx = AccountTransfer::unsigned(
            NativeAssetId::TLT,
            Address::ZERO,
            Address([1u8; 20]),
            Amount::from_base_units(1),
            0,
        );
        assert!(
            apply_account_transfer_checked(&store, &tx, None, &mut batch, &mut journal).is_err()
        );
    }

    #[test]
    fn recipient_overflow_checked_before_debit() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let mut batch = WriteBatch::new();
        put_account_into(
            &mut batch,
            NativeAssetId::DRC,
            &alice.address(),
            &AccountState {
                balance: 10,
                nonce: 0,
            },
        )
        .unwrap();
        put_account_into(
            &mut batch,
            NativeAssetId::DRC,
            &bob.address(),
            &AccountState {
                balance: u64::MAX,
                nonce: 0,
            },
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let tx = AccountTransfer::unsigned(
            NativeAssetId::DRC,
            alice.address(),
            bob.address(),
            Amount::from_base_units(1),
            0,
        );
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let err = apply_account_transfer_checked(&store, &tx, None, &mut batch, &mut journal)
            .unwrap_err();
        assert!(matches!(err, StateError::InvalidTx(_)));
        // Alice unchanged.
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &alice.address())
                .unwrap()
                .balance,
            10
        );
    }
}
