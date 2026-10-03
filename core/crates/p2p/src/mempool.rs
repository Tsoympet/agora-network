use std::collections::{HashMap, HashSet};

use agora_types::{
    AccountTransfer, Address, Block, DrcAccountPolicyTx, DrcDepositPreauthAction,
    DrcDepositPreauthTx, DrcPaymentTx, Hash, NativeAssetId, OutPoint, OvlExecutionTx,
    SignedStakeTx, Transaction,
};

use crate::P2pError;

/// Default cap on how many transfer txs a mining template pulls from the pool.
pub const DEFAULT_TEMPLATE_TX_LIMIT: usize = 128;

/// Default minimum implicit fee (base units) for relay / template admission.
pub const DEFAULT_MIN_RELAY_FEE: u64 = 1;

/// Local mempool with signature-gated admission and outpoint reservation.
#[derive(Debug, Default)]
pub struct Mempool {
    txs: HashMap<Hash, Transaction>,
    /// Implicit fee (`in − out`) recorded at admit for fee-ordered selection.
    fees: HashMap<Hash, u64>,
    /// Outpoints spent by txs currently in the pool (conflict detection).
    reserved: HashSet<OutPoint>,
    account_txs: HashMap<Hash, AccountTransfer>,
    stake_txs: HashMap<Hash, SignedStakeTx>,
    execution_txs: HashMap<Hash, OvlExecutionTx>,
    payment_txs: HashMap<Hash, DrcPaymentTx>,
    drc_policy_txs: HashMap<Hash, DrcAccountPolicyTx>,
    drc_deposit_preauth_txs: HashMap<Hash, DrcDepositPreauthTx>,
    /// Payments admitted while canonical or pending DepositAuth is enabled.
    deposit_auth_required_payments: HashSet<Hash>,
    /// Payments whose source has a canonical dormant/active preauthorization.
    deposit_preauthorized_payments: HashSet<Hash>,
    /// Every OVL/DRC account lane shares the same per-asset account nonce.
    reserved_accounts: HashSet<(NativeAssetId, Address)>,
    max_size: usize,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Self {
            txs: HashMap::new(),
            fees: HashMap::new(),
            reserved: HashSet::new(),
            account_txs: HashMap::new(),
            stake_txs: HashMap::new(),
            execution_txs: HashMap::new(),
            payment_txs: HashMap::new(),
            drc_policy_txs: HashMap::new(),
            drc_deposit_preauth_txs: HashMap::new(),
            deposit_auth_required_payments: HashSet::new(),
            deposit_preauthorized_payments: HashSet::new(),
            reserved_accounts: HashSet::new(),
            max_size,
        }
    }

    pub fn len(&self) -> usize {
        self.txs.len()
            + self.account_txs.len()
            + self.stake_txs.len()
            + self.execution_txs.len()
            + self.payment_txs.len()
            + self.drc_policy_txs.len()
            + self.drc_deposit_preauth_txs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, tx_id: &Hash) -> bool {
        self.txs.contains_key(tx_id)
            || self.account_txs.contains_key(tx_id)
            || self.stake_txs.contains_key(tx_id)
            || self.execution_txs.contains_key(tx_id)
            || self.payment_txs.contains_key(tx_id)
            || self.drc_policy_txs.contains_key(tx_id)
            || self.drc_deposit_preauth_txs.contains_key(tx_id)
    }

    /// Outpoints already claimed by mempool transactions.
    pub fn reserved(&self) -> &HashSet<OutPoint> {
        &self.reserved
    }

    pub fn account_reserved(&self, asset: NativeAssetId, address: &Address) -> bool {
        self.reserved_accounts.contains(&(asset, *address))
    }

    /// Admit a transaction after secp256k1 verification and mempool conflict checks.
    ///
    /// Prefer [`Self::admit_priced`] when the caller already computed the implicit fee
    /// via [`agora_state_machine::validate_mempool_tx`].
    ///
    /// Callers that have a live UTXO set should run
    /// [`agora_state_machine::validate_mempool_tx`] under the same mempool lock
    /// before this method so chain UTXO rules and reserved conflicts stay atomic.
    pub fn admit(&mut self, tx: Transaction) -> Result<Hash, P2pError> {
        self.admit_priced(tx, 0)
    }

    /// Admit with an explicit fee used for mining-template ordering.
    ///
    /// When the pool is full, lower-fee transactions are evicted (Bitcoin-class
    /// fee market) so a higher-fee tx can enter. Admission fails only if every
    /// resident pays at least as much as the newcomer.
    pub fn admit_priced(&mut self, tx: Transaction, fee: u64) -> Result<Hash, P2pError> {
        if tx.inputs.is_empty() {
            return Err(P2pError::MempoolRejected(
                "coinbase not allowed in mempool".into(),
            ));
        }
        // Callers must verify signatures (preferably network-bound) before admit.
        // Structural auth presence only — domain verify belongs to the UTXO layer.
        if tx.public_key.len() != 33 || tx.signature.len() != 64 {
            return Err(P2pError::MempoolRejected(
                "transaction missing secp256k1 auth".into(),
            ));
        }
        let id = tx.tx_id();
        if self.txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size && !self.evict_lowest_below(fee) {
            return Err(P2pError::MempoolRejected(
                "mempool full; fee too low to evict".into(),
            ));
        }

        let mut claimed = HashSet::new();
        for input in &tx.inputs {
            let op = input.previous_outpoint;
            if !claimed.insert(op) || self.reserved.contains(&op) {
                return Err(P2pError::MempoolRejected(format!(
                    "double spend {}:{}",
                    op.tx_id.to_hex(),
                    op.index
                )));
            }
        }
        for op in &claimed {
            self.reserved.insert(*op);
        }
        self.fees.insert(id, fee);
        self.txs.insert(id, tx);
        Ok(id)
    }

    /// Admit a pre-validated OVL/DRC account transfer.
    pub fn admit_account(&mut self, tx: AccountTransfer) -> Result<Hash, P2pError> {
        let id = tx.transfer_id();
        if self.account_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let key = (tx.asset, tx.from);
        if !self.reserved_accounts.insert(key) {
            return Err(P2pError::MempoolRejected(
                "account already has a pending nonce".into(),
            ));
        }
        self.account_txs.insert(id, tx);
        Ok(id)
    }

    /// Admit a pre-validated OVL/DRC stake operation.
    pub fn admit_stake(&mut self, tx: SignedStakeTx) -> Result<Hash, P2pError> {
        let id = tx.stake_tx_id();
        if self.stake_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let key = (tx.asset, tx.actor);
        if !self.reserved_accounts.insert(key) {
            return Err(P2pError::MempoolRejected(
                "account already has a pending nonce".into(),
            ));
        }
        self.stake_txs.insert(id, tx);
        Ok(id)
    }

    /// Admit a pre-validated OVL execution envelope.
    pub fn admit_execution(&mut self, tx: OvlExecutionTx) -> Result<Hash, P2pError> {
        let id = tx.tx_id();
        if self.execution_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let key = (NativeAssetId::OVL, tx.from);
        if !self.reserved_accounts.insert(key) {
            return Err(P2pError::MempoolRejected(
                "account already has a pending nonce".into(),
            ));
        }
        self.execution_txs.insert(id, tx);
        Ok(id)
    }

    /// Admit a pre-validated native DRC payment.
    pub fn admit_payment(&mut self, tx: DrcPaymentTx) -> Result<Hash, P2pError> {
        self.admit_payment_with_context(tx, None, false, false)
    }

    /// Admit a payment after checking expiry against the current virtual blue score.
    pub fn admit_payment_at_blue_score(
        &mut self,
        tx: DrcPaymentTx,
        application_blue_score: u64,
    ) -> Result<Hash, P2pError> {
        self.admit_payment_with_context(tx, Some(application_blue_score), false, false)
    }

    /// Admit a payment with its canonical recipient DepositAuth context.
    ///
    /// Pending grants and disables do not authorize relay before settlement.
    /// Pending enables and revokes do take effect because those lanes precede
    /// payments in every locally built block.
    pub fn admit_payment_with_deposit_auth(
        &mut self,
        tx: DrcPaymentTx,
        canonical_deposit_auth_required: bool,
        canonical_preauthorized: bool,
    ) -> Result<Hash, P2pError> {
        self.admit_payment_with_context(
            tx,
            None,
            canonical_deposit_auth_required,
            canonical_preauthorized,
        )
    }

    /// Admit with expiry and canonical recipient DepositAuth context.
    pub fn admit_payment_with_deposit_auth_at_blue_score(
        &mut self,
        tx: DrcPaymentTx,
        application_blue_score: u64,
        canonical_deposit_auth_required: bool,
        canonical_preauthorized: bool,
    ) -> Result<Hash, P2pError> {
        self.admit_payment_with_context(
            tx,
            Some(application_blue_score),
            canonical_deposit_auth_required,
            canonical_preauthorized,
        )
    }

    fn admit_payment_with_context(
        &mut self,
        tx: DrcPaymentTx,
        application_blue_score: Option<u64>,
        canonical_deposit_auth_required: bool,
        canonical_preauthorized: bool,
    ) -> Result<Hash, P2pError> {
        tx.validate_envelope_version()
            .map_err(|error| P2pError::MempoolRejected(error.to_string()))?;
        if tx.version == agora_types::DRC_PAYMENT_VERSION {
            let score = application_blue_score.ok_or_else(|| {
                P2pError::MempoolRejected(
                    "DRC payment v4 requires an application blue score".into(),
                )
            })?;
            if tx.is_expired_at_blue_score(score) {
                return Err(P2pError::MempoolRejected(format!(
                    "expired DRC payment at blue score {score}"
                )));
            }
        }
        let id = tx.payment_id();
        if self.payment_txs.contains_key(&id) {
            return Ok(id);
        }
        if tx.authenticated_destination_tag().is_none()
            && self.drc_policy_txs.values().any(|policy| {
                policy.account == tx.to && policy.action.destination_tag_requirement() == Some(true)
            })
        {
            return Err(P2pError::MempoolRejected(
                "pending recipient policy requires a DRC destination tag".into(),
            ));
        }
        let pending_enable = self.drc_policy_txs.values().any(|policy| {
            policy.account == tx.to && policy.action.deposit_auth_requirement() == Some(true)
        });
        let pending_revoke = self.drc_deposit_preauth_txs.values().any(|preauth| {
            preauth.owner == tx.to
                && preauth.authorized_source == tx.from
                && preauth.action == DrcDepositPreauthAction::Unauthorize
        });
        let deposit_auth_required = canonical_deposit_auth_required || pending_enable;
        if tx.from != tx.to && deposit_auth_required && (!canonical_preauthorized || pending_revoke)
        {
            return Err(P2pError::MempoolRejected(
                "recipient DepositAuth does not authorize this DRC source".into(),
            ));
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let key = (NativeAssetId::DRC, tx.from);
        if !self.reserved_accounts.insert(key) {
            return Err(P2pError::MempoolRejected(
                "account already has a pending nonce".into(),
            ));
        }
        if deposit_auth_required {
            self.deposit_auth_required_payments.insert(id);
        }
        if canonical_preauthorized {
            self.deposit_preauthorized_payments.insert(id);
        }
        self.payment_txs.insert(id, tx);
        Ok(id)
    }

    /// Admit a pre-validated owner-authorized DRC account-policy operation.
    ///
    /// Account-lane replacement is intentionally disabled: the first resident
    /// operation reserves the shared nonce until removal or block eviction.
    pub fn admit_drc_policy(&mut self, tx: DrcAccountPolicyTx) -> Result<Hash, P2pError> {
        let id = tx.policy_tx_id();
        if self.drc_policy_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let key = (NativeAssetId::DRC, tx.account);
        if !self.reserved_accounts.insert(key) {
            return Err(P2pError::MempoolRejected(
                "account already has a pending nonce".into(),
            ));
        }
        let account = tx.account;
        let requires_destination_tag = tx.action.destination_tag_requirement() == Some(true);
        let enables_deposit_auth = tx.action.deposit_auth_requirement() == Some(true);
        self.drc_policy_txs.insert(id, tx);
        if requires_destination_tag || enables_deposit_auth {
            // A valid owner policy has deterministic precedence over the later
            // payment lane. Drop candidates that would make local templates
            // invalid, and reject equivalent candidates while the policy waits.
            let incompatible: Vec<Hash> = self
                .payment_txs
                .iter()
                .filter_map(|(payment_id, payment)| {
                    let missing_tag = requires_destination_tag
                        && payment.to == account
                        && payment.authenticated_destination_tag().is_none();
                    let unauthorized_deposit = enables_deposit_auth
                        && payment.to == account
                        && payment.from != payment.to
                        && !self.deposit_preauthorized_payments.contains(payment_id);
                    (missing_tag || unauthorized_deposit).then_some(*payment_id)
                })
                .collect();
            for payment_id in incompatible {
                self.remove_payment(&payment_id);
            }
            if enables_deposit_auth {
                let guarded: Vec<Hash> = self
                    .payment_txs
                    .iter()
                    .filter_map(|(payment_id, payment)| {
                        (payment.to == account).then_some(*payment_id)
                    })
                    .collect();
                self.deposit_auth_required_payments.extend(guarded);
            }
        }
        Ok(id)
    }

    /// Admit a pre-validated owner-authorized DRC deposit preauthorization.
    pub fn admit_drc_deposit_preauth(&mut self, tx: DrcDepositPreauthTx) -> Result<Hash, P2pError> {
        let id = tx.preauth_tx_id();
        if self.drc_deposit_preauth_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let key = (NativeAssetId::DRC, tx.owner);
        if !self.reserved_accounts.insert(key) {
            return Err(P2pError::MempoolRejected(
                "account already has a pending nonce".into(),
            ));
        }
        let owner = tx.owner;
        let source = tx.authorized_source;
        let revokes = tx.action == DrcDepositPreauthAction::Unauthorize;
        self.drc_deposit_preauth_txs.insert(id, tx);
        if revokes {
            let incompatible: Vec<Hash> = self
                .payment_txs
                .iter()
                .filter_map(|(payment_id, payment)| {
                    (payment.to == owner
                        && payment.from == source
                        && self.deposit_auth_required_payments.contains(payment_id))
                    .then_some(*payment_id)
                })
                .collect();
            for payment_id in incompatible {
                self.remove_payment(&payment_id);
            }
        }
        Ok(id)
    }

    fn remove_payment(&mut self, payment_id: &Hash) -> Option<DrcPaymentTx> {
        let payment = self.payment_txs.remove(payment_id)?;
        self.reserved_accounts
            .remove(&(NativeAssetId::DRC, payment.from));
        self.deposit_auth_required_payments.remove(payment_id);
        self.deposit_preauthorized_payments.remove(payment_id);
        Some(payment)
    }

    /// Drop the lowest-fee resident if its fee is strictly below `fee`.
    /// Returns true when space was made.
    fn evict_lowest_below(&mut self, fee: u64) -> bool {
        let victim = self
            .fees
            .iter()
            .min_by(|(a, fa), (b, fb)| fa.cmp(fb).then_with(|| a.as_bytes().cmp(b.as_bytes())))
            .map(|(id, f)| (*id, *f));
        match victim {
            Some((id, low)) if low < fee => {
                let _ = self.remove(&id);
                true
            }
            _ => false,
        }
    }

    /// Minimum fee currently in the pool (None if empty).
    pub fn min_fee(&self) -> Option<u64> {
        self.fees.values().copied().min()
    }

    /// Median fee of pending txs (None if empty). Used by fee estimation.
    pub fn median_fee(&self) -> Option<u64> {
        if self.fees.is_empty() {
            return None;
        }
        let mut vals: Vec<u64> = self.fees.values().copied().collect();
        vals.sort_unstable();
        Some(vals[vals.len() / 2])
    }

    pub fn get(&self, tx_id: &Hash) -> Option<&Transaction> {
        self.txs.get(tx_id)
    }

    pub fn fee_of(&self, tx_id: &Hash) -> Option<u64> {
        self.fees.get(tx_id).copied()
    }

    /// Lookup by first 8 bytes of `tx_id` for compact-block inflation.
    pub fn get_by_short_id(&self, short_id: &[u8; 8]) -> Option<&Transaction> {
        self.txs
            .iter()
            .find(|(id, _)| &id.as_bytes()[..8] == short_id.as_slice())
            .map(|(_, tx)| tx)
    }

    pub fn remove(&mut self, tx_id: &Hash) -> Option<Transaction> {
        let tx = self.txs.remove(tx_id)?;
        self.fees.remove(tx_id);
        for input in &tx.inputs {
            self.reserved.remove(&input.previous_outpoint);
        }
        Some(tx)
    }

    /// Fee-ordered pending entries (`fee` desc, then `tx_id`) for RPC / templates.
    pub fn pending_entries(&self, max: usize) -> Vec<(Transaction, u64)> {
        let mut entries: Vec<(Transaction, u64)> = self
            .txs
            .values()
            .map(|tx| {
                let fee = self.fees.get(&tx.tx_id()).copied().unwrap_or(0);
                (tx.clone(), fee)
            })
            .collect();
        entries.sort_by(|(a, fa), (b, fb)| {
            fb.cmp(fa)
                .then_with(|| a.tx_id().as_bytes().cmp(b.tx_id().as_bytes()))
        });
        if entries.len() > max {
            entries.truncate(max);
        }
        entries
    }

    /// Fee-ordered transfer selection for mining templates (fee desc, then `tx_id`).
    pub fn select_transfers(&self, max: usize) -> Vec<Transaction> {
        self.pending_entries(max)
            .into_iter()
            .map(|(tx, _)| tx)
            .collect()
    }

    pub fn select_account_transfers(&self, max: usize) -> Vec<AccountTransfer> {
        let mut txs: Vec<_> = self.account_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            b.fee
                .as_base_units()
                .cmp(&a.fee.as_base_units())
                .then_with(|| a.transfer_id().as_bytes().cmp(b.transfer_id().as_bytes()))
        });
        txs.truncate(max);
        txs
    }

    pub fn select_stake_ops(&self, max: usize) -> Vec<SignedStakeTx> {
        let mut txs: Vec<_> = self.stake_txs.values().cloned().collect();
        txs.sort_by_key(SignedStakeTx::stake_tx_id);
        txs.truncate(max);
        txs
    }

    pub fn select_ovl_executions(&self, max: usize) -> Vec<OvlExecutionTx> {
        let mut txs: Vec<_> = self.execution_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            b.max_fee_per_gas
                .cmp(&a.max_fee_per_gas)
                .then_with(|| a.tx_id().as_bytes().cmp(b.tx_id().as_bytes()))
        });
        txs.truncate(max);
        txs
    }

    pub fn select_drc_payments(&self, max: usize) -> Vec<DrcPaymentTx> {
        self.select_drc_payments_matching(max, |_| true)
    }

    /// Template selection omits entries expired at the candidate block's exact score.
    pub fn select_drc_payments_at_blue_score(
        &self,
        max: usize,
        application_blue_score: u64,
    ) -> Vec<DrcPaymentTx> {
        self.select_drc_payments_matching(max, |tx| {
            !tx.is_expired_at_blue_score(application_blue_score)
        })
    }

    fn select_drc_payments_matching(
        &self,
        max: usize,
        predicate: impl Fn(&DrcPaymentTx) -> bool,
    ) -> Vec<DrcPaymentTx> {
        let mut txs: Vec<_> = self
            .payment_txs
            .values()
            .filter(|tx| predicate(tx))
            .cloned()
            .collect();
        txs.sort_by(|a, b| {
            b.fee
                .as_base_units()
                .cmp(&a.fee.as_base_units())
                .then_with(|| a.payment_id().as_bytes().cmp(b.payment_id().as_bytes()))
        });
        txs.truncate(max);
        txs
    }

    pub fn select_drc_account_policies(&self, max: usize) -> Vec<DrcAccountPolicyTx> {
        let mut txs: Vec<_> = self.drc_policy_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            b.fee
                .as_base_units()
                .cmp(&a.fee.as_base_units())
                .then_with(|| a.policy_tx_id().as_bytes().cmp(b.policy_tx_id().as_bytes()))
        });
        txs.truncate(max);
        txs
    }

    pub fn select_drc_deposit_preauths(&self, max: usize) -> Vec<DrcDepositPreauthTx> {
        let mut txs: Vec<_> = self.drc_deposit_preauth_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            b.fee
                .as_base_units()
                .cmp(&a.fee.as_base_units())
                .then_with(|| {
                    a.preauth_tx_id()
                        .as_bytes()
                        .cmp(b.preauth_tx_id().as_bytes())
                })
        });
        txs.truncate(max);
        txs
    }

    /// Drop included txs and any remaining pool txs that spend the same outpoints.
    pub fn evict_for_block(&mut self, block: &Block) {
        self.reconcile_payments_for_block_policies(block);
        let mut spent = HashSet::new();
        let mut included = HashSet::new();
        let mut consumed_account_nonces = HashSet::new();
        for tx in &block.transactions {
            included.insert(tx.tx_id());
            for input in &tx.inputs {
                spent.insert(input.previous_outpoint);
            }
        }
        for tx in &block.account_transfers {
            consumed_account_nonces.insert((tx.asset, tx.from));
            let id = tx.transfer_id();
            if self.account_txs.remove(&id).is_some() {
                self.reserved_accounts.remove(&(tx.asset, tx.from));
            }
        }
        for tx in &block.stake_ops {
            consumed_account_nonces.insert((tx.asset, tx.actor));
            let id = tx.stake_tx_id();
            if self.stake_txs.remove(&id).is_some() {
                self.reserved_accounts.remove(&(tx.asset, tx.actor));
            }
        }
        for tx in &block.ovl_executions {
            consumed_account_nonces.insert((NativeAssetId::OVL, tx.from));
            let id = tx.tx_id();
            if self.execution_txs.remove(&id).is_some() {
                self.reserved_accounts
                    .remove(&(NativeAssetId::OVL, tx.from));
            }
        }
        for tx in &block.drc_payments {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.from));
            let id = tx.payment_id();
            self.remove_payment(&id);
        }
        for tx in &block.drc_account_policies {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.account));
            let id = tx.policy_tx_id();
            if self.drc_policy_txs.remove(&id).is_some() {
                self.reserved_accounts
                    .remove(&(NativeAssetId::DRC, tx.account));
            }
        }
        for tx in &block.drc_deposit_preauths {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.owner));
            let id = tx.preauth_tx_id();
            if self.drc_deposit_preauth_txs.remove(&id).is_some() {
                self.reserved_accounts
                    .remove(&(NativeAssetId::DRC, tx.owner));
            }
        }
        // A peer block can consume a nonce with a different operation than the
        // local resident. Evict every now-stale operation sharing that nonce.
        self.account_txs
            .retain(|_, tx| !consumed_account_nonces.contains(&(tx.asset, tx.from)));
        self.stake_txs
            .retain(|_, tx| !consumed_account_nonces.contains(&(tx.asset, tx.actor)));
        self.execution_txs
            .retain(|_, tx| !consumed_account_nonces.contains(&(NativeAssetId::OVL, tx.from)));
        let stale_payments: Vec<Hash> = self
            .payment_txs
            .iter()
            .filter_map(|(id, tx)| {
                consumed_account_nonces
                    .contains(&(NativeAssetId::DRC, tx.from))
                    .then_some(*id)
            })
            .collect();
        for id in stale_payments {
            self.remove_payment(&id);
        }
        self.drc_policy_txs
            .retain(|_, tx| !consumed_account_nonces.contains(&(NativeAssetId::DRC, tx.account)));
        self.drc_deposit_preauth_txs
            .retain(|_, tx| !consumed_account_nonces.contains(&(NativeAssetId::DRC, tx.owner)));
        for key in consumed_account_nonces {
            self.reserved_accounts.remove(&key);
        }
        let drop: Vec<Hash> = self
            .txs
            .iter()
            .filter(|(id, tx)| {
                included.contains(id)
                    || tx
                        .inputs
                        .iter()
                        .any(|i| spent.contains(&i.previous_outpoint))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in drop {
            let _ = self.remove(&id);
        }
    }

    /// Reconcile a block and evict payments stale at the advanced virtual score.
    pub fn evict_for_block_at_blue_score(&mut self, block: &Block, virtual_blue_score: u64) {
        self.evict_for_block(block);
        self.evict_expired_drc_payments(virtual_blue_score);
    }

    /// Local policy only: consensus validity still uses the containing block's score.
    pub fn evict_expired_drc_payments(&mut self, application_blue_score: u64) {
        let expired: Vec<Hash> = self
            .payment_txs
            .iter()
            .filter_map(|(id, tx)| {
                tx.is_expired_at_blue_score(application_blue_score)
                    .then_some(*id)
            })
            .collect();
        for id in expired {
            self.remove_payment(&id);
        }
    }

    fn reconcile_payments_for_block_policies(&mut self, block: &Block) {
        let payment_ids: Vec<Hash> = self.payment_txs.keys().copied().collect();
        for payment_id in payment_ids {
            let Some(payment) = self.payment_txs.get(&payment_id) else {
                continue;
            };
            let mut require_destination_tag = None;
            let mut deposit_auth_required =
                self.deposit_auth_required_payments.contains(&payment_id);
            let mut preauthorized = self.deposit_preauthorized_payments.contains(&payment_id);
            for policy in &block.drc_account_policies {
                if policy.account != payment.to {
                    continue;
                }
                if let Some(required) = policy.action.destination_tag_requirement() {
                    require_destination_tag = Some(required);
                }
                if let Some(required) = policy.action.deposit_auth_requirement() {
                    deposit_auth_required = required;
                }
            }
            for preauth in &block.drc_deposit_preauths {
                if preauth.owner == payment.to && preauth.authorized_source == payment.from {
                    preauthorized = preauth.action == DrcDepositPreauthAction::Authorize;
                }
            }
            let invalid_tag = require_destination_tag == Some(true)
                && payment.authenticated_destination_tag().is_none();
            let invalid_deposit =
                payment.from != payment.to && deposit_auth_required && !preauthorized;
            if invalid_tag || invalid_deposit {
                self.remove_payment(&payment_id);
                continue;
            }
            if deposit_auth_required {
                self.deposit_auth_required_payments.insert(payment_id);
            } else {
                self.deposit_auth_required_payments.remove(&payment_id);
            }
            if preauthorized {
                self.deposit_preauthorized_payments.insert(payment_id);
            } else {
                self.deposit_preauthorized_payments.remove(&payment_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_types::{Amount, BlockHeader, Hash, OutPoint, Transaction, TxIn, TxOut};

    use super::*;

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn signed_spend(index: u32, nonce: u64) -> Transaction {
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            nonce,
        );
        sign_transaction(&mut tx, &kp).unwrap();
        tx
    }

    #[test]
    fn admits_valid_signed_tx() {
        let tx = signed_spend(0, 1);
        let mut pool = Mempool::new(16);
        let id = pool.admit(tx.clone()).unwrap();
        assert_eq!(id, tx.tx_id());
        assert!(pool.contains(&id));
        assert!(pool.reserved().contains(&OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        }));
    }

    #[test]
    fn pending_entries_fee_ordered() {
        let low = signed_spend(0, 1);
        let high = signed_spend(1, 2);
        let mut pool = Mempool::new(16);
        pool.admit_priced(low.clone(), 1).unwrap();
        pool.admit_priced(high.clone(), 10).unwrap();
        let entries = pool.pending_entries(16);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0.tx_id(), high.tx_id());
        assert_eq!(entries[0].1, 10);
        assert_eq!(entries[1].0.tx_id(), low.tx_id());
    }

    #[test]
    fn rejects_unsigned_tx() {
        let tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![],
            1,
        );
        let mut pool = Mempool::new(16);
        assert!(pool.admit(tx).is_err());
    }

    #[test]
    fn rejects_coinbase_shaped_tx() {
        let tx = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: agora_types::Address::ZERO,
            }],
            1,
        );
        let mut pool = Mempool::new(16);
        let err = pool.admit(tx).unwrap_err().to_string();
        assert!(err.contains("coinbase"), "{err}");
    }

    #[test]
    fn rejects_mempool_double_spend_and_frees_on_remove() {
        let tx_a = signed_spend(0, 1);
        let tx_b = signed_spend(0, 2);
        let mut pool = Mempool::new(16);
        let id_a = pool.admit(tx_a).unwrap();
        assert!(pool.admit(tx_b).is_err());
        pool.remove(&id_a).unwrap();
        assert!(pool.reserved().is_empty());
        let tx_c = signed_spend(0, 3);
        assert!(pool.admit(tx_c).is_ok());
    }

    #[test]
    fn select_transfers_orders_by_fee_then_txid() {
        let mut pool = Mempool::new(16);
        let low = signed_spend(0, 1);
        let high = signed_spend(1, 2);
        let mid = signed_spend(2, 3);
        pool.admit_priced(low.clone(), 1).unwrap();
        pool.admit_priced(high.clone(), 10).unwrap();
        pool.admit_priced(mid.clone(), 5).unwrap();
        let selected = pool.select_transfers(3);
        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].tx_id(), high.tx_id());
        assert_eq!(selected[1].tx_id(), mid.tx_id());
        assert_eq!(selected[2].tx_id(), low.tx_id());
        let capped = pool.select_transfers(2);
        assert_eq!(capped.len(), 2);
        assert_eq!(capped[0].tx_id(), high.tx_id());
    }

    #[test]
    fn full_pool_evicts_lower_fee_for_higher() {
        let mut pool = Mempool::new(2);
        let low = signed_spend(0, 1);
        let mid = signed_spend(1, 2);
        let high = signed_spend(2, 3);
        pool.admit_priced(low.clone(), 1).unwrap();
        pool.admit_priced(mid.clone(), 5).unwrap();
        assert_eq!(pool.len(), 2);
        // Fee not strictly above the lowest resident cannot enter a full pool.
        assert!(pool.admit_priced(high.clone(), 1).is_err());
        // Strictly higher fee evicts the lowest resident.
        let id = pool.admit_priced(high.clone(), 10).unwrap();
        assert_eq!(id, high.tx_id());
        assert!(!pool.contains(&low.tx_id()));
        assert!(pool.contains(&mid.tx_id()));
        assert!(pool.contains(&high.tx_id()));
        assert_eq!(pool.median_fee(), Some(10));
    }

    #[test]
    fn evict_for_block_drops_included_and_conflicts() {
        use agora_types::Block;

        let mut pool = Mempool::new(16);
        let included = signed_spend(0, 1);
        let other = signed_spend(1, 2);
        pool.admit(included.clone()).unwrap();
        pool.admit(other.clone()).unwrap();
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![included.clone()],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
        };
        pool.evict_for_block(&block);
        assert!(!pool.contains(&included.tx_id()));
        assert!(pool.contains(&other.tx_id()));
        assert!(!pool.reserved().contains(&OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        }));
    }

    #[test]
    fn account_stake_and_execution_share_nonce_reservation() {
        use agora_types::{AccountTransfer, NativeAssetId, OvlExecutionTx, SignedStakeTx};

        let actor = agora_types::Address([3; 20]);
        let mut account = AccountTransfer::unsigned_with_fee(
            NativeAssetId::OVL,
            actor,
            agora_types::Address([4; 20]),
            Amount::from_base_units(5),
            Amount::from_base_units(1),
            0,
        );
        account.public_key = vec![2; 33];
        account.signature = vec![3; 64];

        let mut stake = SignedStakeTx::unsigned_unbond_self(NativeAssetId::OVL, actor, 0);
        stake.public_key = vec![2; 33];
        stake.signature = vec![3; 64];
        let execution = OvlExecutionTx::unsigned(
            actor,
            agora_types::Address([5; 20]),
            Amount::ZERO,
            21_000,
            1,
            0,
            vec![],
        );

        let mut pool = Mempool::new(4);
        let account_id = pool.admit_account(account.clone()).unwrap();
        assert!(pool.account_reserved(NativeAssetId::OVL, &actor));
        assert!(pool.admit_stake(stake).is_err());
        assert!(pool.admit_execution(execution).is_err());

        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![],
            account_transfers: vec![account],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
        };
        pool.evict_for_block(&block);
        assert!(!pool.contains(&account_id));
        assert!(!pool.account_reserved(NativeAssetId::OVL, &actor));
    }

    #[test]
    fn drc_payment_shares_account_nonce_reservation() {
        use agora_types::{AccountTransfer, DrcPaymentTx, NativeAssetId};

        let actor = agora_types::Address([6; 20]);
        let recipient = agora_types::Address([7; 20]);
        let mut account = AccountTransfer::unsigned_with_fee(
            NativeAssetId::DRC,
            actor,
            recipient,
            Amount::from_base_units(5),
            Amount::from_base_units(1),
            0,
        );
        account.public_key = vec![2; 33];
        account.signature = vec![3; 64];
        let payment = DrcPaymentTx::unsigned(
            actor,
            recipient,
            Amount::from_base_units(5),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            0,
        );
        let mut pool = Mempool::new(4);
        pool.admit_account(account).unwrap();
        assert!(pool.admit_payment(payment).is_err());
    }

    #[test]
    fn pending_drc_payment_preserves_v2_source_tag() {
        use agora_types::DrcPaymentTx;

        let payment = DrcPaymentTx::unsigned_v2(
            agora_types::Address([6; 20]),
            agora_types::Address([7; 20]),
            Amount::from_base_units(5),
            Amount::from_base_units(1),
            9,
            Some(u32::MAX),
            Hash::ZERO,
            0,
        );
        let mut pool = Mempool::new(4);
        let id = pool.admit_payment(payment.clone()).unwrap();

        assert_eq!(id, payment.payment_id());
        assert_eq!(pool.select_drc_payments(1), vec![payment]);
        assert_eq!(pool.select_drc_payments(1)[0].source_tag, Some(u32::MAX));
    }

    #[test]
    fn drc_expiry_is_inclusive_and_revalidated_for_templates_and_reorg_resubmission() {
        let sender = Address([6; 20]);
        let payment = DrcPaymentTx::unsigned_v4(
            sender,
            Address([7; 20]),
            Amount::from_base_units(5),
            Amount::from_base_units(1),
            Some(0),
            None,
            Hash::ZERO,
            0,
            Some(11),
        );
        let id = payment.payment_id();
        let mut pool = Mempool::new(4);

        pool.admit_payment_at_blue_score(payment.clone(), 10)
            .unwrap();
        assert_eq!(
            pool.select_drc_payments_at_blue_score(1, 11),
            vec![payment.clone()],
            "the exact cutoff remains eligible"
        );
        assert!(
            pool.select_drc_payments_at_blue_score(1, 12).is_empty(),
            "a cutoff+1 template omits the payment"
        );
        assert!(pool.contains(&id));

        pool.evict_expired_drc_payments(12);
        assert!(!pool.contains(&id));
        assert!(!pool.account_reserved(NativeAssetId::DRC, &sender));
        assert!(
            pool.admit_payment_at_blue_score(payment, 11).is_ok(),
            "a lower-score canonical reorg permits explicit resubmission"
        );
    }

    #[test]
    fn drc_policy_shares_nonce_and_has_no_mempool_replacement() {
        use agora_types::DrcAccountPolicyTx;

        let actor = agora_types::Address([8; 20]);
        let recipient = agora_types::Address([9; 20]);
        let policy =
            DrcAccountPolicyTx::set_require_destination_tag(actor, Amount::from_base_units(2), 0);
        let payment = DrcPaymentTx::unsigned_v3(
            actor,
            recipient,
            Amount::from_base_units(1),
            Amount::from_base_units(3),
            None,
            None,
            Hash::ZERO,
            0,
        );
        let replacement = DrcAccountPolicyTx::clear_require_destination_tag(
            actor,
            Amount::from_base_units(100),
            0,
        );

        let mut pool = Mempool::new(4);
        let id = pool.admit_drc_policy(policy.clone()).unwrap();
        assert_eq!(id, policy.policy_tx_id());
        assert_eq!(pool.select_drc_account_policies(1), vec![policy]);
        assert!(pool.admit_payment(payment).is_err());
        assert!(
            pool.admit_drc_policy(replacement).is_err(),
            "higher fee does not replace a resident account nonce"
        );
    }

    #[test]
    fn pending_set_policy_evicts_and_blocks_untagged_recipient_payments() {
        use agora_types::DrcAccountPolicyTx;

        let owner = agora_types::Address([8; 20]);
        let payer = agora_types::Address([7; 20]);
        let untagged = DrcPaymentTx::unsigned_v3(
            payer,
            owner,
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
        );
        let untagged_id = untagged.payment_id();
        let policy =
            DrcAccountPolicyTx::set_require_destination_tag(owner, Amount::from_base_units(1), 0);

        let mut pool = Mempool::new(8);
        pool.admit_payment(untagged.clone()).unwrap();
        pool.admit_drc_policy(policy).unwrap();
        assert!(!pool.contains(&untagged_id));
        assert!(!pool.account_reserved(NativeAssetId::DRC, &payer));
        assert!(pool.admit_payment(untagged).is_err());

        let tagged_zero = DrcPaymentTx::unsigned_v3(
            payer,
            owner,
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            Some(0),
            None,
            Hash::ZERO,
            0,
        );
        pool.admit_payment(tagged_zero.clone()).unwrap();
        assert_eq!(pool.select_drc_payments(1), vec![tagged_zero]);
    }

    #[test]
    fn deposit_preauth_shares_nonce_and_pending_grant_does_not_authorize_early() {
        let owner = Address([8; 20]);
        let source = Address([7; 20]);
        let grant = DrcDepositPreauthTx::authorize(owner, source, Amount::from_base_units(2), 0);
        let policy =
            DrcAccountPolicyTx::set_deposit_auth_required(owner, Amount::from_base_units(1), 0);
        let payment = DrcPaymentTx::unsigned_v3(
            source,
            owner,
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
        );

        let mut pool = Mempool::new(8);
        let id = pool.admit_drc_deposit_preauth(grant.clone()).unwrap();
        assert_eq!(id, grant.preauth_tx_id());
        assert_eq!(pool.select_drc_deposit_preauths(1), vec![grant]);
        assert!(pool.admit_drc_policy(policy).is_err());
        assert!(
            pool.admit_payment_with_deposit_auth(payment, true, false)
                .is_err(),
            "a pending grant does not authorize relay before canonical settlement"
        );
    }

    #[test]
    fn pending_deposit_auth_enable_keeps_only_canonically_preauthorized_payments() {
        let owner = Address([8; 20]);
        let authorized_source = Address([6; 20]);
        let unauthorized_source = Address([7; 20]);
        let authorized = DrcPaymentTx::unsigned_v3(
            authorized_source,
            owner,
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
        );
        let unauthorized = DrcPaymentTx::unsigned_v3(
            unauthorized_source,
            owner,
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
        );
        let policy =
            DrcAccountPolicyTx::set_deposit_auth_required(owner, Amount::from_base_units(1), 0);

        let mut pool = Mempool::new(8);
        pool.admit_payment_with_deposit_auth(authorized.clone(), false, true)
            .unwrap();
        pool.admit_payment_with_deposit_auth(unauthorized.clone(), false, false)
            .unwrap();
        pool.admit_drc_policy(policy).unwrap();

        assert!(pool.contains(&authorized.payment_id()));
        assert!(!pool.contains(&unauthorized.payment_id()));
        assert!(pool
            .admit_payment_with_deposit_auth(unauthorized, false, false)
            .is_err());
    }

    #[test]
    fn pending_revoke_evicts_and_blocks_guarded_payment() {
        let owner = Address([8; 20]);
        let source = Address([7; 20]);
        let payment = DrcPaymentTx::unsigned_v3(
            source,
            owner,
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
        );
        let revoke = DrcDepositPreauthTx::unauthorize(owner, source, Amount::from_base_units(1), 0);

        let mut pool = Mempool::new(8);
        pool.admit_payment_with_deposit_auth(payment.clone(), true, true)
            .unwrap();
        pool.admit_drc_deposit_preauth(revoke).unwrap();
        assert!(!pool.contains(&payment.payment_id()));
        assert!(pool
            .admit_payment_with_deposit_auth(payment, true, true)
            .is_err());
    }
}
