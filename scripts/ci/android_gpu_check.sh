#!/usr/bin/env bash
# android_gpu_check.sh: cargo check of azul-dll for the Android targets with the phone feature set, on an Ubuntu runner.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/android_gpu_check.sh' --codegen
#
# `cargo check` does not link, so it needs the Rust standard library of the target and the C compiler of the NDK for the build
# scripts of the dependencies (cc-rs), nothing more. What it shows is that the GPU path's files (shell2/common/egl_gles.rs,
# gles_gpu.rs, gpu_renderer.rs, shell2/android/*) compile for Android, where a build on a host never compiles them. One line per
# diagnostic (--message-format short), the warnings of the crate (thousands) left out.
set -uo pipefail

FEATURES="std,logging,link-static,a11y,android-activity,map-tiles,pdf"
NDK_VERSION="27.0.12077973"

export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
if [ ! -d "$ANDROID_HOME/ndk/$NDK_VERSION" ]; then
  yes | "$SDKM" --licenses >/dev/null 2>&1 || true
  "$SDKM" --install "ndk;$NDK_VERSION" >/dev/null || echo "android check: the NDK could not be installed"
fi
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$NDK_VERSION"
ndkbin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
rustup target add aarch64-linux-android x86_64-linux-android

overall=0
for target in x86_64-linux-android aarch64-linux-android; do
  # The same variables scripts/demo_lib.sh sets for the real build: the C parts of the dependencies compile with the NDK's clang.
  lower="${target//-/_}"
  export "CC_${lower}=$ndkbin/${target}24-clang" "CXX_${lower}=$ndkbin/${target}24-clang++" "AR_${lower}=$ndkbin/llvm-ar"
  echo "=== cargo check -p azul-dll --lib --target $target ==="
  log="target/android-check-$target.log"
  mkdir -p target
  cargo check -p azul-dll --lib --target "$target" --no-default-features --features "$FEATURES" --message-format short >"$log" 2>&1
  status=$?
  # The errors (and the build scripts' own words), with their places; the warnings are thousands of lines of the crate.
  grep -E "error(\[E[0-9]+\])?:|could not compile|failed to run custom build|Finished" "$log" | head -n 120
  if [ "$status" -ne 0 ]; then echo "--- the end of the log, without warnings:"; grep -v -E "warning" "$log" | tail -n 40; fi
  echo "android check: $target exit $status"
  if [ "$status" -ne 0 ]; then overall=$status; fi
done
echo "android check: overall exit $overall"
if [ "$overall" -eq 0 ]; then echo "ANDROID CHECK VERDICT: PASS"; else echo "ANDROID CHECK VERDICT: FAIL ($overall)"; fi
# Exit 0 whatever the verdict, so that the runner keeps its build cache (it saves it only for a job that succeeded); the verdict is the line above.
exit 0
