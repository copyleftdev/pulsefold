//! Assay v1: one operation per line, `#` starts a comment.
use crate::model::{Op, Slot, Tick, Window, CHAMBERS};

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn tick(s: &str) -> Result<Tick, String> {
    s.parse::<Tick>().map_err(|_| format!("bad tick: {s}"))
}

fn window(lo: &str, hi: &str) -> Result<Window, String> {
    let (start, end) = (tick(lo)?, tick(hi)?);
    if start >= end {
        return Err("expected start < end".into());
    }
    Ok(Window { start, end })
}

fn op(line: &str) -> Result<Op, String> {
    let words: Vec<_> = line.split_whitespace().collect();
    match words.as_slice() {
        ["reserve", id, chamber, lo, hi] if valid_id(id) => {
            let chamber = usize::from(tick(chamber)?);
            if chamber >= CHAMBERS {
                return Err("chamber must be 0 or 1".into());
            }
            Ok(Op::Reserve((*id).into(), Slot { chamber, window: window(lo, hi)? }))
        }
        ["move", id, lo, hi] if valid_id(id) => Ok(Op::Move((*id).into(), window(lo, hi)?)),
        _ => Err("expected reserve ID CHAMBER START END or move ID START END".into()),
    }
}

pub fn parse(input: &str) -> Result<Vec<Op>, String> {
    let mut ops = Vec::new();
    for (n, raw) in input.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if !line.is_empty() {
            ops.push(op(line).map_err(|e| format!("assay line {}: {e}", n + 1))?);
        }
    }
    if ops.is_empty() {
        return Err("assay has no operations".into());
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_malformed_operations() {
        for input in ["reserve A 2 3 4", "reserve A 0 4 4", "move A -1 9", "reserve x-y 0 1 2", "move A 1", "", "# only"] {
            assert!(parse(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn accepts_comments_and_round_trips() {
        let ops = parse("reserve A 0 600 660 # first\n\n  move   A 1 2").unwrap();
        assert_eq!(ops.iter().map(|o| o.to_string()).collect::<Vec<_>>(), ["reserve A 0 600 660", "move A 1 2"]);
    }
}
