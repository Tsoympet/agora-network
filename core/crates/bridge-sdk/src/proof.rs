use agora_types::Hash;
use sha2::{Digest, Sha256};

use crate::messages::BridgeMessage;
use crate::BridgeError;

/// Inclusion proof that a bridge message was committed in a district batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightClientProof {
    pub message_id: Hash,
    pub root: Hash,
    pub leaf_index: usize,
    pub siblings: Vec<Hash>,
}

/// Binary merkle tree over message IDs (duplicate last leaf when odd).
pub fn merkle_root(leaves: &[Hash]) -> Hash {
    if leaves.is_empty() {
        return Hash::ZERO;
    }
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        if !level.len().is_multiple_of(2) {
            level.push(*level.last().expect("non-empty"));
        }
        level = level
            .chunks(2)
            .map(|pair| hash_pair(&pair[0], &pair[1]))
            .collect();
    }
    level[0]
}

fn hash_pair(left: &Hash, right: &Hash) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(left.as_bytes());
    hasher.update(right.as_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Hash(out)
}

/// Build an inclusion proof for `leaf_index` within `leaves`.
pub fn prove_inclusion(
    leaves: &[Hash],
    leaf_index: usize,
) -> Result<LightClientProof, BridgeError> {
    if leaves.is_empty() || leaf_index >= leaves.len() {
        return Err(BridgeError::InvalidProof("leaf index out of range".into()));
    }
    let mut index = leaf_index;
    let mut level = leaves.to_vec();
    let mut siblings = Vec::new();
    while level.len() > 1 {
        if !level.len().is_multiple_of(2) {
            level.push(*level.last().expect("non-empty"));
        }
        let sibling_index = if index.is_multiple_of(2) {
            index + 1
        } else {
            index - 1
        };
        siblings.push(level[sibling_index]);
        index /= 2;
        level = level
            .chunks(2)
            .map(|pair| hash_pair(&pair[0], &pair[1]))
            .collect();
    }
    Ok(LightClientProof {
        message_id: leaves[leaf_index],
        root: level[0],
        leaf_index,
        siblings,
    })
}

/// Verify a light-client inclusion proof against an expected root.
pub fn verify_inclusion(proof: &LightClientProof, expected_root: &Hash) -> bool {
    if proof.root != *expected_root {
        return false;
    }
    let mut hash = proof.message_id;
    let mut index = proof.leaf_index;
    for sibling in &proof.siblings {
        hash = if index.is_multiple_of(2) {
            hash_pair(&hash, sibling)
        } else {
            hash_pair(sibling, &hash)
        };
        index /= 2;
    }
    hash == proof.root
}

/// Convenience: prove a message against an ordered message set.
pub fn prove_message(
    messages: &[BridgeMessage],
    message_id: Hash,
) -> Result<LightClientProof, BridgeError> {
    let leaves: Vec<Hash> = messages.iter().map(BridgeMessage::id).collect();
    let leaf_index = leaves
        .iter()
        .position(|h| *h == message_id)
        .ok_or_else(|| BridgeError::InvalidProof("message not in set".into()))?;
    prove_inclusion(&leaves, leaf_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{Address, Amount};

    use crate::messages::{BridgeDirection, BridgeMessage};

    fn msg(nonce: u64) -> BridgeMessage {
        BridgeMessage {
            direction: BridgeDirection::LockAndMint,
            source_district: "agora".into(),
            dest_district: "arena".into(),
            sender: Address([1u8; 20]),
            recipient: Address([2u8; 20]),
            amount: Amount::from_base_units(nonce + 1),
            nonce,
        }
    }

    #[test]
    fn merkle_proof_roundtrip() {
        let messages: Vec<_> = (0..5).map(msg).collect();
        let leaves: Vec<_> = messages.iter().map(BridgeMessage::id).collect();
        let root = merkle_root(&leaves);
        for i in 0..leaves.len() {
            let proof = prove_inclusion(&leaves, i).unwrap();
            assert!(verify_inclusion(&proof, &root));
        }
        let mut bad = prove_inclusion(&leaves, 0).unwrap();
        bad.siblings[0] = Hash([9u8; 32]);
        assert!(!verify_inclusion(&bad, &root));
    }
}
