//! Pulsefold runner: replays a multi-turn session against a trusted reference.
//!
//!     pulsefold trace SESSION     # trace v2 TSV; exit 0 all pass, 1 any failure
//!     pulsefold summary SESSION   # first regression, marker at regression, event order
//!
//! Exit 2 for usage or input errors.
mod assay;
mod marker;
mod model;
mod session;
mod stores;

use std::{env, fs, path::Path, process::ExitCode};

const USAGE: &str = "usage: pulsefold trace SESSION | pulsefold summary SESSION \
(exit: 0 pass; 1 invariant failure in trace; 2 input error)";

fn load(path: &str) -> Result<(session::Session, Vec<model::Op>), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let s = session::parse(&text).map_err(|e| format!("{path}: {e}"))?;
    let assay_path = Path::new(path).parent().unwrap_or_else(|| Path::new("")).join(&s.assay);
    let assay_text = fs::read_to_string(&assay_path).map_err(|e| format!("{}: {e}", assay_path.display()))?;
    let ops = assay::parse(&assay_text).map_err(|e| format!("{}: {e}", assay_path.display()))?;
    Ok((s, ops))
}

fn run() -> Result<bool, String> {
    let args: Vec<_> = env::args().skip(1).collect();
    let (command, path) = match args.as_slice() {
        [c, p] if c == "trace" || c == "summary" => (c.as_str(), p.as_str()),
        _ => return Err(USAGE.into()),
    };
    let (s, ops) = load(path)?;
    let rows = session::replay(&s, &ops);
    if command == "summary" {
        print!("{}", session::summarize(&s, &rows));
        return Ok(true);
    }
    print!("{}", session::tsv(&rows));
    Ok(rows.iter().all(session::Row::passed))
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
