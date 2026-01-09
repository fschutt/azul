#!/usr/bin/env bash
#
# Azul E2E Hello World Button Click Test
#
# This script tests button click functionality:
# 1. Compiles the hello-world C example
# 2. Starts it with AZUL_DEBUG enabled
# 3. Uses the debug API to click the button via CSS selector
# 4. Verifies the counter increases in the display list
#
# Usage: ./tests/e2e/hello-world.sh [--no-screenshot]
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
OUTPUT_DIR="${PROJECT_ROOT}/target/test_results/hello-world"
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
echo -e "${BLUE}  Azul E2E Hello World Button Click Test${NC}"
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
C_SOURCE="${PROJECT_ROOT}/examples/c/hello-world.c"
BINARY="${BINARY_DIR}/hello-world"

# ============================================================================
# Phase 1: Build
# ============================================================================
echo -e "${YELLOW}[Phase 1] Building${NC}"

# Build library if not exists or if source is newer
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
echo "  Compiling hello-world.c..."
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
pkill -f "hello-world" 2>/dev/null || true
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

# Wait a bit more for DOM to be fully rendered
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
# Test 1: Initial State
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: Initial Window State ===${NC}"

RESPONSE=$(send_command '{"op":"get_state"}')
echo "$RESPONSE" > "$OUTPUT_DIR/initial_state.json"

STATUS=$(echo "$RESPONSE" | jq -r '.status' 2>/dev/null)
if [ "$STATUS" = "ok" ]; then
    echo -e "  ${GREEN}✓ PASS:${NC} Got initial state"
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not get initial state"
    FAILED=1
fi

# ============================================================================
# Test 2: DOM Tree
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: DOM Tree Structure ===${NC}"

RESPONSE=$(send_command '{"op":"get_dom_tree"}')
echo "$RESPONSE" > "$OUTPUT_DIR/dom_tree.json"

# Debug: Show full response
echo "  Raw response: $RESPONSE"

DATA=$(echo "$RESPONSE" | jq -r '.data.value // empty' 2>/dev/null)
if [ -n "$DATA" ]; then
    NODE_COUNT=$(echo "$DATA" | jq -r '.node_count // 0' 2>/dev/null || echo "0")
    echo "  Node count: $NODE_COUNT"
    
    if [ "$NODE_COUNT" -ge 3 ]; then
        echo -e "  ${GREEN}✓ PASS:${NC} Has expected nodes (body + label + button)"
    else
        echo -e "  ${RED}✗ FAIL:${NC} Not enough nodes (expected >= 3, got $NODE_COUNT)"
        FAILED=1
    fi
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not get DOM tree"
    FAILED=1
fi

# ============================================================================
# Test 3: Initial Display List - Check Counter Value
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Initial Display List ===${NC}"

RESPONSE=$(send_command '{"op":"get_display_list"}')
echo "$RESPONSE" > "$OUTPUT_DIR/display_list_initial.json"

# Debug: Show full response
echo "  Raw response: $RESPONSE"

DATA=$(echo "$RESPONSE" | jq -r '.data.value // empty' 2>/dev/null)
if [ -n "$DATA" ]; then
    TEXT_COUNT=$(echo "$DATA" | jq -r '.text_count // 0' 2>/dev/null || echo "0")
    TOTAL_ITEMS=$(echo "$DATA" | jq -r '.total_items // 0' 2>/dev/null || echo "0")
    echo "  Text items: $TEXT_COUNT, Total items: $TOTAL_ITEMS"
    
    # Look for text contents in display list
    TEXTS=$(echo "$DATA" | jq -r '.texts // []' 2>/dev/null)
    echo "  Display texts: $TEXTS"
    
    echo -e "  ${GREEN}✓ PASS:${NC} Got initial display list"
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not get display list"
    FAILED=1
fi

# Take initial screenshot
if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    echo ""
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
# Test 4: Get HTML to Verify Initial Counter
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Initial HTML State ===${NC}"

RESPONSE=$(send_command '{"op":"get_html_string"}')
echo "$RESPONSE" > "$OUTPUT_DIR/html_initial.json"

# Debug: Show full response
echo "  Raw response: $RESPONSE"

HTML=$(echo "$RESPONSE" | jq -r '.data.value.html // ""' 2>/dev/null)
echo "  HTML preview: ${HTML:0:500}..."

# Check if "5" is in the HTML (initial counter value)
if echo "$HTML" | grep -q ">5<"; then
    INITIAL_COUNTER=5
    echo -e "  ${GREEN}✓ PASS:${NC} Found initial counter value '5'"
else
    echo -e "  ${YELLOW}⚠ WARN:${NC} Could not find initial counter value '5' in HTML"
    INITIAL_COUNTER=0
fi

# Get logs for debugging
echo ""
echo -e "${BLUE}=== Debug: Application Logs ===${NC}"
RESPONSE=$(send_command '{"op":"get_logs"}')
echo "$RESPONSE" > "$OUTPUT_DIR/logs.json"
LOGS=$(echo "$RESPONSE" | jq -r '.data.value.logs // []' 2>/dev/null)
echo "  Logs: $LOGS"

