use agora_types::Hash;

use crate::ConsensusError;

/// Tunables for GHOSTDAG blue-set selection.
#[derive(Debug, Clone)]
pub struct GhostdagConfig {
    /// Maximum anticone size allowed inside the blue set (PHANTOM/GHOSTDAG `k`).
    pub k: u32,
}

impl Default for GhostdagConfig {
    fn default() -> Self {
        Self { k: 18 }
    }
}

/// Block after GHOSTDAG has assigned relative order / color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedBlock {
    pub hash: Hash,
    pub blue_score: u64,
    pub is_blue: bool,
}

/// GHOSTDAG engine placeholder.
///
/// Full recursive blue-set inheritance lands in Phase 2; this API locks the call surface.
#[derive(Debug, Default)]
pub struct Ghostdag {
    config: GhostdagConfig,
}

impl Ghostdag {
    pub fn new(config: GhostdagConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &GhostdagConfig {
        &self.config
    }

    /// Order a tip given already-validated parent hashes.
    ///
    /// Returns genesis-relative placeholders until the DAG store is wired in Phase 2.
    pub fn order_tip(&self, tip: Hash, parents: &[Hash]) -> Result<Vec<OrderedBlock>, ConsensusError> {
        let mut ordered = Vec::with_capacity(parents.len() + 1);
        for (i, parent) in parents.iter().enumerate() {
            ordered.push(OrderedBlock {
                hash: *parent,
                blue_score: i as u64,
                is_blue: true,
            });
        }
        ordered.push(OrderedBlock {
            hash: tip,
            blue_score: parents.len() as u64,
            is_blue: true,
        });
        Ok(ordered)
    }
}
