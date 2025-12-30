#!/usr/bin/env bash
#
# Test Azul's JSON GUI Automation API for DOM Inspection
#
# This script demonstrates using the debug server's JSON API to:
# 1. Take screenshots
# 2. Get HTML representation of the DOM
# 3. Inspect CSS properties of nodes
# 4. Get layout information (position, size)
#
# This is used to verify that the UI is rendering correctly and
# to debug visual issues in compiled binaries.
#
# Usage: ./scripts/test_dom_inspection.sh

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${PROJECT_ROOT}"

# Configuration
DEBUG_PORT="${AZUL_DEBUG_PORT:-8767}"
OUTPUT_DIR="${PROJECT_ROOT}/target/debug_inspection"
EXAMPLE_BINARY="./target/release/hello_world_window"

echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Azul DOM Inspection Test${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""

# Check if the example binary exists
if [ ! -f "$EXAMPLE_BINARY" ]; then
    echo -e "${RED}Error: Example binary not found at $EXAMPLE_BINARY${NC}"
    echo "Please run: cargo build --package azul-dll --features build-dll --release"
    exit 1
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Start the example with debug server enabled
echo -e "${YELLOW}Starting $EXAMPLE_BINARY with AZUL_DEBUG=$DEBUG_PORT...${NC}"
AZUL_DEBUG=$DEBUG_PORT $EXAMPLE_BINARY &
APP_PID=$!
echo "Started with PID $APP_PID"

# Wait for the debug server to be ready
echo "Waiting for debug server to start..."
sleep 3

# Function to cleanup on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    kill $APP_PID 2>/dev/null || true
    wait $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

# Helper function to send a command and get response
send_command() {
    local cmd="$1"
    curl -s "http://localhost:$DEBUG_PORT/" -X POST -d "$cmd" --max-time 15
}

# ============================================================================
# Test 1: Get DOM Tree Info
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: Get DOM Tree Info ===${NC}"
RESPONSE=$(send_command '{"type":"get_dom_tree"}')
echo "$RESPONSE" | jq '.' 2>/dev/null || echo "$RESPONSE"
echo "$RESPONSE" > "$OUTPUT_DIR/dom_tree.json"

# Extract key info
NODE_COUNT=$(echo "$RESPONSE" | jq -r '.data' 2>/dev/null | jq -r '.node_count // "unknown"' 2>/dev/null || echo "parse error")
DPI=$(echo "$RESPONSE" | jq -r '.data' 2>/dev/null | jq -r '.dpi // "unknown"' 2>/dev/null || echo "parse error")
HIDPI=$(echo "$RESPONSE" | jq -r '.data' 2>/dev/null | jq -r '.hidpi_factor // "unknown"' 2>/dev/null || echo "parse error")
LOGICAL_W=$(echo "$RESPONSE" | jq -r '.data' 2>/dev/null | jq -r '.logical_width // "unknown"' 2>/dev/null || echo "parse error")
LOGICAL_H=$(echo "$RESPONSE" | jq -r '.data' 2>/dev/null | jq -r '.logical_height // "unknown"' 2>/dev/null || echo "parse error")

echo -e "  Node count: ${GREEN}$NODE_COUNT${NC}"
echo -e "  DPI: ${GREEN}$DPI${NC}"
echo -e "  HiDPI factor: ${GREEN}$HIDPI${NC}"
echo -e "  Logical size: ${GREEN}${LOGICAL_W}x${LOGICAL_H}${NC}"

# ============================================================================
# Test 2: Get HTML String
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: Get HTML String ===${NC}"
RESPONSE=$(send_command '{"type":"get_html_string"}')
HTML=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
if [ -n "$HTML" ]; then
    echo "$HTML" > "$OUTPUT_DIR/dom.html"
    echo -e "${GREEN}HTML saved to $OUTPUT_DIR/dom.html${NC}"
    # Show first 500 chars
    echo "Preview (first 500 chars):"
    echo "${HTML:0:500}"
else
    echo -e "${RED}Failed to get HTML string${NC}"
    echo "$RESPONSE"
fi

# ============================================================================
# Test 3: Get All Nodes Layout
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Get All Nodes Layout ===${NC}"
RESPONSE=$(send_command '{"type":"get_all_nodes_layout"}')
echo "$RESPONSE" > "$OUTPUT_DIR/all_nodes_layout.json"
echo -e "${GREEN}Layout data saved to $OUTPUT_DIR/all_nodes_layout.json${NC}"

# Parse and display node info
DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
if [ -n "$DATA" ]; then
    echo "$DATA" | jq '.' 2>/dev/null || echo "$DATA"
else
    echo -e "${RED}Failed to parse layout data${NC}"
fi

# ============================================================================
# Test 4: Get CSS Properties for Each Node
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: Get CSS Properties for Nodes ===${NC}"

# Get node count from previous response
NODE_COUNT_NUM=$(echo "$DATA" | jq -r '.node_count // 0' 2>/dev/null || echo "0")
if [ "$NODE_COUNT_NUM" -gt 0 ] 2>/dev/null; then
    for i in $(seq 0 $((NODE_COUNT_NUM - 1))); do
        echo ""
        echo -e "${YELLOW}Node $i CSS Properties:${NC}"
        RESPONSE=$(send_command "{\"type\":\"get_node_css_properties\", \"node_id\": $i}")
        DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
        if [ -n "$DATA" ]; then
            echo "$DATA" | jq '.' 2>/dev/null || echo "$DATA"
            echo "$DATA" > "$OUTPUT_DIR/node_${i}_css.json"
        else
            echo "No CSS properties or error"
        fi
    done
else
    echo "Could not determine node count, trying first 5 nodes..."
    for i in 0 1 2 3 4; do
        echo ""
        echo -e "${YELLOW}Node $i CSS Properties:${NC}"
        RESPONSE=$(send_command "{\"type\":\"get_node_css_properties\", \"node_id\": $i}")
        DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
        if [ -n "$DATA" ]; then
            echo "$DATA" | jq '.' 2>/dev/null || echo "$DATA"
            echo "$DATA" > "$OUTPUT_DIR/node_${i}_css.json"
        fi
    done
fi

# ============================================================================
# Test 5: Take Screenshot
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Take Native Screenshot ===${NC}"
RESPONSE=$(send_command '{"type":"take_native_screenshot"}')
DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)

