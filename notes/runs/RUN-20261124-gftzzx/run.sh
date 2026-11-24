#!/usr/bin/env bash
# Re-creates RUN-20261124-gftzzx for EXP-20261124-gfp268. Run from the repo root, inside the
# project's dev shell (for python3), at the commit recorded on the run page:
#   nix develop -c notes/runs/RUN-20261124-gftzzx/run.sh [CLAUDE]
# CLAUDE is the bare Claude Code binary (default: 2.1.330, as on rogue on 2026-11-24).
# Starts listen.py's stand-ins (the node's proxy socket, answering 401 and logging the
# `model` of each request body; a proxy refusing everything else), then runs Claude Code
# under the node's environment for a hub token with a prompt, case by case, and prints the
# models each case's /v1/messages requests asked for. Nothing leaves the machine.
set -euo pipefail
here=notes/runs/RUN-20261124-gftzzx
claude=${1:-/nix/store/wgzdk53q44byw75w8294mw6c7g550ggv-claude-code-2.1.330/bin/claude}
work=$(mktemp -d)
sock=$(mktemp -u /tmp/probe-XXXXXX).sock   # short: a unix socket path is at most 104 bytes
log=$here/probe.log
port=18766
: > "$log"
python3 "$here/listen.py" "$sock" "$port" "$log" &
listener=$!
trap 'kill $listener 2>/dev/null; rm -f "$sock"; [ -n "${KEEP:-}" ] || rm -rf "$work"' EXIT
until grep -q READY "$log"; do sleep 0.2; done

common=(PATH="/usr/bin:/bin" TERM=xterm-256color
  CLAUDE_CODE_OAUTH_TOKEN=saarthi-proxied
  ANTHROPIC_UNIX_SOCKET="$sock"
  ANTHROPIC_BASE_URL=http://api.anthropic.com
  HTTPS_PROXY="http://127.0.0.1:$port" HTTP_PROXY="http://127.0.0.1:$port"
  https_proxy="http://127.0.0.1:$port" http_proxy="http://127.0.0.1:$port")

# case NAME MODE SETTINGS_MODEL ENV... -- ARGS...: a fresh home and config each time.
case_() {
  local name=$1 mode=$2 settings=$3; shift 3
  local env=() args=()
  while [ $# -gt 0 ] && [ "$1" != -- ]; do env+=("$1"); shift; done
  [ $# -gt 0 ] && shift
  args=("$@")
  local home="$work/$name/home" config="$work/$name/config"
  mkdir -p "$home" "$config"
  [ -n "$settings" ] && printf '{"model": "%s"}\n' "$settings" > "$config/settings.json"
  # Past the first-run screens, as on a machine already logged in: onboarding (whose
  # connectivity check the refusing proxy fails) done, this directory trusted.
  printf '{"hasCompletedOnboarding": true, "theme": "dark", "projects": {"%s": {"hasTrustDialogAccepted": true}}}\n' \
    "$(cd "$work" && pwd -P)" > "$config/.claude.json"
  echo "CASE $name" >> "$log"
  if [ "$mode" = print ]; then
    (cd "$work" && timeout 25 env -i "${common[@]}" HOME="$home" CLAUDE_CONFIG_DIR="$config" ${env[@]+"${env[@]}"} \
      "$claude" -p ${args[@]+"${args[@]}"} "say hi" < /dev/null > "$work/$name.out" 2>&1) || true
  else
    # A terminal of its own, the prompt given on the command line; stopped after 20 s.
    (cd "$work" && timeout 20 python3 -c 'import pty, sys; pty.spawn(sys.argv[1:])' \
      env -i "${common[@]}" HOME="$home" CLAUDE_CONFIG_DIR="$config" ${env[@]+"${env[@]}"} \
      "$claude" ${args[@]+"${args[@]}"} "say hi" < /dev/null > "$work/$name.out" 2>&1) || true
  fi
}

case_ none            print    ""
case_ env-opus        print    ""     ANTHROPIC_MODEL=opus
case_ env-opus-flag   print    ""     ANTHROPIC_MODEL=opus -- --model sonnet
case_ settings-haiku  print    haiku 
case_ settings-env    print    haiku  ANTHROPIC_MODEL=opus
case_ env-opus-tty    terminal ""     ANTHROPIC_MODEL=opus

sleep 1
echo "--- models asked for, by case:"
awk '/^CASE/{c=$2; next} /^SOCKET POST \/v1\/messages/{print c, $NF}' "$log" | sort | uniq -c
echo "--- direct connections, by case:"
awk '/^CASE/{c=$2; next} /^DIRECT/{n[c]++} END{for (c in n) print c, n[c]}' "$log" | sort
