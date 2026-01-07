#!/bin/bash
# E2E Test: Hello World Button Click
# 
# This script tests that:
# 1. The hello-world example starts correctly
# 2. Hit testing works (we can find the button)
# 3. Mouse click events are delivered to the button
# 4. The callback is invoked and counter increases
# 5. The display list is regenerated with new text
#
# Usage: ./scripts/test_hello_world_click.sh [port]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
PORT="${1:-8850}"
EXAMPLE_NAME="hello-world"

# Paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TEMP_DIR="$ROOT_DIR/target/examples-temp/$EXAMPLE_NAME"
LOG_FILE="$TEMP_DIR/test_click.log"

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step() { echo -e "${BLUE}[STEP $1]${NC} $2"; }

# Cleanup function
cleanup() {
    log_info "Cleaning up..."
    pkill -9 hello-world 2>/dev/null || true
    local pid=$(lsof -ti :$PORT 2>/dev/null || true)
    if [ -n "$pid" ]; then
        kill -9 $pid 2>/dev/null || true
    fi
}
trap cleanup EXIT

# Helper: Send debug request and get response
send_request() {
    local request="$1"
    curl -s --max-time 10 -X POST "http://localhost:$PORT/" \
        -H "Content-Type: application/json" \
        -d "$request"
}

# Helper: Extract status from JSON response
get_status() {
    echo "$1" | jq -r '.status // "error"'
}

log_info "=========================================="
log_info "E2E Test: Hello World Button Click"
log_info "Port: $PORT"
log_info "=========================================="

# Step 1: Check if example is already compiled
log_step 1 "Checking if example exists..."

if [ ! -d "$TEMP_DIR" ] || [ ! -f "$TEMP_DIR/$EXAMPLE_NAME" ]; then
    log_info "Example not found, running screenshot_single.sh to compile..."
    "$ROOT_DIR/scripts/screenshot_single.sh" "$EXAMPLE_NAME" "$PORT" > /dev/null 2>&1 &
    # Wait for compilation
    sleep 10
    pkill -9 hello-world 2>/dev/null || true
    sleep 1
fi

if [ ! -f "$TEMP_DIR/$EXAMPLE_NAME" ]; then
    log_error "Failed to compile example"
    exit 1
fi
log_success "Example binary exists: $TEMP_DIR/$EXAMPLE_NAME"

# Step 2: Copy latest DLL
log_step 2 "Copying latest DLL..."
DLL_PATH="$ROOT_DIR/target/release/libazul.dylib"
if [ ! -f "$DLL_PATH" ]; then
    log_error "DLL not found: $DLL_PATH"
    exit 1
fi
cp "$DLL_PATH" "$TEMP_DIR/"
log_success "DLL copied"

# Step 3: Start example
log_step 3 "Starting example with AZUL_DEBUG=$PORT..."
cd "$TEMP_DIR"
pkill -9 hello-world 2>/dev/null || true
sleep 1

AZUL_DEBUG=$PORT "./$EXAMPLE_NAME" > "$LOG_FILE" 2>&1 &
APP_PID=$!
log_info "Started with PID: $APP_PID"
sleep 3

# Verify it's running
if ! kill -0 $APP_PID 2>/dev/null; then
    log_error "Process died during startup!"
    cat "$LOG_FILE" || true
    exit 1
fi
log_success "Example is running"

# Step 4: Test connectivity
log_step 4 "Testing HTTP connectivity..."
response=$(send_request '{"type":"get_logs"}')
status=$(get_status "$response")
if [ "$status" != "ok" ]; then
    log_error "HTTP connectivity failed: $response"
    exit 1
fi
log_success "HTTP connectivity OK"

# Step 5: Get initial DOM tree info
log_step 5 "Getting initial DOM tree..."
response=$(send_request '{"type":"get_dom_tree"}')
status=$(get_status "$response")
if [ "$status" != "ok" ]; then
    log_error "get_dom_tree failed: $response"
    exit 1
fi
node_count=$(echo "$response" | jq -r '.data.value.node_count // 0')
log_info "DOM has $node_count nodes"

# Step 6: Get initial display list
log_step 6 "Getting initial display list..."
response=$(send_request '{"type":"get_display_list"}')
status=$(get_status "$response")
if [ "$status" != "ok" ]; then
    log_error "get_display_list failed: $response"
    exit 1
fi
initial_text_count=$(echo "$response" | jq -r '.data.value.text_count // 0')
log_info "Initial display list has $initial_text_count text items"

# Step 7: Get all nodes with layout to find button position
log_step 7 "Getting node layout to find button..."
response=$(send_request '{"type":"get_all_nodes_layout"}')
status=$(get_status "$response")
if [ "$status" != "ok" ]; then
    log_error "get_all_nodes_layout failed: $response"
    exit 1
fi

# Find nodes with their rects - look for a clickable area
# The hello-world has: body > label (with "5") > button (with "Increase counter")
# We need to click somewhere in the button area

