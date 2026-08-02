/// Integer square root via Newton iteration (floor(sqrt(n))).
///
/// Consensus must not use floating point for vote weight.
pub fn isqrt(n: u64) -> u64 {
    if n <= 1 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = x.saturating_add(n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isqrt_matches_known_values() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(15), 3);
        assert_eq!(isqrt(16), 4);
        assert_eq!(isqrt(17), 4);
        assert_eq!(isqrt(1_000_000), 1_000);
    }
}
