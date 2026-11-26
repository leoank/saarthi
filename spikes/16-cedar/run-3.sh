#!/usr/bin/env bash
# RUN-20261126-fjh6d5 (EXP-20261126-f3e5a9, H1 builds): build the spike for aarch64-darwin, aarch64-linux and
# x86_64-linux through its flake, whose checkPhase runs the case table on each. The Linux
# builds go to the machine's remote builders.
# From the repo root: spikes/16-cedar/run-3.sh. Writes notes/runs/RUN-20261126-fjh6d5/run.log.
set -euo pipefail
cd "$(dirname "$0")"
out=../../notes/runs/RUN-20261126-fjh6d5; mkdir -p "$out"
for s in aarch64-darwin aarch64-linux x86_64-linux; do
  if nix build --no-link --print-build-logs ".#packages.$s.default" 2>"$out/$s.build.log"; then
    echo "$s built, table passed: $(grep -o 'test result: [^;]*' "$out/$s.build.log" | tail -1)"
  else
    echo "$s FAILED: $(tail -3 "$out/$s.build.log" | tr '\n' ' ')"
  fi
done | tee "$out/run.log"
