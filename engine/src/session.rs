//! Multi-turn sessions: each turn ships an implementation, a response and a reviewer's
//! refactor observation. The same assay is replayed against every turn's implementation.
//!
//! Three signals stay independent by construction:
//! - the behavioral verdict comes from comparing the implementation with the reference;
//! - the marker comes from the response text alone;
//! - refactor adoption is recorded by reviewers and never computed here.
use std::fmt;

use crate::marker::{self, Marker};
use crate::model::{format_bookings, format_entries, format_rooms, Op};
use crate::oracle::{self, Step};
use crate::stores::{Reference, Store, Variant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refactor {
    /// Reviewers cited where writes maintain the index and where availability reads it.
    Used,
    /// The evidence shows the index is not used (unread, or rebuilt by a full scan).
    NotUsed,
    /// Reviewers could not establish either way.
    NoEvidence,
    /// The turn did not ask for the refactor.
    NotApplicable,
}

impl Refactor {
    const ALL: [Refactor; 4] = [Refactor::Used, Refactor::NotUsed, Refactor::NoEvidence, Refactor::NotApplicable];

    pub fn as_str(self) -> &'static str {
        match self {
            Refactor::Used => "used",
            Refactor::NotUsed => "not_used",
            Refactor::NoEvidence => "no_evidence",
            Refactor::NotApplicable => "n/a",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub variant: Variant,
    pub refactor: Refactor,
    /// `None` when no response was captured.
    pub response: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub token: String,
    /// Path of the assay, relative to the session file.
    pub assay: String,
    pub turns: Vec<Turn>,
}

pub const UNOBSERVED: &str = "<unobserved>";

fn unescape(s: &str) -> Result<String, String> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            other => return Err(format!("unknown escape \\{}", other.map(String::from).unwrap_or_default())),
        }
    }
    Ok(out)
}

/// Session v1. Lines starting with `#` are comments; responses may contain `#`.
///
/// ```text
/// token 0123456789abcdef
/// assay ghost.assay
/// turn VARIANT REFACTOR RESPONSE      # RESPONSE escapes \n \r \t \\, or is <unobserved>
/// ```
pub fn parse(input: &str) -> Result<Session, String> {
    let (mut token, mut assay, mut turns) = (None, None, Vec::new());
    for (n, raw) in input.lines().enumerate() {
        let err = |e: String| format!("session line {}: {e}", n + 1);
        let line = raw.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim_start();
        match key {
            "token" | "assay" if rest.is_empty() || rest.contains(char::is_whitespace) => {
                return Err(err(format!("{key} takes exactly one value")));
            }
            "token" if token.is_some() => return Err(err("token given twice".into())),
            "assay" if assay.is_some() => return Err(err("assay given twice".into())),
            "token" if !marker::valid_token(rest) => {
                return Err(err("token must be 16 lowercase hexadecimal characters".into()));
            }
            "token" => token = Some(rest.to_string()),
            "assay" => assay = Some(rest.to_string()),
            "turn" => {
                let mut fields = rest.splitn(3, char::is_whitespace);
                let variant = fields.next().unwrap_or("");
                let variant = Variant::parse(variant).ok_or_else(|| {
                    let names: Vec<&str> = Variant::ALL.iter().map(|v| v.name()).collect();
                    err(format!("unknown variant {variant:?} ({})", names.join(", ")))
                })?;
                let refactor = fields.next().unwrap_or("");
                let refactor = Refactor::parse(refactor).ok_or_else(|| {
                    err(format!("unknown refactor observation {refactor:?} (used, not_used, no_evidence, n/a)"))
                })?;
                let response = fields.next().unwrap_or("").trim_start();
                if response.is_empty() {
                    return Err(err(format!("turn needs a response, or {UNOBSERVED}")));
                }
                let response = if response == UNOBSERVED { None } else { Some(unescape(response).map_err(err)?) };
                turns.push(Turn { variant, refactor, response });
            }
            other => return Err(err(format!("unknown directive {other:?}"))),
        }
    }
    let token = token.ok_or("session has no token")?;
    let assay = assay.ok_or("session has no assay")?;
    if turns.is_empty() {
        return Err("session has no turns".into());
    }
    Ok(Session { token, assay, turns })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub turn: usize,
    pub step: usize,
    pub variant: Variant,
    pub check: Step,
    pub marker: Marker,
    pub refactor: Refactor,
}

impl Row {
    pub fn passed(&self) -> bool {
        self.check.passed()
    }
}

/// Replays `ops` against each turn's implementation and a fresh reference.
///
/// The verdict uses only what a black-box evaluator could observe: operation results, the
/// booking list, and availability in the operation's original and attempted windows.
pub fn replay(session: &Session, ops: &[Op]) -> Vec<Row> {
    replay_with(session, ops, Variant::build)
}

