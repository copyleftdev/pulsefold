//! Deterministic simulation, in the style of TigerBeetle's VOPR and Antithesis properties.
//!
//! - **Seeded swarm runs.** A seed fixes the operation mix (the "swarm" knobs) and every
//!   operation, so `--seed N --runs 1` replays a run exactly.
//! - **Always/sometimes properties.** An always-property must hold at every step. A
//!   sometimes-property must hold at least once across the run; if one never fires, a green
//!   result says little, and the run is reported as weak.
//! - **Shrinking.** A failing run is minimized to the shortest operation list that still
//!   fails, written as an assay the viewers can replay.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::model::{Op, Outcome, Slot, Tick, Window, CHAMBERS};
use crate::oracle::{self, elementary, Step};
use crate::rng::Rng;
use crate::stores::{Reference, Store, Variant};

/// Generated windows start on this grid, from `ORIGIN`.
const ORIGIN: Tick = 600;
const GRAIN: Tick = 15;
const MAX_GRAINS: usize = 8;

/// Per-seed knobs, drawn from the seed itself. Different runs stress different mixes
/// instead of one average mix that rarely reaches the corners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Swarm {
    /// Booking IDs in play (`A`, `B`, ...). Few IDs mean more duplicates and more moves of
    /// existing bookings.
    pub ids: usize,
    /// Chance that an operation is a move rather than a reserve.
    pub move_permille: u32,
    /// Chance that a window is placed against an existing booking (touching or overlapping).
    pub boundary_permille: u32,
    /// Width of the region where random windows start.
    pub horizon: Tick,
}

impl Swarm {
    fn draw(rng: &mut Rng) -> Self {
        Swarm {
            ids: 2 + rng.below(5),
            move_permille: [200, 500, 800][rng.below(3)],
            boundary_permille: [0, 300, 700][rng.below(3)],
            horizon: [240, 480][rng.below(2)],
        }
    }
}

impl fmt::Display for Swarm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ids={} move={}\u{2030} boundary={}\u{2030} horizon={}",
            self.ids, self.move_permille, self.boundary_permille, self.horizon
        )
    }
}

fn id_name(i: usize) -> String {
    char::from(b'A' + u8::try_from(i).expect("small id")).to_string()
}

fn grains(rng: &mut Rng) -> Tick {
    GRAIN * (1 + rng.below(MAX_GRAINS) as Tick)
}

/// A window in `chamber`, either random or placed against another booking there.
fn window(rng: &mut Rng, swarm: &Swarm, state: &Reference, chamber: Option<usize>, id: &str) -> Window {
    let len = grains(rng);
    if rng.permille(swarm.boundary_permille) {
        let anchors: Vec<Slot> = state
            .bookings()
            .into_iter()
            .filter(|(k, s)| k != id && !matches!(chamber, Some(c) if s.chamber != c))
            .map(|(_, s)| s)
            .collect();
        if !anchors.is_empty() {
            let a = anchors[rng.below(anchors.len())].window;
            let w = match rng.below(3) {
                0 => Window { start: a.end, end: a.end + len },
                1 => Window { start: a.start - len, end: a.start },
                _ => {
                    let start = a.start + GRAIN * rng.below(usize::from((a.end - a.start) / GRAIN).max(1)) as Tick;
                    Window { start, end: start + len }
                }
            };
            assert!(w.start < w.end);
            return w;
        }
    }
    let start = ORIGIN + GRAIN * rng.below(usize::from(swarm.horizon / GRAIN)) as Tick;
    Window { start, end: start + len }
}

/// The swarm knobs and operations for `seed`. Pure: depends on nothing but the seed and `n`.
pub fn generate(seed: u64, n: usize) -> (Swarm, Vec<Op>) {
    let mut rng = Rng::new(seed);
    let swarm = Swarm::draw(&mut rng);
    let mut state = Reference::default();
    let mut ops = Vec::with_capacity(n);
    for _ in 0..n {
        let id = id_name(rng.below(swarm.ids));
        let op = if rng.permille(swarm.move_permille) {
            let chamber = state.slot(&id).map(|s| s.chamber);
            Op::Move(id.clone(), window(&mut rng, &swarm, &state, chamber, &id))
        } else {
            let chamber = rng.below(CHAMBERS);
            Op::Reserve(id.clone(), Slot { chamber, window: window(&mut rng, &swarm, &state, Some(chamber), &id) })
        };
        state.apply(&op);
        ops.push(op);
    }
    (swarm, ops)
}

