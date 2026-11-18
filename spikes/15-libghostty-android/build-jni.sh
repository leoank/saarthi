#!/usr/bin/env bash
# EXP-20261114-raqjf0 step 2: the JNI shim, linked with libghostty-vt.a for arm64 Android.
# Run in the project's android shell (it sets ANDROID_NDK_HOME), from this directory:
#   nix develop ../..#android -c ./build-jni.sh
set -euo pipefail
VT=$(nix build --no-link --print-out-paths .#android-arm64)
CC="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$(ls "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt" | head -1)/bin/aarch64-linux-android26-clang"
OUT=app/src/main/jniLibs/arm64-v8a
mkdir -p "$OUT"
# libghostty-vt.a carries Highway's C++ (hwy::), so the C++ runtime goes in statically too;
# 16 KB pages for Android 15+, as the library itself is.
"$CC" -O2 -fPIC -shared -o "$OUT/libvtjni.so" jni/vtjni.c \
  -I"$VT/include" "$VT/lib/libghostty-vt.a" \
  -static-libstdc++ -lc++_static -lc++abi -lm -llog \
  -Wl,-z,max-page-size=16384 -Wl,--gc-sections -s
ls -la "$OUT/libvtjni.so"
