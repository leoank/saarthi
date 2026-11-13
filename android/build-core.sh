#!/bin/sh
# Builds saarthi-core for arm64 Android into the app's jniLibs and generates its Kotlin
# bindings into app/build/generated/uniffi. Run from the project root inside
# `nix develop .#android`; Gradle's buildCore task calls it before every build.
set -eu
cargo ndk -t arm64-v8a --platform 26 -o android/app/src/main/jniLibs build -p saarthi-core --release --lib
# The core links libtailscale.so (ADR-0016); the app loads it from beside the core.
install -m 644 "$LIBTAILSCALE_ANDROID_LIB_DIR/libtailscale.so" android/app/src/main/jniLibs/arm64-v8a/
cargo run --quiet -p uniffi-bindgen -- generate \
  --library target/aarch64-linux-android/release/libsaarthi_core.so \
  --language kotlin --no-format --out-dir android/app/build/generated/uniffi
