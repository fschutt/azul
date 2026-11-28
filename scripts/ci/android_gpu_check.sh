#!/usr/bin/env bash
# android_gpu_check.sh: does azul-dll compile for BOTH Android targets with the feature sets of a phone's libazul, on an Ubuntu runner?
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/android_gpu_check.sh' --codegen
#
# scripts/ci/android_check.sh does the work for one target (the shell's feature set, then the QA variant's with the debug server and the
# e2e dispatcher); this runs it for x86_64 (the emulator) and aarch64 (a phone), keeps each one's log in target/android-check-<target>.log
# and prints one verdict line with the real exit codes. What it shows is that the GPU path's files (shell2/common/egl_gles.rs, gles_gpu.rs,
# gpu_renderer.rs, shell2/android/gpu.rs) compile for Android, where a build on a host never compiles them.
set -uo pipefail

mkdir -p target
overall=0
for target in x86_64-linux-android aarch64-linux-android; do
  echo "=== scripts/ci/android_check.sh $target ==="
  log="target/android-check-$target.log"
  bash scripts/ci/android_check.sh "$target" >"$log" 2>&1
  status=$?
  # the errors (and the build scripts' own words), with their places; the warnings are thousands of lines of the crate
  grep -E "error(\[E[0-9]+\])?:|could not compile|failed to run custom build|android shell: cargo check ok" "$log" | head -n 120
  if [ "$status" -ne 0 ]; then echo "--- the end of the log, without warnings:"; grep -v -E "warning" "$log" | tail -n 40; fi
  echo "android check: $target exit $status"
  if [ "$status" -ne 0 ]; then overall=$status; fi
done
echo "android check: overall exit $overall"
if [ "$overall" -eq 0 ]; then echo "ANDROID CHECK VERDICT: PASS"; else echo "ANDROID CHECK VERDICT: FAIL ($overall)"; fi
# Exit 0 whatever the verdict, so that the runner keeps its build cache (it saves it only for a job that succeeded); the verdict is the line above.
exit 0