pub const ALWAYS: &str = "agrees_with_reference";

/// Each must fire at least once, or the run did not exercise what it claims to.
pub const SOMETIMES: [(&str, &str); 9] = [
    ("rejected_move", "a move is rejected with CONFLICT"),
    ("stale_write_observable", "a stale index write on a rejected move would change some availability answer"),
    ("stale_write_hidden_from_named_probes", "...but neither the full original nor the full attempted window would show it"),
    ("valid_move", "a move is accepted"),
    ("move_after_rejection", "a booking moves after one of its moves was rejected"),
    ("touching_accept", "an operation touching another booking end-to-start is accepted"),
    ("duplicate_rejected", "a reserve of an existing ID is rejected"),
    ("missing_move", "a move of an unknown ID is rejected"),
    ("chamber_full", "no chamber is free in a probed window"),
];

#[derive(Default)]
struct History {
    rejected: BTreeSet<String>,
}

fn touches(a: Window, b: Window) -> bool {
    a.end == b.start || b.end == a.start
}

/// Whether `busy` says the chamber is occupied anywhere in `w`.
fn occupied(busy: &[Window], w: Window) -> bool {
    busy.iter().any(|b| b.overlaps(w))
}

/// For a rejected move from `old` to `new` with `others` in the chamber: is a stale write
/// observable at all, and if so, would the full original or attempted window show it?
fn stale_visibility(others: &[Window], old: Window, new: Window) -> (bool, bool) {
    let truth: Vec<Window> = others.iter().copied().chain([old]).collect();
    let ghost: Vec<Window> = others.iter().copied().chain([new]).collect();
    let differs = |w: Window| occupied(&truth, w) != occupied(&ghost, w);
    let observable = elementary(others.iter().copied().chain([old, new])).into_iter().any(differs);
    (observable, differs(old) || differs(new))
}

/// Sometimes-properties that fired on this step, judged from the reference only.
fn observe(before: &Reference, s: &Step, history: &mut History) -> Vec<&'static str> {
    let mut fired = Vec::new();
    let others = |id: &str, chamber: usize| -> Vec<Window> {
        before.bookings().into_iter().filter(|(k, v)| k != id && v.chamber == chamber).map(|(_, v)| v.window).collect()
    };
    let (id, target) = match &s.op {
        Op::Reserve(id, slot) => (id, Some(*slot)),
        Op::Move(id, w) => (id, before.slot(id).map(|old| Slot { chamber: old.chamber, window: *w })),
    };
    match (&s.op, s.expected) {
        (Op::Move(..), Outcome::Conflict) => {
            fired.push("rejected_move");
            let old = before.slot(id).expect("a conflicting move has a booking");
            let new = target.expect("a conflicting move has a target").window;
            let (observable, named) = stale_visibility(&others(id, old.chamber), old.window, new);
            if observable {
                fired.push("stale_write_observable");
                if !named {
                    fired.push("stale_write_hidden_from_named_probes");
                }
            }
            history.rejected.insert(id.clone());
        }
        (Op::Move(..), Outcome::Ok) => {
            fired.push("valid_move");
            if history.rejected.contains(id) {
                fired.push("move_after_rejection");
            }
        }
        (Op::Move(..), Outcome::NotFound) => fired.push("missing_move"),
        (Op::Reserve(..), Outcome::Duplicate) => fired.push("duplicate_rejected"),
        _ => {}
    }
    if s.expected == Outcome::Ok {
        if let Some(t) = target {
            if others(id, t.chamber).iter().any(|w| touches(*w, t.window)) {
                fired.push("touching_accept");
            }
        }
    }
    if s.attempted.reference.is_empty() || matches!(&s.original, Some(p) if p.reference.is_empty()) {
        fired.push("chamber_full");
    }
    fired
}

/// The outcome of replaying one operation list against one implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// Steps checked; the run stops at the first failure.
    pub checked: u64,
    /// Index and failing step, if any.
    pub failure: Option<(usize, Step)>,
    pub hits: BTreeMap<&'static str, u64>,
}

