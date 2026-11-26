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

if command -v godot >/dev/null 2>&1; then
  godot --headless --path viewer -- --validate "--control=$out/control.tsv" "--mutant=$out/mutant.tsv"
elif command -v godot4 >/dev/null 2>&1; then
  godot4 --headless --path viewer -- --validate "--control=$out/control.tsv" "--mutant=$out/mutant.tsv"
else
  echo "Godot not found: native viewer validation skipped." >&2
fi
echo "verify.sh: all checks passed."
