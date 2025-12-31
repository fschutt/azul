#!/usr/bin/env bash
#
# Build script to verify all Azul components compile correctly.
# This script builds:
# - Rust DLL in static mode (with rust-api feature)
# - Rust DLL in dynamic mode (with python-extension feature)
# - All C examples
# - All Rust examples (static linking)
# - All Rust examples (dynamic linking)
#
# Usage: ./scripts/build_all.sh
#
# Exit codes:
#   0 - All builds succeeded
#   1 - One or more builds failed

set -e  # Exit on first error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get the project root directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${PROJECT_ROOT}"

echo -e "${BLUE}============================================${NC}"
echo -e "${BLUE}  Azul Build Verification Script${NC}"
echo -e "${BLUE}============================================${NC}"
echo ""

FAILED=0
PASSED=0

# Helper function to run a build step
run_build() {
    local name="$1"
    shift
    local cmd="$@"
    
    echo -e "${YELLOW}[BUILD]${NC} ${name}..."
    if eval "${cmd}"; then
        echo -e "${GREEN}[PASS]${NC} ${name}"
        ((PASSED++))
        return 0
    else
        echo -e "${RED}[FAIL]${NC} ${name}"
        echo -e "${RED}       Command: ${cmd}${NC}"
        ((FAILED++))
        return 1
    fi
}

# ============================================
# Step 1: Generate codegen files
# ============================================
echo -e "\n${BLUE}--- Step 1: Code Generation ---${NC}"
run_build "Generate all codegen files" "cargo run -p azul-doc -- codegen all" || true

# ============================================
# Step 2: Build DLL in static mode
# ============================================
echo -e "\n${BLUE}--- Step 2: Build DLL (Static Linking Mode) ---${NC}"
run_build "azul-dll with rust-api feature" "cargo build -p azul-dll --features rust-api" || true

# ============================================
# Step 3: Build DLL with Python extension
# ============================================
echo -e "\n${BLUE}--- Step 3: Build DLL (Python Extension) ---${NC}"
run_build "azul-dll with python-extension feature" "cargo build -p azul-dll --features python-extension" || true

# ============================================
# Step 4: Build DLL as shared library (for dynamic linking)
# ============================================
echo -e "\n${BLUE}--- Step 4: Build Shared Library (.dylib/.so/.dll) ---${NC}"
run_build "azul-dll release dylib" "cargo build -p azul-dll --release --features build-dll" || true

# Get the shared library path
if [[ "$OSTYPE" == "darwin"* ]]; then
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul.dylib"
    DYLIB_EXT="dylib"
elif [[ "$OSTYPE" == "linux"* ]]; then
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul.so"
    DYLIB_EXT="so"
else
    DYLIB_PATH="${PROJECT_ROOT}/target/release/azul.dll"
    DYLIB_EXT="dll"
fi

# ============================================
# Step 5: Build all Rust examples (static linking)
# ============================================
echo -e "\n${BLUE}--- Step 5: Build Rust Examples (Static Linking) ---${NC}"

RUST_EXAMPLES=(
    "async"
    "calc"
    "hello-world"
    "infinity"
    "opengl"
    "widgets"
    "xhtml"
)

for example in "${RUST_EXAMPLES[@]}"; do
    run_build "Rust example: ${example} (static)" \
        "cargo build -p azul-examples --example ${example}" || true
done

# ============================================
# Step 6: Build all Rust examples (dynamic linking)
# ============================================
echo -e "\n${BLUE}--- Step 6: Build Rust Examples (Dynamic Linking) ---${NC}"

# First check if the dynamic library exists
if [[ -f "${DYLIB_PATH}" ]]; then
    RUST_DYNAMIC_EXAMPLES=(
        "hello-world-dynamic"
        "calc-dynamic"
    )

    for example in "${RUST_DYNAMIC_EXAMPLES[@]}"; do
        run_build "Rust example: ${example} (dynamic)" \
            "cargo build -p azul-examples-dynamic --example ${example}" || true
    done
else
    echo -e "${YELLOW}[SKIP]${NC} Dynamic Rust examples - shared library not found at ${DYLIB_PATH}"
fi

# ============================================
# Step 7: Build all C examples
# ============================================
echo -e "\n${BLUE}--- Step 7: Build C Examples ---${NC}"

C_EXAMPLES_DIR="${PROJECT_ROOT}/examples/c"
C_HEADER="${PROJECT_ROOT}/target/codegen/v2/azul.h"

# Check if header exists
if [[ ! -f "${C_HEADER}" ]]; then
    echo -e "${YELLOW}[SKIP]${NC} C examples - header file not found at ${C_HEADER}"
else
    # Compiler flags
    if [[ "$OSTYPE" == "darwin"* ]]; then
        CC_FLAGS="-framework Cocoa -framework OpenGL -framework IOKit -framework CoreFoundation -framework CoreGraphics"
        LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul -Wl,-rpath,${PROJECT_ROOT}/target/release"
    elif [[ "$OSTYPE" == "linux"* ]]; then
        CC_FLAGS=""
        LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul -Wl,-rpath,${PROJECT_ROOT}/target/release -lm -lpthread -ldl"
    else
        CC_FLAGS=""
        LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul"
    fi

    C_EXAMPLES=(
        "async"
        "calc"
        "hello-world"
        "infinity"
        "minimal_test"
        "opengl"
        "widgets"
        "xhtml"
    )

    # Create output directory for C binaries
    mkdir -p "${PROJECT_ROOT}/target/c-examples"

    for example in "${C_EXAMPLES[@]}"; do
        c_file="${C_EXAMPLES_DIR}/${example}.c"
        if [[ -f "${c_file}" ]]; then
            run_build "C example: ${example}" \
                "cc -o ${PROJECT_ROOT}/target/c-examples/${example} ${c_file} -I${PROJECT_ROOT}/target/codegen/v2 ${CC_FLAGS} ${LINK_FLAGS}" || true
        else
            echo -e "${YELLOW}[SKIP]${NC} C example: ${example} - file not found"
        fi
    done
fi

# ============================================
# Summary
# ============================================
echo -e "\n${BLUE}============================================${NC}"
echo -e "${BLUE}  Build Summary${NC}"
echo -e "${BLUE}============================================${NC}"
echo -e "${GREEN}Passed:${NC} ${PASSED}"
echo -e "${RED}Failed:${NC} ${FAILED}"
echo ""

if [[ ${FAILED} -gt 0 ]]; then
    echo -e "${RED}Some builds failed. Please check the errors above.${NC}"
    exit 1
else
    echo -e "${GREEN}All builds succeeded!${NC}"
    exit 0
fi