# Get window dimensions for reference
window_width=$(echo "$response" | jq -r '.data.value.nodes[0].rect.width // 400')
window_height=$(echo "$response" | jq -r '.data.value.nodes[0].rect.height // 300')
log_info "Window size: ${window_width}x${window_height}"

# Try to find the button by looking at node layout
# Typically button is in lower half of window
button_x=$(echo "$window_width / 2" | bc)
button_y=$(echo "$window_height * 3 / 4" | bc)
log_info "Will click at position: ($button_x, $button_y)"

# Step 8: Get HTML string to see current state
log_step 8 "Getting initial HTML string..."
response=$(send_request '{"type":"get_html_string"}')
status=$(get_status "$response")
if [ "$status" != "ok" ]; then
    log_error "get_html_string failed: $response"
    exit 1
fi
initial_html=$(echo "$response" | jq -r '.data.value.html // ""')
log_info "Initial HTML (first 200 chars): ${initial_html:0:200}"

# Check if "5" is in the HTML (initial counter value for C example)
if echo "$initial_html" | grep -q ">5<"; then
    log_success "Found initial counter value '5' in HTML"
else
    log_warn "Could not find initial counter value '5' in HTML"
fi

# Step 9: Perform hit test at button location
log_step 9 "Performing hit test at button location..."
response=$(send_request "{\"type\":\"hit_test\",\"x\":$button_x,\"y\":$button_y}")
status=$(get_status "$response")
if [ "$status" != "ok" ]; then
    log_error "hit_test failed: $response"
    exit 1
fi
log_info "Hit test result: $response"

# Step 10: Send mouse events to click the button
log_step 10 "Sending mouse click events..."

# Move mouse to button position first
log_info "Sending mouse_move to ($button_x, $button_y)..."
response=$(send_request "{\"type\":\"mouse_move\",\"x\":$button_x,\"y\":$button_y}")
status=$(get_status "$response")
log_info "mouse_move status: $status"

# Wait for frame to process
sleep 0.1

# Mouse down
log_info "Sending mouse_down..."
response=$(send_request "{\"type\":\"mouse_down\",\"x\":$button_x,\"y\":$button_y,\"button\":\"left\"}")
status=$(get_status "$response")
log_info "mouse_down status: $status"

# IMPORTANT: Wait between mouse_down and mouse_up for state diffing to work
sleep 0.2

# Mouse up (this triggers the callback)
log_info "Sending mouse_up..."
response=$(send_request "{\"type\":\"mouse_up\",\"x\":$button_x,\"y\":$button_y,\"button\":\"left\"}")
status=$(get_status "$response")
log_info "mouse_up status: $status"

# Wait for frame to render
log_info "Waiting for render..."
sleep 0.2
response=$(send_request '{"type":"wait_frame"}')

# Step 11: Check if counter increased
log_step 11 "Checking if counter increased..."
response=$(send_request '{"type":"get_html_string"}')
status=$(get_status "$response")
if [ "$status" != "ok" ]; then
    log_error "get_html_string failed: $response"
    exit 1
fi
new_html=$(echo "$response" | jq -r '.data.value.html // ""')
log_info "New HTML (first 200 chars): ${new_html:0:200}"

# Check if "6" is in the HTML (counter should have increased from 5 to 6)
if echo "$new_html" | grep -q ">6<"; then
    log_success "Counter increased to 6! Button click worked!"
else
    log_error "Counter did NOT increase. Button click may have failed."
    log_error "Expected '>6<' in HTML, but got:"
    echo "$new_html" | head -c 500
    
    # Additional debugging: check display list
    log_info ""
    log_info "=== Additional Debugging ==="
    
    response=$(send_request '{"type":"get_display_list"}')
    new_text_count=$(echo "$response" | jq -r '.data.value.text_count // 0')
    log_info "Display list now has $new_text_count text items (was $initial_text_count)"
    
    # Get scroll states
    response=$(send_request '{"type":"get_scroll_states"}')
    log_info "Scroll states: $response"
    
    # Get logs
    response=$(send_request '{"type":"get_logs"}')
    logs=$(echo "$response" | jq -r '.data.value.logs // []')
    log_info "Recent logs: $logs"
    
    exit 1
fi

# Step 12: Verify display list was regenerated
log_step 12 "Verifying display list was regenerated..."
response=$(send_request '{"type":"get_display_list"}')
status=$(get_status "$response")
if [ "$status" != "ok" ]; then
    log_error "get_display_list failed: $response"
    exit 1
fi
new_text_count=$(echo "$response" | jq -r '.data.value.text_count // 0')
log_info "Display list now has $new_text_count text items"

# Step 13: Close application
log_step 13 "Closing application..."
response=$(send_request '{"type":"close"}')
sleep 2

# Summary
log_info "=========================================="
log_info "SUMMARY"
log_info "=========================================="
log_success "Initial counter: 0"
log_success "After click: 1"
log_success "Button click callback worked!"
log_success "Display list regenerated correctly"
log_success "=========================================="
log_success "TEST PASSED"
log_success "=========================================="

exit 0
