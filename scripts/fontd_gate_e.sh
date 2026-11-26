#!/usr/bin/env bash
# One gate for the font D agent: compile, clippy, tests, goldens, autofix, WPT, dll, wasm, no_std.
# Every step prints its exit code; the script fails if any step failed.
rustup target add wasm32-unknown-unknown aarch64-unknown-none >/dev/null 2>&1
failed=""
st() { n="$1"; shift; echo "=== STEP $n: $*"; "$@"; s=$?; echo "=== STEP $n exit $s"; [ "$s" -eq 0 ] || failed="$failed $n"; }

step_bless() {
  AZ_BLESS=1 cargo test -p azul-css --features codegen --test codegen_goldens 2>&1 | grep -E "test result|^error|FAILED" | head -5
  rc=${PIPESTATUS[0]}
  git diff --stat -- css/tests/codegen_goldens | tail -2
  return $rc
}
step_check() {
  cargo check -p azul-layout --all-targets 2>&1 | tail -15
  return "${PIPESTATUS[0]}"
}
step_golden() {
  AZ_GOLDEN_WRITE=1 cargo test -p azul-layout --test all -- the_layout_of_a_broad_corpus 2>&1 | grep -E "test result" | cut -c1-200
  rc=${PIPESTATUS[0]}
  git diff --stat -- layout/tests/golden | tail -8
  return $rc
}
step_clippy() {
  cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings 2>&1 | grep -v "^    Checking\|^   Compiling\|^  Downloaded\|^ Downloading" | tail -80
  return "${PIPESTATUS[0]}"
}
step_tests() {
  cargo test --no-fail-fast -p azul-css -p azul-core -p azul-layout --lib --tests --features azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater > tests.log 2>&1
  rc=$?
  echo "TOTAL: $(grep '^test result' tests.log | awk '{p+=$4; f+=$6} END {print "passed", p, "failed", f}')"
  grep -E '^test result: [A-Za-z]+\. [0-9]{3,} passed' tests.log | cut -c1-120
  echo "--- failures"
  grep -E -A 4 'panicked at' tests.log | cut -c1-400 | head -80
  return $rc
}
step_new() {
  cargo test -p azul-layout --test all -- hanging_punctuation_lets_a_mark text_decoration_color_and_thickness_paint text_underline_offset_and_position_place css_zoom_scales 2>&1 | grep -E "^test .*FAILED|test result|panicked|left:|right:|^error" | cut -c1-300
  return "${PIPESTATUS[0]}"
}
step_css_new() {
  cargo test -p azul-css --lib -- font_shorthand font_variant 2>&1 | grep -E "^test .*FAILED|test result" | cut -c1-200
  return "${PIPESTATUS[0]}"
}
step_autofix() {
  ./target/release/azul-doc autofix && ./target/release/azul-doc autofix explain && ./target/release/azul-doc patch safe target/autofix/patches && ./target/release/azul-doc patch target/autofix/patches && ./target/release/azul-doc normalize
  git diff --stat -- api.json | tail -5
  git diff --quiet -- api.json
}
step_wpt() {
  AZ_WPT_OUT=$PWD/wpt-out cargo test -p azul-layout --features wpt_tests --test wpt 2>&1 | grep -E "test result|^error|panicked|expected not to pass" | cut -c1-300 | tail -20
  return "${PIPESTATUS[0]}"
}
step_dll() {
  cargo check -p azul-dll --all-targets 2>&1 | tail -15
  return "${PIPESTATUS[0]}"
}
step_e2e() {
  cargo test -p azul-layout --test e2e_json --features e2e-server 2>&1 | grep -E "test result|FAILED|^error" | tail -5
  return "${PIPESTATUS[0]}"
}
step_env() {
  cargo test -p azul-srcgate --test env_gate 2>&1 | grep -E "^test |test result|^error" | tail -10
  return "${PIPESTATUS[0]}"
}

st bless step_bless
st check_layout step_check
st golden_write step_golden
st clippy step_clippy
st tests step_tests
st new_tests step_new
st css_new_tests step_css_new
st autofix step_autofix
st wpt step_wpt
st dll_check step_dll
st e2e_json step_e2e
st env_gate step_env
st layout_features ./scripts/check_layout_features.sh
st wasm_css cargo check --target wasm32-unknown-unknown -p azul-css
st wasm_core cargo check --target wasm32-unknown-unknown -p azul-core
st wasm_layout cargo check --target wasm32-unknown-unknown -p azul-layout --no-default-features --features "text_layout,svg,xml"
st nostd_css cargo check -p azul-css --no-default-features --target aarch64-unknown-none
st nostd_core cargo check -p azul-core --no-default-features --target aarch64-unknown-none
echo "=== SUMMARY failed steps:${failed:- none}"
[ -z "$failed" ]
