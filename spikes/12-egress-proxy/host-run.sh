#!/usr/bin/env bash
# EXP-20261110-g5mh89, step 1: on the host, no sandbox. Claude Code with a dummy token
# sends its API requests as plain HTTP over a unix socket to proxy.py, which adds the real
# token and relays them over TLS; a logging CONNECT proxy (HTTPS_PROXY) names whatever
# still goes direct. A headless turn, then a resume of it.
#   CLAUDE_TOKEN_FILE=<setup-token file> CLAUDE_BIN=<claude> spikes/12-egress-proxy/host-run.sh <out dir>
# Prints nothing secret: the token is only read by proxy.py.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="${1:?out dir}"
mkdir -p "$out"
P="$(mktemp -d /tmp/ep.XXXX)"   # short: a unix socket path has a length limit
mkdir -p "$P/cfg" "$P/home" "$P/work"
python3 -I "$here/proxy.py" anthropic "$P/a.sock" "${CLAUDE_TOKEN_FILE:?}" dummy-token "$out/requests.jsonl" 2>"$out/proxy.err" &
A=$!
python3 -I "$here/proxy.py" connect 18911 "$out/requests.jsonl" 2>"$out/connect.err" &
C=$!
trap 'kill $A $C 2>/dev/null; rm -rf "$P"' EXIT
sleep 1

to="$(command -v timeout)"
run() {
  (cd "$P/work" && env -i PATH=/usr/bin:/bin HOME="$P/home" CLAUDE_CONFIG_DIR="$P/cfg" USER="$USER" TERM=dumb \
    CLAUDE_CODE_OAUTH_TOKEN=dummy-token ANTHROPIC_UNIX_SOCKET="$P/a.sock" ANTHROPIC_BASE_URL=http://api.anthropic.com \
    HTTPS_PROXY=http://127.0.0.1:18911 \
    "$to" 120 "${CLAUDE_BIN:-claude}" "$@")
}
run -p "Reply with exactly: pong" --output-format json >"$out/turn1.json" 2>"$out/turn1.err" || echo "turn1 failed: $?"
sid="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['session_id'])" "$out/turn1.json")"
run -p "What did I ask you to reply with? One word." --resume "$sid" --output-format json >"$out/turn2.json" 2>"$out/turn2.err" || echo "turn2 failed: $?"

python3 - "$out" <<'EOF'
import collections, json, sys
out = sys.argv[1]
summary = {}
for f in ("turn1", "turn2"):
    d = json.load(open(f"{out}/{f}.json"))
    summary[f] = {"subtype": d.get("subtype"), "is_error": d.get("is_error"), "result": d.get("result"),
                  "models": sorted((d.get("modelUsage") or {}).keys())}
c = collections.Counter()
for line in open(f"{out}/requests.jsonl"):
    d = json.loads(line); d.pop("t", None); d.pop("session", None)
    c[json.dumps(d, sort_keys=True)] += 1
summary["requests"] = [dict(json.loads(k), n=n) for k, n in c.items()]
print("===RESULT=== " + json.dumps(summary))
EOF
