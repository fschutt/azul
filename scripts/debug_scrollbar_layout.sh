#!/bin/bash
#
# Debug script for scrollbar layout issues
# 
# Usage:
#   1. Start the application with debug server: AZUL_DEBUG=8765 cargo run --release --example scrolling
#   2. Run this script: ./scripts/debug_scrollbar_layout.sh
#   3. Check the output files in target/debug_output/
#

set -e

PORT="${AZUL_DEBUG_PORT:-8765}"
API="http://localhost:$PORT"
OUTPUT_DIR="target/debug_output"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Azul Scrollbar Layout Debug Script ===${NC}"
echo "API endpoint: $API"
echo "Output directory: $OUTPUT_DIR"
echo ""

# Check if server is running
echo -e "${YELLOW}Checking debug server...${NC}"
if ! curl -s "$API/" > /dev/null 2>&1; then
    echo -e "${RED}Error: Debug server not running on port $PORT${NC}"
    echo "Start your app with: AZUL_DEBUG=$PORT cargo run --release --example scrolling"
    exit 1
fi
echo -e "${GREEN}Debug server is running${NC}"
echo ""

# Get display list
echo -e "${YELLOW}Fetching display list...${NC}"
curl -s -X POST "$API/" -d '{"op":"get_display_list"}' > "$OUTPUT_DIR/display_list.json"
echo "Saved to $OUTPUT_DIR/display_list.json"

# Get layout tree
echo -e "${YELLOW}Fetching layout tree...${NC}"
curl -s -X POST "$API/" -d '{"op":"get_layout_tree"}' > "$OUTPUT_DIR/layout_tree.json"
echo "Saved to $OUTPUT_DIR/layout_tree.json"

# Get scroll states
echo -e "${YELLOW}Fetching scroll states...${NC}"
curl -s -X POST "$API/" -d '{"op":"get_scroll_states"}' > "$OUTPUT_DIR/scroll_states.json"
echo "Saved to $OUTPUT_DIR/scroll_states.json"

# Get scrollable nodes
echo -e "${YELLOW}Fetching scrollable nodes...${NC}"
curl -s -X POST "$API/" -d '{"op":"get_scrollable_nodes"}' > "$OUTPUT_DIR/scrollable_nodes.json"
echo "Saved to $OUTPUT_DIR/scrollable_nodes.json"

# Get logs
echo -e "${YELLOW}Fetching debug logs...${NC}"
curl -s -X POST "$API/" -d '{"op":"get_logs"}' > "$OUTPUT_DIR/logs.json"
echo "Saved to $OUTPUT_DIR/logs.json"

echo ""
echo -e "${BLUE}=== Analysis ===${NC}"

# Analyze scroll frame clip
echo ""
echo -e "${YELLOW}Scroll Frame Clip:${NC}"
cat "$OUTPUT_DIR/display_list.json" | python3 -c "
import sys, json
d = json.load(sys.stdin)
analysis = d.get('data', {}).get('value', {}).get('clip_analysis', {})
for op in analysis.get('operations', []):
    if op.get('op') == 'PushScrollFrame':
        b = op.get('bounds', {})
        cs = op.get('content_size', {})
        print(f'  clip bounds: x={b.get(\"x\")}, y={b.get(\"y\")}, w={b.get(\"width\")}, h={b.get(\"height\")}')
        print(f'  content_size: w={cs.get(\"width\")}, h={cs.get(\"height\")}')
        print(f'  scroll_id: {op.get(\"scroll_id\")}')
"

# Analyze child positions
echo ""
echo -e "${YELLOW}Child Rects (inside scroll frame):${NC}"
cat "$OUTPUT_DIR/display_list.json" | python3 -c "
import sys, json
d = json.load(sys.stdin)
items = d.get('data', {}).get('value', {}).get('items', [])
for item in items:
    if item.get('scroll_depth') == 1 and item.get('type') == 'rect':
        print(f'  x={item[\"x\"]:6.1f} y={item[\"y\"]:6.1f} w={item[\"width\"]:6.1f} h={item[\"height\"]:6.1f} {item[\"color\"]}')
