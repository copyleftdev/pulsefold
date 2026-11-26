//! Deterministic concurrency: clients run operations as sequences of atomic micro-steps, a
//! seeded scheduler interleaves them, and every history is checked for linearizability
//! against the sequential reference. Same seed, same interleaving, every time.
use std::collections::{BTreeMap, HashSet};
use std::fmt;

use crate::assay;
use crate::catalog::wilson;
use crate::model::{format_bookings, format_entries, format_rooms, sort_entries, Bookings, Op, Outcome, Slot, Tick, Window, CHAMBERS};
use crate::oracle::elementary;
use crate::rng::Rng;
use crate::stores::{Reference, Store};

/// Concurrent implementations. Each bug is one change away from a correct variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaceVariant {
    /// Every operation, reads included, holds the store lock for its whole duration.
    Locked,
    /// Writers hold the lock; readers do not, but a move updates the index in one step.
    LockedWrites,
    /// `LockedWrites` without the write lock: check, then act, with a gap between.
    CheckThenAct,
    /// `LockedWrites` with a move's index update split into unindex, then index.
    TornMove,
}

impl RaceVariant {
    pub const ALL: [RaceVariant; 4] = [RaceVariant::Locked, RaceVariant::LockedWrites, RaceVariant::CheckThenAct, RaceVariant::TornMove];

    pub fn name(self) -> &'static str {
        match self {
            RaceVariant::Locked => "locked",
            RaceVariant::LockedWrites => "locked-writes",
            RaceVariant::CheckThenAct => "check-then-act",
            RaceVariant::TornMove => "torn-move",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == name)
    }

    pub fn correct(self) -> bool {
        matches!(self, RaceVariant::Locked | RaceVariant::LockedWrites)
    }
}

/// A client operation: the sequential operations plus an availability query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum COp {
    Reserve(String, Slot),
    Move(String, Window),
    Query(Window),
}

impl fmt::Display for COp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            COp::Reserve(id, s) => write!(f, "{}", Op::Reserve(id.clone(), *s)),
            COp::Move(id, w) => write!(f, "{}", Op::Move(id.clone(), *w)),
            COp::Query(w) => write!(f, "query {} {}", w.start, w.end),
        }
    }
}

fn parse_cop(text: &str) -> Result<COp, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.first() == Some(&"query") {
        let tick = |s: &str| s.parse::<Tick>().map_err(|_| format!("bad tick: {s}"));
        return match words.as_slice() {
            [_, lo, hi] => {
                let (start, end) = (tick(lo)?, tick(hi)?);
                if start >= end {
                    return Err("expected start < end".into());
                }
                Ok(COp::Query(Window { start, end }))
            }
            _ => Err("expected query START END".into()),
        };
    }
    match assay::parse(text)?.as_slice() {
        [Op::Reserve(id, s)] => Ok(COp::Reserve(id.clone(), *s)),
        [Op::Move(id, w)] => Ok(COp::Move(id.clone(), *w)),
        _ => Err("expected one operation".into()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    Outcome(Outcome),
    Rooms(Vec<usize>),
}

impl fmt::Display for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Answer::Outcome(o) => f.write_str(o.as_str()),
            Answer::Rooms(r) => f.write_str(&format_rooms(r)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Instr {
    Acquire,
    Check,
    WriteLedger,
    Unindex,
    Index,
    Reindex,
    Read,
    Release,
    Done,
}

impl Instr {
    fn name(self) -> &'static str {
        match self {
            Instr::Acquire => "acquire",
            Instr::Check => "check",
            Instr::WriteLedger => "write-ledger",
            Instr::Unindex => "unindex",
            Instr::Index => "index",
            Instr::Reindex => "reindex",
            Instr::Read => "read",
            Instr::Release => "release",
            Instr::Done => "done",
        }
    }
}

/// The micro-steps an operation runs as, for a variant.
fn program(v: RaceVariant, op: &COp) -> Vec<Instr> {
    use Instr::*;
    let wrap = |lock: bool, body: &[Instr]| {
        let mut p = Vec::new();
        if lock {
            p.push(Acquire);
        }
        p.extend_from_slice(body);
        if lock {
            p.push(Release);
        }
        p.push(Done);
        p
    };
    let lock_writes = v != RaceVariant::CheckThenAct;
    let split_move = matches!(v, RaceVariant::Locked | RaceVariant::TornMove);
    match op {
        COp::Query(_) => wrap(v == RaceVariant::Locked, &[Read]),
        COp::Reserve(..) => wrap(lock_writes, &[Check, WriteLedger, Index]),
        COp::Move(..) if split_move => wrap(lock_writes, &[Check, WriteLedger, Unindex, Index]),
        COp::Move(..) => wrap(lock_writes, &[Check, WriteLedger, Reindex]),
    }
}

#[derive(Clone, Default)]
struct Shared {
    ledger: Bookings,
    index: [Vec<(String, Slot)>; CHAMBERS],
    lock: Option<usize>,
}

impl Shared {
    fn conflicts(&self, slot: Slot, exclude: Option<&str>) -> bool {
        self.ledger.iter().any(|(k, s)| Some(k.as_str()) != exclude && s.overlaps(slot))
    }

    /// Availability, answered from the index.
    fn read(&self, w: Window) -> Vec<usize> {
        (0..CHAMBERS).filter(|&c| !self.index[c].iter().any(|(_, s)| s.window.overlaps(w))).collect()
    }

    fn unindex(&mut self, id: &str) {
        for room in &mut self.index {
            room.retain(|(k, _)| k != id);
        }
    }

    /// The decision a check makes, and the slot an accepted write will store.
    fn decide(&self, op: &COp) -> (Outcome, Option<Slot>) {
        match op {
            COp::Reserve(id, _) if self.ledger.contains_key(id) => (Outcome::Duplicate, None),
            COp::Reserve(_, slot) if self.conflicts(*slot, None) => (Outcome::Conflict, None),
            COp::Reserve(_, slot) => (Outcome::Ok, Some(*slot)),
            COp::Move(id, w) => match self.ledger.get(id) {
                None => (Outcome::NotFound, None),
                Some(old) => {
                    let target = Slot { chamber: old.chamber, window: *w };
                    if self.conflicts(target, Some(id)) {
                        (Outcome::Conflict, None)
                    } else {
                        (Outcome::Ok, Some(target))
                    }
                }
            },
            COp::Query(_) => unreachable!("queries do not check"),
        }
    }
}

struct InFlight {
    op: COp,
    program: Vec<Instr>,
    pc: usize,
    decision: Option<Outcome>,
    target: Option<Slot>,
    rooms: Option<Vec<usize>>,
    invoked: u64,
}

struct Client {
    script: Vec<COp>,
    next: usize,
    current: Option<InFlight>,
}

/// One completed operation: who ran it, when it started and finished, and what it returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// 0 for setup, then 1, 2, ... for the concurrent clients.
    pub client: usize,
    pub op: COp,
    pub invoked: u64,
    pub responded: u64,
    pub answer: Answer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub time: u64,
    pub client: usize,
    pub op: String,
    pub step: &'static str,
    pub note: String,
    /// The index entries after this step, as `format_entries` writes them.
    pub index: String,
}

