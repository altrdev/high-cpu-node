//! The coin that `/flaky` tosses: which requests fail, for a share of them.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// A generator of random numbers that needs no lock: every call takes the next step of a counter and
/// scrambles it (SplitMix64), so requests that arrive together don't repeat a number.
pub struct Flaky {
    state: AtomicU64,
}

impl Default for Flaky {
    fn default() -> Flaky {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos() as u64);
        Flaky::seeded(seed)
    }
}

impl Flaky {
    pub fn seeded(seed: u64) -> Flaky {
        Flaky { state: AtomicU64::new(seed) }
    }

    fn next(&self) -> u64 {
        let mut z = self.state.fetch_add(GAMMA, Ordering::Relaxed).wrapping_add(GAMMA);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Whether this request fails, `rate` percent of the time: never at 0 and always at 100
    pub fn fails(&self, rate: i64) -> bool {
        (self.next() % 100) < rate.clamp(0, 100) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_never_fails_and_a_hundred_always_does() {
        let flaky = Flaky::seeded(1);
        assert!((0..10_000).all(|_| !flaky.fails(0)));
        assert!((0..10_000).all(|_| flaky.fails(100)));
    }

    #[test]
    fn the_share_of_failures_follows_the_rate() {
        let flaky = Flaky::seeded(42);
        for rate in [10, 25, 50, 90] {
            let failed = (0..20_000).filter(|_| flaky.fails(rate)).count() as f64 / 200.0;
            assert!((failed - rate as f64).abs() < 2.0, "{failed}% failed with rate {rate}");
        }
    }
}
