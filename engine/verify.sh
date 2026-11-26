#!/bin/sh
# Rebuild the runner, regenerate every packaged output, and require byte equality.
set -eu
cd "$(dirname "$0")"
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT HUP INT TERM
cargo test --offline
cargo build --offline
# Honor a relocated Cargo target directory (absolute or relative to engine/).
bin="${CARGO_TARGET_DIR:-target}/debug/pulsefold"

# The packaged viewer traces: control passes (exit 0), the planted bug fails (exit 1).
"$bin" trace sessions/control.session > "$out/control.tsv"
code=0
"$bin" trace sessions/ghost.session > "$out/mutant.tsv" || code=$?
test "$code" -eq 1
for f in control mutant; do
  cmp "$out/$f.tsv" "viewer/$f.tsv"
  cmp "$out/$f.tsv" "../dist/$f.tsv"
done

# Every session's summary matches its recorded expectation.
for s in sessions/*.session; do
  "$bin" summary "$s" > "$out/summary.tsv"
  cmp "$out/summary.tsv" "${s%.session}.summary"
done

# Simulation: reports are deterministic, exit codes carry the verdict, and the shrunk
# failure replays through the same trace path the viewers use.
expect() { want=$1; shift; code=0; "$@" > "$out/sim.txt" || code=$?; test "$code" -eq "$want"; }
expect 0 "$bin" sim --variant indexed --seed 1 --runs 200 --ops 48
cmp "$out/sim.txt" sim/indexed.expected
expect 1 "$bin" sim --variant stale-index --seed 1 --runs 200 --ops 48 --shrink-to "$out/shrunk"
cmp "$out/sim.txt" sim/stale-index.expected
cmp "$out/shrunk/seed-2.assay" sim/seed-2.assay
cmp "$out/shrunk/seed-2.session" sim/seed-2.session
expect 1 "$bin" trace "$out/shrunk/seed-2.session"
expect 3 "$bin" sim --variant indexed --seed 1 --runs 1 --ops 1
cmp "$out/sim.txt" sim/weak.expected
for v in scan rebuild; do expect 0 "$bin" sim --variant "$v" --seed 1 --runs 200 --ops 48; done
expect 2 "$bin" sim --runs 0

if command -v godot >/dev/null 2>&1; then
  godot --headless --path viewer -- --validate "--control=$out/control.tsv" "--mutant=$out/mutant.tsv"
elif command -v godot4 >/dev/null 2>&1; then
  godot4 --headless --path viewer -- --validate "--control=$out/control.tsv" "--mutant=$out/mutant.tsv"
else
  echo "Godot not found: native viewer validation skipped." >&2
fi
echo "verify.sh: all checks passed."