struct Exec {
    variant: RaceVariant,
    shared: Shared,
    /// Client 0 runs setup alone; clients 1.. run concurrently.
    clients: Vec<Client>,
    /// Serial mode: an operation runs to completion before another client may step.
    serial: bool,
    time: u64,
    history: Vec<Record>,
    events: Vec<Event>,
    lock_contention: bool,
    query_during_move: bool,
}

impl Exec {
    fn new(variant: RaceVariant, sc: &Scenario, serial: bool) -> Self {
        let mut clients = vec![Client { script: sc.setup.clone(), next: 0, current: None }];
        clients.extend(sc.scripts.iter().map(|s| Client { script: s.clone(), next: 0, current: None }));
        Exec {
            variant,
            shared: Shared::default(),
            clients,
            serial,
            time: 0,
            history: Vec::new(),
            events: Vec::new(),
            lock_contention: false,
            query_during_move: false,
        }
    }

    fn finished(&self, c: usize) -> bool {
        let cl = &self.clients[c];
        cl.current.is_none() && cl.next >= cl.script.len()
    }

    fn runnable(&self, c: usize) -> bool {
        if self.finished(c) {
            return false;
        }
        let cl = &self.clients[c];
        let instr = match &cl.current {
            Some(f) => f.program[f.pc],
            None => program(self.variant, &cl.script[cl.next])[0],
        };
        !(instr == Instr::Acquire && matches!(self.shared.lock, Some(holder) if holder != c))
    }

