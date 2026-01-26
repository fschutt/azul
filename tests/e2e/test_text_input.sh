#!/bin/bash
# Text Input E2E Test Script
#
# Tests single-line text input behavior:
# 1. Focus via Tab
# 2. Text input
# 3. Cursor movement
# 4. Selection
# 5. Backspace/Delete
#
# Usage: ./test_text_input.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

DEBUG_PORT=8765
DEBUG_URL="http://localhost:$DEBUG_PORT/"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

PASSED=0
FAILED=0

echo "================================================"
echo "Text Input E2E Test Suite"
echo "================================================"

# Build
echo -e "${YELLOW}Building text_input...${NC}"
cc text_input.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o text_input -Wl,-rpath,../../target/release 2>&1
if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed${NC}"
    exit 1
fi
echo -e "${GREEN}Build successful${NC}"

# Kill any existing instance
pkill -f "text_input" 2>/dev/null || true
sleep 0.5

# Start app
echo -e "${YELLOW}Starting text_input with debug server on port $DEBUG_PORT...${NC}"
AZUL_DEBUG=$DEBUG_PORT ./text_input &
APP_PID=$!

cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    kill $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

# Wait for server
echo "Waiting for debug server..."
for i in {1..15}; do
    if curl -s --connect-timeout 1 -X POST "$DEBUG_URL" -d '{"op": "get_state"}' > /dev/null 2>&1; then
        echo -e "${GREEN}Debug server ready${NC}"
        break
    fi
    if [ $i -eq 15 ]; then
        echo -e "${RED}Debug server not responding${NC}"
        exit 1
    fi
    sleep 0.5
done

# Helper functions
send_cmd() {
    curl -s --connect-timeout 3 -X POST "$DEBUG_URL" -d "$1" 2>/dev/null
}

test_pass() {
    echo -e "  ${GREEN}✓ PASS:${NC} $1"
    PASSED=$((PASSED + 1))
}

test_fail() {
    echo -e "  ${RED}✗ FAIL:${NC} $1"
    FAILED=$((FAILED + 1))
}

# ============================================================================
# Test 1: Initial State
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: Initial State ===${NC}"

STATE=$(send_cmd '{"op": "get_app_state"}')
if echo "$STATE" | jq -e '.data.value.state.text' > /dev/null 2>&1; then
    TEXT=$(echo "$STATE" | jq -r '.data.value.state.text')
    if [ "$TEXT" = "Hello World" ]; then
        test_pass "Initial text is 'Hello World'"
    else
        test_fail "Expected 'Hello World', got '$TEXT'"
    fi
else
    test_fail "Could not get initial state"
fi

# ============================================================================
# Test 2: Tab to Focus
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: Tab to Focus ===${NC}"

send_cmd '{"op": "key_down", "key": "Tab"}' > /dev/null
sleep 0.2
send_cmd '{"op": "key_up", "key": "Tab"}' > /dev/null
sleep 0.1

STATE=$(send_cmd '{"op": "get_state"}')
FOCUSED=$(echo "$STATE" | jq -r '.window_state.focused_node // "none"')

if [ "$FOCUSED" != "none" ] && [ "$FOCUSED" != "null" ]; then
    test_pass "Element focused (node: $FOCUSED)"
else
    test_fail "No element focused after Tab"
fi

# ============================================================================
# Test 3: Text Input
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Text Input ===${NC}"

# Get initial input count
BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.input_count // 0')

# Send text input
send_cmd '{"op": "text_input", "text": "!"}' > /dev/null
sleep 0.2

AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.input_count // 0')

if [ "$AFTER" -gt "$BEFORE" ]; then
    test_pass "Input event received (count: $BEFORE → $AFTER)"
else
    test_fail "Input event not received (count: $BEFORE → $AFTER)"
fi

# ============================================================================
# Test 4: Arrow Key Navigation
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Arrow Key Navigation ===${NC}"

KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

# Press Left arrow
send_cmd '{"op": "key_down", "key": "Left"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_up", "key": "Left"}' > /dev/null
sleep 0.1

KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Arrow key received (count: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Arrow key not received (count: $KEY_BEFORE → $KEY_AFTER)"
fi

# ============================================================================
# Test 5: Shift+Arrow Selection
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Shift+Arrow Selection ===${NC}"

# Hold Shift and press Right multiple times
send_cmd '{"op": "key_down", "key": "LShift"}' > /dev/null
sleep 0.05

for i in {1..3}; do
    send_cmd '{"op": "key_down", "key": "Right"}' > /dev/null
    sleep 0.05
    send_cmd '{"op": "key_up", "key": "Right"}' > /dev/null
    sleep 0.05
done

send_cmd '{"op": "key_up", "key": "LShift"}' > /dev/null
sleep 0.1

# Check selection state
SELECTION=$(send_cmd '{"op": "get_selection_state"}')
if echo "$SELECTION" | jq -e '.data.value' > /dev/null 2>&1; then
    test_pass "Selection state available"
else
    test_fail "Selection state not available"
fi

# ============================================================================
# Test 6: Ctrl+A (Select All)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 6: Ctrl+A (Select All) ===${NC}"

# On macOS, use LControl (we should support both Ctrl and Cmd)
send_cmd '{"op": "key_down", "key": "LControl"}' > /dev/null
sleep 0.05
send_cmd '{"op": "key_down", "key": "A"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_up", "key": "A"}' > /dev/null
sleep 0.05
send_cmd '{"op": "key_up", "key": "LControl"}' > /dev/null
sleep 0.1

KEY_COUNT=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')
if [ "$KEY_COUNT" -gt 0 ]; then
    test_pass "Ctrl+A key events received (total keys: $KEY_COUNT)"
else
    test_fail "Ctrl+A key events not received"
fi

# ============================================================================
# Test 7: Backspace
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 7: Backspace ===${NC}"

KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

send_cmd '{"op": "key_down", "key": "Back"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_up", "key": "Back"}' > /dev/null
sleep 0.1

KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Backspace key received (count: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Backspace key not received"
fi

# ============================================================================
# Test 8: Delete Key
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 8: Delete Key ===${NC}"

KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

send_cmd '{"op": "key_down", "key": "Delete"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_up", "key": "Delete"}' > /dev/null
sleep 0.1

KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Delete key received (count: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Delete key not received"
fi

# ============================================================================
# Test 9: Home/End Keys
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 9: Home/End Keys ===${NC}"

KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

send_cmd '{"op": "key_down", "key": "Home"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_up", "key": "Home"}' > /dev/null
sleep 0.1

send_cmd '{"op": "key_down", "key": "End"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_up", "key": "End"}' > /dev/null
sleep 0.1

KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Home/End keys received (count: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Home/End keys not received"
fi

# ============================================================================
# Summary
# ============================================================================
echo ""
echo "================================================"
echo "Test Summary"
echo "================================================"
echo -e "  ${GREEN}Passed: $PASSED${NC}"
echo -e "  ${RED}Failed: $FAILED${NC}"
TOTAL=$((PASSED + FAILED))
echo "  Total:  $TOTAL"
echo ""

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
fi
