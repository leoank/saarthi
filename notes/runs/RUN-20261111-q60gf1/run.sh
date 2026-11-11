#!/usr/bin/env bash
# EXP-20261110-g5mht5: does GET /api/oauth/usage accept a `claude setup-token` token?
# Calls the endpoint once with the headers the node's usage sampler sends
# (crates/node/src/usage.rs), the token from TOKEN_FILE passed to curl in a header file so
# it is never on a command line or printed. Records the status and the response's
# top-level keys only, never the body's figures or the account's details.
# Usage: TOKEN_FILE=~/.config/saarthi/sandbox-claude-token notes/runs/RUN-20261111-q60gf1/run.sh
set -euo pipefail
: "${TOKEN_FILE:?set TOKEN_FILE to a file holding a setup-token}"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
umask 077
printf 'Authorization: Bearer %s\n' "$(tr -d '[:space:]' < "$TOKEN_FILE")" > "$tmp/h"
status=$(curl -sS -o "$tmp/body" -w '%{http_code}' \
  -H @"$tmp/h" -H 'anthropic-beta: oauth-2025-04-20' \
  https://api.anthropic.com/api/oauth/usage)
python3 - "$status" "$tmp/body" <<'PY'
import json, sys
status, path = int(sys.argv[1]), sys.argv[2]
raw = open(path).read()
try:
    body = json.loads(raw)
except ValueError:
    body = None
keys = sorted(body) if isinstance(body, dict) else []
# for an error, its type and message say why; they carry no secret
err = body.get("error") if isinstance(body, dict) else None
err = {k: err.get(k) for k in ("type", "message")} if isinstance(err, dict) else None
windows = {k: sorted(v) for k, v in body.items() if isinstance(v, dict)} if isinstance(body, dict) and status == 200 else {}
print("===RESULT===", json.dumps({"status": status, "keys": keys, "window_fields": windows, "error": err}))
PY