    fn step(&mut self, c: usize) {
        self.time += 1;
        let t = self.time;
        if self.clients[c].current.is_none() {
            let cl = &mut self.clients[c];
            let op = cl.script[cl.next].clone();
            cl.next += 1;
            let program = program(self.variant, &op);
            cl.current = Some(InFlight { op, program, pc: 0, decision: None, target: None, rooms: None, invoked: t });
        }
        let mut f = self.clients[c].current.take().expect("in flight");
        let instr = f.program[f.pc];
        let id = match &f.op {
            COp::Reserve(id, _) | COp::Move(id, _) => id.clone(),
            COp::Query(_) => String::new(),
        };
        let mut note = String::new();
        match instr {
            Instr::Acquire => {
                assert!(!matches!(self.shared.lock, Some(h) if h != c), "acquire while held");
                self.shared.lock = Some(c);
            }
            Instr::Check => {
                let (d, target) = self.shared.decide(&f.op);
                f.decision = Some(d);
                f.target = target;
                note = d.as_str().into();
            }
            Instr::WriteLedger => {
                self.shared.ledger.insert(id, f.target.expect("checked"));
            }
            Instr::Unindex => self.shared.unindex(&id),
            Instr::Index => {
                let s = f.target.expect("checked");
                self.shared.index[s.chamber].push((id, s));
            }
            Instr::Reindex => {
                let s = f.target.expect("checked");
                self.shared.unindex(&id);
                self.shared.index[s.chamber].push((id, s));
            }
            Instr::Read => {
                let COp::Query(w) = f.op else { unreachable!() };
                let moving = self.clients.iter().enumerate().any(|(o, cl)| {
                    o != c && matches!(&cl.current, Some(g) if matches!(g.op, COp::Move(..)) && g.decision == Some(Outcome::Ok))
                });
                self.query_during_move |= moving;
                let rooms = self.shared.read(w);
                note = format_rooms(&rooms);
                f.rooms = Some(rooms);
            }
            Instr::Release => self.shared.lock = None,
            Instr::Done => {}
        }
        let index = sort_entries(self.shared.index.iter().flatten().cloned().collect());
        self.events.push(Event { time: t, client: c, op: f.op.to_string(), step: instr.name(), note, index: format_entries(&index) });
        if instr == Instr::Done {
            let answer = match f.rooms {
                Some(r) => Answer::Rooms(r),
                None => Answer::Outcome(f.decision.expect("checked")),
            };
            self.history.push(Record { client: c, op: f.op, invoked: f.invoked, responded: t, answer });
            return;
        }
        f.pc = if instr == Instr::Check && f.decision != Some(Outcome::Ok) {
            // A rejected write skips to its release (if it holds the lock) or straight to done.
            f.pc + 1 + f.program[f.pc + 1..].iter().position(|i| matches!(i, Instr::Release | Instr::Done)).expect("ends in done")
        } else {
            f.pc + 1
        };
        self.clients[c].current = Some(f);
    }

    /// Runs setup alone, then the clients under `schedule`; returns the clients actually stepped.
    fn run(&mut self, schedule: &[usize]) -> Vec<usize> {
        while !self.finished(0) {
            self.step(0);
        }
        let workers: Vec<usize> = (1..self.clients.len()).collect();
        let mut taken = Vec::new();
        let mut entries = schedule.iter();
        loop {
            let mid_op = workers.iter().copied().find(|&c| self.clients[c].current.is_some());
            if workers.iter().all(|&c| self.finished(c)) {
                break;
            }
            let c = if let (true, Some(c)) = (self.serial, mid_op) {
                c
            } else {
                let want = entries.next().copied().unwrap_or(1);
                let start = (want.max(1) - 1) % workers.len();
                if !self.finished(workers[start]) && !self.runnable(workers[start]) {
                    self.lock_contention = true;
                }
                let found = (0..workers.len()).map(|k| workers[(start + k) % workers.len()]).find(|&c| self.runnable(c));
                found.expect("a lock holder is always runnable, so some client can step")
            };
            self.step(c);
            taken.push(c);
        }
        taken
    }
}

fn answer_on(reference: &mut Reference, op: &COp) -> Answer {
    match op {
        COp::Reserve(id, s) => Answer::Outcome(reference.reserve(id, *s)),
        COp::Move(id, w) => Answer::Outcome(reference.reschedule(id, *w)),
        COp::Query(w) => Answer::Rooms(reference.available(*w)),
    }
}

/// Finds an order of `history` that respects real time (an operation that finished before
/// another started comes first), reproduces every recorded answer on the sequential
/// reference, and ends in a state `accept` allows. Returns the order and that end state.
///
/// Several orders can match every answer yet end in different states, so the end state has
/// to be part of the search, not checked against whichever order is found first.
pub fn linearize(history: &[Record], accept: &dyn Fn(&Reference) -> bool) -> Option<(Vec<usize>, Reference)> {
    assert!(history.len() <= 64, "linearize handles at most 64 operations");
    fn search(
        h: &[Record],
        remaining: u64,
        state: &Reference,
        accept: &dyn Fn(&Reference) -> bool,
        order: &mut Vec<usize>,
        dead: &mut HashSet<(u64, String)>,
    ) -> Option<Reference> {
        if remaining == 0 {
            return accept(state).then(|| state.clone());
        }
        let key = (remaining, format_bookings(&state.bookings()));
        if dead.contains(&key) {
            return None;
        }
        let live: Vec<usize> = (0..h.len()).filter(|i| remaining & (1 << i) != 0).collect();
        for &i in &live {
            if live.iter().any(|&j| h[j].responded < h[i].invoked) {
                continue;
            }
            let mut next = state.clone();
            if answer_on(&mut next, &h[i].op) != h[i].answer {
                continue;
            }
            order.push(i);
            if let Some(end) = search(h, remaining & !(1 << i), &next, accept, order, dead) {
                return Some(end);
            }
            order.pop();
        }
        dead.insert(key);
        None
    }
    let all = if history.len() == 64 { u64::MAX } else { (1u64 << history.len()) - 1 };
    let mut order = Vec::new();
    search(history, all, &Reference::default(), accept, &mut order, &mut HashSet::new()).map(|end| (order, end))
}

