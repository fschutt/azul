#!/usr/bin/env bash
# The brush tests against the old walks (the metered methods run the old algorithm): the work tests must fail.
# Two tests are skipped because they cannot end on the old code (2 billion dabs for a segment with an infinite end).
set -u
cargo test -p azul-core --lib -- resources_brush_test paint_stroke_state_tests --skip a_segment_with_huge_finite_coordinates --skip a_stroke_that_is_not_finite_draws_nothing --nocapture --test-threads=2 2>&1 | grep -E "^test |test result|panicked|assertion|left:|right:|BRUSH_WORK off|BRUSH_EQUIV"
echo "RED_EXIT=${PIPESTATUS[0]}"