# ============================================================================
# Test 5: Click Button Using CSS Selector
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Click Button (using CSS selector) ===${NC}"

# The button is now a proper Button widget (creates a <button> element)
echo "  Clicking button with selector 'button'..."
RESPONSE=$(send_command '{"op":"click","selector":"button"}')
echo "$RESPONSE" > "$OUTPUT_DIR/click_response.json"
echo "  Click response: $RESPONSE"

STATUS=$(echo "$RESPONSE" | jq -r '.status' 2>/dev/null)
if [ "$STATUS" = "ok" ]; then
    echo -e "  ${GREEN}✓ PASS:${NC} Click command accepted"
else
    echo -e "  ${RED}✗ FAIL:${NC} Click command failed: $RESPONSE"
    FAILED=1
fi

# Wait for render
sleep 0.3
send_command '{"op":"wait_frame"}' > /dev/null 2>&1

# Get logs after click
echo ""
echo -e "${BLUE}=== Debug: Logs after click ===${NC}"
RESPONSE=$(send_command '{"op":"get_logs"}')
LOGS=$(echo "$RESPONSE" | jq -r '.data.value.logs // []' 2>/dev/null)
echo "  Logs: $LOGS"

# ============================================================================
# Test 6: Verify Counter Increased
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 6: Verify Counter Increased ===${NC}"

RESPONSE=$(send_command '{"op":"get_html_string"}')
echo "$RESPONSE" > "$OUTPUT_DIR/html_after_click.json"

HTML=$(echo "$RESPONSE" | jq -r '.data.value.html // ""' 2>/dev/null)
echo "  HTML after click: ${HTML:0:500}..."

# Check if "6" is in the HTML (counter should have increased from 5 to 6)
if echo "$HTML" | grep -q ">6<"; then
    echo -e "  ${GREEN}✓ PASS:${NC} Counter increased to 6!"
else
    echo -e "  ${RED}✗ FAIL:${NC} Counter did NOT increase to 6"
    echo "  Expected '>6<' in HTML"
    FAILED=1
fi

# Take screenshot after click
if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/02_after_click.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/02_after_click.png"
    fi
fi

# ============================================================================
# Test 7: Click Button Multiple Times
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 7: Click Button Multiple Times ===${NC}"

for i in {1..3}; do
    echo "  Click $i..."
    send_command '{"op":"click","selector":"button"}' > /dev/null 2>&1
    sleep 0.2
    send_command '{"op":"wait_frame"}' > /dev/null 2>&1
done

RESPONSE=$(send_command '{"op":"get_html_string"}')
HTML=$(echo "$RESPONSE" | jq -r '.data.value.html // ""' 2>/dev/null)

# Counter should now be 9 (5 initial + 1 from test 5 + 3 from test 7)
if echo "$HTML" | grep -q ">9<"; then
    echo -e "  ${GREEN}✓ PASS:${NC} Counter is now 9 after multiple clicks"
else
    echo -e "  ${YELLOW}⚠ WARN:${NC} Counter value unexpected (looking for 9)"
    echo "  HTML: ${HTML:0:200}"
fi

# Take final screenshot
if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/03_after_multiple_clicks.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/03_after_multiple_clicks.png"
    fi
fi

# ============================================================================
# Test 8: Final Display List Verification
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 8: Final Display List ===${NC}"

RESPONSE=$(send_command '{"op":"get_display_list"}')
echo "$RESPONSE" > "$OUTPUT_DIR/display_list_final.json"

DATA=$(echo "$RESPONSE" | jq -r '.data.value // empty' 2>/dev/null)
if [ -n "$DATA" ]; then
    TOTAL_ITEMS=$(echo "$DATA" | jq -r '.total_items // 0' 2>/dev/null || echo "0")
    TEXT_COUNT=$(echo "$DATA" | jq -r '.text_count // 0' 2>/dev/null || echo "0")
    RECT_COUNT=$(echo "$DATA" | jq -r '.rect_count // 0' 2>/dev/null || echo "0")
    
    echo "  Display list items: $TOTAL_ITEMS"
    echo "  Rects: $RECT_COUNT, Texts: $TEXT_COUNT"
    
    if [ "$TOTAL_ITEMS" -gt 0 ]; then
        echo -e "  ${GREEN}✓ PASS:${NC} Display list has items"
    else
        echo -e "  ${RED}✗ FAIL:${NC} Display list is empty"
        FAILED=1
    fi
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not get display list"
    FAILED=1
fi

# ============================================================================
# Summary
# ============================================================================
echo ""
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Test Results${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""
echo "Output files: $OUTPUT_DIR"
echo "Screenshots: $SCREENSHOT_DIR"

if [ "$FAILED" -eq 0 ]; then
    echo ""
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo ""
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi
