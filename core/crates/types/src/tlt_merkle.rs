//! Pairwise TLT transaction-id Merkle proofs.
//!
//! The leaf order and odd-node duplication match [`crate::Block::compute_tx_root`].
//! Multi-lane block headers commit a body root that wraps this Merkle root, so a
//! proof against `header.tx_root` is only a direct inclusion proof for UTXO-only blocks.

use crate::Hash;

/// Inclusion of one transaction id in a pairwise Merkle root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TltTxMerkleProof {
    pub index: u32,
    pub tx_id: Hash,
    pub siblings: Vec<Hash>,
}

/// Merkle root over transaction ids. Empty input is [`Hash::ZERO`].
pub fn tlt_tx_merkle_root(tx_ids: &[Hash]) -> Hash {
    if tx_ids.is_empty() {
        return Hash::ZERO;
    }
    let mut level = tx_ids.to_vec();
    while level.len() > 1 {
        if level.len() % 2 == 1 {
            level.push(*level.last().expect("non-empty merkle level"));
        }
        level = pair_level(&level);
    }
    level[0]
}

/// Build a proof that `tx_ids[index]` is in [`tlt_tx_merkle_root`].
pub fn prove_tlt_tx_merkle(tx_ids: &[Hash], index: usize) -> Option<TltTxMerkleProof> {
    if index >= tx_ids.len() {
        return None;
    }
    let tx_id = tx_ids[index];
    let mut idx = index;
    let mut level = tx_ids.to_vec();
    let mut siblings = Vec::new();
    while level.len() > 1 {
        if level.len() % 2 == 1 {
            level.push(*level.last().expect("non-empty merkle level"));
        }
        let sibling_idx = idx ^ 1;
        siblings.push(level[sibling_idx]);
        level = pair_level(&level);
        idx /= 2;
    }
    Some(TltTxMerkleProof {
        index: u32::try_from(index).ok()?,
        tx_id,
        siblings,
    })
}

/// Recompute the root from a proof.
pub fn verify_tlt_tx_merkle(root: &Hash, proof: &TltTxMerkleProof) -> bool {
    let mut hash = proof.tx_id;
    let mut idx = proof.index as usize;
    for sibling in &proof.siblings {
        let (left, right) = if idx.is_multiple_of(2) {
            (hash, *sibling)
        } else {
            (*sibling, hash)
        };
        hash = hash_pair(&left, &right);
        idx /= 2;
    }
    hash == *root
}

fn pair_level(level: &[Hash]) -> Vec<Hash> {
    level
        .chunks(2)
        .map(|pair| hash_pair(&pair[0], &pair[1]))
        .collect()
}

fn hash_pair(left: &Hash, right: &Hash) -> Hash {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(left.as_bytes());
    buf[32..].copy_from_slice(right.as_bytes());
    Hash::hash_bytes(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Block;

    fn ids(n: usize) -> Vec<Hash> {
        (0..n)
            .map(|i| Hash([u8::try_from(i + 1).unwrap_or(1); 32]))
            .collect()
    }

    #[test]
    fn root_matches_block_tx_root_and_proofs_roundtrip() {
        for n in 0..8 {
            let leaves = ids(n);
            let root = tlt_tx_merkle_root(&leaves);
            if n == 0 {
                assert_eq!(root, Hash::ZERO);
                continue;
            }
            for index in 0..n {
                let proof = prove_tlt_tx_merkle(&leaves, index).unwrap();
                assert!(verify_tlt_tx_merkle(&root, &proof));
            }
            let txs: Vec<crate::Transaction> = (0..n)
                .map(|i| crate::Transaction::unsigned(1, vec![], vec![], i as u64))
                .collect();
            let tx_ids: Vec<Hash> = txs.iter().map(crate::Transaction::tx_id).collect();
            assert_eq!(tlt_tx_merkle_root(&tx_ids), Block::compute_tx_root(&txs));
            let proof = prove_tlt_tx_merkle(&tx_ids, 0).unwrap();
            let mut tampered = proof.clone();
            tampered.tx_id.0[0] ^= 0xff;
            assert!(!verify_tlt_tx_merkle(
                &tlt_tx_merkle_root(&tx_ids),
                &tampered
            ));
        }
    }

    #[test]
    fn odd_leaf_duplicates_the_last_node() {
        let leaves = ids(3);
        let root = tlt_tx_merkle_root(&leaves);
        let proof = prove_tlt_tx_merkle(&leaves, 2).unwrap();
        assert_eq!(proof.siblings[0], leaves[2]);
        assert!(verify_tlt_tx_merkle(&root, &proof));
    }
}
