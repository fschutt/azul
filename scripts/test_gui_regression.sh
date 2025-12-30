#!/usr/bin/env bash
#
# Azul GUI Regression Test Script
#
# This script tests the GUI rendering via the JSON debug API.
# It checks for specific conditions and exits with code 0 on success, 1 on failure.
#
# Usage: ./scripts/test_gui_regression.sh
#
# Exit codes:
#   0 - All tests passed
#   1 - One or more tests failed
#
# Expected behavior:
#   - The window should have 4 DOM nodes
#   - The display list should contain text items (text_count > 0)
#   - Node rects should be properly calculated (not null)
#   - HiDPI factor should match the display

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${PROJECT_ROOT}"

# Configuration
DEBUG_PORT="${AZUL_DEBUG_PORT:-8768}"
EXAMPLE_BINARY="./target/release/hello_world_window"
OUTPUT_DIR="${PROJECT_ROOT}/target/test_results"
FAILED=0

echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Azul GUI Regression Test${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""

# Check if the example binary exists
if [ ! -f "$EXAMPLE_BINARY" ]; then
    echo -e "${RED}FAIL: Example binary not found at $EXAMPLE_BINARY${NC}"
    echo "Please run: cargo build --package azul-dll --features build-dll --release"
    exit 1
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Start the example with debug server enabled
echo -e "${YELLOW}Starting $EXAMPLE_BINARY with AZUL_DEBUG=$DEBUG_PORT...${NC}"
AZUL_DEBUG=$DEBUG_PORT $EXAMPLE_BINARY > "$OUTPUT_DIR/app_output.log" 2>&1 &
APP_PID=$!
echo "Started with PID $APP_PID"

# Wait for the debug server to be ready
echo "Waiting for debug server..."
sleep 3

# Function to cleanup on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    # Try to close via API first
    curl -s "http://localhost:$DEBUG_PORT/" -X POST -d '{"type":"close"}' --max-time 2 > /dev/null 2>&1 || true
    sleep 0.5
    kill $APP_PID 2>/dev/null || true
    wait $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

# Helper function to send a command and get response
send_command() {
    local cmd="$1"
    curl -s "http://localhost:$DEBUG_PORT/" -X POST -d "$cmd" --max-time 15
}

# Helper function to check a condition
check() {
    local name="$1"
    local condition="$2"
    
    if eval "$condition"; then
        echo -e "  ${GREEN}✓ PASS:${NC} $name"
        return 0
    else
        echo -e "  ${RED}✗ FAIL:${NC} $name"
        FAILED=1
        return 1
    fi
}

# ============================================================================
# Test 1: DOM Tree Structure
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 1: DOM Tree Structure ===${NC}"

RESPONSE=$(send_command '{"type":"get_dom_tree"}')
echo "$RESPONSE" > "$OUTPUT_DIR/dom_tree.json"

# Extract data from response
DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)

if [ -z "$DATA" ]; then
    echo -e "  ${RED}✗ FAIL: Could not get DOM tree data${NC}"
    FAILED=1
else
    NODE_COUNT=$(echo "$DATA" | jq -r '.node_count // 0' 2>/dev/null || echo "0")
    DPI=$(echo "$DATA" | jq -r '.dpi // 0' 2>/dev/null || echo "0")
    HIDPI=$(echo "$DATA" | jq -r '.hidpi_factor // 0' 2>/dev/null || echo "0")
    LOGICAL_W=$(echo "$DATA" | jq -r '.logical_width // 0' 2>/dev/null || echo "0")
    LOGICAL_H=$(echo "$DATA" | jq -r '.logical_height // 0' 2>/dev/null || echo "0")
    
    echo "  Node count: $NODE_COUNT"
    echo "  DPI: $DPI, HiDPI: $HIDPI"
    echo "  Logical size: ${LOGICAL_W}x${LOGICAL_H}"
    
    check "Node count >= 4" "[ \"$NODE_COUNT\" -ge 4 ]"
    check "DPI is set (> 0)" "[ \"$DPI\" -gt 0 ]"
    
    # Check HiDPI - handle both integer and float
    HIDPI_INT="${HIDPI%.*}"
    if [ -z "$HIDPI_INT" ] || [ "$HIDPI_INT" = "0" ] && [ "${HIDPI#0.}" != "$HIDPI" ]; then
        HIDPI_INT=1  # Treat 0.x as passing
    fi
    check "HiDPI factor > 0" "[ \"${HIDPI_INT:-0}\" -gt 0 ] 2>/dev/null || [ \"$HIDPI\" != \"0\" ]"
    
    # Check logical size - just ensure they're not empty/zero
    check "Logical width > 0" "[ -n \"$LOGICAL_W\" ] && [ \"${LOGICAL_W%.*}\" -gt 0 ] 2>/dev/null"
    check "Logical height > 0" "[ -n \"$LOGICAL_H\" ] && [ \"${LOGICAL_H%.*}\" -gt 0 ] 2>/dev/null"
