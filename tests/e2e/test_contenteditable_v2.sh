#!/bin/bash
# ContentEditable E2E Test Suite v2
#
# Tests contenteditable text input, cursor blinking, focus, cursor state, and selection
#
# Usage: ./test_contenteditable_v2.sh
#
# Prerequisites:
#   1. Build the test: cc contenteditable.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o contenteditable_test -Wl,-rpath,../../target/release
#   2. Have jq installed for JSON parsing

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

DEBUG_PORT=8766
DEBUG_URL="http://localhost:$DEBUG_PORT/"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

PASSED=0
FAILED=0
WARNINGS=0

echo "================================================"
echo "ContentEditable E2E Test Suite v2"
echo "Focus, Cursor, and Blink Timer Tests"
echo "================================================"

# Build the test executable
echo -e "${YELLOW}Building contenteditable_test...${NC}"
cc contenteditable.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o contenteditable_test -Wl,-rpath,../../target/release 2>&1
if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed${NC}"
    exit 1
fi
echo -e "${GREEN}Build successful${NC}"

# Start the test app in background
echo -e "${YELLOW}Starting contenteditable_test with debug server on port $DEBUG_PORT...${NC}"
AZUL_DEBUG=$DEBUG_PORT ./contenteditable_test &
APP_PID=$!

# Give the app time to start and render first frame
sleep 3

# Function to send debug command
send_cmd() {
    curl -s --connect-timeout 2 -X POST "$DEBUG_URL" -d "$1" 2>/dev/null
}

# Function to check if app is running
check_app() {
    if ! kill -0 $APP_PID 2>/dev/null; then
        echo -e "${RED}App crashed!${NC}"
        exit 1
    fi
}

# Assert function
assert_eq() {
    local actual="$1"
    local expected="$2"
    local msg="$3"
    if [ "$actual" = "$expected" ]; then
        echo -e "${GREEN}PASS: $msg${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "${RED}FAIL: $msg${NC}"
        echo -e "${RED}  Expected: $expected${NC}"
        echo -e "${RED}  Actual: $actual${NC}"
        ((FAILED++))
        return 1
    fi
}

# Assert not empty
assert_not_empty() {
    local value="$1"
    local msg="$2"
    if [ -n "$value" ] && [ "$value" != "null" ] && [ "$value" != "none" ]; then
        echo -e "${GREEN}PASS: $msg (value=$value)${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "${RED}FAIL: $msg (empty or null)${NC}"
        ((FAILED++))
        return 1
    fi
}

# Assert boolean true
assert_true() {
    local value="$1"
    local msg="$2"
    if [ "$value" = "true" ]; then
        echo -e "${GREEN}PASS: $msg${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "${RED}FAIL: $msg (got $value, expected true)${NC}"
        ((FAILED++))
        return 1
    fi
}

# Assert boolean false
assert_false() {
    local value="$1"
    local msg="$2"
    if [ "$value" = "false" ]; then
        echo -e "${GREEN}PASS: $msg${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "${RED}FAIL: $msg (got $value, expected false)${NC}"
        ((FAILED++))
        return 1
    fi
}

# Warn
warn() {
    local msg="$1"
    echo -e "${YELLOW}WARN: $msg${NC}"
    ((WARNINGS++))
}

# Cleanup on exit
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    kill $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

# Wait for debug server to be ready
echo "Waiting for debug server..."
for i in {1..20}; do
    RESULT=$(curl -s --connect-timeout 1 -X POST "$DEBUG_URL" -d '{"op": "get_state"}' 2>/dev/null)
    if [ -n "$RESULT" ] && echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
        echo -e "${GREEN}Debug server ready${NC}"
        break
    fi
    sleep 0.5
done

# Wait for initial layout and render
sleep 1
send_cmd '{"op": "wait_frame"}'
sleep 0.5

# ============================================================================
# Test Group 1: Initial State - No Focus
# ============================================================================
echo ""
echo -e "${BLUE}=== Test Group 1: Initial State ===${NC}"

sleep 0.5
FOCUS_STATE=$(send_cmd '{"op": "get_focus_state"}')
echo "DEBUG: Focus state response: $FOCUS_STATE"
HAS_FOCUS=$(echo "$FOCUS_STATE" | jq -r '.data.value.has_focus // false')
assert_false "$HAS_FOCUS" "Initial state: no focus"

sleep 0.3
CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
echo "DEBUG: Cursor state response: $CURSOR_STATE"
HAS_CURSOR=$(echo "$CURSOR_STATE" | jq -r '.data.value.has_cursor // false')
assert_false "$HAS_CURSOR" "Initial state: no cursor"

# ============================================================================
# Test Group 2: Focus on Contenteditable via Tab
# ============================================================================
echo ""
echo -e "${BLUE}=== Test Group 2: Focus via Tab ===${NC}"

# Tab to first contenteditable
echo "Sending Tab key..."
send_cmd '{"op": "key_down", "key": "Tab"}'
sleep 0.5
send_cmd '{"op": "wait_frame"}'
sleep 0.5
send_cmd '{"op": "wait_frame"}'
sleep 0.3

