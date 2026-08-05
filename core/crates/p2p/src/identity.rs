//! Persistent libp2p identity (ed25519) so peer IDs survive restarts.

use std::fs;
use std::path::Path;

use libp2p::identity::Keypair;

use crate::P2pError;

/// Load an ed25519 keypair from `path`, or generate and persist a new one.
pub fn load_or_create_identity(path: &Path) -> Result<Keypair, P2pError> {
    if path.exists() {
        let bytes = fs::read(path).map_err(|e| P2pError::Network(e.to_string()))?;
        return Keypair::from_protobuf_encoding(&bytes)
            .map_err(|e| P2pError::Network(format!("invalid identity file: {e}")));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| P2pError::Network(e.to_string()))?;
    }
    let keypair = Keypair::generate_ed25519();
    let bytes = keypair
        .to_protobuf_encoding()
        .map_err(|e| P2pError::Network(e.to_string()))?;
    fs::write(path, bytes).map_err(|e| P2pError::Network(e.to_string()))?;
    Ok(keypair)
}
