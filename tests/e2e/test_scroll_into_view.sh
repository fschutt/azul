#!/bin/bash
#
# Test script for scroll-into-view behavior when tabbing
#
# Tests:
# 1. Tabbing to off-screen element triggers scroll
# 2. Shift+Tab to element above viewport triggers scroll up
# 3. Scroll position is preserved between focus changes
# 4. Wrap-around scrolls back to top/bottom
#
# NOTE: This test requires scroll position reporting from the debug API.
#       Currently tests focus tracking; scroll verification is manual.
#

set -e

PORT=8765
BASE_URL="http://localhost:$PORT"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

TESTS_PASSED=0
TESTS_FAILED=0

# Helper functions
send_key() {
    local key=$1
    curl -s -X POST "$BASE_URL/" -d "{\"op\": \"key_down\", \"key\": \"$key\"}" > /dev/null
    sleep 0.1
    curl -s -X POST "$BASE_URL/" -d "{\"op\": \"key_up\", \"key\": \"$key\"}" > /dev/null
    sleep 0.1
}

send_key_with_shift() {
    local key=$1
    curl -s -X POST "$BASE_URL/" -d "{\"op\": \"key_down\", \"key\": \"LShift\"}" > /dev/null
    sleep 0.05
    curl -s -X POST "$BASE_URL/" -d "{\"op\": \"key_down\", \"key\": \"$key\"}" > /dev/null
    sleep 0.1
    curl -s -X POST "$BASE_URL/" -d "{\"op\": \"key_up\", \"key\": \"$key\"}" > /dev/null
    sleep 0.05
    curl -s -X POST "$BASE_URL/" -d "{\"op\": \"key_up\", \"key\": \"LShift\"}" > /dev/null
    sleep 0.1
}

get_app_state() {
    curl -s -X POST "$BASE_URL/" -d '{"op": "get_app_state"}' 2>/dev/null
}

get_last_focused() {
    get_app_state | jq -r '.data.value.state.last_focused_item // empty'
}

reset_focus() {
    curl -s -X POST "$BASE_URL/" -d '{"op": "key_down", "key": "Escape"}' > /dev/null
    sleep 0.1
    curl -s -X POST "$BASE_URL/" -d '{"op": "key_up", "key": "Escape"}' > /dev/null
    sleep 0.1
}

check_test() {
    local test_name=$1
    local expected=$2
    local actual=$3
    
    if [ "$expected" = "$actual" ]; then
        echo -e "${GREEN}✓ PASS:${NC} $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗ FAIL:${NC} $test_name - Expected '$expected', got '$actual'"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

# Check server
echo "Testing connection to debug server..."
if ! curl -s --connect-timeout 2 "$BASE_URL/" > /dev/null 2>&1; then
    echo -e "${RED}Error: Debug server not responding on port $PORT${NC}"
    echo "Start the test app with: AZUL_DEBUG=$PORT ./focus_scroll"
    exit 1
fi
echo -e "${GREEN}Connected to debug server${NC}"
echo ""

# ============================================
# Test 1: Tab to off-screen element
# ============================================
echo "=== Test 1: Tab to Off-Screen Element ==="
echo -e "${BLUE}Info: Viewport shows ~4 items. Items 5+ are initially off-screen.${NC}"
reset_focus
sleep 0.2

# Tab through first 10 items
for i in {1..10}; do
    send_key "Tab"
done

last=$(get_last_focused)
check_test "Tab reaches item 10 (off-screen)" "10" "$last"
echo -e "${YELLOW}Manual check: Item 10 should be visible in the scroll container${NC}"

# ============================================
# Test 2: Continue to last item
# ============================================
echo ""
echo "=== Test 2: Tab to Last Item ==="
for i in {11..20}; do
    send_key "Tab"
done

last=$(get_last_focused)
check_test "Tab reaches item 20 (last item)" "20" "$last"
echo -e "${YELLOW}Manual check: Scroll container should be at bottom${NC}"

# ============================================
# Test 3: Wrap-around scrolls back to top
# ============================================
echo ""
echo "=== Test 3: Tab Wrap-Around Scrolls to Top ==="
send_key "Tab"  # Should wrap to item 1

last=$(get_last_focused)
check_test "Tab wraps to item 1" "1" "$last"
echo -e "${YELLOW}Manual check: Scroll container should scroll back to top${NC}"

# ============================================
# Test 4: Shift+Tab from top wraps to bottom
# ============================================
echo ""
echo "=== Test 4: Shift+Tab Wrap-Around to Bottom ==="
send_key_with_shift "Tab"  # Should wrap to item 20

last=$(get_last_focused)
check_test "Shift+Tab wraps to item 20" "20" "$last"
echo -e "${YELLOW}Manual check: Scroll container should scroll to bottom${NC}"

# ============================================
# Test 5: Shift+Tab navigates backwards
# ============================================
echo ""
echo "=== Test 5: Shift+Tab Navigates Up ==="
# Go back 5 items
for i in {1..5}; do
    send_key_with_shift "Tab"
done

last=$(get_last_focused)
check_test "Shift+Tab from 20 by 5 reaches item 15" "15" "$last"

# ============================================
# Test 6: Tab mid-scroll
# ============================================
echo ""
echo "=== Test 6: Tab Forward from Middle ==="
for i in {1..3}; do
    send_key "Tab"
done

last=$(get_last_focused)
check_test "Tab from 15 by 3 reaches item 18" "18" "$last"

# ============================================
# Summary
# ============================================
echo ""
echo "========================================"
echo "Test Summary"
echo "========================================"
echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed: ${RED}$TESTS_FAILED${NC}"
echo ""
echo -e "${YELLOW}NOTE: Scroll position verification requires manual observation.${NC}"
echo -e "${YELLOW}The focused element should always be visible in the scroll container.${NC}"

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "\n${GREEN}All focus tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}Some tests failed!${NC}"
    exit 1
fi
