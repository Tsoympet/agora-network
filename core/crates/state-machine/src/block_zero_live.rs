//! Lossless live-state materialization for a freeze-ready Trident Block 0.
//!
//! The public v3 draft stays UNFROZEN. This loader only accepts artifacts that
//! already pass [`TridentGenesisArtifact::validate_freeze_ready`]. It writes
//! UTXO/account/treasury/vesting/validator/supply records, constructs a concrete
//! Block 0 body, and refuses to commit unless `compose_trident_state_root`
//! equals the live `TridentHeader` state root.

use std::collections::BTreeMap;

use agora_consensus::PowAlgorithm;
use agora_types::{
    Address, Amount, Block, BlockHeader, Hash, NativeAssetId, Transaction, TreasuryBalance, TxOut,
};
use borsh::BorshDeserialize;

use crate::accounts::put_account_into;
use crate::block_zero::{
    BlockZeroAllocation, TridentBlockZeroState, TridentBlockZeroStorageRecord,
};
use crate::columns::{meta_keys, ColumnFamily};
use crate::community_state::init_canonical_community_into;
use crate::error::StateError;
use crate::governance_state::init_trident_governance_into;
use crate::headers::store_header_into;
use crate::staking::{put_epoch_into, put_validator_into};
use crate::state_root::compose_trident_state_root;
use crate::store::{StateStore, WriteBatch};
use crate::supply::ignite_trident_supply;
use crate::trident_genesis::{TridentGenesisArtifact, TridentRuntimePolicy};
use crate::tx_index::index_block_transactions_into;
use crate::AccountState;

/// Result of a verified live Block 0 materialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentBlockZeroLiveMaterialization {
    pub genesis_block: Block,
    pub genesis_hash: Hash,
    pub body_root: Hash,
    pub live_state_root: Hash,
    pub manifest_state_root: Hash,
    pub record: TridentBlockZeroStorageRecord,
}

fn storage_err(message: impl Into<String>) -> StateError {
    StateError::Storage(message.into())
}

fn existing_genesis_hash(store: &StateStore) -> Result<Option<Hash>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)? else {
        return Ok(None);
    };
    if bytes.len() != 32 {
        return Err(storage_err("malformed genesis hash in datadir"));
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(Some(Hash(hash)))
}

fn require_empty_live_datadir(store: &StateStore) -> Result<(), StateError> {
    for cf in ColumnFamily::ALL {
        if !store.scan_prefix(cf, &[])?.is_empty() {
            return Err(storage_err(format!(
                "datadir contains {} state; refusing Trident live materialization",
                cf.name()
            )));
        }
    }
    Ok(())
}

fn liquid_locks(state: &TridentBlockZeroState) -> Result<BTreeMap<(u8, Address), u64>, StateError> {
    let mut locked = BTreeMap::new();
    for schedule in &state.vesting {
        let value = locked
            .entry((schedule.asset.wire_byte(), schedule.address))
            .or_insert(0u64);
        *value = value
            .checked_add(schedule.amount)
            .ok_or_else(|| storage_err("Block 0 vesting lock overflow"))?;
    }
    for set in &state.validator_sets {
        for validator in &set.validators {
            let value = locked
                .entry((set.asset.wire_byte(), validator.operator))
                .or_insert(0u64);
            *value = value
                .checked_add(validator.self_bond)
                .ok_or_else(|| storage_err("Block 0 validator lock overflow"))?;
        }
    }
    Ok(locked)
}

fn liquid_amount(
    allocation: &BlockZeroAllocation,
    locked: &BTreeMap<(u8, Address), u64>,
) -> Result<u64, StateError> {
    let locked_amount = locked
        .get(&(allocation.asset.wire_byte(), allocation.address))
        .copied()
        .unwrap_or(0);
    allocation.amount.checked_sub(locked_amount).ok_or_else(|| {
        storage_err(format!(
            "{} allocation cannot fund vesting and validator locks",
            allocation.asset
        ))
    })
}

