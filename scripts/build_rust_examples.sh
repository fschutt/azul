#!/bin/bash
# Build Rust examples for Azul
# Usage: ./scripts/build_rust_examples.sh [--debug]
#
# This script builds all Rust examples that are currently compatible
# with the generated API.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
EXAMPLES_DIR="${PROJECT_ROOT}/examples/rust"

# Parse arguments
BUILD_TYPE="release"
CARGO_BUILD_FLAGS="--release"

if [ "$1" == "--debug" ]; then
    BUILD_TYPE="debug"
    CARGO_BUILD_FLAGS=""
fi

echo "========================================"
echo "Building Rust examples (${BUILD_TYPE} mode)"
echo "========================================"

# List of compatible examples (update as examples are fixed)
COMPATIBLE_EXAMPLES=(
    "hello-world"
)

# List of examples that need fixing (for reference)
NEEDS_FIXING=(
    "async"
    "calc"
    "infinity"
    "opengl"
    "widgets"
    "xhtml"
)

cd "${EXAMPLES_DIR}"

# Build compatible examples
BUILT=0
FAILED=0

for example in "${COMPATIBLE_EXAMPLES[@]}"; do
    echo ""
    echo "[Building] ${example}..."
    if cargo build ${CARGO_BUILD_FLAGS} --example "${example}" 2>&1; then
        echo "[OK] ${example} compiled successfully"
        ((BUILT++))
    else
        echo "[FAIL] ${example} failed to compile"
        ((FAILED++))
    fi
done

echo ""
echo "========================================"
echo "Build Summary"
echo "========================================"
echo "Compatible examples built: ${BUILT}"
echo "Failed: ${FAILED}"
echo ""
echo "Examples needing API migration (${#NEEDS_FIXING[@]} total):"
for example in "${NEEDS_FIXING[@]}"; do
    echo "  - ${example}"
done

# List binaries
if [ "${BUILD_TYPE}" == "release" ]; then
    BINDIR="${PROJECT_ROOT}/target/release/examples"
else
    BINDIR="${PROJECT_ROOT}/target/debug/examples"
fi

echo ""
echo "Built binaries in ${BINDIR}:"
for example in "${COMPATIBLE_EXAMPLES[@]}"; do
    BINARY="${BINDIR}/${example//-/_}"
    if [ -f "${BINARY}" ]; then
        SIZE=$(du -h "${BINARY}" | cut -f1)
        echo "  ${example}: ${SIZE}"
    fi
done

if [ ${FAILED} -gt 0 ]; then
    exit 1
fi

exit 0
