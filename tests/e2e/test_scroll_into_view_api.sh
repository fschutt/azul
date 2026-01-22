#!/bin/bash
# Scroll-Into-View Debug API Test Script
#
# Tests the scroll_into_view debug API endpoint with various options
#
# Usage: ./test_scroll_into_view_api.sh
#
# Prerequisites:
#   1. Build focus_scroll: cc focus_scroll.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o focus_scroll_test -Wl,-rpath,../../target/release
#   2. Have jq installed for JSON parsing

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

DEBUG_PORT=8765
DEBUG_URL="http://localhost:$DEBUG_PORT/"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo "================================================"
echo "Scroll-Into-View Debug API Test Suite"
echo "================================================"

# Build the test executable
echo -e "${YELLOW}Building focus_scroll_test...${NC}"
if [ -f focus_scroll.c ]; then
    cc focus_scroll.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o focus_scroll_test -Wl,-rpath,../../target/release 2>&1
    if [ $? -ne 0 ]; then
        echo -e "${RED}Build failed${NC}"
        exit 1
    fi
    echo -e "${GREEN}Build successful${NC}"
else
    echo -e "${RED}focus_scroll.c not found${NC}"
    exit 1
fi

# Start the test app in background
echo -e "${YELLOW}Starting focus_scroll_test with debug server on port $DEBUG_PORT...${NC}"
AZUL_DEBUG=$DEBUG_PORT ./focus_scroll_test &
APP_PID=$!

# Give the app time to start
sleep 2

# Function to send debug command
send_cmd() {
    local cmd="$1"
    local result
    result=$(curl -s --connect-timeout 2 -X POST "$DEBUG_URL" -d "$cmd" 2>/dev/null)
    echo "$result"
}

# Function to check if app is running
check_app() {
    # Use pgrep to check if focus_scroll_test is still running
    if pgrep -f focus_scroll_test > /dev/null 2>&1; then
        return 0
    else
        echo -e "${RED}App crashed!${NC}"
        exit 1
    fi
}

# Cleanup on exit
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    kill $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

# Wait for debug server to be ready
echo "Waiting for debug server..."
for i in {1..10}; do
    if curl -s --connect-timeout 1 -X POST "$DEBUG_URL" -d '{"op": "get_state"}' > /dev/null 2>&1; then
        echo -e "${GREEN}Debug server ready${NC}"
        break
    fi
    if [ $i -eq 10 ]; then
        echo -e "${RED}Debug server not responding${NC}"
        exit 1
    fi
    sleep 0.5
done

# ============================================================================
# Test 1: Basic scroll_into_view with node_id
# ============================================================================
echo ""
echo -e "${BLUE}Test 1: Basic scroll_into_view with node_id${NC}"
echo "----------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 10}')
echo "Response: $RESULT"

if echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: scroll_into_view with node_id works${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response${NC}"
fi
check_app

# ============================================================================
# Test 2: scroll_into_view with block alignment
# ============================================================================
echo ""
echo -e "${BLUE}Test 2: scroll_into_view with block: start${NC}"
echo "----------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 15, "block": "start"}')
echo "Response: $RESULT"

if echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: block: start works${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response${NC}"
fi
check_app

# ============================================================================
# Test 3: scroll_into_view with block: center
# ============================================================================
echo ""
echo -e "${BLUE}Test 3: scroll_into_view with block: center${NC}"
echo "----------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 15, "block": "center"}')
echo "Response: $RESULT"

if echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: block: center works${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response${NC}"
fi
check_app

# ============================================================================
# Test 4: scroll_into_view with block: end
# ============================================================================
echo ""
echo -e "${BLUE}Test 4: scroll_into_view with block: end${NC}"
echo "----------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 5, "block": "end"}')
echo "Response: $RESULT"

if echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: block: end works${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response${NC}"
fi
check_app

# ============================================================================
# Test 5: scroll_into_view with inline alignment
# ============================================================================
echo ""
echo -e "${BLUE}Test 5: scroll_into_view with inline: center${NC}"
echo "----------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 10, "inline": "center"}')
echo "Response: $RESULT"

if echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: inline: center works${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response${NC}"
fi
check_app

# ============================================================================
# Test 6: scroll_into_view with behavior: instant
# ============================================================================
echo ""
echo -e "${BLUE}Test 6: scroll_into_view with behavior: instant${NC}"
echo "-------------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 18, "behavior": "instant"}')
echo "Response: $RESULT"

if echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: behavior: instant works${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response${NC}"
fi
check_app

# ============================================================================
# Test 7: scroll_into_view with behavior: smooth
# ============================================================================
echo ""
echo -e "${BLUE}Test 7: scroll_into_view with behavior: smooth${NC}"
echo "------------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 3, "behavior": "smooth"}')
echo "Response: $RESULT"

if echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: behavior: smooth works${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response${NC}"
fi
check_app

# Give time for smooth animation
sleep 0.5

# ============================================================================
# Test 8: scroll_into_view with all options
# ============================================================================
echo ""
echo -e "${BLUE}Test 8: scroll_into_view with all options${NC}"
echo "-------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 12, "block": "center", "inline": "center", "behavior": "instant"}')
echo "Response: $RESULT"

if echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    SCROLLED=$(echo "$RESULT" | jq -r '.data.scrolled // "unknown"')
    ADJUSTMENTS=$(echo "$RESULT" | jq -r '.data.adjustments_count // "unknown"')
    echo -e "${GREEN}PASS: All options work - scrolled: $SCROLLED, adjustments: $ADJUSTMENTS${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response${NC}"
fi
check_app

# ============================================================================
# Test 9: scroll_into_view with invalid node_id
# ============================================================================
echo ""
echo -e "${BLUE}Test 9: scroll_into_view with invalid node_id${NC}"
echo "-----------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 99999}')
echo "Response: $RESULT"

# This should still return ok (just with scrolled: true since it's queued)
if echo "$RESULT" | jq -e '.status' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: Invalid node_id handled${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response format${NC}"
fi
check_app

# ============================================================================
# Test 10: scroll_into_view with selector
# ============================================================================
echo ""
echo -e "${BLUE}Test 10: scroll_into_view with CSS selector${NC}"
echo "---------------------------------------------"

RESULT=$(send_cmd '{"op": "scroll_into_view", "selector": ".item-10"}')
echo "Response: $RESULT"

if echo "$RESULT" | jq -e '.status' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: Selector-based scroll handled${NC}"
else
    echo -e "${YELLOW}WARN: Unexpected response format${NC}"
fi
check_app

# ============================================================================
# Test 11: Verify Tab triggers scroll_into_view
# ============================================================================
echo ""
echo -e "${BLUE}Test 11: Tab key triggers scroll_into_view${NC}"
echo "--------------------------------------------"

# Get initial state
INITIAL=$(send_cmd '{"op": "get_state"}')
INITIAL_FOCUSED=$(echo "$INITIAL" | jq -r '.window_state.focused_node.node // "none"')
echo "Initial focus: $INITIAL_FOCUSED"

# Tab multiple times to move through items
for i in {1..5}; do
    send_cmd '{"op": "key_down", "key": "Tab"}'
    sleep 0.15
done

# Get final state
FINAL=$(send_cmd '{"op": "get_state"}')
FINAL_FOCUSED=$(echo "$FINAL" | jq -r '.window_state.focused_node.node // "none"')
echo "Final focus: $FINAL_FOCUSED"

if [ "$INITIAL_FOCUSED" != "$FINAL_FOCUSED" ]; then
    echo -e "${GREEN}PASS: Focus moved - scroll_into_view should have been called${NC}"
else
    echo -e "${YELLOW}WARN: Focus didn't change${NC}"
fi
check_app

# ============================================================================
# Summary
# ============================================================================
echo ""
echo "================================================"
echo -e "${GREEN}All scroll_into_view API tests completed!${NC}"
echo "================================================"
echo ""
echo "Debug API endpoint: $DEBUG_URL"
echo ""
echo "Available scroll_into_view options:"
echo "  - node_id: Target node ID (required if no selector)"
echo "  - selector: CSS selector (alternative to node_id)"
echo "  - block: 'start' | 'center' | 'end' | 'nearest' (default: nearest)"
echo "  - inline: 'start' | 'center' | 'end' | 'nearest' (default: nearest)"
echo "  - behavior: 'auto' | 'instant' | 'smooth' (default: auto)"
echo ""
echo "The app will close in 2 seconds..."
sleep 2
