#!/usr/bin/env bash
# E22 on an ubuntu runner: the unit tests of the overlay and of the inline shift, then the integration test of the page that
# shrinks before a typed field. The end of the cargo output is printed when a build or a test fails.
set -u
out="${RUNNER_TEMP:-/tmp}/e22.out"
cargo test -p azul-layout --lib -- overlay_test positioning_test > "$out" 2>&1
code=$?
grep -E "^test |test result|panicked|left:|right:|^error" "$out" | head -60
if [ "$code" -ne 0 ]; then tail -40 "$out"; fi
echo "LIB_EXIT=$code"
cargo test -p azul-layout --test all -- a_typed_field_keeps_laying_out > "$out" 2>&1
code=$?
grep -E "^test |test result|panicked|left:|right:|^error" "$out" | head -60
if [ "$code" -ne 0 ]; then tail -40 "$out"; fi
echo "ALL_EXIT=$code"
