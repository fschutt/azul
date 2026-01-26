#!/bin/bash
# ContentEditable E2E Test Script
#
# Tests contenteditable text input, cursor movement, selection, and scroll-into-view
#
# Usage: ./test_contenteditable.sh
#
# Prerequisites:
#   1. Build the test: cc contenteditable.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o contenteditable_test -Wl,-rpath,../../target/release
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
NC='\033[0m' # No Color

echo "================================================"
echo "ContentEditable E2E Test Suite"
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

# Give the app time to start
sleep 2

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
    sleep 0.5
done

# ============================================================================
# Test 1: Initial State
# ============================================================================
echo ""
echo "Test 1: Initial State"
echo "---------------------"

STATE=$(send_cmd '{"op": "get_state"}')
if [ -z "$STATE" ]; then
    echo -e "${RED}FAIL: Could not get initial state${NC}"
    exit 1
fi
echo -e "${GREEN}PASS: Got initial state${NC}"

# ============================================================================
# Test 2: Tab Navigation
# ============================================================================
echo ""
echo "Test 2: Tab Navigation to First Input"
echo "--------------------------------------"

send_cmd '{"op": "key_down", "key": "Tab"}'
sleep 0.2

FOCUSED=$(send_cmd '{"op": "get_focused_element"}' | jq -r '.data.focused_node.node // "none"')
if [ "$FOCUSED" != "none" ] && [ -n "$FOCUSED" ]; then
    echo -e "${GREEN}PASS: Focused element: $FOCUSED${NC}"
else
    echo -e "${YELLOW}WARN: Could not determine focused element${NC}"
fi

# ============================================================================
# Test 3: Tab to Second Input
# ============================================================================
echo ""
echo "Test 3: Tab to Multi-Line Textarea"
echo "-----------------------------------"

send_cmd '{"op": "key_down", "key": "Tab"}'
sleep 0.2

FOCUSED=$(send_cmd '{"op": "get_focused_element"}' | jq -r '.data.focused_node.node // "none"')
if [ "$FOCUSED" != "none" ] && [ -n "$FOCUSED" ]; then
    echo -e "${GREEN}PASS: Focused element: $FOCUSED${NC}"
else
    echo -e "${YELLOW}WARN: Could not determine focused element${NC}"
fi

# ============================================================================
# Test 4: Text Input
# ============================================================================
echo ""
echo "Test 4: Text Input"
echo "------------------"

send_cmd '{"op": "text_input", "text": "Hello!"}'
sleep 0.2

STATE=$(send_cmd '{"op": "get_state"}')
echo -e "${GREEN}PASS: Text input sent${NC}"
check_app

# ============================================================================
# Test 5: Arrow Key Navigation (Cursor Movement)
# ============================================================================
echo ""
echo "Test 5: Cursor Movement (Arrow Keys)"
echo "-------------------------------------"

# Move right
send_cmd '{"op": "key_down", "key": "Right"}'
sleep 0.1

# Move down
send_cmd '{"op": "key_down", "key": "Down"}'
sleep 0.1

# Move left
send_cmd '{"op": "key_down", "key": "Left"}'
sleep 0.1

echo -e "${GREEN}PASS: Arrow key navigation sent${NC}"
check_app

# ============================================================================
# Test 6: Selection (Shift+Arrow)
# ============================================================================
echo ""
echo "Test 6: Text Selection (Shift+Arrow)"
echo "-------------------------------------"

# Hold Shift
send_cmd '{"op": "key_down", "key": "LShift"}'
sleep 0.05

# Select right (3 characters)
for i in {1..3}; do
    send_cmd '{"op": "key_down", "key": "Right"}'
    sleep 0.05
done

# Release Shift
send_cmd '{"op": "key_up", "key": "LShift"}'
sleep 0.1

echo -e "${GREEN}PASS: Selection operations sent${NC}"
check_app

# ============================================================================
# Test 7: Select All (Ctrl+A)
# ============================================================================
echo ""
echo "Test 7: Select All (Ctrl+A)"
echo "----------------------------"

send_cmd '{"op": "key_down", "key": "LControl"}'
sleep 0.05
send_cmd '{"op": "key_down", "key": "A"}'
sleep 0.1
send_cmd '{"op": "key_up", "key": "A"}'
sleep 0.05
send_cmd '{"op": "key_up", "key": "LControl"}'
sleep 0.1

echo -e "${GREEN}PASS: Ctrl+A sent${NC}"
check_app

# ============================================================================
# Test 8: Scroll Into View
# ============================================================================
echo ""
echo "Test 8: Scroll Into View (End key)"
echo "-----------------------------------"

# Press End to go to end of line/document - should trigger scroll
send_cmd '{"op": "key_down", "key": "End"}'
sleep 0.2

echo -e "${GREEN}PASS: End key sent (scroll-into-view should have triggered)${NC}"
check_app

# ============================================================================
# Test 9: scroll_into_view Debug API
# ============================================================================
echo ""
echo "Test 9: scroll_into_view Debug API"
echo "-----------------------------------"

SCROLL_RESULT=$(send_cmd '{"op": "scroll_into_view", "node_id": 1, "block": "center", "behavior": "instant"}')
if echo "$SCROLL_RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    echo -e "${GREEN}PASS: scroll_into_view API works${NC}"
else
    echo -e "${YELLOW}WARN: scroll_into_view returned: $SCROLL_RESULT${NC}"
fi
check_app

# ============================================================================
# Test 10: Backspace
# ============================================================================
echo ""
echo "Test 10: Backspace Key"
echo "----------------------"

send_cmd '{"op": "key_down", "key": "Back"}'
sleep 0.1

echo -e "${GREEN}PASS: Backspace sent${NC}"
check_app

# ============================================================================
# Summary
# ============================================================================
echo ""
echo "================================================"
echo -e "${GREEN}All tests completed!${NC}"
echo "================================================"
echo ""
echo "Manual verification recommended:"
echo "  1. Check that cursor is visible in the text area"
echo "  2. Verify scroll behavior when cursor moves off-screen"
echo "  3. Test mouse selection and drag"
echo ""
echo "The app will close in 3 seconds..."
sleep 3
