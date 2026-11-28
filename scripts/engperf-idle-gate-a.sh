#!/usr/bin/env bash
# E41 gate A (ubuntu, no codegen): both clippy lines of the CI, then the CI's workspace test command of css/core/layout/webrender.
# The real exit code of each is printed; failures print their messages.
set -u
out="${RUNNER_TEMP:-/tmp}/e41_gate_a.out"
F=azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater
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
