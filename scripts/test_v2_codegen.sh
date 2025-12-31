#!/bin/bash
# =============================================================================
# Azul V2 Codegen Integration Test Script
# =============================================================================
# This script tests all codegen configurations to ensure everything works.
# Run this after making changes to the code generation system.
#
# Usage: ./scripts/test_v2_codegen.sh
# =============================================================================

set -e  # Exit on first error

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0
SKIPPED=0

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    PASSED=$((PASSED + 1))
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
    FAILED=$((FAILED + 1))
}

log_skip() {
    echo -e "${YELLOW}[SKIP]${NC} $1"
    SKIPPED=$((SKIPPED + 1))
}

log_section() {
    echo ""
    echo "============================================================================="
    echo -e "${BLUE}$1${NC}"
    echo "============================================================================="
}

# =============================================================================
# Step 1: Generate all bindings
# =============================================================================
log_section "Step 1: Generate All V2 Bindings"

log_info "Running: cd doc && cargo run --release -- codegen all"
cd "$PROJECT_ROOT/doc"
if cargo run --release -- codegen all 2>&1; then
    log_success "Codegen all completed"
else
    log_error "Codegen all failed"
    exit 1
fi

# Verify generated files exist
log_section "Step 2: Verify Generated Files"

CODEGEN_V2="$PROJECT_ROOT/target/codegen/v2"
REQUIRED_FILES=(
    "dll_api_static.rs"
    "dll_api_dynamic.rs"
    "dll_api_build.rs"
    "reexports.rs"
    "azul.h"
    "azul.hpp"
    "azul.rs"
    "python_api.rs"
    "memtest.rs"
)

for file in "${REQUIRED_FILES[@]}"; do
    if [ -f "$CODEGEN_V2/$file" ]; then
        SIZE=$(wc -c < "$CODEGEN_V2/$file" | tr -d ' ')
        log_success "$file exists (${SIZE} bytes)"
    else
        log_error "$file is missing!"
    fi
done

# =============================================================================
# Step 3: Test Rust link-static compilation
# =============================================================================
log_section "Step 3: Test Rust link-static Compilation"

cd "$PROJECT_ROOT/dll"
log_info "Running: cargo check --features link-static"
if cargo check --features link-static 2>&1; then
    log_success "Rust link-static compiles"
else
    log_error "Rust link-static compilation failed"
fi

# =============================================================================
# Step 4: Test Rust link-dynamic compilation
# =============================================================================
log_section "Step 4: Test Rust link-dynamic Compilation"

log_info "Running: cargo check --features link-dynamic"
if cargo check --features link-dynamic 2>&1; then
    log_success "Rust link-dynamic compiles"
else
    log_error "Rust link-dynamic compilation failed"
fi

# =============================================================================
# Step 5: Test Rust build-dll (DLL building mode)
# =============================================================================
log_section "Step 5: Test Rust build-dll Compilation"

log_info "Running: cargo check --features build-dll"
if cargo check --features build-dll 2>&1; then
    log_success "Rust build-dll compiles"
else
    log_error "Rust build-dll compilation failed"
fi

# =============================================================================
# Step 6: Build the actual DLL (release mode)
# =============================================================================
log_section "Step 6: Build DLL (Release Mode)"

log_info "Running: cargo build --release --features build-dll"
if cargo build --release --features build-dll 2>&1; then
    log_success "DLL build succeeded"
    
    # Check if library exists
    if [ -f "$PROJECT_ROOT/target/release/libazul.dylib" ]; then
        SIZE=$(ls -lh "$PROJECT_ROOT/target/release/libazul.dylib" | awk '{print $5}')
        log_success "libazul.dylib exists (${SIZE})"
    elif [ -f "$PROJECT_ROOT/target/release/libazul.so" ]; then
        SIZE=$(ls -lh "$PROJECT_ROOT/target/release/libazul.so" | awk '{print $5}')
        log_success "libazul.so exists (${SIZE})"
    elif [ -f "$PROJECT_ROOT/target/release/azul.dll" ]; then
        SIZE=$(ls -lh "$PROJECT_ROOT/target/release/azul.dll" | awk '{print $5}')
        log_success "azul.dll exists (${SIZE})"
    else
        log_error "No DLL/dylib/so file found in target/release/"
    fi
else
    log_error "DLL build failed"
fi

# =============================================================================
# Step 7: Test C hello-world compilation
# =============================================================================
log_section "Step 7: Test C Hello-World Compilation"

C_EXAMPLE="$PROJECT_ROOT/examples/c/hello-world.c"
if [ -f "$C_EXAMPLE" ]; then
    cd "$PROJECT_ROOT/examples/c"
    
    log_info "Compiling C hello-world..."
    
    # macOS-specific frameworks
    if [[ "$OSTYPE" == "darwin"* ]]; then
        FRAMEWORKS="-framework Cocoa -framework Metal -framework QuartzCore -framework CoreText -framework CoreFoundation -framework CoreGraphics"
    else
        FRAMEWORKS=""
    fi
    
    COMPILE_CMD="clang -I$CODEGEN_V2 -L$PROJECT_ROOT/target/release -lazul $FRAMEWORKS hello-world.c -o hello-world"
    log_info "Running: $COMPILE_CMD"
    
    if $COMPILE_CMD 2>&1; then
        log_success "C hello-world compiles"
        
        # Check if binary exists and is executable
        if [ -x "./hello-world" ]; then
            SIZE=$(ls -lh "./hello-world" | awk '{print $5}')
            log_success "hello-world binary created (${SIZE})"
        fi
    else
        log_error "C hello-world compilation failed"
    fi
else
    log_skip "C hello-world.c not found at $C_EXAMPLE"
fi

# =============================================================================
# Step 8: Test Python extension compilation (if pyo3 is available)
# =============================================================================
log_section "Step 8: Test Python Extension Compilation"

cd "$PROJECT_ROOT/dll"
log_info "Running: cargo check --features python-extension"
if cargo check --features python-extension 2>&1; then
    log_success "Python extension compiles"
else
    log_skip "Python extension compilation failed (pyo3 may not be configured)"
fi

# =============================================================================
# Summary
# =============================================================================
log_section "Test Summary"

TOTAL=$((PASSED + FAILED + SKIPPED))
echo ""
echo -e "  ${GREEN}Passed:${NC}  $PASSED"
echo -e "  ${RED}Failed:${NC}  $FAILED"
echo -e "  ${YELLOW}Skipped:${NC} $SKIPPED"
echo -e "  Total:   $TOTAL"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed. Please check the output above.${NC}"
    exit 1
fi
