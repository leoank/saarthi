#!/usr/bin/env bash
# EXP-20261114-d82rcz: Claude Code through a node hop (unix socket) and a hub hop (TCP) that
# adds the token, as in the design where the hub holds every hub token.
#   CLAUDE_TOKEN_FILE=<setup-token file> CLAUDE_BIN=<claude> spikes/13-hub-proxy/run.sh tui|tui-direct|modes|modes-direct|restart|bytes <out dir>
# tui:     an interactive claude in a PTY, fresh config directory, two prompts.
# tui-direct: the same with the real token in its environment and no socket, the baseline.
# modes:   a headless turn with a shell command in auto mode and in manual; modes-direct the baseline.
# restart: a long streamed answer; the hub exits mid-stream and is back after 2, 15 and
#          90 s; then a turn started while the hub is down for 20 s; a resume after each.
# bytes:   a headless session with tools over a copy of the crates, and two resumes.
# Prints nothing secret: the token is read only by the hub hop. Run in the dev shell on macOS.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
part="${1:?tui|restart|bytes}"
out="$(mkdir -p "${2:?out dir}" && cd "$2" && pwd)"
token_file="${CLAUDE_TOKEN_FILE:?}"   # not resolved: an agenix link can move between hub starts
claude="${CLAUDE_BIN:?}"
port=18931
P="$(realpath "$(mktemp -d /tmp/hp.XXXX)")"   # short: a unix socket path has a length limit
mkdir -p "$P/cfg" "$P/home" "$P/work"
H=""
hub() { python3 -I "$here/rig.py" hub $port "$token_file" "$1" "${@:2}" 2>>"$out/hub.err" & H=$!; sleep 0.5; }
python3 -I "$here/rig.py" node "$P/a.sock" $port "$out/node.jsonl" 2>"$out/node.err" &
N=$!
trap 'kill $N $H 2>/dev/null; rm -rf "$P"' EXIT
sleep 0.5
to="$(command -v timeout)"
envs=(PATH=/usr/bin:/bin HOME="$P/home" CLAUDE_CONFIG_DIR="$P/cfg" USER="$USER" LANG=en_US.UTF-8
  CLAUDE_CODE_OAUTH_TOKEN=saarthi-proxied ANTHROPIC_UNIX_SOCKET="$P/a.sock" ANTHROPIC_BASE_URL=http://api.anthropic.com)
cl() { (cd "$P/work" && env -i "${envs[@]}" TERM=dumb "$to" "$1" "$claude" "${@:2}"); }
field() { python3 -c "import json,sys
try: print(json.load(open(sys.argv[1])).get(sys.argv[2]))
except Exception as e: print('unreadable')" "$1" "$2"; }

case "$part" in
tui|tui-direct)
  hub "$out/requests.jsonl"
  # The baseline: the real token in the environment and no socket, as ADR-0039 delivers it.
  [ "$part" = tui-direct ] && envs=("${envs[@]:0:5}" CLAUDE_CODE_OAUTH_TOKEN="$(tr -d '\n' < "$token_file")")
  python3 -c "import json,sys; json.dump({'hasCompletedOnboarding': True, 'theme': 'dark',
    'projects': {sys.argv[2]: {'hasTrustDialogAccepted': True}}}, open(sys.argv[1], 'w'))" "$P/cfg/.claude.json" "$P/work"
  env -i "${envs[@]}" TERM=xterm-256color python3 -I "$here/tui.py" "$out" "$claude" "$P/work" \
    "pong=Reverse the letters of the word gnop and reply with only the result, in lowercase." \
    "Kathmandu=What is the capital of Nepal? One word." \
    'automodeon=KEY:\x1b[Z' 'automodeon=KEY:\x1b[Z' 'automodeon=KEY:\x1b[Z' 'automodeon=KEY:\x1b[Z' | tee "$out/steps.jsonl"
  ;;
