#!/usr/bin/env bash
# RUN-20261126-hsee10: two `saarthi node serve` daemons on one machine, against a hub of
# their own on 127.0.0.1:7811. Run from the repo root in the dev shell:
#   nix develop -c spikes/17-two-nodes/run-2.sh
#
# A (H3): both nodes with the default paths, in a HOME and XDG_RUNTIME_DIR of their own so
#   the machine's real daemon is not touched; only the hook port is given.
# B (H1, H2): both nodes on the user's real Claude dir (the case of an app beside the
#   owner's daemon), every path given apart; a session spawned on one, seen by the other?
#
# Prints counts only: the owner's own sessions are not named. The spawned session runs
# `claude` under the machine's own login with Haiku, one short prompt.
set -uo pipefail
repo=$(pwd)
out=$repo/notes/runs/RUN-20261126-hsee10
mkdir -p "$out"
cargo build -q --bin saarthi --bin saarthi-hub || exit 1
bin=$repo/target/debug
W=$(mktemp -d /tmp/s2n.XXXX)      # short: a socket path has at most 104 bytes on macOS
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null; done; wait 2>/dev/null; [ -n "${KEEP:-}" ] && echo "kept $W" || rm -rf "$W"; }
trap cleanup EXIT

tok() { head -c16 /dev/urandom | od -An -tx1 | tr -d ' \n'; }
# x and y for part A, a and b for part B: a node's token is its identity on the hub, and
# one restarted at once under the same token finds the hub still holding its old stream.
for n in x y a b; do t=$(tok); echo "$n $t" >> "$W/nodes"; echo "$t" > "$W/$n.token"; done
tc=$(tok); printf 'ctl %s\n' "$tc" > "$W/clients"; echo "$tc" > "$W/c.token"
: > "$W/claude-tokens"
hub=http://127.0.0.1:7811
"$bin/saarthi-hub" --listen 127.0.0.1:7811 --db "$W/hub.db" --node-tokens "$W/nodes" \
  --client-tokens "$W/clients" --claude-tokens "$W/claude-tokens" --no-notify 1 2> "$W/hub.log" &
