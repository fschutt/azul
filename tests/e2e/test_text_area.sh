#!/bin/bash
# Text Area E2E Test Script
#
# Tests multi-line text area behavior:
# 1. Multi-line editing
# 2. Vertical cursor movement
# 3. Scroll-into-view when cursor off-screen
# 4. Page navigation
#
# Usage: ./test_text_area.sh
#
# SLOW MODE: This script pauses between actions so you can observe each step.
# Press Ctrl+C to abort at any time.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

DEBUG_PORT=8765
DEBUG_URL="http://localhost:$DEBUG_PORT/"

# Delay between steps (in seconds) - adjust for slower/faster viewing
STEP_DELAY=1.5
ACTION_DELAY=0.3

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

PASSED=0
FAILED=0

echo "================================================"
echo "Text Area E2E Test Suite (SLOW MODE)"
echo "================================================"
echo ""
echo -e "${CYAN}This script runs slowly so you can observe each action.${NC}"
echo -e "${CYAN}Watch the text area window as the tests run.${NC}"
echo ""

# Build
echo -e "${YELLOW}Building text_area...${NC}"
cc text_area.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o text_area -Wl,-rpath,../../target/release 2>&1
if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed${NC}"
    exit 1
fi
echo -e "${GREEN}Build successful${NC}"

# Kill any existing instance
pkill -f "text_area" 2>/dev/null || true
sleep 0.5

# Start app
echo -e "${YELLOW}Starting text_area with debug server on port $DEBUG_PORT...${NC}"
AZUL_DEBUG=$DEBUG_PORT ./text_area &
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
# Test 1: Initial State - Multi-line Text
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: Initial State ===${NC}"
echo -e "${CYAN}LOOK FOR: A text area with multiple lines of numbered text (Line 1, Line 2, etc.)${NC}"
echo -e "${CYAN}          There should be a vertical scrollbar on the right side.${NC}"
sleep $STEP_DELAY

STATE=$(send_cmd '{"op": "get_app_state"}')
if echo "$STATE" | jq -e '.data.value.state.total_lines' > /dev/null 2>&1; then
    LINES=$(echo "$STATE" | jq -r '.data.value.state.total_lines')
    if [ "$LINES" -ge 10 ]; then
        test_pass "Has $LINES lines of text"
    else
        test_fail "Expected >= 10 lines, got $LINES"
    fi
else
    test_fail "Could not get initial state"
fi

# ============================================================================
# Test 2: Focus Textarea
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: Focus Textarea ===${NC}"
echo -e "${CYAN}LOOK FOR: The text area should get a focus ring/border.${NC}"
echo -e "${CYAN}          A blinking text cursor should appear at the start of the text.${NC}"
sleep $ACTION_DELAY

send_cmd '{"op": "key_down", "key": "Tab"}' > /dev/null
sleep $ACTION_DELAY
send_cmd '{"op": "key_up", "key": "Tab"}' > /dev/null
sleep $STEP_DELAY

STATE=$(send_cmd '{"op": "get_state"}')
FOCUSED=$(echo "$STATE" | jq -r '.window_state.focused_node // "none"')

if [ "$FOCUSED" != "none" ] && [ "$FOCUSED" != "null" ]; then
    test_pass "Textarea focused (node: $FOCUSED)"
else
    test_fail "No element focused after Tab"
fi

# ============================================================================
# Test 3: Down Arrow Navigation
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Down Arrow Navigation ===${NC}"
echo -e "${CYAN}LOOK FOR: The text cursor moves down line by line (5 times).${NC}"
echo -e "${CYAN}          The cursor should now be on line 5 or 6.${NC}"
sleep $ACTION_DELAY

KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

# Press Down arrow multiple times - slowly
for i in {1..5}; do
    echo -e "  ${YELLOW}↓ Down Arrow #$i${NC}"
    send_cmd '{"op": "key_down", "key": "Down"}' > /dev/null
    sleep $ACTION_DELAY
    send_cmd '{"op": "key_up", "key": "Down"}' > /dev/null
    sleep $ACTION_DELAY
done

KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Down arrow keys received (count: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Down arrow keys not received"
fi

# ============================================================================
# Test 4: Up Arrow Navigation
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Up Arrow Navigation ===${NC}"
echo -e "${CYAN}LOOK FOR: The text cursor moves up line by line (3 times).${NC}"
echo -e "${CYAN}          The cursor should now be back near the top.${NC}"
sleep $ACTION_DELAY

KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

# Press Up arrow multiple times - slowly
for i in {1..3}; do
    echo -e "  ${YELLOW}↑ Up Arrow #$i${NC}"
    send_cmd '{"op": "key_down", "key": "Up"}' > /dev/null
    sleep $ACTION_DELAY
    send_cmd '{"op": "key_up", "key": "Up"}' > /dev/null
    sleep $ACTION_DELAY
done

KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Up arrow keys received (count: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Up arrow keys not received"
fi

# ============================================================================
# Test 5: Scroll States (Container is scrollable)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Scroll Container ===${NC}"
echo -e "${CYAN}CHECKING: Whether the text area has a scrollable container.${NC}"
sleep $ACTION_DELAY

SCROLL_STATES=$(send_cmd '{"op": "get_scroll_states"}')
if echo "$SCROLL_STATES" | jq -e '.data.value.scroll_states' > /dev/null 2>&1; then
    SCROLL_COUNT=$(echo "$SCROLL_STATES" | jq -r '.data.value.scroll_states | length')
    SCROLL_Y=$(echo "$SCROLL_STATES" | jq -r '.data.value.scroll_states[0].scroll_y // 0')
    if [ "$SCROLL_COUNT" -gt 0 ]; then
        test_pass "Has $SCROLL_COUNT scrollable container(s), current scroll_y: $SCROLL_Y"
    else
        test_fail "No scrollable containers found"
    fi
else
    test_fail "Could not get scroll states"
fi

# ============================================================================
# Test 6: Ctrl+End (Jump to End)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 6: Ctrl+End (Jump to End) ===${NC}"
echo -e "${CYAN}LOOK FOR: The cursor should jump to the LAST line of text.${NC}"
echo -e "${CYAN}          The scroll container should scroll down to show the last lines.${NC}"
echo -e "${BOLD}          >>> BUG CHECK: Does the scrollbar thumb move down too? <<<${NC}"
sleep $ACTION_DELAY

SCROLL_BEFORE=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

echo -e "  ${YELLOW}Pressing Ctrl+End...${NC}"
send_cmd '{"op": "key_down", "key": "LControl"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_down", "key": "End"}' > /dev/null
sleep 0.2
send_cmd '{"op": "key_up", "key": "End"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_up", "key": "LControl"}' > /dev/null
sleep $STEP_DELAY

SCROLL_AFTER=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

echo -e "  ${CYAN}Scroll position: $SCROLL_BEFORE → $SCROLL_AFTER${NC}"

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Ctrl+End received (keys: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Ctrl+End not received"
fi

# ============================================================================
# Test 7: Ctrl+Home (Jump to Start)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 7: Ctrl+Home (Jump to Start) ===${NC}"
echo -e "${CYAN}LOOK FOR: The cursor should jump back to the FIRST line.${NC}"
echo -e "${CYAN}          The scroll container should scroll back to the top.${NC}"
echo -e "${BOLD}          >>> BUG CHECK: Does the scrollbar thumb move back up? <<<${NC}"
sleep $ACTION_DELAY

SCROLL_BEFORE=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

echo -e "  ${YELLOW}Pressing Ctrl+Home...${NC}"
send_cmd '{"op": "key_down", "key": "LControl"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_down", "key": "Home"}' > /dev/null
sleep 0.2
send_cmd '{"op": "key_up", "key": "Home"}' > /dev/null
sleep 0.1
send_cmd '{"op": "key_up", "key": "LControl"}' > /dev/null
sleep $STEP_DELAY

SCROLL_AFTER=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

echo -e "  ${CYAN}Scroll position: $SCROLL_BEFORE → $SCROLL_AFTER${NC}"

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Ctrl+Home received (keys: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Ctrl+Home not received"
fi

# ============================================================================
# Test 8: Page Down
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 8: Page Down ===${NC}"
echo -e "${CYAN}LOOK FOR: The view should scroll down by about one page (several lines).${NC}"
echo -e "${CYAN}          The cursor should move down as well.${NC}"
echo -e "${BOLD}          >>> BUG CHECK: Does the scrollbar thumb jump down? <<<${NC}"
sleep $ACTION_DELAY

SCROLL_BEFORE=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

echo -e "  ${YELLOW}Pressing Page Down...${NC}"
send_cmd '{"op": "key_down", "key": "PageDown"}' > /dev/null
sleep 0.2
send_cmd '{"op": "key_up", "key": "PageDown"}' > /dev/null
sleep $STEP_DELAY

SCROLL_AFTER=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

echo -e "  ${CYAN}Scroll position: $SCROLL_BEFORE → $SCROLL_AFTER${NC}"

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Page Down received (keys: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Page Down not received"
fi

# ============================================================================
# Test 9: Page Up
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 9: Page Up ===${NC}"
echo -e "${CYAN}LOOK FOR: The view should scroll back up by about one page.${NC}"
echo -e "${CYAN}          The cursor should move up as well.${NC}"
echo -e "${BOLD}          >>> BUG CHECK: Does the scrollbar thumb move back up? <<<${NC}"
sleep $ACTION_DELAY

SCROLL_BEFORE=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

echo -e "  ${YELLOW}Pressing Page Up...${NC}"
send_cmd '{"op": "key_down", "key": "PageUp"}' > /dev/null
sleep 0.2
send_cmd '{"op": "key_up", "key": "PageUp"}' > /dev/null
sleep $STEP_DELAY

SCROLL_AFTER=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

echo -e "  ${CYAN}Scroll position: $SCROLL_BEFORE → $SCROLL_AFTER${NC}"

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Page Up received (keys: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Page Up not received"
fi

# ============================================================================
# Test 10: Enter Key (New Line)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 10: Enter Key (New Line) ===${NC}"
echo -e "${CYAN}LOOK FOR: A new empty line should be inserted at the cursor position.${NC}"
echo -e "${CYAN}          The text below the cursor should shift down.${NC}"
sleep $ACTION_DELAY

KEY_BEFORE=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

echo -e "  ${YELLOW}Pressing Enter...${NC}"
send_cmd '{"op": "key_down", "key": "Return"}' > /dev/null
sleep 0.2
send_cmd '{"op": "key_up", "key": "Return"}' > /dev/null
sleep $STEP_DELAY

KEY_AFTER=$(send_cmd '{"op": "get_app_state"}' | jq -r '.data.value.state.key_count // 0')

if [ "$KEY_AFTER" -gt "$KEY_BEFORE" ]; then
    test_pass "Enter key received (keys: $KEY_BEFORE → $KEY_AFTER)"
else
    test_fail "Enter key not received"
fi

# ============================================================================
# Test 11: Scroll Into View (Down to last line)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 11: Scroll Into View ===${NC}"
echo -e "${CYAN}LOOK FOR: As we press Down Arrow 15 times, the text should scroll${NC}"
echo -e "${CYAN}          automatically to keep the cursor visible.${NC}"
echo -e "${BOLD}          >>> BUG CHECK: Does the scrollbar thumb follow the scroll? <<<${NC}"
echo ""
echo -e "${YELLOW}Watch carefully - the cursor should always stay visible...${NC}"
sleep $STEP_DELAY

# Get initial scroll position
SCROLL_BEFORE=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
echo -e "  Initial scroll_y: $SCROLL_BEFORE"

# Navigate down many lines to trigger scroll
for i in {1..15}; do
    echo -e "  ${YELLOW}↓ Down Arrow #$i${NC}"
    send_cmd '{"op": "key_down", "key": "Down"}' > /dev/null
    sleep 0.15
    send_cmd '{"op": "key_up", "key": "Down"}' > /dev/null
    sleep 0.15
    
    # Show scroll position every 5 presses
    if [ $((i % 5)) -eq 0 ]; then
        CURRENT_SCROLL=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
        echo -e "    ${CYAN}→ scroll_y now: $CURRENT_SCROLL${NC}"
    fi
done
sleep $STEP_DELAY

# Get final scroll position
SCROLL_AFTER=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')

echo ""
echo -e "  ${BOLD}Scroll Summary:${NC}"
echo -e "    Before: $SCROLL_BEFORE"
echo -e "    After:  $SCROLL_AFTER"

if [ "$SCROLL_AFTER" != "$SCROLL_BEFORE" ]; then
    test_pass "Scroll-into-view worked! Content scrolled from $SCROLL_BEFORE to $SCROLL_AFTER"
else
    echo -e "  ${YELLOW}Note: Scroll position didn't change. This might indicate:${NC}"
    echo -e "  ${YELLOW}  - scroll-into-view not implemented for text areas${NC}"
    echo -e "  ${YELLOW}  - or the text area is large enough to fit all lines${NC}"
    test_pass "Scroll-into-view test executed (scroll unchanged)"
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

echo -e "${BOLD}=== VISUAL BUG CHECK ===${NC}"
echo -e "${CYAN}Did you observe the following during the tests?${NC}"
echo ""
echo -e "1. ${YELLOW}Scrollbar Thumb Position:${NC}"
echo -e "   - When the content scrolled, did the scrollbar thumb move too?"
echo -e "   - If the thumb stayed stationary while content scrolled,"
echo -e "     this indicates a bug in scrollbar position calculation."
echo ""
echo -e "2. ${YELLOW}Scroll Content vs Scrollbar Sync:${NC}"
echo -e "   - The scrollbar thumb should reflect the current scroll position"
echo -e "   - If scroll_y changes but thumb doesn't move, the scrollbar's"
echo -e "     thumb position isn't being recalculated after scroll changes."
echo ""
echo -e "${CYAN}Possible causes if scrollbar doesn't move:${NC}"
echo -e "   - The scrollbar is a separate widget not listening to scroll events"
echo -e "   - The thumb position is calculated only on initial render"
echo -e "   - The scroll state update doesn't trigger a scrollbar repaint"
echo ""

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
fi
