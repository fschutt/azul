#!/usr/bin/env bash
# The CPU brush on an ubuntu runner: clippy of azul-core, the dev-profile tests, the release-profile tests, the timing.
set -u
cargo clippy -p azul-core --all-targets --no-deps -- -D warnings 2>&1 | tail -40
echo "CLIPPY_EXIT=${PIPESTATUS[0]}"
cargo test -p azul-core --lib -- resources paint_stroke_state_tests texture_paint --test-threads=2 2>&1 | grep -E "test result|FAILED|panicked"
echo "DEV_EXIT=${PIPESTATUS[0]}"
cargo test --release -p azul-core --lib -- resources_brush_test --nocapture 2>&1 | grep -E "BRUSH_EQUIV|BRUSH_ROUNDING|test result|panicked|FAILED"
echo "REL_EXIT=${PIPESTATUS[0]}"
cargo test --release -p azul-core --lib -- --ignored --nocapture --test-threads=1 how_long_the_brush_takes 2>&1 | grep -E "BRUSH_BENCH|test result|panicked|FAILED"
echo "BENCH_EXIT=${PIPESTATUS[0]}"
