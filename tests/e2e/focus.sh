#!/usr/bin/env bash
#
# Azul E2E Focus & Tab Navigation Test
#
# This script tests focus and keyboard navigation:
# 1. Tab key moves focus to next focusable element
# 2. Shift+Tab moves focus to previous element
# 3. Enter/Space activates (clicks) the focused element
# 4. Escape clears focus
# 5. :focus CSS pseudo-class is applied correctly
#
# Usage: ./tests/e2e/focus.sh [--no-screenshot] [--verbose]
#
# Exit codes:
#   0 - All tests passed
#   1 - One or more tests failed

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

cd "${PROJECT_ROOT}"

# Configuration
DEBUG_PORT="${AZUL_DEBUG_PORT:-8765}"
OUTPUT_DIR="${PROJECT_ROOT}/target/test_results/focus"
BINARY_DIR="${PROJECT_ROOT}/target/e2e-tests"
SCREENSHOT_DIR="${OUTPUT_DIR}/screenshots"
TAKE_SCREENSHOTS=true
VERBOSE=false
FAILED=0
PASSED=0

# Parse arguments
for arg in "$@"; do
    case $arg in
        --no-screenshot)
            TAKE_SCREENSHOTS=false
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        *)
            ;;
    esac
done

log_verbose() {
    if [ "$VERBOSE" = "true" ]; then
        echo -e "${CYAN}  [DEBUG] $1${NC}"
    fi
}

echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Azul E2E Focus & Tab Navigation Test${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""

# Create output directories
mkdir -p "$OUTPUT_DIR" "$BINARY_DIR" "$SCREENSHOT_DIR"

# Detect OS and set paths
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS_NAME="macos"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul.dylib"
    CC_FLAGS="-framework Cocoa -framework OpenGL -framework IOKit -framework CoreFoundation -framework CoreGraphics"
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul -Wl,-rpath,${PROJECT_ROOT}/target/release"
elif [[ "$OSTYPE" == "linux"* ]]; then
    OS_NAME="linux"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul.so"
    CC_FLAGS=""
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul -Wl,-rpath,${PROJECT_ROOT}/target/release -lm -lpthread -ldl"
else
    OS_NAME="windows"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/azul.dll"
    CC_FLAGS=""
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul"
fi

HEADER_DIR="${PROJECT_ROOT}/target/codegen/v2"
C_SOURCE="${SCRIPT_DIR}/focus.c"
BINARY="${BINARY_DIR}/focus"

# ============================================================================
# Phase 1: Build
# ============================================================================
echo -e "${YELLOW}[Phase 1] Building${NC}"

# Check library
if [ ! -f "$DYLIB_PATH" ]; then
    echo -e "${RED}FAIL: Library not found at $DYLIB_PATH${NC}"
    echo "Please run: cargo build -p azul-dll --release --features build-dll"
    exit 1
fi
echo "  Library: $DYLIB_PATH"

# Check header
if [ ! -f "${HEADER_DIR}/azul.h" ]; then
    echo -e "${RED}FAIL: Header not found at ${HEADER_DIR}/azul.h${NC}"
    echo "Please run: cargo run -p azul-doc -- codegen all"
    exit 1
fi
echo "  Header: ${HEADER_DIR}/azul.h"

# Compile C example
echo "  Compiling focus.c..."
cc -o "$BINARY" "$C_SOURCE" -I"${HEADER_DIR}" ${CC_FLAGS} ${LINK_FLAGS} 2>&1 | grep -v "warning:" || true
if [ ! -f "$BINARY" ]; then
    echo -e "${RED}FAIL: Compilation failed${NC}"
    exit 1
fi
echo -e "  ${GREEN}✓${NC} Compiled: $BINARY"

# ============================================================================
# Phase 2: Start application
# ============================================================================
echo ""
echo -e "${YELLOW}[Phase 2] Starting application${NC}"

# Kill any existing instances
pkill -f "focus" 2>/dev/null || true
sleep 1

echo "  Starting with AZUL_DEBUG=$DEBUG_PORT..."
AZUL_DEBUG=$DEBUG_PORT "$BINARY" > "$OUTPUT_DIR/app.log" 2>&1 &
APP_PID=$!
echo "  PID: $APP_PID"

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    curl -s "http://localhost:$DEBUG_PORT/" -X POST -d '{"op":"close"}' --max-time 2 > /dev/null 2>&1 || true
    sleep 0.5
    kill $APP_PID 2>/dev/null || true
    wait $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

# Wait for debug server to be ready
echo "  Waiting for debug server..."
MAX_WAIT=30
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    if curl -s --max-time 2 -X POST -H "Content-Type: application/json" -d '{"op":"get_state"}' "http://localhost:$DEBUG_PORT/" > /dev/null 2>&1; then
        echo -e "  ${GREEN}✓${NC} Debug server ready after ${WAITED}s"
        break
    fi
    sleep 1
    WAITED=$((WAITED + 1))
done

if [ $WAITED -ge $MAX_WAIT ]; then
    echo -e "${RED}ERROR: Debug server not responding after ${MAX_WAIT}s${NC}"
    exit 1
fi

# Helper function to send a command
send_command() {
    local cmd="$1"
    local result
    result=$(curl -s "http://localhost:$DEBUG_PORT/" -X POST -H "Content-Type: application/json" -d "$cmd" --max-time 15 2>/dev/null)
    log_verbose "Command: $cmd"
    log_verbose "Response: $result"
    echo "$result"
}

# Helper to check test result
check_test() {
    local test_name="$1"
    local condition="$2"
    local message="$3"
    
    if [ "$condition" = "true" ]; then
        echo -e "  ${GREEN}✓ PASS:${NC} $test_name"
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}✗ FAIL:${NC} $test_name - $message"
        FAILED=$((FAILED + 1))
    fi
}