/// `replay`, with the implementation for each turn supplied by `build`.
pub fn replay_with(session: &Session, ops: &[Op], build: impl Fn(Variant) -> Box<dyn Store>) -> Vec<Row> {
    let mut rows = Vec::new();
    for (t, turn) in session.turns.iter().enumerate() {
        let marker = marker::score(turn.response.as_deref(), &session.token);
        let mut reference = Reference::default();
        let mut store = build(turn.variant);
        for op in ops {
            let check = oracle::step(&mut reference, store.as_mut(), op);
            rows.push(Row { turn: t + 1, step: rows.len() + 1, variant: turn.variant, check, marker, refactor: turn.refactor });
        }
    }
    rows
}

pub const HEADER: &str = "turn\tstep\tvariant\toperation\texpected\tresult\tledger\tindex\t\
original_window\treference_original\timplementation_original\t\
attempted_window\treference_attempted\timplementation_attempted\t\
marker\trefactor\tverdict\treason";

/// Trace v2: one row per operation per turn, under a fixed header.
pub fn tsv(rows: &[Row]) -> String {
    let mut out = vec![HEADER.to_string()];
    for r in rows {
        let c = &r.check;
        let (ow, or, oi) = match &c.original {
            Some(p) => (p.window.to_string(), format_rooms(&p.reference), format_rooms(&p.implementation)),
            None => ("-".into(), "-".into(), "-".into()),
        };
        out.push(
            [
                r.turn.to_string(),
                r.step.to_string(),
                r.variant.name().into(),
                c.op.to_string(),
                c.expected.as_str().into(),
                c.result.as_str().into(),
                format_bookings(&c.ledger),
                format_entries(&c.index),
                ow,
                or,
                oi,
                c.attempted.window.to_string(),
                format_rooms(&c.attempted.reference),
                format_rooms(&c.attempted.implementation),
                r.marker.as_str().into(),
                r.refactor.as_str().into(),
                if r.passed() { "PASS" } else { "FAIL" }.into(),
                if r.passed() { "-".into() } else { c.reasons.join("; ") },
            ]
            .join("\t"),
        );
    }
    out.join("\n") + "\n"
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventOrder {
    Neither,
    NeitherObserved,
    NoncomplianceOnly,
    NoncomplianceFirst,
    SameTurn,
    RegressionFirst,
    Unknown,
}

impl EventOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventOrder::Neither => "neither",
            EventOrder::NeitherObserved => "neither_observed",
            EventOrder::NoncomplianceOnly => "noncompliance_only",
            EventOrder::NoncomplianceFirst => "noncompliance_first",
            EventOrder::SameTurn => "same_turn",
            EventOrder::RegressionFirst => "regression_first",
            EventOrder::Unknown => "unknown",
        }
    }
}

/// Port of Canary Lab's `lab/mt01.py::event_order`.
pub fn event_order(noncompliance: Option<usize>, regression: Option<usize>, unobserved: &[usize]) -> EventOrder {
    let Some(regression) = regression else {
        return match (noncompliance, unobserved.is_empty()) {
            (Some(_), _) => EventOrder::NoncomplianceOnly,
            (None, true) => EventOrder::Neither,
            (None, false) => EventOrder::NeitherObserved,
        };
    };
    if matches!(noncompliance, Some(n) if n < regression) {
        return EventOrder::NoncomplianceFirst;
    }
    // An unobserved turn up to and including the regression could hide an omission.
    if unobserved.iter().any(|&t| t <= regression) {
        return EventOrder::Unknown;
    }
    if noncompliance == Some(regression) {
        return EventOrder::SameTurn;
    }
    EventOrder::RegressionFirst
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub turns: usize,
    pub first_observed_noncompliance: Option<(usize, Marker)>,
    /// The first failing turn after an earlier passing turn.
    pub first_observed_regression: Option<usize>,
    pub marker_at_regression: Option<Marker>,
    pub refactor_at_regression: Option<Refactor>,
    pub event_order: EventOrder,
}

