#!/bin/bash
# Debug script for analyzing coordinate space and clipping issues
# Uses the AZUL_DEBUG HTTP API to collect debug information

set -e

PORT=8765
API="http://localhost:$PORT"
OUTPUT_DIR="target/debug_output"
EXAMPLE="scrolling"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Azul Clipping Debug Script ===${NC}"

# Create output directory
mkdir -p "$OUTPUT_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
OUTPUT_FILE="$OUTPUT_DIR/debug_${TIMESTAMP}.json"

# Kill any existing instances
echo -e "${YELLOW}Killing any existing scrolling processes...${NC}"
pkill -f "scrolling" 2>/dev/null || true
sleep 1

# Build in release mode first
echo -e "${YELLOW}Building release example...${NC}"
cargo build --release --example "$EXAMPLE" 2>&1 | tail -3

# Start the application with debug server
echo -e "${YELLOW}Starting application with AZUL_DEBUG=$PORT...${NC}"
AZUL_DEBUG=$PORT cargo run --release --example "$EXAMPLE" &
APP_PID=$!

# Wait for the debug server to be ready
echo -e "${YELLOW}Waiting for debug server to start...${NC}"
MAX_WAIT=30
WAITED=0
while ! curl -s "$API/" > /dev/null 2>&1; do
    sleep 0.5
    WAITED=$((WAITED + 1))
    if [ $WAITED -ge $MAX_WAIT ]; then
        echo -e "${RED}Timeout waiting for debug server${NC}"
        kill $APP_PID 2>/dev/null || true
        exit 1
    fi
done
echo -e "${GREEN}Debug server is ready!${NC}"

# Wait a bit more for the window to render
sleep 1

# Collect all debug information
echo -e "${YELLOW}Collecting debug information...${NC}"

# Create a combined JSON output
echo "{" > "$OUTPUT_FILE"

# 1. Get window state
echo -e "  ${GREEN}Getting window state...${NC}"
echo '  "window_state": ' >> "$OUTPUT_FILE"
curl -s -X POST "$API/" -d '{"op": "get_state"}' >> "$OUTPUT_FILE"
echo "," >> "$OUTPUT_FILE"

# 2. Get DOM tree
echo -e "  ${GREEN}Getting DOM tree...${NC}"
echo '  "dom_tree": ' >> "$OUTPUT_FILE"
curl -s -X POST "$API/" -d '{"op": "get_dom_tree"}' >> "$OUTPUT_FILE"
echo "," >> "$OUTPUT_FILE"

# 3. Get display list (IMPORTANT for clipping analysis)
echo -e "  ${GREEN}Getting display list...${NC}"
echo '  "display_list": ' >> "$OUTPUT_FILE"
curl -s -X POST "$API/" -d '{"op": "get_display_list"}' >> "$OUTPUT_FILE"
echo "," >> "$OUTPUT_FILE"

# 4. Get all nodes layout
echo -e "  ${GREEN}Getting all nodes layout...${NC}"
echo '  "all_nodes_layout": ' >> "$OUTPUT_FILE"
curl -s -X POST "$API/" -d '{"op": "get_all_nodes_layout"}' >> "$OUTPUT_FILE"
echo "," >> "$OUTPUT_FILE"

# 5. Get scrollable nodes
echo -e "  ${GREEN}Getting scrollable nodes...${NC}"
echo '  "scrollable_nodes": ' >> "$OUTPUT_FILE"
curl -s -X POST "$API/" -d '{"op": "get_scrollable_nodes"}' >> "$OUTPUT_FILE"
echo "," >> "$OUTPUT_FILE"

# 6. Get scroll states
echo -e "  ${GREEN}Getting scroll states...${NC}"
echo '  "scroll_states": ' >> "$OUTPUT_FILE"
curl -s -X POST "$API/" -d '{"op": "get_scroll_states"}' >> "$OUTPUT_FILE"
echo "," >> "$OUTPUT_FILE"

# 7. Get layout tree
echo -e "  ${GREEN}Getting layout tree...${NC}"
echo '  "layout_tree": ' >> "$OUTPUT_FILE"
curl -s -X POST "$API/" -d '{"op": "get_layout_tree"}' >> "$OUTPUT_FILE"
echo "," >> "$OUTPUT_FILE"

# 8. Get logs
echo -e "  ${GREEN}Getting logs...${NC}"
echo '  "logs": ' >> "$OUTPUT_FILE"
curl -s -X POST "$API/" -d '{"op": "get_logs"}' >> "$OUTPUT_FILE"

echo "}" >> "$OUTPUT_FILE"

# Take a screenshot for visual reference
echo -e "${YELLOW}Taking screenshot...${NC}"
SCREENSHOT_FILE="$OUTPUT_DIR/screenshot_${TIMESTAMP}.png"
curl -s -X POST "$API/" -d '{"op": "take_screenshot"}' | jq -r '.data.screenshot // empty' | base64 -d > "$SCREENSHOT_FILE" 2>/dev/null || echo "Screenshot failed"

# Close the window
echo -e "${YELLOW}Closing window...${NC}"
curl -s -X POST "$API/" -d '{"op": "close"}' > /dev/null 2>&1 || true

# Wait for process to exit
sleep 1
kill $APP_PID 2>/dev/null || true

echo -e "${GREEN}Debug data collected!${NC}"
echo -e "Output file: ${YELLOW}$OUTPUT_FILE${NC}"
echo -e "Screenshot: ${YELLOW}$SCREENSHOT_FILE${NC}"

# Print summary analysis
echo ""
echo -e "${YELLOW}=== Quick Analysis ===${NC}"

# Check if display list is balanced
echo -e "\n${GREEN}Clip/Scroll Balance:${NC}"
jq '.display_list.data.value.clip_analysis // "No clip analysis available"' "$OUTPUT_FILE" 2>/dev/null || echo "Could not parse clip analysis"

# Show display list item count
echo -e "\n${GREEN}Display List Item Count:${NC}"
jq '.display_list.data.value.items | length // "Unknown"' "$OUTPUT_FILE" 2>/dev/null || echo "Could not count items"

# Show PushClip and PushScrollFrame operations
echo -e "\n${GREEN}Clip/Scroll Operations:${NC}"
jq '.display_list.data.value.clip_analysis.operations // []' "$OUTPUT_FILE" 2>/dev/null | head -50

# Show scrollbar items
echo -e "\n${GREEN}ScrollBar Items:${NC}"
jq '[.display_list.data.value.items[] | select(.type == "scrollbar_styled" or .type == "scrollbar")] | length' "$OUTPUT_FILE" 2>/dev/null || echo "Could not find scrollbar items"

# Show all item types
echo -e "\n${GREEN}All Display List Item Types:${NC}"
jq '[.display_list.data.value.items[].type] | group_by(.) | map({type: .[0], count: length})' "$OUTPUT_FILE" 2>/dev/null | head -30

echo ""
echo -e "${GREEN}For detailed analysis, run:${NC}"
echo "  cat $OUTPUT_FILE | jq '.display_list.data.value.items[] | select(.type | contains(\"scroll\"))'"
echo "  cat $OUTPUT_FILE | jq '.display_list.data.value.clip_analysis'"
