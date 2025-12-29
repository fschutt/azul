#!/bin/bash
# HiDPI Remote Control Test Script
#
# This script tests the HiDPI rendering pipeline by:
# 1. Starting the application with remote control enabled
# 2. Sending commands via HTTP to inspect state
# 3. Capturing debug output to a file for analysis
#
# Usage:
#   ./scripts/test_hidpi_remote.sh
#
# The output will be written to target/test_output/hidpi_test.log

set -e

# Configuration
PORT=8765
URL="http://localhost:$PORT"
OUTPUT_DIR="target/test_output"
LOG_FILE="$OUTPUT_DIR/hidpi_test.log"
TIMEOUT=30

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Azul HiDPI Remote Control Test ===${NC}"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Clean up any previous test
echo "[1/7] Cleaning up previous test artifacts..."
rm -f "$LOG_FILE"
pkill -f "hello_world_window" 2>/dev/null || true
sleep 1

# Build the test binary
echo "[2/7] Building hello_world_window..."
cd "$(dirname "$0")/.."
cargo build --bin hello_world_window --features build-dll 2>&1 | tail -5

# Start the application with remote control enabled
echo "[3/7] Starting application with AZUL_REMOTE_CONTROL=$PORT..."
AZUL_REMOTE_CONTROL=$PORT cargo run --bin hello_world_window --features build-dll 2>&1 > "$LOG_FILE" &
APP_PID=$!

echo "    Application PID: $APP_PID"
echo "    Log file: $LOG_FILE"

# Wait for server to start
echo "[4/7] Waiting for remote control server to start..."
sleep 3

# Check if application is still running
if ! kill -0 $APP_PID 2>/dev/null; then
    echo -e "${RED}ERROR: Application crashed during startup${NC}"
    echo "Last 20 lines of log:"
    tail -20 "$LOG_FILE"
    exit 1
fi

# Function to send event
send_event() {
    local json="$1"
    local description="$2"
    echo "    Sending: $description"
    curl -s -X POST "$URL/event" -d "$json" 2>/dev/null || echo "    (curl failed)"
    sleep 0.5
}

# Test health endpoint
echo "[5/7] Testing remote control connection..."
if curl -s "$URL/health" | grep -q "ok"; then
    echo -e "    ${GREEN}Remote control server responding${NC}"
else
    echo -e "    ${YELLOW}Health check failed (may still work)${NC}"
fi

# Send test events
echo "[6/7] Sending test events..."

# Get initial state
send_event '{"type":"get_state"}' "get_state (initial)"
sleep 1

# Force relayout
send_event '{"type":"relayout"}' "relayout"
sleep 1

# Get state after relayout
send_event '{"type":"get_state"}' "get_state (after relayout)"
sleep 1

# Simulate resize to 800x600
send_event '{"type":"resize","width":800.0,"height":600.0}' "resize to 800x600"
sleep 1

# Get state after resize
send_event '{"type":"get_state"}' "get_state (after resize)"
sleep 1

# Simulate DPI change to 192 (2x scale)
send_event '{"type":"dpi_changed","dpi":192}' "dpi_changed to 192"
sleep 1

# Get final state
send_event '{"type":"get_state"}' "get_state (final)"
sleep 2

# Shutdown
echo "[7/7] Shutting down application..."
send_event '{"type":"shutdown"}' "shutdown"
sleep 2

# Wait for application to exit
wait $APP_PID 2>/dev/null || true

echo ""
echo -e "${GREEN}=== Test Complete ===${NC}"
echo ""
echo "Log file: $LOG_FILE"
echo ""

# Analyze the log file
echo "=== Key Debug Information ==="
echo ""
echo "--- Window Size & DPI ---"
grep -E "\[my_layout_func\]|Logical size|Physical size|DPI:|HiDPI factor" "$LOG_FILE" | head -30
echo ""

echo "--- Font Loading ---"
grep -E "\[DEBUG FontLoading\]" "$LOG_FILE" | head -20
echo ""

echo "--- Layout ---"
grep -E "\[DEBUG Layout\]|\[DEBUG DisplayList\]" "$LOG_FILE" | head -20
echo ""

echo "--- Remote Control Events ---"
grep -E "\[RemoteControl" "$LOG_FILE" | head -30
echo ""

echo "--- Errors/Warnings ---"
grep -iE "error|warning|failed|panic" "$LOG_FILE" | head -20 || echo "(none found)"
echo ""

# Check for specific issues
echo "=== Issue Detection ==="
echo ""

# Check if text was rendered
if grep -q "DisplayListItem::Text\|push_text\|Glyph" "$LOG_FILE"; then
    echo -e "${GREEN}✓ Text rendering detected${NC}"
else
    echo -e "${RED}✗ NO TEXT RENDERING DETECTED - This is a bug!${NC}"
fi

# Check for font loading
if grep -q "fonts from disk\|Loaded.*fonts" "$LOG_FILE"; then
    echo -e "${GREEN}✓ Font loading detected${NC}"
else
    echo -e "${YELLOW}? Font loading not logged${NC}"
fi

# Check for layout callback
if grep -q "\[my_layout_func\]" "$LOG_FILE"; then
    echo -e "${GREEN}✓ Layout callback called${NC}"
else
    echo -e "${RED}✗ Layout callback NOT called${NC}"
fi

# Check for HiDPI factor
if grep -q "HiDPI factor: 2\|dpi_factor=2" "$LOG_FILE"; then
    echo -e "${GREEN}✓ HiDPI factor 2x detected${NC}"
else
    echo -e "${YELLOW}? HiDPI factor 2x not detected${NC}"
fi

echo ""
echo "Full log available at: $LOG_FILE"
