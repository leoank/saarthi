#!/bin/sh
# Builds saarthi-core for arm64 Android into the app's jniLibs and generates its Kotlin
# bindings into app/build/generated/uniffi. Run from the project root inside
# `nix develop .#android`; Gradle's buildCore task calls it before every build.
set -eu
cargo ndk -t arm64-v8a --platform 26 -o android/app/src/main/jniLibs build -p saarthi-core --release --lib
# The core links libtailscale.so (ADR-0016); the app loads it from beside the core.
install -m 644 "$LIBTAILSCALE_ANDROID_LIB_DIR/libtailscale.so" android/app/src/main/jniLibs/arm64-v8a/
# The terminal renderer (ADR-0054): the JNI shim with libghostty-vt linked in statically,
# and libc++ with it for the library's C++ (Highway); 16 KB pages for Android 15+.
CC="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$(ls "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt" | head -1)/bin/aarch64-linux-android26-clang"
"$CC" -O2 -fPIC -shared -o android/app/src/main/jniLibs/arm64-v8a/libvtjni.so android/vt/vtjni.c \
  -I"$LIBGHOSTTY_VT_ANDROID/include" "$LIBGHOSTTY_VT_ANDROID/lib/libghostty-vt.a" \
  -lc++_static -lc++abi -lm -llog -Wl,-z,max-page-size=16384 -Wl,--gc-sections -s
# The app's monospace face (ADR-0069), from the dev shell's JetBrains Mono.
mkdir -p android/app/src/main/res/font
for w in Regular Medium SemiBold Bold; do
  install -m 644 "$JETBRAINS_MONO/JetBrainsMono-$w.ttf" "android/app/src/main/res/font/jetbrains_mono_$(echo $w | tr A-Z a-z).ttf"
done
cargo run --quiet -p uniffi-bindgen -- generate \
  --library target/aarch64-linux-android/release/libsaarthi_core.so \
  --language kotlin --no-format --out-dir android/app/build/generated/uniffi
