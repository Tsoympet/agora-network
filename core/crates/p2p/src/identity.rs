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

    #[test]
    fn load_or_generate_persists_stable_peer_id() {
        let dir = std::env::temp_dir().join(format!("agora-p2p-identity-{}", std::process::id()));
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
}
