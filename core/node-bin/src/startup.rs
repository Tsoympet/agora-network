//! Datadir preflight that must complete before any P2P or RPC side effect.

use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agora_state_machine::{ChainParams, StateStore};
use agora_types::Hash;

use crate::storage_policy::StoragePolicy;

pub(crate) struct PreparedLegacyDatadir<T> {
    pub store: Arc<StateStore>,
    pub genesis_hash: Hash,
    pub p2p_identity: T,
}

pub(crate) fn p2p_identity_path(data_dir: &Path) -> PathBuf {
    data_dir.join("p2p").join("identity.key")
}

/// Open and verify the legacy/v2 datadir before invoking the identity loader.
///
/// Keeping the callback inside this function makes the ordering testable: any
/// storage identity error returns before an existing libp2p key is read or a
/// new key and parent directory are created.
pub(crate) fn prepare_legacy_datadir<T, E>(
    data_dir: &Path,
    chain_params: &ChainParams,
    storage: StoragePolicy,
    identity_loader: impl FnOnce(&Path) -> Result<T, E>,
) -> Result<PreparedLegacyDatadir<T>, String>
where
    E: Display,
{
    let store =
        Arc::new(StateStore::open(data_dir).map_err(|error| format!("open state store: {error}"))?);
    let genesis_hash = chain_params
        .builder()
        .with_archival(storage.archival)
        .load_or_ignite_checked(store.as_ref(), chain_params.expected_genesis)
        .map_err(|error| format!("genesis preflight: {error}"))?;
    let identity_path = p2p_identity_path(data_dir);
    let p2p_identity = identity_loader(&identity_path)
        .map_err(|error| format!("load p2p identity after datadir preflight: {error}"))?;
    Ok(PreparedLegacyDatadir {
        store,
        genesis_hash,
        p2p_identity,
    })
}

#[cfg(all(test, feature = "rocksdb"))]
mod tests {
    use std::cell::Cell;

    use agora_state_machine::{
        meta_keys, ColumnFamily, TridentDatadirHeaderIdentity, TridentDatadirIdentity,
        TRIDENT_DATADIR_IDENTITY_VERSION, TRIDENT_PROTOCOL_VERSION,
        TRIDENT_STATE_TRANSITION_VERSION,
    };

    use super::*;

    fn temp_rocks_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agora-node-startup-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn sample_trident_identity() -> TridentDatadirIdentity {
        let chain_id = "agora-trident-testnet-synthetic";
        let artifact_identity = Hash([0x11; 32]);
        let consensus_policy_hash = Hash([0x22; 32]);
        let block_zero_commitment = Hash([0x33; 32]);
        let identity = TridentDatadirIdentity {
            version: TRIDENT_DATADIR_IDENTITY_VERSION,
            chain_id: chain_id.into(),
            network_fingerprint: agora_p2p::trident_network_fingerprint(
                chain_id,
                &artifact_identity,
                &consensus_policy_hash,
            ),
            artifact_identity,
            consensus_policy_hash,
            block_zero_commitment,
            committed_state_root: Hash([0x44; 32]),
            header_identity: TridentDatadirHeaderIdentity {
                protocol_version: TRIDENT_PROTOCOL_VERSION,
                state_transition_version: TRIDENT_STATE_TRANSITION_VERSION.into(),
                block_zero_commitment,
                artifact_identity,
                consensus_policy_hash,
            },
            block_zero_header_hash: None,
        };
        identity.verify().expect("synthetic identity");
        identity
    }

    #[test]
    fn trident_identity_refuses_legacy_startup_before_p2p_side_effects() {
        let dir = temp_rocks_dir("trident-refusal");
        let identity_path = p2p_identity_path(&dir);
        let identity = sample_trident_identity();
        let identity_bytes = identity.canonical_bytes().unwrap();
        {
            let store = StateStore::open(&dir).unwrap();
            store
                .put_cf(
                    ColumnFamily::Meta,
                    meta_keys::TRIDENT_DATADIR_IDENTITY_VERSION,
                    &identity.version.to_le_bytes(),
                )
                .unwrap();
            store
                .put_cf(
                    ColumnFamily::Meta,
                    meta_keys::TRIDENT_DATADIR_IDENTITY,
                    &identity_bytes,
                )
                .unwrap();
        }

        let identity_loader_called = Cell::new(false);
        let error = prepare_legacy_datadir(
            &dir,
            &ChainParams::dev(),
            StoragePolicy::default(),
            |path| -> Result<(), String> {
                identity_loader_called.set(true);
                std::fs::create_dir_all(path.parent().expect("identity parent"))
                    .map_err(|error| error.to_string())?;
                std::fs::write(path, b"must-not-exist").map_err(|error| error.to_string())
            },
        )
        .err()
        .expect("Trident datadir must be refused");

        assert!(error.contains("legacy/v2 startup refuses"));
        assert!(!identity_loader_called.get());
        assert!(!identity_path.exists());
        assert!(!dir.join("p2p").exists());
        {
            let store = StateStore::open(&dir).unwrap();
            assert!(store
                .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
                .unwrap()
                .is_none());
            assert_eq!(
                store
                    .get_cf(ColumnFamily::Meta, meta_keys::TRIDENT_DATADIR_IDENTITY)
                    .unwrap(),
                Some(identity_bytes)
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trident_live_state_marker_refuses_legacy_startup_before_p2p_side_effects() {
        let dir = temp_rocks_dir("trident-live-state-refusal");
        let identity_path = p2p_identity_path(&dir);
        {
            let store = StateStore::open(&dir).unwrap();
            store
                .put_cf(
                    ColumnFamily::Meta,
                    meta_keys::TRIDENT_LIVE_STATE_PLAN_VERSION,
                    &1u32.to_le_bytes(),
                )
                .unwrap();
        }

        let identity_loader_called = Cell::new(false);
        let error = prepare_legacy_datadir(
            &dir,
            &ChainParams::dev(),
            StoragePolicy::default(),
            |path| -> Result<(), String> {
                identity_loader_called.set(true);
                std::fs::create_dir_all(path.parent().expect("identity parent"))
                    .map_err(|error| error.to_string())?;
                std::fs::write(path, b"must-not-exist").map_err(|error| error.to_string())
            },
        )
        .err()
        .expect("Trident live state must be refused");

        assert!(error.contains("legacy/v2 startup refuses"));
        assert!(!identity_loader_called.get());
        assert!(!identity_path.exists());
        assert!(!dir.join("p2p").exists());
        {
            let store = StateStore::open(&dir).unwrap();
            assert_eq!(
                store
                    .get_cf(
                        ColumnFamily::Meta,
                        meta_keys::TRIDENT_LIVE_STATE_PLAN_VERSION
                    )
                    .unwrap(),
                Some(1u32.to_le_bytes().to_vec())
            );
            assert!(store
                .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
                .unwrap()
                .is_none());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
