#!/usr/bin/env bash
# RUN-20261116-ca6zg1 (EXP-20261114-raqjjy): does HTTP/2's default flow-control window (64 KB per stream,
# which tonic keeps unless told) cap the relay over a long link, and does tonic's adaptive
# window lift it? RUN-20261115-2t8qqy could not tell, its link being slower than the cap.
# Here the link is simulated on this machine: the node and the client each reach the hub
# through the spike's `delay` forwarder, 43 ms each way (86 ms round trip, ghost001's from
# this Mac), no bandwidth limit. Measured twice, SPIKE_ADAPTIVE=0 then 1, with the same
# requests as RUN-20261115-2t8qqy. Run from the repo root.
set -euo pipefail
R=$(pwd)
D=$R/notes/runs/RUN-20261116-ca6zg1
(cd spikes/14-service-relay && nix develop "$R" -c cargo build --release -q)
BIN=$R/spikes/14-service-relay/target/release/service-relay
E=$(mktemp -d "${TMPDIR:-/tmp}/relaywin.XXXX")
Y="env XDG_CONFIG_HOME=$E/config XDG_STATE_HOME=$E/state XDG_CACHE_HOME=$E/cache yajna"
mkdir -p "$E/nb/data"
(cd "$E/nb" && git init -q && $Y init project relaytest >/dev/null)
printf '\n[files]\n[[files.roots]]\nname = "data"\npath = "data"\n' >> "$E/nb/yajna.toml"
truncate -s 1G "$E/nb/data/big.bin"
: > "$E/nb/data/log.txt"
SP=18765 HP=18766 PP=18767 NL=18768 CL=18769
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$E"' EXIT
(cd "$E/nb" && exec $Y serve --port $SP > "$E/serve.log" 2>&1) &
"$BIN" delay 127.0.0.1:$NL 127.0.0.1:$HP 43 &
"$BIN" delay 127.0.0.1:$CL 127.0.0.1:$HP 43 &
for i in $(seq 1 30); do curl -sf -o /dev/null "http://127.0.0.1:$SP/api/info" && break; sleep 1; done
for mode in 0 1; do
  SPIKE_ADAPTIVE=$mode "$BIN" hub 127.0.0.1:$HP > "$E/hub.log" 2>&1 & hub=$!
  sleep 1
  SPIKE_ADAPTIVE=$mode "$BIN" node "http://127.0.0.1:$NL" laptop yajna=tcp:127.0.0.1:$SP > "$E/node.log" 2>&1 & node=$!
  sleep 1
  SPIKE_ADAPTIVE=$mode "$BIN" proxy "http://127.0.0.1:$CL" $PP laptop yajna > "$E/proxy.log" 2>&1 & proxy=$!
  for i in $(seq 1 30); do curl -sf -o /dev/null "http://127.0.0.1:$PP/api/info" && break; sleep 1; done
  python3 -I "$D/measure.py" "$SP" "$PP" "$E/nb/data/log.txt" | tee "$D/result-adaptive-$mode.txt"
  kill $proxy $node $hub; wait $proxy $node $hub 2>/dev/null || true
done