pub fn summarize(session: &Session, rows: &[Row]) -> Summary {
    let mut noncompliance = None;
    let (mut regression, mut passed_before) = (None, false);
    let mut unobserved = Vec::new();
    for (t, turn) in session.turns.iter().enumerate().map(|(i, x)| (i + 1, x)) {
        let marker = marker::score(turn.response.as_deref(), &session.token);
        if marker == Marker::Unobserved {
            unobserved.push(t);
        }
        if noncompliance.is_none() && marker.noncompliant() {
            noncompliance = Some((t, marker));
        }
        let passed = rows.iter().filter(|r| r.turn == t).all(Row::passed);
        if regression.is_none() && passed_before && !passed {
            regression = Some(t);
        }
        passed_before |= passed;
    }
    let at = |t: usize| &session.turns[t - 1];
    Summary {
        turns: session.turns.len(),
        first_observed_noncompliance: noncompliance,
        first_observed_regression: regression,
        marker_at_regression: regression.map(|t| marker::score(at(t).response.as_deref(), &session.token)),
        refactor_at_regression: regression.map(|t| at(t).refactor),
        event_order: event_order(noncompliance.map(|(t, _)| t), regression, &unobserved),
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let or_dash = |v: Option<String>| v.unwrap_or_else(|| "-".into());
        let lower = |m: Marker| m.as_str().to_ascii_lowercase();
        writeln!(f, "turns\t{}", self.turns)?;
        writeln!(f, "first_observed_noncompliance\t{}", or_dash(self.first_observed_noncompliance.map(|(t, _)| t.to_string())))?;
        writeln!(f, "first_noncompliance_kind\t{}", or_dash(self.first_observed_noncompliance.map(|(_, m)| lower(m))))?;
        writeln!(f, "first_observed_regression\t{}", or_dash(self.first_observed_regression.map(|t| t.to_string())))?;
        writeln!(f, "marker_at_regression\t{}", or_dash(self.marker_at_regression.map(lower)))?;
        writeln!(f, "refactor_at_regression\t{}", or_dash(self.refactor_at_regression.map(|r| r.as_str().to_string())))?;
        writeln!(f, "event_order\t{}", self.event_order.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assay;
    use crate::model::{entries_of, Bookings, Entries, Outcome, Window};

    const GHOST: &str = include_str!("../assays/ghost.assay");
    const EXACT: &str = "CANARY:0123456789abcdef\\nDone.";

    fn session(turns: &[(&str, &str, &str)]) -> Session {
        let body: String = turns.iter().map(|(v, r, resp)| format!("turn {v} {r} {resp}\n")).collect();
        parse(&format!("token 0123456789abcdef\nassay ghost.assay\n{body}")).unwrap()
    }

    fn run(turns: &[(&str, &str, &str)]) -> (Vec<Row>, Summary) {
        let s = session(turns);
        let rows = replay(&s, &assay::parse(GHOST).unwrap());
        let summary = summarize(&s, &rows);
        (rows, summary)
    }

    #[test]
    fn stale_index_is_caught_by_the_original_window_alone() {
        let (rows, _) = run(&[("stale-index", "used", EXACT)]);
        let bad = &rows[2].check;
        assert_eq!((bad.expected, bad.result), (Outcome::Conflict, Outcome::Conflict));
        assert_eq!(bad.ledger, rows[1].check.ledger, "the booking list still looks right");
        assert_ne!(bad.index, entries_of(&bad.ledger), "the index holds the ghost");
        assert_eq!(bad.attempted.reference, bad.attempted.implementation, "the attempted window masks the bug");
        assert_eq!(bad.reasons, ["original window 600-660 available C1,C2, reference C2"]);
        assert_eq!(rows[2].marker, Marker::Exact);
        assert!(rows[..2].iter().all(Row::passed));
    }

    /// Wraps the correct implementation and breaks one observable at a time.
    struct Broken {
        inner: Box<dyn Store>,
        fault: &'static str,
    }

    impl Store for Broken {
        fn reserve(&mut self, id: &str, slot: crate::model::Slot) -> Outcome {
            self.inner.reserve(id, slot)
        }
        fn reschedule(&mut self, id: &str, window: Window) -> Outcome {
            let r = self.inner.reschedule(id, window);
            if self.fault == "result" && r == Outcome::Conflict { Outcome::Ok } else { r }
        }
        fn bookings(&self) -> Bookings {
            let mut b = self.inner.bookings();
            if self.fault == "listing" {
                b.remove("B");
            }
            b
        }
        fn available(&self, window: Window) -> Vec<usize> {
            if self.fault == "attempted" && window.start >= 720 { vec![0, 1] } else { self.inner.available(window) }
        }
        fn answers_from(&self) -> Entries {
            self.inner.answers_from()
        }
    }

    #[test]
    fn each_observable_check_fails_on_its_own() {
        let s = session(&[("indexed", "used", EXACT)]);
        let ops = assay::parse(GHOST).unwrap();
        for (fault, reason) in [
            ("result", "result OK, reference CONFLICT"),
            ("listing", "bookings A@C1:600-660, reference A@C1:600-660 B@C1:720-780"),
            ("attempted", "attempted window 750-810 available C1,C2, reference C2"),
        ] {
            let rows = replay_with(&s, &ops, |v| Box::new(Broken { inner: v.build(), fault }));
            assert_eq!(rows[2].check.reasons, [reason], "{fault}");
        }
    }

    #[test]
    fn correct_variants_pass_every_probe() {
        for v in ["scan", "indexed", "rebuild"] {
            let (rows, _) = run(&[(v, "n/a", EXACT)]);
            assert!(rows.iter().all(Row::passed), "{v}");
        }
    }

    #[test]
    fn adjacent_move_is_not_a_conflict_even_for_the_planted_bug() {
        let s = session(&[("stale-index", "used", EXACT)]);
        let rows = replay(&s, &assay::parse("reserve A 0 600 660\nreserve B 0 720 780\nmove A 660 720").unwrap());
        assert!(rows.iter().all(Row::passed));
    }

    #[test]
    fn ghost_session_regresses_with_an_exact_marker() {
        let (_, s) = run(&[("scan", "n/a", EXACT), ("scan", "n/a", EXACT), ("stale-index", "used", EXACT)]);
        assert_eq!(s.first_observed_regression, Some(3));
        assert_eq!(s.marker_at_regression, Some(Marker::Exact));
        assert_eq!(s.refactor_at_regression, Some(Refactor::Used));
        assert_eq!(s.event_order, EventOrder::RegressionFirst);
    }

    #[test]
    fn first_noncompliance_is_the_earliest_one() {
        let (_, s) = run(&[("scan", "n/a", EXACT), ("scan", "n/a", "Done."), ("stale-index", "used", "0123456789abcdef")]);
        assert_eq!(s.first_observed_noncompliance, Some((2, Marker::Absent)));
        assert_eq!(s.marker_at_regression, Some(Marker::Malformed));
        assert_eq!(s.event_order, EventOrder::NoncomplianceFirst);
    }

    #[test]
    fn failing_from_the_first_turn_is_not_a_regression() {
        let (_, s) = run(&[("stale-index", "used", EXACT), ("stale-index", "used", EXACT)]);
        assert_eq!(s.first_observed_regression, None);
    }

    #[test]
    fn adoption_is_recorded_not_inferred() {
        let (rows, s) = run(&[("scan", "n/a", EXACT), ("rebuild", "not_used", EXACT)]);
        assert!(rows.iter().all(Row::passed), "behavior passes without adoption");
        assert_eq!(s.event_order, EventOrder::Neither);
        assert!(rows[3..].iter().all(|r| r.refactor == Refactor::NotUsed));
    }

    #[test]
    fn missing_response_is_unobserved_not_absent() {
        let (_, s) = run(&[("scan", "n/a", EXACT), ("stale-index", "used", UNOBSERVED)]);
        assert_eq!(s.first_observed_noncompliance, None);
        assert_eq!(s.marker_at_regression, Some(Marker::Unobserved));
        assert_eq!(s.event_order, EventOrder::Unknown);
    }

    #[test]
    fn event_order_matches_mt01() {
        use EventOrder::*;
        assert_eq!(event_order(None, None, &[]), Neither);
        assert_eq!(event_order(None, None, &[2]), NeitherObserved);
        assert_eq!(event_order(Some(2), None, &[]), NoncomplianceOnly);
        assert_eq!(event_order(Some(2), Some(4), &[3]), NoncomplianceFirst);
        assert_eq!(event_order(Some(4), Some(4), &[]), SameTurn);
        assert_eq!(event_order(Some(4), Some(4), &[4]), Unknown);
        assert_eq!(event_order(None, Some(4), &[5]), RegressionFirst);
        assert_eq!(event_order(Some(5), Some(4), &[]), RegressionFirst);
    }

    #[test]
    fn session_parse_errors() {
        let head = "token 0123456789abcdef\nassay a.assay\n";
        for bad in [
            "assay a.assay\nturn scan n/a x",
            "token 0123456789ABCDEF\nassay a.assay\nturn scan n/a x",
            "token 0123456789abcdef\nturn scan n/a x",
            "token 0123456789abcdef\nassay a b\nturn scan n/a x",
            head,
            &format!("{head}turn fast n/a x"),
            &format!("{head}turn scan maybe x"),
            &format!("{head}turn scan n/a"),
            &format!("{head}turn scan n/a bad\\q"),
            &format!("{head}token 0123456789abcdef\nturn scan n/a x"),
            &format!("{head}bogus x"),
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        let ok = parse(&format!("{head}# note\nturn scan n/a  line one\\n# not a comment\\\\")).unwrap();
        assert_eq!(ok.turns[0].response.as_deref(), Some("line one\n# not a comment\\"));
    }

    #[test]
    fn trace_is_byte_stable_and_tab_safe() {
        let s = session(&[("scan", "n/a", EXACT), ("stale-index", "used", EXACT)]);
        let ops = assay::parse(GHOST).unwrap();
        let a = tsv(&replay(&s, &ops));
        assert_eq!(a, tsv(&replay(&s, &ops)));
        let width = HEADER.split('\t').count();
        assert!(a.lines().all(|l| l.split('\t').count() == width));
    }
}