/// After everything finished: the store must match the linearized reference exactly.
fn final_state(shared: &Shared, reference: &Reference) -> Option<String> {
    let expected = reference.bookings();
    if shared.ledger != expected {
        return Some(format!(
            "final bookings {}, linearized reference {}",
            format_bookings(&shared.ledger),
            format_bookings(&expected)
        ));
    }
    let windows = expected.values().map(|s| s.window).chain(shared.index.iter().flatten().map(|(_, s)| s.window));
    elementary(windows).into_iter().find_map(|w| {
        let (got, want) = (shared.read(w), reference.available(w));
        (got != want).then(|| format!("final availability {w}: {}, linearized reference {}", format_rooms(&got), format_rooms(&want)))
    })
}

/// Setup operations, one script per concurrent client, and the scheduler's choices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    pub setup: Vec<COp>,
    pub scripts: Vec<Vec<COp>>,
    /// Client numbers (1-based). If the chosen client cannot step, the next one that can
    /// does; when the list runs out, client 1 is asked first.
    pub schedule: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceRun {
    pub taken: Vec<usize>,
    pub history: Vec<Record>,
    pub events: Vec<Event>,
    pub failure: Option<String>,
    pub fired: Vec<&'static str>,
}

pub const SOMETIMES: [(&str, &str); 5] = [
    ("overlapping_ops", "two clients' operations overlap in time"),
    ("contended_write", "two overlapping writes target overlapping windows"),
    ("conflict_under_contention", "a write overlapping another write is rejected with CONFLICT"),
    ("lock_contention", "the scheduler picks a client that is blocked on the lock"),
    ("query_during_move", "a query reads while an accepted move is still in flight"),
];

fn window_of(op: &COp) -> Option<Window> {
    match op {
        COp::Reserve(_, s) => Some(s.window),
        COp::Move(_, w) => Some(*w),
        COp::Query(_) => None,
    }
}

pub fn execute(v: RaceVariant, sc: &Scenario, serial: bool) -> RaceRun {
    let mut ex = Exec::new(v, sc, serial);
    let taken = ex.run(&sc.schedule);
    let failure = match linearize(&ex.history, &|_| true) {
        None => Some(format!(
            "not linearizable: no order of these {} operations that respects real time reproduces their results",
            ex.history.len()
        )),
        Some((_, first)) => match linearize(&ex.history, &|end| final_state(&ex.shared, end).is_none()) {
            Some(_) => None,
            // Report against the first matching order; no matching order ends in this state.
            None => final_state(&ex.shared, &first).map(|r| format!("{r}; no order matching every answer ends in the store's state")),
        },
    };
    let concurrent: Vec<&Record> = ex.history.iter().filter(|r| r.client > 0).collect();
    let mut pairs = Vec::new();
    for (i, a) in concurrent.iter().enumerate() {
        for b in &concurrent[i + 1..] {
            if a.client != b.client && a.invoked < b.responded && b.invoked < a.responded {
                pairs.push((*a, *b));
            }
        }
    }
    let writes = |a: &Record, b: &Record| window_of(&a.op).zip(window_of(&b.op));
    let mut fired = Vec::new();
    if !pairs.is_empty() {
        fired.push("overlapping_ops");
    }
    if pairs.iter().any(|(a, b)| matches!(writes(a, b), Some((x, y)) if x.overlaps(y))) {
        fired.push("contended_write");
    }
    let conflict = Answer::Outcome(Outcome::Conflict);
    if pairs.iter().any(|(a, b)| writes(a, b).is_some() && (a.answer == conflict || b.answer == conflict)) {
        fired.push("conflict_under_contention");
    }
    if ex.lock_contention {
        fired.push("lock_contention");
    }
    if ex.query_during_move {
        fired.push("query_during_move");
    }
    RaceRun { taken, history: ex.history, events: ex.events, failure, fired }
}

fn fails(v: RaceVariant, sc: &Scenario) -> bool {
    execute(v, sc, false).failure.is_some()
}

/// Per-seed knobs, drawn from the seed (swarm testing).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceSwarm {
    pub clients: usize,
    pub ops_per_client: usize,
    pub setup: usize,
    /// Chance the scheduler switches client at each step: low means long runs, high means
    /// fine-grained interleaving.
    pub switch_permille: u32,
    pub query_permille: u32,
    pub move_permille: u32,
}

impl fmt::Display for RaceSwarm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "clients={} ops={} setup={} switch={}\u{2030} query={}\u{2030} move={}\u{2030}",
            self.clients, self.ops_per_client, self.setup, self.switch_permille, self.query_permille, self.move_permille
        )
    }
}

const IDS: [&str; 3] = ["A", "B", "C"];

fn small_window(rng: &mut Rng) -> Window {
    // A narrow region, so clients contend for the same slots.
    let start = 600 + 15 * rng.below(8) as Tick;
    Window { start, end: start + 15 * (1 + rng.below(4) as Tick) }
}

