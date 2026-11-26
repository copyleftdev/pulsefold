//! One text-in, text-out entry point. The browser console calls it through WebAssembly;
//! `pulsefold api` calls it natively, so the two can be compared byte for byte.
//!
//! Requests (fields separated by spaces):
//!
//! - `sim VARIANT SEED OPS RUNS`: a simulation report, then [`SEP`], then a trace v2 of the
//!   first failure after shrinking (or of the first seed's whole run if nothing failed).
//! - `race VARIANT SEED RUNS`: a race report, then [`SEP`], then a `.race` file: the shrunk
//!   first failure, or the first seed's scenario with its realized schedule.
//! - `replay` on its own line, followed by a `.race` file: every micro-step, the history and
//!   the verdict.
//!
//! The status is the CLI's exit code: 0 pass, 1 failure, 2 bad request, 3 weak.
use crate::race::{self, RaceVariant};
use crate::session::{self, Refactor, Session, Turn};
use crate::sim;
use crate::stores::Variant;

/// Separates a report from the trace or file that follows it.
pub const SEP: char = '\u{1e}';

/// Bounds that keep one request responsive in a browser tab.
pub const MAX_RUNS: u64 = 5000;
pub const MAX_OPS: usize = 2000;

fn trace(variant: Variant, ops: &[crate::model::Op]) -> String {
    let s = Session {
        token: "0000000000000000".into(),
        assay: String::new(),
        turns: vec![Turn { variant, refactor: Refactor::NotApplicable, response: None }],
    };
    session::tsv(&session::replay(&s, ops))
}

fn number<T: std::str::FromStr>(name: &str, s: Option<&&str>) -> Result<T, String> {
    s.and_then(|v| v.parse().ok()).ok_or_else(|| format!("{name} must be a non-negative integer"))
}

fn dispatch(request: &str) -> Result<(i32, String), String> {
    let (head, body) = request.split_once('\n').unwrap_or((request, ""));
    let words: Vec<&str> = head.split_whitespace().collect();
    match words.first().copied() {
        Some("sim") => {
            let variant = words.get(1).and_then(|n| Variant::parse(n)).ok_or("unknown variant")?;
            let seed: u64 = number("seed", words.get(2))?;
            let ops: usize = number("ops", words.get(3))?;
            let runs: u64 = number("runs", words.get(4))?;
            if !(1..=MAX_OPS).contains(&ops) || !(1..=MAX_RUNS).contains(&runs) {
                return Err(format!("ops must be 1-{MAX_OPS} and runs 1-{MAX_RUNS}"));
            }
            let report = sim::simulate(variant, seed, runs, ops);
            let shown = match &report.first_failure {
                Some(x) => x.shrunk.clone(),
                None => sim::generate(seed, ops).1,
            };
            let code = match report.verdict() {
                sim::Verdict::Pass => 0,
                sim::Verdict::Fail => 1,
                sim::Verdict::Weak => 3,
            };
            Ok((code, format!("{report}{SEP}{}", trace(variant, &shown))))
        }
        Some("race") => {
            let variant = words.get(1).and_then(|n| RaceVariant::parse(n)).ok_or("unknown race variant")?;
            let seed: u64 = number("seed", words.get(2))?;
            let runs: u64 = number("runs", words.get(3))?;
            if !(1..=MAX_RUNS).contains(&runs) {
                return Err(format!("runs must be 1-{MAX_RUNS}"));
            }
            let report = race::simulate(variant, seed, runs);
            let file = match &report.first {
                Some((s, _, reason, shrunk)) => {
                    race::race_text(variant, shrunk, &format!("Shrunk from seed {s}; the unshrunk run failed with: {reason}."))
                }
                None => {
                    let (_, sc) = race::generate(seed);
                    let realized = race::Scenario { schedule: race::execute(variant, &sc, false).taken, ..sc };
                    race::race_text(variant, &realized, &format!("Seed {seed}, which did not fail."))
                }
            };
            Ok((i32::from(report.exit_code()), format!("{report}{SEP}{file}")))
        }
        Some("replay") => {
            let (variant, scenario) = race::parse_race(body)?;
            let code = if race::execute(variant, &scenario, false).failure.is_some() { 1 } else { 0 };
            Ok((code, race::narrate(variant, &scenario)))
        }
        _ => Err("expected sim, race or replay".into()),
    }
}

pub fn call(request: &str) -> (i32, String) {
    match dispatch(request) {
        Ok(x) => x,
        Err(e) => (2, format!("error: {e}\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_reports_and_traces_the_shrunk_failure() {
        let (code, out) = call("sim stale-index 1 48 200");
        assert_eq!(code, 1);
        let (report, trace) = out.split_once(SEP).unwrap();
        assert_eq!(report, include_str!("../sim/stale-index.expected"));
        assert_eq!(trace.lines().count(), 4, "header and three shrunk operations");
        assert!(trace.lines().last().unwrap().contains("\tFAIL\toriginal window"));
    }

    #[test]
    fn race_returns_a_replayable_file() {
        let (code, out) = call("race torn-move 1 1000");
        assert_eq!(code, 1);
        let (report, file) = out.split_once(SEP).unwrap();
        assert_eq!(report, include_str!("../sim/race-torn-move.expected"));
        let (code, narration) = call(&format!("replay\n{file}"));
        assert_eq!(code, 1);
        assert!(narration.ends_with("verdict\tnot linearizable: no order of these 3 operations that respects real time reproduces their results\n"));
        let (code, _) = call(&format!("replay\n{}", file.replace("variant torn-move", "variant locked")));
        assert_eq!(code, 0, "the lock fixes it");
    }

    #[test]
    fn passing_runs_still_return_something_to_look_at() {
        let (code, out) = call("race locked 1 1");
        assert_eq!(code, race::simulate(RaceVariant::Locked, 1, 1).exit_code().into());
        assert_ne!(code, 1, "the control does not fail");
        let (_, file) = out.split_once(SEP).unwrap();
        assert!(file.contains("Seed 1, which did not fail."));
        assert_eq!(call(&format!("replay\n{file}")).0, 0);
        let (_, out) = call("sim indexed 1 16 1");
        assert_eq!(out.split_once(SEP).unwrap().1.lines().count(), 17);
    }

    #[test]
    fn bad_requests_are_status_2() {
        for bad in ["", "fly", "sim nope 1 2 3", "sim indexed 1 0 1", "sim indexed 1 1 999999", "race nope 1 1", "race locked x 1", "replay\nvariant locked"] {
            let (code, out) = call(bad);
            assert_eq!(code, 2, "{bad:?}");
            assert!(out.starts_with("error: "), "{bad:?}");
        }
    }
}
