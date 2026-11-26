#!/usr/bin/env bash
# The short gate of the font D agent after the clippy sweep: compile, both clippy steps of the CI
# (without and with the feature set the tests compile), the new tests, the golden dumps, the
# workspace tests of the CI job. Every step prints its exit code; the script fails if any step failed.
failed=""
st() { n="$1"; shift; echo "=== STEP $n: $*"; "$@"; s=$?; echo "=== STEP $n exit $s"; [ "$s" -eq 0 ] || failed="$failed $n"; }

step_check() {
  cargo check -p azul-layout --all-targets 2>&1 | tail -15
  return "${PIPESTATUS[0]}"
}
step_clippy() {
  cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings 2>&1 | grep -v "^    Checking\|^   Compiling\|^  Downloaded\|^ Downloading" | tail -80
  return "${PIPESTATUS[0]}"
}
step_clippy_features() {
  cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --features azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater -- -D warnings 2>&1 | grep -v "^    Checking\|^   Compiling\|^  Downloaded\|^ Downloading" | tail -80
  return "${PIPESTATUS[0]}"
}
step_bless() {
  AZ_BLESS=1 cargo test -p azul-css --features codegen --test codegen_goldens 2>&1 | grep -E "test result|^error|FAILED" | head -5
  rc=${PIPESTATUS[0]}
  git diff --stat -- css/tests/codegen_goldens | tail -2
  return $rc
}
step_golden() {
  AZ_GOLDEN_WRITE=1 cargo test -p azul-layout --test all -- the_layout_of_a_broad_corpus 2>&1 | grep -E "test result" | cut -c1-200
  rc=${PIPESTATUS[0]}
  git diff --stat -- layout/tests/golden | tail -8
  return $rc
}
step_tests() {
  AZ_REQUIRE_TEST_FONTS=1 cargo test -p azul-css -p azul-core -p azul-layout -p webrender --lib --tests --features azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater --no-fail-fast > workspace-tests.txt 2>&1
  rc=$?
  ./scripts/zero_test_guard.sh workspace-tests.txt
  echo "TOTAL: $(grep '^test result' workspace-tests.txt | awk '{p+=$4; f+=$6} END {print "passed", p, "failed", f}')"
  echo "--- failures"
  grep -E -A 4 'panicked at' workspace-tests.txt | cut -c1-400 | head -80
  return $rc
}
step_new() {
  cargo test -p azul-layout --test all -- hanging_punctuation_lets_a_mark text_decoration_color_and_thickness_paint text_underline_offset_and_position_place css_zoom_scales 2>&1 | grep -E "^test .*FAILED|test result|panicked|left:|right:|^error" | cut -c1-300
  return "${PIPESTATUS[0]}"
}

st check_layout step_check
st clippy step_clippy
st clippy_features step_clippy_features
st bless step_bless
st golden_write step_golden
st tests step_tests
st new_tests step_new
echo "=== SUMMARY failed steps:${failed:- none}"
[ -z "$failed" ]
