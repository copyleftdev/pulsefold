//! Shared vocabulary: integer mission ticks, half-open windows, chambers and bookings.
use std::{collections::BTreeMap, fmt};

pub type Tick = u16;
pub const CHAMBERS: usize = 2;

/// A half-open interval `[start, end)` of mission ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub start: Tick,
    pub end: Tick,
}

impl Window {
    pub fn overlaps(self, rhs: Self) -> bool {
        self.start < rhs.end && rhs.start < self.end
    }
}

impl fmt::Display for Window {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub chamber: usize,
    pub window: Window,
}

impl Slot {
    pub fn overlaps(self, rhs: Self) -> bool {
        self.chamber == rhs.chamber && self.window.overlaps(rhs.window)
    }
}

/// Bookings keyed by ID, so every listing has one canonical order.
pub type Bookings = BTreeMap<String, Slot>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Reserve(String, Slot),
    Move(String, Window),
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::Reserve(id, s) => write!(f, "reserve {id} {} {} {}", s.chamber, s.window.start, s.window.end),
            Op::Move(id, w) => write!(f, "move {id} {} {}", w.start, w.end),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Duplicate,
    Conflict,
    NotFound,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Ok => "OK",
            Outcome::Duplicate => "DUPLICATE",
            Outcome::Conflict => "CONFLICT",
            Outcome::NotFound => "NOT_FOUND",
        }
    }
}

pub fn chamber_name(chamber: usize) -> String {
    format!("C{}", chamber + 1)
}

/// `A@C1:600-660 B@C1:720-780`, or `none`.
pub fn format_bookings(bookings: &Bookings) -> String {
    if bookings.is_empty() {
        return "none".into();
    }
    bookings
        .iter()
        .map(|(id, s)| format!("{id}@{}:{}", chamber_name(s.chamber), s.window))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Index entries as stored, possibly repeating an ID (a leftover entry) or filed under the
/// wrong chamber. Sorted by ID, then chamber, then window, so output is stable.
pub type Entries = Vec<(String, Slot)>;

pub fn sort_entries(mut entries: Entries) -> Entries {
    entries.sort_by(|(a, x), (b, y)| (a, x.chamber, x.window.start, x.window.end).cmp(&(b, y.chamber, y.window.start, y.window.end)));
    entries
}

pub fn entries_of(bookings: &Bookings) -> Entries {
    bookings.iter().map(|(k, v)| (k.clone(), *v)).collect()
}

/// `A@C1:600-660 B@C1:720-780`, or `none`; an ID can repeat.
pub fn format_entries(entries: &Entries) -> String {
    if entries.is_empty() {
        return "none".into();
    }
    entries
        .iter()
        .map(|(id, s)| format!("{id}@{}:{}", chamber_name(s.chamber), s.window))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `C1,C2`, or `none` when no chamber is free.
pub fn format_rooms(rooms: &[usize]) -> String {
    if rooms.is_empty() {
        return "none".into();
    }
    rooms.iter().map(|&c| chamber_name(c)).collect::<Vec<_>>().join(",")
}