"

# Analyze scrollbars
echo ""
echo -e "${YELLOW}Scrollbars:${NC}"
cat "$OUTPUT_DIR/display_list.json" | python3 -c "
import sys, json
d = json.load(sys.stdin)
items = d.get('data', {}).get('value', {}).get('items', [])
for item in items:
    t = item.get('type', '')
    if 'scrollbar' in t:
        print(f'  {t}:')
        print(f'    position: x={item.get(\"x\")}, y={item.get(\"y\")}')
        print(f'    size: w={item.get(\"width\")}, h={item.get(\"height\")}')
"

# Check for layout issues
echo ""
echo -e "${YELLOW}Layout Issue Check:${NC}"
cat "$OUTPUT_DIR/display_list.json" | python3 -c "
import sys, json
d = json.load(sys.stdin)
items = d.get('data', {}).get('value', {}).get('items', [])
analysis = d.get('data', {}).get('value', {}).get('clip_analysis', {})

# Find scroll frame clip
clip_x, clip_y, clip_w, clip_h = 0, 0, 0, 0
for op in analysis.get('operations', []):
    if op.get('op') == 'PushScrollFrame':
        b = op.get('bounds', {})
        clip_x, clip_y = b.get('x', 0), b.get('y', 0)
        clip_w, clip_h = b.get('width', 0), b.get('height', 0)

# Find vertical scrollbar
scrollbar_x = None
for item in items:
    if 'scrollbar_vertical' in item.get('type', ''):
        scrollbar_x = item.get('x', 0)

# Check child positions
issues = []
for item in items:
    if item.get('scroll_depth') == 1 and item.get('type') == 'rect':
        child_x = item.get('x', 0)
        child_w = item.get('width', 0)
        child_end_x = child_x + child_w
        clip_end_x = clip_x + clip_w
        
        if scrollbar_x and child_end_x > scrollbar_x:
            overlap = child_end_x - scrollbar_x
            issues.append(f'Child rect overlaps scrollbar by {overlap:.1f}px (child ends at x={child_end_x:.1f}, scrollbar at x={scrollbar_x:.1f})')
            break
        elif child_end_x > clip_end_x:
            overflow = child_end_x - clip_end_x
            issues.append(f'Child rect overflows clip by {overflow:.1f}px (child ends at x={child_end_x:.1f}, clip ends at x={clip_end_x:.1f})')
            break

if issues:
    print('  ⚠️  Issues found:')
    for issue in issues:
        print(f'    - {issue}')
else:
    print('  ✅ No layout issues detected')
"

# Filter logs for scrollbar-related entries
echo ""
echo -e "${YELLOW}Scrollbar-related logs:${NC}"
cat "$OUTPUT_DIR/logs.json" | python3 -c "
import sys, json
d = json.load(sys.stdin)
logs = d.get('data', {}).get('logs', [])
scrollbar_logs = [l for l in logs if 'scroll' in l.get('message', '').lower() or 'reflow' in l.get('message', '').lower()]
if scrollbar_logs:
    for log in scrollbar_logs[-10:]:
        print(f'  [{log.get(\"level\", \"\")}] {log.get(\"message\", \"\")}')
else:
    print('  (no scrollbar-related logs found)')
" 2>/dev/null || echo "  (logs not available or empty)"

echo ""
echo -e "${GREEN}Debug data saved to $OUTPUT_DIR/${NC}"
echo ""
echo "Useful commands:"
echo "  # View full display list:"
echo "  cat $OUTPUT_DIR/display_list.json | python3 -m json.tool | less"
echo ""
echo "  # Filter for specific item types:"
echo "  cat $OUTPUT_DIR/display_list.json | jq '.data.value.items[] | select(.type | contains(\"scroll\"))'"
echo ""
echo "  # View layout tree:"
echo "  cat $OUTPUT_DIR/layout_tree.json | python3 -m json.tool | less"