pub fn generate(seed: u64) -> (RaceSwarm, Scenario) {
    let mut rng = Rng::new(seed);
    let swarm = RaceSwarm {
        clients: 2 + rng.below(2),
        ops_per_client: 1 + rng.below(3),
        setup: rng.below(4),
        switch_permille: [100, 500, 900][rng.below(3)],
        query_permille: [200, 400][rng.below(2)],
        move_permille: [300, 600][rng.below(2)],
    };
    let setup = (0..swarm.setup)
        .map(|i| COp::Reserve(IDS[i % IDS.len()].into(), Slot { chamber: rng.below(CHAMBERS), window: small_window(&mut rng) }))
        .collect();
    let scripts = (0..swarm.clients)
        .map(|_| {
            (0..swarm.ops_per_client)
                .map(|_| {
                    if rng.permille(swarm.query_permille) {
                        let a = small_window(&mut rng);
                        let b = small_window(&mut rng);
                        COp::Query(Window { start: a.start.min(b.start), end: a.end.max(b.end) })
                    } else if rng.permille(swarm.move_permille) {
                        COp::Move(IDS[rng.below(IDS.len())].into(), small_window(&mut rng))
                    } else {
                        let chamber = rng.below(CHAMBERS);
                        COp::Reserve(IDS[rng.below(IDS.len())].into(), Slot { chamber, window: small_window(&mut rng) })
                    }
                })
                .collect()
        })
        .collect();
    let total: usize = swarm.clients * swarm.ops_per_client;
    let mut schedule = Vec::with_capacity(16 * total);
    let mut current = 1 + rng.below(swarm.clients);
    for _ in 0..16 * total {
        if rng.permille(swarm.switch_permille) {
            current = 1 + rng.below(swarm.clients);
        }
        schedule.push(current);
    }
    (swarm, Scenario { setup, scripts, schedule })
}

/// Smallest failing scenario reachable by dropping setup operations, client operations,
/// whole clients and schedule entries. The schedule is replaced by the steps actually
/// taken after each improvement, so the result replays exactly.
pub fn shrink(v: RaceVariant, sc: &Scenario) -> Scenario {
    assert!(fails(v, sc), "shrink needs a failing scenario");
    let realized = |s: Scenario| Scenario { schedule: execute(v, &s, false).taken, ..s };
    // Every accepted candidate must strictly lower this, so shrinking terminates.
    let size = |s: &Scenario| {
        let ops = s.setup.len() + s.scripts.iter().map(Vec::len).sum::<usize>();
        let switches = s.schedule.windows(2).filter(|w| w[0] != w[1]).count();
        (ops, switches, s.schedule.len())
    };
    let mut best = realized(sc.clone());
    loop {
        let mut candidates: Vec<Scenario> = Vec::new();
        for i in 0..best.setup.len() {
            let mut c = best.clone();
            c.setup.remove(i);
            candidates.push(c);
        }
        for k in 0..best.scripts.len() {
            let mut c = best.clone();
            c.scripts.remove(k);
            let client = k + 1;
            c.schedule = c.schedule.iter().filter(|&&x| x != client).map(|&x| if x > client { x - 1 } else { x }).collect();
            if !c.scripts.is_empty() {
                candidates.push(c);
            }
            for i in 0..best.scripts[k].len() {
                let mut c = best.clone();
                c.scripts[k].remove(i);
                candidates.push(c);
            }
        }
        // A removed operation changes the timing, so also try simple schedules of the same
        // length: round-robin from each client, and two blocks split at every point.
        let structural = candidates.clone();
        for c in structural {
            let (n, len) = (c.scripts.len(), c.schedule.len());
            for first in 1..=n {
                candidates.push(Scenario { schedule: (0..len).map(|i| (first - 1 + i) % n + 1).collect(), ..c.clone() });
                for second in (1..=n).filter(|&s| s != first) {
                    for split in 0..=len {
                        let schedule = (0..len).map(|i| if i < split { first } else { second }).collect();
                        candidates.push(Scenario { schedule, ..c.clone() });
                    }
                }
            }
        }
        let mut chunk = best.schedule.len().max(1);
        while chunk > 0 {
            for at in (0..best.schedule.len()).step_by(chunk) {
                let mut c = best.clone();
                c.schedule.drain(at..(at + chunk).min(c.schedule.len()));
                candidates.push(c);
            }
            chunk /= 2;
        }
        let current = size(&best);
        let next = candidates.into_iter().filter(|c| fails(v, c)).map(realized).find(|c| size(c) < current);
        match next {
            Some(c) => best = c,
            None => break,
        }
    }
    best
}

/// The scenario as a replayable `.race` file.
pub fn race_text(v: RaceVariant, sc: &Scenario, comment: &str) -> String {
    let mut out = format!("# Pulsefold race v1. {comment}\nvariant {}\n", v.name());
    for op in &sc.setup {
        out.push_str(&format!("setup {op}\n"));
    }
    for (k, script) in sc.scripts.iter().enumerate() {
        for op in script {
            out.push_str(&format!("client {} {op}\n", k + 1));
        }
    }
    let schedule: Vec<String> = sc.schedule.iter().map(usize::to_string).collect();
    out.push_str(&format!("schedule {}\n", schedule.join(" ")));
    out
}

