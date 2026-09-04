//! Integer-only supermajority quorum math for Trident dual PoS.
//!
//! Predicate: `3 * signed_stake >= 2 * active_stake` with `active_stake > 0`.
//! Empty validator sets never finalize (no bootstrap bypass that drops a quorum).

/// Returns true iff `signed` is at least two-thirds of `active` (ceil via multiply).
pub fn has_two_thirds_quorum(signed_stake: u64, active_stake: u64) -> bool {
    if active_stake == 0 {
        return false;
    }
    // Use u128 to avoid overflow on large stake totals.
    let signed = u128::from(signed_stake);
    let active = u128::from(active_stake);
    signed.saturating_mul(3) >= active.saturating_mul(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_active_never_quorum() {
        assert!(!has_two_thirds_quorum(0, 0));
        assert!(!has_two_thirds_quorum(100, 0));
    }

    #[test]
    fn exact_two_thirds() {
        // 2 of 3
        assert!(has_two_thirds_quorum(2, 3));
        assert!(!has_two_thirds_quorum(1, 3));
        // 66 of 100 → 198 >= 200? no
        assert!(!has_two_thirds_quorum(66, 100));
        assert!(has_two_thirds_quorum(67, 100));
    }

    #[test]
    fn full_set() {
        assert!(has_two_thirds_quorum(10, 10));
    }
}
