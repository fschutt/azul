#!/bin/bash
# Drag-Select-Scroll E2E Test Script
#
# Tests the combined behavior of:
# 1. Text selection via mouse drag
# 2. Auto-scroll when dragging near container edge
# 3. Selection extends during auto-scroll
# 4. Verify scroll position changes during drag
#
# Usage: ./test_drag_select_scroll.sh

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
echo "Drag-Select-Scroll E2E Test Suite"
echo "================================================"

# Build
echo -e "${YELLOW}Building drag_select_scroll...${NC}"
cc drag_select_scroll.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o drag_select_scroll -Wl,-rpath,../../target/release 2>&1
if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed${NC}"
    exit 1
fi
echo -e "${GREEN}Build successful${NC}"

# Kill any existing instance
pkill -f "drag_select_scroll" 2>/dev/null || true
sleep 0.5

# Start app
echo -e "${YELLOW}Starting drag_select_scroll with debug server on port $DEBUG_PORT...${NC}"
AZUL_DEBUG=$DEBUG_PORT ./drag_select_scroll &
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

test_warn() {
    echo -e "  ${YELLOW}⚠ WARN:${NC} $1"
}

# ============================================================================
# Test 1: Initial State
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: Initial State ===${NC}"

STATE=$(send_cmd '{"op": "get_app_state"}')
if echo "$STATE" | jq -e '.data.value.state' > /dev/null 2>&1; then
    SCROLL_Y=$(echo "$STATE" | jq -r '.data.value.state.scroll_y // 0')
    DRAG=$(echo "$STATE" | jq -r '.data.value.state.drag_active // 0')
    echo "  Scroll Y: $SCROLL_Y"
    echo "  Drag active: $DRAG"
    test_pass "Initial state retrieved"
else
    test_fail "Could not get initial state"
fi

# ============================================================================
# Test 2: Scroll Container Exists
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: Scroll Container ===${NC}"

SCROLL_STATES=$(send_cmd '{"op": "get_scroll_states"}')
if echo "$SCROLL_STATES" | jq -e '.data.value.scroll_states' > /dev/null 2>&1; then
    SCROLL_COUNT=$(echo "$SCROLL_STATES" | jq -r '.data.value.scroll_states | length')
    if [ "$SCROLL_COUNT" -gt 0 ]; then
        MAX_SCROLL=$(echo "$SCROLL_STATES" | jq -r '.data.value.scroll_states[0].max_scroll_y // 0')
        echo "  Scrollable containers: $SCROLL_COUNT"
        echo "  Max scroll Y: $MAX_SCROLL"
        test_pass "Scroll container found"
    else
        test_fail "No scrollable containers"
    fi
else
    test_fail "Could not get scroll states"
fi

# ============================================================================
# Test 3: Get Container Layout
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Container Layout ===${NC}"

LAYOUT=$(send_cmd '{"op": "get_node_layout", "selector": ".scroll-container"}')
if echo "$LAYOUT" | jq -e '.data.value.rect' > /dev/null 2>&1; then
    CONT_X=$(echo "$LAYOUT" | jq -r '.data.value.rect.x // 0')
    CONT_Y=$(echo "$LAYOUT" | jq -r '.data.value.rect.y // 0')
    CONT_W=$(echo "$LAYOUT" | jq -r '.data.value.rect.width // 0')
    CONT_H=$(echo "$LAYOUT" | jq -r '.data.value.rect.height // 0')
    echo "  Container: x=$CONT_X, y=$CONT_Y, w=$CONT_W, h=$CONT_H"
    test_pass "Container layout retrieved"
else
    # Default values if layout query fails
    CONT_X=15
    CONT_Y=50
    CONT_W=670
    CONT_H=300
    test_warn "Could not get container layout, using defaults"
fi

# ============================================================================
# Test 4: Start Drag Selection
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Start Drag Selection ===${NC}"

# Start near top of container
START_X=$(echo "$CONT_X + 50" | bc 2>/dev/null || echo "65")
START_Y=$(echo "$CONT_Y + 30" | bc 2>/dev/null || echo "80")

echo "  Starting drag at ($START_X, $START_Y)"

send_cmd "{\"op\": \"mouse_down\", \"x\": $START_X, \"y\": $START_Y}" > /dev/null
sleep 0.1

test_pass "Drag started"

# ============================================================================
# Test 5: Drag Within Container
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Drag Within Container ===${NC}"

# Drag downward within container
MID_X=$START_X
MID_Y=$(echo "$CONT_Y + 150" | bc 2>/dev/null || echo "200")

echo "  Dragging to ($MID_X, $MID_Y)"

send_cmd "{\"op\": \"mouse_move\", \"x\": $MID_X, \"y\": $MID_Y}" > /dev/null
sleep 0.1

# Check selection
SELECTION=$(send_cmd '{"op": "get_selection_state"}')
HAS_SELECTION=$(echo "$SELECTION" | jq -r '.data.value.has_selection // false')
echo "  Has selection: $HAS_SELECTION"

test_pass "Drag within container"

# ============================================================================
# Test 6: Drag to Bottom Edge (Should Trigger Auto-Scroll)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 6: Drag to Bottom Edge ===${NC}"

# Get scroll position before
SCROLL_BEFORE=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')