if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
    BASE64_DATA="${DATA#data:image/png;base64,}"
    TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    OUTPUT_FILE="$OUTPUT_DIR/screenshot_$TIMESTAMP.png"
    echo "$BASE64_DATA" | base64 -d > "$OUTPUT_FILE"
    echo -e "${GREEN}✓ Screenshot saved to: $OUTPUT_FILE${NC}"
    ls -la "$OUTPUT_FILE"
    file "$OUTPUT_FILE"
else
    echo -e "${RED}✗ Failed to take screenshot${NC}"
    echo "Response: ${RESPONSE:0:500}"
fi

# ============================================================================
# Test 6: Check for Visual Bugs (Expected vs Actual)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 6: Visual Bug Detection ===${NC}"

# Define expected values based on the example
EXPECTED_NODE_COUNT=4  # Root + 3 children
EXPECTED_LOGICAL_WIDTH=640
EXPECTED_LOGICAL_HEIGHT=480

# Check node count
ACTUAL_NODE_COUNT=$(echo "$NODE_COUNT" | grep -o '[0-9]*' || echo "0")
if [ "$ACTUAL_NODE_COUNT" -eq "$EXPECTED_NODE_COUNT" ] 2>/dev/null; then
    echo -e "${GREEN}✓ Node count correct: $ACTUAL_NODE_COUNT${NC}"
else
    echo -e "${RED}✗ Node count mismatch: expected $EXPECTED_NODE_COUNT, got $ACTUAL_NODE_COUNT${NC}"
fi

# Check if text is present in the HTML (should have "Hello World" or counter text)
if [ -f "$OUTPUT_DIR/dom.html" ]; then
    if grep -q "counter\|Hello\|label\|text" "$OUTPUT_DIR/dom.html" 2>/dev/null; then
        echo -e "${GREEN}✓ Text content found in DOM${NC}"
    else
        echo -e "${RED}✗ No text content found in DOM - TEXT RENDERING BUG?${NC}"
    fi
fi

# Check HiDPI factor
if [ "$HIDPI" = "1" ] || [ "$HIDPI" = "1.0" ]; then
    echo -e "${YELLOW}⚠ HiDPI factor is 1.0 - might be incorrect on Retina displays${NC}"
elif [ "$HIDPI" = "2" ] || [ "$HIDPI" = "2.0" ]; then
    echo -e "${GREEN}✓ HiDPI factor is 2.0 (correct for Retina)${NC}"
else
    echo -e "${YELLOW}? HiDPI factor: $HIDPI${NC}"
fi

# Check layout - red box should extend to right edge
# In the example, the red box should have width close to window width minus padding
echo ""
echo -e "${YELLOW}Checking layout bounds...${NC}"
if [ -f "$OUTPUT_DIR/all_nodes_layout.json" ]; then
    cat "$OUTPUT_DIR/all_nodes_layout.json" | jq -r '.data' 2>/dev/null | jq '.' 2>/dev/null || cat "$OUTPUT_DIR/all_nodes_layout.json"
fi

# ============================================================================
# Summary
# ============================================================================
echo ""
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Test Summary${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""
echo "Output files saved to: $OUTPUT_DIR"
ls -la "$OUTPUT_DIR"

# Close the window
echo ""
echo -e "${YELLOW}Closing window...${NC}"
send_command '{"type":"close"}' > /dev/null 2>&1 || true

echo ""
echo -e "${GREEN}Done!${NC}"
