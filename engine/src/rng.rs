//! SplitMix64: a small, fully specified generator, so a seed means the same run on every
//! platform and toolchain. Changing this file changes what every seed replays.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` without modulo bias (Lemire's method).
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0)");
        let n = n as u64;
        let threshold = n.wrapping_neg() % n;
        loop {
            let m = u128::from(self.next_u64()) * u128::from(n);
            if (m as u64) >= threshold {
                return (m >> 64) as usize;
            }
        }
    }

    /// True with probability `p / 1000`.
    pub fn permille(&mut self, p: u32) -> bool {
        self.below(1000) < p as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_reference_splitmix64_sequence() {
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    }

    #[test]
    fn below_rejects_the_biased_region() {
        // Just over 2^63, almost half of all draws fall in the biased region and must be
        // redrawn; a modulo-style shortcut would change these values.
        let n = (1usize << 63) + 1;
        let got: Vec<usize> = (0..4).map(|seed| Rng::new(seed).below(n)).collect();
        assert_eq!(got, PINNED_BELOW);
        assert!(got.iter().all(|&x| x < n));
    }
    const PINNED_BELOW: [usize; 4] = [243_808_509_735_772_839, 8_955_919_645_141_445_295, 5_452_762_862_878_174_055, 1_046_394_712_501_569_526];

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut r = Rng::new(7);
        let mut seen = [false; 5];
        for _ in 0..1000 {
            let x = r.below(5);
            seen[x] = true;
        }
        assert!(seen.iter().all(|&s| s));
        assert!(!Rng::new(1).permille(0));
        assert!(Rng::new(1).permille(1000));
    }
}
