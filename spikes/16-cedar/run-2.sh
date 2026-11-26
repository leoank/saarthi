#!/usr/bin/env bash
# RUN-20261126-fjh6wk (EXP-20261126-f3e5a9, H3): time authorizations at 100, 1000 and 10000 sessions with
# 10 people, 40 apps and 20 nodes, release build. Timings depend on the machine; the
# recorded run was on the owner's Apple silicon Mac.
# From the repo root: spikes/16-cedar/run-2.sh. Writes notes/runs/RUN-20261126-fjh6wk/run.log.
set -euo pipefail
cd "$(dirname "$0")"
out=../../notes/runs/RUN-20261126-fjh6wk; mkdir -p "$out"
nix develop ../.. -c cargo run -q --locked --release -- bench | tee "$out/run.log"
