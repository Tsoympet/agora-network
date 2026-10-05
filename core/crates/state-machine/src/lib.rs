//! Persistent state application for consensus-ordered blocks.
//!
//! Five column families (hot / warm / archival / meta / utxo) keep tip validation
//! off cold compaction paths while fixing genesis supply caps in `meta`.

#![cfg_attr(test, allow(clippy::too_many_arguments))]

mod acceptance;
mod accounts;
mod apply;
mod block_zero;
mod columns;
mod community_state;
mod data_availability;
mod drc_account_auth;
mod drc_check;
#[cfg(test)]
mod drc_check_multisign_adversary_matrix;
#[cfg(test)]
mod drc_check_terminal_tests;
#[cfg(test)]
mod drc_check_test_harness;
#[cfg(test)]
mod drc_check_tests;
mod drc_deposit_preauth;
mod drc_escrow;
#[cfg(test)]
mod drc_escrow_auth_cutoff_tranche_tests;
#[cfg(test)]
mod drc_escrow_hardening_multisign_tests;
#[cfg(test)]
mod drc_escrow_hardening_tests;
#[cfg(test)]
mod drc_escrow_multisign_adversary_matrix;
#[cfg(test)]
mod drc_escrow_terminal_tests;
#[cfg(test)]
mod drc_escrow_test_harness;
#[cfg(test)]
mod drc_escrow_tests;
mod drc_issued_controls;
#[cfg(test)]
mod drc_issued_controls_hardening_tests;
#[cfg(test)]
mod drc_issued_controls_master_auth_matrix_tests;
#[cfg(test)]
mod drc_issued_controls_matrix_tests;
#[cfg(test)]
mod drc_issued_controls_multisign_adversary_matrix;
#[cfg(test)]
mod drc_issued_controls_policy_transition_tests;
#[cfg(test)]
mod drc_issued_controls_require_auth_migration_tests;
#[cfg(test)]
mod drc_issued_controls_sequence_auth_matrix_tests;
#[cfg(test)]
mod drc_issued_controls_test_harness;
#[cfg(test)]
mod drc_master_key_disable_hardening_tests;
#[cfg(test)]
mod drc_master_key_disable_tests;
mod drc_master_key_recovery;
mod drc_mempool;
mod drc_multisig_tests;
#[cfg(test)]
mod drc_multisign_attachment_tests;
mod drc_payment_channel;
#[cfg(test)]
mod drc_payment_channel_auth_cutoff_tranche_tests;
#[cfg(test)]
mod drc_payment_channel_hardening_tests;
#[cfg(test)]
mod drc_payment_channel_multisign_adversary_matrix;
#[cfg(test)]
mod drc_payment_channel_sequence_auth_matrix_tests;
#[cfg(test)]
mod drc_payment_channel_terminal_tests;
#[cfg(test)]
mod drc_payment_channel_test_harness;
#[cfg(test)]
mod drc_payment_channel_tests;
mod drc_policy;
mod drc_regular_key;
#[cfg(test)]
mod drc_regular_key_tests;
mod drc_signer_list;
mod drc_ticket;
#[cfg(test)]
mod drc_ticket_tests;
#[cfg(test)]
mod drc_tickets_stage_a_tests;
#[cfg(test)]
mod drc_tickets_stage_b_tests;
#[cfg(test)]
mod drc_tickets_stage_c_tests;
#[cfg(test)]
mod drc_tickets_stage_d_multisign_tests;
#[cfg(test)]
mod drc_tickets_stage_d_selector_tests;
#[cfg(test)]
mod drc_tickets_stage_d_semantic_tests;
#[cfg(test)]
mod drc_tickets_test_harness;
mod drc_trust_line;
#[cfg(test)]
mod drc_trust_line_hardening_tests;
#[cfg(test)]
mod drc_trust_line_liability_matrix_tests;
#[cfg(test)]
mod drc_trust_line_master_auth_matrix_tests;
#[cfg(test)]
mod drc_trust_line_multisign_adversary_matrix;
#[cfg(test)]
mod drc_trust_line_policy_matrix_tests;
#[cfg(test)]
mod drc_trust_line_public_core_parity_tests;
#[cfg(test)]
mod drc_trust_line_semantics_matrix_tests;
#[cfg(test)]
mod drc_trust_line_sequence_auth_matrix_tests;
#[cfg(test)]
mod drc_trust_line_terminal_tests;
#[cfg(test)]
mod drc_trust_line_test_harness;
#[cfg(test)]
mod drc_trust_line_tests;
mod error;
mod execution;
mod finality_store;
mod genesis;
mod ghostdag_store;
mod governance_state;
mod headers;
mod marks;
mod monetary;
mod network;
mod orphans;
mod payments;
mod staking;
mod state_root;
mod store;
mod supply;
mod trident_genesis;
mod tx_index;
mod utxo;
mod utxo_diff;
mod zones;

