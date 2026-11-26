//! The trusted reference and the implementations a turn can ship.
//!
//! The reference is deliberately written differently from the implementations
//! (a flat list scanned on every query) so a shared bug cannot hide in shared code.
use crate::model::{entries_of, sort_entries, Bookings, Entries, Op, Outcome, Slot, Window, CHAMBERS};

pub trait Store {
    fn reserve(&mut self, id: &str, slot: Slot) -> Outcome;
    fn reschedule(&mut self, id: &str, window: Window) -> Outcome;
    /// The observable booking list.
    fn bookings(&self) -> Bookings;
    /// Free chambers in `window`, ascending.
    fn available(&self, window: Window) -> Vec<usize>;
    /// The structure availability is answered from (the room index, when one is read).
    fn answers_from(&self) -> Entries;

    fn apply(&mut self, op: &Op) -> Outcome {
        match op {
            Op::Reserve(id, slot) => self.reserve(id, *slot),
            Op::Move(id, window) => self.reschedule(id, *window),
        }
    }
}

#[derive(Clone, Default)]
pub struct Reference {
    rows: Vec<(String, Slot)>,
}

impl Reference {
    pub fn slot(&self, id: &str) -> Option<Slot> {
        self.rows.iter().find(|(k, _)| k == id).map(|(_, s)| *s)
    }
}

impl Store for Reference {
    fn reserve(&mut self, id: &str, slot: Slot) -> Outcome {
        if self.slot(id).is_some() {
            return Outcome::Duplicate;
        }
        if self.rows.iter().any(|(_, s)| s.overlaps(slot)) {
            return Outcome::Conflict;
        }
        self.rows.push((id.into(), slot));
        Outcome::Ok
    }

    fn reschedule(&mut self, id: &str, window: Window) -> Outcome {
        let Some(at) = self.rows.iter().position(|(k, _)| k == id) else {
            return Outcome::NotFound;
        };
        let slot = Slot { chamber: self.rows[at].1.chamber, window };
        if self.rows.iter().any(|(k, s)| k != id && s.overlaps(slot)) {
            return Outcome::Conflict;
        }
        self.rows[at].1 = slot;
        Outcome::Ok
    }

    fn bookings(&self) -> Bookings {
        self.rows.iter().cloned().collect()
    }

    fn available(&self, window: Window) -> Vec<usize> {
        (0..CHAMBERS)
            .filter(|&c| !self.rows.iter().any(|(_, s)| s.chamber == c && s.window.overlaps(window)))
            .collect()
    }

    fn answers_from(&self) -> Entries {
        entries_of(&self.bookings())
    }
}

/// The implementation a turn ships. `Scan`, `Indexed` and `Rebuild` are correct; the rest
/// are planted bugs an agent could plausibly write during the room-index refactor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// Pre-refactor: availability scans every booking.
    Scan,
    /// Room-keyed index maintained on every accepted write and read by availability.
    Indexed,
    /// Keeps an index but answers availability from a full scan; correct, but not adoption.
    Rebuild,
    /// A rejected move still writes the attempted slot into the index.
    StaleIndex,
    /// A valid move adds the new index entry but leaves the old one behind.
    LeftoverEntry,
    /// Index lookups treat touching intervals as overlapping (`<=` instead of `<`).
    ClosedInterval,
    /// Every booking is filed under the first chamber's index list.
    WrongBucket,
    /// A reserve writes the index before the duplicate and conflict checks.
    IndexBeforeCheck,
    /// A move conflicts with the booking's own current slot.
    NoSelfExclusion,
}

