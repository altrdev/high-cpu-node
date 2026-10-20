//! SplitMix64, the small generator of random numbers that `/flaky` and `/payload` use: a counter that
//! goes up by `GAMMA` at each step, and a function that scrambles it. Nothing here is secret.

/// What the counter goes up by at each step
pub const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// Scrambles a step of the counter into a number that looks random
pub fn mix(step: u64) -> u64 {
    let mut z = step;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_number_of_seed_zero_is_the_known_one() {
        assert_eq!(mix(GAMMA), 0xE220_A839_7B1D_CDAF);
    }
}
