//! Persistent libp2p ed25519 identity under the node data directory.

use std::path::Path;

use libp2p::identity::Keypair;

use crate::P2pError;

/// Load a protobuf-encoded [`Keypair`] from `path`, or generate + save one.
///
/// File format is libp2p's `Keypair::to_protobuf_encoding` (type-tagged).
/// Parent directories are created as needed. Writes go through a `.tmp` rename.
pub fn load_or_generate_identity(path: impl AsRef<Path>) -> Result<Keypair, P2pError> {
    let path = path.as_ref();
    if path.exists() {
        let bytes = std::fs::read(path)
            .map_err(|e| P2pError::Identity(format!("read {}: {e}", path.display())))?;
        return Keypair::from_protobuf_encoding(&bytes)
            .map_err(|e| P2pError::Identity(format!("decode {}: {e}", path.display())));
    }

    let keypair = Keypair::generate_ed25519();
    save_identity(path, &keypair)?;
    Ok(keypair)
}

/// Persist `keypair` as protobuf bytes at `path` (atomic replace via `.tmp`).
pub fn save_identity(path: impl AsRef<Path>, keypair: &Keypair) -> Result<(), P2pError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| P2pError::Identity(format!("mkdir {}: {e}", parent.display())))?;
    }
    let bytes = keypair
        .to_protobuf_encoding()
        .map_err(|e| P2pError::Identity(format!("encode: {e}")))?;
    let tmp = path.with_extension("key.tmp");
    std::fs::write(&tmp, &bytes)
        .map_err(|e| P2pError::Identity(format!("write {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| P2pError::Identity(format!("rename {}: {e}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_identity_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "agora-p2p-identity-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn load_or_generate_persists_stable_peer_id() {
        let dir = temp_identity_dir("stable");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("p2p").join("identity.key");
        let first = load_or_generate_identity(&path).unwrap();
        let peer_a = first.public().to_peer_id();
        assert!(path.exists());

        let second = load_or_generate_identity(&path).unwrap();
        let peer_b = second.public().to_peer_id();
        assert_eq!(peer_a, peer_b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_existing_identity_is_never_overwritten() {
        let dir = temp_identity_dir("malformed");
        let path = dir.join("p2p").join("identity.key");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not-a-libp2p-key").unwrap();

        let error = match load_or_generate_identity(&path) {
            Ok(_) => panic!("malformed identity must be rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("decode"));
        assert_eq!(std::fs::read(&path).unwrap(), b"not-a-libp2p-key");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
