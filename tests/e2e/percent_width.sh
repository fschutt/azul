#!/usr/bin/env bash
#
# Azul E2E Percentage Width Test
#
# This script tests that CSS percentage widths are correctly resolved
# in the layout solver:
# 1. Compiles the percent_width C example
# 2. Starts it with AZUL_DEBUG enabled
# 3. Uses the debug API to verify that percentage widths resolve correctly
#
# Expected behavior:
# - Container (100% of 400px body) = 400px wide
# - Child with 25% = 100px wide
# - Child with 50% = 200px wide
# - Child with 75% = 300px wide
# - Child with 100% = 400px wide
#
# Usage: ./tests/e2e/percent_width.sh
#
# Exit codes:
#   0 - All tests passed
#   1 - One or more tests failed

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

cd "${PROJECT_ROOT}"

# Configuration
DEBUG_PORT="${AZUL_DEBUG_PORT:-8766}"
OUTPUT_DIR="${PROJECT_ROOT}/target/test_results/percent_width"
BINARY_DIR="${PROJECT_ROOT}/target/e2e-tests"
FAILED=0

# Tolerance for floating point comparison (pixels)
TOLERANCE=2.0

echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Azul E2E Percentage Width Test${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""

# Create output directories
mkdir -p "$OUTPUT_DIR" "$BINARY_DIR"

# Detect OS and set paths
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS_NAME="macos"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul.dylib"
    CC_FLAGS="-framework Cocoa -framework OpenGL -framework IOKit -framework CoreFoundation -framework CoreGraphics"
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul -Wl,-rpath,${PROJECT_ROOT}/target/release"
elif [[ "$OSTYPE" == "linux"* ]]; then
    OS_NAME="linux"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul.so"
    CC_FLAGS=""
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul -Wl,-rpath,${PROJECT_ROOT}/target/release -lm -lpthread -ldl"
else
    OS_NAME="windows"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/azul.dll"
    CC_FLAGS=""
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul"
fi

HEADER_DIR="${PROJECT_ROOT}/target/codegen/v2"
C_SOURCE="${SCRIPT_DIR}/percent_width.c"
BINARY="${BINARY_DIR}/percent_width"

# ============================================================================
# Phase 1: Build
# ============================================================================
echo -e "${YELLOW}[Phase 1] Building${NC}"

# Check library
if [ ! -f "$DYLIB_PATH" ]; then
    echo -e "${RED}FAIL: Library not found at $DYLIB_PATH${NC}"
    echo "Please run: cargo build -p azul-dll --release --features build-dll"
    exit 1
fi
echo "  Library: $DYLIB_PATH"

# Check header
if [ ! -f "${HEADER_DIR}/azul.h" ]; then
    echo -e "${RED}FAIL: Header not found at ${HEADER_DIR}/azul.h${NC}"
    echo "Please run: cargo run -p azul-doc -- codegen all"
    exit 1
fi
echo "  Header: ${HEADER_DIR}/azul.h"

# Compile C example
echo "  Compiling percent_width.c..."
if ! cc -o "$BINARY" "$C_SOURCE" -I"${HEADER_DIR}" ${CC_FLAGS} ${LINK_FLAGS} 2>&1 | grep -v "warning:"; then
    echo -e "${RED}FAIL: Compilation failed${NC}"
    exit 1
fi
echo -e "  ${GREEN}✓${NC} Compiled: $BINARY"

# ============================================================================
# Phase 2: Start application
# ============================================================================
echo ""
echo -e "${YELLOW}[Phase 2] Starting application${NC}"

# Kill any existing instances
pkill -f "percent_width" 2>/dev/null || true
sleep 1

echo "  Starting with AZUL_DEBUG=$DEBUG_PORT..."
AZUL_DEBUG=$DEBUG_PORT "$BINARY" > "$OUTPUT_DIR/app.log" 2>&1 &
APP_PID=$!
echo "  PID: $APP_PID"

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    curl -s "http://localhost:$DEBUG_PORT/" -X POST -d '{"op":"close"}' --max-time 2 > /dev/null 2>&1 || true
    sleep 0.5
    kill $APP_PID 2>/dev/null || true
    wait $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

# Wait for debug server to be ready
echo "  Waiting for debug server..."
MAX_WAIT=30
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    if curl -s --max-time 2 -X POST -H "Content-Type: application/json" -d '{"op":"get_state"}' "http://localhost:$DEBUG_PORT/" > /dev/null 2>&1; then
        echo -e "  ${GREEN}✓${NC} Debug server ready after ${WAITED}s"
        break
    fi
    sleep 1
    WAITED=$((WAITED + 1))
done

if [ $WAITED -ge $MAX_WAIT ]; then
    echo -e "${RED}ERROR: Debug server not responding after ${MAX_WAIT}s${NC}"
    exit 1
fi

# Helper function to send a command
send_command() {
    local cmd="$1"
    curl -s "http://localhost:$DEBUG_PORT/" -X POST -H "Content-Type: application/json" -d "$cmd" --max-time 15
}

# Helper function to check width within tolerance
check_width() {
    local actual="$1"
    local expected="$2"
    local name="$3"
    
    # Use awk for floating point comparison
    local diff=$(echo "$actual $expected" | awk '{print ($1 > $2) ? ($1 - $2) : ($2 - $1)}')
    local ok=$(echo "$diff $TOLERANCE" | awk '{print ($1 <= $2) ? "yes" : "no"}')
    
    if [ "$ok" = "yes" ]; then
        echo -e "  ${GREEN}✓ PASS:${NC} $name width = ${actual}px (expected ${expected}px, diff=${diff}px)"
        return 0
    else
        echo -e "  ${RED}✗ FAIL:${NC} $name width = ${actual}px (expected ${expected}px, diff=${diff}px)"
        return 1
    fi
}

# ============================================================================
# Test 1: Get all node layouts
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: Get Node Layouts ===${NC}"

RESPONSE=$(send_command '{"op":"get_all_nodes_layout"}')
echo "$RESPONSE" > "$OUTPUT_DIR/all_nodes_layout.json"

STATUS=$(echo "$RESPONSE" | jq -r '.status' 2>/dev/null)
if [ "$STATUS" = "ok" ]; then
    echo -e "  ${GREEN}✓ PASS:${NC} Got node layouts"
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not get node layouts"
    echo "$RESPONSE"
    FAILED=1
fi

# ============================================================================
# Test 2: Verify container width (should be ~400px)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: Container Width (100% of 400px body) ===${NC}"

CONTAINER_LAYOUT=$(send_command '{"op":"get_node_layout", "selector": ".test-container"}')
echo "$CONTAINER_LAYOUT" > "$OUTPUT_DIR/container_layout.json"

CONTAINER_WIDTH=$(echo "$CONTAINER_LAYOUT" | jq -r '.data.value.rect.width // 0' 2>/dev/null)
if ! check_width "$CONTAINER_WIDTH" "400" "Container (100%)"; then
    FAILED=1
fi

# ============================================================================
# Test 3: Verify 25% child width (should be ~100px)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Child 25% Width ===${NC}"

CHILD_25_LAYOUT=$(send_command '{"op":"get_node_layout", "selector": ".child-25"}')
echo "$CHILD_25_LAYOUT" > "$OUTPUT_DIR/child_25_layout.json"

CHILD_25_WIDTH=$(echo "$CHILD_25_LAYOUT" | jq -r '.data.value.rect.width // 0' 2>/dev/null)
if ! check_width "$CHILD_25_WIDTH" "100" "Child (25%)"; then
    FAILED=1
fi

# ============================================================================
# Test 4: Verify 50% child width (should be ~200px)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Child 50% Width ===${NC}"

CHILD_50_LAYOUT=$(send_command '{"op":"get_node_layout", "selector": ".child-50"}')
echo "$CHILD_50_LAYOUT" > "$OUTPUT_DIR/child_50_layout.json"

CHILD_50_WIDTH=$(echo "$CHILD_50_LAYOUT" | jq -r '.data.value.rect.width // 0' 2>/dev/null)
if ! check_width "$CHILD_50_WIDTH" "200" "Child (50%)"; then
    FAILED=1
fi

# ============================================================================
# Test 5: Verify 75% child width (should be ~300px)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Child 75% Width ===${NC}"

CHILD_75_LAYOUT=$(send_command '{"op":"get_node_layout", "selector": ".child-75"}')
echo "$CHILD_75_LAYOUT" > "$OUTPUT_DIR/child_75_layout.json"

CHILD_75_WIDTH=$(echo "$CHILD_75_LAYOUT" | jq -r '.data.value.rect.width // 0' 2>/dev/null)
if ! check_width "$CHILD_75_WIDTH" "300" "Child (75%)"; then
    FAILED=1
fi

# ============================================================================
# Test 6: Verify 100% child width (should be ~400px)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 6: Child 100% Width ===${NC}"

CHILD_100_LAYOUT=$(send_command '{"op":"get_node_layout", "selector": ".child-100"}')
echo "$CHILD_100_LAYOUT" > "$OUTPUT_DIR/child_100_layout.json"

CHILD_100_WIDTH=$(echo "$CHILD_100_LAYOUT" | jq -r '.data.value.rect.width // 0' 2>/dev/null)
if ! check_width "$CHILD_100_WIDTH" "400" "Child (100%)"; then
    FAILED=1
fi

# ============================================================================
# Summary
# ============================================================================
echo ""
echo -e "${BLUE}════════════════════════════════════════════${NC}"
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}  ✓ ALL TESTS PASSED${NC}"
    echo -e "${BLUE}════════════════════════════════════════════${NC}"
    exit 0
else
    echo -e "${RED}  ✗ SOME TESTS FAILED${NC}"
    echo -e "${BLUE}════════════════════════════════════════════${NC}"
    echo ""
    echo "Debug output saved to: $OUTPUT_DIR"
    exit 1
fi
