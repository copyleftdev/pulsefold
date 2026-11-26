use std::{collections::BTreeMap, env, fs, process::ExitCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot { chamber: usize, start: u16, end: u16 }
impl Slot {
    fn overlaps(self, rhs: Self) -> bool {
        self.chamber == rhs.chamber && self.start < rhs.end && rhs.start < self.end
    }
}
type Map = BTreeMap<String, Slot>;
enum Op { Reserve(String, Slot), Move(String, u16, u16) }

#[derive(Default)]
struct Sim { ledger: Map, index: [Map; 2] }
impl Sim {
    fn put(&mut self, id: String, slot: Slot) {
        self.index[slot.chamber].insert(id.clone(), slot);
        self.ledger.insert(id, slot);
    }

    fn apply(&mut self, op: Op, stale: bool) -> &'static str {
        match op {
            Op::Reserve(id, slot) => {
                if self.ledger.contains_key(&id) { return "DUPLICATE"; }
                if self.ledger.values().any(|s| s.overlaps(slot)) { return "CONFLICT"; }
                self.put(id, slot);
            }
            Op::Move(id, start, end) => {
                let Some(old) = self.ledger.get(&id) else { return "NOT_FOUND"; };
                let slot = Slot { chamber: old.chamber, start, end };
                if self.ledger.iter().any(|(other, s)| other != &id && s.overlaps(slot)) {
                    if stale { self.index[slot.chamber].insert(id, slot); }
                    return "CONFLICT";
                }
                self.put(id, slot);
            }
        }
        "OK"
    }

    fn consistent(&self) -> bool {
        self.index.iter().enumerate().all(|(room, map)| {
            map.iter().all(|(k, v)| v.chamber == room && self.ledger.get(k) == Some(v))
        }) && self.ledger.iter().all(|(k, v)| self.index[v.chamber].get(k) == Some(v))
    }
}

fn parse(line: &str) -> Result<Op, String> {
    let words: Vec<_> = line.split_whitespace().collect();
    let num = |s: &str| s.parse::<u16>().map_err(|_| format!("bad tick: {s}"));
    let window = |lo: &str, hi: &str| {
        let (a, b) = (num(lo)?, num(hi)?);
        if a >= b { return Err(String::from("expected start < end")); }
        Ok((a, b))
    };
    let valid_id = |id: &str| !id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
    match words.as_slice() {
        ["reserve", id, chamber, lo, hi] if valid_id(id) => {
            let chamber = usize::from(num(chamber)?);
            if chamber >= 2 { return Err("chamber must be 0 or 1".into()); }
            let (start, end) = window(lo, hi)?;
            Ok(Op::Reserve((*id).into(), Slot { chamber, start, end }))
        }
        ["move", id, lo, hi] if valid_id(id) => {
            let (start, end) = window(lo, hi)?;
            Ok(Op::Move((*id).into(), start, end))
        }
        _ => Err("expected reserve ID ROOM START END or move ID START END".into()),
    }
}

fn open_rooms(sim: &Sim, start: u16, end: u16, indexed: bool) -> String {
    let names = ["C1", "C2"];
    (0..2).filter_map(|chamber| {
        let probe = Slot { chamber, start, end };
        let occupied = if indexed {
            sim.index[chamber].values().any(|s| s.overlaps(probe))
        } else { sim.ledger.values().any(|s| s.overlaps(probe)) };
        (!occupied).then_some(names[chamber])
    }).collect::<Vec<_>>().join(",")
}

fn replay(input: &str, stale: bool) -> Result<(Vec<String>, bool), String> {
    let mut sim = Sim::default();
    let mut rows = vec![String::from("step\toperation\tresult\tledger\tindex\toracle_old\tindex_old\toracle_new\tindex_new\tcanary\tverdict")];
    let mut passed = true;
    let fmt = |map: &Map| map.iter().map(|(k, s)| {
        format!("{k}@C{}:{}-{}", s.chamber + 1, s.start, s.end)
    }).collect::<Vec<_>>().join(" ");
    for (n, raw) in input.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() { continue; }
        let op = parse(line).map_err(|e| format!("line {}: {e}", n + 1))?;
        let sim_before = (sim.ledger.clone(), sim.index.clone());
        let result = sim.apply(op, stale);
        let index: Map = sim.index.iter().flat_map(|m| m.iter()).map(|(k, v)| (k.clone(), *v)).collect();
        let (bo, io) = (open_rooms(&sim, 615, 645, false), open_rooms(&sim, 615, 645, true));
        let (bn, inn) = (open_rooms(&sim, 780, 795, false), open_rooms(&sim, 780, 795, true));
        let unchanged = result == "OK" || (sim.ledger == sim_before.0 && sim.index == sim_before.1);
        let holds = sim.consistent() && unchanged && bo == io && bn == inn;
        passed &= holds;
        let operation = line.split_whitespace().collect::<Vec<_>>().join(" ");
        rows.push(format!("{}\t{operation}\t{result}\t{}\t{}\t{bo}\t{io}\t{bn}\t{inn}\tEXACT\t{}",
            rows.len(), fmt(&sim.ledger), fmt(&index), if holds { "PASS" } else { "FAIL" }));
    }
    if rows.len() == 1 { return Err("scenario has no operations".into()); }
    Ok((rows, passed))
}

fn run() -> Result<bool, String> {
    let args: Vec<_> = env::args().skip(1).collect();
    let (file, stale) = match args.as_slice() {
        [file] => (file, false),
        [file, flag] if flag == "--stale-index" => (file, true),
        _ => return Err("usage: pulsefold SCENARIO [--stale-index] (exit: 0 pass; 1 invariant failure; 2 input error)".into()),
    };
    let input = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let (rows, passed) = replay(&input, stale)?;
    println!("{}", rows.join("\n"));
    Ok(passed)
}
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => { eprintln!("{e}"); ExitCode::from(2) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DEMO: &str = include_str!("../assays/ghost.assay");

    #[test]
    fn normal_and_mutant_are_distinguished() {
        let (good, good_pass) = replay(DEMO, false).unwrap();
        let (bad, bad_pass) = replay(DEMO, true).unwrap();
        assert!(good_pass);
        assert!(!bad_pass);
        assert_eq!(&good[..3], &bad[..3]);
        let good: Vec<_> = good[3].split('\t').collect();
        let bad: Vec<_> = bad[3].split('\t').collect();
        assert_eq!(&good[..4], &bad[..4]);
        assert_eq!(bad[2], "CONFLICT");
        assert_eq!(&bad[5..], &["C2", "C1,C2", "C1,C2", "C2", "EXACT", "FAIL"]);
    }

    #[test]
    fn adjacent_moves_and_conflicting_moves() {
        assert!(replay("reserve A 0 600 660\nreserve B 0 720 780\nmove A 660 720", true).unwrap().1);
        for input in ["reserve A 2 3 4", "reserve A 0 4 4", "move A -1 9", "reserve x-y 0 1 2", ""] {
            assert!(replay(input, false).is_err());
        }
    }

    #[test]
    fn replay_is_byte_stable() {
        assert_eq!(replay(DEMO, true).unwrap(), replay(DEMO, true).unwrap());
    }
}