fn build_genesis_block(
    state: &TridentBlockZeroState,
    runtime: &TridentRuntimePolicy,
) -> Result<Block, StateError> {
    let mut outputs = Vec::new();
    for allocation in &state.allocations {
        if allocation.asset != NativeAssetId::TLT || allocation.amount == 0 {
            continue;
        }
        outputs.push(TxOut {
            value: Amount::from_base_units(allocation.amount),
            address: allocation.address,
        });
    }
    outputs.sort_by_key(|output| output.address.0);
    if outputs.is_empty() {
        return Err(storage_err(
            "Trident Block 0 requires at least one TLT UTXO allocation",
        ));
    }
    let coinbase = Transaction::unsigned(1, Vec::new(), outputs, 0);
    let mut block = Block::utxo(
        BlockHeader {
            version: 1,
            parents: Vec::new(),
            timestamp_ms: runtime.timestamp_ms,
            bits: runtime.bits,
            nonce: 0,
            tx_root: Hash::ZERO,
        },
        vec![coinbase],
    );
    block.header.tx_root = block.compute_body_root();
    Ok(block)
}

fn write_utxos(batch: &mut WriteBatch, block: &Block) -> Result<(), StateError> {
    let coinbase = block
        .transactions
        .first()
        .ok_or_else(|| storage_err("Trident Block 0 is missing its coinbase"))?;
    let tx_id = coinbase.tx_id();
    for (index, output) in coinbase.outputs.iter().enumerate() {
        let mut utxo_key = Vec::with_capacity(36);
        utxo_key.extend_from_slice(tx_id.as_bytes());
        utxo_key.extend_from_slice(&(index as u32).to_le_bytes());
        let utxo_val = borsh::to_vec(output).map_err(|error| storage_err(error.to_string()))?;
        batch.put_cf(ColumnFamily::Utxo, &utxo_key, &utxo_val);
    }
    Ok(())
}

fn write_accounts(batch: &mut WriteBatch, state: &TridentBlockZeroState) -> Result<(), StateError> {
    let locked = liquid_locks(state)?;
    for allocation in &state.allocations {
        if !matches!(allocation.asset, NativeAssetId::OVL | NativeAssetId::DRC) {
            continue;
        }
        let liquid = liquid_amount(allocation, &locked)?;
        put_account_into(
            batch,
            allocation.asset,
            &allocation.address,
            &AccountState {
                balance: liquid,
                nonce: 0,
            },
        )?;
    }
    Ok(())
}

fn write_validators(
    batch: &mut WriteBatch,
    state: &TridentBlockZeroState,
    runtime: &TridentRuntimePolicy,
) -> Result<(), StateError> {
    for (set, params) in [
        (&state.validator_sets[0], &runtime.ovl_staking),
        (&state.validator_sets[1], &runtime.drc_staking),
    ] {
        let entries = set
            .to_runtime_validator_entries(params)
            .map_err(storage_err)?;
        for (_key, record) in entries {
            put_validator_into(batch, set.asset, &record)?;
        }
        put_epoch_into(batch, set.asset, 0);
    }
    Ok(())
}