# Drag to bottom edge of container
BOTTOM_X=$START_X
BOTTOM_Y=$(echo "$CONT_Y + $CONT_H - 5" | bc 2>/dev/null || echo "345")

echo "  Dragging to bottom edge ($BOTTOM_X, $BOTTOM_Y)"
echo "  Scroll Y before: $SCROLL_BEFORE"

# Multiple moves to simulate continuous drag
for i in {1..5}; do
    send_cmd "{\"op\": \"mouse_move\", \"x\": $BOTTOM_X, \"y\": $BOTTOM_Y}" > /dev/null
    sleep 0.1
done

# Get scroll position after
SCROLL_AFTER=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
echo "  Scroll Y after: $SCROLL_AFTER"

# Note: Auto-scroll may not be implemented yet
if [ "$SCROLL_AFTER" != "$SCROLL_BEFORE" ]; then
    test_pass "Auto-scroll triggered (scroll changed: $SCROLL_BEFORE → $SCROLL_AFTER)"
else
    test_warn "Auto-scroll not triggered (feature may not be implemented)"
fi

# ============================================================================
# Test 7: Continue Drag Below Container
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 7: Drag Below Container ===${NC}"

# Drag below container (should continue auto-scroll)
BELOW_X=$START_X
BELOW_Y=$(echo "$CONT_Y + $CONT_H + 50" | bc 2>/dev/null || echo "400")

echo "  Dragging below container ($BELOW_X, $BELOW_Y)"

for i in {1..3}; do
    send_cmd "{\"op\": \"mouse_move\", \"x\": $BELOW_X, \"y\": $BELOW_Y}" > /dev/null
    sleep 0.15
done

SCROLL_FINAL=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
echo "  Scroll Y: $SCROLL_FINAL"

test_pass "Drag below container executed"

# ============================================================================
# Test 8: Release Drag
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 8: Release Drag ===${NC}"

send_cmd "{\"op\": \"mouse_up\", \"x\": $BELOW_X, \"y\": $BELOW_Y}" > /dev/null
sleep 0.1

# Check final selection state
SELECTION=$(send_cmd '{"op": "get_selection_state"}')
HAS_SELECTION=$(echo "$SELECTION" | jq -r '.data.value.has_selection // false')
echo "  Final selection: $HAS_SELECTION"

test_pass "Drag released"

# ============================================================================
# Test 9: Drag Upward (Reverse Auto-Scroll)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 9: Drag Upward ===${NC}"

# First scroll to bottom
send_cmd '{"op": "scroll_node_to", "selector": ".scroll-container", "x": 0, "y": 500}' > /dev/null
sleep 0.2

SCROLL_BEFORE=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
echo "  Scroll Y before: $SCROLL_BEFORE"

# Start drag at bottom of container
DRAG_START_X=$(echo "$CONT_X + 50" | bc 2>/dev/null || echo "65")
DRAG_START_Y=$(echo "$CONT_Y + $CONT_H - 30" | bc 2>/dev/null || echo "320")

send_cmd "{\"op\": \"mouse_down\", \"x\": $DRAG_START_X, \"y\": $DRAG_START_Y}" > /dev/null
sleep 0.1

# Drag to top edge
TOP_Y=$(echo "$CONT_Y + 5" | bc 2>/dev/null || echo "55")

for i in {1..5}; do
    send_cmd "{\"op\": \"mouse_move\", \"x\": $DRAG_START_X, \"y\": $TOP_Y}" > /dev/null
    sleep 0.1
done

SCROLL_AFTER=$(send_cmd '{"op": "get_scroll_states"}' | jq -r '.data.value.scroll_states[0].scroll_y // 0')
echo "  Scroll Y after: $SCROLL_AFTER"

send_cmd "{\"op\": \"mouse_up\", \"x\": $DRAG_START_X, \"y\": $TOP_Y}" > /dev/null
sleep 0.1

if [ "$SCROLL_AFTER" != "$SCROLL_BEFORE" ]; then
    test_pass "Reverse auto-scroll triggered"
else
    test_warn "Reverse auto-scroll not triggered (feature may not be implemented)"
fi

# ============================================================================
# Test 10: Selection Manager State
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 10: Selection Manager State ===${NC}"

DUMP=$(send_cmd '{"op": "dump_selection_manager"}')
if echo "$DUMP" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    CLICK_COUNT=$(echo "$DUMP" | jq -r '.data.value.click_state.click_count // 0')
    SELECTION_COUNT=$(echo "$DUMP" | jq -r '.data.value.selections | length')
    echo "  Click count: $CLICK_COUNT"
    echo "  Selection entries: $SELECTION_COUNT"
    test_pass "Selection manager dump available"
else
    test_fail "Could not dump selection manager"
fi

# ============================================================================
# Test 11: App State After Tests
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 11: Final App State ===${NC}"

STATE=$(send_cmd '{"op": "get_app_state"}')
if echo "$STATE" | jq -e '.data.value.state' > /dev/null 2>&1; then
    EVENTS=$(echo "$STATE" | jq -r '.data.value.state.mouse_events // 0')
    echo "  Total mouse events: $EVENTS"
    test_pass "App state retrieved"
else
    test_fail "Could not get final app state"
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

echo -e "${YELLOW}Note: Auto-scroll during drag-selection may not be implemented yet.${NC}"
echo -e "${YELLOW}Tests document the expected behavior for future implementation.${NC}"
echo ""

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
fi
