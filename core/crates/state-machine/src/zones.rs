use crate::columns::ColumnFamily;

/// Logical storage zones for BlockDAG state (subset of column families).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateZone {
    /// Recent tips and headers required for sub-second validation.
    Hot,
    /// Pruned-but-queryable history for RPC / explorer.
    Warm,
    /// Long-term archival data, eligible for slower media.
    Archival,
}

impl StateZone {
    pub fn column_family(self) -> ColumnFamily {
        match self {
            Self::Hot => ColumnFamily::Hot,
            Self::Warm => ColumnFamily::Warm,
            Self::Archival => ColumnFamily::Archival,
        }
    }
}
