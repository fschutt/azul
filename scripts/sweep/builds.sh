#!/usr/bin/env bash
# The clippy sweep's runner command: the builds of the other CI jobs that the lint fixes could break:
# the no_std check, the wasm32 checks, azul-layout in every feature set a consumer turns on, the source gates.
set +e
rustup target add aarch64-unknown-none wasm32-unknown-unknown
cargo check -p azul-css --no-default-features --target aarch64-unknown-none
echo "css no_std exit $?"
cargo check -p azul-core --no-default-features --target aarch64-unknown-none
echo "core no_std exit $?"
cargo check --target wasm32-unknown-unknown -p azul-css
echo "css wasm exit $?"
cargo check --target wasm32-unknown-unknown -p azul-core
echo "core wasm exit $?"
cargo check --target wasm32-unknown-unknown -p azul-layout --no-default-features --features "text_layout,svg,xml"
echo "layout wasm exit $?"
bash scripts/check_layout_features.sh 2>&1 | tail -15
echo "layout feature sets exit ${PIPESTATUS[0]}"
cargo test -p azul-srcgate --no-fail-fast > srcgate.txt 2>&1
echo "srcgate exit $?"
grep -E "^test result: FAILED|FAILED|panicked" srcgate.txt | head -20
true
