#!/usr/bin/env bash
# Re-creates RUN-20261113-2s4d8h of EXP-20261110-g5mh89 on macOS (rogue). Needs a `claude setup-token`
# token in a mode-600 file; it is read only by the proxy and never printed:
#   CLAUDE_TOKEN_FILE=<file> notes/runs/RUN-20261113-2s4d8h/run.sh
# Writes requests.jsonl, the turns' JSON and the ===RESULT=== line into this directory.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
claude="$(readlink -f "$(command -v claude)")"
# The wrapper on PATH may add plugins; the run uses the bare binary it execs.
bin="$(grep -o '/nix/store/[^ "]*-claude-code-[0-9.]*/bin/claude' "$claude" | head -1 || true)"
export CLAUDE_TOKEN_FILE="${CLAUDE_TOKEN_FILE:?set CLAUDE_TOKEN_FILE}" CLAUDE_BIN="${bin:-$claude}"
nix develop -c spikes/12-egress-proxy/sandbox-run.sh "${YAJNA_RUN_DIR:-$here}" | tee "${YAJNA_RUN_DIR:-$here}/result.txt"
