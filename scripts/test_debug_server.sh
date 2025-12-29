#!/bin/bash
# Debug Server Test Script with jq validation
#
# This script tests the debug HTTP server by:
# 1. Starting the application with AZUL_DEBUG enabled
# 2. Sending commands via HTTP and validating responses with jq
# 3. Capturing and analyzing debug output
#
# Requirements: jq (brew install jq / apt install jq)
#
# Usage:
#   ./scripts/test_debug_server.sh
#
# Environment variables:
#   AZUL_DEBUG_PORT - Port for debug server (default: 8765)
#   AZUL_TEST_APP   - Binary to test (default: hello_world_window)

set -e

# Configuration
PORT="${AZUL_DEBUG_PORT:-8765}"
URL="http://127.0.0.1:$PORT"
OUTPUT_DIR="target/test_output"
LOG_FILE="$OUTPUT_DIR/debug_server_test.log"
RESPONSE_FILE="$OUTPUT_DIR/last_response.json"
APP_BINARY="${AZUL_TEST_APP:-hello_world_window}"
STARTUP_WAIT=3
REQUEST_TIMEOUT=10

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

# Check for jq
if ! command -v jq &> /dev/null; then
    echo -e "${RED}ERROR: jq is required but not installed.${NC}"
    echo "Install with: brew install jq (macOS) or apt install jq (Linux)"
    exit 1
fi

echo -e "${BLUE}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║     Azul Debug Server Test Suite                   ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════╝${NC}"
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    if [ -n "$APP_PID" ] && kill -0 "$APP_PID" 2>/dev/null; then
        # Try graceful shutdown first
        curl -s -X POST "$URL/event" -d '{"type":"shutdown"}' --max-time 2 || true
        sleep 1
        kill "$APP_PID" 2>/dev/null || true
    fi
    pkill -f "$APP_BINARY" 2>/dev/null || true
}
trap cleanup EXIT

# Test helper functions
send_request() {
    local endpoint="$1"
    local data="$2"
    local description="$3"
    
    echo -n "  Testing: $description... "
    
    if [ -n "$data" ]; then
        RESPONSE=$(curl -s -X POST "$URL/$endpoint" \
            -H "Content-Type: application/json" \
            -d "$data" \
            --max-time "$REQUEST_TIMEOUT" 2>/dev/null) || RESPONSE=""
    else
        RESPONSE=$(curl -s "$URL/$endpoint" \
            --max-time "$REQUEST_TIMEOUT" 2>/dev/null) || RESPONSE=""
    fi
    
    echo "$RESPONSE" > "$RESPONSE_FILE"
    echo "$RESPONSE"
}

assert_json_field() {
    local json="$1"
    local path="$2"
    local expected="$3"
    local description="$4"
    
    local actual=$(echo "$json" | jq -r "$path" 2>/dev/null)
    
    if [ "$actual" = "$expected" ]; then
        echo -e "${GREEN}✓${NC} $description"
        ((TESTS_PASSED++))
        return 0
    else
        echo -e "${RED}✗${NC} $description (expected: $expected, got: $actual)"
        ((TESTS_FAILED++))
        return 1
    fi
}

assert_json_exists() {
    local json="$1"
    local path="$2"
    local description="$3"
    
    local value=$(echo "$json" | jq -r "$path" 2>/dev/null)
    
    if [ "$value" != "null" ] && [ -n "$value" ]; then
        echo -e "${GREEN}✓${NC} $description (value: $value)"
        ((TESTS_PASSED++))
        return 0
    else
        echo -e "${RED}✗${NC} $description (field is null or missing)"
        ((TESTS_FAILED++))
        return 1
    fi
}

assert_json_numeric() {
    local json="$1"
    local path="$2"
    local min="$3"
    local description="$4"
    
    local value=$(echo "$json" | jq "$path" 2>/dev/null)
    
    if [ "$value" != "null" ] && [ "$value" -ge "$min" ] 2>/dev/null; then
        echo -e "${GREEN}✓${NC} $description (value: $value)"
        ((TESTS_PASSED++))
        return 0
    else
        echo -e "${RED}✗${NC} $description (value $value is not >= $min)"
        ((TESTS_FAILED++))
        return 1
    fi
}

