#!/bin/bash
# =============================================================================
# Azul Header Syntax Check Script
# =============================================================================
# This script checks all generated headers for syntax errors.
#
# Usage: ./scripts/test_headers.sh
# =============================================================================

set -e  # Exit on first error

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CODEGEN_V2="$PROJECT_ROOT/target/codegen/v2"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0

# Helper functions
log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    PASSED=$((PASSED + 1))
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
    FAILED=$((FAILED + 1))
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_section() {
    echo ""
    echo "============================================================================="
    echo -e "${BLUE}$1${NC}"
    echo "============================================================================="
}

# =============================================================================
# Step 1: Check C Header Syntax
# =============================================================================
log_section "Step 1: Check C Header Syntax (azul.h)"

if [ -f "$CODEGEN_V2/azul.h" ]; then
    log_info "Checking azul.h..."
    if clang -fsyntax-only -I "$CODEGEN_V2" "$CODEGEN_V2/azul.h" 2>&1; then
        log_success "azul.h has valid C syntax"
    else
        log_error "azul.h has syntax errors"
    fi
else
    log_error "azul.h not found"
fi

# =============================================================================
# Step 2: Check C++ Header Syntax (all versions)
# =============================================================================
log_section "Step 2: Check C++ Header Syntax"

CPP_HEADERS=(
    "azul03.hpp:c++03"
    "azul11.hpp:c++11"
    "azul14.hpp:c++14"
    "azul17.hpp:c++17"
    "azul20.hpp:c++20"
    "azul23.hpp:c++23"
)

for entry in "${CPP_HEADERS[@]}"; do
    header="${entry%%:*}"
    std="${entry##*:}"
    
    if [ -f "$CODEGEN_V2/$header" ]; then
        log_info "Checking $header with -std=$std..."
        if clang++ -fsyntax-only -std=$std -I "$CODEGEN_V2" "$CODEGEN_V2/$header" 2>&1; then
            log_success "$header has valid C++ syntax"
        else
            log_error "$header has syntax errors"
        fi
    else
        log_info "$header not found (skipping)"
    fi
done

# =============================================================================
# Step 3: Check C Examples Compilation (syntax only)
# =============================================================================
log_section "Step 3: Check C Examples Syntax"

C_EXAMPLES_DIR="$PROJECT_ROOT/examples/c"
for c_file in "$C_EXAMPLES_DIR"/*.c; do
    if [ -f "$c_file" ]; then
        filename=$(basename "$c_file")
        log_info "Checking $filename..."
        
        # Only syntax check, don't link
        if clang -fsyntax-only -I "$CODEGEN_V2" "$c_file" 2>&1; then
            log_success "$filename has valid syntax"
        else
            log_error "$filename has syntax errors"
        fi
    fi
done

# =============================================================================
# Step 4: Check C++ Examples Compilation (syntax only)
# =============================================================================
log_section "Step 4: Check C++ Examples Syntax"

CPP_EXAMPLES_DIR="$PROJECT_ROOT/examples/cpp"
for cpp_file in "$CPP_EXAMPLES_DIR"/*.cpp; do
    if [ -f "$cpp_file" ]; then
        filename=$(basename "$cpp_file")
        log_info "Checking $filename..."
        
        # Use C++17 as default for examples
        if clang++ -fsyntax-only -std=c++17 -I "$CODEGEN_V2" "$cpp_file" 2>&1; then
            log_success "$filename has valid syntax"
        else
            log_error "$filename has syntax errors"
        fi
    fi
done

# =============================================================================
# Summary
# =============================================================================
log_section "Summary"

TOTAL=$((PASSED + FAILED))
echo ""
echo -e "  ${GREEN}Passed:${NC}  $PASSED"
echo -e "  ${RED}Failed:${NC}  $FAILED"
echo -e "  Total:   $TOTAL"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All syntax checks passed!${NC}"
    exit 0
else
    echo -e "${RED}Some syntax checks failed. Please check the output above.${NC}"
    exit 1
fi
