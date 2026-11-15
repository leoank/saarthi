#!/usr/bin/env bash
# RUN-20261114-rn027z (EXP-20261114-raqjjy): requests from a client through the hub to a
# local service on a node and back, with the spike relay of spikes/14-service-relay, all
# on this machine. The service is a real `yajna serve` on a scratch notebook with a 1 GB
# file and a log to append to; each request is made directly and through the relay.
# Run from the repo root: notes/runs/RUN-20261114-rn027z/run.sh
set -euo pipefail
R=$(pwd)
D=$R/notes/runs/RUN-20261114-rn027z
(cd spikes/14-service-relay && nix develop "$R" -c cargo build --release -q)
BIN=$R/spikes/14-service-relay/target/release/service-relay
E=$(mktemp -d "${TMPDIR:-/tmp}/relay.XXXX")
export XDG_CONFIG_HOME=$E/config XDG_STATE_HOME=$E/state XDG_CACHE_HOME=$E/cache
mkdir -p "$E/nb/data"
(cd "$E/nb" && git init -q && yajna init project relaytest >/dev/null)
cat >> "$E/nb/yajna.toml" <<TOML

[files]
[[files.roots]]
name = "data"
path = "data"
TOML
truncate -s 1G "$E/nb/data/big.bin"      # sparse: reads are zeros, the disk does not matter
: > "$E/nb/data/log.txt"
SP=18765 HP=18766 PP=18767
(cd "$E/nb" && exec yajna serve --port $SP > "$E/serve.log" 2>&1) &
"$BIN" hub 127.0.0.1:$HP > "$E/hub.log" 2>&1 &
pids=$(jobs -p)
trap 'kill $pids $(jobs -p) 2>/dev/null; rm -rf "$E"' EXIT
sleep 1
"$BIN" node http://127.0.0.1:$HP laptop yajna=tcp:127.0.0.1:$SP > "$E/node.log" 2>&1 &
sleep 1
"$BIN" proxy http://127.0.0.1:$HP $PP laptop yajna > "$E/proxy.log" 2>&1 &
for i in $(seq 1 30); do curl -sf -o /dev/null "http://127.0.0.1:$SP/api/info" && break; sleep 1; done
python3 -I "$D/measure.py" "$SP" "$PP" "$E/nb/data/log.txt" | tee "$D/result.txt"