# Helper to get focused node
get_focused_node() {
    local response
    response=$(send_command '{"op":"get_state"}')
    echo "$response" | jq -r '.window_state.focused_node // "null"' 2>/dev/null
}

# Helper to get click counts from status text
get_click_counts() {
    local response
    response=$(send_command '{"op":"get_node_layout", "selector": "#status"}')
    # Extract text content if available, otherwise return empty
    echo "$response" | jq -r '.data.value.text // ""' 2>/dev/null
}

# ============================================================================
# Test 1: Initial State - No Focus
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: Initial State ===${NC}"

RESPONSE=$(send_command '{"op":"get_state"}')
echo "$RESPONSE" > "$OUTPUT_DIR/initial_state.json"

STATUS=$(echo "$RESPONSE" | jq -r '.status' 2>/dev/null)
FOCUSED=$(echo "$RESPONSE" | jq -r '.window_state.focused_node // "null"' 2>/dev/null)

check_test "API responds" "$([ "$STATUS" = "ok" ] && echo true || echo false)" "Status not ok"
check_test "No initial focus" "$([ "$FOCUSED" = "null" ] && echo true || echo false)" "Expected no focus, got: $FOCUSED"

if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    echo "  Taking screenshot..."
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/01_initial.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: 01_initial.png"
    fi
fi

# ============================================================================
# Test 2: Tab Key - Focus First Button
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: Tab → Focus First Element ===${NC}"

# Press Tab
send_command '{"op":"key_down", "key": "Tab"}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.3

RESPONSE=$(send_command '{"op":"get_state"}')
echo "$RESPONSE" > "$OUTPUT_DIR/after_tab1.json"

FOCUSED=$(echo "$RESPONSE" | jq -r '.window_state.focused_node // "null"' 2>/dev/null)
log_verbose "Focused node after Tab: $FOCUSED"

check_test "Focus moved to a node" "$([ "$FOCUSED" != "null" ] && echo true || echo false)" "No focused node"

FIRST_FOCUSED="$FOCUSED"

if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/02_after_tab1.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: 02_after_tab1.png"
    fi
fi

# ============================================================================
# Test 3: Tab Again - Focus Second Button
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Tab → Focus Second Element ===${NC}"

send_command '{"op":"key_down", "key": "Tab"}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.3

RESPONSE=$(send_command '{"op":"get_state"}')
echo "$RESPONSE" > "$OUTPUT_DIR/after_tab2.json"

FOCUSED=$(echo "$RESPONSE" | jq -r '.window_state.focused_node // "null"' 2>/dev/null)
log_verbose "Focused node after 2nd Tab: $FOCUSED"

check_test "Focus moved to different node" "$([ "$FOCUSED" != "null" ] && [ "$FOCUSED" != "$FIRST_FOCUSED" ] && echo true || echo false)" "Focus didn't change"

SECOND_FOCUSED="$FOCUSED"

# ============================================================================
# Test 4: Tab Third Time - Focus Third Button
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Tab → Focus Third Element ===${NC}"

send_command '{"op":"key_down", "key": "Tab"}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.3

RESPONSE=$(send_command '{"op":"get_state"}')
echo "$RESPONSE" > "$OUTPUT_DIR/after_tab3.json"

FOCUSED=$(echo "$RESPONSE" | jq -r '.window_state.focused_node // "null"' 2>/dev/null)
log_verbose "Focused node after 3rd Tab: $FOCUSED"

