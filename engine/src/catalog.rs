//! The planted-bug catalog: how often, how fast and by which check the simulator finds each
//! bug, with the correct implementations as controls for false positives.
use std::collections::BTreeMap;
use std::fmt;

use crate::model::Op;
use crate::oracle::Mode;
use crate::sim::{detector, generate, run_ops_with, shrink};
use crate::stores::Variant;

/// Wilson score interval for `k` successes in `n` trials, at 95% (z = 1.96).
pub fn wilson(k: u64, n: u64) -> (f64, f64) {
    assert!(n > 0 && k <= n);
    let (k, n, z) = (k as f64, n as f64, 1.96_f64);
    let p = k / n;
    let denom = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denom;
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denom;
    // The bounds are exactly 0 at k = 0 and exactly 1 at k = n; floating point can land a
    // hair inside them, which would print 0.999 where 1.000 is the right answer.
    let lo = if k == 0.0 { 0.0 } else { (center - half).max(0.0) };
    let hi = if k == n { 1.0 } else { (center + half).min(1.0) };
    (lo, hi)
}

/// Nearest-rank percentile of a sorted, nonempty slice.
fn percentile(sorted: &[usize], pct: usize) -> usize {
    assert!(!sorted.is_empty() && pct <= 100);
    // Written out because `div_ceil` needs Rust 1.73.
    let rank = ((pct * sorted.len() + 99) / 100).max(1);
    sorted[rank - 1]
}

#[derive(Clone, Debug, PartialEq)]
pub struct Measure {
    pub variant: Variant,
    pub runs: u64,
    pub detected: u64,
    /// Seeds detected when the oracle probes only the two named windows.
    pub detected_named_only: u64,
    /// 1-based operation number of the first failure, for each detected seed, sorted.
    pub detection_ops: Vec<usize>,
    /// The same, when the oracle probes only the two named windows.
    pub detection_ops_named_only: Vec<usize>,
    pub first_check: BTreeMap<&'static str, u64>,
    /// The first detected seed and its shrunk failure.
    pub first: Option<(u64, Vec<Op>)>,
}

pub fn measure(variant: Variant, first_seed: u64, runs: u64, ops: usize) -> Measure {
    let mut m = Measure {
        variant,
        runs,
        detected: 0,
        detected_named_only: 0,
        detection_ops: Vec::new(),
        detection_ops_named_only: Vec::new(),
        first_check: BTreeMap::new(),
        first: None,
    };
    for seed in (0..runs).map(|i| first_seed.wrapping_add(i)) {
        let (_, generated) = generate(seed, ops);
        if let Some((at, step)) = run_ops_with(variant, &generated, Mode::Full).failure {
            m.detected += 1;
            m.detection_ops.push(at + 1);
            *m.first_check.entry(detector(&step)).or_default() += 1;
            if m.first.is_none() {
                m.first = Some((seed, shrink(variant, &generated)));
            }
        }
        if let Some((at, _)) = run_ops_with(variant, &generated, Mode::NamedOnly).failure {
            m.detected_named_only += 1;
            m.detection_ops_named_only.push(at + 1);
        }
    }
    m.detection_ops.sort_unstable();
    m.detection_ops_named_only.sort_unstable();
    m
}

pub struct Catalog {
    pub first_seed: u64,
    pub runs: u64,
    pub ops: usize,
    pub rows: Vec<Measure>,
}

pub fn catalog(first_seed: u64, runs: u64, ops: usize) -> Catalog {
    let rows = Variant::ALL.into_iter().map(|v| measure(v, first_seed, runs, ops)).collect();
    Catalog { first_seed, runs, ops, rows }
}

impl Catalog {
    /// Every bug found at least once, and no control ever flagged.
    pub fn sound(&self) -> bool {
        self.rows.iter().all(|m| if m.variant.correct() { m.detected == 0 } else { m.detected > 0 })
    }
}

