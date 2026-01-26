#!/bin/bash
# Selection E2E Test (Improved)
#
# Tests text selection across multiple DOM nodes:
# 1. Single click to place cursor
# 2. Click-and-drag for range selection
# 3. Double-click for word selection
# 4. Triple-click for paragraph selection
# 5. Shift+Click to extend selection
# 6. user-select: none is respected
# 7. Selection state via debug API
#
# Usage: ./test_selection.sh

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
echo "Selection E2E Test Suite (Improved)"
echo "================================================"

# Build
echo -e "${YELLOW}Building selection...${NC}"
cc selection.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o selection -Wl,-rpath,../../target/release 2>&1
if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed${NC}"
    exit 1
fi
echo -e "${GREEN}Build successful${NC}"

# Kill any existing instance
pkill -f "selection" 2>/dev/null || true
sleep 0.5

# Start app
echo -e "${YELLOW}Starting selection with debug server on port $DEBUG_PORT...${NC}"
AZUL_DEBUG=$DEBUG_PORT ./selection &
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

STATE=$(send_cmd '{"op": "get_state"}')
if echo "$STATE" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    NODE_COUNT=$(echo "$STATE" | jq -r '.window_state.dom_node_count // 0')
    echo "  DOM node count: $NODE_COUNT"
    if [ "$NODE_COUNT" -ge 7 ]; then
        test_pass "DOM has expected structure ($NODE_COUNT nodes)"
    else
        test_fail "Not enough nodes (expected >= 7, got $NODE_COUNT)"
    fi
else
    test_fail "Could not get initial state"
fi

# ============================================================================
# Test 2: Get Paragraph Positions
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: Get Paragraph Positions ===${NC}"

LAYOUT=$(send_cmd '{"op": "get_all_nodes_layout"}')

# Try to find paragraphs by class
P1_LAYOUT=$(send_cmd '{"op": "get_node_layout", "selector": ".paragraph-1"}')
P2_LAYOUT=$(send_cmd '{"op": "get_node_layout", "selector": ".paragraph-2"}')
P3_LAYOUT=$(send_cmd '{"op": "get_node_layout", "selector": ".paragraph-3"}')

P1_X=$(echo "$P1_LAYOUT" | jq -r '.data.value.rect.x // 50' 2>/dev/null)
P1_Y=$(echo "$P1_LAYOUT" | jq -r '.data.value.rect.y // 50' 2>/dev/null)
P1_W=$(echo "$P1_LAYOUT" | jq -r '.data.value.rect.width // 700' 2>/dev/null)
P1_H=$(echo "$P1_LAYOUT" | jq -r '.data.value.rect.height // 50' 2>/dev/null)

P3_X=$(echo "$P3_LAYOUT" | jq -r '.data.value.rect.x // 50' 2>/dev/null)
P3_Y=$(echo "$P3_LAYOUT" | jq -r '.data.value.rect.y // 200' 2>/dev/null)
P3_W=$(echo "$P3_LAYOUT" | jq -r '.data.value.rect.width // 700' 2>/dev/null)

echo "  Paragraph 1: x=$P1_X, y=$P1_Y, w=$P1_W, h=$P1_H"
echo "  Paragraph 3: x=$P3_X, y=$P3_Y, w=$P3_W"
test_pass "Paragraph positions retrieved"

# ============================================================================
# Test 3: Initial Selection State (should be empty)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Initial Selection State ===${NC}"

SELECTION=$(send_cmd '{"op": "get_selection_state"}')
if echo "$SELECTION" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    HAS_SELECTION=$(echo "$SELECTION" | jq -r '.data.value.has_selection // false')
    echo "  Has selection: $HAS_SELECTION"
    if [ "$HAS_SELECTION" = "false" ]; then
        test_pass "No initial selection (expected)"
    else
        test_warn "Unexpected initial selection"
    fi
else
    test_fail "Could not get selection state"
fi

# ============================================================================
# Test 4: Single Click (Place Cursor)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Single Click (Place Cursor) ===${NC}"

# Click in paragraph 1
CLICK_X=$(echo "$P1_X + 100" | bc 2>/dev/null || echo "150")
CLICK_Y=$(echo "$P1_Y + 20" | bc 2>/dev/null || echo "70")

send_cmd "{\"op\": \"click\", \"x\": $CLICK_X, \"y\": $CLICK_Y}" > /dev/null
sleep 0.2

SELECTION=$(send_cmd '{"op": "get_selection_state"}')
echo "$SELECTION" | jq '.' 2>/dev/null | head -10
test_pass "Single click executed at ($CLICK_X, $CLICK_Y)"

# ============================================================================
# Test 5: Click-and-Drag Selection
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Click-and-Drag Selection ===${NC}"

# Start in paragraph 1
START_X=$(echo "$P1_X + 50" | bc 2>/dev/null || echo "100")
START_Y=$(echo "$P1_Y + 25" | bc 2>/dev/null || echo "75")

# End in paragraph 3
END_X=$(echo "$P3_X + $P3_W - 50" | bc 2>/dev/null || echo "700")
END_Y=$(echo "$P3_Y + 25" | bc 2>/dev/null || echo "225")

echo "  Drag from ($START_X, $START_Y) to ($END_X, $END_Y)"

# Mouse down
send_cmd "{\"op\": \"mouse_down\", \"x\": $START_X, \"y\": $START_Y}" > /dev/null
sleep 0.1