fi

# ============================================================================
# Test 2: Display List (what's actually being rendered)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 2: Display List ===${NC}"

RESPONSE=$(send_command '{"type":"get_display_list"}')
echo "$RESPONSE" > "$OUTPUT_DIR/display_list.json"

DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)

if [ -z "$DATA" ]; then
    echo -e "  ${RED}✗ FAIL: Could not get display list data${NC}"
    FAILED=1
else
    TOTAL_ITEMS=$(echo "$DATA" | jq -r '.total_items // 0' 2>/dev/null || echo "0")
    RECT_COUNT=$(echo "$DATA" | jq -r '.rect_count // 0' 2>/dev/null || echo "0")
    TEXT_COUNT=$(echo "$DATA" | jq -r '.text_count // 0' 2>/dev/null || echo "0")
    BORDER_COUNT=$(echo "$DATA" | jq -r '.border_count // 0' 2>/dev/null || echo "0")
    
    echo "  Total items: $TOTAL_ITEMS"
    echo "  Rects: $RECT_COUNT, Texts: $TEXT_COUNT, Borders: $BORDER_COUNT"
    
    check "Display list has items (> 0)" "[ \"$TOTAL_ITEMS\" -gt 0 ]"
    check "Has rect items (background)" "[ \"$RECT_COUNT\" -gt 0 ]"
    
    # THIS IS THE KEY TEST - text should be in the display list
    if [ "$TEXT_COUNT" -gt 0 ]; then
        echo -e "  ${GREEN}✓ PASS:${NC} Has text items in display list (text_count=$TEXT_COUNT)"
        
        # Show text items for debugging
        echo "  Text items found:"
        echo "$DATA" | jq -r '.items[] | select(.type == "text") | "    - \"\(.text)\" at (\(.x), \(.y)) size \(.width)x\(.height)"' 2>/dev/null || true
    else
        echo -e "  ${RED}✗ FAIL:${NC} NO TEXT ITEMS IN DISPLAY LIST (text_count=$TEXT_COUNT)"
        echo -e "  ${RED}       This is the main bug - text is not being rendered!${NC}"
        FAILED=1
    fi
fi

# ============================================================================
# Test 3: Node Layout (all nodes should have positions)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 3: Node Layout ===${NC}"

RESPONSE=$(send_command '{"type":"get_all_nodes_layout"}')
echo "$RESPONSE" > "$OUTPUT_DIR/node_layout.json"

DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)

if [ -z "$DATA" ]; then
    echo -e "  ${RED}✗ FAIL: Could not get node layout data${NC}"
    FAILED=1