pub fn run_ops(variant: Variant, ops: &[Op]) -> Run {
    let mut reference = Reference::default();
    let mut store: Box<dyn Store> = variant.build();
    let mut history = History::default();
    let mut run = Run { checked: 0, failure: None, hits: BTreeMap::new() };
    for (i, op) in ops.iter().enumerate() {
        let before = reference.clone();
        let s = oracle::step(&mut reference, store.as_mut(), op);
        run.checked += 1;
        for p in observe(&before, &s, &mut history) {
            *run.hits.entry(p).or_default() += 1;
        }
        if !s.passed() {
            run.failure = Some((i, s));
            break;
        }
    }
    run
}

fn fails(variant: Variant, ops: &[Op]) -> bool {
    run_ops(variant, ops).failure.is_some()
}

/// Renames IDs to `A`, `B`, ... in order of first appearance. Only equality of IDs matters
/// to the checks, so this keeps the failure while making the result easier to read.
fn canonical_ids(ops: &[Op]) -> Vec<Op> {
    let mut names: Vec<String> = Vec::new();
    let mut rename = |id: &str| -> String {
        let at = names.iter().position(|n| n == id).unwrap_or_else(|| {
            names.push(id.to_string());
            names.len() - 1
        });
        id_name(at)
    };
    ops.iter()
        .map(|op| match op {
            Op::Reserve(id, s) => Op::Reserve(rename(id), *s),
            Op::Move(id, w) => Op::Move(rename(id), *w),
        })
        .collect()
}

/// Shortest failing operation list reachable by deleting chunks, then single operations.
/// Requires `ops` to fail; the result always fails too.
pub fn shrink(variant: Variant, ops: &[Op]) -> Vec<Op> {
    let first = run_ops(variant, ops).failure.expect("shrink needs a failing run").0;
    let mut ops = ops[..=first].to_vec();
    loop {
        ops = delete_chunks(variant, ops);
        match collapse_move(variant, &ops) {
            Some(shorter) => ops = shorter,
            None => break,
        }
    }
    let renamed = canonical_ids(&ops);
    if fails(variant, &renamed) {
        renamed
    } else {
        ops
    }
}

/// Folds a move into the reserve of the same booking (reserve at the moved-to window)
/// when that still fails. Deletion alone cannot make this simplification.
fn collapse_move(variant: Variant, ops: &[Op]) -> Option<Vec<Op>> {
    for j in 0..ops.len() {
        let Op::Move(id, w) = &ops[j] else { continue };
        let Some(i) = ops[..j].iter().rposition(|op| matches!(op, Op::Reserve(r, _) | Op::Move(r, _) if r == id)) else {
            continue;
        };
        let Op::Reserve(_, slot) = &ops[i] else { continue };
        let mut candidate = ops.to_vec();
        candidate[i] = Op::Reserve(id.clone(), Slot { chamber: slot.chamber, window: *w });
        candidate.remove(j);
        if fails(variant, &candidate) {
            return Some(candidate);
        }
    }
    None
}

fn delete_chunks(variant: Variant, mut ops: Vec<Op>) -> Vec<Op> {
    let mut chunk = ops.len() / 2;
    while chunk > 0 {
        let mut progressed = false;
        let mut i = 0;
        while i < ops.len() {
            let mut candidate = ops.clone();
            candidate.drain(i..(i + chunk).min(ops.len()));
            if !candidate.is_empty() && fails(variant, &candidate) {
                ops = candidate;
                progressed = true;
            } else {
                i += chunk;
            }
        }
        if !progressed {
            chunk /= 2;
        }
    }
    ops
}

/// An assay that replays a shrunk failure.
pub fn assay_text(seed: u64, variant: Variant, original_len: usize, ops: &[Op]) -> String {
    let mut out = format!(
        "# Shrunk by `pulsefold sim`: seed {seed}, variant {}, {original_len} -> {} operations.\n",
        variant.name(),
        ops.len()
    );
    for op in ops {
        out.push_str(&op.to_string());
        out.push('\n');
    }
    out
}

