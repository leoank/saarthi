#!/usr/bin/env bash
# RUN-20261115-218yt4 (EXP-20261114-raqjjy): the bridge chosen after RUN-20261114-rwcse4.
# The relay and `yajna serve` as there, on this machine; the spike app on the rc2s2 emulator
# in mode=bridge: pages go through shouldInterceptRequest, every HTML page gets a script at
# its top that replaces EventSource with one fed over a WebMessagePort, and the app reads
# the event stream itself. The test page's events are timed, and the files viewer, whose
# code is unchanged, is loaded the same way. Run from the repo root on aarch64-darwin.
set -euo pipefail
R=$(pwd)
D=$R/notes/runs/RUN-20261115-218yt4
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
run_mode bridge /__sse.html 8
: > "$D/writes-bridge.txt"
for i in $(seq 1 10); do
  printf 'line %s\n' "$i" >> "$E/nb/data/log.txt"
  python3 -c 'import time; print(int(time.time()*1000))' >> "$D/writes-bridge.txt"
  sleep 1
done
sleep 3
nix develop "$R#android" -c sh -c "$A logcat -d -s SPIKE:I" > "$D/logcat-bridge.txt"
# The files viewer, unchanged, through the bridge, with the log open (it watches open files
# and expanded directories only): does its own EventSource go over the bridge, and does the
# open file follow three appends?
run_mode bridge /files/#data/log.txt 10
for i in 1 2 3; do printf 'appended while open %s\n' "$i" >> "$E/nb/data/log.txt"; sleep 1; done
sleep 3
nix develop "$R#android" -c sh -c "$A exec-out screencap -p" > "$D/files-bridge.png"
nix develop "$R#android" -c sh -c "$A logcat -d -s SPIKE:I" > "$D/logcat-files-bridge.txt"
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
