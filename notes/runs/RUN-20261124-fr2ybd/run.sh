#!/usr/bin/env bash
# Re-creates RUN-20261124-fr2ybd for EXP-20261124-1pg2cq: does the policy-limits answer a hub
# token gets (RUN-20261124-fbreex: 200, with `defaults`) carry the organization's default
# model? The owner's organizations set it to Opus, yet a session saarthi starts under a hub
# token gets Sonnet: Claude Code's own call for it goes out with the dummy token (RUN-20261124-1qtntf).
#
# Runs ON THE HUB, as a user that can read its token file (root). sudo needs a terminal:
#   scp notes/runs/RUN-20261124-fr2ybd/run.sh ghost001:/tmp/hub-token-probe.sh
#   ssh -t ghost001 'sudo bash /tmp/hub-token-probe.sh 2>&1 | tee /tmp/hub-token-probe.out; rm /tmp/hub-token-probe.sh'
# TOKENS, if set, is the file of `<name> <token>` lines (default: the hub module's).
#
# Prints the shape of the policy-limits answer: every key path with its value's type, and
# the value itself only where the path names a model or effort (an alias or id, nothing
# private). Never a token.
set -euo pipefail
set +x
if ! command -v python3 >/dev/null; then
  exec nix --extra-experimental-features 'nix-command flakes' shell nixpkgs#python3 -c bash "$0" "$@"
fi
tokens=${TOKENS:-/run/agenix/saarthiClaudeTokens}
body=$(mktemp)
trap 'rm -f "$body"' EXIT

shape() { python3 -c 'import json,re,sys
shown=re.compile(r"model|effort", re.I)
def walk(path, v):
    if isinstance(v, dict):
        if not v: print(f"  {path} = {{}}")
        for k in sorted(v): walk(f"{path}.{k}" if path else k, v[k])
    elif isinstance(v, list):
        print(f"  {path} : list[{len(v)}]")
        for i, x in enumerate(v[:5]): walk(f"{path}[{i}]", x)
    else:
        t = type(v).__name__
        if shown.search(path) and (v is None or isinstance(v, (str, bool, int, float))): print(f"  {path} : {t} = {v!r}")
        else: print(f"  {path} : {t}")
walk("", json.load(open(sys.argv[1])))' "$1"; }

while read -r name token; do
  [ -z "${name:-}" ] || [ "${name#\#}" != "$name" ] && continue
  code=$(curl -sS -o "$body" -w '%{http_code}' \
    -H "Authorization: Bearer $token" -H "anthropic-beta: oauth-2025-04-20" -H "anthropic-version: 2023-06-01" \
    https://api.anthropic.com/api/claude_code/policy_limits || echo 000)
  echo "$name GET /api/claude_code/policy_limits -> $code"
  [ "$code" = 200 ] && shape "$body"
done < "$tokens"
