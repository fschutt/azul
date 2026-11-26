#!/usr/bin/env bash
# The CPU brush on a runner in release: its work and picture tests, then the timing of old against new.
set -u
uname -m
cargo test --release -p azul-core --lib -- resources_brush_test --nocapture 2>&1 | grep -E "BRUSH_|test result|panicked|FAILED|failed"
echo "TESTS_EXIT=${PIPESTATUS[0]}"
cargo test --release -p azul-core --lib -- --ignored --nocapture --test-threads=1 how_long_the_brush_takes 2>&1 | grep -E "BRUSH_BENCH|test result|panicked|FAILED"
echo "BENCH_EXIT=${PIPESTATUS[0]}"
