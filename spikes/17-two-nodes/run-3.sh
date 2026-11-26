#!/usr/bin/env bash
# RUN-20261126-hseepp: two nodes in one process through the library (src/main.rs), against a
# hub of their own on 127.0.0.1:7813, with a stand-in for claude that logs its input and
# answers each prompt with a result. Run from the repo root:
#   nix develop -c spikes/17-two-nodes/run-3.sh
#
# 1. Both register, each spawns a session and is prompted again; is each session listed
#    under its own node only, and does each prompt reach its own process?
# 2. The hub restarts: do both reconnect?
# 3. The same, with node q set to dial through a tailnet (`--tailnet q`): does p still
#    reconnect over plain TCP?
set -uo pipefail
repo=$(pwd)
out=$repo/notes/runs/RUN-20261126-hseepp
mkdir -p "$out"
cargo build -q --bin saarthi --bin saarthi-hub || exit 1
(cd spikes/17-two-nodes && cargo build -q --locked) || exit 1
bin=$repo/target/debug
spike=$repo/spikes/17-two-nodes/target/debug/two-nodes
W=$(mktemp -d /tmp/s2p.XXXX)
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null; done; wait 2>/dev/null; [ -n "${KEEP:-}" ] && echo "kept $W" || rm -rf "$W"; }
trap cleanup EXIT

tok() { head -c16 /dev/urandom | od -An -tx1 | tr -d ' \n'; }
for n in p q; do t=$(tok); echo "$n $t" >> "$W/nodes"; echo "$t" > "$W/$n.token"; done
tc=$(tok); echo "ctl $tc" > "$W/clients"; echo "$tc" > "$W/c.token"; : > "$W/claude-tokens"
cat > "$W/claude" <<'EOF'
#!/bin/sh
while IFS= read -r line; do
  echo "$line" >> "$PWD/stdin.log"
  case "$line" in *'"type":"user"'*) echo '{"type":"result","subtype":"success"}' ;; esac
done
EOF
chmod +x "$W/claude"
mkdir -p "$W/work"
hub=http://127.0.0.1:7813
hubpid=
start_hub() {
  "$bin/saarthi-hub" --listen 127.0.0.1:7813 --db "$W/hub.db" --node-tokens "$W/nodes" \
    --client-tokens "$W/clients" --claude-tokens "$W/claude-tokens" --no-notify 1 2>> "$W/hub.log" &
  hubpid=$!; pids+=($hubpid); sleep 2
}
ctl() { "$bin/saarthi" ctl --hub $hub --token-file "$W/c.token" "$@"; }
state() { ctl nodes | awk '{printf "%s %s  ", $2, $4}'; echo; }
waitboth() { for i in $(seq 1 ${1:-30}); do [ "$(ctl nodes 2>/dev/null | grep -c ' connected')" = 2 ] && break; sleep 1; done; echo "after ${i}s: $(state)"; }
restart_hub() { kill "$hubpid"; wait "$hubpid" 2>/dev/null; start_hub; }

start_hub
echo "=== 1. two nodes in one process ==="
"$spike" $hub "$W" "$W/claude" p="$W/p.token" q="$W/q.token" 2> "$W/spike1.log" & sp=$!; pids+=($sp)
waitboth
idp=$(ctl nodes | awk '$2=="p"{print $1}'); idq=$(ctl nodes | awk '$2=="q"{print $1}')
sp_=$(ctl spawn p "$W/work/p" --create "hello from p" | tail -1)
sq_=$(ctl spawn q "$W/work/q" --create "hello from q" | tail -1)
sleep 2
for pair in "p $idp $sp_" "q $idq $sq_"; do
  set -- $pair
  echo "session spawned on $1: listed under p $(ctl sessions | awk -v n=$idp -v s=$3 '$1==n&&$2==s' | wc -l | tr -d ' '), under q $(ctl sessions | awk -v n=$idq -v s=$3 '$1==n&&$2==s' | wc -l | tr -d ' ')"
done
ctl prompt p "$sp_" "again to p" >/dev/null; ctl prompt q "$sq_" "again to q" >/dev/null; sleep 2
for n in p q; do echo "prompts $n's process read: $(grep -o 'hello from [pq]\|again to [pq]' "$W/work/$n/stdin.log" | tr '\n' ',')"; done
grep -h 'hooks on' "$W/spike1.log" | sed "s|$W|\$W|g; s/^.*node/  node/"
echo "--- the hub restarts"
restart_hub; waitboth 40
ctl kill p "$sp_" >/dev/null; ctl kill q "$sq_" >/dev/null
kill $sp; wait $sp 2>/dev/null

echo "=== 2. the same, q set to dial through its tailnet ==="
"$spike" $hub "$W" "$W/claude" p="$W/p.token" q="$W/q.token" --tailnet q 2> "$W/spike2.log" & sp=$!; pids+=($sp)
sleep 1; waitboth 20
echo "--- the hub restarts"
mark=$(wc -l < "$W/spike2.log")
restart_hub; waitboth 40
echo "each node's link since the restart:"
for n in p q; do echo "  before: $n $(head -n "$mark" "$W/spike2.log" | grep "node{name=\"$n\"}" | grep -oE 'not on the tailnet yet|registered with the hub' | sort | uniq -c | tr -s ' ' | tr '\n' ';')"; done
for n in p q; do tail -n +"$((mark+1))" "$W/spike2.log" | grep "node{name=\"$n\"}" | grep -oE 'not on the tailnet yet|registered with the hub' | sort | uniq -c | tr -s ' ' | tr '\n' ';' | sed "s/^/  after: $n/"; echo; done