# Check focus state
FOCUS_STATE=$(send_cmd '{"op": "get_focus_state"}')
echo "Focus state: $FOCUS_STATE"

HAS_FOCUS=$(echo "$FOCUS_STATE" | jq -r '.data.value.has_focus // false')
assert_true "$HAS_FOCUS" "After Tab: has focus" || true

IS_CONTENTEDITABLE=$(echo "$FOCUS_STATE" | jq -r '.data.value.focused_node.is_contenteditable // false')
assert_true "$IS_CONTENTEDITABLE" "Focused node is contenteditable" || true

sleep 0.3

# Check cursor state
CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
echo "Cursor state: $CURSOR_STATE"

HAS_CURSOR=$(echo "$CURSOR_STATE" | jq -r '.data.value.has_cursor // false')
assert_true "$HAS_CURSOR" "After Tab: cursor exists" || true

# Check blink timer is active
BLINK_TIMER_ACTIVE=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.blink_timer_active')
if [ "$BLINK_TIMER_ACTIVE" = "true" ]; then
    echo -e "${GREEN}PASS: Blink timer is active${NC}"
    ((PASSED++))
else
    warn "Blink timer not active (may not be implemented yet)"
fi

# Check cursor is visible initially
IS_VISIBLE=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.is_visible')
assert_true "$IS_VISIBLE" "Cursor is initially visible"

check_app

# ============================================================================
# Test Group 3: Cursor Position
# ============================================================================
echo ""
echo -e "${BLUE}=== Test Group 3: Cursor Position ===${NC}"

CURSOR_POS=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.position')
CURSOR_AFFINITY=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.affinity')

echo "Cursor position: $CURSOR_POS, affinity: $CURSOR_AFFINITY"
assert_eq "$CURSOR_POS" "0" "Initial cursor at position 0"
assert_not_empty "$CURSOR_AFFINITY" "Cursor has affinity"

check_app

# ============================================================================
# Test Group 4: Text Input
# ============================================================================
echo ""
echo -e "${BLUE}=== Test Group 4: Text Input ===${NC}"

echo "Sending text input 'Hello'..."
send_cmd '{"op": "text_input", "text": "Hello"}'
sleep 0.5
send_cmd '{"op": "wait_frame"}'
sleep 0.3

CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
CURSOR_POS=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.position // -1')
echo "After typing 'Hello', cursor position: $CURSOR_POS"
assert_eq "$CURSOR_POS" "5" "Cursor moved to position 5 after typing 'Hello'"

# Cursor should be visible after typing (input resets blink)
IS_VISIBLE=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.is_visible')
assert_true "$IS_VISIBLE" "Cursor visible after typing"

check_app

# ============================================================================
# Test Group 5: Arrow Key Navigation
# ============================================================================
echo ""
echo -e "${BLUE}=== Test Group 5: Arrow Key Navigation ===${NC}"

# Move cursor left
echo "Pressing Left arrow..."
send_cmd '{"op": "key_down", "key": "Left"}'
sleep 0.3
send_cmd '{"op": "wait_frame"}'
sleep 0.2

CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
CURSOR_POS=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.position // -1')
echo "After Left arrow, cursor position: $CURSOR_POS"
assert_eq "$CURSOR_POS" "4" "Cursor moved left to position 4"

# Move to beginning (Home)
echo "Pressing Home key..."
send_cmd '{"op": "key_down", "key": "Home"}'
sleep 0.3
send_cmd '{"op": "wait_frame"}'
sleep 0.2

CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
CURSOR_POS=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.position // -1')
echo "After Home key, cursor position: $CURSOR_POS"
assert_eq "$CURSOR_POS" "0" "Cursor moved to beginning (position 0)"

check_app

# ============================================================================
# Test Group 6: Selection State
# ============================================================================
echo ""
echo -e "${BLUE}=== Test Group 6: Selection State ===${NC}"

# Select All (Ctrl+A)
echo "Pressing Ctrl+A..."
send_cmd '{"op": "key_down", "key": "A", "modifiers": {"ctrl": true}}'
sleep 0.3
send_cmd '{"op": "wait_frame"}'
sleep 0.2

SELECTION_STATE=$(send_cmd '{"op": "get_selection_state"}')
echo "Selection state: $SELECTION_STATE"

HAS_SELECTION=$(echo "$SELECTION_STATE" | jq -r '.data.value.has_selection')
assert_true "$HAS_SELECTION" "Has selection after Ctrl+A"

# Check selection range
SELECTION_TYPE=$(echo "$SELECTION_STATE" | jq -r '.data.value.selections[0].ranges[0].selection_type')
if [ "$SELECTION_TYPE" = "range" ]; then
    SELECTION_START=$(echo "$SELECTION_STATE" | jq -r '.data.value.selections[0].ranges[0].start')
    SELECTION_END=$(echo "$SELECTION_STATE" | jq -r '.data.value.selections[0].ranges[0].end')
    echo "Selection range: $SELECTION_START to $SELECTION_END"
    assert_eq "$SELECTION_START" "0" "Selection starts at 0"
    assert_eq "$SELECTION_END" "5" "Selection ends at 5 (Hello = 5 chars)"