pub use acceptance::{
    acceptance_key, delete_acceptance_into, load_acceptance, put_acceptance_into, store_acceptance,
    tx_acceptance_status, BlockAcceptanceRecord,
};
pub use accounts::{
    account_exists, account_key, account_root, apply_account_transfer, credit_account_into,
    genesis_credit, load_account, put_account_into, revert_account_journal_into, AccountJournal,
    AccountState,
};
pub use apply::{
    apply_block, apply_block_batched, apply_block_batched_virtual,
    apply_block_batched_virtual_at_blue_score, apply_block_batched_with_auth,
    apply_block_batched_with_auth_at_blue_score, apply_block_with_auth, balance_of, revert_journal,
    revert_journal_batched, sum_transfer_fees, transfer_fee, validate_mempool_tx,
    validate_mempool_tx_with_auth, ApplyMode, BlockApplyResult, TxAuthContext, UtxoJournal,
};
pub use block_zero::{
    ensure_legacy_v2_datadir, load_verified_trident_block_zero, verify_trident_datadir_identity,
    BlockZeroAllocation, BlockZeroFinality, BlockZeroSupply, BlockZeroTreasury, BlockZeroValidator,
    BlockZeroValidatorSet, BlockZeroVesting, TridentBlockZeroCommitment, TridentBlockZeroState,
    TridentBlockZeroStorageRecord, TridentDatadirHeaderIdentity, TridentDatadirIdentity,
    TRIDENT_BLOCK_ZERO_COMMITMENT_DOMAIN, TRIDENT_BLOCK_ZERO_STATE_DOMAIN,
    TRIDENT_BLOCK_ZERO_STATE_VERSION, TRIDENT_BLOCK_ZERO_STORAGE_VERSION,
    TRIDENT_DATADIR_IDENTITY_VERSION,
};
pub use columns::{meta_keys, ColumnFamily, SCHEMA_VERSION};
pub use community_state::{
    canonical_community_root, init_canonical_community_into, list_grants, list_hubs, list_missions,
    list_passport_attestations, load_canonical_community_summary, register_grant_into,
    register_hub_into, register_mission_into, register_passport_attestation_into,
    CanonicalCommunitySummary, CANONICAL_COMMUNITY_VERSION,
};
pub use data_availability::{
    apply_data_commitment, data_availability_root, data_commitment_key, data_commitment_nonce_key,
    load_data_commitment, load_data_commitment_nonce, revert_data_commitment_meta_into,
    AcceptedDataCommitment, ACCEPTED_DATA_COMMITMENT_VERSION, DATA_AVAILABILITY_ROOT_DOMAIN,
};
pub use drc_check::{
    apply_drc_check_cancel, apply_drc_check_cash, apply_drc_check_create, drc_check_root,
    load_drc_check_live, load_drc_check_receipt, lookup_drc_check_point,
};
pub use drc_deposit_preauth::{
    apply_drc_deposit_preauth, drc_deposit_preauth_key, drc_deposit_preauth_meta_keys,
    drc_deposit_preauth_root, load_drc_deposit_preauth, load_known_drc_deposit_authorization,
    DrcDepositAuthorization, DRC_DEPOSIT_PREAUTH_ROOT_DOMAIN,
};
pub use drc_escrow::{
    apply_drc_escrow_cancel, apply_drc_escrow_create, apply_drc_escrow_finish, drc_escrow_root,
    load_drc_escrow_live, load_drc_escrow_receipt, lookup_drc_escrow_point,
};
pub use drc_issued_controls::{
    apply_drc_issued_asset_policy_set, apply_drc_issued_clawback,
    apply_drc_trust_line_issuer_control, issued_movement_allowed, load_drc_issued_asset_policy,
    load_drc_issued_asset_policy_receipt, load_drc_issued_clawback_receipt,
    load_drc_trust_line_issuer_control_receipt, normalize_trust_line_live, IssuedMovementKind,
    DRC_ISSUED_CONTROLS_ROOT_DOMAIN,
};
pub use drc_mempool::{
    drc_ticket_sequence_for_create_nonce, lookup_drc_ticket_point, plan_drc_mempool_reservation,
    DrcMempoolReservation, DrcTicketPointStatus,
};
pub use drc_payment_channel::{
    apply_drc_payment_channel_claim, apply_drc_payment_channel_close,
    apply_drc_payment_channel_create, apply_drc_payment_channel_fund, drc_payment_channel_root,
    load_drc_payment_channel_claim_event, load_drc_payment_channel_fund_event,
    load_drc_payment_channel_live, load_drc_payment_channel_receipt,
    load_drc_payment_channel_schedule_event, lookup_drc_payment_channel_point,
    payment_channel_meta_keys_for_claim, payment_channel_meta_keys_for_create,
    payment_channel_meta_keys_for_fund, payment_channel_meta_keys_for_mutating,
    payment_channel_meta_keys_for_schedule_close,
};
pub use drc_policy::{
    apply_drc_account_policy, drc_account_policy_key, drc_account_policy_meta_keys,
    drc_account_policy_root, load_drc_account_policy, load_known_drc_account_policy,
    DRC_ACCOUNT_POLICY_ROOT_DOMAIN,
};
pub use drc_regular_key::{
    apply_drc_regular_key, drc_regular_key_meta_key, drc_regular_key_meta_keys,
    drc_regular_key_root, load_drc_account_regular_key, load_known_drc_account_keys,
    DRC_REGULAR_KEY_ROOT_DOMAIN,
};
pub use drc_signer_list::{
    apply_drc_signer_list, drc_signer_list_meta_key, drc_signer_list_meta_keys,
    drc_signer_list_root, load_drc_account_signer_list, load_known_drc_account_signer_summary,
    DRC_SIGNER_LIST_ROOT_DOMAIN,
};
pub use drc_ticket::{apply_drc_ticket_create, load_drc_account_tickets};
pub use drc_trust_line::{
    apply_drc_issued_transfer, apply_drc_trust_line_set, count_live_trust_line_holders_for_issuer,
    count_live_trust_lines_for_holder, drc_trust_line_root, issued_transfer_receipt_key,
    issuer_liability_key, load_drc_issued_transfer_receipt, load_drc_issuer_liability,
    load_drc_trust_line_live, lookup_drc_issuer_liability_point, lookup_drc_trust_line_point,
    require_auth_migration_meta_keys, sum_holder_balances_for_asset, trust_line_key,
    trust_line_meta_keys, DRC_TRUST_LINE_ROOT_DOMAIN,
};
pub use error::StateError;
pub use execution::{
    apply_ovl_execution, execution_fee, OvlExecutionReceipt, OVL_EXECUTION_VERSION,
    OVL_INTRINSIC_GAS,
};
pub use finality_store::{
    certificate_key, load_attestation_index, load_certificate, load_finalized_blue_score,
    load_last_attestation, put_attestation_index_into, put_certificate_into,
    put_last_attestation_into, AttestationIndex,
};
pub use genesis::{GenesisBuilder, SupplyCaps};
pub use ghostdag_store::{
    ghostdag_key, load_ghostdag_record, store_ghostdag_record, GhostdagRecord,
};
pub use governance_state::{
    authorization_policy_root, governance_treasury_root, init_canonical_governance_into,
    load_canonical_governance_policy, load_protocol_treasuries, load_protocol_treasury,
    CanonicalGovernancePolicy, CANONICAL_GOVERNANCE_VERSION,
};
pub use headers::{header_key, load_header, store_header, store_header_into};
pub use marks::{default_token_marks, TokenMark};
pub use monetary::{
    issued_within_cap, AssetMonetaryPolicy, EmissionKind, TridentMonetaryPolicy,
    DRC_MAX_SUPPLY_BASE, DRC_WORKING_RESERVE_BASE, OVL_MAX_SUPPLY_BASE, OVL_WORKING_RESERVE_BASE,
    TLT_MAX_SUPPLY_BASE, WORKING_EPOCH_RESERVE_DRIP,
};
pub use network::{
    daa_config_mainnet, daa_config_testnet, ChainParams, GenesisArtifact, GenesisConsensusPolicy,
    GenesisWalletPolicy, NetworkId, DEFAULT_GHOSTDAG_K, TESTNET_GENESIS_BITS,
    TESTNET_GENESIS_HASH_HEX, TESTNET_GENESIS_TIMESTAMP_MS, TESTNET_PREMINE_ADDRESS_HEX,
};
pub use orphans::{delete_orphan, list_orphans, load_orphan, orphan_key, store_orphan};
pub use payments::{
    apply_drc_payment, apply_drc_payment_at_blue_score, drc_payment_root, list_drc_outbox,
    load_drc_outbox_event, load_drc_payment_by_invoice, load_drc_payment_receipt,
    payment_invoice_key, payment_meta_keys, payment_outbox_key, payment_receipt_key,
    payment_seen_key, DRC_PAYMENT_DESTINATION_TAG_VERSION, DRC_PAYMENT_LEGACY_VERSION,
    DRC_PAYMENT_SOURCE_TAG_VERSION, DRC_PAYMENT_VERSION,
};
pub use staking::{
    advance_epoch, advance_epoch_with_params, apply_evidence, apply_signed_stake_tx,
    begin_unbond_self, bond_validator, build_snapshot, credit_fee_share_to_reward_pool,
    credit_reward_pool_into, delegate, distribute_reward_pool, distribute_reward_pool_amount,
    drip_staking_reserve, init_staking_reserve_into, load_epoch, load_reward_pool, load_snapshot,
    load_staking_reserve_remaining, load_validator, put_epoch_into, put_reward_pool_into,
    put_staking_reserve_remaining_into, put_validator_into, reward_pool_meta_key, signed_stake_for,
    snapshot_meta_keys, stake_meta_keys_touched, validator_key_matches, validator_meta_key,
    withdraw_unbonded, DelegationRecord, StakingParams, UnbondingEntry, ValidatorRecord,
    ValidatorSetSnapshot, ValidatorStatus, MAX_VALIDATOR_COMMISSION_BPS,
};
pub use state_root::{
    acceptance_root, compose_trident_state_root, finalized_tip_commitment, utxo_commitment,
    STATE_ROOT_DOMAIN,
};
pub use store::{StateStore, WriteBatch};
pub use supply::{
    ignite_trident_supply, issued_supply_key, load_issued_supply, load_max_supply,
    load_schema_version, max_supply_key, put_issued_supply_into, put_max_supply_into,
    put_schema_version_into, verify_supply_invariants,
};
pub use trident_genesis::{
    TridentFinalityPolicy, TridentGenesisArtifact, TridentGenesisValidator,
    TridentRuntimeFinalityPolicy, TridentRuntimePolicy, TridentValidatorGenesis,
    TRIDENT_CONSENSUS_POLICY_DOMAIN, TRIDENT_CONSENSUS_POLICY_VERSION, TRIDENT_GENESIS_SCHEMA,
    TRIDENT_NET_FP_DOMAIN, TRIDENT_PROTOCOL_VERSION, TRIDENT_STATE_TRANSITION_VERSION,
    TRIDENT_TX_SIGNING_VERSION,
};
pub use tx_index::{
    decode_tx_location, encode_tx_location, index_block_transactions,
    index_block_transactions_into, list_tx_inclusions, lookup_tx_location, set_primary_tx_location,
    tx_inclusion_key, tx_index_key,
};
pub use utxo::{outpoint_key, outpoint_key_parts};
pub use utxo_diff::{delete_utxo_journal, load_utxo_journal, store_utxo_journal, utxo_diff_key};
pub use zones::StateZone;
