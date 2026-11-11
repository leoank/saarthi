#!/usr/bin/env bash
# EXP-20261110-g5mht5, the alternatives after the usage endpoint refused a setup-token
# (RUN-20261111-q60gf1): what gives an account's 5-hour and 7-day utilisation from such
# a token, and at what cost?
#  1. `claude -p "/usage"` in an empty config directory with the token in the environment;
#  2. POST /v1/messages/count_tokens (no model turn): the rate-limit response headers;
#  3. POST /v1/messages, Haiku, max_tokens 1: the rate-limit response headers.
# The token is read from TOKEN_FILE into a header file or the child's environment only.
# Prints the HTTP status and every `anthropic-ratelimit-unified-*` header (utilisations
# and reset times, no account details), and /usage's result text and cost.
# Usage: TOKEN_FILE=... notes/runs/RUN-20261111-q6afjv/run.sh
set -euo pipefail
: "${TOKEN_FILE:?set TOKEN_FILE to a file holding a setup-token}"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
umask 077
token=$(tr -d '[:space:]' < "$TOKEN_FILE")
printf 'Authorization: Bearer %s\n' "$token" > "$tmp/h"

echo "--- 1. claude -p /usage"
mkdir -p "$tmp/cfg"
CLAUDE_CONFIG_DIR="$tmp/cfg" CLAUDE_CODE_OAUTH_TOKEN="$token" \
  claude -p "/usage" --output-format json < /dev/null > "$tmp/usage.json" 2> "$tmp/usage.err" || true
python3 - "$tmp/usage.json" <<'PY'
import json, sys
try:
    r = json.load(open(sys.argv[1]))
    print(json.dumps({"subtype": r.get("subtype"), "is_error": r.get("is_error"),
                      "cost_usd": r.get("total_cost_usd"), "num_turns": r.get("num_turns"),
                      "result": (r.get("result") or "")[:600]}))
except Exception as e:
    print("no JSON result:", e, open(sys.argv[1]).read()[:300])
PY

probe() {  # $1 label, $2 path, $3 body
  echo "--- $1"
  code=$(curl -sS -D "$tmp/hdr" -o "$tmp/body" -w '%{http_code}' \
    -H @"$tmp/h" -H 'anthropic-beta: oauth-2025-04-20' -H 'anthropic-version: 2023-06-01' \
    -H 'content-type: application/json' -d "$3" "https://api.anthropic.com$2")
  echo "status $code"
  grep -i '^anthropic-ratelimit-unified' "$tmp/hdr" | tr -d '\r' || echo "(no unified rate-limit headers)"
  [ "$code" = 200 ] || python3 -c 'import json,sys; b=json.load(open(sys.argv[1])); print(b.get("error"))' "$tmp/body" 2>/dev/null || true
  [ "$code" = 200 ] && python3 -c 'import json,sys; b=json.load(open(sys.argv[1])); print("usage", b.get("usage"))' "$tmp/body" 2>/dev/null || true
}
sys='"system":[{"type":"text","text":"You are Claude Code, Anthropic'"'"'s official CLI for Claude."}]'
probe "2. count_tokens" /v1/messages/count_tokens \
  '{"model":"claude-haiku-4-5",'"$sys"',"messages":[{"role":"user","content":"hi"}]}'
probe "3. messages, Haiku, max_tokens 1" /v1/messages \
  '{"model":"claude-haiku-4-5","max_tokens":1,'"$sys"',"messages":[{"role":"user","content":"hi"}]}'
