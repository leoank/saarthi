#!/usr/bin/env bash
# Re-creates RUN-20261124-fbreex for EXP-20261124-1pg2cq (H2), RUN-20261124-1ypdgw again
# with what that run could not show: the top-level keys of a 200 (ghost001 has no python3,
# so this script fetches one through nix) and, for any other status, the error's type and
# message, to tell "no managed settings for this organization" from "not for this token".
#
# Runs ON THE HUB, as a user that can read its token file (root). sudo needs a terminal:
#   scp notes/runs/RUN-20261124-fbreex/run.sh ghost001:/tmp/hub-token-probe.sh
#   ssh -t ghost001 'sudo bash /tmp/hub-token-probe.sh 2>&1 | tee /tmp/hub-token-probe.out; rm /tmp/hub-token-probe.sh'
# TOKENS, if set, is the file of `<name> <token>` lines (default: the hub module's).
#
# Prints per token and path the HTTP status, content type, size, and the keys of a 200 or
# the error type and message (cut to 200 characters) of anything else. Never a token, never
# a value from a successful body. Read-only GETs, plus one `count_tokens` POST as a control.
set -euo pipefail
set +x
if ! command -v python3 >/dev/null; then
  exec nix --extra-experimental-features 'nix-command flakes' shell nixpkgs#python3 -c bash "$0" "$@"
fi
tokens=${TOKENS:-/run/agenix/saarthiClaudeTokens}
api=https://api.anthropic.com
paths=(/api/claude_code/settings /api/claude_code/policy_limits /api/oauth/profile)
body=$(mktemp)
trap 'rm -f "$body"' EXIT

# What a body shows: keys for a success, the error's type and message otherwise.
shown() { python3 -c 'import json,sys
ok=sys.argv[2].startswith("2")
try:
    d=json.load(open(sys.argv[1]))
except Exception:
    print("not-json"); sys.exit()
if ok or not isinstance(d,dict):
    print("keys="+(",".join(sorted(d)) if isinstance(d,dict) else type(d).__name__))
else:
    e=d.get("error",d)
    e=e if isinstance(e,dict) else {"message":str(e)}
    print("error=%s message=%r" % (e.get("type","-"), str(e.get("message",""))[:200]))' "$1" "$2" 2>/dev/null || echo "-"; }

while read -r name token; do
  [ -z "${name:-}" ] || [ "${name#\#}" != "$name" ] && continue
  for p in "${paths[@]}"; do
    out=$(curl -sS -o "$body" -w '%{http_code} %{content_type} %{size_download}' \
      -H "Authorization: Bearer $token" -H "anthropic-beta: oauth-2025-04-20" -H "anthropic-version: 2023-06-01" \
      "$api$p" || echo "000 - 0")
    echo "$name GET $p -> $out $(shown "$body" "${out%% *}")"
  done
  out=$(curl -sS -o "$body" -w '%{http_code} %{size_download}' -X POST \
    -H "Authorization: Bearer $token" -H "anthropic-beta: oauth-2025-04-20" -H "anthropic-version: 2023-06-01" \
    -H "content-type: application/json" \
    -d '{"model":"claude-haiku-4-5","messages":[{"role":"user","content":"hi"}]}' \
    "$api/v1/messages/count_tokens" || echo "000 0")
  echo "$name control POST /v1/messages/count_tokens -> $out $(shown "$body" "${out%% *}")"
done < "$tokens"