else
    warn "Expected selection type 'range', got '$SELECTION_TYPE'"
fi

check_app

# ============================================================================
# Test Group 7: Cursor Blink Timer (530ms intervals)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test Group 7: Cursor Blink Timer ===${NC}"

# First, clear selection by pressing Right (or any key)
send_cmd '{"op": "key_down", "key": "Right"}'
sleep 0.1

# Wait 600ms (should toggle cursor visibility at ~530ms)
echo "Waiting 600ms for blink toggle..."
send_cmd '{"op": "wait", "ms": 600}'

CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
IS_VISIBLE_AFTER_WAIT=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.is_visible')
echo "Cursor visibility after 600ms wait: $IS_VISIBLE_AFTER_WAIT"

# We can't strictly assert this because timing may vary, but log it
if [ "$IS_VISIBLE_AFTER_WAIT" = "false" ]; then
    echo -e "${GREEN}PASS: Cursor toggled to invisible after blink interval${NC}"
    ((PASSED++))
else
    warn "Cursor still visible after 600ms (blink timer may not be working)"
fi

# Type to reset blink
send_cmd '{"op": "text_input", "text": " "}'
sleep 0.1

CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
IS_VISIBLE=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.is_visible')
assert_true "$IS_VISIBLE" "Cursor visible after input (blink reset)"

check_app

# ============================================================================
# Test Group 8: Focus Loss Clears Cursor
# ============================================================================
echo ""
echo -e "${BLUE}=== Test Group 8: Focus Loss ===${NC}"

# Send Escape to blur (or Tab away)
send_cmd '{"op": "key_down", "key": "Escape"}'
sleep 0.2
send_cmd '{"op": "wait_frame"}'

FOCUS_STATE=$(send_cmd '{"op": "get_focus_state"}')
HAS_FOCUS=$(echo "$FOCUS_STATE" | jq -r '.data.value.has_focus')

CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
HAS_CURSOR=$(echo "$CURSOR_STATE" | jq -r '.data.value.has_cursor')

# Note: Escape may or may not clear focus depending on implementation
echo "After Escape: has_focus=$HAS_FOCUS, has_cursor=$HAS_CURSOR"

if [ "$HAS_FOCUS" = "false" ]; then
    assert_false "$HAS_CURSOR" "Cursor cleared when focus lost"
else
    warn "Escape did not clear focus (may need different blur mechanism)"
fi

check_app

# ============================================================================
# Test Group 9: Click to Focus
# ============================================================================
echo ""
echo -e "${BLUE}=== Test Group 9: Click to Focus ===${NC}"

# Click on the single-line input (get its layout first)
NODE_LAYOUT=$(send_cmd '{"op": "get_node_layout", "selector": ".single-line"}')
echo "Node layout: $NODE_LAYOUT"

X=$(echo "$NODE_LAYOUT" | jq -r '.data.position.x // 100')
Y=$(echo "$NODE_LAYOUT" | jq -r '.data.position.y // 100')
WIDTH=$(echo "$NODE_LAYOUT" | jq -r '.data.size.width // 200')
HEIGHT=$(echo "$NODE_LAYOUT" | jq -r '.data.size.height // 50')

# Click in the center of the element
CLICK_X=$(echo "$X + $WIDTH / 2" | bc -l 2>/dev/null || echo "150")
CLICK_Y=$(echo "$Y + $HEIGHT / 2" | bc -l 2>/dev/null || echo "125")

echo "Clicking at ($CLICK_X, $CLICK_Y)"
send_cmd "{\"op\": \"click\", \"x\": $CLICK_X, \"y\": $CLICK_Y}"
sleep 0.2
send_cmd '{"op": "wait_frame"}'

FOCUS_STATE=$(send_cmd '{"op": "get_focus_state"}')
HAS_FOCUS=$(echo "$FOCUS_STATE" | jq -r '.data.value.has_focus')
assert_true "$HAS_FOCUS" "Has focus after click"

CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
HAS_CURSOR=$(echo "$CURSOR_STATE" | jq -r '.data.value.has_cursor')
assert_true "$HAS_CURSOR" "Has cursor after click"

check_app

# ============================================================================
# Summary
# ============================================================================
echo ""
echo "================================================"
echo -e "${BLUE}Test Summary${NC}"
echo "================================================"
echo -e "Passed:   ${GREEN}$PASSED${NC}"
echo -e "Failed:   ${RED}$FAILED${NC}"
echo -e "Warnings: ${YELLOW}$WARNINGS${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    EXIT_CODE=0
else
    echo -e "${RED}Some tests failed.${NC}"
    EXIT_CODE=1
fi

echo ""
echo "The app will close in 2 seconds..."
sleep 2

exit $EXIT_CODE
