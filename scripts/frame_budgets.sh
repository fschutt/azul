#!/usr/bin/env bash
#
# scripts/frame_budgets.sh - the layout tests' frame-time budgets, checked
# where a budget in time means something.
#
# The `*_fits_its_time_budget*` tests in layout/tests measure one frame's CPU
# time and assert it only with AZ_TIMING=1: beside the whole suite the cores
# clock down and share their caches, and even a thread's own CPU time grows
# by half, so the suite run only reports the times. This runs them in a
# release build, one at a time, with AZ_TIMING=1 - the run whose numbers the
# budgets are written for. CI runs it in its own job (`frame_budgets`).
#
# USAGE
#   scripts/frame_budgets.sh            # from the repository root or anywhere
#   CARGO_TARGET_DIR=... scripts/frame_budgets.sh
set -euo pipefail
cd "$(dirname "$0")/.."
out="${FRAME_BUDGETS_OUT:-target/frame-budgets.txt}"
mkdir -p "$(dirname "$out")"
AZ_TIMING=1 cargo test --release -p azul-layout --test all -- \
  --test-threads=1 fits_its_time_budget 2>&1 | tee "$out"
# Two budgets today; a filter that selects nothing is a renamed test, not a pass.
./scripts/zero_test_guard.sh --strict "$out"
