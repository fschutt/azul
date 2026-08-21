#!/bin/bash

# coverage.sh — Generate code-coverage report for azul (macOS / Linux)
#
# Uses grcov + llvm-tools-preview (same approach as project/scripts/gcov.sh).
# Outputs:
#   coverage/          — HTML report (grcov)
#   coverage.txt       — per-file summary (stdout-friendly)
#
# Usage:
#   ./scripts/coverage.sh          # full run (install deps, test, report)
#   ./scripts/coverage.sh --skip-install   # skip tool checks
#
# CI usage:
#   The script writes COVERAGE_SUMMARY into $GITHUB_STEP_SUMMARY when
#   running inside GitHub Actions, so the per-file table shows up in the
#   job summary.

set -euo pipefail

SKIP_INSTALL=false
if [[ "${1:-}" == "--skip-install" ]]; then
  SKIP_INSTALL=true
fi

# ── 1. Prerequisites ────────────────────────────────────────────────────
if [[ "$SKIP_INSTALL" == false ]]; then
  echo "--- Checking prerequisites ---"

  if ! command -v rustc &>/dev/null; then
    echo "Rust not found. Install from https://rustup.rs/"
    exit 1
  fi

  if ! command -v grcov &>/dev/null; then
    echo "Installing grcov..."
    cargo install grcov
  else
    echo "grcov already installed."
  fi

  if ! rustup component list --installed | grep -q 'llvm-tools'; then
    echo "Installing llvm-tools-preview..."
    rustup component add llvm-tools-preview
  else
    echo "llvm-tools-preview already installed."
  fi
fi

