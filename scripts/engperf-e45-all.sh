#!/usr/bin/env bash
# E45 + the idle counter, ONE run (the runner queue allows one run per agent): the whole azul-dll lib + the source gates + the Windows type check, then both clippy
# lines of the CI and the CI's workspace test command. Real exit codes; failures print their messages.
set -u
out="${RUNNER_TEMP:-/tmp}/gate_all.out"
F=azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater
cargo test -p azul-dll --lib > "$out" 2>&1
code=$?
echo "dll tests: $(grep -aE '^test result' "$out" | head -3)"
grep -aE "^test .* FAILED|^error" "$out" | head -40
awk '/^---- .* stdout ----$/{p=14} p>0{print; p--}' "$out" | head -150
echo "DLL_LIB_EXIT=$code"
cargo test -p azul-srcgate --no-fail-fast > "$out" 2>&1
code=$?
grep -aE "^test result: FAILED|^test .* FAILED|^error" "$out" | head -20
awk '/^---- .* stdout ----$/{p=14} p>0{print; p--}' "$out" | head -80
echo "SRCGATE_EXIT=$code"
# The Windows shell compiles (a Linux runner type-checks it for x86_64-pc-windows-gnu: scripts/ci/startup_check_targets.sh).
bash scripts/ci/startup_check_targets.sh windows 2>&1 | tail -70
echo "WINDOWS_CHECK_EXIT=${PIPESTATUS[0]}"
cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --features "$F" -- -D warnings > "$out" 2>&1
code=$?
grep -aE "^(error|warning)" -A9 "$out" | head -80
echo "CLIPPY_FEATURES_EXIT=$code"
cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings > "$out" 2>&1
code=$?
grep -aE "^(error|warning)" -A9 "$out" | head -80
echo "CLIPPY_EXIT=$code"
cargo test -p azul-css -p azul-core -p azul-layout -p webrender --lib --tests --features "$F" --no-fail-fast > "$out" 2>&1
code=$?
echo "test binaries ok: $(grep -ac '^test result: ok' "$out"); failed: $(grep -ac '^test result: FAILED' "$out")"
grep -aE "^test result: FAILED|^test .* FAILED|^error" "$out" | head -60
awk '/^---- .* stdout ----$/{p=14} p>0{print; p--}' "$out" | head -120
echo "WORKSPACE_TEST_EXIT=$code"
