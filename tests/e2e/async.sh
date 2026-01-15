#!/usr/bin/env bash
#
# Azul E2E Async Background Thread Test
#
# This script tests background thread functionality:
# 1. Compiles the async C example
# 2. Starts it with AZUL_DEBUG enabled
# 3. Uses the debug API to click the "Start" button
# 4. Verifies the progress bar updates via background thread
#
# Usage: ./tests/e2e/async.sh [--no-screenshot]
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
DEBUG_PORT="${AZUL_DEBUG_PORT:-8767}"
OUTPUT_DIR="${PROJECT_ROOT}/target/test_results/async"
BINARY_DIR="${PROJECT_ROOT}/target/e2e-tests"
SCREENSHOT_DIR="${OUTPUT_DIR}/screenshots"
TAKE_SCREENSHOTS=true
FAILED=0

# Parse arguments
for arg in "$@"; do
    case $arg in
        --no-screenshot)
            TAKE_SCREENSHOTS=false
            shift
            ;;
        *)
            ;;
    esac
done

echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Azul E2E Async Background Thread Test${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""

# Create output directories
mkdir -p "$OUTPUT_DIR" "$BINARY_DIR" "$SCREENSHOT_DIR"

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
C_SOURCE="${SCRIPT_DIR}/async.c"
BINARY="${BINARY_DIR}/async"

# ============================================================================
# Phase 1: Build
# ============================================================================
echo -e "${YELLOW}[Phase 1] Building${NC}"

# Build library if not exists
if [ ! -f "$DYLIB_PATH" ]; then
    echo "  Building azul-dll (this may take a while)..."
    if ! cargo build -p azul-dll --release --features build-dll 2>&1 | tail -5; then
        echo -e "${RED}FAIL: DLL build failed${NC}"
        exit 1
    fi
    echo -e "  ${GREEN}✓${NC} Built DLL"
fi
echo "  Library: $DYLIB_PATH"

# Build header if not exists
if [ ! -f "${HEADER_DIR}/azul.h" ]; then
    echo "  Generating C headers..."
    if ! cargo run -p azul-doc -- codegen all 2>&1 | tail -5; then
        echo -e "${RED}FAIL: Header generation failed${NC}"
        exit 1
    fi
    echo -e "  ${GREEN}✓${NC} Generated headers"
fi
echo "  Header: ${HEADER_DIR}/azul.h"

# Compile C example
echo "  Compiling async.c..."
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
pkill -f "e2e-tests/async" 2>/dev/null || true
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

# Wait for first frame
sleep 2

# Helper function to send a command
send_command() {
    local cmd="$1"
    curl -s "http://localhost:$DEBUG_PORT/" -X POST -H "Content-Type: application/json" -d "$cmd" --max-time 15
}

# Wait for first frame to render
echo "  Waiting for first frame..."
send_command '{"op":"wait_frame"}' > /dev/null 2>&1
sleep 0.5

# ============================================================================
# Test 1: Initial State - Check for Start Button
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: Initial State ===${NC}"

RESPONSE=$(send_command '{"op":"get_html_string"}')
echo "$RESPONSE" > "$OUTPUT_DIR/html_initial.json"

HTML=$(echo "$RESPONSE" | jq -r '.data.value.html // ""' 2>/dev/null)
echo "  HTML preview: ${HTML:0:300}..."

# Check if "Start" button is visible
if echo "$HTML" | grep -qi "Start"; then
    echo -e "  ${GREEN}✓ PASS:${NC} Found 'Start' button"
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not find 'Start' button"
    FAILED=1
fi

# Check for initial progress (0%)
if echo "$HTML" | grep -q "Progress: 0%"; then
    echo -e "  ${GREEN}✓ PASS:${NC} Initial progress is 0%"
else
    echo -e "  ${YELLOW}⚠ WARN:${NC} Could not verify initial progress"
fi

# Take initial screenshot
if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    echo "  Taking initial screenshot..."
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/01_initial.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/01_initial.png"
    fi
fi

# ============================================================================
# Test 2: Click Start Button
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: Click Start Button ===${NC}"

echo "  Clicking 'Start' button..."
RESPONSE=$(send_command '{"op":"click","selector":"button"}')
echo "$RESPONSE" > "$OUTPUT_DIR/click_response.json"
echo "  Click response: $RESPONSE"

STATUS=$(echo "$RESPONSE" | jq -r '.status' 2>/dev/null)
if [ "$STATUS" = "ok" ]; then
    echo -e "  ${GREEN}✓ PASS:${NC} Click command accepted"
else
    echo -e "  ${RED}✗ FAIL:${NC} Click command failed"
    FAILED=1
fi

# Wait for render
sleep 0.3
send_command '{"op":"wait_frame"}' > /dev/null 2>&1

# ============================================================================
# Test 3: Verify Background Thread Started
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Background Thread Started ===${NC}"

RESPONSE=$(send_command '{"op":"get_html_string"}')
echo "$RESPONSE" > "$OUTPUT_DIR/html_after_start.json"

HTML=$(echo "$RESPONSE" | jq -r '.data.value.html // ""' 2>/dev/null)
echo "  HTML: ${HTML:0:300}..."

# The "Start" button should be gone, replaced by "Processing..."
if echo "$HTML" | grep -qi "Processing"; then
    echo -e "  ${GREEN}✓ PASS:${NC} Background thread started - showing 'Processing...'"
else
    echo -e "  ${RED}✗ FAIL:${NC} Thread did not start (expected 'Processing...')"
    FAILED=1
fi

# Take screenshot after start
if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/02_processing.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/02_processing.png"
    fi
fi

# ============================================================================
# Test 4: Wait for Progress Updates
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Progress Updates ===${NC}"

echo "  Waiting 2 seconds for progress updates..."
sleep 2

RESPONSE=$(send_command '{"op":"get_html_string"}')
echo "$RESPONSE" > "$OUTPUT_DIR/html_progress.json"

HTML=$(echo "$RESPONSE" | jq -r '.data.value.html // ""' 2>/dev/null)
echo "  HTML: ${HTML:0:300}..."

# Extract progress percentage
PROGRESS=$(echo "$HTML" | grep -oE "Progress: [0-9]+%" | grep -oE "[0-9]+" | head -1)
if [ -n "$PROGRESS" ] && [ "$PROGRESS" -gt 0 ]; then
    echo -e "  ${GREEN}✓ PASS:${NC} Progress updated to ${PROGRESS}%"
else
    echo -e "  ${RED}✗ FAIL:${NC} Progress did not update (expected > 0%)"
    FAILED=1
fi

# Take mid-progress screenshot
if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/03_progress.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/03_progress.png"
    fi
fi

# ============================================================================
# Test 5: Wait for Completion
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Wait for Completion ===${NC}"

echo "  Waiting for thread to complete (max 10 seconds)..."
MAX_WAIT=10
WAITED=0
COMPLETED=false

while [ $WAITED -lt $MAX_WAIT ]; do
    RESPONSE=$(send_command '{"op":"get_html_string"}')
    HTML=$(echo "$RESPONSE" | jq -r '.data.value.html // ""' 2>/dev/null)
    
    # Check if "Start" button is back (indicates completion)
    if echo "$HTML" | grep -qi "Start"; then
        # Also check if progress is 100%
        if echo "$HTML" | grep -q "Progress: 100%"; then
            echo -e "  ${GREEN}✓ PASS:${NC} Thread completed! Progress: 100%"
            COMPLETED=true
            break
        fi
    fi
    
    sleep 1
    WAITED=$((WAITED + 1))
    echo "  Waiting... ${WAITED}s"
done

if [ "$COMPLETED" = "false" ]; then
    echo -e "  ${RED}✗ FAIL:${NC} Thread did not complete within ${MAX_WAIT}s"
    FAILED=1
fi

# Take final screenshot
if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/04_complete.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/04_complete.png"
    fi
fi

# ============================================================================
# Summary
# ============================================================================
echo ""
echo -e "${BLUE}════════════════════════════════════════════${NC}"
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}  ALL TESTS PASSED ✓${NC}"
else
    echo -e "${RED}  SOME TESTS FAILED ✗${NC}"
fi
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""
echo "Results saved to: $OUTPUT_DIR"

exit $FAILED
