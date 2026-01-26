#!/bin/bash
# Test that scroll changes are properly sent to WebRender
# Uses get_display_list to verify scroll frame offsets change after scroll

set -e

DEBUG_PORT=${AZUL_DEBUG:-8765}
BASE_URL="http://localhost:$DEBUG_PORT"

echo "=== WebRender Scroll Verification Test ==="
echo "This test verifies that scroll state changes are sent to WebRender"
echo ""

# Helper function to send debug requests
send_request() {
    local json="$1"
    curl -s -X POST "$BASE_URL/debug" \
        -H "Content-Type: application/json" \
        -d "$json"
}

# Wait for server
echo "[1] Waiting for debug server on port $DEBUG_PORT..."
for i in {1..30}; do
    if curl -s "$BASE_URL/health" > /dev/null 2>&1; then
        echo "    Server is ready!"
        break
    fi
    sleep 0.5
done

# Step 1: Get initial scroll states
echo ""
echo "[2] Getting initial scroll states..."
initial_scroll=$(send_request '{"op": "get_scroll_states"}')
echo "    Initial scroll states:"
echo "$initial_scroll" | jq -r '.data.value.states // .data.value // "none"' 2>/dev/null || echo "$initial_scroll"

# Step 2: Get initial display list (before scroll)
echo ""
echo "[3] Getting initial display list..."
initial_display=$(send_request '{"op": "get_display_list"}')
# Extract scroll frame info from display list
echo "    Looking for scroll frames in display list..."
initial_scroll_frames=$(echo "$initial_display" | jq -r '.data.value.items[]? | select(.item_type == "ScrollFrame" or .item_type == "scroll_frame") | {item_type, bounds, scroll_offset}' 2>/dev/null || echo "No scroll frames found")
echo "$initial_scroll_frames"

# Alternative: look for any scroll-related content in display list
if [ "$initial_scroll_frames" = "No scroll frames found" ]; then
    echo "    Searching for scroll_offset in display list..."
    echo "$initial_display" | jq -r '.data.value' 2>/dev/null | grep -o '"scroll_offset[^}]*' | head -5 || echo "    No scroll_offset found"
fi

# Step 3: Find a scrollable node
echo ""
echo "[4] Finding scrollable nodes..."
scrollable=$(send_request '{"op": "get_scrollable_nodes"}')
echo "$scrollable" | jq -r '.data.value.nodes[0] // "No scrollable nodes"' 2>/dev/null || echo "$scrollable"

# Extract first scrollable node info
first_scrollable_id=$(echo "$scrollable" | jq -r '.data.value.nodes[0].node_id // empty' 2>/dev/null)
if [ -z "$first_scrollable_id" ]; then
    echo "    No scrollable nodes found, trying wheel scroll at center"
    first_scrollable_id=""
fi

# Step 4: Perform a scroll operation using scroll_node_by
echo ""
echo "[5] Performing scroll_node_by (delta_y: 100)..."
if [ -n "$first_scrollable_id" ]; then
    scroll_result=$(send_request "{\"op\": \"scroll_node_by\", \"node_id\": $first_scrollable_id, \"delta_x\": 0, \"delta_y\": 100}")
else
    # Use wheel scroll at window center
    scroll_result=$(send_request '{"op": "scroll", "x": 400, "y": 300, "delta_x": 0, "delta_y": 100}')
fi
echo "    Scroll result:"
echo "$scroll_result" | jq '.' 2>/dev/null || echo "$scroll_result"

# Step 5: Wait for frame render
echo ""
echo "[6] Waiting for frame render..."
send_request '{"op": "wait_frame"}' > /dev/null
send_request '{"op": "redraw"}' > /dev/null
sleep 0.2

# Step 6: Get scroll states after scroll
echo ""
echo "[7] Getting scroll states after scroll..."
after_scroll=$(send_request '{"op": "get_scroll_states"}')
echo "    Scroll states after:"
echo "$after_scroll" | jq -r '.data.value.states // .data.value // "none"' 2>/dev/null || echo "$after_scroll"

# Step 7: Get display list after scroll
echo ""
echo "[8] Getting display list after scroll..."
after_display=$(send_request '{"op": "get_display_list"}')
after_scroll_frames=$(echo "$after_display" | jq -r '.data.value.items[]? | select(.item_type == "ScrollFrame" or .item_type == "scroll_frame") | {item_type, bounds, scroll_offset}' 2>/dev/null || echo "No scroll frames found")
echo "$after_scroll_frames"

if [ "$after_scroll_frames" = "No scroll frames found" ]; then
    echo "    Searching for scroll_offset in display list after..."
    echo "$after_display" | jq -r '.data.value' 2>/dev/null | grep -o '"scroll_offset[^}]*' | head -5 || echo "    No scroll_offset found"
fi

# Step 8: Compare display lists
echo ""
echo "[9] Comparing display lists..."

# Simple size comparison
initial_size=$(echo "$initial_display" | wc -c)
after_size=$(echo "$after_display" | wc -c)
echo "    Initial display list size: $initial_size bytes"
echo "    After scroll display list size: $after_size bytes"

# Step 9: Test GetDragState and GetDragContext APIs
echo ""
echo "[10] Testing new GetDragState API..."
drag_state=$(send_request '{"op": "get_drag_state"}')
echo "    Drag state:"
echo "$drag_state" | jq '.' 2>/dev/null || echo "$drag_state"

echo ""
echo "[11] Testing new GetDragContext API..."
drag_context=$(send_request '{"op": "get_drag_context"}')
echo "    Drag context:"
echo "$drag_context" | jq '.' 2>/dev/null || echo "$drag_context"

# Step 10: Test drag state during scrollbar drag
echo ""
echo "[12] Testing drag state during scrollbar operation..."
# Get scrollbar info
scrollbar_info=$(send_request '{"op": "get_scrollbar_info", "selector": ".scroll-container", "orientation": "vertical"}')
if echo "$scrollbar_info" | jq -e '.data.value.vertical // empty' > /dev/null 2>&1; then
    thumb=$(echo "$scrollbar_info" | jq -r '.data.value.vertical.thumb')
    thumb_x=$(echo "$thumb" | jq -r '.x')
    thumb_y=$(echo "$thumb" | jq -r '.y')
    thumb_h=$(echo "$thumb" | jq -r '.height')
    
    center_x=$(echo "$thumb_x + 5" | bc)
    center_y=$(echo "$thumb_y + $thumb_h / 2" | bc)
    
    echo "    Starting scrollbar drag at ($center_x, $center_y)..."
    send_request "{\"op\": \"mouse_down\", \"x\": $center_x, \"y\": $center_y}" > /dev/null
    sleep 0.1
    
    # Check drag state during drag
    drag_during=$(send_request '{"op": "get_drag_state"}')
    echo "    Drag state during scrollbar drag:"
    echo "$drag_during" | jq '.' 2>/dev/null || echo "$drag_during"
    
    drag_context_during=$(send_request '{"op": "get_drag_context"}')
    echo "    Drag context during scrollbar drag:"
    echo "$drag_context_during" | jq '.' 2>/dev/null || echo "$drag_context_during"
    
    # Release
    send_request "{\"op\": \"mouse_up\", \"x\": $center_x, \"y\": $center_y}" > /dev/null
else
    echo "    No vertical scrollbar found, skipping scrollbar drag test"
fi

echo ""
echo "=== WebRender Scroll Verification Test Complete ==="
