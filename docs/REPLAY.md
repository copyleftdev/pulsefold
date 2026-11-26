# Assay and replay contract

**Inputs:** `reserve ID CHAMBER START END` and `move ID START END`, one operation per line. Time uses integer mission minutes and half-open intervals. Chambers are 0 and 1 in the runner, displayed C1 and C2. Comments begin with `#`. Input syntax errors exit 2.

**Outputs:** UTF-8 TSV with one row per operation. Columns are `step`, `operation`, `result`, `ledger`, `index`, `oracle_old`, `index_old`, `oracle_new`, `index_new`, `canary`, and `verdict`. A booking is written as `A@C1:600-660`. Multiple bookings are space-separated; available rooms are comma-separated. The two availability probes are fixed to the scene in this revision. A full trace can be exported or reimported as TSV; it is not an Assay input.

**Boundaries:** The Rust runner owns checks; the UI reads their recorded results. An accepted operation, a correctly rejected operation, a passing behavioral verdict, and an exact marker are different signals. Current trace data has no real agent outputs. A future format should add the source (scripted, observed, or unobserved), revision, operation windows, and generator details rather than silently changing this format.

**Release check:** Node validates both fixtures and rejects malformed input. `engine/verify.sh` builds the Rust source and checks both exit codes and both generated TSVs against the native viewer fixtures. The browser copy in `dist/` must match those same fixtures.