pub fn parse_race(text: &str) -> Result<(RaceVariant, Scenario), String> {
    let (mut variant, mut setup, mut clients, mut schedule) = (None, Vec::new(), BTreeMap::<usize, Vec<COp>>::new(), None);
    for (n, raw) in text.lines().enumerate() {
        let err = |e: String| format!("race line {}: {e}", n + 1);
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        match key {
            "variant" => {
                variant = Some(RaceVariant::parse(rest).ok_or_else(|| err(format!("unknown race variant {rest:?}")))?);
            }
            "setup" => setup.push(parse_cop(rest).map_err(err)?),
            "client" => {
                let (k, op) = rest.split_once(char::is_whitespace).ok_or_else(|| err("expected client N OPERATION".into()))?;
                let k: usize = k.parse().map_err(|_| err(format!("bad client number {k}")))?;
                if k == 0 {
                    return Err(err("clients are numbered from 1".into()));
                }
                clients.entry(k).or_default().push(parse_cop(op.trim()).map_err(err)?);
            }
            "schedule" => {
                let s: Result<Vec<usize>, _> = rest.split_whitespace().map(str::parse).collect();
                schedule = Some(s.map_err(|_| err("schedule takes client numbers".into()))?);
            }
            other => return Err(err(format!("unknown directive {other:?}"))),
        }
    }
    let variant = variant.ok_or("race file has no variant")?;
    if clients.is_empty() || clients.keys().copied().ne(1..=clients.len()) {
        return Err("clients must be numbered 1, 2, ... without gaps".into());
    }
    let schedule = schedule.unwrap_or_default();
    if schedule.iter().any(|&c| c == 0 || c > clients.len()) {
        return Err("schedule names a client that does not exist".into());
    }
    Ok((variant, Scenario { setup, scripts: clients.into_values().collect(), schedule }))
}

/// A readable account of one run: every micro-step, then the history and the verdict.
pub fn narrate(v: RaceVariant, sc: &Scenario) -> String {
    let run = execute(v, sc, false);
    let mut out = format!("variant\t{}\ntime\tclient\toperation\tstep\tnote\tindex\n", v.name());
    for e in &run.events {
        let who = if e.client == 0 { "setup".to_string() } else { e.client.to_string() };
        let note = if e.note.is_empty() { "-" } else { &e.note };
        out.push_str(&format!("{}\t{who}\t{}\t{}\t{note}\t{}\n", e.time, e.op, e.step, e.index));
    }
    out.push_str("history\tclient\toperation\tinvoked\tresponded\tanswer\n");
    for r in &run.history {
        let who = if r.client == 0 { "setup".to_string() } else { r.client.to_string() };
        out.push_str(&format!("-\t{who}\t{}\t{}\t{}\t{}\n", r.op, r.invoked, r.responded, r.answer));
    }
    out.push_str(&format!("verdict\t{}\n", run.failure.as_deref().unwrap_or("linearizable")));
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct RaceReport {
    pub variant: RaceVariant,
    pub first_seed: u64,
    pub runs: u64,
    pub operations: u64,
    pub failing: u64,
    pub failing_serial: u64,
    pub hits: BTreeMap<&'static str, u64>,
    pub first: Option<(u64, RaceSwarm, String, Scenario)>,
}

pub fn simulate(v: RaceVariant, first_seed: u64, runs: u64) -> RaceReport {
    let mut r = RaceReport {
        variant: v,
        first_seed,
        runs,
        operations: 0,
        failing: 0,
        failing_serial: 0,
        hits: BTreeMap::new(),
        first: None,
    };
    for seed in (0..runs).map(|i| first_seed.wrapping_add(i)) {
        let (swarm, sc) = generate(seed);
        let run = execute(v, &sc, false);
        r.operations += run.history.len() as u64;
        for p in run.fired {
            *r.hits.entry(p).or_default() += 1;
        }
        if execute(v, &sc, true).failure.is_some() {
            r.failing_serial += 1;
        }
        if let Some(reason) = run.failure {
            r.failing += 1;
            if r.first.is_none() {
                r.first = Some((seed, swarm, reason, shrink(v, &sc)));
            }
        }
    }
    r
}

/// Whether a sometimes-property can fire for a variant at all (`lock_contention` needs a lock).
pub fn applicable(property: &str, v: RaceVariant) -> bool {
    property != "lock_contention" || v != RaceVariant::CheckThenAct
}

impl RaceReport {
    pub fn unreached(&self) -> Vec<&'static str> {
        SOMETIMES
            .iter()
            .map(|(p, _)| *p)
            .filter(|p| applicable(p, self.variant) && !self.hits.contains_key(p))
            .collect()
    }

    /// 0 pass, 1 failure found, 3 weak (a sometimes-property never fired).
    pub fn exit_code(&self) -> u8 {
        if self.failing > 0 {
            1
        } else if self.unreached().is_empty() {
            0
        } else {
            3
        }
    }
}

