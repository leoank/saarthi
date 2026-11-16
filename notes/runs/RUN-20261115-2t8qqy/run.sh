#!/usr/bin/env bash
# RUN-20261115-2t8qqy (EXP-20261114-raqjjy): the spike relay with its hub on ghost001, the hub
# host, and the node, the client and `yajna serve` on this machine, so every request crosses
# to ghost001 and back twice (client to hub to node, and the answer the same way). The hub is
# the spike built for aarch64-linux (spikes/14-service-relay/flake.nix), run from ghost001's
# store on its loopback and reached through an ssh tunnel, as this machine's own network may
# not resolve the hub's name. Sizes are smaller than RUN-20261114-rn027z's, for a real link.
# Run from the repo root; needs ssh to ghost001.
set -euo pipefail
R=$(pwd)
D=$R/notes/runs/RUN-20261115-2t8qqy
H=ghost001
HP=17766 SP=18765 PP=18767
(cd spikes/14-service-relay && nix develop "$R" -c cargo build --release -q)
BIN=$R/spikes/14-service-relay/target/release/service-relay
REMOTE=$(cd spikes/14-service-relay && nix build .#packages.aarch64-linux.default --no-link --print-out-paths \
  --builders "ssh-ng://nixremote@$H aarch64-linux /etc/nix/remote-build-key 2 1 benchmark - -")
ssh -o BatchMode=yes "$H" "test -x $REMOTE/bin/service-relay"   # built there, so in its store
E=$(mktemp -d "${TMPDIR:-/tmp}/relaywan.XXXX")
Y="env XDG_CONFIG_HOME=$E/config XDG_STATE_HOME=$E/state XDG_CACHE_HOME=$E/cache yajna"
mkdir -p "$E/nb/data"
(cd "$E/nb" && git init -q && $Y init project relaytest >/dev/null)
printf '\n[files]\n[[files.roots]]\nname = "data"\npath = "data"\n' >> "$E/nb/yajna.toml"
truncate -s 1G "$E/nb/data/big.bin"
: > "$E/nb/data/log.txt"
cleanup() {
  kill $(jobs -p) 2>/dev/null || true
  ssh -o BatchMode=yes "$H" "pkill -f '$REMOTE/bin/service-relay hub' || true"
  rm -rf "$E"
}
trap cleanup EXIT
ssh -o BatchMode=yes "$H" "nohup timeout 900 $REMOTE/bin/service-relay hub 127.0.0.1:$HP > /dev/null 2>&1 &"
ssh -o BatchMode=yes -o ExitOnForwardFailure=yes -N -L "$HP:127.0.0.1:$HP" "$H" &
sleep 3
(cd "$E/nb" && exec $Y serve --port $SP > "$E/serve.log" 2>&1) &
"$BIN" node "http://127.0.0.1:$HP" laptop yajna=tcp:127.0.0.1:$SP > "$E/node.log" 2>&1 &
sleep 2
"$BIN" proxy "http://127.0.0.1:$HP" $PP laptop yajna > "$E/proxy.log" 2>&1 &
for i in $(seq 1 30); do curl -sf -o /dev/null "http://127.0.0.1:$SP/api/info" && break; sleep 1; done
for i in $(seq 1 30); do curl -sf -o /dev/null "http://127.0.0.1:$PP/api/info" && break; sleep 1; done
# The link's round trip, for scale: an ssh exec to the hub host and back, ten times.
python3 - "$H" > "$D/rtt.txt" <<'PY'
import statistics, subprocess, sys, time
t = []
for _ in range(10):
    a = time.perf_counter(); subprocess.run(["ssh", "-o", "BatchMode=yes", sys.argv[1], "true"], check=True); t.append((time.perf_counter() - a) * 1000)
print(f"ssh exec round trip ms p50={statistics.median(t):.0f} min={min(t):.0f}")
PY
ping -c 10 -q "$H" | tail -1 >> "$D/rtt.txt" || true
python3 -I "$D/measure.py" "$SP" "$PP" "$E/nb/data/log.txt" | tee "$D/result.txt"
