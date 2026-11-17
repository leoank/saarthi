#!/usr/bin/env bash
# RUN-20261117-de5vqw (EXP-20261117-de4f82): /yajna:project spawned headless, as the app
# would spawn it. A scratch lab notebook, a hub and a node on loopback (spawning allowed
# only under the scratch directory), the node's own Claude login, `claude` as the login
# shell finds it (with the yajna plugin). Permission requests are allowed as they come
# (the default mode; under a hub token a spawn runs in auto and asks none) until the
# session is idle, then the conversation is printed. Run from the repo root on a machine
# with Claude Code logged in and the yajna plugin installed; it takes a few minutes and
# spends a little of the account's usage.
set -euo pipefail
R=$(pwd)
D=$R/notes/runs/RUN-20261117-de5vqw
nix develop -c cargo build -q --release --bin saarthi --bin saarthi-hub
W=$(mktemp -d); K=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$W" "$K"' EXIT
(mkdir -p "$W/work/lab" && cd "$W/work/lab" && git init -q && yajna init lab >/dev/null && git add -A && git -c user.email=t@t -c user.name=t commit -qm init)
echo "phone t-phone" > "$W/clients"; echo "laptop t-node" > "$W/nodes"
echo t-node > "$W/node.token"; echo t-phone > "$W/client.token"; chmod 600 "$W"/*.token
"$R/target/release/saarthi-hub" --db "$W/hub.db" --listen 127.0.0.1:7821 \
  --node-tokens "$W/nodes" --client-tokens "$W/clients" > "$W/hub.log" 2>&1 &
sleep 1
"$R/target/release/saarthi" node serve --hub http://127.0.0.1:7821 --token-file "$W/node.token" \
  --name laptop --claude-dir "$HOME/.claude" --hook-addr 127.0.0.1:7822 \
  --wrapper-socket "$K/w.sock" --hook-socket "$K/h.sock" --spawn-root "$W/work" \
  --claude "$(command -v claude)" > "$W/node.log" 2>&1 &
sleep 3
C="$R/target/release/saarthi ctl --hub http://127.0.0.1:7821 --token-file $W/client.token"
N=$($C nodes | awk '{print $1}')
id=$($C spawn --own-login --model haiku "$N" "$W/work/lab" \
  "/yajna:project a small study of how long green tea should steep for the best taste")
echo "session $id"
sleep 20
for _ in $(seq 1 90); do
  if $C pending | grep -q QUESTION; then echo "held a question"; break; fi
  p=$($C pending | grep PERMISSION | head -1 || true)
  [ -n "$p" ] && $C allow "$N" "$(echo "$p" | awk '{print $2}')" > /dev/null
  $C sessions | grep -F "$id" | grep -q SESSION_STATUS_IDLE && { echo "idle"; break; }
  sleep 10
done
python3 "$D/show.py" "$HOME"/.claude/projects/*/"$id".jsonl | tee "$D/conversation.txt"
$C kill "$N" "$id" > /dev/null
