#!/usr/bin/env bash
# gpu_gate.sh: what a push of the Android GPU path is gated on, in ONE runner job (one cold build of the engine instead of five).
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/gpu_gate.sh' --codegen --artifacts 'target/gles-host/*.png'
#
# 1. the Android targets: cargo check of azul-dll for aarch64 and x86_64 with the shell's feature set and the QA variant's
#    (scripts/ci/android_gpu_check.sh, which runs scripts/ci/android_check.sh per target)
# 2. clippy of azul-core, azul-css and azul-layout with the feature set the tests compile and -D warnings (the CI step)
# 3. the WHOLE `cargo test -p azul-dll --lib` (a subset hides what a new struct or a renamed function breaks elsewhere)
# 4. `cargo test -p azul-srcgate --no-fail-fast` (the source gates of the tree)
# 5. the OpenGL ES host tests (scripts/ci/gles_host.sh: WebRender over Mesa's OpenGL ES 3 against the CPU renderer, the lifecycle
#    executor with real EGL, the pure tests), one process per GL test; they are off in step 3 (AZ_GLES_HOST_TESTS)
# 6. the guide: `azul-doc check` and `azul-doc check layout`
# 7. autofix (`azul-doc autofix`, patch, normalize, no change left): the API drift and the FFI safety of every type, a type of the
#    engine that has the name of an API type fails it
# AZ_GATE_SKIP="clippy srcgate doc" leaves out the steps named (a change that touches the dll alone needs none of them); the names are
# android, clippy, srcgate, doc, autofix (a run for one step skips the others).
#
# Every step prints its own verdict line with the real exit code; the job exits 0 whatever they say, so that the runner keeps its build
# cache (it saves it only for a job that succeeded). The last line is the verdict of all of them.
set -uo pipefail

# AZ_GATE_SKIP="clippy srcgate doc": steps a change does not touch (a change to the dll alone needs none of them)
skip() { case " ${AZ_GATE_SKIP:-} " in *" $1 "*) echo "(skipped: $1)"; record "$1 (skipped)" 0; return 0;; esac; return 1; }

overall=0
summary=""
record() {   # name, exit status
  if [ "$2" -eq 0 ]; then summary="$summary"$'\n'"PASS $1"; else summary="$summary"$'\n'"FAIL($2) $1"; overall=$2; fi
}
mkdir -p target

if ! skip android; then
  echo "=== 1. the Android targets ==="
  bash scripts/ci/android_gpu_check.sh
  # the script exits 0 whatever it found: its verdict per target is read from the logs it keeps
  for target in x86_64-linux-android aarch64-linux-android; do
    log="target/android-check-$target.log"
    if [ -f "$log" ] && grep -q "android shell: cargo check ok" "$log" && ! grep -q -E "error(\[E[0-9]+\])?:" "$log"; then
      record "android check $target" 0
    else
      record "android check $target" 1
    fi
  done
fi

if ! skip clippy; then
  echo "=== 2. clippy (azul-core, azul-css, azul-layout; the feature set the tests compile) ==="
  cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets \
    --features azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater \
    -- -D warnings >target/gate-clippy.log 2>&1
  status=$?
  if [ "$status" -ne 0 ]; then grep -n -E "^(error|warning)" -A12 target/gate-clippy.log | head -n 200; fi
  tail -n 5 target/gate-clippy.log
  echo "gate: clippy exit $status"
  record "clippy" "$status"
fi

echo "=== 3. the whole cargo test -p azul-dll --lib ==="
cargo test -p azul-dll --lib >target/gate-dll-lib.log 2>&1
status=$?
grep -E "^test result|^error(\[E[0-9]+\])?:|could not compile|^failures:|^    [a-z_:0-9]+$" target/gate-dll-lib.log | tail -n 80
grep -E "^test .* FAILED" target/gate-dll-lib.log | head -n 60
echo "gate: azul-dll lib tests exit $status"
record "azul-dll lib tests (the whole)" "$status"

if ! skip srcgate; then
  echo "=== 4. cargo test -p azul-srcgate --no-fail-fast ==="
  cargo test -p azul-srcgate --no-fail-fast >target/gate-srcgate.log 2>&1
  status=$?
  grep -E "^test result|^error(\[E[0-9]+\])?:|could not compile|^test .* FAILED" target/gate-srcgate.log | tail -n 40
  echo "gate: srcgate exit $status"
  record "srcgate" "$status"
fi

if grep -q "^test result: " target/gate-dll-lib.log; then
  echo "=== 5. the OpenGL ES host tests ==="
  mkdir -p target/gles-host
  echo "NOT RUN" >target/gles-host/verdict.txt   # the cache may hold the verdict of an earlier run
  bash scripts/ci/gles_host.sh
  host_status=1
  grep -q "^PASS" target/gles-host/verdict.txt 2>/dev/null && host_status=0
  record "gles host tests (see the SUMMARY above for each test)" "$host_status"
else
  echo "=== 5. the OpenGL ES host tests: not run, the test crate did not build ==="
  record "gles host tests (not run)" 1
fi

if ! skip doc; then
  echo "=== 6. the guide: azul-doc check (double-drop invariant, guide links) and azul-doc check layout ==="
  cargo run -r -p azul-doc -- check >target/gate-doc-check.log 2>&1
  status=$?
  tail -n 12 target/gate-doc-check.log
  echo "gate: azul-doc check exit $status"
  record "azul-doc check" "$status"
  cargo run -r -p azul-doc -- check layout >target/gate-doc-layout.log 2>&1
  status=$?
  tail -n 12 target/gate-doc-layout.log
  echo "gate: azul-doc check layout exit $status"
  record "azul-doc check layout" "$status"
fi

if ! skip autofix; then
  echo "=== 7. autofix: api.json drift and the FFI safety of every type the engine defines (a name that is also an API type fails here) ==="
  (
    set -e
    cargo run -r -p azul-doc -- autofix
    cargo run -r -p azul-doc -- autofix explain
    cargo run -r -p azul-doc -- patch safe target/autofix/patches
    cargo run -r -p azul-doc -- patch target/autofix/patches
    cargo run -r -p azul-doc -- normalize
  ) >target/gate-autofix.log 2>&1
  status=$?
  grep -E "CRITICAL|critical|error|Error|FFI safety|defined twice|duplicate" target/gate-autofix.log | head -n 20
  tail -n 8 target/gate-autofix.log
  if [ "$status" -eq 0 ] && ! git diff --quiet; then echo "autofix changed files:"; git diff --stat | tail -n 10; status=1; fi
  echo "gate: autofix exit $status"
  record "autofix" "$status"
fi

echo "=== GPU GATE SUMMARY ==="
echo "$summary"
if [ "$overall" -eq 0 ]; then echo "GPU GATE VERDICT: PASS"; else echo "GPU GATE VERDICT: FAIL ($overall)"; fi
exit 0