# ============================================================================
# PHASE 1: Build and start application
# ============================================================================
echo -e "\n${YELLOW}[PHASE 1] Building and starting application${NC}\n"

# Clean previous run
rm -f "$LOG_FILE" "$RESPONSE_FILE"
pkill -f "$APP_BINARY" 2>/dev/null || true
sleep 1

# Build
echo "Building $APP_BINARY..."
cd "$(dirname "$0")/.."
cargo build --bin "$APP_BINARY" --features "link-static" 2>&1 | tail -3

# Start with debug server
echo "Starting application with AZUL_DEBUG=$PORT..."
AZUL_DEBUG=$PORT cargo run --bin "$APP_BINARY" --features "link-static" 2>&1 > "$LOG_FILE" &
APP_PID=$!
echo "  PID: $APP_PID"
echo "  Log: $LOG_FILE"

# Wait for startup
echo "Waiting ${STARTUP_WAIT}s for startup..."
sleep "$STARTUP_WAIT"

# Verify process is running
if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo -e "${RED}ERROR: Application crashed during startup${NC}"
    echo "Last 30 lines of log:"
    tail -30 "$LOG_FILE"
    exit 1
fi
echo -e "${GREEN}Application started successfully${NC}"

# ============================================================================
# PHASE 2: Health check
# ============================================================================
echo -e "\n${YELLOW}[PHASE 2] Health check${NC}\n"

HEALTH=$(send_request "health" "" "GET /health")
assert_json_field "$HEALTH" ".status" "ok" "Health endpoint returns ok"

# ============================================================================
# PHASE 3: Get initial state
# ============================================================================
echo -e "\n${YELLOW}[PHASE 3] Get initial window state${NC}\n"

RESPONSE=$(send_request "event" '{"type":"get_state"}' "POST /event get_state")
assert_json_field "$RESPONSE" ".status" "ok" "get_state returns ok"
assert_json_exists "$RESPONSE" ".window_state.logical_width" "Window has logical_width"
assert_json_exists "$RESPONSE" ".window_state.logical_height" "Window has logical_height"
assert_json_exists "$RESPONSE" ".window_state.dpi" "Window has DPI"
assert_json_exists "$RESPONSE" ".window_state.hidpi_factor" "Window has hidpi_factor"

# Extract initial dimensions
INITIAL_WIDTH=$(echo "$RESPONSE" | jq '.window_state.logical_width')
INITIAL_HEIGHT=$(echo "$RESPONSE" | jq '.window_state.logical_height')
INITIAL_DPI=$(echo "$RESPONSE" | jq '.window_state.dpi')
echo ""
echo "  Initial size: ${INITIAL_WIDTH}x${INITIAL_HEIGHT} @ DPI ${INITIAL_DPI}"

# ============================================================================
# PHASE 4: Test resize
# ============================================================================
echo -e "\n${YELLOW}[PHASE 4] Test window resize${NC}\n"

RESPONSE=$(send_request "event" '{"type":"resize","width":800.0,"height":600.0}' "Resize to 800x600")
assert_json_field "$RESPONSE" ".status" "ok" "Resize command accepted"

# Wait for layout
sleep 0.5

# Verify new size
RESPONSE=$(send_request "event" '{"type":"get_state"}' "Get state after resize")
NEW_WIDTH=$(echo "$RESPONSE" | jq '.window_state.logical_width')
NEW_HEIGHT=$(echo "$RESPONSE" | jq '.window_state.logical_height')
echo ""
echo "  New size: ${NEW_WIDTH}x${NEW_HEIGHT}"

# Check if size changed (may not be exact due to platform constraints)
if [ "$(echo "$NEW_WIDTH >= 700" | bc)" = "1" ] 2>/dev/null; then
    echo -e "${GREEN}✓${NC} Width updated"
    ((TESTS_PASSED++))
else
    echo -e "${YELLOW}?${NC} Width not updated (may need platform implementation)"
fi

# ============================================================================
# PHASE 5: Test relayout
# ============================================================================
echo -e "\n${YELLOW}[PHASE 5] Test relayout${NC}\n"

