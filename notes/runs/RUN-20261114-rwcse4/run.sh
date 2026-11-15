#!/usr/bin/env bash
# RUN-20261114-rwcse4 (EXP-20261114-raqjjy): can a WebView be fed from the relay with
# server-sent events kept live? The relay and `yajna serve` as in RUN-20261114-rn027z, on
# this machine; the spike app (spikes/14-service-relay/webview) on the rc2s2 emulator loads
# a page whose EventSource watches a file appended to once a second, two ways:
#   loopback   the WebView loads the relay's proxy port (as a port the core would serve)
#   intercept  shouldInterceptRequest answers from a stream (an HTTP connection to the same
#              proxy, standing in for the core's ServiceRequest stream)
# and the files viewer each way, captured. Run from the repo root on aarch64-darwin.
set -euo pipefail
R=$(pwd)
D=$R/notes/runs/RUN-20261114-rwcse4
(cd spikes/14-service-relay && nix develop "$R" -c cargo build --release -q)
BIN=$R/spikes/14-service-relay/target/release/service-relay
E=$(mktemp -d "${TMPDIR:-/tmp}/relayweb.XXXX")
# yajna keeps its registry and caches in the scratch directory; the Android tools keep theirs.
Y="env XDG_CONFIG_HOME=$E/config XDG_STATE_HOME=$E/state XDG_CACHE_HOME=$E/cache yajna"
mkdir -p "$E/nb/data"
(cd "$E/nb" && git init -q && $Y init project relaytest >/dev/null)
printf '\n[files]\n[[files.roots]]\nname = "data"\npath = "data"\n' >> "$E/nb/yajna.toml"
printf 'a log the run appends to\n' > "$E/nb/data/log.txt"
SP=18765 HP=18766 PP=18767
(cd "$E/nb" && exec $Y serve --port $SP > "$E/serve.log" 2>&1) &
"$BIN" hub 127.0.0.1:$HP > "$E/hub.log" 2>&1 &
trap 'kill $(jobs -p) 2>/dev/null; adb -s emulator-5586 emu kill >/dev/null 2>&1 || true; rm -rf "$E"' EXIT
sleep 1
"$BIN" node http://127.0.0.1:$HP laptop yajna=tcp:127.0.0.1:$SP > "$E/node.log" 2>&1 &
sleep 1
"$BIN" proxy http://127.0.0.1:$HP $PP laptop yajna > "$E/proxy.log" 2>&1 &
for i in $(seq 1 30); do curl -sf -o /dev/null "http://127.0.0.1:$SP/api/info" && break; sleep 1; done

A="adb -s emulator-5586"
nix develop "$R#android" -c sh -c "
  export ANDROID_AVD_HOME=\$HOME/.local/state/rc2s2/android/avd
  emulator -avd rc2s2 -port 5586 -no-snapshot -no-boot-anim -no-window -no-audio >/dev/null 2>&1 &
  (cd spikes/14-service-relay/webview && gradle -q --offline assembleDebug)
  $A wait-for-device
  until [ \"\$($A shell getprop sys.boot_completed | tr -d '\r')\" = 1 ]; do sleep 2; done
  $A install -r spikes/14-service-relay/webview/app/build/outputs/apk/debug/app-debug.apk >/dev/null
  $A shell dumpsys package com.google.android.webview | grep -m1 versionName
" | tee "$D/webview-version.txt"

run_mode() { # mode path seconds-to-load
  nix develop "$R#android" -c sh -c "$A logcat -c; $A shell am start -S -n me.leoank.relayspike/.MainActivity --es mode $1 --es path $2 >/dev/null"
  sleep "$3"
}
for mode in loopback intercept; do
  run_mode $mode /__sse.html 8
  : > "$D/writes-$mode.txt"
  for i in $(seq 1 10); do
    printf 'line %s\n' "$i" >> "$E/nb/data/log.txt"
    python3 -c 'import time; print(int(time.time()*1000))' >> "$D/writes-$mode.txt"
    sleep 1
  done
  sleep 3
  nix develop "$R#android" -c sh -c "$A logcat -d -s SPIKE:I" > "$D/logcat-$mode.txt"
  run_mode $mode /files/ 10
  nix develop "$R#android" -c sh -c "$A exec-out screencap -p" > "$D/files-$mode.png"
  nix develop "$R#android" -c sh -c "$A logcat -d -s SPIKE:I" > "$D/logcat-files-$mode.txt"
done
# Does intercept buffer (events late, in batches) or never deliver? 200 writes 0.1 s apart.
run_mode intercept /__sse.html 8
: > "$D/writes-burst.txt"
for i in $(seq 1 200); do
  printf 'burst %s\n' "$i" >> "$E/nb/data/log.txt"
  python3 -c 'import time; print(int(time.time()*1000))' >> "$D/writes-burst.txt"
  sleep 0.1
done
sleep 5
nix develop "$R#android" -c sh -c "$A logcat -d -s SPIKE:I" > "$D/logcat-burst.txt"
# The emulator's clock against this machine's: device ms minus the host's midpoint, 5 times.
for i in 1 2 3 4 5; do
  nix develop "$R#android" -c python3 - "$A" <<'PY' >> "$D/skew.txt"
import subprocess, sys, time
a = time.time() * 1000
dev = int(subprocess.run(sys.argv[1].split() + ["shell", "date +%s%3N"], capture_output=True, text=True).stdout.strip())
b = time.time() * 1000
print(round(dev - (a + b) / 2))
PY
done
python3 -I "$D/analyze.py" "$D" | tee "$D/result.txt"
