#!/usr/bin/env bash
# Re-creates the runs of EXP-20261112-rq8kgc (RUN-20261112-s5yqfc on aarch64-darwin,
# RUN-20261112-s5z9yk on aarch64-linux). Run from the repo root on the machine to test:
#   TSNET_AUTHKEY_FILE=<mode-600 file with a reusable, pre-approved, non-ephemeral key> \
#     notes/runs/RUN-20261112-s5yqfc/run.sh
# Two devices, <prefix>-hub and <prefix>-node, join the key's tailnet; remove them after.
# The key is read by the program and never printed. Results are written with the tailnet
# addresses and DNS names removed, since the repo is public.
set -euo pipefail

key="${TSNET_AUTHKEY_FILE:?set TSNET_AUTHKEY_FILE}"
prefix="${TSNET_PREFIX:-saarthi-spike}"
out="${YAJNA_RUN_DIR:-$(dirname "$0")}"
mkdir -p "$out"
state="$(mktemp -d)"
trap 'rm -rf "$state"' EXIT

bin="$(nix build ./spikes/11-tsnet-grpc#default --no-link --print-out-paths)/bin/tsnet-grpc"

redact() {
  python3 -I -c '
import json, sys
for line in sys.stdin:
    if not line.startswith("===RESULT=== "):
        continue
    d = json.loads(line[len("===RESULT=== "):])
    for k in ("hub", "node"):
        d[k].pop("ips", None)
    d["say"]["seen"]["peer"].pop("dns", None)
    d["say"]["seen"]["remote"] = "(tailnet address)"
    print(json.dumps(d, indent=2, sort_keys=True))
'
}

# 1. First start: joins with the key; 1000-message stream and a 60 s idle stream.
"$bin" --state "$state" --prefix "$prefix" --authkey-file "$key" 2> "$out/first.err" | redact > "$out/first.json"
# 2. Second start from the saved state without the key; no idle wait.
"$bin" --state "$state" --prefix "$prefix" --idle-secs 0 2> "$out/restart.err" | redact > "$out/restart.json"
grep -h -E 'both up|dial try' "$out/first.err" "$out/restart.err" > "$out/timeline.txt" || true
rm -f "$out/first.err" "$out/restart.err"
