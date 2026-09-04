//! Persistent state application for consensus-ordered blocks.
//!
//! Five column families (hot / warm / archival / meta / utxo) keep tip validation
//! off cold compaction paths while fixing genesis supply caps in `meta`.

mod acceptance;
mod accounts;
mod apply;
mod block_zero;
mod columns;
mod community_state;
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
    account_key, account_root, apply_account_transfer, credit_account_into, genesis_credit,
    load_account, put_account_into, revert_account_journal_into, AccountJournal, AccountState,
};
pub use apply::{
    apply_block, apply_block_batched, apply_block_batched_virtual, apply_block_batched_with_auth,
    apply_block_with_auth, balance_of, revert_journal, revert_journal_batched, sum_transfer_fees,
    transfer_fee, validate_mempool_tx, validate_mempool_tx_with_auth, ApplyMode, BlockApplyResult,
    TxAuthContext, UtxoJournal,
};
pub use block_zero::{
    BlockZeroAllocation, BlockZeroFinality, BlockZeroSupply, BlockZeroTreasury, BlockZeroValidator,
    BlockZeroValidatorSet, BlockZeroVesting, TridentBlockZeroCommitment, TridentBlockZeroState,
    TRIDENT_BLOCK_ZERO_COMMITMENT_DOMAIN, TRIDENT_BLOCK_ZERO_STATE_DOMAIN,
    TRIDENT_BLOCK_ZERO_STATE_VERSION,
};
pub use columns::{meta_keys, ColumnFamily, SCHEMA_VERSION};
pub use community_state::{
    canonical_community_root, init_canonical_community_into, list_grants, list_hubs, list_missions,
    list_passport_attestations, load_canonical_community_summary, register_grant_into,
    register_hub_into, register_mission_into, register_passport_attestation_into,
    CanonicalCommunitySummary, CANONICAL_COMMUNITY_VERSION,
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
    apply_drc_payment, drc_payment_root, list_drc_outbox, load_drc_outbox_event,
    payment_invoice_key, payment_meta_keys, payment_outbox_key, payment_seen_key,
    DrcPaymentReceipt, DRC_PAYMENT_VERSION,
};
pub use staking::{
    advance_epoch, advance_epoch_with_params, apply_evidence, apply_signed_stake_tx,
    begin_unbond_self, bond_validator, build_snapshot, credit_fee_share_to_reward_pool,
    credit_reward_pool_into, delegate, distribute_reward_pool, distribute_reward_pool_amount,
    drip_staking_reserve, init_staking_reserve_into, load_epoch, load_reward_pool, load_snapshot,
    load_staking_reserve_remaining, load_validator, put_epoch_into, put_reward_pool_into,
    put_staking_reserve_remaining_into, put_validator_into, reward_pool_meta_key, signed_stake_for,
    snapshot_meta_keys, stake_meta_keys_touched, validator_key_matches, withdraw_unbonded,
    DelegationRecord, StakingParams, UnbondingEntry, ValidatorRecord, ValidatorSetSnapshot,
    ValidatorStatus,
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
    TridentFinalityPolicy, TridentGenesisArtifact, TridentRuntimeFinalityPolicy,
    TridentRuntimePolicy, TridentValidatorGenesis, TRIDENT_CONSENSUS_POLICY_DOMAIN,
    TRIDENT_CONSENSUS_POLICY_VERSION, TRIDENT_GENESIS_SCHEMA, TRIDENT_NET_FP_DOMAIN,
    TRIDENT_PROTOCOL_VERSION, TRIDENT_STATE_TRANSITION_VERSION, TRIDENT_TX_SIGNING_VERSION,
};
pub use tx_index::{
    decode_tx_location, encode_tx_location, index_block_transactions,
    index_block_transactions_into, list_tx_inclusions, lookup_tx_location, set_primary_tx_location,
    tx_inclusion_key, tx_index_key,
};
pub use utxo::{outpoint_key, outpoint_key_parts};
pub use utxo_diff::{delete_utxo_journal, load_utxo_journal, store_utxo_journal, utxo_diff_key};
pub use zones::StateZone;
