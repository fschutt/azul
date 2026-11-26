#!/usr/bin/env bash
# The gate of E22 (a change in azul-layout) on an ubuntu runner: both clippy lines of the CI (the second with the feature set the tests
# compile), the lib tests and the folded integration binary of azul-layout, with the real exit code of each printed.
set -u
out="${RUNNER_TEMP:-/tmp}/e22_gate.out"
cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --features azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater -- -D warnings > "$out" 2>&1
code=$?
tail -60 "$out"
echo "CLIPPY_FEATURES_EXIT=$code"
cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings > "$out" 2>&1
code=$?
tail -40 "$out"
echo "CLIPPY_EXIT=$code"
cargo test -p azul-layout --lib > "$out" 2>&1
code=$?
grep -E "test result|FAILED|failed|panicked" "$out" | head -30
echo "LIB_EXIT=$code"
cargo test -p azul-layout --test all > "$out" 2>&1
code=$?
grep -E "test result|FAILED|failed|panicked" "$out" | head -30
echo "ALL_EXIT=$code"
