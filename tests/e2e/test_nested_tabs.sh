#!/bin/bash
#
# Test script for nested DOM tab navigation
#
# Tests:
# 1. Tab navigates through nested elements in correct order
# 2. Non-focusable containers are skipped
# 3. Focusable containers receive focus before their children
# 4. Shift+Tab works in reverse order
#

set -e

PORT=8765
BASE_URL="http://localhost:$PORT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

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

get_focus_order() {
    local state=$(get_app_state)
    echo "$state" | jq -r '.data.value.state.focus_order | map(tostring) | join(",")'
}

get_last_focused() {
    local state=$(get_app_state)
    echo "$state" | jq -r '.data.value.state.last_focused_box'
}

reset_focus() {
    curl -s -X POST "$BASE_URL/" -d '{"op": "key_down", "key": "Escape"}' > /dev/null
    sleep 0.1
    curl -s -X POST "$BASE_URL/" -d '{"op": "key_up", "key": "Escape"}' > /dev/null
    sleep 0.1
}

reset_app_state() {
    curl -s -X POST "$BASE_URL/" -d '{"op": "set_app_state", "state": {"focus_count": 0, "focus_order": [], "last_focused_box": 0}}' > /dev/null
    sleep 0.1
}

check_test() {
    local test_name=$1
    local expected=$2
    local actual=$3
    
    if [ "$expected" = "$actual" ]; then
        echo -e "${GREEN}✓ PASS:${NC} $test_name"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}✗ FAIL:${NC} $test_name - Expected '$expected', got '$actual'"
        ((TESTS_FAILED++))
    fi
}

# Check if server is running, if not start it
echo "Testing connection to debug server..."
if ! curl -s --connect-timeout 2 "$BASE_URL/" > /dev/null 2>&1; then
    echo "Starting focus_nested..."
    pkill -f focus_nested 2>/dev/null || true
    sleep 0.5
    AZUL_DEBUG=$PORT ./focus_nested > /tmp/focus_nested.log 2>&1 &
    PID=$!
    sleep 3
    if ! curl -s --connect-timeout 2 "$BASE_URL/" > /dev/null 2>&1; then
        echo -e "${RED}Error: Debug server not responding on port $PORT${NC}"
        echo "Check /tmp/focus_nested.log for errors"
        exit 1
    fi
fi
echo -e "${GREEN}Connected to debug server${NC}"
echo ""

# ============================================
# Test 1: Forward tab through nested elements
# ============================================
echo "=== Test 1: Forward Tab Navigation ==="
reset_app_state
reset_focus
sleep 0.2

# Tab through all elements
for i in {1..7}; do
    send_key "Tab"
done

order=$(get_focus_order)
expected_order="1,2,3,4,5,6,7"
check_test "Tab order through nested elements" "$expected_order" "$order"

# ============================================
# Test 2: Tab wraps around after last element
# ============================================
echo ""
echo "=== Test 2: Tab Wrap-Around ==="
reset_app_state
send_key "Tab"  # Should wrap to 1

last=$(get_last_focused)
check_test "Tab wraps to first element" "1" "$last"

# ============================================
# Test 3: Shift+Tab reverse navigation
# ============================================
echo ""
echo "=== Test 3: Shift+Tab Reverse Navigation ==="
reset_app_state
reset_focus
sleep 0.2

# Go to element 4
for i in {1..4}; do
    send_key "Tab"
done

# Now Shift+Tab back
send_key_with_shift "Tab"
last=$(get_last_focused)
check_test "Shift+Tab from 4 goes to 3" "3" "$last"

send_key_with_shift "Tab"
last=$(get_last_focused)
check_test "Shift+Tab from 3 goes to 2" "2" "$last"

# ============================================
# Test 4: Shift+Tab wraps from first to last
# ============================================
echo ""
echo "=== Test 4: Shift+Tab Wrap-Around ==="
reset_app_state
reset_focus
sleep 0.2

send_key "Tab"  # Focus element 1
send_key_with_shift "Tab"  # Should wrap to 7

last=$(get_last_focused)
check_test "Shift+Tab from 1 wraps to 7" "7" "$last"

# ============================================
# Test 5: Focusable group receives focus
# ============================================
echo ""
echo "=== Test 5: Focusable Container Receives Focus ==="
reset_app_state
reset_focus
sleep 0.2

# Tab to element 5 (group-b which is focusable)
for i in {1..5}; do
    send_key "Tab"
done

last=$(get_last_focused)
check_test "Group-b (element 5) receives focus" "5" "$last"

# ============================================
# Summary
# ============================================
echo ""
echo "========================================"
echo "Test Summary"
echo "========================================"
echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed: ${RED}$TESTS_FAILED${NC}"

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "\n${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}Some tests failed!${NC}"
    exit 1
fi