pids+=($!)
sleep 2
ctl() { "$bin/saarthi" ctl --hub $hub --token-file "$W/c.token" "$@"; }
alive() { kill -0 "$1" 2>/dev/null && echo alive || echo exited; }
ino() { [ -e "$1" ] && ls -di "$1" | awk '{print $1}' || echo none; }
# curl exits 7 when nothing accepts the connection; anything else means a listener took it.
answers() { curl -s -m 2 --unix-socket "$1" http://x/ -o /dev/null; [ $? -ne 7 ] && echo answers || echo dead; }

echo "=== A: default paths, one HOME, one runtime dir ==="
mkdir -p "$W/home" "$W/run"
envA=(env HOME="$W/home" XDG_RUNTIME_DIR="$W/run")
nodeA() { "${envA[@]}" "$bin/saarthi" node serve --hub $hub --token-file "$W/$1.token" --name "$1" --hook-addr "$2" 2> "$W/$3.log" & pids+=($!); }
nodeA x 127.0.0.1:7791 x1; pa=$!; sleep 3
wrap=$(find "$W/run" -name node.sock); changes="$W/home/.local/state/saarthi/changes.sock"
echo "x up: $(alive $pa); wrapper socket inode $(ino "$wrap") $(answers "$wrap"); changes $(answers "$changes")"
nodeA y 127.0.0.1:7791 y1; pb=$!; sleep 3
echo "y on x's hook port: $(alive $pb)"
grep -hoE 'binding the hook endpoint[^"]*|Address already in use' "$W/y1.log" | sort -u | sed 's/^/  y: /'
echo "after y: wrapper socket inode $(ino "$wrap") $(answers "$wrap") (x still $(alive $pa))"
nodeA y 127.0.0.1:7792 y2; pb=$!; sleep 3
echo "y on its own hook port: $(alive $pb); wrapper socket inode $(ino "$wrap") $(answers "$wrap")"
grep -hoE 'another server answers there[^"]*' "$W/y2.log" | sort -u | sed "s|$W|\$W|g; s/^/  y: /"
ctl nodes | awk '{print "  hub: node", $2, $4}'
kill $pb; sleep 1
echo "y stopped, x $(alive $pa): wrapper socket $(ino "$wrap") $(answers "$wrap"), changes $(answers "$changes")"
kill $pa; sleep 1

echo "=== B: the user's Claude dir, every path apart ==="
mkdir -p "$W/work" "$W/a" "$W/b"
nodeB() {
  "$bin/saarthi" node serve --hub $hub --token-file "$W/$1.token" --name "$1" --hook-addr "$2" \
    --wrapper-socket "$W/$1/wrap.sock" --changes-socket "$W/$1/changes.sock" \
    --shells-socket "$W/$1/shells.sock" --hook-socket "$W/$1/hook.sock" \
    --spawn-root "$W/work" 2> "$W/$1.log" & pids+=($!)
}
nodeB a 127.0.0.1:7791; pa=$!; nodeB b 127.0.0.1:7792; pb=$!; sleep 5
echo "a $(alive $pa), b $(alive $pb)"
ctl nodes | awk '{print "  hub: node", $2, $4}'
ida=$(ctl nodes | awk '$2=="a"{print $1}'); idb=$(ctl nodes | awk '$2=="b"{print $1}')
count() { ctl sessions | awk -v n="$1" '$1==n' | wc -l | tr -d ' '; }
both() { comm -12 <(ctl sessions | awk -v n="$ida" '$1==n{print $2}' | sort) <(ctl sessions | awk -v n="$idb" '$1==n{print $2}' | sort) | wc -l | tr -d ' '; }
echo "live sessions: a lists $(count $ida), b lists $(count $idb), listed by both $(both)"
# Each node uploads every live session's transcript from its start; wait for the hub to
# take both copies in before spawning (a spawn's reply waits 15 s at most).
evts() { sqlite3 "$W/hub.db" "select count(*) from events where node_id='$1'"; }
prev=-1; for i in $(seq 1 90); do n=$(sqlite3 "$W/hub.db" "select count(*) from events"); [ "$n" = "$prev" ] && break; prev=$n; sleep 2; done
echo "hub events settled after about $((i*2)) s: from a $(evts $ida), from b $(evts $idb); 'database is locked' $(grep -c 'database is locked' "$W/hub.log") times"
echo "user settings name hook urls: $(grep -oE 'http://127\.0\.0\.1:[0-9]+/hook' ~/.claude/settings.json 2>/dev/null | sort -u | tr '\n' ' ')"

sid=$(ctl spawn a "$W/work/s" --create --model haiku "Reply with the single word ok." 2>&1 | tail -1)
echo "spawned on a: $sid"
for i in $(seq 1 60); do
  ona=$(ctl sessions | awk -v n="$ida" -v s="$sid" '$1==n && $2==s' | wc -l | tr -d ' ')
  onb=$(ctl sessions | awk -v n="$idb" -v s="$sid" '$1==n && $2==s' | wc -l | tr -d ' ')
  [ "$onb" != 0 ] && break; sleep 1
done
echo "after ${i}s: the spawned session listed under a: $ona, under b: $onb"
ctl sessions | awk -v s="$sid" '$2==s {$1=($1=="'"$ida"'"?"a":"b"); print "  hub:", $0}' | sed "s|$W|\$W|g"
ctl history 2>/dev/null | grep "$sid" | sed "s|$W|\$W|g; s/^/  history: /" | head -4
echo "kill on a: $(ctl kill a "$sid" 2>&1 | tail -1)"
for i in $(seq 1 20); do ctl sessions | grep -q "$sid" || break; sleep 1; done
echo "after ${i}s: listed under a $(ctl sessions | awk -v n="$ida" -v s="$sid" '$1==n && $2==s' | wc -l | tr -d ' '), under b $(ctl sessions | awk -v n="$idb" -v s="$sid" '$1==n && $2==s' | wc -l | tr -d ' ')"
# The messages only, without their fields: those may name the owner's directories.
for n in a b; do grep -hE ' (ERROR|WARN) ' "$W/$n.log" | sed -E 's/\x1b\[[0-9;]*m//g; s/^.* (ERROR|WARN) [^ ]+: /\1 /; s/ [a-z_.]+=.*//' | sort | uniq -c | sed "s/^/  $n log:/"; done