modes|modes-direct)
  # A spawn asks for auto mode, as the app can (ADR-0037): what the session's init reports.
  hub "$out/requests.jsonl"
  [ "$part" = modes-direct ] && envs=("${envs[@]:0:5}" CLAUDE_CODE_OAUTH_TOKEN="$(tr -d '\n' < "$token_file")")
  for mode in auto manual; do
    rm -f "$P/work/note.txt"
    cl 180 -p "Run the shell command: echo saarthi > note.txt. Then reply done." --permission-mode $mode --output-format stream-json --verbose >"$out/$part-$mode.jsonl" 2>"$out/$part-$mode.err" || true
    python3 -c "import json,sys
for l in open(sys.argv[1]):
    d=json.loads(l)
    if d.get('type')=='system' and d.get('subtype')=='init': print('asked', sys.argv[2], 'init permissionMode', d.get('permissionMode'), 'model', d.get('model'))
    if d.get('type')=='result': print('  result', d.get('subtype'), repr(d.get('result'))[:80], 'denials', len(d.get('permission_denials') or []))" "$out/$part-$mode.jsonl" $mode
    echo "  note.txt: $(cat "$P/work/note.txt" 2>/dev/null || echo absent)"
  done | tee "$out/$part.txt"
  ;;
restart)
  ask="Write every whole number from 1 to 400 in order, separated by single spaces, and nothing else."
  for gap in 2 15 90; do
    log="$out/restart-$gap.jsonl"
    hub "$log" --die-after 2000
    s=$(date +%s)
    cl 600 -p "$ask" --output-format json >"$out/restart-$gap.json" 2>"$out/restart-$gap.err" &
    C=$!
    for i in $(seq 1 240); do kill -0 $H 2>/dev/null || break; sleep 0.5; done
    sleep "$gap"
    hub "$log"
    wait $C && rc=0 || rc=$?
    took=$(( $(date +%s) - s ))
    sid=$(field "$out/restart-$gap.json" session_id)
    cl 120 -p "What was the last number you wrote? The number only." --resume "$sid" --output-format json \
      >"$out/restart-$gap-resume.json" 2>>"$out/restart-$gap.err" || true
    kill $H 2>/dev/null || true; wait $H 2>/dev/null || true
    echo "gap=$gap exit=$rc secs=$took"
  done | tee "$out/restart.txt"
  log="$out/down-at-start.jsonl"
  s=$(date +%s)
  cl 600 -p "Reply with exactly: pong" --output-format json >"$out/down-at-start.json" 2>"$out/down-at-start.err" &
  C=$!
  sleep 20
  hub "$log"
  wait $C && rc=0 || rc=$?
  echo "down-at-start=20 exit=$rc secs=$(( $(date +%s) - s ))" | tee -a "$out/restart.txt"
  kill $H 2>/dev/null || true
  ;;
bytes)
  hub "$out/requests.jsonl"
  cp -R "$repo/crates" "$P/work/crates"
  tools=(--allowedTools "Read" "Grep" "Glob" --output-format json)
  cl 300 -p "Read crates/node/src/claude_api.rs and crates/node/src/containment.rs, then explain in five sentences how a sandboxed session gets its Claude API proxy." "${tools[@]}" >"$out/turn1.json" 2>"$out/turn1.err"
  echo "turn2 $(python3 -c "import time;print(time.time())")" >"$out/marks.txt"
  sid=$(field "$out/turn1.json" session_id)
  cl 300 -p "Find every place that calls Proxy::start and quote each line with its file." --resume "$sid" "${tools[@]}" >"$out/turn2.json" 2>"$out/turn2.err"
  echo "turn3 $(python3 -c "import time;print(time.time())")" >>"$out/marks.txt"
  cl 300 -p "Which tests cover the proxy and the sandbox's token? List their names and files." --resume "$sid" "${tools[@]}" >"$out/turn3.json" 2>"$out/turn3.err"
  ;;
esac