/// A one-turn session around that assay, so `pulsefold trace` and the viewers can replay it.
pub fn session_text(seed: u64, variant: Variant, assay_file: &str) -> String {
    format!(
        "# Pulsefold session v1. Generated by `pulsefold sim` for seed {seed}.\n\
         # A simulation has no agent response, so the marker is unobserved.\n\
         token 0000000000000000\nassay {assay_file}\nturn {} n/a <unobserved>\n",
        variant.name()
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirstFailure {
    pub seed: u64,
    pub swarm: Swarm,
    pub generated: usize,
    /// 1-based operation number within the seed's run.
    pub at: usize,
    pub reasons: Vec<String>,
    pub shrunk: Vec<Op>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub variant: Variant,
    pub first_seed: u64,
    pub runs: u64,
    pub ops: usize,
    pub checked: u64,
    pub failing_seeds: u64,
    pub first_failure: Option<FirstFailure>,
    pub hits: BTreeMap<&'static str, u64>,
    /// For failing seeds: which check reported the first failure.
    pub detected_by: BTreeMap<&'static str, u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    /// No failure, but some sometimes-property never fired.
    Weak,
}

impl Report {
    pub fn unreached(&self) -> Vec<&'static str> {
        SOMETIMES.iter().map(|(p, _)| *p).filter(|p| !self.hits.contains_key(p)).collect()
    }

    pub fn verdict(&self) -> Verdict {
        if self.failing_seeds > 0 {
            Verdict::Fail
        } else if self.unreached().is_empty() {
            Verdict::Pass
        } else {
            Verdict::Weak
        }
    }
}

/// The first check that reported a failing step.
fn detector(step: &Step) -> &'static str {
    let first = step.reasons.first().map(String::as_str).unwrap_or("");
    ["result", "bookings", "original window", "attempted window", "sweep window"]
        .into_iter()
        .find(|k| first.starts_with(k))
        .unwrap_or("other")
}