impl fmt::Display for Catalog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let last = self.first_seed.wrapping_add(self.runs.saturating_sub(1));
        writeln!(f, "# seeds {}..={last}, {} operations each; rates are per seed, ci95 is Wilson", self.first_seed, self.ops)?;
        writeln!(f, "variant\tkind\tdetected\trate\tci95\tmedian_op\tp90_op\tnamed_only\tnamed_only_median_op\tshrunk_ops\tfirst_check")?;
        for m in &self.rows {
            let (lo, hi) = wilson(m.detected, m.runs);
            let (median, p90) = if m.detection_ops.is_empty() {
                ("-".into(), "-".into())
            } else {
                (percentile(&m.detection_ops, 50).to_string(), percentile(&m.detection_ops, 90).to_string())
            };
            let named_median = if m.detection_ops_named_only.is_empty() {
                "-".to_string()
            } else {
                percentile(&m.detection_ops_named_only, 50).to_string()
            };
            let checks: Vec<String> = m.first_check.iter().map(|(k, n)| format!("{}={n}", k.replace(' ', "_"))).collect();
            writeln!(
                f,
                "{}\t{}\t{}/{}\t{:.3}\t{:.3}-{:.3}\t{median}\t{p90}\t{}/{}\t{named_median}\t{}\t{}",
                m.variant.name(),
                if m.variant.correct() { "control" } else { "bug" },
                m.detected,
                m.runs,
                m.detected as f64 / m.runs as f64,
                lo,
                hi,
                m.detected_named_only,
                m.runs,
                m.first.as_ref().map_or_else(|| "-".to_string(), |(_, ops)| ops.len().to_string()),
                if checks.is_empty() { "-".into() } else { checks.join(",") },
            )?;
        }
        writeln!(f, "result\t{}", if self.sound() { "sound: every bug found, no control flagged" } else { "unsound" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assay;
    use crate::sim::run_ops;

    #[test]
    fn wilson_matches_known_values() {
        let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 5e-4 && (a.1 - b.1).abs() < 5e-4;
        assert!(close(wilson(0, 1000), (0.0, 0.00383)), "{:?}", wilson(0, 1000));
        assert!(close(wilson(1000, 1000), (0.99617, 1.0)), "{:?}", wilson(1000, 1000));
        assert!(close(wilson(5, 10), (0.23659, 0.76341)), "{:?}", wilson(5, 10));
    }

    #[test]
    fn wilson_stays_within_zero_and_one() {
        for n in [1, 2, 3, 7, 10, 200, 1000, 12345] {
            for k in [0, 1, n / 2, n - 1, n] {
                let (lo, hi) = wilson(k, n);
                assert!((0.0..=1.0).contains(&lo) && (0.0..=1.0).contains(&hi) && lo <= hi, "{k}/{n}: {lo} {hi}");
            }
            assert_eq!(wilson(0, n).0, 0.0);
            assert_eq!(wilson(n, n).1, 1.0);
        }
    }

    #[test]
    fn a_flagged_control_makes_the_catalog_unsound() {
        let mut c = catalog(1, 20, 48);
        assert!(c.sound());
        c.rows.iter_mut().find(|m| m.variant == Variant::Indexed).unwrap().detected = 1;
        assert!(!c.sound());
        let mut c = catalog(1, 20, 48);
        c.rows.iter_mut().find(|m| m.variant == Variant::StaleIndex).unwrap().detected = 0;
        assert!(!c.sound());
    }

    #[test]
    fn the_shrunk_example_comes_from_the_first_detected_seed() {
        for v in Variant::ALL.into_iter().filter(|v| !v.correct()) {
            let m = measure(v, 1, 50, 48);
            let (seed, shrunk) = m.first.clone().unwrap();
            let earliest = (1..=50).find(|&s| run_ops_with(v, &generate(s, 48).1, Mode::Full).failure.is_some());
            assert_eq!(Some(seed), earliest, "{}", v.name());
            assert_eq!(shrunk, shrink(v, &generate(seed, 48).1), "{}", v.name());
        }
    }

    #[test]
    fn percentile_is_nearest_rank() {
        assert_eq!(percentile(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 50), 5);
        assert_eq!(percentile(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 90), 9);
        assert_eq!(percentile(&[7], 90), 7);
    }

    /// The smallest hand-written case for each bug: it fails there and passes on `indexed`.
    const MINIMAL: [(Variant, &str, &str); 6] = [
        (Variant::StaleIndex, "reserve A 0 600 660\nreserve B 0 720 780\nmove A 750 810", "original window"),
        (Variant::LeftoverEntry, "reserve A 0 600 660\nmove A 700 760", "original window"),
        (Variant::ClosedInterval, "reserve A 0 600 660\nreserve B 1 660 720", "attempted window"),
        (Variant::WrongBucket, "reserve A 1 600 660", "attempted window"),
        (Variant::IndexBeforeCheck, "reserve A 0 600 660\nreserve A 0 700 760", "attempted window"),
        (Variant::NoSelfExclusion, "reserve A 0 600 660\nmove A 630 690", "result"),
    ];

    #[test]
    fn each_bug_has_a_minimal_witness() {
        for (variant, text, check) in MINIMAL {
            let ops = assay::parse(text).unwrap();
            let failure = run_ops(variant, &ops).failure.unwrap_or_else(|| panic!("{} not caught", variant.name()));
            assert_eq!(failure.0, ops.len() - 1, "{} caught on the last operation", variant.name());
            assert_eq!(detector(&failure.1), check, "{}: {:?}", variant.name(), failure.1.reasons);
            assert!(run_ops(Variant::Indexed, &ops).failure.is_none());
        }
        let covered: Vec<Variant> = MINIMAL.iter().map(|m| m.0).collect();
        assert!(Variant::ALL.iter().all(|v| v.correct() || covered.contains(v)), "every bug has a witness");
    }

    #[test]
    fn the_catalog_is_sound_and_the_sweep_never_hurts() {
        let c = catalog(1, 200, 48);
        assert!(c.sound(), "{c}");
        for m in &c.rows {
            assert!(m.detected >= m.detected_named_only, "{}", m.variant.name());
        }
        // Per seed: the full oracle only adds probes, so it never detects later.
        for v in Variant::ALL {
            for seed in 1..=200 {
                let (_, ops) = generate(seed, 48);
                let full = run_ops_with(v, &ops, Mode::Full).failure.map(|f| f.0);
                let named = run_ops_with(v, &ops, Mode::NamedOnly).failure.map(|f| f.0);
                if let Some(n) = named {
                    assert!(matches!(full, Some(f) if f <= n), "{} seed {seed}: full {full:?}, named {n}", v.name());
                }
            }
        }
    }

    /// Whether `ops` ever reach the situation that triggers `variant`'s bug, judged from the
    /// reference alone.
    fn triggered(variant: Variant, ops: &[crate::model::Op]) -> bool {
        use crate::model::{Op, Outcome};
        use crate::stores::{Reference, Store};
        let mut state = Reference::default();
        ops.iter().any(|op| {
            let before = state.clone();
            let accepted = state.apply(op) == Outcome::Ok;
            match (variant, op) {
                (Variant::WrongBucket, Op::Reserve(_, slot)) => accepted && slot.chamber == 1,
                (Variant::NoSelfExclusion, Op::Move(id, w)) => {
                    accepted && before.slot(id).map_or(false, |old| old.window.overlaps(*w))
                }
                _ => false,
            }
        })
    }

    #[test]
    fn every_miss_is_a_coverage_gap_not_a_blind_oracle() {
        // For these bugs a miss must mean the generator never triggered them; if it did, the
        // oracle has to have caught it.
        for v in [Variant::WrongBucket, Variant::NoSelfExclusion] {
            let (mut misses, mut caught) = (0, 0);
            for seed in 1..=1000 {
                let (_, ops) = generate(seed, 64);
                let detected = run_ops_with(v, &ops, Mode::Full).failure.is_some();
                assert_eq!(detected, triggered(v, &ops), "{} seed {seed}", v.name());
                if detected { caught += 1 } else { misses += 1 }
            }
            assert!(misses > 0 && caught > 0, "{}", v.name());
        }
    }

    #[test]
    fn catalog_matches_the_golden_file() {
        assert_eq!(catalog(1, 200, 48).to_string(), include_str!("../sim/catalog.expected"));
    }
}
