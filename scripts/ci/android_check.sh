#!/usr/bin/env bash
# android_check.sh: does the Android shell COMPILE for a phone, with the features an APK's libazul is built with?
#
#   scripts/ci/android_check.sh [rust-target]        (default aarch64-linux-android; needs the bindings: remote.sh --codegen)
#
# `cargo check` does not link, but the C of the build scripts (zstd, blake3, ...) is compiled for the target, so the
# NDK's clang is named the way scripts/demo_lib.sh names it. shell2/android is compiled for target_os = "android"
# only, and only with android-activity + ndk + jni + cpurender: scripts/mobile-check-all.sh does not turn those on,
# so a type error in the shell never showed there.
set -euo pipefail
target="${1:-aarch64-linux-android}"
export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
sdkm="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$sdkm" --licenses >/dev/null 2>&1 || true
"$sdkm" --install "ndk;27.0.12077973" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
rustup target add "$target"
ndkbin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
lower="${target//-/_}"
upper="$(echo "$lower" | tr '[:lower:]' '[:upper:]')"
export "CC_${lower}=$ndkbin/${target}24-clang" "CXX_${lower}=$ndkbin/${target}24-clang++" "AR_${lower}=$ndkbin/llvm-ar"
export "CARGO_TARGET_${upper}_LINKER=$ndkbin/${target}24-clang" "CARGO_TARGET_${upper}_AR=$ndkbin/llvm-ar"
FEATURES="std,logging,link-static,a11y,android-activity,ndk,jni,cpurender,map-tiles,pdf"
cargo check -p azul-dll --lib --target "$target" --no-default-features --features "$FEATURES" --message-format short
# the QA APK's lib carries the debug server and the e2e op dispatcher: android_main has blocks behind
# `any(feature = "debug-server", feature = "e2e-scripting")` that the list above does not compile
cargo check -p azul-dll --lib --target "$target" --no-default-features --features "$FEATURES,debug-server,e2e-scripting" --message-format short
echo "android shell: cargo check ok ($target)"
