#!/usr/bin/env bash
# EXP-20261114-raqjf0, step 1: build libghostty-vt (Ghostty v1.4.0, Zig 0.17.1) for the
# host and for aarch64-linux-android (API 26, NDK from nixpkgs' androidenv), run a smoke
# test on the host and link the same test for Android with the NDK's clang (not run: no
# device in this step). Run from the repo root; the flake and smoke.c are in
# spikes/15-libghostty-android. Prints one ===RESULT=== line.
set -euo pipefail
cd spikes/15-libghostty-android
out=${OUT:-$(mktemp -d)}
host=$(nix build .#host --no-link --print-out-paths)
droid=$(nix build .#android-arm64 --no-link --print-out-paths)
ndk=$(nix eval --raw --impure --expr '
  let f = builtins.getFlake (toString ./.);
      pkgs = import f.inputs.nixpkgs { system = builtins.currentSystem;
        config = { android_sdk.accept_license = true; allowUnfree = true; }; };
      a = pkgs.androidenv.composeAndroidPackages { includeNDK = true; includeEmulator = false; includeSystemImages = false; };
  in "${a.ndk-bundle}/libexec/android-sdk/ndk/${a.ndk-bundle.version}"')
tb=$(echo "$ndk"/toolchains/llvm/prebuilt/*/bin)
nix build --no-link "$(nix eval --raw --impure --expr '
  let f = builtins.getFlake (toString ./.);
      pkgs = import f.inputs.nixpkgs { system = builtins.currentSystem;
        config = { android_sdk.accept_license = true; allowUnfree = true; }; };
  in (pkgs.androidenv.composeAndroidPackages { includeNDK = true; includeEmulator = false; includeSystemImages = false; }).ndk-bundle.drvPath')^out"

cc -I"$host/include" smoke.c "$host/lib/libghostty-vt.a" -o "$out/smoke-host"
"$out/smoke-host" > "$out/smoke-host.txt" 2>/dev/null
host_ok=$(grep -q '"screen_matches": true' "$out/smoke-host.txt" && echo true || echo false)

"$tb/aarch64-linux-android26-clang" -I"$droid/include" smoke.c "$droid/lib/libghostty-vt.a" -o "$out/smoke-android"
"$tb/llvm-strip" -o "$out/libghostty-vt.stripped.so" "$droid/lib/libghostty-vt.so"
size() { stat -L -f %z "$1" 2>/dev/null || stat -L -c %s "$1"; }
exports=$("$tb/llvm-nm" -D --defined-only "$droid/lib/libghostty-vt.so" | grep -c ' T ghostty_')
align=$("$tb/llvm-readelf" -lW "$droid/lib/libghostty-vt.so" | awk '/LOAD/{print $NF; exit}')
needed=$("$tb/llvm-readelf" -d "$droid/lib/libghostty-vt.so" | sed -n 's/.*Shared library: \[\(.*\)\]/\1/p' | paste -sd, -)
soname=$("$tb/llvm-readelf" -d "$droid/lib/libghostty-vt.so" | sed -n 's/.*Library soname: \[\(.*\)\]/\1/p')
echo "===RESULT=== {\"android_builds\": true, \"host_smoke\": $host_ok, \"android_links\": true, \"so_bytes\": $(size "$droid/lib/libghostty-vt.so"), \"so_stripped_bytes\": $(size "$out/libghostty-vt.stripped.so"), \"a_bytes\": $(size "$droid/lib/libghostty-vt.a"), \"ghostty_exports\": $exports, \"load_align\": \"$align\", \"needed\": \"$needed\", \"soname\": \"$soname\"}"
