#!/bin/sh
# Compare the real Rust runner's trace and process result to the packaged oracle.
set -eu
cd "$(dirname "$0")"
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT HUP INT TERM
cargo test --offline
cargo build --offline
bin=target/debug/pulsefold
"$bin" assays/ghost.assay > "$out/control.tsv"
code=0
"$bin" assays/ghost.assay --stale-index > "$out/mutant.tsv" || code=$?
test "$code" -eq 1
cmp "$out/control.tsv" viewer/control.tsv
cmp "$out/mutant.tsv" viewer/mutant.tsv
if command -v godot >/dev/null 2>&1; then
  godot --headless --path viewer -- --validate "--control=$out/control.tsv" "--mutant=$out/mutant.tsv"
elif command -v godot4 >/dev/null 2>&1; then
  godot4 --headless --path viewer -- --validate "--control=$out/control.tsv" "--mutant=$out/mutant.tsv"
fi