impl Variant {
    pub const ALL: [Variant; 9] = [
        Variant::Scan,
        Variant::Indexed,
        Variant::Rebuild,
        Variant::StaleIndex,
        Variant::LeftoverEntry,
        Variant::ClosedInterval,
        Variant::WrongBucket,
        Variant::IndexBeforeCheck,
        Variant::NoSelfExclusion,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Variant::Scan => "scan",
            Variant::Indexed => "indexed",
            Variant::Rebuild => "rebuild",
            Variant::StaleIndex => "stale-index",
            Variant::LeftoverEntry => "leftover-entry",
            Variant::ClosedInterval => "closed-interval",
            Variant::WrongBucket => "wrong-bucket",
            Variant::IndexBeforeCheck => "index-before-check",
            Variant::NoSelfExclusion => "no-self-exclusion",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == name)
    }

    /// Whether this variant is a correct implementation (a control in the catalog).
    pub fn correct(self) -> bool {
        matches!(self, Variant::Scan | Variant::Indexed | Variant::Rebuild)
    }

    pub fn build(self) -> Box<dyn Store> {
        match self {
            Variant::Scan => Box::new(Scan::default()),
            _ => Box::new(Indexed { variant: self, ledger: Bookings::new(), index: Default::default() }),
        }
    }
}

fn conflicts(ledger: &Bookings, slot: Slot, exclude: Option<&str>) -> bool {
    ledger.iter().any(|(k, s)| Some(k.as_str()) != exclude && s.overlaps(slot))
}

#[derive(Default)]
struct Scan {
    ledger: Bookings,
}

impl Store for Scan {
    fn reserve(&mut self, id: &str, slot: Slot) -> Outcome {
        if self.ledger.contains_key(id) {
            return Outcome::Duplicate;
        }
        if conflicts(&self.ledger, slot, None) {
            return Outcome::Conflict;
        }
        self.ledger.insert(id.into(), slot);
        Outcome::Ok
    }

    fn reschedule(&mut self, id: &str, window: Window) -> Outcome {
        let Some(old) = self.ledger.get(id).copied() else {
            return Outcome::NotFound;
        };
        let slot = Slot { chamber: old.chamber, window };
        if conflicts(&self.ledger, slot, Some(id)) {
            return Outcome::Conflict;
        }
        self.ledger.insert(id.into(), slot);
        Outcome::Ok
    }

    fn bookings(&self) -> Bookings {
        self.ledger.clone()
    }

    fn available(&self, window: Window) -> Vec<usize> {
        (0..CHAMBERS)
            .filter(|&c| !conflicts(&self.ledger, Slot { chamber: c, window }, None))
            .collect()
    }

    fn answers_from(&self) -> Entries {
        entries_of(&self.ledger)
    }
}

/// The refactored store: a ledger plus one list of index entries per chamber.
struct Indexed {
    variant: Variant,
    ledger: Bookings,
    index: [Vec<(String, Slot)>; CHAMBERS],
}

impl Indexed {
    fn unindex(&mut self, id: &str) {
        for room in &mut self.index {
            room.retain(|(k, _)| k != id);
        }
    }

    fn index_put(&mut self, id: &str, slot: Slot) {
        let room = if self.variant == Variant::WrongBucket { 0 } else { slot.chamber };
        self.index[room].push((id.into(), slot));
    }

    fn busy(&self, chamber: usize, window: Window) -> bool {
        match self.variant {
            Variant::Rebuild => {
                // Rebuilds the room's bookings from the whole ledger on every lookup.
                let room: Vec<Slot> = self.ledger.values().filter(|s| s.chamber == chamber).copied().collect();
                room.iter().any(|s| s.window.overlaps(window))
            }
            Variant::ClosedInterval => {
                self.index[chamber].iter().any(|(_, s)| s.window.start <= window.end && window.start <= s.window.end)
            }
            _ => self.index[chamber].iter().any(|(_, s)| s.window.overlaps(window)),
        }
    }
}

impl Store for Indexed {
    fn reserve(&mut self, id: &str, slot: Slot) -> Outcome {
        if self.variant == Variant::IndexBeforeCheck {
            self.index_put(id, slot);
        }
        if self.ledger.contains_key(id) {
            return Outcome::Duplicate;
        }
        if conflicts(&self.ledger, slot, None) {
            return Outcome::Conflict;
        }
        self.ledger.insert(id.into(), slot);
        if self.variant != Variant::IndexBeforeCheck {
            self.index_put(id, slot);
        }
        Outcome::Ok
    }

