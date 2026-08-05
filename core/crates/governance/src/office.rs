//! Seated officers and elections.

use agora_types::Address;
use serde::{Deserialize, Serialize};

use crate::ranks::CivicRank;
use crate::GovernanceError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeSeat {
    pub rank: CivicRank,
    pub seat_index: u16,
    pub holder: Option<Address>,
    pub elected_slot: Option<u64>,
    pub term_end_slot: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeBoard {
    pub seats: Vec<OfficeSeat>,
}

impl OfficeBoard {
    pub fn with_defaults(boule_seats: u16, tamias_seats: u16) -> Self {
        let mut seats = Vec::new();
        for rank in [
            CivicRank::ArchonEponymous,
            CivicRank::ArchonBasileus,
            CivicRank::ArchonPolemarch,
        ] {
            seats.push(OfficeSeat {
                rank,
                seat_index: 0,
                holder: None,
                elected_slot: None,
                term_end_slot: None,
            });
        }
        for i in 0..boule_seats {
            seats.push(OfficeSeat {
                rank: CivicRank::Bouleutes,
                seat_index: i,
                holder: None,
                elected_slot: None,
                term_end_slot: None,
            });
        }
        for i in 0..tamias_seats {
            seats.push(OfficeSeat {
                rank: CivicRank::Tamias,
                seat_index: i,
                holder: None,
                elected_slot: None,
                term_end_slot: None,
            });
        }
        Self { seats }
    }

    pub fn seat_mut(
        &mut self,
        rank: CivicRank,
        seat_index: u16,
    ) -> Result<&mut OfficeSeat, GovernanceError> {
        self.seats
            .iter_mut()
            .find(|s| s.rank == rank && s.seat_index == seat_index)
            .ok_or(GovernanceError::InvalidSeat)
    }

    pub fn seat(&self, rank: CivicRank, seat_index: u16) -> Result<&OfficeSeat, GovernanceError> {
        self.seats
            .iter()
            .find(|s| s.rank == rank && s.seat_index == seat_index)
            .ok_or(GovernanceError::InvalidSeat)
    }

    pub fn holders(&self, rank: CivicRank) -> Vec<Address> {
        self.seats
            .iter()
            .filter(|s| s.rank == rank)
            .filter_map(|s| s.holder)
            .collect()
    }

    pub fn is_seated(&self, rank: CivicRank, who: Address) -> bool {
        self.seats
            .iter()
            .any(|s| s.rank == rank && s.holder == Some(who))
    }

    pub fn is_any_archon(&self, who: Address) -> bool {
        self.seats
            .iter()
            .any(|s| s.rank.is_archon() && s.holder == Some(who))
    }

    pub fn seat_holder(
        &mut self,
        rank: CivicRank,
        seat_index: u16,
        who: Address,
        now: u64,
        term_slots: u64,
    ) -> Result<(), GovernanceError> {
        let seat = self.seat_mut(rank, seat_index)?;
        if seat.holder.is_some() {
            return Err(GovernanceError::SeatOccupied);
        }
        seat.holder = Some(who);
        seat.elected_slot = Some(now);
        seat.term_end_slot = Some(now.saturating_add(term_slots));
        Ok(())
    }

    pub fn vacate(&mut self, rank: CivicRank, seat_index: u16) -> Result<Address, GovernanceError> {
        let seat = self.seat_mut(rank, seat_index)?;
        let prev = seat.holder.take().ok_or(GovernanceError::SeatVacant)?;
        seat.elected_slot = None;
        seat.term_end_slot = None;
        Ok(prev)
    }
}