pub fn simulate(variant: Variant, first_seed: u64, runs: u64, ops: usize) -> Report {
    let mut report = Report {
        variant,
        first_seed,
        runs,
        ops,
        checked: 0,
        failing_seeds: 0,
        first_failure: None,
        hits: BTreeMap::new(),
        detected_by: BTreeMap::new(),
    };
    for seed in (0..runs).map(|i| first_seed.wrapping_add(i)) {
        let (swarm, generated) = generate(seed, ops);
        let run = run_ops(variant, &generated);
        report.checked += run.checked;
        for (p, n) in run.hits {
            *report.hits.entry(p).or_default() += n;
        }
        if let Some((at, step)) = run.failure {
            report.failing_seeds += 1;
            *report.detected_by.entry(detector(&step)).or_default() += 1;
            if report.first_failure.is_none() {
                let shrunk = shrink(variant, &generated);
                report.first_failure =
                    Some(FirstFailure { seed, swarm, generated: generated.len(), at: at + 1, reasons: step.reasons, shrunk });
            }
        }
    }
    report
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let last = self.first_seed.wrapping_add(self.runs.saturating_sub(1));
        writeln!(f, "variant\t{}", self.variant.name())?;
        writeln!(f, "seeds\t{}..={last}", self.first_seed)?;
        writeln!(f, "ops_per_seed\t{}", self.ops)?;
        // A run stops at its first failure, so each failing seed contributes one failed step.
        writeln!(f, "always\t{ALWAYS}\t{}/{} checked steps held", self.checked - self.failing_seeds, self.checked)?;
        for (p, _) in SOMETIMES {
            writeln!(f, "sometimes\t{p}\t{}", self.hits.get(p).copied().unwrap_or(0))?;
        }
        writeln!(f, "failing_seeds\t{}/{}", self.failing_seeds, self.runs)?;
        for (k, n) in &self.detected_by {
            writeln!(f, "detected_by\t{k}\t{n}")?;
        }
        match &self.first_failure {
            None => writeln!(f, "first_failure\t-")?,
            Some(x) => {
                writeln!(f, "first_failure\tseed {} ({}), operation {} of {}", x.seed, x.swarm, x.at, x.generated)?;
                writeln!(f, "reason\t{}", x.reasons.join("; "))?;
                writeln!(f, "shrunk\t{} operations", x.shrunk.len())?;
                for op in &x.shrunk {
                    writeln!(f, "shrunk_op\t{op}")?;
                }
                writeln!(f, "reproduce\tpulsefold sim --variant {} --seed {} --runs 1 --ops {}", self.variant.name(), x.seed, self.ops)?;
            }
        }
        let unreached = self.unreached();
        writeln!(f, "unreached\t{}", if unreached.is_empty() { "-".into() } else { unreached.join(",") })?;
        let verdict = match self.verdict() {
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
            Verdict::Weak => "weak: no failure, but some sometimes-properties never fired",
        };
        writeln!(f, "result\t{verdict}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assay;

    #[test]
    fn a_seed_fixes_the_whole_run() {
        assert_eq!(generate(42, 64), generate(42, 64));
        assert_ne!(generate(42, 64).1, generate(43, 64).1);
        assert_eq!(generate(42, 10).1, generate(42, 64).1[..10], "prefix-stable");
        assert_eq!(simulate(Variant::StaleIndex, 1, 50, 48), simulate(Variant::StaleIndex, 1, 50, 48));
    }

    #[test]
    fn generator_is_pinned() {
        // Changing the generator changes what every published seed means. Update deliberately.
        let ops: Vec<String> = generate(1, 4).1.iter().map(Op::to_string).collect();
        assert_eq!(ops, PINNED_SEED_1);
    }
    const PINNED_SEED_1: [&str; 4] = ["move B 660 780", "move D 720 795", "move B 750 840", "move D 615 630"];

    /// FNV-1a over the text of every generated operation, seeds 1..=100, 64 operations each.
    fn generator_fingerprint() -> u64 {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        for seed in 1..=100 {
            for op in generate(seed, 64).1 {
                for b in op.to_string().bytes().chain([b'\n']) {
                    h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
                }
            }
        }
        h
    }

    #[test]
    fn generator_fingerprint_is_pinned() {
        assert_eq!(generator_fingerprint(), PINNED_FINGERPRINT, "{:#x}", generator_fingerprint());
    }
    const PINNED_FINGERPRINT: u64 = 0xf328_e725_e252_119a;

    #[test]
    fn reports_match_the_golden_files() {
        // The same files verify.sh compares against the CLI, so library and binary agree.
        for (variant, runs, ops, golden) in [
            (Variant::Indexed, 200, 48, include_str!("../sim/indexed.expected")),
            (Variant::StaleIndex, 200, 48, include_str!("../sim/stale-index.expected")),
            (Variant::Indexed, 1, 1, include_str!("../sim/weak.expected")),
        ] {
            assert_eq!(simulate(variant, 1, runs, ops).to_string(), golden, "{}", variant.name());
        }
    }

    #[test]
    fn shrunk_ids_are_canonical() {
        let ops = assay::parse("reserve Q 0 1 2\nmove Z 1 2\nmove Q 3 4\nreserve Z 1 5 6").unwrap();
        let names: Vec<String> = canonical_ids(&ops).iter().map(|o| o.to_string()).collect();
        assert_eq!(names, ["reserve A 0 1 2", "move B 1 2", "move A 3 4", "reserve B 1 5 6"]);
        let r = simulate(Variant::StaleIndex, 1, 200, 48);
        let x = r.first_failure.unwrap();
        let first_ids: Vec<String> = x.shrunk.iter().map(|o| o.to_string().split(' ').nth(1).unwrap().to_string()).collect();
        assert_eq!(first_ids, ["A", "B", "A"]);
    }

    #[test]
    fn generated_operations_parse_back() {
        for seed in 0..50 {
            let (_, ops) = generate(seed, 32);
            let text: String = ops.iter().map(|o| format!("{o}\n")).collect();
            assert_eq!(assay::parse(&text).unwrap(), ops);
        }
    }

    #[test]
    fn correct_variants_hold_everywhere_and_reach_every_corner() {
        for v in [Variant::Scan, Variant::Indexed, Variant::Rebuild] {
            let r = simulate(v, 1, 300, 48);
            assert_eq!(r.failing_seeds, 0, "{}: {r}", v.name());
            assert_eq!(r.verdict(), Verdict::Pass, "{}: {r}", v.name());
        }
    }

    #[test]
    fn the_planted_bug_is_found_and_shrunk_to_three_operations() {
        let r = simulate(Variant::StaleIndex, 1, 300, 48);
        assert_eq!(r.verdict(), Verdict::Fail);
        let x = r.first_failure.as_ref().unwrap();
        assert_eq!(x.shrunk.len(), 3, "{r}");
        assert!(matches!(x.shrunk[..], [Op::Reserve(..), Op::Reserve(..), Op::Move(..)]), "{r}");
        assert!(fails(Variant::StaleIndex, &x.shrunk));
        assert!(!fails(Variant::Indexed, &x.shrunk));
    }

    /// Rejected moves of `id` among `ops[..end]`, with whether a stale write was observable then.
    fn rejected_moves(ops: &[Op], end: usize, id: &str) -> Vec<bool> {
        let mut state = Reference::default();
        let mut seen = Vec::new();
        for op in &ops[..end] {
            if let Op::Move(m, w) = op {
                if m == id {
                    if let Some(old) = state.slot(id) {
                        let mut probe = state.clone();
                        if probe.apply(op) == Outcome::Conflict {
                            let others: Vec<Window> = state
                                .bookings()
                                .into_iter()
                                .filter(|(k, v)| k != id && v.chamber == old.chamber)
                                .map(|(_, v)| v.window)
                                .collect();
                            seen.push(stale_visibility(&others, old.window, *w).0);
                        }
                    }
                }
            }
            state.apply(op);
        }
        seen
    }

    #[test]
    fn stale_writes_are_caught_when_observable_or_surface_later_when_latent() {
        let (mut immediate, mut latent, mut swept) = (0, 0, 0);
        for seed in 1..=500 {
            let (_, ops) = generate(seed, 48);
            let Some((at, step)) = run_ops(Variant::StaleIndex, &ops).failure else { continue };
            assert!(!step.reasons.iter().any(|r| r.starts_with("attempted")), "seed {seed}: {:?}", step.reasons);
            swept += usize::from(step.reasons[0].starts_with("sweep"));
            if matches!(ops[at], Op::Move(..)) && step.expected == Outcome::Conflict {
                immediate += 1;
                continue;
            }
            // Caught later: every stale write before it must have been invisible when made.
            latent += 1;
            let ids: BTreeSet<String> = ops[..at]
                .iter()
                .filter_map(|op| match op { Op::Move(id, _) => Some(id.clone()), _ => None })
                .collect();
            let earlier: Vec<bool> = ids.iter().flat_map(|id| rejected_moves(&ops, at, id)).collect();
            assert!(!earlier.is_empty() && earlier.iter().all(|o| !o), "seed {seed}: an observable stale write went unreported");
        }
        assert!(immediate > 0 && latent > 0 && swept > 0, "immediate {immediate}, latent {latent}, swept {swept}");
    }

    #[test]
    fn visibility_model_matches_the_known_cases() {
        let w = |start, end| Window { start, end };
        // Ghost Cycle: the original window shows it.
        assert_eq!(stale_visibility(&[w(720, 780)], w(600, 660), w(750, 810)), (true, true));
        // Seed 7: the attempted window overlaps the original, so only the vacated part shows it.
        assert_eq!(stale_visibility(&[w(750, 855)], w(855, 960), w(825, 870)), (true, false));
        // Fully covered by another booking: nothing can show it.
        assert_eq!(stale_visibility(&[w(500, 1000)], w(600, 660), w(700, 760)), (false, false));
    }

    #[test]
    fn shrinking_keeps_the_failure() {
        for seed in 1..=100 {
            let (_, ops) = generate(seed, 48);
            if fails(Variant::StaleIndex, &ops) {
                let s = shrink(Variant::StaleIndex, &ops);
                assert!(fails(Variant::StaleIndex, &s) && s.len() <= ops.len(), "seed {seed}");
            }
        }
    }

    #[test]
    fn a_short_run_is_weak_not_green() {
        let r = simulate(Variant::Indexed, 1, 1, 1);
        assert_eq!(r.verdict(), Verdict::Weak);
        assert!(!r.unreached().is_empty());
    }

    #[test]
    fn shrunk_failure_replays_as_a_session() {
        let r = simulate(Variant::StaleIndex, 1, 300, 48);
        let x = r.first_failure.unwrap();
        let assay_ops = assay::parse(&assay_text(x.seed, Variant::StaleIndex, x.generated, &x.shrunk)).unwrap();
        let s = crate::session::parse(&session_text(x.seed, Variant::StaleIndex, "f.assay")).unwrap();
        let rows = crate::session::replay(&s, &assay_ops);
        assert!(!rows.last().unwrap().passed());
        assert!(rows[..rows.len() - 1].iter().all(crate::session::Row::passed));
    }
}
