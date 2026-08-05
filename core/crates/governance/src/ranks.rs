//! Elected civic ranks (classical Agora offices).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Elected office in Agora civic governance.
///
/// These are protocol rank names (not vanity titles). Seat counts are defaults
/// from Constitution v1 and may be changed only by constitution amendment.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum CivicRank {
    /// Ἄρχων Ἐπώνυμος — chairs the Boule; ceremonial year-name.
    ArchonEponymous,
    /// Ἄρχων Βασιλεύς — guardian of the Constitution / higher law.
    ArchonBasileus,
    /// Ἄρχων Πολέμαρχος — security & emergency track.
    ArchonPolemarch,
    /// Βουλευτής — seated member of the Boule.
    Bouleutes,
    /// Ταμίας — treasury steward.
    Tamias,
}

impl CivicRank {
    pub const ALL: [CivicRank; 5] = [
        CivicRank::ArchonEponymous,
        CivicRank::ArchonBasileus,
        CivicRank::ArchonPolemarch,
        CivicRank::Bouleutes,
        CivicRank::Tamias,
    ];

    /// English display name.
    pub fn title(self) -> &'static str {
        match self {
            CivicRank::ArchonEponymous => "Archon Eponymous",
            CivicRank::ArchonBasileus => "Archon Basileus",
            CivicRank::ArchonPolemarch => "Archon Polemarch",
            CivicRank::Bouleutes => "Bouleutes",
            CivicRank::Tamias => "Tamias",
        }
    }

    /// Classical Greek title (polytonic where conventional).
    pub fn greek(self) -> &'static str {
        match self {
            CivicRank::ArchonEponymous => "Ἄρχων Ἐπώνυμος",
            CivicRank::ArchonBasileus => "Ἄρχων Βασιλεύς",
            CivicRank::ArchonPolemarch => "Ἄρχων Πολέμαρχος",
            CivicRank::Bouleutes => "Βουλευτής",
            CivicRank::Tamias => "Ταμίας",
        }
    }

    /// Default number of seats (Constitution v1).
    pub fn default_seats(self) -> u16 {
        match self {
            CivicRank::ArchonEponymous => 1,
            CivicRank::ArchonBasileus => 1,
            CivicRank::ArchonPolemarch => 1,
            CivicRank::Bouleutes => 21,
            CivicRank::Tamias => 3,
        }
    }

    pub fn is_archon(self) -> bool {
        matches!(
            self,
            CivicRank::ArchonEponymous | CivicRank::ArchonBasileus | CivicRank::ArchonPolemarch
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_seat_total_matches_constitution_v1() {
        let total: u16 = CivicRank::ALL.iter().map(|r| r.default_seats()).sum();
        // 1+1+1+21+3
        assert_eq!(total, 27);
    }
}