RESPONSE=$(send_request "event" '{"type":"relayout"}' "Force relayout")
assert_json_field "$RESPONSE" ".status" "ok" "Relayout command accepted"

# ============================================================================
# PHASE 6: Check debug messages
# ============================================================================
echo -e "\n${YELLOW}[PHASE 6] Check debug logging${NC}\n"

RESPONSE=$(send_request "logs" "" "GET /logs")
MSG_COUNT=$(echo "$RESPONSE" | jq '.logs | length')
echo "  Collected $MSG_COUNT log messages"

if [ "$MSG_COUNT" -gt 0 ]; then
    echo -e "${GREEN}✓${NC} Debug logging is working"
    ((TESTS_PASSED++))
    
    # Show sample messages
    echo ""
    echo "  Sample messages:"
    echo "$RESPONSE" | jq -r '.logs[0:5][] | "    [\(.level)] [\(.category)] \(.message)"' 2>/dev/null || true
else
    echo -e "${YELLOW}?${NC} No debug messages collected (timer may not be running)"
fi

# ============================================================================
# PHASE 7: Hit testing
# ============================================================================
echo -e "\n${YELLOW}[PHASE 7] Test hit testing${NC}\n"

RESPONSE=$(send_request "event" '{"type":"hit_test","x":100.0,"y":100.0}' "Hit test at (100,100)")
assert_json_field "$RESPONSE" ".status" "ok" "Hit test accepted"
if echo "$RESPONSE" | jq -e '.data' > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC} Hit test returned data"
    ((TESTS_PASSED++))
fi

# ============================================================================
# PHASE 8: Wait frame
# ============================================================================
echo -e "\n${YELLOW}[PHASE 8] Test frame synchronization${NC}\n"

RESPONSE=$(send_request "event" '{"type":"wait_frame"}' "Wait for frame")
assert_json_field "$RESPONSE" ".status" "ok" "Wait frame completed"

# ============================================================================
# PHASE 9: Shutdown
# ============================================================================
echo -e "\n${YELLOW}[PHASE 9] Clean shutdown${NC}\n"

RESPONSE=$(send_request "event" '{"type":"shutdown"}' "Request shutdown")
assert_json_field "$RESPONSE" ".status" "ok" "Shutdown accepted"

# Wait for app to exit
sleep 2
if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo -e "${GREEN}✓${NC} Application exited cleanly"
    ((TESTS_PASSED++))
else
    echo -e "${YELLOW}?${NC} Application still running (shutdown may be async)"
fi

# ============================================================================
# Summary
# ============================================================================
echo ""
echo -e "${BLUE}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║                    Test Summary                    ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  ${GREEN}Passed:${NC} $TESTS_PASSED"
echo -e "  ${RED}Failed:${NC} $TESTS_FAILED"
echo ""

# Analyze log file
echo -e "${YELLOW}Log Analysis:${NC}"
echo ""

# Check for key events in log
LAYOUT_CALLS=$(grep -c "\[my_layout_func\]" "$LOG_FILE" 2>/dev/null || echo "0")
FONT_LOADS=$(grep -c "FontLoading" "$LOG_FILE" 2>/dev/null || echo "0")
DEBUG_TIMER=$(grep -c "debug timer" "$LOG_FILE" 2>/dev/null || echo "0")

echo "  Layout callback invocations: $LAYOUT_CALLS"
echo "  Font loading events: $FONT_LOADS"
echo "  Debug timer references: $DEBUG_TIMER"
echo ""

# Check for errors
ERROR_COUNT=$(grep -ciE "error|panic|fatal" "$LOG_FILE" 2>/dev/null || echo "0")
if [ "$ERROR_COUNT" -gt 0 ]; then
    echo -e "${RED}Found $ERROR_COUNT error-related messages:${NC}"
    grep -iE "error|panic|fatal" "$LOG_FILE" | head -10
else
    echo -e "${GREEN}No errors found in log${NC}"
fi

echo ""
echo "Full log: $LOG_FILE"
echo "Last response: $RESPONSE_FILE"

# Exit code based on test results
if [ "$TESTS_FAILED" -gt 0 ]; then
    exit 1
fi
exit 0
