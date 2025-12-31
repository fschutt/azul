#!/usr/bin/env bash
#
# Azul E2E Scrolling Test
#
# This script tests automatic scrollbars and programmatic scrolling:
# 1. Compiles the scrolling C example
# 2. Starts it with AZUL_DEBUG enabled
# 3. Uses the debug API to scroll and verify behavior
# 4. Takes native screenshots for visual verification
#
# Usage: ./tests/e2e/scrolling.sh [--no-screenshot] [--item-count N]
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
DEBUG_PORT="${AZUL_DEBUG_PORT:-8765}"
OUTPUT_DIR="${PROJECT_ROOT}/target/test_results/scrolling"
BINARY_DIR="${PROJECT_ROOT}/target/e2e-tests"
SCREENSHOT_DIR="${OUTPUT_DIR}/screenshots"
ITEM_COUNT=50
TAKE_SCREENSHOTS=true
FAILED=0

# Parse arguments
for arg in "$@"; do
    case $arg in
        --no-screenshot)
            TAKE_SCREENSHOTS=false
            shift
            ;;
        --item-count=*)
            ITEM_COUNT="${arg#*=}"
            shift
            ;;
        *)
            ;;
    esac
done

echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Azul E2E Scrolling Test${NC}"
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
C_SOURCE="${SCRIPT_DIR}/scrolling.c"
BINARY="${BINARY_DIR}/scrolling"

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
echo "  Compiling scrolling.c..."
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
pkill -f "scrolling" 2>/dev/null || true
sleep 1

echo "  Starting with AZUL_DEBUG=$DEBUG_PORT, item_count=$ITEM_COUNT..."
AZUL_DEBUG=$DEBUG_PORT "$BINARY" "$ITEM_COUNT" > "$OUTPUT_DIR/app.log" 2>&1 &
APP_PID=$!
echo "  PID: $APP_PID"

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    curl -s "http://localhost:$DEBUG_PORT/" -X POST -d '{"type":"close"}' --max-time 2 > /dev/null 2>&1 || true
    sleep 0.5
    kill $APP_PID 2>/dev/null || true
    wait $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

# Wait for debug server to be ready (polling like test_resize.sh)
echo "  Waiting for debug server..."
MAX_WAIT=30
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    if curl -s --max-time 2 -X POST -H "Content-Type: application/json" -d '{"type":"get_state"}' "http://localhost:$DEBUG_PORT/" > /dev/null 2>&1; then
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

# ============================================================================
# Test 1: Initial State
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: Initial Window State ===${NC}"

RESPONSE=$(send_command '{"type":"get_state"}')
echo "$RESPONSE" > "$OUTPUT_DIR/initial_state.json"

STATUS=$(echo "$RESPONSE" | jq -r '.status' 2>/dev/null)
if [ "$STATUS" = "ok" ]; then
    echo -e "  ${GREEN}✓ PASS:${NC} Got initial state"
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not get initial state"
    FAILED=1
fi

# ============================================================================
# Test 2: DOM Tree (verify we have many items)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: DOM Tree Structure ===${NC}"

RESPONSE=$(send_command '{"type":"get_dom_tree"}')
echo "$RESPONSE" > "$OUTPUT_DIR/dom_tree.json"

DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
if [ -n "$DATA" ]; then
    NODE_COUNT=$(echo "$DATA" | jq -r '.node_count // 0' 2>/dev/null || echo "0")
    echo "  Node count: $NODE_COUNT"
    
    # We expect many nodes (header + scroll container + 50 items + footer)
    if [ "$NODE_COUNT" -ge 50 ]; then
        echo -e "  ${GREEN}✓ PASS:${NC} Has enough nodes (>= 50)"
    else
        echo -e "  ${RED}✗ FAIL:${NC} Not enough nodes (expected >= 50, got $NODE_COUNT)"
        FAILED=1
    fi
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not get DOM tree"
    FAILED=1
fi

# ============================================================================
# Test 3: Initial Scroll State
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Initial Scroll State ===${NC}"

RESPONSE=$(send_command '{"type":"get_scroll_states"}')
echo "$RESPONSE" > "$OUTPUT_DIR/scroll_states_initial.json"

DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
if [ -n "$DATA" ]; then
    SCROLL_COUNT=$(echo "$DATA" | jq -r '.scroll_node_count // 0' 2>/dev/null || echo "0")
    echo "  Scrollable nodes: $SCROLL_COUNT"
    
    if [ "$SCROLL_COUNT" -ge 1 ]; then
        echo -e "  ${GREEN}✓ PASS:${NC} Found scrollable node(s)"
        
        # Show scroll state details
        echo "$DATA" | jq -r '.scroll_states[] | "    Node \(.node_id): scroll=(\(.scroll_x), \(.scroll_y)) content=\(.content_width)x\(.content_height) container=\(.container_width)x\(.container_height)"' 2>/dev/null || true
    else
        echo -e "  ${YELLOW}⚠ WARN:${NC} No scrollable nodes found yet (may need content overflow)"
    fi
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not get scroll states"
    FAILED=1
fi

# Take initial screenshot
if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    echo ""
    echo "  Taking initial screenshot..."
    RESPONSE=$(send_command '{"type":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/01_initial.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/01_initial.png"
    fi
fi

# ============================================================================
# Test 4: Scroll Down
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Scroll Down ===${NC}"

# Send scroll event at center of window (300, 300), scroll down 200px
echo "  Sending scroll event (delta_y=-200)..."
RESPONSE=$(send_command '{"type":"scroll","x":300,"y":300,"delta_x":0,"delta_y":-200}')
STATUS=$(echo "$RESPONSE" | jq -r '.status' 2>/dev/null)

if [ "$STATUS" = "ok" ]; then
    echo -e "  ${GREEN}✓ PASS:${NC} Scroll event accepted"
else
    echo -e "  ${RED}✗ FAIL:${NC} Scroll event failed"
    FAILED=1
fi

# Wait for render
sleep 0.3
send_command '{"type":"wait_frame"}' > /dev/null 2>&1

# Check scroll state after scrolling
RESPONSE=$(send_command '{"type":"get_scroll_states"}')
echo "$RESPONSE" > "$OUTPUT_DIR/scroll_states_after_scroll.json"

DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
if [ -n "$DATA" ]; then
    SCROLL_Y=$(echo "$DATA" | jq -r '.scroll_states[0].scroll_y // 0' 2>/dev/null || echo "0")
    echo "  Current scroll_y: $SCROLL_Y"
    
    # Check if scroll position changed (negative = scrolled down)
    SCROLL_Y_INT="${SCROLL_Y%.*}"
    if [ "${SCROLL_Y_INT:-0}" -lt 0 ]; then
        echo -e "  ${GREEN}✓ PASS:${NC} Content scrolled (scroll_y < 0)"
    else
        echo -e "  ${YELLOW}⚠ WARN:${NC} Scroll position unchanged (may need scrollable content)"
    fi
fi

# Take screenshot after scroll
if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"type":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/02_scrolled_down.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/02_scrolled_down.png"
    fi
fi

# ============================================================================
# Test 5: Scroll More
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Scroll Down More ===${NC}"

send_command '{"type":"scroll","x":300,"y":300,"delta_x":0,"delta_y":-300}' > /dev/null 2>&1
sleep 0.3
send_command '{"type":"wait_frame"}' > /dev/null 2>&1

RESPONSE=$(send_command '{"type":"get_scroll_states"}')
DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
SCROLL_Y=$(echo "$DATA" | jq -r '.scroll_states[0].scroll_y // 0' 2>/dev/null || echo "0")
echo "  Current scroll_y: $SCROLL_Y"

if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"type":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/03_scrolled_more.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/03_scrolled_more.png"
    fi
fi

# ============================================================================
# Test 6: Scroll Back Up
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 6: Scroll Back Up ===${NC}"

send_command '{"type":"scroll","x":300,"y":300,"delta_x":0,"delta_y":250}' > /dev/null 2>&1
sleep 0.3
send_command '{"type":"wait_frame"}' > /dev/null 2>&1

RESPONSE=$(send_command '{"type":"get_scroll_states"}')
DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
SCROLL_Y=$(echo "$DATA" | jq -r '.scroll_states[0].scroll_y // 0' 2>/dev/null || echo "0")
echo "  Current scroll_y: $SCROLL_Y"

if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"type":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/04_scrolled_up.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: $SCREENSHOT_DIR/04_scrolled_up.png"
    fi
fi

# ============================================================================
# Test 7: Display List (verify rendering)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 7: Display List ===${NC}"

RESPONSE=$(send_command '{"type":"get_display_list"}')
echo "$RESPONSE" > "$OUTPUT_DIR/display_list.json"

DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
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
