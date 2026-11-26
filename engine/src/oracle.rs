//! One checked operation: the implementation and the reference take the same step, then
//! every black-box observable is compared. Sessions and the simulator share this.
use crate::model::{format_bookings, format_rooms, Bookings, Entries, Op, Outcome, Tick, Window};
use crate::stores::{Reference, Store};

/// Availability answered by the reference and by the implementation in one probe window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub window: Window,
    pub reference: Vec<usize>,
    pub implementation: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub op: Op,
    pub expected: Outcome,
    pub result: Outcome,
    /// The reference's bookings after the step.
    pub ledger: Bookings,
    /// What the implementation answers availability from after the step.
    pub index: Entries,
    /// The moved booking's slot before the operation, probed after it.
    pub original: Option<Probe>,
    /// The requested or attempted window, probed after the operation.
    pub attempted: Probe,
    pub reasons: Vec<String>,
}

impl Step {
    pub fn passed(&self) -> bool {
        self.reasons.is_empty()
    }
}

/// How much availability the oracle probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The original and attempted windows, plus the sweep when both agree.
    Full,
    /// Only the original and attempted windows (for measuring what the sweep adds).
    NamedOnly,
}

fn probe(reference: &Reference, store: &dyn Store, window: Window) -> Probe {
    Probe { window, reference: reference.available(window), implementation: store.available(window) }
}

/// Every elementary interval between consecutive endpoints of `windows`.
pub fn elementary(windows: impl IntoIterator<Item = Window>) -> Vec<Window> {
    let mut points: Vec<Tick> = windows.into_iter().flat_map(|w| [w.start, w.end]).collect();
    points.sort_unstable();
    points.dedup();
    points.windows(2).map(|p| Window { start: p[0], end: p[1] }).collect()
}

/// Applies `op` to both stores and compares the result, the booking list, and availability
/// in the original and attempted windows. When those two probes agree, a sweep probes every
/// interval between consecutive endpoints of the reference's bookings and the operation's
/// windows, and reports the first disagreement. A stale index entry only ever has endpoints
/// from those sets, so for that class of bug the sweep sees every observable difference.
/// The implementation's index is never inspected directly.
pub fn step(reference: &mut Reference, store: &mut dyn Store, op: &Op) -> Step {
    step_with(reference, store, op, Mode::Full)
}

pub fn step_with(reference: &mut Reference, store: &mut dyn Store, op: &Op, mode: Mode) -> Step {
    let before = match op {
        Op::Move(id, _) => reference.slot(id).map(|s| s.window),
        Op::Reserve(..) => None,
    };
    let expected = reference.apply(op);
    let result = store.apply(op);
    let attempted = match op {
        Op::Reserve(_, slot) => slot.window,
        Op::Move(_, window) => *window,
    };
    let original = before.map(|w| probe(reference, store, w));
    let attempted = probe(reference, store, attempted);
    let mut reasons = Vec::new();
    if result != expected {
        reasons.push(format!("result {}, reference {}", result.as_str(), expected.as_str()));
    }
    let ledger = reference.bookings();
    let listed = store.bookings();
    if listed != ledger {
        reasons.push(format!("bookings {}, reference {}", format_bookings(&listed), format_bookings(&ledger)));
    }
    let mut probes_agree = true;
    for (label, p) in [("original", original.as_ref()), ("attempted", Some(&attempted))] {
        if let Some(p) = p.filter(|p| p.reference != p.implementation) {
            probes_agree = false;
            reasons.push(format!(
                "{label} window {} available {}, reference {}",
                p.window,
                format_rooms(&p.implementation),
                format_rooms(&p.reference)
            ));
        }
    }
    if probes_agree && mode == Mode::Full {
        let windows = ledger.values().map(|s| s.window).chain(before).chain([attempted.window]);
        if let Some(p) = elementary(windows).into_iter().map(|w| probe(reference, store, w)).find(|p| p.reference != p.implementation) {
            reasons.push(format!(
                "sweep window {} available {}, reference {}",
                p.window,
                format_rooms(&p.implementation),
                format_rooms(&p.reference)
            ));
        }
    }
    Step { op: op.clone(), expected, result, ledger, index: store.answers_from(), original, attempted, reasons }
}
