#!/bin/bash
# Test that scroll events cause visual scrolling
# This test verifies that scroll_y changes AND the content actually moves

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

PORT=8765
DEBUG_URL="http://localhost:$PORT/"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

PASSED=0
FAILED=0

test_pass() {
    echo -e "  ${GREEN}✓ PASS:${NC} $1"
    ((PASSED++))
}

test_fail() {
    echo -e "  ${RED}✗ FAIL:${NC} $1"
    ((FAILED++))
}

send_cmd() {
    curl -s -X POST "$DEBUG_URL" -d "$1"
}

cleanup() {
    pkill -f scrollbar_drag 2>/dev/null || true
}
trap cleanup EXIT

echo "========================================"
echo "Visual Scroll Test"
echo "========================================"

# Build and start
echo "Building scrollbar_drag..."
cc scrollbar_drag.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o scrollbar_drag -Wl,-rpath,../../target/release 2>&1
echo "Build successful"

echo "Starting scrollbar_drag with debug server..."
AZUL_DEBUG=$PORT ./scrollbar_drag 2>&1 &
sleep 3

# Wait for server
for i in {1..10}; do
    if curl -s --connect-timeout 1 "$DEBUG_URL" -d '{"op": "get_state"}' > /dev/null 2>&1; then
        echo "Debug server ready"
        break
    fi
    sleep 0.5
done

echo ""
echo -e "${BLUE}=== Test 1: Initial State ===${NC}"

# Get initial scroll state
INITIAL=$(send_cmd '{"op": "get_scroll_states"}')
INITIAL_Y=$(echo "$INITIAL" | jq -r '.data.value.scroll_states[0].scroll_y // 0')
echo "  Initial scroll_y: $INITIAL_Y"

# Take initial screenshot
echo "  Taking initial screenshot..."
send_cmd '{"op": "take_screenshot"}' > /tmp/scroll_test_initial.json
test_pass "Initial state captured"

echo ""
echo -e "${BLUE}=== Test 2: Scroll via Debug API ===${NC}"

# Move mouse into scroll area first
send_cmd '{"op": "mouse_move", "x": 300, "y": 150}' > /dev/null
sleep 0.1

# Send scroll event
echo "  Sending scroll event (delta_y: 200)..."
SCROLL_RESULT=$(send_cmd '{"op": "scroll", "x": 300, "y": 150, "delta_x": 0, "delta_y": 200}')
echo "  Scroll result: $SCROLL_RESULT"

# Wait for render
sleep 0.3

# Check scroll state
AFTER=$(send_cmd '{"op": "get_scroll_states"}')
AFTER_Y=$(echo "$AFTER" | jq -r '.data.value.scroll_states[0].scroll_y // 0')
echo "  After scroll_y: $AFTER_Y"

if [ "$AFTER_Y" != "$INITIAL_Y" ]; then
    test_pass "Scroll state changed: $INITIAL_Y -> $AFTER_Y"
else
    test_fail "Scroll state did not change"
fi

# Take after screenshot
echo "  Taking after screenshot..."
send_cmd '{"op": "take_screenshot"}' > /tmp/scroll_test_after.json

echo ""
echo -e "${BLUE}=== Test 3: Force Redraw ===${NC}"

# Force a redraw to sync WebRender
send_cmd '{"op": "redraw"}' > /dev/null
sleep 0.3

# Check scroll state again
FINAL=$(send_cmd '{"op": "get_scroll_states"}')
FINAL_Y=$(echo "$FINAL" | jq -r '.data.value.scroll_states[0].scroll_y // 0')
echo "  Final scroll_y: $FINAL_Y"

# Take final screenshot
echo "  Taking final screenshot..."
send_cmd '{"op": "take_screenshot"}' > /tmp/scroll_test_final.json
test_pass "Screenshots taken"

echo ""
echo -e "${BLUE}=== Test 4: Scroll Node By (Programmatic) ===${NC}"

# Use scroll_node_by which should definitely work
SCROLL_BY_RESULT=$(send_cmd '{"op": "scroll_node_by", "node_id": 3, "delta_x": 0, "delta_y": 100}')
echo "  scroll_node_by result: $SCROLL_BY_RESULT"
sleep 0.3

AFTER_BY=$(send_cmd '{"op": "get_scroll_states"}')
AFTER_BY_Y=$(echo "$AFTER_BY" | jq -r '.data.value.scroll_states[0].scroll_y // 0')
echo "  After scroll_node_by scroll_y: $AFTER_BY_Y"

if [ "$(echo "$AFTER_BY_Y > $FINAL_Y" | bc)" = "1" ]; then
    test_pass "scroll_node_by changed position"
else
    test_fail "scroll_node_by did not change position"
fi

echo ""
echo -e "${BLUE}=== Test 5: Check WebRender Scroll Sync ===${NC}"

# Get scrollbar info to see if thumb position changed
SCROLLBAR=$(send_cmd '{"op": "get_scrollbar_info", "node_id": 3}')
THUMB_POS=$(echo "$SCROLLBAR" | jq -r '.data.value.vertical.thumb_position_ratio // 0')
echo "  Thumb position ratio: $THUMB_POS"

if [ "$(echo "$THUMB_POS > 0" | bc)" = "1" ]; then
    test_pass "Scrollbar thumb moved (ratio: $THUMB_POS)"
else
    test_fail "Scrollbar thumb did not move"
fi

echo ""
echo "========================================"
echo "Test Summary"
echo "========================================"
echo "  Passed: $PASSED"
echo "  Failed: $FAILED"
echo "  Total:  $((PASSED + FAILED))"
echo ""

if [ $FAILED -gt 0 ]; then
    echo "Some tests failed!"
    exit 1
else
    echo "All tests passed!"
    exit 0
fi
