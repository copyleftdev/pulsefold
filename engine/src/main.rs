//! Pulsefold runner.
//!
//!     pulsefold trace SESSION      # trace v2 TSV; exit 0 all pass, 1 any failure
//!     pulsefold summary SESSION    # first regression, marker at regression, event order
//!     pulsefold sim [--variant V] [--seed N] [--runs K] [--ops L] [--shrink-to DIR]
//!                                  # exit 0 pass, 1 failure found, 3 weak (a sometimes-
//!                                  # property never fired)
//!     pulsefold catalog [--seed N] [--runs K] [--ops L]
//!                                  # detection rates for every planted bug; exit 0 when
//!                                  # every bug is found and no control is flagged
//!     pulsefold race [--variant V] [--seed N] [--runs K] [--shrink-to DIR]
//!     pulsefold race --replay FILE.race
//!                                  # concurrent clients under a seeded scheduler, checked
//!                                  # for linearizability; exits like sim
//!     pulsefold api < REQUEST      # the browser engine's text API, for parity checks
//!
//! Exit 2 for usage or input errors.
use std::io::Read;
use std::{env, fs, path::Path, process::ExitCode};

use pulsefold::{api, assay, catalog, model, race, session, sim, stores};

const USAGE: &str = "usage: pulsefold trace SESSION | pulsefold summary SESSION | \
pulsefold sim [--variant V] [--seed N] [--runs K] [--ops L] [--shrink-to DIR] | \
pulsefold catalog [--seed N] [--runs K] [--ops L] | \
pulsefold race [--variant V] [--seed N] [--runs K] [--shrink-to DIR] | pulsefold race --replay FILE | \
pulsefold api < REQUEST \
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
        [c] if c == "api" => {
            let mut request = String::new();
            std::io::stdin().read_to_string(&mut request).map_err(|e| format!("stdin: {e}"))?;
            let (code, out) = api::call(&request);
            print!("{out}");
            Ok(u8::try_from(code).unwrap_or(2))
        }
        [c, flag, file] if c == "race" && flag == "--replay" => {
            let text = fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
            let (variant, scenario) = race::parse_race(&text).map_err(|e| format!("{file}: {e}"))?;
            print!("{}", race::narrate(variant, &scenario));
            Ok(if race::execute(variant, &scenario, false).failure.is_some() { 1 } else { 0 })
        }
        [c, rest @ ..] if c == "race" => {
            let (mut variant, mut seed, mut runs, mut out) = (race::RaceVariant::Locked, 1u64, 200u64, None);
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--variant" => {
                        let name = it.next().map(String::as_str).unwrap_or("");
                        variant = race::RaceVariant::parse(name).ok_or_else(|| format!("unknown race variant {name:?}\n{USAGE}"))?;
                    }
                    "--seed" => seed = number(flag, it.next())?,
                    "--runs" => runs = number(flag, it.next())?,
                    "--shrink-to" => out = Some(it.next().ok_or(USAGE)?.clone()),
                    _ => return Err(USAGE.into()),
                }
            }
            if runs == 0 {
                return Err(format!("--runs must be at least 1\n{USAGE}"));
            }
            let report = race::simulate(variant, seed, runs);
            print!("{report}");
            if let (Some(dir), Some((seed, _, reason, shrunk))) = (out, &report.first) {
                let dir = Path::new(&dir);
                fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                let path = dir.join(format!("race-seed-{seed}.race"));
                let text = race::race_text(variant, shrunk, &format!("Shrunk by `pulsefold race` from seed {seed}; the unshrunk run failed with: {reason}."));
                fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            Ok(report.exit_code())
        }
        [c, rest @ ..] if c == "catalog" => {
            let (mut seed, mut runs, mut ops) = (1u64, 1000u64, 64usize);
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--seed" => seed = number(flag, it.next())?,
                    "--runs" => runs = number(flag, it.next())?,
                    "--ops" => ops = number(flag, it.next())?,
                    _ => return Err(USAGE.into()),
                }
            }
            if runs == 0 || ops == 0 || ops > 10_000 {
                return Err(format!("--runs must be at least 1 and --ops between 1 and 10000\n{USAGE}"));
            }
            let c = catalog::catalog(seed, runs, ops);
            print!("{c}");
            Ok(if c.sound() { 0 } else { 1 })
        }
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
