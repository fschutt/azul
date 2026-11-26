#!/usr/bin/env bash
# gpu_gate.sh: what the Android GPU path is gated on, in ONE runner job (one cold build of the engine instead of four).
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/gpu_gate.sh' --codegen --artifacts 'target/gles-host/*.png'
#
# 1. the Android targets: cargo check of azul-dll with the phone feature set (scripts/ci/android_gpu_check.sh)
# 2. clippy of azul-core, azul-css and azul-layout with the feature set the tests compile and -D warnings (the CI step; a push that
#    touches them, or the vendored webrender, is gated on it)
# 3. the test crate of azul-dll builds (one line per diagnostic)
# 4. the OpenGL ES host tests (scripts/ci/gles_host.sh: WebRender over Mesa's OpenGL ES 3 against the CPU renderer, the lifecycle
#    executor with real EGL, the pure tests), only when 3 built
#
# Every step prints its own verdict line with the real exit code; the job exits 0 whatever they say, so that the runner keeps its build
# cache (it saves it only for a job that succeeded). The last line is the verdict of all four.
set -uo pipefail

overall=0
summary=""
record() {   # name, exit status
  if [ "$2" -eq 0 ]; then summary="$summary"$'\n'"PASS $1"; else summary="$summary"$'\n'"FAIL($2) $1"; overall=$2; fi
}

echo "=== 1. the Android targets ==="
bash scripts/ci/android_gpu_check.sh
# the script exits 0 whatever it found: its verdict per target is read from the logs it keeps
for target in x86_64-linux-android aarch64-linux-android; do
  log="target/android-check-$target.log"
  if [ -f "$log" ] && grep -q "Finished" "$log" && ! grep -q -E "error(\[E[0-9]+\])?:" "$log"; then
    record "android check $target" 0
  else
    record "android check $target" 1
  fi
done

echo "=== 2. clippy (azul-core, azul-css, azul-layout; the feature set the tests compile) ==="
mkdir -p target
cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets \
  --features azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater \
  -- -D warnings >target/gate-clippy.log 2>&1
status=$?
if [ "$status" -ne 0 ]; then grep -n -E "^(error|warning)" -A12 target/gate-clippy.log | head -n 200; fi
tail -n 5 target/gate-clippy.log
echo "gate: clippy exit $status"
record "clippy" "$status"

echo "=== 3. the test crate of azul-dll builds ==="
cargo test -p azul-dll --lib --no-run --message-format short >target/gate-test-build.log 2>&1
status=$?
grep -E "error(\[E[0-9]+\])?:|could not compile|Finished|Executable" target/gate-test-build.log | head -n 120
if [ "$status" -ne 0 ]; then echo "--- the end of the log, without warnings:"; grep -v -E "warning" target/gate-test-build.log | tail -n 40; fi
echo "gate: test build exit $status"
record "azul-dll test build" "$status"

if [ "$status" -eq 0 ]; then
  echo "=== 4. the OpenGL ES host tests ==="
  mkdir -p target/gles-host
  echo "NOT RUN" >target/gles-host/verdict.txt   # the cache may hold the verdict of an earlier run
  bash scripts/ci/gles_host.sh
  host_status=1
  grep -q "^PASS" target/gles-host/verdict.txt 2>/dev/null && host_status=0
  record "gles host tests (see the SUMMARY above for each test)" "$host_status"
else
  echo "=== 4. the OpenGL ES host tests: not run, the test crate did not build ==="
  record "gles host tests (not run)" 1
fi

echo "=== GPU GATE SUMMARY ==="
echo "$summary"
if [ "$overall" -eq 0 ]; then echo "GPU GATE VERDICT: PASS"; else echo "GPU GATE VERDICT: FAIL ($overall)"; fi
exit 0
