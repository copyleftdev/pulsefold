//! Pulsefold runner.
//!
//!     pulsefold trace SESSION      # trace v2 TSV; exit 0 all pass, 1 any failure
//!     pulsefold summary SESSION    # first regression, marker at regression, event order
//!     pulsefold sim [--variant V] [--seed N] [--runs K] [--ops L] [--shrink-to DIR]
//!                                  # exit 0 pass, 1 failure found, 3 weak (a sometimes-
//!                                  # property never fired)
//!
//! Exit 2 for usage or input errors.
mod assay;
mod marker;
mod model;
mod oracle;
mod rng;
mod session;
mod sim;
mod stores;

use std::{env, fs, path::Path, process::ExitCode};

const USAGE: &str = "usage: pulsefold trace SESSION | pulsefold summary SESSION | \
pulsefold sim [--variant V] [--seed N] [--runs K] [--ops L] [--shrink-to DIR] \
(exit: 0 pass; 1 failure; 2 input error; 3 weak simulation)";

fn load(path: &str) -> Result<(session::Session, Vec<model::Op>), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let s = session::parse(&text).map_err(|e| format!("{path}: {e}"))?;
    let assay_path = Path::new(path).parent().unwrap_or_else(|| Path::new("")).join(&s.assay);
    let assay_text = fs::read_to_string(&assay_path).map_err(|e| format!("{}: {e}", assay_path.display()))?;
    let ops = assay::parse(&assay_text).map_err(|e| format!("{}: {e}", assay_path.display()))?;
    Ok((s, ops))
}

fn number<T: std::str::FromStr>(flag: &str, value: Option<&String>) -> Result<T, String> {
    value.and_then(|v| v.parse().ok()).ok_or_else(|| format!("{flag} needs a non-negative integer\n{USAGE}"))
}

fn simulate(args: &[String]) -> Result<u8, String> {
    let (mut variant, mut seed, mut runs, mut ops, mut out) = (stores::Variant::Indexed, 1u64, 100u64, 48usize, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--variant" => {
                let name = it.next().map(String::as_str).unwrap_or("");
                variant = stores::Variant::parse(name).ok_or_else(|| format!("unknown variant {name:?}\n{USAGE}"))?;
            }
            "--seed" => seed = number(flag, it.next())?,
            "--runs" => runs = number(flag, it.next())?,
            "--ops" => ops = number(flag, it.next())?,
            "--shrink-to" => out = Some(it.next().ok_or(USAGE)?.clone()),
            _ => return Err(USAGE.into()),
        }
    }
    if runs == 0 || ops == 0 || ops > 10_000 {
        return Err(format!("--runs must be at least 1 and --ops between 1 and 10000\n{USAGE}"));
    }
    let report = sim::simulate(variant, seed, runs, ops);
    print!("{report}");
    if let (Some(dir), Some(x)) = (out, &report.first_failure) {
        let assay_file = format!("seed-{}.assay", x.seed);
        let dir = Path::new(&dir);
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let write = |name: &str, text: String| {
            fs::write(dir.join(name), text).map_err(|e| format!("{}: {e}", dir.join(name).display()))
        };
        write(&assay_file, sim::assay_text(x.seed, variant, x.generated, &x.shrunk))?;
        write(&format!("seed-{}.session", x.seed), sim::session_text(x.seed, variant, &assay_file))?;
    }
    Ok(match report.verdict() {
        sim::Verdict::Pass => 0,
        sim::Verdict::Fail => 1,
        sim::Verdict::Weak => 3,
    })
}

fn run() -> Result<u8, String> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.as_slice() {
        [c, rest @ ..] if c == "sim" => simulate(rest),
        [c, path] if c == "trace" || c == "summary" => {
            let (s, ops) = load(path)?;
            let rows = session::replay(&s, &ops);
            if c == "summary" {
                print!("{}", session::summarize(&s, &rows));
                return Ok(0);
            }
            print!("{}", session::tsv(&rows));
            Ok(if rows.iter().all(session::Row::passed) { 0 } else { 1 })
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