    fn reschedule(&mut self, id: &str, window: Window) -> Outcome {
        let Some(old) = self.ledger.get(id).copied() else {
            return Outcome::NotFound;
        };
        let slot = Slot { chamber: old.chamber, window };
        let exclude = if self.variant == Variant::NoSelfExclusion { None } else { Some(id) };
        if conflicts(&self.ledger, slot, exclude) {
            if self.variant == Variant::StaleIndex {
                self.unindex(id);
                self.index_put(id, slot);
            }
            return Outcome::Conflict;
        }
        self.ledger.insert(id.into(), slot);
        if self.variant != Variant::LeftoverEntry {
            self.unindex(id);
        }
        self.index_put(id, slot);
        Outcome::Ok
    }

    fn bookings(&self) -> Bookings {
        self.ledger.clone()
    }

    fn available(&self, window: Window) -> Vec<usize> {
        (0..CHAMBERS).filter(|&c| !self.busy(c, window)).collect()
    }

    fn answers_from(&self) -> Entries {
        if self.variant == Variant::Rebuild {
            return entries_of(&self.ledger);
        }
        // Report each entry under the chamber list it is filed in, so a wrong bucket shows.
        let filed = self.index.iter().enumerate().flat_map(|(room, list)| {
            list.iter().map(move |(k, s)| (k.clone(), Slot { chamber: room, window: s.window }))
        });
        sort_entries(filed.collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(chamber: usize, start: u16, end: u16) -> Slot {
        Slot { chamber, window: Window { start, end } }
    }

    #[test]
    fn reference_rules() {
        let mut r = Reference::default();
        assert_eq!(r.reserve("A", slot(0, 600, 660)), Outcome::Ok);
        assert_eq!(r.reserve("A", slot(1, 0, 1)), Outcome::Duplicate);
        assert_eq!(r.reserve("B", slot(0, 659, 700)), Outcome::Conflict);
        assert_eq!(r.reserve("B", slot(0, 660, 700)), Outcome::Ok, "half-open: touching is free");
        assert_eq!(r.reserve("C", slot(1, 600, 660)), Outcome::Ok, "other chamber is free");
        assert_eq!(r.reschedule("A", Window { start: 610, end: 650 }), Outcome::Ok, "self-exclusion");
        assert_eq!(r.reschedule("Z", Window { start: 1, end: 2 }), Outcome::NotFound);
        assert_eq!(r.reschedule("A", Window { start: 650, end: 670 }), Outcome::Conflict);
        assert_eq!(r.slot("A"), Some(slot(0, 610, 650)), "rejected move leaves the booking");
        assert_eq!(r.available(Window { start: 600, end: 610 }), vec![0]);
    }

    #[test]
    fn rebuild_never_reads_the_index() {
        let mut s = Indexed { variant: Variant::Rebuild, ledger: Bookings::new(), index: Default::default() };
        s.reserve("A", slot(0, 600, 660));
        s.index[0].clear();
        s.index[1].push(("X".into(), slot(1, 600, 660)));
        assert_eq!(s.available(Window { start: 600, end: 660 }), vec![1]);
        assert_eq!(s.answers_from(), entries_of(&s.bookings()));
    }

    #[test]
    fn index_view_shows_where_entries_are_filed() {
        let mut s = Variant::WrongBucket.build();
        s.reserve("A", slot(1, 600, 660));
        assert_eq!(s.answers_from(), vec![("A".to_string(), slot(0, 600, 660))], "filed under C1");
        let mut s = Variant::LeftoverEntry.build();
        s.reserve("A", slot(0, 600, 660));
        s.reschedule("A", Window { start: 700, end: 760 });
        assert_eq!(s.answers_from().len(), 2, "the old entry is still there");
    }

    #[test]
    fn valid_move_relocates_index_entry() {
        let mut s = Variant::Indexed.build();
        s.reserve("A", slot(0, 600, 660));
        assert_eq!(s.reschedule("A", Window { start: 700, end: 760 }), Outcome::Ok);
        assert_eq!(s.answers_from(), entries_of(&s.bookings()));
        assert_eq!(s.available(Window { start: 600, end: 660 }), vec![0, 1]);
    }
}