impl fmt::Display for RaceReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let last = self.first_seed.wrapping_add(self.runs.saturating_sub(1));
        writeln!(f, "variant\t{}", self.variant.name())?;
        writeln!(f, "kind\t{}", if self.variant.correct() { "control" } else { "bug" })?;
        writeln!(f, "seeds\t{}..={last}", self.first_seed)?;
        writeln!(f, "operations\t{}", self.operations)?;
        for (p, _) in SOMETIMES {
            if applicable(p, self.variant) {
                writeln!(f, "sometimes\t{p}\t{}", self.hits.get(p).copied().unwrap_or(0))?;
            } else {
                writeln!(f, "sometimes\t{p}\tn/a")?;
            }
        }
        let (lo, hi) = wilson(self.failing, self.runs);
        writeln!(f, "failing_seeds\t{}/{}\tci95 {lo:.3}-{hi:.3}", self.failing, self.runs)?;
        writeln!(f, "failing_seeds_serial\t{}/{}\tsame scenarios, no preemption inside an operation", self.failing_serial, self.runs)?;
        match &self.first {
            None => writeln!(f, "first_failure\t-")?,
            Some((seed, swarm, reason, shrunk)) => {
                writeln!(f, "first_failure\tseed {seed} ({swarm})")?;
                writeln!(f, "reason\t{reason}")?;
                let ops: usize = shrunk.setup.len() + shrunk.scripts.iter().map(Vec::len).sum::<usize>();
                writeln!(f, "shrunk\t{} clients, {ops} operations, {} scheduled steps", shrunk.scripts.len(), shrunk.schedule.len())?;
                for line in race_text(self.variant, shrunk, "").lines().skip(2) {
                    writeln!(f, "shrunk_line\t{line}")?;
                }
                writeln!(f, "reproduce\tpulsefold race --variant {} --seed {seed} --runs 1", self.variant.name())?;
            }
        }
        let unreached = self.unreached();
        writeln!(f, "unreached\t{}", if unreached.is_empty() { "-".into() } else { unreached.join(",") })?;
        let result = match self.exit_code() {
            0 => "pass",
            1 => "fail",
            _ => "weak: no failure, but some sometimes-properties never fired",
        };
        writeln!(f, "result\t{result}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Op;

    fn rec(client: usize, op: &str, invoked: u64, responded: u64, answer: Answer) -> Record {
        Record { client, op: parse_cop(op).unwrap(), invoked, responded, answer }
    }
    const OK: Answer = Answer::Outcome(Outcome::Ok);

    #[test]
    fn linearizability_checker_on_known_histories() {
        let any = |_: &Reference| true;
        // Sequential and consistent.
        let h = [rec(1, "reserve A 0 600 660", 1, 2, OK), rec(2, "query 600 660", 3, 4, Answer::Rooms(vec![1]))];
        assert_eq!(linearize(&h, &any).map(|x| x.0), Some(vec![0, 1]));
        // The query finished before the reserve started, so it cannot see it.
        let h = [rec(1, "reserve A 0 600 660", 3, 4, OK), rec(2, "query 600 660", 1, 2, Answer::Rooms(vec![1]))];
        assert!(linearize(&h, &any).is_none());
        // ...but if they overlap, the query may be ordered after the reserve.
        let h = [rec(1, "reserve A 0 600 660", 1, 4, OK), rec(2, "query 600 660", 2, 3, Answer::Rooms(vec![1]))];
        assert_eq!(linearize(&h, &any).map(|x| x.0), Some(vec![0, 1]));
        // Two overlapping reserves of one ID cannot both succeed.
        let h = [rec(1, "reserve A 0 600 660", 1, 3, OK), rec(2, "reserve A 0 600 660", 2, 4, OK)];
        assert!(linearize(&h, &any).is_none());
        // Two overlapping moves: both orders match the answers; the end state decides.
        let h = [
            rec(0, "reserve A 0 600 660", 1, 2, OK),
            rec(1, "move A 700 720", 3, 6, OK),
            rec(2, "move A 800 820", 4, 5, OK),
        ];
        let ends_at = |start: u16| move |r: &Reference| r.slot("A").map(|s| s.window.start) == Some(start);
        assert_eq!(linearize(&h, &ends_at(700)).map(|x| x.0), Some(vec![0, 2, 1]));
        assert_eq!(linearize(&h, &ends_at(800)).map(|x| x.0), Some(vec![0, 1, 2]));
        assert!(linearize(&h, &ends_at(600)).is_none());
    }

    #[test]
    fn dead_state_memo_keeps_the_search_polynomial_in_practice() {
        // 14 overlapping queries commute, and two overlapping reserves of one ID cannot both
        // succeed. Without the memo this explores about 14! query orders before giving up.
        let mut h: Vec<Record> = (0..14).map(|i| rec(i + 1, "query 0 10", 1, 100, Answer::Rooms(vec![0, 1]))).collect();
        h.push(rec(20, "reserve A 0 600 660", 1, 100, OK));
        h.push(rec(21, "reserve A 1 600 660", 1, 100, OK));
        let started = std::time::Instant::now();
        assert!(linearize(&h, &|_| true).is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
    }

    #[test]
    fn controls_hold_and_reach_every_corner() {
        for v in [RaceVariant::Locked, RaceVariant::LockedWrites] {
            let r = simulate(v, 1, 1000);
            assert_eq!((r.failing, r.exit_code()), (0, 0), "{r}");
        }
    }

    #[test]
    fn races_are_found_and_need_preemption_inside_an_operation() {
        for v in RaceVariant::ALL {
            let r = simulate(v, 1, 1000);
            assert_eq!(r.failing_serial, 0, "{}: serial execution never fails", v.name());
            assert_eq!(r.failing > 0, !v.correct(), "{r}");
        }
    }

    #[test]
    fn one_client_behaves_exactly_like_the_reference() {
        // Every race variant is sequentially correct: the bugs need concurrency.
        for v in RaceVariant::ALL {
            for seed in 1..=100 {
                let (_, ops) = crate::sim::generate(seed, 32);
                let script: Vec<COp> = ops
                    .iter()
                    .map(|op| match op {
                        Op::Reserve(id, s) => COp::Reserve(id.clone(), *s),
                        Op::Move(id, w) => COp::Move(id.clone(), *w),
                    })
                    .collect();
                let sc = Scenario { setup: vec![], scripts: vec![script], schedule: vec![] };
                let run = execute(v, &sc, false);
                assert_eq!(run.failure, None, "{} seed {seed}", v.name());
                let mut reference = Reference::default();
                for r in &run.history {
                    assert_eq!(r.answer, answer_on(&mut reference, &r.op), "{} seed {seed}", v.name());
                }
            }
        }
    }

    #[test]
    fn shrunk_races_are_minimal_and_replay_from_text() {
        let expect = [(RaceVariant::CheckThenAct, 0, 2), (RaceVariant::TornMove, 1, 2)];
        for (v, setup, client_ops) in expect {
            let r = simulate(v, 1, 1000);
            let (_, _, _, shrunk) = r.first.unwrap();
            assert_eq!(shrunk.setup.len(), setup, "{}", v.name());
            assert_eq!(shrunk.scripts.iter().map(Vec::len).sum::<usize>(), client_ops, "{}", v.name());
            let (pv, parsed) = parse_race(&race_text(v, &shrunk, "test")).unwrap();
            assert_eq!((pv, &parsed), (v, &shrunk));
            assert!(execute(v, &parsed, false).failure.is_some());
            assert!(execute(RaceVariant::Locked, &parsed, false).failure.is_none(), "the lock fixes it");
        }
    }

    #[test]
    fn the_torn_read_lands_between_unindex_and_index() {
        let (v, sc) = parse_race(
            "variant torn-move\nsetup reserve C 0 600 630\nclient 1 query 600 750\nclient 2 move C 645 660\nschedule 2 2 2 2 1 1 2 2 2\n",
        )
        .unwrap();
        let run = execute(v, &sc, false);
        let steps: Vec<(usize, &str)> = run.events.iter().map(|e| (e.client, e.step)).collect();
        let at = |c: usize, s: &str| steps.iter().position(|&x| x == (c, s)).unwrap();
        assert!(at(2, "unindex") < at(1, "read") && at(1, "read") < at(2, "index"));
        assert_eq!(run.history[1].answer, Answer::Rooms(vec![0, 1]));
        assert!(run.failure.unwrap().starts_with("not linearizable"));
    }

    #[test]
    fn lock_contention_does_not_apply_without_a_lock() {
        assert!(!applicable("lock_contention", RaceVariant::CheckThenAct));
        assert!(applicable("lock_contention", RaceVariant::TornMove));
        assert!(applicable("query_during_move", RaceVariant::CheckThenAct));
        let r = simulate(RaceVariant::CheckThenAct, 1, 1000);
        assert!(!r.unreached().contains(&"lock_contention"));
        assert!(r.to_string().contains("sometimes\tlock_contention\tn/a\n"));
    }

    #[test]
    fn race_files_reject_bad_input() {
        for bad in [
            "client 1 query 1 2",
            "variant fast\nclient 1 query 1 2",
            "variant locked",
            "variant locked\nclient 2 query 1 2",
            "variant locked\nclient 0 query 1 2",
            "variant locked\nclient 1 query 2 1",
            "variant locked\nclient 1 query 1",
            "variant locked\nclient 1 query 1 2\nschedule 1 3",
            "variant locked\nclient 1 query 1 2\nschedule x",
            "variant locked\nclient 1 fly A",
            "variant locked\nclient 1 query 1 2\nbogus",
        ] {
            assert!(parse_race(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn reports_are_deterministic_and_match_the_golden_files() {
        for (v, golden) in [
            (RaceVariant::Locked, include_str!("../sim/race-locked.expected")),
            (RaceVariant::LockedWrites, include_str!("../sim/race-locked-writes.expected")),
            (RaceVariant::CheckThenAct, include_str!("../sim/race-check-then-act.expected")),
            (RaceVariant::TornMove, include_str!("../sim/race-torn-move.expected")),
        ] {
            assert_eq!(simulate(v, 1, 1000).to_string(), golden, "{}", v.name());
        }
    }
}
