#!/bin/bash
# Test script for the get_scrollbar_info Debug API
# Tests scrollbar geometry retrieval and scrollbar interaction

set -e

PORT=8765
API="http://localhost:$PORT"
EXAMPLE="scrolling"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Scrollbar Info API Test ===${NC}"

# Check if debug server is running
if ! curl -s "$API/" > /dev/null 2>&1; then
    echo -e "${RED}Debug server not running on port $PORT${NC}"
    echo "Please start the scrolling example with: AZUL_DEBUG=$PORT cargo run --release --example $EXAMPLE"
    exit 1
fi

echo -e "${GREEN}Debug server is running${NC}"

# Helper function to send request and pretty-print response
send_request() {
    local data="$1"
    local response=$(curl -s -X POST "$API/" -d "$data")
    echo "$response"
}

# Helper function to extract JSON field
get_json_field() {
    local json="$1"
    local field="$2"
    echo "$json" | python3 -c "import sys, json; d=json.load(sys.stdin); print(json.dumps(d.get('data', {}).get('value', {}).get('$field', 'N/A'), indent=2))"
}

echo ""
echo -e "${BLUE}=== Step 1: Get scrollable nodes ===${NC}"
SCROLLABLE=$(send_request '{"op":"get_scrollable_nodes"}')
echo "$SCROLLABLE" | python3 -m json.tool 2>/dev/null | head -30