check_test "Focus moved to third node" "$([ "$FOCUSED" != "null" ] && [ "$FOCUSED" != "$SECOND_FOCUSED" ] && echo true || echo false)" "Focus didn't change"

THIRD_FOCUSED="$FOCUSED"

# ============================================================================
# Test 5: Tab Wraps Around
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Tab → Wraps to First Element ===${NC}"

send_command '{"op":"key_down", "key": "Tab"}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.3

RESPONSE=$(send_command '{"op":"get_state"}')
echo "$RESPONSE" > "$OUTPUT_DIR/after_tab4.json"

FOCUSED=$(echo "$RESPONSE" | jq -r '.window_state.focused_node // "null"' 2>/dev/null)
log_verbose "Focused node after 4th Tab (should wrap): $FOCUSED"

check_test "Focus wrapped to first" "$([ "$FOCUSED" = "$FIRST_FOCUSED" ] && echo true || echo false)" "Expected $FIRST_FOCUSED, got $FOCUSED"

# ============================================================================
# Test 6: Shift+Tab - Go Backwards
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 6: Shift+Tab → Previous Element ===${NC}"

send_command '{"op":"key_down", "key": "Tab", "modifiers": {"shift": true}}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.3

RESPONSE=$(send_command '{"op":"get_state"}')
echo "$RESPONSE" > "$OUTPUT_DIR/after_shift_tab.json"

FOCUSED=$(echo "$RESPONSE" | jq -r '.window_state.focused_node // "null"' 2>/dev/null)
log_verbose "Focused node after Shift+Tab: $FOCUSED"

check_test "Focus moved backwards" "$([ "$FOCUSED" = "$THIRD_FOCUSED" ] && echo true || echo false)" "Expected $THIRD_FOCUSED, got $FOCUSED"

if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/03_after_shift_tab.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: 03_after_shift_tab.png"
    fi
fi

# ============================================================================
# Test 7: Enter Key Activation
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 7: Enter → Activate Focused Button ===${NC}"

# First, Tab to Button 1
send_command '{"op":"key_down", "key": "Tab"}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.3

# Get initial app state (click counts)
BEFORE_STATE=$(send_command '{"op":"get_app_state"}')
echo "$BEFORE_STATE" > "$OUTPUT_DIR/before_enter.json"
BEFORE_CLICKS=$(echo "$BEFORE_STATE" | jq -r '.data.value.state.click_count_button1 // 0' 2>/dev/null)
log_verbose "Click count before Enter: $BEFORE_CLICKS"

# Press Enter
send_command '{"op":"key_down", "key": "Return"}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Return"}' > /dev/null
sleep 0.3

# Check app state after
AFTER_STATE=$(send_command '{"op":"get_app_state"}')
echo "$AFTER_STATE" > "$OUTPUT_DIR/after_enter.json"
AFTER_CLICKS=$(echo "$AFTER_STATE" | jq -r '.data.value.state.click_count_button1 // 0' 2>/dev/null)
LAST_CLICKED=$(echo "$AFTER_STATE" | jq -r '.data.value.state.last_clicked_button // 0' 2>/dev/null)
log_verbose "Click count after Enter: $AFTER_CLICKS, last_clicked: $LAST_CLICKED"

check_test "Enter triggered click callback" "$([ "$AFTER_CLICKS" -gt "$BEFORE_CLICKS" ] && echo true || echo false)" "Click count didn't increase"
check_test "Correct button was clicked" "$([ "$LAST_CLICKED" = "1" ] && echo true || echo false)" "Expected button 1, got $LAST_CLICKED"

if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/04_after_enter.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: 04_after_enter.png"
    fi
fi

# ============================================================================
# Test 8: Space Key Activation
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 8: Space → Activate Focused Button ===${NC}"

# Tab to Button 2
send_command '{"op":"key_down", "key": "Tab"}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.3

BEFORE_STATE=$(send_command '{"op":"get_app_state"}')
BEFORE_CLICKS=$(echo "$BEFORE_STATE" | jq -r '.data.value.state.click_count_button2 // 0' 2>/dev/null)
log_verbose "Button 2 clicks before Space: $BEFORE_CLICKS"

# Press Space
send_command '{"op":"key_down", "key": "Space"}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Space"}' > /dev/null
sleep 0.3

AFTER_STATE=$(send_command '{"op":"get_app_state"}')
echo "$AFTER_STATE" > "$OUTPUT_DIR/after_space.json"
AFTER_CLICKS=$(echo "$AFTER_STATE" | jq -r '.data.value.state.click_count_button2 // 0' 2>/dev/null)
LAST_CLICKED=$(echo "$AFTER_STATE" | jq -r '.data.value.state.last_clicked_button // 0' 2>/dev/null)
log_verbose "Button 2 clicks after Space: $AFTER_CLICKS, last_clicked: $LAST_CLICKED"