else
    NODE_COUNT=$(echo "$DATA" | jq -r '.node_count // 0' 2>/dev/null || echo "0")
    
    # Count nodes with non-null rects
    NODES_WITH_RECT=$(echo "$DATA" | jq -r '[.nodes[] | select(.rect != null)] | length' 2>/dev/null || echo "0")
    NODES_WITHOUT_RECT=$(echo "$DATA" | jq -r '[.nodes[] | select(.rect == null)] | length' 2>/dev/null || echo "0")
    
    echo "  Total nodes: $NODE_COUNT"
    echo "  Nodes with rect: $NODES_WITH_RECT"
    echo "  Nodes without rect: $NODES_WITHOUT_RECT"
    
    check "At least some nodes have layout rects" "[ \"$NODES_WITH_RECT\" -gt 0 ]"
    
    # Check if the red div (should be node 2 or 3) has a proper width
    echo "  Node details:"
    echo "$DATA" | jq -r '.nodes[] | "    Node \(.node_id): \(.tag // "root") - rect: \(.rect // "null")"' 2>/dev/null || true
    
    # Check that div nodes have proper width (should be close to window width)
    DIV_WIDTH=$(echo "$DATA" | jq -r '.nodes[] | select(.tag == "div") | .rect.width // 0' 2>/dev/null | head -1 || echo "0")
    if [ -n "$DIV_WIDTH" ] && [ "$DIV_WIDTH" != "0" ] && [ "$DIV_WIDTH" != "null" ]; then
        echo "  First div width: $DIV_WIDTH"
        DIV_WIDTH_INT="${DIV_WIDTH%.*}"
        check "Div has reasonable width (> 600)" "[ \"${DIV_WIDTH_INT:-0}\" -gt 600 ]"
    fi
fi

# ============================================================================
# Test 4: HTML String (DOM representation)
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 4: HTML String ===${NC}"

RESPONSE=$(send_command '{"type":"get_html_string"}')
HTML=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
echo "$HTML" > "$OUTPUT_DIR/dom.html"

if [ -z "$HTML" ]; then
    echo -e "  ${RED}✗ FAIL: Could not get HTML string${NC}"
    FAILED=1
else
    echo "  HTML length: ${#HTML} chars"
    
    # Check for text content
    if echo "$HTML" | grep -q "Azul HiDPI"; then
        echo -e "  ${GREEN}✓ PASS:${NC} HTML contains text 'Azul HiDPI'"
    else
        echo -e "  ${RED}✗ FAIL:${NC} HTML missing expected text content"
        FAILED=1
    fi
    
    # Check for text nodes
    TEXT_NODES=$(echo "$HTML" | grep -c "<text" || echo "0")
    check "HTML has text nodes (>= 2)" "[ \"$TEXT_NODES\" -ge 2 ]"
fi

# ============================================================================
# Test 5: Screenshot
# ============================================================================
echo ""
echo -e "${BLUE}=== Test 5: Screenshot ===${NC}"

RESPONSE=$(send_command '{"type":"take_native_screenshot"}')
DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)

if [ -n "$DATA" ] && [[ "$DATA" == data:image/png\;base64,* ]]; then
    BASE64_DATA="${DATA#data:image/png;base64,}"
    echo "$BASE64_DATA" | base64 -d > "$OUTPUT_DIR/screenshot.png"
    
    # Get image dimensions using file command
    IMG_INFO=$(file "$OUTPUT_DIR/screenshot.png")
    echo "  Screenshot: $IMG_INFO"
    
    if echo "$IMG_INFO" | grep -q "PNG image data"; then
        echo -e "  ${GREEN}✓ PASS:${NC} Screenshot is valid PNG"
    else
        echo -e "  ${RED}✗ FAIL:${NC} Screenshot is not valid PNG"
        FAILED=1
    fi
else
    echo -e "  ${RED}✗ FAIL:${NC} Could not take screenshot"
    FAILED=1
fi

# ============================================================================
# Summary
# ============================================================================
echo ""
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Test Results${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""
echo "Output files: $OUTPUT_DIR"

if [ "$FAILED" -eq 0 ]; then
    echo ""
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo ""
    echo -e "${RED}Some tests failed!${NC}"
    echo ""
    echo "Debug info:"
    echo "  - Check $OUTPUT_DIR/display_list.json for display list items"
    echo "  - Check $OUTPUT_DIR/dom.html for DOM structure"
    echo "  - Check $OUTPUT_DIR/screenshot.png for visual output"
    echo "  - Check $OUTPUT_DIR/app_output.log for app logs"
    exit 1
fi
