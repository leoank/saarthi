#!/usr/bin/env bash
# Re-creates RUN-20261124-1ypdgw for EXP-20261124-1pg2cq (H2): does a hub token, a
# `claude setup-token` token as the hub holds it (ADR-0039), get answers from the endpoints
# Claude Code fetches its organization's managed settings and policy limits from?
#
# Runs ON THE HUB, as a user that can read its token file (root):
#   ssh ghost001 sudo bash -s < notes/runs/RUN-20261124-1ypdgw/run.sh
# TOKENS, if set, is the file of `<name> <token>` lines (default: the hub module's).
#
# Prints, per token and path, the HTTP status, the content type, the body's size and its
# top-level JSON keys. Never a token, never a value from a body. Read-only GETs, plus one
# `count_tokens` POST as a control that the token works at all (it costs no usage).
set -euo pipefail
set +x
tokens=${TOKENS:-/run/agenix/saarthiClaudeTokens}
api=https://api.anthropic.com
paths=(/api/claude_code/settings /api/claude_code/policy_limits /api/oauth/profile)
body=$(mktemp)
trap 'rm -f "$body"' EXIT

keys() { python3 -c 'import json,sys
try:
    d=json.load(open(sys.argv[1]))
    print(",".join(sorted(d)) if isinstance(d,dict) else type(d).__name__)
except Exception: print("-")' "$1" 2>/dev/null || echo "-"; }

while read -r name token; do
  [ -z "${name:-}" ] || [ "${name#\#}" != "$name" ] && continue
  for beta in oauth-2025-04-20 none; do
    hdr=()
    [ "$beta" != none ] && hdr=(-H "anthropic-beta: $beta")
    for p in "${paths[@]}"; do
      out=$(curl -sS -o "$body" -w '%{http_code} %{content_type} %{size_download}' \
        -H "Authorization: Bearer $token" "${hdr[@]}" -H "anthropic-version: 2023-06-01" "$api$p" || echo "000 - 0")
      echo "$name beta=$beta GET $p -> $out keys=$(keys "$body")"
    done
  done
  # The control: the token is good for the API at all.
  out=$(curl -sS -o "$body" -w '%{http_code} %{size_download}' -X POST \
    -H "Authorization: Bearer $token" -H "anthropic-beta: oauth-2025-04-20" -H "anthropic-version: 2023-06-01" \
    -H "content-type: application/json" \
    -d '{"model":"claude-haiku-4-5","messages":[{"role":"user","content":"hi"}]}' \
    "$api/v1/messages/count_tokens" || echo "000 0")
  echo "$name control POST /v1/messages/count_tokens -> $out keys=$(keys "$body")"
done < "$tokens"
