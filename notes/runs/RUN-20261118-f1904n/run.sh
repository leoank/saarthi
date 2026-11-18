#!/usr/bin/env bash
# EXP-20261114-raqjf0 steps 3-4: the spike app (spikes/15-libghostty-android) on a device,
# the same loads through libghostty-vt and through Termux's library, one ===RESULT=== line.
# Usage, from the repo root: nix develop .#android -c notes/runs/RUN-20261118-f1904n/run.sh <adb serial>
# It builds the JNI shim and the app, installs the app on that device, runs each load in
# turn and reads the spike's own stats line from logcat. The device's screen must be on.
set -euo pipefail
SERIAL=${1:?adb serial}
A="adb -s $SERIAL"
cd spikes/15-libghostty-android
./build-jni.sh >/dev/null
gradle -q assembleDebug
$A install -r app/build/outputs/apk/debug/app-debug.apk >/dev/null
results=()
for lib in ghostty termux; do
  for load in yes log flood top; do
    $A shell am force-stop me.leoank.vtspike
    $A logcat -c
    $A shell am start -n me.leoank.vtspike/.MainActivity -e lib $lib -e load $load >/dev/null
    # A load that ends is waited for (up to two minutes); top runs for 15 s.
    line=""
    for i in $(seq 1 240); do
      sleep 0.5
      line=$($A logcat -d -s vtspike:I | tail -1 | sed 's/.*vtspike *: //')
      if [ "$load" = top ]; then [ $i -ge 30 ] && break; else case "$line" in *"done in"*) break ;; esac; fi
    done
    sleep 1
    line=$($A logcat -d -s vtspike:I | tail -1 | sed 's/.*vtspike *: //')
    echo "$line"
    results+=("\"$lib $load\": \"$line\"")
  done
done
$A shell am force-stop me.leoank.vtspike
model=$($A shell getprop ro.product.model | tr -d '\r')
printf '===RESULT=== {"device": "%s", %s}\n' "$model" "$(IFS=,; echo "${results[*]}")"