# Extract first scrollable node ID
FIRST_NODE=$(echo "$SCROLLABLE" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    nodes = d.get('data', {}).get('value', {}).get('scrollable_nodes', [])
    if nodes:
        print(nodes[0].get('dom_node_id', nodes[0].get('node_id', '')))
    else:
        print('')
except:
    print('')
" 2>/dev/null)

if [ -z "$FIRST_NODE" ] || [ "$FIRST_NODE" = "None" ]; then
    echo -e "${RED}No scrollable nodes found!${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}Found scrollable node ID: $FIRST_NODE${NC}"

echo ""
echo -e "${BLUE}=== Step 2: Get scrollbar info for node $FIRST_NODE ===${NC}"
SCROLLBAR_INFO=$(send_request "{\"op\":\"get_scrollbar_info\", \"node_id\": $FIRST_NODE}")
echo "$SCROLLBAR_INFO" | python3 -m json.tool 2>/dev/null

# Extract vertical scrollbar info
echo ""
echo -e "${BLUE}=== Vertical Scrollbar Geometry ===${NC}"
echo "$SCROLLBAR_INFO" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    v = d.get('data', {}).get('value', {}).get('vertical', {})
    if v:
        print(f\"  Visible: {v.get('visible', 'N/A')}\")
        track = v.get('track_rect', {})
        print(f\"  Track: x={track.get('x', 0):.1f}, y={track.get('y', 0):.1f}, w={track.get('width', 0):.1f}, h={track.get('height', 0):.1f}\")
        thumb = v.get('thumb_rect', {})
        print(f\"  Thumb: x={thumb.get('x', 0):.1f}, y={thumb.get('y', 0):.1f}, w={thumb.get('width', 0):.1f}, h={thumb.get('height', 0):.1f}\")
        thumb_center = v.get('thumb_center', {})
        print(f\"  Thumb Center: x={thumb_center.get('x', 0):.1f}, y={thumb_center.get('y', 0):.1f}\")
        print(f\"  Thumb Position Ratio: {v.get('thumb_position_ratio', 0):.3f}\")
        print(f\"  Thumb Size Ratio: {v.get('thumb_size_ratio', 0):.3f}\")
        top_btn = v.get('top_button_rect', {})
        print(f\"  Top Button: x={top_btn.get('x', 0):.1f}, y={top_btn.get('y', 0):.1f}, w={top_btn.get('width', 0):.1f}, h={top_btn.get('height', 0):.1f}\")
        bottom_btn = v.get('bottom_button_rect', {})
        print(f\"  Bottom Button: x={bottom_btn.get('x', 0):.1f}, y={bottom_btn.get('y', 0):.1f}, w={bottom_btn.get('width', 0):.1f}, h={bottom_btn.get('height', 0):.1f}\")
    else:
        print('  No vertical scrollbar found')
except Exception as e:
    print(f'  Error: {e}')
" 2>/dev/null

echo ""
echo -e "${BLUE}=== Step 3: Test scroll animation (10 steps x 5px each) ===${NC}"
echo "Initial scroll state:"
send_request '{"op":"get_scroll_states"}' | python3 -c "
import sys, json
d = json.load(sys.stdin)
states = d.get('data', {}).get('value', {}).get('scroll_states', [])
for s in states:
    print(f\"  Node {s['node_id']}: scroll_y={s['scroll_y']:.1f}, max_scroll_y={s['max_scroll_y']:.1f}\")
" 2>/dev/null

echo ""
echo "Scrolling..."
for i in {1..10}; do
    send_request "{\"op\":\"scroll_node_by\", \"node_id\": $FIRST_NODE, \"delta_x\": 0, \"delta_y\": 5}" > /dev/null
    # Get updated scroll position
    SCROLL_Y=$(send_request '{"op":"get_scroll_states"}' | python3 -c "
import sys, json
d = json.load(sys.stdin)
states = d.get('data', {}).get('value', {}).get('scroll_states', [])
for s in states:
    if s['node_id'] == $FIRST_NODE:
        print(f\"{s['scroll_y']:.1f}\")
        break
" 2>/dev/null)
    echo "  Step $i: scroll_y = $SCROLL_Y"
    sleep 0.15
done

echo ""
echo -e "${BLUE}=== Step 4: Get updated scrollbar info ===${NC}"
UPDATED_INFO=$(send_request "{\"op\":\"get_scrollbar_info\", \"node_id\": $FIRST_NODE}")
echo "$UPDATED_INFO" | python3 -c "
import sys, json
d = json.load(sys.stdin)
v = d.get('data', {}).get('value', {})
print(f\"  scroll_y: {v.get('scroll_y', 0):.1f}\")
print(f\"  max_scroll_y: {v.get('max_scroll_y', 0):.1f}\")
vert = v.get('vertical', {})
if vert:
    print(f\"  thumb_position_ratio: {vert.get('thumb_position_ratio', 0):.3f}\")
" 2>/dev/null

echo ""
echo -e "${BLUE}=== Step 5: Test clicking on scrollbar track ===${NC}"

# Get the track center position
TRACK_CENTER_Y=$(echo "$SCROLLBAR_INFO" | python3 -c "
import sys, json
d = json.load(sys.stdin)
v = d.get('data', {}).get('value', {}).get('vertical', {})
track = v.get('track_rect', {})
# Click in the middle of the track (below the current thumb position)
center_y = track.get('y', 0) + track.get('height', 0) * 0.8
center_x = track.get('x', 0) + track.get('width', 0) / 2
print(f\"{center_x:.1f} {center_y:.1f}\")
" 2>/dev/null)

CLICK_X=$(echo "$TRACK_CENTER_Y" | cut -d' ' -f1)
CLICK_Y=$(echo "$TRACK_CENTER_Y" | cut -d' ' -f2)

if [ -n "$CLICK_X" ] && [ -n "$CLICK_Y" ]; then
    echo "Clicking at track position ($CLICK_X, $CLICK_Y)..."
    send_request "{\"op\":\"click\", \"x\": $CLICK_X, \"y\": $CLICK_Y}" > /dev/null
    sleep 0.2
    
    # Check new scroll position
    AFTER_CLICK=$(send_request '{"op":"get_scroll_states"}')
    echo "$AFTER_CLICK" | python3 -c "
import sys, json
d = json.load(sys.stdin)
states = d.get('data', {}).get('value', {}).get('scroll_states', [])
for s in states:
    print(f\"  Node {s['node_id']}: scroll_y={s['scroll_y']:.1f} (after track click)\")
" 2>/dev/null
else
    echo -e "${YELLOW}Could not determine track click position${NC}"
fi

echo ""
echo -e "${BLUE}=== Step 6: Reset scroll to beginning ===${NC}"
send_request "{\"op\":\"scroll_node_to\", \"node_id\": $FIRST_NODE, \"x\": 0, \"y\": 0}" > /dev/null
RESET=$(send_request '{"op":"get_scroll_states"}')
echo "$RESET" | python3 -c "
import sys, json
d = json.load(sys.stdin)
states = d.get('data', {}).get('value', {}).get('scroll_states', [])
for s in states:
    print(f\"  Node {s['node_id']}: scroll_y={s['scroll_y']:.1f} (after reset)\")
" 2>/dev/null

echo ""
echo -e "${GREEN}=== Test Complete ===${NC}"