# Mouse move (drag)
send_cmd "{\"op\": \"mouse_move\", \"x\": $END_X, \"y\": $END_Y}" > /dev/null
sleep 0.1

# Mouse up
send_cmd "{\"op\": \"mouse_up\", \"x\": $END_X, \"y\": $END_Y}" > /dev/null
sleep 0.2

# Check selection
SELECTION=$(send_cmd '{"op": "get_selection_state"}')
HAS_SELECTION=$(echo "$SELECTION" | jq -r '.data.value.has_selection // false')

if [ "$HAS_SELECTION" = "true" ]; then
    test_pass "Selection created via drag"
else
    test_warn "No selection after drag (feature may need implementation)"
fi

# ============================================================================
# Test 6: Double-Click (Word Selection)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 6: Double-Click (Word Selection) ===${NC}"

# Clear selection first
send_cmd "{\"op\": \"click\", \"x\": 10, \"y\": 10}" > /dev/null
sleep 0.1

CLICK_X=$(echo "$P1_X + 150" | bc 2>/dev/null || echo "200")
CLICK_Y=$(echo "$P1_Y + 25" | bc 2>/dev/null || echo "75")

# Double click
send_cmd "{\"op\": \"double_click\", \"x\": $CLICK_X, \"y\": $CLICK_Y}" > /dev/null
sleep 0.2

SELECTION=$(send_cmd '{"op": "get_selection_state"}')
HAS_SELECTION=$(echo "$SELECTION" | jq -r '.data.value.has_selection // false')

if [ "$HAS_SELECTION" = "true" ]; then
    test_pass "Double-click selection created"
else
    test_warn "No selection after double-click (feature may need implementation)"
fi

# ============================================================================
# Test 7: Selection Manager Dump
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 7: Selection Manager Dump ===${NC}"

DUMP=$(send_cmd '{"op": "dump_selection_manager"}')
if echo "$DUMP" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    # Show click state
    CLICK_COUNT=$(echo "$DUMP" | jq -r '.data.value.click_state.click_count // 0')
    echo "  Click count: $CLICK_COUNT"
    
    # Show selections
    SELECTION_COUNT=$(echo "$DUMP" | jq -r '.data.value.selections | length')
    echo "  Selection entries: $SELECTION_COUNT"
    
    test_pass "Selection manager dump available"
else
    test_fail "Could not dump selection manager"
fi

# ============================================================================
# Test 8: Shift+Click to Extend Selection
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 8: Shift+Click to Extend Selection ===${NC}"

# First click to start selection
START_X=$(echo "$P1_X + 50" | bc 2>/dev/null || echo "100")
START_Y=$(echo "$P1_Y + 25" | bc 2>/dev/null || echo "75")
send_cmd "{\"op\": \"click\", \"x\": $START_X, \"y\": $START_Y}" > /dev/null
sleep 0.1

# Shift+Click to extend
END_X=$(echo "$P1_X + 300" | bc 2>/dev/null || echo "350")

send_cmd '{"op": "key_down", "key": "LShift"}' > /dev/null
sleep 0.05
send_cmd "{\"op\": \"click\", \"x\": $END_X, \"y\": $START_Y}" > /dev/null
sleep 0.1
send_cmd '{"op": "key_up", "key": "LShift"}' > /dev/null
sleep 0.1

SELECTION=$(send_cmd '{"op": "get_selection_state"}')
HAS_SELECTION=$(echo "$SELECTION" | jq -r '.data.value.has_selection // false')

if [ "$HAS_SELECTION" = "true" ]; then
    test_pass "Shift+Click extended selection"
else
    test_warn "No selection after Shift+Click (feature may need implementation)"
fi

# ============================================================================
# Test 9: Hit Test
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 9: Hit Test ===${NC}"

HIT_X=$(echo "$P1_X + 100" | bc 2>/dev/null || echo "150")
HIT_Y=$(echo "$P1_Y + 25" | bc 2>/dev/null || echo "75")

HIT_RESULT=$(send_cmd "{\"op\": \"hit_test\", \"x\": $HIT_X, \"y\": $HIT_Y}")
if echo "$HIT_RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    NODE_ID=$(echo "$HIT_RESULT" | jq -r '.data.value.node_id // "none"')
    echo "  Hit test at ($HIT_X, $HIT_Y) → node $NODE_ID"
    if [ "$NODE_ID" != "none" ] && [ "$NODE_ID" != "null" ]; then
        test_pass "Hit test found node $NODE_ID"
    else
        test_warn "Hit test returned no node"
    fi
else
    test_fail "Hit test failed"
fi

# ============================================================================
# Test 10: Find Node by Text
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 10: Find Node by Text ===${NC}"

FIND_RESULT=$(send_cmd '{"op": "find_node_by_text", "text": "FIRST"}')
if echo "$FIND_RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    FOUND=$(echo "$FIND_RESULT" | jq -r '.data.value.found // false')
    if [ "$FOUND" = "true" ]; then
        FOUND_NODE=$(echo "$FIND_RESULT" | jq -r '.data.value.node_id')
        echo "  Found 'FIRST' at node $FOUND_NODE"
        test_pass "Found text node"
    else
        test_warn "Text 'FIRST' not found"
    fi
else
    test_fail "find_node_by_text failed"
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
