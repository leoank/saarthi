#!/usr/bin/env bash
# EXP-20261111-q6sgt8: in the owner's default config directory (logged in, keychain),
# does a session given CLAUDE_CODE_OAUTH_TOKEN run under that token? One Haiku turn
# without it and one with it, from a directory the node daemon ignores
# (`*/target/live-work-*`), comparing what Claude Code reports: the stream-json init's
# `apiKeySource`, `claude auth status`, and the rate_limit_event's windows. The token
# goes into the child's environment only; nothing about the account but those fields is
# printed.
# Usage: TOKEN_FILE=... notes/runs/RUN-20261111-q6t69j/run.sh
set -euo pipefail
: "${TOKEN_FILE:?set TOKEN_FILE to a file holding a setup-token}"
work=$(mktemp -d)/target/live-work-q6t69j; mkdir -p "$work"; cd "$work"
token=$(tr -d '[:space:]' < "$TOKEN_FILE")
turn() {  # $1 label, then env assignments
  label=$1; shift
  echo "--- $label"
  env "$@" claude auth status 2>&1 | python3 -c '
import json,sys
t=sys.stdin.read()
try:
    d=json.loads(t); print("auth status:", {k:d.get(k) for k in ("loggedIn","authMethod","apiProvider","subscriptionType")})
except ValueError:
    print("auth status:", t.strip()[:200])'
  env "$@" claude -p "Reply with the single word ok." --model haiku \
    --output-format stream-json --verbose < /dev/null 2>/dev/null | python3 -c '
import json,sys
for line in sys.stdin:
    try: m=json.loads(line)
    except ValueError: continue
    if m.get("type")=="system" and m.get("subtype")=="init":
        print("init apiKeySource:", m.get("apiKeySource"))
    elif m.get("type")=="rate_limit_event":
        i=m.get("rate_limit_info",{})
        print("rate_limit_event:", {k:v.get("utilization") for k,v in (i.get("unifiedWindows") or {}).items()})
    elif m.get("type")=="result":
        print("result:", m.get("subtype"), "error" if m.get("is_error") else "ok", repr((m.get("result") or "")[:40]))'
}
turn "owner login (no token in the environment)" CLAUDE_CODE_OAUTH_TOKEN=
turn "with CLAUDE_CODE_OAUTH_TOKEN" CLAUDE_CODE_OAUTH_TOKEN="$token"
rm -rf "$(dirname "$(dirname "$work")")"
