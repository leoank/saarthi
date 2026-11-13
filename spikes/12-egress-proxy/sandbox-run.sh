#!/usr/bin/env bash
# EXP-20261110-g5mh89, step 2: the same inside mxc's seatbelt sandbox under the node's
# network policy (egress allowed, host loopback denied). proxy.py listens on a unix socket
# in the sandbox's read-write state directory; claude inside holds only a dummy token.
# Checks from inside: loopback TCP refused, the token file unreadable, only the dummy in the
# environment; then a headless turn and a resume.
#   CLAUDE_TOKEN_FILE=<setup-token file> CLAUDE_BIN=<claude> spikes/12-egress-proxy/sandbox-run.sh <out dir>
# Run inside the project's dev shell on macOS. Prints nothing secret.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="${1:?out dir}"
mkdir -p "$out"
token_file="$(realpath "${CLAUDE_TOKEN_FILE:?}")"
claude="${CLAUDE_BIN:?}"
cargo build -q --manifest-path "$here/Cargo.toml"
box="$here/target/debug/egress-proxy-spike"

S="$(realpath "$(mktemp -d /tmp/es.XXXX)")"   # short: a unix socket path has a length limit
W="$S/work"
mkdir -p "$S/claude" "$S/home" "$S/tmp" "$W"
python3 -I "$here/proxy.py" anthropic "$S/proxy.sock" "$token_file" dummy-token "$out/requests.jsonl" 2>"$out/proxy.err" &
A=$!
python3 -I "$here/proxy.py" connect 18911 "$out/loopback.jsonl" 2>/dev/null &
C=$!
trap 'kill $A $C 2>/dev/null; rm -rf "$S"' EXIT
sleep 1

env=(PATH="$(dirname "$claude"):/usr/bin:/bin" HOME="$S/home" USER="$USER" TERM=dumb LANG=en_US.UTF-8
  CLAUDE_CONFIG_DIR="$S/claude" TMPDIR="$S/tmp" CLAUDE_CODE_TMPDIR="$S/tmp"
  CLAUDE_CODE_OAUTH_TOKEN=dummy-token ANTHROPIC_UNIX_SOCKET="$S/proxy.sock" ANTHROPIC_BASE_URL=http://api.anthropic.com)
inside() { "$box" "$S" "$W" "$1" "${env[@]}"; }

{
  echo "loopback: $(inside '/usr/bin/curl -s -m 5 -o /dev/null -w %{http_code} http://127.0.0.1:18911/ || echo refused')"
  echo "token file: $(inside "/bin/cat '$token_file' >/dev/null 2>&1 && echo readable || echo unreadable")"
  echo "token env: $(inside 'echo "$CLAUDE_CODE_OAUTH_TOKEN"')"
  echo "internet: $(inside '/usr/bin/curl -s -m 10 -o /dev/null -w %{http_code} https://example.com/ || echo refused')"
} | tee "$out/checks.txt"

to="$(command -v timeout)"
"$to" 150 "$box" "$S" "$W" "exec claude -p 'Reply with exactly: pong' --output-format json" "${env[@]}" >"$out/turn1.json" 2>"$out/turn1.err" || echo "turn1 failed: $?"
sid="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['session_id'])" "$out/turn1.json")"
"$to" 150 "$box" "$S" "$W" "exec claude -p 'What did I ask you to reply with? One word.' --resume $sid --output-format json" "${env[@]}" >"$out/turn2.json" 2>"$out/turn2.err" || echo "turn2 failed: $?"

python3 - "$out" <<'EOF'
import collections, json, sys
out = sys.argv[1]
summary = {"checks": dict(l.rstrip("\n").split(": ", 1) for l in open(f"{out}/checks.txt"))}
for f in ("turn1", "turn2"):
    d = json.load(open(f"{out}/{f}.json"))
    summary[f] = {"subtype": d.get("subtype"), "is_error": d.get("is_error"), "result": d.get("result")}
c = collections.Counter()
for line in open(f"{out}/requests.jsonl"):
    d = json.loads(line); d.pop("t", None)
    c[json.dumps(d, sort_keys=True)] += 1
summary["requests"] = [dict(json.loads(k), n=n) for k, n in c.items()]
print("===RESULT=== " + json.dumps(summary))
EOF
