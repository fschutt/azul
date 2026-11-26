#!/usr/bin/env bash
# android_gpu_check.sh: cargo check of azul-dll for the Android targets with the phone feature set, on an Ubuntu runner.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/android_gpu_check.sh' --codegen
#
# `cargo check` does not link, so it needs the Rust standard library of the target and nothing else (no NDK): what it
# shows is that the GPU path's files (shell2/common/egl_gles.rs, gles_gpu.rs, gpu_renderer.rs, shell2/android/*) compile for
# Android, where a normal build on a host never compiles them. The same command string every time, so the cache is reused.
set -uo pipefail

FEATURES="std,logging,link-static,a11y,android-activity,map-tiles,pdf"
rustup target add aarch64-linux-android x86_64-linux-android

overall=0
for target in x86_64-linux-android aarch64-linux-android; do
  echo "=== cargo check -p azul-dll --lib --target $target ==="
  cargo check -p azul-dll --lib --target "$target" --no-default-features --features "$FEATURES"
  status=$?
  echo "android check: $target exit $status"
  if [ "$status" -ne 0 ]; then overall=$status; fi
done
echo "android check: overall exit $overall"
if [ "$overall" -eq 0 ]; then echo "ANDROID CHECK VERDICT: PASS"; else echo "ANDROID CHECK VERDICT: FAIL ($overall)"; fi
# Exit 0 whatever the verdict, so that the runner keeps its build cache (it saves it only for a job that succeeded); the verdict is the line above.
exit 0
