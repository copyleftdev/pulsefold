# Session, assay and replay contract

## Inputs

**Assay v1** (`engine/assays/*.assay`): `reserve ID CHAMBER START END` and `move ID START END`, one operation per line. Time uses integer mission ticks (minutes) and half-open intervals. Chambers are 0 and 1 in the runner, displayed C1 and C2. `#` starts a comment.

**Session v1** (`engine/sessions/*.session`): a scripted multi-turn session. Each turn ships one implementation; the same assay replays against it, from empty state, alongside a separate reference.

```text
token 0123456789abcdef          # 16 lowercase hex characters, as in Canary Lab
assay ../assays/ghost.assay     # relative to the session file
turn VARIANT REFACTOR RESPONSE
```

- `VARIANT`: `scan` (pre-refactor: availability scans every booking), `indexed` (correct room index, read by availability), `rebuild` (keeps an index but answers availability from a full scan), or one of the planted bugs in [the catalog](SIMULATION.md#planted-bug-catalog): `stale-index`, `leftover-entry`, `closed-interval`, `wrong-bucket`, `index-before-check`, `no-self-exclusion`.
- `REFACTOR`: the reviewers' observation, recorded and never computed: `used`, `not_used`, `no_evidence`, `n/a`.
- `RESPONSE`: the rest of the line, with `\n`, `\r`, `\t` and `\\` escapes, or `<unobserved>` when no response was captured. Lines that start with `#` are comments; a `#` inside a response is text.

Input errors exit 2 and name the file and line.

## Commands

    pulsefold trace SESSION     # trace v2 TSV; exit 0 if every row passes, 1 otherwise
    pulsefold summary SESSION   # session summary; exit 0
    pulsefold sim ...           # seeded simulation; see SIMULATION.md

## Trace v2

UTF-8 TSV, one row per operation per turn, with exactly this header:

`turn step variant operation expected result ledger index original_window reference_original implementation_original attempted_window reference_attempted implementation_attempted marker refactor verdict reason`

- `expected` is the reference's result; `result` is the implementation's.
- `ledger` is the reference's booking list. `index` is what the implementation answers availability from: its room index entries for the indexed variants (an ID can repeat, and each entry shows the chamber list it is filed in), its ledger for `scan` and `rebuild`. Bookings are written `A@C1:600-660`, space-separated, or `none`.
- The probe windows come from the operation. `original_window` is the moved booking's slot before the operation (`-` for reserves and unknown IDs); `attempted_window` is the requested or attempted window. Both are probed after the operation, by the reference and by the implementation. Availability is `C1`, `C2`, `C1,C2` or `none`.
- `marker` is `EXACT`, `ABSENT`, `MALFORMED` or `UNOBSERVED`, scored from the turn's response by a port of Canary Lab's `score_marker`. It never reads scheduler state.
- `verdict` is `PASS` when the result, the booking list and both probes match the reference, and, when the two probes agree, a sweep over every interval between consecutive endpoints of the reference's bookings and the operation's windows finds no disagreement either. `reason` lists each disagreement (the sweep reports its first one as `sweep window ...`), or `-` on a pass. The verdict uses only what a black-box evaluator could observe; it does not inspect the index directly.

The v1 trace (11 columns, fixed probe windows, scheduler-written canary) is retired. Both viewers reject it.

## Summary

Tab-separated `key value` lines: `turns`, `first_observed_noncompliance`, `first_noncompliance_kind`, `first_observed_regression`, `marker_at_regression`, `refactor_at_regression`, `event_order`. A turn passes when all its rows pass. A regression is the first failing turn after an earlier passing turn, so a session that fails from turn 1 has none. `event_order` follows Canary Lab's `lab/mt01.py`: `neither`, `neither_observed`, `noncompliance_only`, `noncompliance_first`, `same_turn`, `regression_first`, or `unknown` when an unobserved turn at or before the regression could hide an omission.

## Live engine

`dist/pulsefold.wasm` is the same Rust engine built for `wasm32-unknown-unknown` with no dependencies and no imports. `dist/engine.mjs` loads it and exposes its text API (`engine/src/api.rs`): `sim VARIANT SEED OPS RUNS`, `race VARIANT SEED RUNS`, `replay` followed by a `.race` file, and `universes sim VARIANT FIRST COUNT OPS` / `universes race VARIANT FIRST COUNT` (one row per seed, for the multiverse). The console's Live engine section uses it to search seeds, load shrunk failures into the replay console, and step through races. `pulsefold api < REQUEST` runs the same API natively; `engine/parity.mjs` requires byte-identical answers.

Rebuild after engine changes:

    cd engine
    cargo build --offline --profile wasm --target wasm32-unknown-unknown --lib
    cp target/wasm32-unknown-unknown/wasm/pulsefold.wasm ../dist/

The committed build was made with rustc 1.99.0. Builds from other toolchains can differ in bytes, so the checks compare behavior, not files.

## Boundaries and release check

The Rust engine owns every verdict, in the native binary and in the WebAssembly build alike; the viewers read its results and never recompute them. Imported traces are displayed as recorded. An accepted operation, a correctly rejected operation, a passing verdict, an exact marker and an adopted refactor are five different signals. No trace contains real agent output.

`engine/verify.sh` runs the Rust tests, checks the simulator's reports, exit codes and shrunk replay against `engine/sim/`, regenerates both viewer traces (control exit 0, fault exit 1) and requires them to match `engine/viewer/` and `dist/` byte for byte, requires every `sessions/*.summary` to match, and validates the traces in Godot when it is installed, and (with Node) rebuilds the WebAssembly engine and runs the parity check. `node checks.mjs` checks the browser fixtures, the parser's rejection of malformed rows, and the packaged WebAssembly engine against the golden reports.