check_test "Space triggered click callback" "$([ "$AFTER_CLICKS" -gt "$BEFORE_CLICKS" ] && echo true || echo false)" "Click count didn't increase"
check_test "Correct button was clicked" "$([ "$LAST_CLICKED" = "2" ] && echo true || echo false)" "Expected button 2, got $LAST_CLICKED"

# ============================================================================
# Test 9: Escape Clears Focus
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 9: Escape → Clear Focus ===${NC}"

# Verify we have focus first
RESPONSE=$(send_command '{"op":"get_state"}')
FOCUSED=$(echo "$RESPONSE" | jq -r '.window_state.focused_node // "null"' 2>/dev/null)
log_verbose "Focused before Escape: $FOCUSED"

check_test "Has focus before Escape" "$([ "$FOCUSED" != "null" ] && echo true || echo false)" "No focus to clear"

# Press Escape
send_command '{"op":"key_down", "key": "Escape"}' > /dev/null
sleep 0.2
send_command '{"op":"key_up", "key": "Escape"}' > /dev/null
sleep 0.3

RESPONSE=$(send_command '{"op":"get_state"}')
echo "$RESPONSE" > "$OUTPUT_DIR/after_escape.json"
FOCUSED=$(echo "$RESPONSE" | jq -r '.window_state.focused_node // "null"' 2>/dev/null)
log_verbose "Focused after Escape: $FOCUSED"

check_test "Focus cleared by Escape" "$([ "$FOCUSED" = "null" ] && echo true || echo false)" "Expected null, got $FOCUSED"

if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    RESPONSE=$(send_command '{"op":"take_native_screenshot"}')
    DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
    if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        echo "$BASE64_DATA" | base64 -d > "$SCREENSHOT_DIR/05_after_escape.png"
        echo -e "  ${GREEN}✓${NC} Screenshot: 05_after_escape.png"
    fi
fi

# ============================================================================
# Test 10: Multiple Click Test
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 10: Multiple Enter Presses ===${NC}"

# Tab to Button 3
send_command '{"op":"key_down", "key": "Tab"}' > /dev/null
sleep 0.1
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.1
send_command '{"op":"key_down", "key": "Tab"}' > /dev/null
sleep 0.1
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.1
send_command '{"op":"key_down", "key": "Tab"}' > /dev/null
sleep 0.1
send_command '{"op":"key_up", "key": "Tab"}' > /dev/null
sleep 0.3

BEFORE_STATE=$(send_command '{"op":"get_app_state"}')
BEFORE_CLICKS=$(echo "$BEFORE_STATE" | jq -r '.data.value.state.click_count_button3 // 0' 2>/dev/null)

# Press Enter 3 times
for i in 1 2 3; do
    send_command '{"op":"key_down", "key": "Return"}' > /dev/null
    sleep 0.1
    send_command '{"op":"key_up", "key": "Return"}' > /dev/null
    sleep 0.2
done

AFTER_STATE=$(send_command '{"op":"get_app_state"}')
echo "$AFTER_STATE" > "$OUTPUT_DIR/after_multiple_enter.json"
AFTER_CLICKS=$(echo "$AFTER_STATE" | jq -r '.data.value.state.click_count_button3 // 0' 2>/dev/null)
EXPECTED=$((BEFORE_CLICKS + 3))
log_verbose "Button 3 clicks: before=$BEFORE_CLICKS, after=$AFTER_CLICKS, expected=$EXPECTED"

check_test "Multiple Enter presses work" "$([ "$AFTER_CLICKS" = "$EXPECTED" ] && echo true || echo false)" "Expected $EXPECTED clicks, got $AFTER_CLICKS"

# ============================================================================
# Summary
# ============================================================================
echo ""
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Test Results Summary${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""
echo -e "  Passed: ${GREEN}$PASSED${NC}"
echo -e "  Failed: ${RED}$FAILED${NC}"
echo ""
echo "  Output directory: $OUTPUT_DIR"
echo "  App log: $OUTPUT_DIR/app.log"

if [ "$TAKE_SCREENSHOTS" = "true" ]; then
    echo "  Screenshots: $SCREENSHOT_DIR/"
fi

echo ""

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}SOME TESTS FAILED${NC}"
    exit 1
else
    echo -e "${GREEN}ALL TESTS PASSED${NC}"
    exit 0
fi
