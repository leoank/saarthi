#!/usr/bin/env bash
# Re-creates RUN-20261113-1v2rh6 of EXP-20261113-1h4ztf: libtailscale cross-built for arm64
# Android, linked into the client core, and the app reaching a hub through its own tailnet
# identity on an emulator with no Tailscale app. Run from the repo root on aarch64-darwin:
#   TSNET_AUTHKEY_FILE=<mode-600 file with a reusable, pre-approved, non-ephemeral key> \
#     notes/runs/RUN-20261113-1v2rh6/run.sh
# Two devices join the key's tailnet, saarthi-run-hub and saarthi-<emulator model>; remove
# them after. The key is only ever read from its file and typed into the emulator.
#
# Adding the hub in the app is done by hand: the script starts everything, then prints the
# steps and waits. The checks after it read the boot log from the screen captures and the
# tsnet log from the app's files.
set -euo pipefail

key="${TSNET_AUTHKEY_FILE:?set TSNET_AUTHKEY_FILE}"
out="${YAJNA_RUN_DIR:-$(dirname "$0")}"
work="$(mktemp -d)"
trap 'kill $(jobs -p) 2>/dev/null || true; rm -rf "$work"' EXIT

# 1. The Android library, and what it needs at load.
lib="$(nix build .#libtailscale-android --no-link --print-out-paths)/lib/libtailscale.so"
file -L "$lib" | tee "$out/libtailscale.txt"

# 2. A hub serving on the tailnet only by its MagicDNS name (and on localhost for nothing).
nix develop -c cargo build -q --release --bin saarthi-hub
token="run$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
echo "phone $token" > "$work/clients"
echo "node n$token" > "$work/nodes"
./target/release/saarthi-hub --db "$work/hub.db" --listen 127.0.0.1:7799 \
  --node-tokens "$work/nodes" --client-tokens "$work/clients" \
  --tailnet "run=$key" --tailnet-state "$work/tailnet" --tailnet-hostname saarthi-run-hub \
  > "$work/hub.log" 2>&1 &
until grep -q "hub serving on the tailnet" "$work/hub.log"; do sleep 1; done

# 3. The app, built and installed on a fresh emulator (no Tailscale app on it).
nix develop .#android -c sh -c '
  export ANDROID_AVD_HOME="$HOME/.local/state/rc2s2/android/avd"
  emulator -avd rc2s2 -port 5584 -no-snapshot -no-boot-anim -no-window >/dev/null 2>&1 &
  cd android && gradle -q assembleDebug && cd ..
  adb -s emulator-5584 wait-for-device
  until [ "$(adb -s emulator-5584 shell getprop sys.boot_completed | tr -d "\r")" = 1 ]; do sleep 2; done
  adb -s emulator-5584 install -r android/app/build/outputs/apk/debug/app-debug.apk
  adb -s emulator-5584 shell pm list packages | grep -c tailscale || true
  adb -s emulator-5584 shell am start -n me.leoank.saarthi/.MainActivity
' | tee "$out/install.txt"

cat <<STEPS
In the emulator's Add a hub:
  Address  http://saarthi-run-hub:7700
  Token    $token
  Auth key (typed for you on Enter: adb shell input text from $key)
then Save and connect. Press Enter when the editor is open with the key field focused.
STEPS
read -r
nix develop .#android -c sh -c "adb -s emulator-5584 shell input text \"\$(tr -d '\n' < $key)\""
echo "Press Enter once the sessions screen shows."
read -r

# 4. A cold start from the saved state, captured as it boots.
nix develop .#android -c sh -c "
  adb -s emulator-5584 shell am force-stop me.leoank.saarthi
  adb -s emulator-5584 shell am start -n me.leoank.saarthi/.MainActivity >/dev/null
  sleep 3; adb -s emulator-5584 exec-out screencap -p > $work/cold-start.png  # shows tailnet addresses: not kept in the public repo
  adb -s emulator-5584 shell run-as me.leoank.saarthi sh -c 'cat files/tailnet/*/tsnet.log' > $work/tsnet.log
  adb -s emulator-5584 logcat -d | grep -E 'avc.*netlink|E Go ' > $work/denials.txt || true
"
grep -c "netlink_route_socket" "$work/denials.txt" | sed 's/^/netlink denials: /' | tee "$out/checks.txt"
grep -cE "panic|netlinkrib" "$work/tsnet.log" | sed 's/^/tsnet errors: /' | tee -a "$out/checks.txt"
