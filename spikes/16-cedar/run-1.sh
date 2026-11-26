#!/usr/bin/env bash
# RUN-20261126-fjh58z (EXP-20261126-f3e5a9, H1 schema and H2, H4): validate the policies against the schema
# in strict mode, decide the table of 59 requests, and swap the policy set at run time.
# From the repo root: spikes/16-cedar/run-1.sh. Writes notes/runs/RUN-20261126-fjh58z/run.log.
set -euo pipefail
cd "$(dirname "$0")"
out=../../notes/runs/RUN-20261126-fjh58z; mkdir -p "$out"
nix develop ../.. -c cargo run -q --locked -- check | tee "$out/run.log"
