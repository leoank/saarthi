#!/usr/bin/env bash
# Re-creates RUN-20261124-1qtntf for EXP-20261124-1pg2cq. Run from the repo root, inside the
# project's dev shell (for python3), at the commit recorded on the run page:
#   nix develop -c notes/runs/RUN-20261124-1qtntf/run.sh [CLAUDE]
# EXTRA, if set, is one more NAME=value given to Claude Code (a later run sets
# CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1); LOG, if set, where the log goes.
# CLAUDE is the bare Claude Code binary (default: 2.1.330, as on rogue on 2026-11-24); not
# neusis's wrapper, which at a terminal would start `saarthi run` against the real node.
# Starts listen.py's stand-ins, runs Claude Code under the node's environment for a session
# under a hub token (ADR-0051) once in -p mode and once at a terminal, and counts what each
# stand-in saw. Nothing leaves the machine: the socket answers 401, the proxy refuses all.
set -euo pipefail
here=notes/runs/RUN-20261124-1qtntf
claude=${1:-/nix/store/wgzdk53q44byw75w8294mw6c7g550ggv-claude-code-2.1.330/bin/claude}
work=$(mktemp -d)
# Short: a unix socket path has at most 104 bytes on macOS.
sock=$(mktemp -u /tmp/probe-XXXXXX).sock
log=${LOG:-$here/probe.log}
port=18765
: > "$log"
python3 "$here/listen.py" "$sock" "$port" "$log" &
listener=$!
trap 'kill $listener 2>/dev/null; rm -f "$sock"; rm -rf "$work"' EXIT
until grep -q READY "$log"; do sleep 0.2; done

# What the node gives a session under a hub token, and a fresh config: no settings, no
# hooks, no login of this machine's. The proxies: everything not for the socket.
common=(HOME="$work/home" PATH="/usr/bin:/bin" TERM=xterm-256color
  CLAUDE_CODE_OAUTH_TOKEN=saarthi-proxied
  ANTHROPIC_UNIX_SOCKET="$sock"
  ANTHROPIC_BASE_URL=http://api.anthropic.com
  HTTPS_PROXY="http://127.0.0.1:$port" HTTP_PROXY="http://127.0.0.1:$port"
  https_proxy="http://127.0.0.1:$port" http_proxy="http://127.0.0.1:$port"
  ${EXTRA:+"$EXTRA"})
mkdir -p "$work/home"

echo "MODE print" >> "$log"
(cd "$work" && timeout 25 env -i "${common[@]}" CLAUDE_CONFIG_DIR="$work/config-print" "$claude" -p "say hi" < /dev/null > "$work/print.out" 2>&1) || true

echo "MODE terminal" >> "$log"
# A terminal of its own (a pty from Python: `script` wants one on its own input); Claude
# Code is left at its first screen and stopped after 20 s.
(cd "$work" && timeout 20 python3 -c 'import pty, sys; pty.spawn(sys.argv[1:])' \
  env -i "${common[@]}" CLAUDE_CONFIG_DIR="$work/config-terminal" "$claude" < /dev/null > "$work/terminal.out" 2>&1) || true

sleep 1
echo "--- print mode said:"; tail -c 400 "$work/print.out"; echo
echo "--- the terminal showed (text only):"; sed -E 's/\x1b\[[0-9;?]*[a-zA-Z]//g; s/\x1b[]()][^\x07]*\x07?//g' "$work/terminal.out" | tr -s ' \r' | tail -c 600; echo
echo "--- what came, by mode:"
awk '/^MODE/{m=$2; next} /^(SOCKET|DIRECT)/{print m, $0}' "$log" | sort | uniq -c | sort -k2,2 -k1,1nr
api=$(grep -c '^SOCKET [A-Z]* /api/' "$log" || true)
direct=$(grep -c '^DIRECT' "$log" || true)
socket=$(grep -c '^SOCKET' "$log" || true)
echo "===RESULT=== {\"socket_requests\": $socket, \"socket_api_requests\": $api, \"direct_requests\": $direct}"
