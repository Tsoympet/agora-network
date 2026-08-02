/// Logical storage zones for BlockDAG state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateZone {
    /// Recent tips and UTXOs required for sub-second validation.
    Hot,
    /// Pruned-but-queryable history for RPC / explorer.
    Warm,
    /// Long-term archival data, eligible for slower media.
    Archival,
}

impl StateZone {
    pub fn column_family(self) -> &'static str {
        match self {
            Self::Hot => "zone_hot",
            Self::Warm => "zone_warm",
            Self::Archival => "zone_archival",
        }
    }
}
