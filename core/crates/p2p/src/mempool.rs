use std::collections::{HashMap, HashSet};

use agora_types::{
    resolve_drc_account_sequence, AccountTransfer, Address, Block, DrcAccountPolicyTx,
    DrcAccountSequence, DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx,
    DrcDepositPreauthAction, DrcDepositPreauthTx, DrcEscrowCancelTx, DrcEscrowCreateTx,
    DrcEscrowFinishTx, DrcPaymentTx, DrcRegularKeyTx, DrcSignerListTx, DrcTicketCreateTx, Hash,
    NativeAssetId, OutPoint, OvlExecutionTx, SignedStakeTx, Transaction,
    ACCOUNT_TRANSFER_DRC_TICKET_VERSION, DRC_ACCOUNT_POLICY_TICKET_TX_VERSION,
    DRC_CHECK_CANCEL_TICKET_VERSION, DRC_CHECK_CASH_TICKET_VERSION,
    DRC_CHECK_CREATE_TICKET_VERSION, DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION,
    DRC_ESCROW_CANCEL_TICKET_VERSION, DRC_ESCROW_CREATE_TICKET_VERSION,
    DRC_ESCROW_FINISH_TICKET_VERSION, DRC_PAYMENT_TICKET_VERSION,
    DRC_REGULAR_KEY_TICKET_TX_VERSION, DRC_SIGNER_LIST_TICKET_TX_VERSION, STAKE_TX_TICKET_VERSION,
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
    drc_regular_key_txs: HashMap<Hash, DrcRegularKeyTx>,
    drc_signer_list_txs: HashMap<Hash, DrcSignerListTx>,
    drc_ticket_create_txs: HashMap<Hash, DrcTicketCreateTx>,
    drc_escrow_create_txs: HashMap<Hash, DrcEscrowCreateTx>,
    drc_escrow_finish_txs: HashMap<Hash, DrcEscrowFinishTx>,
    drc_escrow_cancel_txs: HashMap<Hash, DrcEscrowCancelTx>,
    /// Mempool-only: escrow ids with a pending create not yet canonical.
    pending_escrow_ids: HashSet<Hash>,
    /// At most one pending finish or cancel per live escrow object id.
    reserved_escrow_settlements: HashMap<Hash, Hash>,
    drc_check_create_txs: HashMap<Hash, DrcCheckCreateTx>,
    drc_check_cash_txs: HashMap<Hash, DrcCheckCashTx>,
    drc_check_cancel_txs: HashMap<Hash, DrcCheckCancelTx>,
    pending_check_ids: HashSet<Hash>,
    reserved_check_settlements: HashMap<Hash, Hash>,
    /// One pending consumer per `(owner, ticket_sequence)`.
    reserved_tickets: HashSet<(Address, u64)>,
    /// How each DRC lane operation reserved its sender slot (release on eviction).
    drc_slot_reservations: HashMap<Hash, DrcSlotReservation>,
    /// Payments admitted while canonical or pending DepositAuth is enabled.
    deposit_auth_required_payments: HashSet<Hash>,
    /// Payments whose source has a canonical dormant/active preauthorization.
    deposit_preauthorized_payments: HashSet<Hash>,
    /// Every OVL/DRC account lane shares the same per-asset account nonce.
    reserved_accounts: HashSet<(NativeAssetId, Address)>,
    max_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrcSlotReservation {
    AccountNonce,
    Ticket(u64),
    /// Pending ticket-create: reserves owner nonce and the prospective ticket sequence.
    TicketCreate {
        ticket_sequence: u64,
    },
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
            drc_regular_key_txs: HashMap::new(),
            drc_signer_list_txs: HashMap::new(),
            drc_ticket_create_txs: HashMap::new(),
            drc_escrow_create_txs: HashMap::new(),
            drc_escrow_finish_txs: HashMap::new(),
            drc_escrow_cancel_txs: HashMap::new(),
            pending_escrow_ids: HashSet::new(),
            reserved_escrow_settlements: HashMap::new(),
            drc_check_create_txs: HashMap::new(),
            drc_check_cash_txs: HashMap::new(),
            drc_check_cancel_txs: HashMap::new(),
            pending_check_ids: HashSet::new(),
            reserved_check_settlements: HashMap::new(),
            reserved_tickets: HashSet::new(),
            drc_slot_reservations: HashMap::new(),
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
            + self.drc_regular_key_txs.len()
            + self.drc_signer_list_txs.len()
            + self.drc_ticket_create_txs.len()
            + self.drc_escrow_create_txs.len()
            + self.drc_escrow_finish_txs.len()
            + self.drc_escrow_cancel_txs.len()
            + self.drc_check_create_txs.len()
            + self.drc_check_cash_txs.len()
            + self.drc_check_cancel_txs.len()
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
            || self.drc_regular_key_txs.contains_key(tx_id)
            || self.drc_signer_list_txs.contains_key(tx_id)
            || self.drc_ticket_create_txs.contains_key(tx_id)
            || self.drc_escrow_create_txs.contains_key(tx_id)
            || self.drc_escrow_finish_txs.contains_key(tx_id)
            || self.drc_escrow_cancel_txs.contains_key(tx_id)
            || self.drc_check_create_txs.contains_key(tx_id)
            || self.drc_check_cash_txs.contains_key(tx_id)
            || self.drc_check_cancel_txs.contains_key(tx_id)
    }

    pub fn ticket_consumer_reserved(&self, owner: &Address, ticket_sequence: u64) -> bool {
        self.reserved_tickets.contains(&(*owner, ticket_sequence))
    }

    fn reserve_drc_slot(
        &mut self,
        tx_id: Hash,
        owner: Address,
        reservation: DrcSlotReservation,
    ) -> Result<(), P2pError> {
        match reservation {
            DrcSlotReservation::AccountNonce => {
                if !self.reserved_accounts.insert((NativeAssetId::DRC, owner)) {
                    return Err(P2pError::MempoolRejected(
                        "account already has a pending nonce".into(),
                    ));
                }
            }
            DrcSlotReservation::Ticket(sequence) => {
                if !self.reserved_tickets.insert((owner, sequence)) {
                    return Err(P2pError::MempoolRejected(format!(
                        "DRC ticket {sequence} already has a pending consumer"
                    )));
                }
            }
            DrcSlotReservation::TicketCreate { ticket_sequence } => {
                if !self.reserved_accounts.insert((NativeAssetId::DRC, owner)) {
                    return Err(P2pError::MempoolRejected(
                        "account already has a pending nonce".into(),
                    ));
                }
                if !self.reserved_tickets.insert((owner, ticket_sequence)) {
                    self.reserved_accounts.remove(&(NativeAssetId::DRC, owner));
                    return Err(P2pError::MempoolRejected(format!(
                        "DRC ticket {ticket_sequence} already reserved"
                    )));
                }
            }
        }
        self.drc_slot_reservations.insert(tx_id, reservation);
        Ok(())
    }

    fn release_drc_slot(&mut self, tx_id: &Hash, owner: Address) {
        let Some(reservation) = self.drc_slot_reservations.remove(tx_id) else {
            return;
        };
        match reservation {
            DrcSlotReservation::AccountNonce => {
                self.reserved_accounts.remove(&(NativeAssetId::DRC, owner));
            }
            DrcSlotReservation::Ticket(sequence) => {
                self.reserved_tickets.remove(&(owner, sequence));
            }
            DrcSlotReservation::TicketCreate { ticket_sequence } => {
                self.reserved_accounts.remove(&(NativeAssetId::DRC, owner));
                self.reserved_tickets.remove(&(owner, ticket_sequence));
            }
        }
    }

    fn drc_sender_reservation_from_selector(
        _owner: Address,
        version: u32,
        ticket_capable_version: u32,
        nonce: u64,
        account_sequence: Option<agora_types::DrcAccountSequenceSelector>,
    ) -> Result<DrcSlotReservation, P2pError> {
        let selector =
            resolve_drc_account_sequence(version, ticket_capable_version, nonce, account_sequence)
                .map_err(|error| P2pError::MempoolRejected(error.to_string()))?;
        Ok(match selector.kind {
            DrcAccountSequence::Nonce => DrcSlotReservation::AccountNonce,
            DrcAccountSequence::Ticket => DrcSlotReservation::Ticket(selector.value),
        })
    }

    fn drc_account_transfer_reservation(
        tx: &AccountTransfer,
    ) -> Result<DrcSlotReservation, P2pError> {
        if tx.asset != NativeAssetId::DRC {
            return Ok(DrcSlotReservation::AccountNonce);
        }
        Self::drc_sender_reservation_from_selector(
            tx.from,
            tx.version,
            ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
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
        if tx.asset == NativeAssetId::DRC && tx.version >= ACCOUNT_TRANSFER_DRC_TICKET_VERSION {
            let reservation = Self::drc_account_transfer_reservation(&tx)?;
            self.reserve_drc_slot(id, tx.from, reservation)?;
        } else {
            let key = (tx.asset, tx.from);
            if !self.reserved_accounts.insert(key) {
                return Err(P2pError::MempoolRejected(
                    "account already has a pending nonce".into(),
                ));
            }
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
        if tx.asset == NativeAssetId::DRC && tx.version >= STAKE_TX_TICKET_VERSION {
            let reservation = Self::drc_sender_reservation_from_selector(
                tx.actor,
                tx.version,
                STAKE_TX_TICKET_VERSION,
                tx.nonce,
                tx.account_sequence,
            )?;
            self.reserve_drc_slot(id, tx.actor, reservation)?;
        } else {
            let key = (tx.asset, tx.actor);
            if !self.reserved_accounts.insert(key) {
                return Err(P2pError::MempoolRejected(
                    "account already has a pending nonce".into(),
                ));
            }
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
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.from,
            tx.version,
            DRC_PAYMENT_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_drc_slot(id, tx.from, reservation)?;
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
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.account,
            tx.version,
            DRC_ACCOUNT_POLICY_TICKET_TX_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_drc_slot(id, tx.account, reservation)?;
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
    pub fn admit_drc_regular_key(&mut self, tx: DrcRegularKeyTx) -> Result<Hash, P2pError> {
        let id = tx.regular_key_tx_id();
        if self.drc_regular_key_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.owner,
            tx.version,
            DRC_REGULAR_KEY_TICKET_TX_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_drc_slot(id, tx.owner, reservation)?;
        self.drc_regular_key_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_signer_list(&mut self, tx: DrcSignerListTx) -> Result<Hash, P2pError> {
        let id = tx.signer_list_tx_id();
        if self.drc_signer_list_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.owner,
            tx.version,
            DRC_SIGNER_LIST_TICKET_TX_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_drc_slot(id, tx.owner, reservation)?;
        self.drc_signer_list_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_deposit_preauth(&mut self, tx: DrcDepositPreauthTx) -> Result<Hash, P2pError> {
        let id = tx.preauth_tx_id();
        if self.drc_deposit_preauth_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.owner,
            tx.version,
            DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_drc_slot(id, tx.owner, reservation)?;
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
        self.release_drc_slot(payment_id, payment.from);
        self.deposit_auth_required_payments.remove(payment_id);
        self.deposit_preauthorized_payments.remove(payment_id);
        Some(payment)
    }

    pub fn admit_drc_ticket_create(&mut self, tx: DrcTicketCreateTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|error| P2pError::MempoolRejected(error.to_string()))?;
        let id = tx.ticket_create_tx_id();
        if self.drc_ticket_create_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let ticket_sequence = tx
            .nonce
            .checked_add(1)
            .ok_or_else(|| P2pError::MempoolRejected("DRC ticket sequence overflow".into()))?;
        self.reserve_drc_slot(
            id,
            tx.owner,
            DrcSlotReservation::TicketCreate { ticket_sequence },
        )?;
        self.drc_ticket_create_txs.insert(id, tx);
        Ok(id)
    }

    pub fn remove_drc_ticket_create(&mut self, id: &Hash) -> Option<DrcTicketCreateTx> {
        let tx = self.drc_ticket_create_txs.remove(id)?;
        self.release_drc_slot(id, tx.owner);
        Some(tx)
    }

    pub fn pending_escrow_create(&self, escrow_id: &Hash) -> bool {
        self.pending_escrow_ids.contains(escrow_id)
    }

    pub fn escrow_settlement_reserved(&self, escrow_id: &Hash) -> bool {
        self.reserved_escrow_settlements.contains_key(escrow_id)
    }

    fn reserve_escrow_settlement(&mut self, escrow_id: Hash, tx_id: Hash) -> Result<(), P2pError> {
        if self.reserved_escrow_settlements.contains_key(&escrow_id) {
            return Err(P2pError::MempoolRejected(
                "escrow already has a pending finish or cancel".into(),
            ));
        }
        self.reserved_escrow_settlements.insert(escrow_id, tx_id);
        Ok(())
    }

    fn release_escrow_settlement(&mut self, escrow_id: &Hash) {
        self.reserved_escrow_settlements.remove(escrow_id);
    }

    pub fn pending_check_create(&self, check_id: &Hash) -> bool {
        self.pending_check_ids.contains(check_id)
    }

    pub fn check_settlement_reserved(&self, check_id: &Hash) -> bool {
        self.reserved_check_settlements.contains_key(check_id)
    }

    fn reserve_check_settlement(&mut self, check_id: Hash, tx_id: Hash) -> Result<(), P2pError> {
        if self.reserved_check_settlements.contains_key(&check_id) {
            return Err(P2pError::MempoolRejected(
                "check already has a pending cash or cancel".into(),
            ));
        }
        self.reserved_check_settlements.insert(check_id, tx_id);
        Ok(())
    }

    fn release_check_settlement(&mut self, check_id: &Hash) {
        self.reserved_check_settlements.remove(check_id);
    }

    pub fn admit_drc_escrow_create(&mut self, tx: DrcEscrowCreateTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        let id = tx.escrow_id();
        if self.drc_escrow_create_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.owner,
            tx.version,
            DRC_ESCROW_CREATE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_drc_slot(id, tx.owner, reservation)?;
        self.pending_escrow_ids.insert(id);
        self.drc_escrow_create_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_escrow_finish(&mut self, tx: DrcEscrowFinishTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        if self.pending_escrow_ids.contains(&tx.escrow_id) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects escrow finish while create is pending (same-block is consensus-only)"
                    .into(),
            ));
        }
        let id = tx.finish_tx_id();
        if self.drc_escrow_finish_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self.reserved_escrow_settlements.contains_key(&tx.escrow_id) {
            return Err(P2pError::MempoolRejected(
                "escrow already has a pending finish or cancel".into(),
            ));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.submitter,
            tx.version,
            DRC_ESCROW_FINISH_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_escrow_settlement(tx.escrow_id, id)?;
        if let Err(e) = self.reserve_drc_slot(id, tx.submitter, reservation) {
            self.release_escrow_settlement(&tx.escrow_id);
            return Err(e);
        }
        self.drc_escrow_finish_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_escrow_cancel(&mut self, tx: DrcEscrowCancelTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        if self.pending_escrow_ids.contains(&tx.escrow_id) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects escrow cancel while create is pending (same-block is consensus-only)"
                    .into(),
            ));
        }
        let id = tx.cancel_tx_id();
        if self.drc_escrow_cancel_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self.reserved_escrow_settlements.contains_key(&tx.escrow_id) {
            return Err(P2pError::MempoolRejected(
                "escrow already has a pending finish or cancel".into(),
            ));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.submitter,
            tx.version,
            DRC_ESCROW_CANCEL_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_escrow_settlement(tx.escrow_id, id)?;
        if let Err(e) = self.reserve_drc_slot(id, tx.submitter, reservation) {
            self.release_escrow_settlement(&tx.escrow_id);
            return Err(e);
        }
        self.drc_escrow_cancel_txs.insert(id, tx);
        Ok(id)
    }

    pub fn remove_drc_escrow_create(&mut self, id: &Hash) -> Option<DrcEscrowCreateTx> {
        let tx = self.drc_escrow_create_txs.remove(id)?;
        self.pending_escrow_ids.remove(id);
        self.release_drc_slot(id, tx.owner);
        Some(tx)
    }

    pub fn remove_drc_escrow_finish(&mut self, id: &Hash) -> Option<DrcEscrowFinishTx> {
        let tx = self.drc_escrow_finish_txs.remove(id)?;
        self.release_escrow_settlement(&tx.escrow_id);
        self.release_drc_slot(id, tx.submitter);
        Some(tx)
    }

    pub fn remove_drc_escrow_cancel(&mut self, id: &Hash) -> Option<DrcEscrowCancelTx> {
        let tx = self.drc_escrow_cancel_txs.remove(id)?;
        self.release_escrow_settlement(&tx.escrow_id);
        self.release_drc_slot(id, tx.submitter);
        Some(tx)
    }

    pub fn select_drc_escrow_creates(&self, max: usize) -> Vec<DrcEscrowCreateTx> {
        let mut txs: Vec<_> = self.drc_escrow_create_txs.values().cloned().collect();
        txs.sort_by(|a, b| a.escrow_id().as_bytes().cmp(b.escrow_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn select_drc_escrow_finishes(&self, max: usize) -> Vec<DrcEscrowFinishTx> {
        let mut txs: Vec<_> = self.drc_escrow_finish_txs.values().cloned().collect();
        txs.sort_by(|a, b| a.finish_tx_id().as_bytes().cmp(b.finish_tx_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn select_drc_escrow_cancels(&self, max: usize) -> Vec<DrcEscrowCancelTx> {
        let mut txs: Vec<_> = self.drc_escrow_cancel_txs.values().cloned().collect();
        txs.sort_by(|a, b| a.cancel_tx_id().as_bytes().cmp(b.cancel_tx_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn admit_drc_check_create(&mut self, tx: DrcCheckCreateTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        let id = tx.check_id();
        if self.drc_check_create_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.owner,
            tx.version,
            DRC_CHECK_CREATE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_drc_slot(id, tx.owner, reservation)?;
        self.pending_check_ids.insert(id);
        self.drc_check_create_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_check_cash(&mut self, tx: DrcCheckCashTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        if self.pending_check_ids.contains(&tx.check_id) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects check cash while create is pending (same-block is consensus-only)"
                    .into(),
            ));
        }
        let id = tx.cash_tx_id();
        if self.drc_check_cash_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self.reserved_check_settlements.contains_key(&tx.check_id) {
            return Err(P2pError::MempoolRejected(
                "check already has a pending cash or cancel".into(),
            ));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.submitter,
            tx.version,
            DRC_CHECK_CASH_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_check_settlement(tx.check_id, id)?;
        if let Err(e) = self.reserve_drc_slot(id, tx.submitter, reservation) {
            self.release_check_settlement(&tx.check_id);
            return Err(e);
        }
        self.drc_check_cash_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_check_cancel(&mut self, tx: DrcCheckCancelTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        if self.pending_check_ids.contains(&tx.check_id) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects check cancel while create is pending (same-block is consensus-only)"
                    .into(),
            ));
        }
        let id = tx.cancel_tx_id();
        if self.drc_check_cancel_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self.reserved_check_settlements.contains_key(&tx.check_id) {
            return Err(P2pError::MempoolRejected(
                "check already has a pending cash or cancel".into(),
            ));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.submitter,
            tx.version,
            DRC_CHECK_CANCEL_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_check_settlement(tx.check_id, id)?;
        if let Err(e) = self.reserve_drc_slot(id, tx.submitter, reservation) {
            self.release_check_settlement(&tx.check_id);
            return Err(e);
        }
        self.drc_check_cancel_txs.insert(id, tx);
        Ok(id)
    }

    pub fn remove_drc_check_create(&mut self, id: &Hash) -> Option<DrcCheckCreateTx> {
        let tx = self.drc_check_create_txs.remove(id)?;
        self.pending_check_ids.remove(id);
        self.release_drc_slot(id, tx.owner);
        Some(tx)
    }

    pub fn remove_drc_check_cash(&mut self, id: &Hash) -> Option<DrcCheckCashTx> {
        let tx = self.drc_check_cash_txs.remove(id)?;
        self.release_check_settlement(&tx.check_id);
        self.release_drc_slot(id, tx.submitter);
        Some(tx)
    }

    pub fn remove_drc_check_cancel(&mut self, id: &Hash) -> Option<DrcCheckCancelTx> {
        let tx = self.drc_check_cancel_txs.remove(id)?;
        self.release_check_settlement(&tx.check_id);
        self.release_drc_slot(id, tx.submitter);
        Some(tx)
    }

    pub fn select_drc_check_creates(&self, max: usize) -> Vec<DrcCheckCreateTx> {
        let mut txs: Vec<_> = self.drc_check_create_txs.values().cloned().collect();
        txs.sort_by(|a, b| a.check_id().as_bytes().cmp(b.check_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn select_drc_check_cashes(&self, max: usize) -> Vec<DrcCheckCashTx> {
        let mut txs: Vec<_> = self.drc_check_cash_txs.values().cloned().collect();
        txs.sort_by(|a, b| a.cash_tx_id().as_bytes().cmp(b.cash_tx_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn select_drc_check_cancels(&self, max: usize) -> Vec<DrcCheckCancelTx> {
        let mut txs: Vec<_> = self.drc_check_cancel_txs.values().cloned().collect();
        txs.sort_by(|a, b| a.cancel_tx_id().as_bytes().cmp(b.cancel_tx_id().as_bytes()));
        txs.truncate(max);
        txs
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

    pub fn select_drc_regular_keys(&self, max: usize) -> Vec<DrcRegularKeyTx> {
        let mut txs: Vec<_> = self.drc_regular_key_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            b.fee
                .as_base_units()
                .cmp(&a.fee.as_base_units())
                .then_with(|| {
                    a.regular_key_tx_id()
                        .as_bytes()
                        .cmp(b.regular_key_tx_id().as_bytes())
                })
        });
        txs.truncate(max);
        txs
    }

    pub fn select_drc_signer_lists(&self, max: usize) -> Vec<DrcSignerListTx> {
        let mut txs: Vec<_> = self.drc_signer_list_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            b.fee
                .as_base_units()
                .cmp(&a.fee.as_base_units())
                .then_with(|| {
                    a.signer_list_tx_id()
                        .as_bytes()
                        .cmp(b.signer_list_tx_id().as_bytes())
                })
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

    pub fn select_drc_ticket_creates(&self, max: usize) -> Vec<DrcTicketCreateTx> {
        let mut txs: Vec<_> = self.drc_ticket_create_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            b.fee
                .as_base_units()
                .cmp(&a.fee.as_base_units())
                .then_with(|| {
                    a.ticket_create_tx_id()
                        .as_bytes()
                        .cmp(b.ticket_create_tx_id().as_bytes())
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
            if let Some(removed) = self.account_txs.remove(&id) {
                if self.drc_slot_reservations.contains_key(&id) {
                    self.release_drc_slot(&id, removed.from);
                } else {
                    self.reserved_accounts
                        .remove(&(removed.asset, removed.from));
                }
            }
        }
        for tx in &block.stake_ops {
            consumed_account_nonces.insert((tx.asset, tx.actor));
            let id = tx.stake_tx_id();
            if let Some(removed) = self.stake_txs.remove(&id) {
                if self.drc_slot_reservations.contains_key(&id) {
                    self.release_drc_slot(&id, removed.actor);
                } else {
                    self.reserved_accounts
                        .remove(&(removed.asset, removed.actor));
                }
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
        for tx in &block.drc_regular_keys {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.owner));
            let id = tx.regular_key_tx_id();
            if self.drc_regular_key_txs.remove(&id).is_some() {
                self.release_drc_slot(&id, tx.owner);
            }
        }
        for tx in &block.drc_signer_lists {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.owner));
            let id = tx.signer_list_tx_id();
            if self.drc_signer_list_txs.remove(&id).is_some() {
                self.release_drc_slot(&id, tx.owner);
            }
        }
        for tx in &block.drc_ticket_creates {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.owner));
            let id = tx.ticket_create_tx_id();
            let _ = self.remove_drc_ticket_create(&id);
        }
        for tx in &block.drc_escrow_creates {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.owner));
            let id = tx.escrow_id();
            let _ = self.remove_drc_escrow_create(&id);
        }
        for tx in &block.drc_escrow_finishes {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.submitter));
            let id = tx.finish_tx_id();
            let _ = self.remove_drc_escrow_finish(&id);
        }
        for tx in &block.drc_escrow_cancels {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.submitter));
            let id = tx.cancel_tx_id();
            let _ = self.remove_drc_escrow_cancel(&id);
        }
        for tx in &block.drc_account_policies {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.account));
            let id = tx.policy_tx_id();
            if self.drc_policy_txs.remove(&id).is_some() {
                self.release_drc_slot(&id, tx.account);
            }
        }
        for tx in &block.drc_deposit_preauths {
            consumed_account_nonces.insert((NativeAssetId::DRC, tx.owner));
            let id = tx.preauth_tx_id();
            if self.drc_deposit_preauth_txs.remove(&id).is_some() {
                self.release_drc_slot(&id, tx.owner);
            }
        }
        // A peer block can consume a nonce with a different operation than the
        // local resident. Evict every now-stale operation sharing that nonce.
        let stale_accounts: Vec<(Hash, Address, NativeAssetId)> = self
            .account_txs
            .iter()
            .filter_map(|(id, tx)| {
                consumed_account_nonces
                    .contains(&(tx.asset, tx.from))
                    .then_some((*id, tx.from, tx.asset))
            })
            .collect();
        for (id, from, asset) in stale_accounts {
            self.account_txs.remove(&id);
            if self.drc_slot_reservations.contains_key(&id) {
                self.release_drc_slot(&id, from);
            } else {
                self.reserved_accounts.remove(&(asset, from));
            }
        }
        let stale_stakes: Vec<(Hash, Address, NativeAssetId)> = self
            .stake_txs
            .iter()
            .filter_map(|(id, tx)| {
                consumed_account_nonces
                    .contains(&(tx.asset, tx.actor))
                    .then_some((*id, tx.actor, tx.asset))
            })
            .collect();
        for (id, actor, asset) in stale_stakes {
            self.stake_txs.remove(&id);
            if self.drc_slot_reservations.contains_key(&id) {
                self.release_drc_slot(&id, actor);
            } else {
                self.reserved_accounts.remove(&(asset, actor));
            }
        }
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
        let stale_policies: Vec<(Hash, Address)> = self
            .drc_policy_txs
            .iter()
            .filter_map(|(id, tx)| {
                consumed_account_nonces
                    .contains(&(NativeAssetId::DRC, tx.account))
                    .then_some((*id, tx.account))
            })
            .collect();
        for (id, account) in stale_policies {
            self.drc_policy_txs.remove(&id);
            self.release_drc_slot(&id, account);
        }
        let stale_preauths: Vec<(Hash, Address)> = self
            .drc_deposit_preauth_txs
            .iter()
            .filter_map(|(id, tx)| {
                consumed_account_nonces
                    .contains(&(NativeAssetId::DRC, tx.owner))
                    .then_some((*id, tx.owner))
            })
            .collect();
        for (id, owner) in stale_preauths {
            self.drc_deposit_preauth_txs.remove(&id);
            self.release_drc_slot(&id, owner);
        }
        let stale_regular_keys: Vec<(Hash, Address)> = self
            .drc_regular_key_txs
            .iter()
            .filter_map(|(id, tx)| {
                consumed_account_nonces
                    .contains(&(NativeAssetId::DRC, tx.owner))
                    .then_some((*id, tx.owner))
            })
            .collect();
        for (id, owner) in stale_regular_keys {
            self.drc_regular_key_txs.remove(&id);
            self.release_drc_slot(&id, owner);
        }
        let stale_signer_lists: Vec<(Hash, Address)> = self
            .drc_signer_list_txs
            .iter()
            .filter_map(|(id, tx)| {
                consumed_account_nonces
                    .contains(&(NativeAssetId::DRC, tx.owner))
                    .then_some((*id, tx.owner))
            })
            .collect();
        for (id, owner) in stale_signer_lists {
            self.drc_signer_list_txs.remove(&id);
            self.release_drc_slot(&id, owner);
        }
        let stale_ticket_creates: Vec<Hash> = self
            .drc_ticket_create_txs
            .iter()
            .filter_map(|(id, tx)| {
                consumed_account_nonces
                    .contains(&(NativeAssetId::DRC, tx.owner))
                    .then_some(*id)
            })
            .collect();
        for id in stale_ticket_creates {
            let _ = self.remove_drc_ticket_create(&id);
        }
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
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_multisign_attachments: vec![],
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
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_multisign_attachments: vec![],
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
    fn master_disable_policy_shares_nonce_without_mempool_recovery_overlay() {
        use agora_types::DrcAccountPolicyTx;

        let owner = Address([8; 20]);
        let disable =
            DrcAccountPolicyTx::set_master_key_disabled(owner, Amount::from_base_units(1), 0);
        let clear =
            DrcAccountPolicyTx::clear_master_key_disabled(owner, Amount::from_base_units(2), 0);

        let mut pool = Mempool::new(4);
        pool.admit_drc_policy(disable.clone()).unwrap();
        assert!(
            pool.admit_drc_policy(clear).is_err(),
            "mempool does not model same-nonce recovery pairing; block apply orders lanes"
        );
        assert_eq!(pool.select_drc_account_policies(2), vec![disable]);
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

    #[test]
    fn drc_ticket_create_reserves_nonce_and_prospective_sequence() {
        let owner = Address([3; 20]);
        let create = DrcTicketCreateTx::unsigned(owner, Amount::from_base_units(1), 0);
        let mut pool = Mempool::new(8);
        let id = pool.admit_drc_ticket_create(create).unwrap();
        assert!(pool.account_reserved(NativeAssetId::DRC, &owner));
        assert!(pool.ticket_consumer_reserved(&owner, 1));
        pool.remove_drc_ticket_create(&id).unwrap();
        assert!(!pool.account_reserved(NativeAssetId::DRC, &owner));
        assert!(!pool.ticket_consumer_reserved(&owner, 1));
    }

    #[test]
    fn duplicate_ticket_consumer_rejected_in_mempool() {
        let owner = Address([4; 20]);
        let mut pay_a = DrcPaymentTx::unsigned_v4(
            owner,
            Address([5; 20]),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        pay_a.version = DRC_PAYMENT_TICKET_VERSION;
        pay_a.account_sequence = Some(agora_types::DrcAccountSequenceSelector::ticket(9));
        let mut pay_b = pay_a.clone();
        pay_b.nonce = 1;
        let mut pool = Mempool::new(8);
        pool.admit_payment(pay_a.clone()).unwrap();
        assert!(pool.ticket_consumer_reserved(&owner, 9));
        pay_b.amount = Amount::from_base_units(2);
        assert!(pool.admit_payment(pay_b).is_err());
    }

    #[test]
    fn pending_ticket_create_blocks_prospective_ticket_consumer() {
        let owner = Address([6; 20]);
        let create = DrcTicketCreateTx::unsigned(owner, Amount::from_base_units(1), 0);
        let mut pay = DrcPaymentTx::unsigned_v4(
            owner,
            Address([7; 20]),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        pay.version = DRC_PAYMENT_TICKET_VERSION;
        pay.account_sequence = Some(agora_types::DrcAccountSequenceSelector::ticket(1));
        let mut pool = Mempool::new(8);
        pool.admit_drc_ticket_create(create).unwrap();
        assert!(pool.ticket_consumer_reserved(&owner, 1));
        assert!(pool.admit_payment(pay).is_err());
    }

    #[test]
    fn ticket_create_block_inclusion_releases_create_and_consumer_reservations() {
        let owner = Address([8; 20]);
        let create = DrcTicketCreateTx::unsigned(owner, Amount::from_base_units(1), 0);
        let mut pool = Mempool::new(8);
        let id = pool.admit_drc_ticket_create(create.clone()).unwrap();
        assert!(pool.account_reserved(NativeAssetId::DRC, &owner));
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_ticket_creates.push(create);
        block.header.tx_root = block.compute_body_root();
        pool.evict_for_block(&block);
        assert!(!pool.contains(&id));
        assert!(!pool.account_reserved(NativeAssetId::DRC, &owner));
        assert!(!pool.ticket_consumer_reserved(&owner, 1));
        let mut pay = DrcPaymentTx::unsigned_v4(
            owner,
            Address([9; 20]),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        pay.version = DRC_PAYMENT_TICKET_VERSION;
        pay.account_sequence = Some(agora_types::DrcAccountSequenceSelector::ticket(1));
        assert!(pool.admit_payment(pay).is_ok());
        assert!(pool.ticket_consumer_reserved(&owner, 1));
    }

    #[test]
    fn pending_escrow_create_blocks_finish_and_cancel_in_mempool() {
        let owner = Address([10; 20]);
        let recipient = Address([11; 20]);
        let create = DrcEscrowCreateTx {
            version: agora_types::DRC_ESCROW_CREATE_TX_VERSION,
            owner,
            recipient,
            amount: Amount::from_base_units(10),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(100),
            nonce: 0,
            account_sequence: None,
            public_key: vec![1; 33],
            signature: vec![1; 64],
            multisign: None,
        };
        let finish = DrcEscrowFinishTx {
            version: agora_types::DRC_ESCROW_FINISH_TX_VERSION,
            submitter: owner,
            escrow_id: create.escrow_id(),
            fee: Amount::from_base_units(1),
            nonce: 1,
            account_sequence: None,
            public_key: vec![1; 33],
            signature: vec![1; 64],
            multisign: None,
        };
        let cancel = DrcEscrowCancelTx {
            version: agora_types::DRC_ESCROW_CANCEL_TX_VERSION,
            submitter: owner,
            escrow_id: create.escrow_id(),
            fee: Amount::from_base_units(1),
            nonce: 1,
            account_sequence: None,
            public_key: vec![1; 33],
            signature: vec![1; 64],
            multisign: None,
        };
        let mut pool = Mempool::new(16);
        pool.admit_drc_escrow_create(create).unwrap();
        assert!(pool.pending_escrow_create(&finish.escrow_id));
        assert!(pool.admit_drc_escrow_finish(finish).is_err());
        assert!(pool.admit_drc_escrow_cancel(cancel).is_err());
    }

    #[test]
    fn escrow_cancel_reserves_submitter_nonce_not_owner() {
        let owner = Address([12; 20]);
        let helper = Address([13; 20]);
        let create = DrcEscrowCreateTx {
            version: agora_types::DRC_ESCROW_CREATE_TX_VERSION,
            owner,
            recipient: Address([14; 20]),
            amount: Amount::from_base_units(5),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(50),
            nonce: 0,
            account_sequence: None,
            public_key: vec![1; 33],
            signature: vec![1; 64],
            multisign: None,
        };
        let cancel = DrcEscrowCancelTx {
            version: agora_types::DRC_ESCROW_CANCEL_TX_VERSION,
            submitter: helper,
            escrow_id: create.escrow_id(),
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: vec![1; 33],
            signature: vec![1; 64],
            multisign: None,
        };
        let mut pool = Mempool::new(8);
        let create_id = create.escrow_id();
        pool.admit_drc_escrow_create(create).unwrap();
        pool.remove_drc_escrow_create(&create_id);
        let cancel_id = pool.admit_drc_escrow_cancel(cancel).unwrap();
        assert!(pool.account_reserved(NativeAssetId::DRC, &helper));
        assert!(!pool.account_reserved(NativeAssetId::DRC, &owner));
        pool.remove_drc_escrow_cancel(&cancel_id).unwrap();
        assert!(!pool.account_reserved(NativeAssetId::DRC, &helper));
    }

    #[test]
    fn pending_escrow_finish_blocks_cancel_on_same_escrow_id() {
        let owner = Address([15; 20]);
        let escrow_id = Hash([16; 32]);
        let finish = DrcEscrowFinishTx {
            version: agora_types::DRC_ESCROW_FINISH_TX_VERSION,
            submitter: owner,
            escrow_id,
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: vec![1; 33],
            signature: vec![1; 64],
            multisign: None,
        };
        let cancel = DrcEscrowCancelTx {
            version: agora_types::DRC_ESCROW_CANCEL_TX_VERSION,
            submitter: owner,
            escrow_id,
            fee: Amount::from_base_units(1),
            nonce: 1,
            account_sequence: None,
            public_key: vec![1; 33],
            signature: vec![1; 64],
            multisign: None,
        };
        let mut pool = Mempool::new(8);
        pool.admit_drc_escrow_finish(finish).unwrap();
        assert!(pool.escrow_settlement_reserved(&escrow_id));
        assert!(pool.admit_drc_escrow_cancel(cancel).is_err());
    }

    #[test]
    fn escrow_create_ticket_sequence_reserved_in_mempool() {
        let owner = Address([17; 20]);
        let create = DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TICKET_VERSION,
            owner,
            recipient: Address([18; 20]),
            amount: Amount::from_base_units(3),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(40),
            nonce: 0,
            account_sequence: Some(agora_types::DrcAccountSequenceSelector::ticket(7)),
            public_key: vec![1; 33],
            signature: vec![1; 64],
            multisign: None,
        };
        let mut pool = Mempool::new(8);
        let id = create.escrow_id();
        pool.admit_drc_escrow_create(create).unwrap();
        assert!(pool.ticket_consumer_reserved(&owner, 7));
        assert!(pool.pending_escrow_create(&id));
        pool.remove_drc_escrow_create(&id).unwrap();
        assert!(!pool.ticket_consumer_reserved(&owner, 7));
    }
}