# ── 2. Clean previous data ──────────────────────────────────────────────
echo "--- Cleaning old coverage data ---"
PROFRAW_DIR="$(pwd)/profraw"
rm -rf ./coverage/ "${PROFRAW_DIR}" coverage.txt
# Also clean stray profraw files from previous runs
rm -f ./*.profraw
mkdir -p "${PROFRAW_DIR}"

# ── 3. Run tests with instrumentation ───────────────────────────────────
echo "--- Running tests with coverage instrumentation ---"

export CARGO_INCREMENTAL=0
export RUSTFLAGS="-C instrument-coverage"
export LLVM_PROFILE_FILE="${PROFRAW_DIR}/azul-%p-%m.profraw"

# Use custom 'coverage' profile (release + debug symbols, no strip/lto)
PROFILE="coverage"
TARGET_DIR="./target/${PROFILE}/deps"

# Lib + integration tests. Doctests are deliberately NOT run: `cargo test --doc`
# does not work under `-C instrument-coverage` in any useful way (each doctest is
# its own binary and its profraw is not attributable back to the library), and
# NO other CI job runs doctests for azul-css / azul-core / azul-layout either —
# `cd dll && cargo test` only covers the DLL's. Treat library doctests as
# UNVERIFIED; do not read this comment as a claim that something else checks
# them.
# Run a cargo-test invocation, showing the tail on success but the FAILURE
# DETAIL on failure.
#
# Every call here used to be `... 2>&1 | tail -3`. With `set -o pipefail` that
# still fails the job, but it discards the `failures:` block — so CI reported
# only "7010 passed; 1 failed" with no way to tell WHICH test, and the failure
# had to be re-derived by hand. Keep the output short when green; print the test
# names, panic messages and assertion diffs when red.
run_tests() {
  local label="$1"; shift
  local log
  log="$(mktemp)"
  if "$@" >"${log}" 2>&1; then
    tail -3 "${log}"
    rm -f "${log}"
  else
    local status=$?
    echo "::error::${label} FAILED (exit ${status}) — detail follows"
    # The `failures:` block, panics and assertion diffs; fall back to the tail.
    grep -aE '^(failures|error|test .* FAILED)|^    [a-zA-Z_:]+$|panicked at|assertion|left ==|right ==' "${log}" | tail -60 \
      || tail -40 "${log}"
    rm -f "${log}"
    return "${status}"
  fi
}

echo "  [1/3] azul-css tests"
run_tests "azul-css" cargo test --profile "${PROFILE}" --package azul-css --lib --tests

echo "  [2/3] azul-core tests"
run_tests "azul-core" cargo test --profile "${PROFILE}" --package azul-core --lib --tests

echo "  [3/3] azul-layout tests"
# Exclude slow integration tests (>10s in debug) that blow up under coverage
# instrumentation. These still run in the `test_lib` CI job without coverage.
SLOW_TESTS=(
  "test_scrollbar_detection"
  "flexbox_integration"
  "inline_gradient_border"
  "cache_and_dirty_propagation"
  "inline_block_text"
  "ifc_caching"
  "margin_escape_regression"
)
run_tests "azul-layout --lib" cargo test --profile "${PROFILE}" --package azul-layout --lib

# Enumerate the test targets from the MANIFEST, not from `layout/tests/*.rs`.
# That glob is top-level-only, so the three targets whose `path` points into a
# subdirectory — solver3_calc (tests/solver3/calc.rs), solver3_sizing
# (tests/solver3/sizing.rs) and managers_scroll_into_view
# (tests/managers/scroll_into_view.rs) — were never run and never measured.
mapfile -t LAYOUT_TESTS < <(
  cargo metadata --no-deps --format-version 1 --manifest-path layout/Cargo.toml \
    | python3 -c "
import json, sys
md = json.load(sys.stdin)
for pkg in md['packages']:
    if pkg['name'] != 'azul-layout':
        continue
    for t in pkg['targets']:
        if 'test' in t['kind']:
            # name<TAB>comma-joined required-features, so the runner can
            # pass them instead of carrying a hand-maintained table.
            print(t['name'] + '\t' + ','.join(t.get('required-features') or []))
"
)

for test_entry in "${LAYOUT_TESTS[@]}"; do
  test_name="${test_entry%%	*}"
  test_feats="${test_entry#*	}"
  [[ "${test_feats}" == "${test_entry}" ]] && test_feats=""
  skip=false
  for slow in "${SLOW_TESTS[@]}"; do
    if [[ "${test_name}" == "${slow}" ]]; then
      skip=true
      break
    fi
  done
  if [[ "${skip}" == false ]]; then
    # Some test targets declare `required-features`, and the two selection
    # modes fail in OPPOSITE ways. Selected implicitly (--tests), cargo
    # silently skips a target whose features are unmet: no warning, no error,
    # and the suite looks green while never having been built. Selected
    # explicitly, as here, cargo ERRORS instead:
    #   error: target `coretext_autoregression` in package `azul-layout`
    #          requires the features: `coretext_tests`
    # That error failed this job, and coverage BLOCKS deploy_pages, so the
    # website did not publish.
    #
    # The features come from cargo metadata now, not a hand-maintained case
    # list, so a target that gains required-features tomorrow works without
    # anyone remembering this script exists.
    extra_features=""
    if [[ -n "${test_feats}" ]]; then
      extra_features="--features ${test_feats}"
    fi
    # shellcheck disable=SC2086  # word splitting of the feature flag is intended
    run_tests "azul-layout --test ${test_name}" \
      cargo test --profile "${PROFILE}" --package azul-layout --test "${test_name}" ${extra_features}
  fi
done

# ── 4. Generate lcov data ────────────────────────────────────────────────
echo "--- Generating coverage data ---"
COVERAGE_DIR="./coverage"
LCOV_FILE="./coverage.lcov"
SOURCE_DIR="$(pwd)"

GRCOV_FILTERS=(
  --ignore "*/.cargo/*"
  --ignore "*/.rustup/*"
  --ignore "*/tests/*"
  --ignore "*/examples/*"

  # Drop the generated test suites from the MEASURED SOURCE.
  #
  # The ~2,530 `autotest_generated` tests live in 219 inline
  # `#[cfg(test)] mod autotest_generated { … }` blocks INSIDE `*/src/*.rs` —
  # 250,625 lines against 511,392 total src LOC. They are test code, ~100%
  # covered by construction, and the `--ignore "*/tests/*"` filter above is
  # path-based so it never touched them. Every coverage figure published before
  # this change (70.27% overall, window.rs 42.30% line) therefore measured a
  # denominator that was ~49% test code, i.e. was inflated roughly 2x and did
  # not describe library coverage at all.
  #
  # `--excl-start`/`--excl-stop` are regexes over the source, so this needs no
  # edit to the 219 files and picks up any suite generated later. The stop
  # pattern is a closing brace in COLUMN 0: items nested inside a module are
  # indented, so the first such line after `mod autotest_generated {` is the
  # module's own terminator. Verified against a brace-matching lexer over all
  # 219 blocks: 214 land exactly on the module end, and the 5 blocks that are
  # themselves nested one level deep overshoot by exactly one line — the
  # enclosing module's `}`, which carries no coverage regions.
  --excl-start '^\s*mod autotest_generated\s*\{'
  --excl-stop  '^\}'
)

grcov "${PROFRAW_DIR}" \
  --binary-path "${TARGET_DIR}" \
  -s "${SOURCE_DIR}" \
  -t lcov \
  "${GRCOV_FILTERS[@]}" \
  -o "${LCOV_FILE}"

# ── 5. Generate HTML report ─────────────────────────────────────────────
echo "--- Generating HTML report ---"

grcov "${PROFRAW_DIR}" \
  --binary-path "${TARGET_DIR}" \
  -s "${SOURCE_DIR}" \
  -t html \
  --branch \
  "${GRCOV_FILTERS[@]}" \
  -o "${COVERAGE_DIR}"

# ── 6. Generate text summary from lcov ──────────────────────────────────
echo "--- Generating text summary ---"

python3 -c "
import sys, os, collections

# Parse lcov file into per-file coverage
files = collections.OrderedDict()
current = None
for line in open('${LCOV_FILE}'):
    line = line.strip()
    if line.startswith('SF:'):
        current = line[3:]
    elif line.startswith('DA:') and current:
        parts = line[3:].split(',')
        if len(parts) >= 2:
            count = int(parts[1])
            if current not in files:
                files[current] = [0, 0]
            files[current][1] += 1  # total
            if count > 0:
                files[current][0] += 1  # covered
    elif line == 'end_of_record':
        current = None

# Print
print(f'{\"Coverage\":>8s}  {\"Lines\":>9s}  File')
print('-' * 65)
total_c, total_t = 0, 0
for path, (covered, total) in sorted(files.items()):
    pct = (covered / total * 100) if total > 0 else 0
    total_c += covered
    total_t += total
    print(f'{pct:7.1f}%  {covered:4d}/{total:<4d}  {path}')
pct = (total_c / total_t * 100) if total_t > 0 else 0
print('-' * 65)
print(f'{pct:7.1f}%  {total_c:4d}/{total_t:<4d}  TOTAL')
" | tee coverage.txt

rm -f "${LCOV_FILE}"

# ── 7. CI integration ───────────────────────────────────────────────────
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  echo "--- Writing coverage summary to GitHub Actions ---"
  {
    echo '### Coverage Report'
    echo ''
    echo '```'
    cat coverage.txt
    echo '```'
    echo ''
    echo "Full HTML report: download the \`coverage-report\` artifact."
  } >> "$GITHUB_STEP_SUMMARY"
fi

# ── 8. Done ─────────────────────────────────────────────────────────────
echo ""
echo "--- Done ---"
echo "HTML report: ${COVERAGE_DIR}/index.html"
echo "Text summary: coverage.txt"