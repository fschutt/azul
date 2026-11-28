#!/usr/bin/env bash
# Both clippy lines of the CI with --keep-going (every finding in one run), real exit codes.
set +e
F=azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater
out="${RUNNER_TEMP:-/tmp}/clippy_only.out"
cargo clippy --keep-going -p azul-core -p azul-css -p azul-layout --all-targets --features "$F" -- -D warnings > "$out" 2>&1
echo "CLIPPY_FEATURES_EXIT=$?"; grep -aE "^(error|warning)" -A9 "$out" | head -60
cargo clippy --keep-going -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings > "$out" 2>&1
echo "CLIPPY_EXIT=$?"; grep -aE "^(error|warning)" -A9 "$out" | head -60