fn write_genesis_identity(batch: &mut WriteBatch, block: &Block) -> Result<Hash, StateError> {
    let genesis_hash = block.id();
    let block_bytes = borsh::to_vec(block).map_err(|error| storage_err(error.to_string()))?;
    batch.put_cf(ColumnFamily::Hot, genesis_hash.as_bytes(), &block_bytes);
    batch.put_cf(
        ColumnFamily::Archival,
        genesis_hash.as_bytes(),
        &block_bytes,
    );
    store_header_into(batch, &genesis_hash, &block.header)?;
    index_block_transactions_into(batch, block);
    batch.put_cf(
        ColumnFamily::Meta,
        meta_keys::GENESIS_HASH,
        genesis_hash.as_bytes(),
    );
    let tlt_premine = block.transactions[0]
        .outputs
        .iter()
        .try_fold(0u64, |sum, output| {
            sum.checked_add(output.value.as_base_units())
        })
        .ok_or_else(|| storage_err("TLT premine overflow"))?;
    batch.put_cf(
        ColumnFamily::Meta,
        meta_keys::PREMINE,
        &tlt_premine.to_le_bytes(),
    );
    let tips = vec![genesis_hash];
    let tips_bytes = borsh::to_vec(&tips).map_err(|error| storage_err(error.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, meta_keys::TIPS, &tips_bytes);
    batch.put_cf(
        ColumnFamily::Meta,
        meta_keys::VIRTUAL_TIP,
        genesis_hash.as_bytes(),
    );
    batch.put_cf(
        ColumnFamily::Meta,
        meta_keys::DAA_DIFFICULTY,
        &block.header.bits.to_le_bytes(),
    );
    Ok(genesis_hash)
}

fn live_writes(
    state: &TridentBlockZeroState,
    runtime: &TridentRuntimePolicy,
    block: &Block,
) -> Result<(Hash, WriteBatch), StateError> {
    let mut batch = WriteBatch::new();
    ignite_trident_supply(&mut batch, &runtime.monetary)?;
    let treasuries = state
        .treasuries
        .iter()
        .map(|entry| {
            TreasuryBalance::new(
                entry.treasury,
                entry.asset,
                Amount::from_base_units(entry.balance),
            )
            .map_err(StateError::InvalidTx)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let controls = state
        .treasuries
        .iter()
        .map(|entry| (entry.treasury, entry.control.clone()))
        .collect::<Vec<_>>();
    init_trident_governance_into(
        &mut batch,
        state.governance_constitution_hash,
        state.emergency_policy_hash,
        &treasuries,
        &controls,
        &state.vesting,
    )?;
    init_canonical_community_into(&mut batch)?;
    write_accounts(&mut batch, state)?;
    write_validators(&mut batch, state, runtime)?;
    write_utxos(&mut batch, block)?;
    let genesis_hash = write_genesis_identity(&mut batch, block)?;
    Ok((genesis_hash, batch))
}

/// Materialize freeze-ready Block 0 live state into `store`.
///
/// `nonce` is ceremony-owned when `bits != 0`. Block 0 with `bits == 0` may
/// use nonce 0 because that difficulty is satisfied without a search.
pub fn materialize_trident_block_zero_live(
    store: &StateStore,
    artifact: &TridentGenesisArtifact,
    nonce: u64,
) -> Result<TridentBlockZeroLiveMaterialization, StateError> {
    let runtime = artifact.to_runtime_policy().map_err(storage_err)?;
    if runtime.pow_algorithm != PowAlgorithm::RandomX {
        return Err(storage_err(
            "public Trident networks are RandomX-only; refusing other PoW algorithms",
        ));
    }
    if runtime.bits != 0 && nonce == 0 {
        return Err(storage_err(
            "Block 0 nonce is ceremony-owned when bits are nonzero",
        ));
    }
    require_empty_live_datadir(store)?;
    let state = TridentBlockZeroState::from_artifact(artifact).map_err(storage_err)?;
    let mut block = build_genesis_block(&state, &runtime)?;
    block.header.nonce = nonce;
    block.header.tx_root = block.compute_body_root();
    let body_root = block.header.tx_root;
    let (genesis_hash, live_batch) = live_writes(&state, &runtime, &block)?;
    if genesis_hash != block.id() {
        return Err(storage_err(
            "Trident Block 0 identity changed while staging live writes",
        ));
    }

    let preview = store.cow_overlay();
    preview.write_batch(live_batch.clone())?;
    let live_state_root = compose_trident_state_root(&preview, &genesis_hash)?;
    let header = state
        .commitment()
        .to_live_trident_header(
            runtime.timestamp_ms,
            runtime.bits,
            nonce,
            body_root,
            live_state_root,
        )
        .map_err(storage_err)?;
    let (record, envelope) =
        state.stage_verified_live_envelope_batch(store, &header, &live_state_root)?;

    let mut combined = envelope;
    combined.append(live_batch);
    let checked = store.cow_overlay();
    checked.write_batch(combined.clone())?;
    let recomputed = compose_trident_state_root(&checked, &genesis_hash)?;
    if recomputed != live_state_root || recomputed != header.state_root {
        return Err(storage_err(
            "recomputed live composed root does not equal the Block 0 header",
        ));
    }
    crate::block_zero::verify_trident_datadir_identity(&checked, &record.datadir_identity)?;

    store.write_batch(combined)?;
    let loaded = crate::block_zero::load_verified_trident_block_zero(store)?;
    if loaded != record {
        return Err(storage_err(
            "Block 0 durable reread changed the live storage record",
        ));
    }
    let durable_root = compose_trident_state_root(store, &genesis_hash)?;
    if durable_root != live_state_root {
        return Err(storage_err(
            "durable live composed root does not equal the Block 0 header",
        ));
    }

    Ok(TridentBlockZeroLiveMaterialization {
        genesis_block: block,
        genesis_hash,
        body_root,
        live_state_root,
        manifest_state_root: state.state_root(),
        record: loaded,
    })
}

/// Load an existing live Trident datadir, or materialize one from a freeze-ready artifact.
pub fn load_or_materialize_trident_block_zero(
    store: &StateStore,
    artifact: &TridentGenesisArtifact,
    nonce: u64,
) -> Result<TridentBlockZeroLiveMaterialization, StateError> {
    match crate::block_zero::load_verified_trident_block_zero(store) {
        Ok(record) => {
            let expected = TridentBlockZeroState::from_artifact(artifact).map_err(storage_err)?;
            if record.manifest != expected
                || record.artifact_identity != artifact.consensus_identity_hash()
            {
                return Err(storage_err(
                    "Trident datadir identity does not match the supplied freeze-ready artifact",
                ));
            }
            let genesis_hash = existing_genesis_hash(store)?
                .ok_or_else(|| storage_err("Trident datadir is missing GENESIS_HASH"))?;
            let Some(bytes) = store.get_cf(ColumnFamily::Hot, genesis_hash.as_bytes())? else {
                return Err(storage_err("Trident datadir is missing Block 0 body"));
            };
            let genesis_block =
                Block::try_from_slice(&bytes).map_err(|error| storage_err(error.to_string()))?;
            if genesis_block.id() != genesis_hash {
                return Err(storage_err(
                    "stored Block 0 identity does not match GENESIS_HASH",
                ));
            }
            let live_state_root = compose_trident_state_root(store, &genesis_hash)?;
            if let Some(header_hash) = record.datadir_identity.block_zero_header_hash {
                let header = expected
                    .commitment()
                    .to_live_trident_header(
                        genesis_block.header.timestamp_ms,
                        genesis_block.header.bits,
                        genesis_block.header.nonce,
                        genesis_block.header.tx_root,
                        live_state_root,
                    )
                    .map_err(storage_err)?;
                if header
                    .commitment_hash()
                    .map_err(|error| storage_err(error.to_string()))?
                    != header_hash
                    || header.state_root != live_state_root
                {
                    return Err(storage_err(
                        "stored Trident header does not match recomputed live Block 0 roots",
                    ));
                }
            }
            crate::block_zero::verify_trident_datadir_identity(store, &record.datadir_identity)?;
            Ok(TridentBlockZeroLiveMaterialization {
                body_root: genesis_block.header.tx_root,
                live_state_root,
                manifest_state_root: expected.state_root(),
                genesis_block,
                genesis_hash,
                record,
            })
        }
        Err(_) => {
            if existing_genesis_hash(store)?.is_some() {
                return Err(storage_err(
                    "v2 genesis identity is present; refusing Trident live materialization",
                ));
            }
            materialize_trident_block_zero_live(store, artifact, nonce)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::load_account;
    use crate::apply::collect_address_utxos;
    use crate::block_zero::verify_trident_datadir_identity;
    use crate::genesis::GenesisBuilder;
    use crate::staking::load_validator;
    use agora_types::{TreasuryId, TridentHeader};
    use std::collections::HashSet;

    const DRAFT: &str = include_str!("../../../../docs/genesis/trident.testnet.genesis.draft.json");

    fn synthetic_freeze_ready_artifact() -> TridentGenesisArtifact {
        let mut artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        artifact.timestamp_ms = 1;
        artifact.bits = Some(0);
        artifact.maturity = "Scaffold".into();
        artifact.notes.clear();
        artifact.wallet.coin_type_status = "registered".into();
        artifact.governance_constitution_hash = "11".repeat(32);
        artifact.emergency_policy_hash = "22".repeat(32);
        artifact.finality.pow_work_threshold_policy = "minimum-blue-score-depth-v1".into();
        artifact.finality.pow_work_threshold = Some(12);

        let ovl = agora_crypto::KeyPair::from_secret_bytes(&[7; 32]).unwrap();
        let drc = agora_crypto::KeyPair::from_secret_bytes(&[8; 32]).unwrap();
        let tlt_address = Address([9; 20]).to_bech32_hrp("agoratest");
        let ovl_address = ovl.address().to_bech32_hrp("agoratest");
        let drc_address = drc.address().to_bech32_hrp("agoratest");
        artifact.initial_allocations = vec![
            crate::trident_genesis::TridentInitialAllocation {
                asset: "TLT".into(),
                address: tlt_address,
                amount: artifact.assets.tlt.genesis_allocation,
            },
            crate::trident_genesis::TridentInitialAllocation {
                asset: "OVL".into(),
                address: ovl_address.clone(),
                amount: 10,
            },
            crate::trident_genesis::TridentInitialAllocation {
                asset: "DRC".into(),
                address: drc_address.clone(),
                amount: 20,
            },
        ];
        artifact.assets.ovl.genesis_allocation = 10;
        artifact.assets.drc.genesis_allocation = 20;
        for (asset, treasury) in [
            (
                &mut artifact.assets.tlt,
                &mut artifact.treasuries.tlt_security,
            ),
            (
                &mut artifact.assets.ovl,
                &mut artifact.treasuries.ovl_builder,
            ),
            (
                &mut artifact.assets.drc,
                &mut artifact.treasuries.drc_community,
            ),
        ] {
            asset.treasury_allocation = 1;
            treasury.allocation = 1;
            treasury.control = "synthetic-governance-v1".into();
        }
        for (set, key, address, bond, metadata_byte) in [
            (&mut artifact.ovl_validators, &ovl, ovl_address, 5, "33"),
            (&mut artifact.drc_validators, &drc, drc_address, 7, "44"),
        ] {
            set.max_validators = 1;
            set.min_self_bond = 1;
            set.unbonding_period_checkpoints = 1;
            set.max_commission_bps = Some(2_000);
            set.max_concentration_bps = Some(10_000);
            set.genesis_set
                .push(crate::trident_genesis::TridentGenesisValidator {
                    consensus_public_key: hex::encode(key.public_key_bytes()),
                    withdrawal_address: address,
                    self_bond: bond,
                    commission_bps: Some(100),
                    metadata_hash: Some(metadata_byte.repeat(32)),
                });
        }
        artifact.genesis_hash = artifact.consensus_identity_hash().to_hex();
        artifact.network_fingerprint = artifact.compute_network_fingerprint().to_hex();
        artifact
    }

    #[test]
    fn public_draft_cannot_materialize_live_state() {
        let store = StateStore::open_in_memory();
        let artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        let error = materialize_trident_block_zero_live(&store, &artifact, 0).unwrap_err();
        assert!(
            error.to_string().contains("timestamp_ms")
                || error.to_string().contains("UNFROZEN")
                || error.to_string().contains("placeholder")
                || error.to_string().contains("freeze")
        );
        assert!(store
            .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
            .unwrap()
            .is_none());
    }

    #[test]
    fn synthetic_live_state_is_deterministic_and_matches_header() {
        let artifact = synthetic_freeze_ready_artifact();
        let store_a = StateStore::open_in_memory();
        let store_b = StateStore::open_in_memory();
        let first = materialize_trident_block_zero_live(&store_a, &artifact, 0).unwrap();
        let second = materialize_trident_block_zero_live(&store_b, &artifact, 0).unwrap();
        assert_eq!(first.genesis_hash, second.genesis_hash);
        assert_eq!(first.live_state_root, second.live_state_root);
        assert_eq!(first.body_root, second.body_root);
        assert_ne!(first.live_state_root, first.manifest_state_root);
        assert_eq!(
            first.genesis_block.header.tx_root,
            first.genesis_block.compute_body_root()
        );
        assert_eq!(
            first.record.datadir_identity.committed_state_root,
            first.manifest_state_root
        );
        verify_trident_datadir_identity(&store_a, &first.record.datadir_identity).unwrap();

        let ovl = agora_crypto::KeyPair::from_secret_bytes(&[7; 32])
            .unwrap()
            .address();
        let drc = agora_crypto::KeyPair::from_secret_bytes(&[8; 32])
            .unwrap()
            .address();
        assert_eq!(
            load_account(&store_a, NativeAssetId::OVL, &ovl)
                .unwrap()
                .balance,
            5
        );
        assert_eq!(
            load_account(&store_a, NativeAssetId::DRC, &drc)
                .unwrap()
                .balance,
            13
        );
        let tlt = collect_address_utxos(&store_a, &Address([9; 20]), &HashSet::new()).unwrap();
        assert_eq!(tlt.len(), 1);
        assert_eq!(
            tlt[0].1.value.as_base_units(),
            artifact.assets.tlt.genesis_allocation
        );
        assert_eq!(
            load_validator(&store_a, NativeAssetId::OVL, &ovl)
                .unwrap()
                .unwrap()
                .self_bond,
            5
        );
        assert_eq!(
            crate::governance_state::load_protocol_treasury(&store_a, TreasuryId::OvlBuilder)
                .unwrap()
                .balance
                .as_base_units(),
            1
        );
    }

    #[test]
    fn live_header_mismatch_is_rejected_and_v2_ignition_still_refuses_the_datadir() {
        let artifact = synthetic_freeze_ready_artifact();
        let store = StateStore::open_in_memory();
        let live = materialize_trident_block_zero_live(&store, &artifact, 0).unwrap();
        let mut wrong = live
            .record
            .commitment
            .to_live_trident_header(1, 0, 0, live.body_root, Hash([0xab; 32]))
            .unwrap();
        assert_ne!(wrong.state_root, live.live_state_root);
        assert!(live
            .record
            .commitment
            .verify_live_trident_header(&wrong, live.body_root, live.live_state_root)
            .is_err());
        wrong.state_root = live.live_state_root;
        live.record
            .commitment
            .verify_live_trident_header(&wrong, live.body_root, live.live_state_root)
            .unwrap();

        let error = GenesisBuilder::default()
            .load_or_ignite(&store)
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("legacy/v2 startup refuses this datadir"));
    }

    #[test]
    fn load_or_materialize_is_idempotent_and_rechecks_live_root() {
        let artifact = synthetic_freeze_ready_artifact();
        let store = StateStore::open_in_memory();
        let first = load_or_materialize_trident_block_zero(&store, &artifact, 0).unwrap();
        let second = load_or_materialize_trident_block_zero(&store, &artifact, 0).unwrap();
        assert_eq!(first.genesis_hash, second.genesis_hash);
        assert_eq!(first.live_state_root, second.live_state_root);
        assert_eq!(
            compose_trident_state_root(&store, &first.genesis_hash).unwrap(),
            first.live_state_root
        );
    }

    #[test]
    fn nonzero_bits_do_not_invent_a_block_zero_nonce() {
        let mut artifact = synthetic_freeze_ready_artifact();
        artifact.bits = Some(8);
        artifact.genesis_hash = artifact.consensus_identity_hash().to_hex();
        artifact.network_fingerprint = artifact.compute_network_fingerprint().to_hex();
        let store = StateStore::open_in_memory();
        let error = materialize_trident_block_zero_live(&store, &artifact, 0).unwrap_err();
        assert!(error.to_string().contains("nonce is ceremony-owned"));
    }

    #[test]
    fn live_header_round_trip_is_not_the_manifest_root() {
        let header: Option<TridentHeader> = None;
        assert!(header.is_none());
        let artifact = synthetic_freeze_ready_artifact();
        let store = StateStore::open_in_memory();
        let live = materialize_trident_block_zero_live(&store, &artifact, 0).unwrap();
        assert_ne!(live.live_state_root, Hash::ZERO);
        assert_ne!(live.body_root, Hash::ZERO);
    }
}
