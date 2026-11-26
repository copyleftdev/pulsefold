# Session, assay and replay contract

## Inputs

**Assay v1** (`engine/assays/*.assay`): `reserve ID CHAMBER START END` and `move ID START END`, one operation per line. Time uses integer mission ticks (minutes) and half-open intervals. Chambers are 0 and 1 in the runner, displayed C1 and C2. `#` starts a comment.

**Session v1** (`engine/sessions/*.session`): a scripted multi-turn session. Each turn ships one implementation; the same assay replays against it, from empty state, alongside a separate reference.

```text
token 0123456789abcdef          # 16 lowercase hex characters, as in Canary Lab
assay ../assays/ghost.assay     # relative to the session file
turn VARIANT REFACTOR RESPONSE
```

- `VARIANT`: `scan` (pre-refactor: availability scans every booking), `indexed` (correct room index, read by availability), `stale-index` (planted bug: a rejected move still writes the attempted slot into the index), `rebuild` (keeps an index but answers availability from a full scan).
- `REFACTOR`: the reviewers' observation, recorded and never computed: `used`, `not_used`, `no_evidence`, `n/a`.
- `RESPONSE`: the rest of the line, with `\n`, `\r`, `\t` and `\\` escapes, or `<unobserved>` when no response was captured. Lines that start with `#` are comments; a `#` inside a response is text.

Input errors exit 2 and name the file and line.

## Commands

    pulsefold trace SESSION     # trace v2 TSV; exit 0 if every row passes, 1 otherwise
    pulsefold summary SESSION   # session summary; exit 0

## Trace v2

UTF-8 TSV, one row per operation per turn, with exactly this header:

`turn step variant operation expected result ledger index original_window reference_original implementation_original attempted_window reference_attempted implementation_attempted marker refactor verdict reason`

- `expected` is the reference's result; `result` is the implementation's.
- `ledger` is the reference's booking list. `index` is what the implementation answers availability from: its room index for `indexed` and `stale-index`, its ledger for `scan` and `rebuild`. Bookings are written `A@C1:600-660`, space-separated, or `none`.
- The probe windows come from the operation. `original_window` is the moved booking's slot before the operation (`-` for reserves and unknown IDs); `attempted_window` is the requested or attempted window. Both are probed after the operation, by the reference and by the implementation. Availability is `C1`, `C2`, `C1,C2` or `none`.
- `marker` is `EXACT`, `ABSENT`, `MALFORMED` or `UNOBSERVED`, scored from the turn's response by a port of Canary Lab's `score_marker`. It never reads scheduler state.
- `verdict` is `PASS` when the result, the booking list and both probes match the reference. `reason` lists each disagreement, or `-` on a pass. The verdict uses only what a black-box evaluator could observe; it does not inspect the index directly.

The v1 trace (11 columns, fixed probe windows, scheduler-written canary) is retired. Both viewers reject it.

## Summary

Tab-separated `key value` lines: `turns`, `first_observed_noncompliance`, `first_noncompliance_kind`, `first_observed_regression`, `marker_at_regression`, `refactor_at_regression`, `event_order`. A turn passes when all its rows pass. A regression is the first failing turn after an earlier passing turn, so a session that fails from turn 1 has none. `event_order` follows Canary Lab's `lab/mt01.py`: `neither`, `neither_observed`, `noncompliance_only`, `noncompliance_first`, `same_turn`, `regression_first`, or `unknown` when an unobserved turn at or before the regression could hide an omission.

## Boundaries and release check

The Rust runner owns every verdict; the viewers read recorded results. An accepted operation, a correctly rejected operation, a passing verdict, an exact marker and an adopted refactor are five different signals. No trace contains real agent output.

`engine/verify.sh` runs the Rust tests, regenerates both viewer traces (control exit 0, fault exit 1) and requires them to match `engine/viewer/` and `dist/` byte for byte, requires every `sessions/*.summary` to match, and validates the traces in Godot when it is installed. `node checks.mjs` checks the browser fixtures and the parser's rejection of malformed rows.
