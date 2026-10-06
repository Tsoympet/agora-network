//! Selected-parent header spine checks for the Agora light client.
//!
//! These checks recompute header hashes and require each reported selected
//! parent to be a real parent link with a strictly higher blue score toward
//! the tip. They do not recompute GHOSTDAG, RandomX, or validator signatures.
//! A spine that does not end at the caller's expected genesis fails closed.

use agora_types::{BlockHeader, Hash};

/// One header plus the GHOSTDAG selected-parent metadata a full node reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportedLightHeader {
    pub header: BlockHeader,
    pub selected_parent: Option<Hash>,
    pub blue_score: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightHeaderError {
    EmptyChain,
    DuplicateHeader,
    BrokenAncestry,
    SelectedParentNotInParents,
    GenesisMismatch,
    BlueScoreNotIncreasing,
    AnchorHasSelectedParent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpineRelation {
    Same,
    Extended,
    Reorg,
    Mismatch,
}

/// Verify a tip-to-genesis selected-parent spine.
///
/// `tip_to_genesis[0]` is the newest header. The last header must hash to
/// `expected_genesis` and must not itself have a selected parent.
pub fn verify_light_header_spine(
    tip_to_genesis: &[ReportedLightHeader],
    expected_genesis: &Hash,
) -> Result<Hash, LightHeaderError> {
    if tip_to_genesis.is_empty() {
        return Err(LightHeaderError::EmptyChain);
    }
    let mut seen = Vec::with_capacity(tip_to_genesis.len());
    for (index, record) in tip_to_genesis.iter().enumerate() {
        let hash = record.header.hash();
        if seen.contains(&hash) {
            return Err(LightHeaderError::DuplicateHeader);
        }
        seen.push(hash);
        let is_anchor = index + 1 == tip_to_genesis.len();
        if is_anchor {
            if hash != *expected_genesis {
                return Err(LightHeaderError::GenesisMismatch);
            }
            if record.selected_parent.is_some() {
                return Err(LightHeaderError::AnchorHasSelectedParent);
            }
            continue;
        }
        let parent_hash = tip_to_genesis[index + 1].header.hash();
        if record.selected_parent != Some(parent_hash) {
            return Err(LightHeaderError::BrokenAncestry);
        }
        if !record.header.parents.contains(&parent_hash) {
            return Err(LightHeaderError::SelectedParentNotInParents);
        }
        if record.blue_score <= tip_to_genesis[index + 1].blue_score {
            return Err(LightHeaderError::BlueScoreNotIncreasing);
        }
    }
    Ok(tip_to_genesis[0].header.hash())
}

/// Compare two verified spines. Same header hash with different selected-parent
/// metadata is a mismatch. A new tip that drops the previous tip is a reorg.
pub fn compare_light_spines(
    previous: &[ReportedLightHeader],
    next: &[ReportedLightHeader],
) -> SpineRelation {
    if previous.is_empty() {
        return if next.is_empty() {
            SpineRelation::Same
        } else {
            SpineRelation::Extended
        };
    }
    let mut next_by_hash: Vec<(Hash, &ReportedLightHeader)> = Vec::new();
    for record in next {
        let hash = record.header.hash();
        if let Some((_, prior)) = next_by_hash.iter().find(|(existing, _)| *existing == hash) {
            if !same_metadata(prior, record) {
                return SpineRelation::Mismatch;
            }
        }
        next_by_hash.push((hash, record));
    }
    for record in previous {
        let hash = record.header.hash();
        if let Some((_, other)) = next_by_hash.iter().find(|(existing, _)| *existing == hash) {
            if !same_metadata(record, other) {
                return SpineRelation::Mismatch;
            }
        }
    }
    let prev_tip = previous[0].header.hash();
    let next_tip = next.first().map(|record| record.header.hash());
    if next_tip == Some(prev_tip) {
        return SpineRelation::Same;
    }
    if next_by_hash.iter().any(|(hash, _)| *hash == prev_tip) {
        return SpineRelation::Extended;
    }
    SpineRelation::Reorg
}

fn same_metadata(left: &ReportedLightHeader, right: &ReportedLightHeader) -> bool {
    left.header == right.header
        && left.selected_parent == right.selected_parent
        && left.blue_score == right.blue_score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(parents: Vec<Hash>, nonce: u64) -> BlockHeader {
        BlockHeader {
            version: 1,
            parents,
            timestamp_ms: nonce,
            bits: 1,
            nonce,
            tx_root: Hash([u8::try_from(nonce).unwrap_or(1); 32]),
        }
    }

    fn spine() -> (Vec<ReportedLightHeader>, Hash) {
        let genesis = header(vec![], 1);
        let genesis_hash = genesis.hash();
        let child = header(vec![genesis_hash], 2);
        let tip = header(vec![child.hash()], 3);
        let records = vec![
            ReportedLightHeader {
                header: tip,
                selected_parent: Some(child.hash()),
                blue_score: 3,
            },
            ReportedLightHeader {
                header: child,
                selected_parent: Some(genesis_hash),
                blue_score: 2,
            },
            ReportedLightHeader {
                header: genesis,
                selected_parent: None,
                blue_score: 1,
            },
        ];
        (records, genesis_hash)
    }

    #[test]
    fn accepts_linked_spine_and_rejects_mismatch() {
        let (records, genesis) = spine();
        assert_eq!(
            verify_light_header_spine(&records, &genesis).unwrap(),
            records[0].header.hash()
        );

        let mut wrong_genesis = records.clone();
        assert_eq!(
            verify_light_header_spine(&wrong_genesis, &Hash([9u8; 32])),
            Err(LightHeaderError::GenesisMismatch)
        );

        wrong_genesis[0].selected_parent = Some(Hash([4u8; 32]));
        assert_eq!(
            verify_light_header_spine(&wrong_genesis, &genesis),
            Err(LightHeaderError::BrokenAncestry)
        );

        let mut flat = records.clone();
        flat[0].blue_score = 1;
        assert_eq!(
            verify_light_header_spine(&flat, &genesis),
            Err(LightHeaderError::BlueScoreNotIncreasing)
        );

        let mut orphan_parent = records.clone();
        orphan_parent[0].header.parents = vec![Hash([5u8; 32])];
        orphan_parent[0].selected_parent = Some(records[1].header.hash());
        assert_eq!(
            verify_light_header_spine(&orphan_parent, &genesis),
            Err(LightHeaderError::SelectedParentNotInParents)
        );
    }

    #[test]
    fn reorg_and_metadata_mismatch() {
        let (records, _) = spine();
        assert_eq!(
            compare_light_spines(&records, &records),
            SpineRelation::Same
        );
        let mut extended = records.clone();
        let parent = extended[0].header.hash();
        extended.insert(
            0,
            ReportedLightHeader {
                header: header(vec![parent], 4),
                selected_parent: Some(parent),
                blue_score: 4,
            },
        );
        assert_eq!(
            compare_light_spines(&records, &extended),
            SpineRelation::Extended
        );

        let mut reorg = records.clone();
        reorg[0].header.nonce = 99;
        reorg[0].header.parents = vec![records[1].header.hash()];
        reorg[0].selected_parent = Some(records[1].header.hash());
        assert_eq!(compare_light_spines(&records, &reorg), SpineRelation::Reorg);

        let mut mismatch = records.clone();
        mismatch[1].blue_score = 9;
        assert_eq!(
            compare_light_spines(&records, &mismatch),
            SpineRelation::Mismatch
        );
    }
}
