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

# Run lib + integration tests only (no doctests — those are validated in CI separately)
echo "  [1/3] azul-css tests"
cargo test --profile "${PROFILE}" --package azul-css --lib --tests 2>&1 | tail -3

echo "  [2/3] azul-core tests"
cargo test --profile "${PROFILE}" --package azul-core --lib --tests 2>&1 | tail -3

echo "  [3/3] azul-layout tests"
# Exclude slow integration tests (>10s in debug) that blow up under coverage instrumentation.
# These are still run in the regular CI test job without coverage.
SLOW_TESTS=(
  "test_scrollbar_detection"
  "flexbox_integration"
  "inline_gradient_border"
  "cache_and_dirty_propagation"
  "inline_block_text"
  "ifc_caching"
  "margin_escape_regression"
)
EXCLUDE_ARGS=""
for t in "${SLOW_TESTS[@]}"; do
  EXCLUDE_ARGS="${EXCLUDE_ARGS} --exclude-test ${t}"
done
# cargo test doesn't support --exclude-test, so we run only lib + named fast test binaries
cargo test --profile "${PROFILE}" --package azul-layout --lib 2>&1 | tail -3
for test_file in layout/tests/*.rs; do
  test_name="$(basename "${test_file}" .rs)"
  skip=false
  for slow in "${SLOW_TESTS[@]}"; do
    if [[ "${test_name}" == "${slow}" ]]; then
      skip=true
      break
    fi
  done
  if [[ "${skip}" == false ]]; then
    cargo test --profile "${PROFILE}" --package azul-layout --test "${test_name}" 2>&1 | tail -1
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