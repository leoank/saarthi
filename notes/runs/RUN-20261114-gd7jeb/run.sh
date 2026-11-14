#!/usr/bin/env bash
# Re-creates RUN-20261114-gd7jeb of EXP-20261114-d82rcz on macOS (rogue). Needs a `claude setup-token`
# token in a file; only the rig's hub hop reads it, and nothing prints it:
#   CLAUDE_TOKEN_FILE=<file> notes/runs/RUN-20261114-gd7jeb/run.sh
# Writes each part's outputs into a directory of its own under this one.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
claude="$(readlink -f "$(command -v claude)")"
# The wrapper on PATH may add plugins; the run uses the bare binary it execs.
bin="$(grep -o '/nix/store/[^ "]*-claude-code-[0-9.]*/bin/claude' "$claude" | head -1 || true)"
export CLAUDE_TOKEN_FILE="${CLAUDE_TOKEN_FILE:?set CLAUDE_TOKEN_FILE}" CLAUDE_BIN="${bin:-$claude}"
nix develop -c spikes/13-hub-proxy/run.sh tui "${YAJNA_RUN_DIR:-$here}/tui"
nix develop -c spikes/13-hub-proxy/run.sh tui-direct "${YAJNA_RUN_DIR:-$here}/tui-direct"
nix develop -c spikes/13-hub-proxy/run.sh modes "${YAJNA_RUN_DIR:-$here}/modes"
nix develop -c spikes/13-hub-proxy/run.sh modes-direct "${YAJNA_RUN_DIR:-$here}/modes-direct"
