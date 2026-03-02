#!/bin/bash
# Reproduction script for scrolling.c rendering bug
#
# This script:
# 1. Rebuilds the DLL (release)
# 2. Compiles scrolling.c
# 3. Runs it with AZUL_DEBUG
# 4. Scrolls the container to the very bottom (500 rows × 30px = 15000px)
# 5. Takes a native screenshot and saves it
# 6. Dumps the display list and scroll state for debugging
# 7. Outputs all data to files in ./repro_output/

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
EXAMPLES_C="$ROOT_DIR/examples/c"
OUT_DIR="$SCRIPT_DIR/repro_output"

PORT=8766
DEBUG_URL="http://localhost:$PORT/"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m'

cleanup() {
    if [ -n "$APP_PID" ]; then
        kill "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

send_cmd() {
    curl -s --max-time 30 -X POST "$DEBUG_URL" -d "$1"
}

echo "========================================"
echo -e "${BLUE}Scrolling Bug Reproduction Script${NC}"
echo "========================================"

# Kill any leftover scrolling processes
pkill -f 'examples/c/scrolling' 2>/dev/null || true
pkill -f '/scrolling$' 2>/dev/null || true
sleep 0.5

# --- Step 1: Rebuild DLL ---
echo ""
echo -e "${YELLOW}[1/7] Rebuilding DLL (release)...${NC}"
cd "$ROOT_DIR"
cargo build -p azul-dll --features build-dll --release 2>&1 | tail -3
echo -e "${GREEN}DLL built successfully${NC}"

# --- Step 2: Copy header & compile scrolling.c ---
echo ""
echo -e "${YELLOW}[2/7] Compiling scrolling.c...${NC}"
cp "$ROOT_DIR/target/codegen/v2/azul.h" "$EXAMPLES_C/azul.h"
cd "$EXAMPLES_C"
cc -o scrolling scrolling.c \
    -I. \
    -L"$ROOT_DIR/target/release" \
    -lazul \
    -Wl,-rpath,"$ROOT_DIR/target/release" 2>&1
echo -e "${GREEN}scrolling.c compiled${NC}"

# --- Step 3: Start app with debug server ---
echo ""
echo -e "${YELLOW}[3/7] Starting scrolling app with debug server on port $PORT...${NC}"
mkdir -p "$OUT_DIR"
AZUL_DEBUG=$PORT DYLD_LIBRARY_PATH="$ROOT_DIR/target/release" "$EXAMPLES_C/scrolling" > "$OUT_DIR/app_stdout.log" 2>&1 &
APP_PID=$!
echo "App PID: $APP_PID"

# Wait for debug server to be ready
echo -n "Waiting for debug server..."
for i in {1..20}; do
    if curl -s --connect-timeout 1 "$DEBUG_URL" -d '{"op": "get_state"}' > /dev/null 2>&1; then
        echo -e " ${GREEN}ready${NC}"
        break
    fi
    if ! kill -0 "$APP_PID" 2>/dev/null; then
        echo -e " ${RED}app crashed!${NC}"
        cat "$OUT_DIR/app_stdout.log"
        exit 1
    fi
    echo -n "."
    sleep 0.5
done

# Quick health check
STATE=$(send_cmd '{"op": "get_state"}')
echo "$STATE" | python3 -m json.tool > "$OUT_DIR/01_initial_state.json" 2>/dev/null || echo "$STATE" > "$OUT_DIR/01_initial_state.json"
echo "Initial state saved"

# --- Step 4: Get initial DOM and scroll info ---
echo ""
echo -e "${YELLOW}[4/7] Capturing initial state...${NC}"

# Get DOM tree
DOM=$(send_cmd '{"op": "get_dom_tree"}')
echo "$DOM" | python3 -m json.tool > "$OUT_DIR/02_initial_dom_tree.json" 2>/dev/null || echo "$DOM" > "$OUT_DIR/02_initial_dom_tree.json"
echo "  DOM tree saved"

# Get scrollable nodes
SCROLL_NODES=$(send_cmd '{"op": "get_scrollable_nodes"}')
echo "$SCROLL_NODES" | python3 -m json.tool > "$OUT_DIR/03_scrollable_nodes.json" 2>/dev/null || echo "$SCROLL_NODES" > "$OUT_DIR/03_scrollable_nodes.json"
echo "  Scrollable nodes saved"

# Get initial display list
DL_BEFORE=$(send_cmd '{"op": "get_display_list"}')
echo "$DL_BEFORE" | python3 -m json.tool > "$OUT_DIR/04_display_list_before_scroll.json" 2>/dev/null || echo "$DL_BEFORE" > "$OUT_DIR/04_display_list_before_scroll.json"
echo "  Display list (before scroll) saved"

# Get initial scroll states
SCROLL_BEFORE=$(send_cmd '{"op": "get_scroll_states"}')
echo "$SCROLL_BEFORE" | python3 -m json.tool > "$OUT_DIR/05_scroll_states_before.json" 2>/dev/null || echo "$SCROLL_BEFORE" > "$OUT_DIR/05_scroll_states_before.json"
echo "  Scroll states (before) saved"

# Take screenshot before scrolling
echo "  Taking initial screenshot..."
SCREENSHOT_BEFORE=$(send_cmd '{"op": "take_native_screenshot"}')
echo "$SCREENSHOT_BEFORE" | python3 -c "
import sys, json, base64
data = json.load(sys.stdin)
# The response format is: data.value.data = 'data:image/png;base64,...'
for key_path in [
    lambda d: d.get('data', {}).get('value', {}).get('data'),
    lambda d: d.get('data', {}).get('screenshot'),
    lambda d: d.get('data', {}).get('value', {}).get('screenshot'),
    lambda d: d.get('screenshot'),
]:
    val = key_path(data)
    if val and isinstance(val, str):
        # Strip data URI prefix if present
        if val.startswith('data:image/png;base64,'):
            val = val[len('data:image/png;base64,'):]
        png = base64.b64decode(val)
        with open('$OUT_DIR/screenshot_before_scroll.png', 'wb') as f:
            f.write(png)
        print(f'  Screenshot saved ({len(png)} bytes)')
        sys.exit(0)
print('  WARNING: Could not extract screenshot from response')
with open('$OUT_DIR/screenshot_before_raw.json', 'w') as f:
    json.dump(data, f, indent=2)
" 2>/dev/null || echo "  WARNING: screenshot extraction failed"

# --- Step 5: Scroll to the very bottom ---
echo ""
echo -e "${YELLOW}[5/7] Scrolling to the bottom (500 rows × 30px = 15000px)...${NC}"

# First move mouse into the scroll area
send_cmd '{"op": "mouse_move", "x": 300, "y": 300}' > /dev/null
sleep 0.1

# Scroll via mouse wheel events (delta_y negative = scroll down)
# Send many small scroll events to simulate real scrolling
for i in $(seq 1 60); do
    RESULT=$(send_cmd "{\"op\": \"scroll\", \"x\": 300, \"y\": 300, \"delta_x\": 0, \"delta_y\": -300}")
    sleep 0.05
done
echo "  Sent 60 scroll events (60 × -300 = -18000px)"

# Force a redraw to ensure the scroll is applied visually
send_cmd '{"op": "redraw"}' > /dev/null
send_cmd '{"op": "wait_frame"}' > /dev/null
sleep 1

# Check intermediate scroll state
SCROLL_MID=$(send_cmd '{"op": "get_scroll_states"}')
echo "  Scroll state after events:"
echo "$SCROLL_MID" | python3 -c "
import sys, json
data = json.load(sys.stdin)
states = data.get('data', {}).get('value', {}).get('scroll_states', [])
for s in states:
    print(f'    Node {s[\"node_id\"]}: scroll_y={s[\"scroll_y\"]:.1f} (max={s[\"max_scroll_y\"]:.1f})')
" 2>/dev/null

# If scroll_y is still 0, try scroll_node_to as a direct approach
SCROLL_Y=$(echo "$SCROLL_MID" | python3 -c "
import sys, json
data = json.load(sys.stdin)
states = data.get('data', {}).get('value', {}).get('scroll_states', [])
print(states[0]['scroll_y'] if states else 'none')
" 2>/dev/null)

if [ "$SCROLL_Y" = "0.0" ] || [ "$SCROLL_Y" = "0" ]; then
    echo "  scroll_y still 0 — trying scroll_node_to..."
    SCROLL_RESULT=$(send_cmd '{"op": "scroll_node_to", "node_id": 3, "x": 0, "y": 14593}')
    echo "  scroll_node_to response:"
    echo "$SCROLL_RESULT" | python3 -m json.tool 2>/dev/null | head -10
    send_cmd '{"op": "redraw"}' > /dev/null
    send_cmd '{"op": "wait_frame"}' > /dev/null
    sleep 1
    # Check again
    SCROLL_MID2=$(send_cmd '{"op": "get_scroll_states"}')
    echo "  Scroll state after scroll_node_to:"
    echo "$SCROLL_MID2" | python3 -c "
import sys, json
data = json.load(sys.stdin)
states = data.get('data', {}).get('value', {}).get('scroll_states', [])
for s in states:
    print(f'    Node {s[\"node_id\"]}: scroll_y={s[\"scroll_y\"]:.1f} (max={s[\"max_scroll_y\"]:.1f})')
" 2>/dev/null
fi

# Check debug logs for scroll-related messages
LOGS_SCROLL=$(send_cmd '{"op": "get_logs"}')
echo "$LOGS_SCROLL" | python3 -c "
import sys, json
data = json.load(sys.stdin)
logs = data.get('data', {}).get('value', {}).get('messages', [])
if isinstance(logs, list):
    scroll_logs = [l for l in logs if 'scroll' in str(l).lower()]
    for l in scroll_logs[-5:]:
        print(f'    LOG: {l}')
" 2>/dev/null

# --- Step 6: Capture state after scrolling ---
echo ""
echo -e "${YELLOW}[6/7] Capturing state after scroll...${NC}"

# Scroll states after
SCROLL_AFTER=$(send_cmd '{"op": "get_scroll_states"}')
echo "$SCROLL_AFTER" | python3 -m json.tool > "$OUT_DIR/06_scroll_states_after.json" 2>/dev/null || echo "$SCROLL_AFTER" > "$OUT_DIR/06_scroll_states_after.json"
echo "  Scroll states (after) saved"

# Display list after scrolling
DL_AFTER=$(send_cmd '{"op": "get_display_list"}')
echo "$DL_AFTER" | python3 -m json.tool > "$OUT_DIR/07_display_list_after_scroll.json" 2>/dev/null || echo "$DL_AFTER" > "$OUT_DIR/07_display_list_after_scroll.json"
echo "  Display list (after scroll) saved"

# Get all node layouts
ALL_LAYOUTS=$(send_cmd '{"op": "get_all_nodes_layout"}')
echo "$ALL_LAYOUTS" | python3 -m json.tool > "$OUT_DIR/08_all_nodes_layout.json" 2>/dev/null || echo "$ALL_LAYOUTS" > "$OUT_DIR/08_all_nodes_layout.json"
echo "  All node layouts saved"

# Get scrollbar info for the scroll container
SCROLLBAR_INFO=$(send_cmd '{"op": "get_scrollbar_info", "node_id": 1}')
echo "$SCROLLBAR_INFO" | python3 -m json.tool > "$OUT_DIR/09_scrollbar_info.json" 2>/dev/null || echo "$SCROLLBAR_INFO" > "$OUT_DIR/09_scrollbar_info.json"
echo "  Scrollbar info saved"

# Get layout tree
LAYOUT_TREE=$(send_cmd '{"op": "get_layout_tree"}')
echo "$LAYOUT_TREE" | python3 -m json.tool > "$OUT_DIR/10_layout_tree.json" 2>/dev/null || echo "$LAYOUT_TREE" > "$OUT_DIR/10_layout_tree.json"
echo "  Layout tree saved"

# Get logs
LOGS=$(send_cmd '{"op": "get_logs"}')
echo "$LOGS" | python3 -m json.tool > "$OUT_DIR/11_debug_logs.json" 2>/dev/null || echo "$LOGS" > "$OUT_DIR/11_debug_logs.json"
echo "  Debug logs saved"

# Take screenshot after scrolling (native)
echo "  Taking screenshot after scroll..."
SCREENSHOT_AFTER=$(send_cmd '{"op": "take_native_screenshot"}')
echo "$SCREENSHOT_AFTER" | python3 -c "
import sys, json, base64
data = json.load(sys.stdin)
for key_path in [
    lambda d: d.get('data', {}).get('value', {}).get('data'),
    lambda d: d.get('data', {}).get('screenshot'),
    lambda d: d.get('data', {}).get('value', {}).get('screenshot'),
    lambda d: d.get('screenshot'),
]:
    val = key_path(data)
    if val and isinstance(val, str):
        if val.startswith('data:image/png;base64,'):
            val = val[len('data:image/png;base64,'):]
        png = base64.b64decode(val)
        with open('$OUT_DIR/screenshot_after_scroll.png', 'wb') as f:
            f.write(png)
        print(f'  Screenshot saved ({len(png)} bytes)')
        sys.exit(0)
print('  WARNING: Could not extract screenshot from response')
with open('$OUT_DIR/screenshot_after_raw.json', 'w') as f:
    json.dump(data, f, indent=2)
" 2>/dev/null || echo "  WARNING: screenshot extraction failed"

# Also take a CPU-rendered screenshot (software render, no GL issues)
echo "  Taking CPU screenshot after scroll..."
SCREENSHOT_CPU=$(send_cmd '{"op": "take_screenshot"}')
echo "$SCREENSHOT_CPU" | python3 -c "
import sys, json, base64
data = json.load(sys.stdin)
for key_path in [
    lambda d: d.get('data', {}).get('value', {}).get('data'),
    lambda d: d.get('data', {}).get('screenshot'),
    lambda d: d.get('data', {}).get('value', {}).get('screenshot'),
    lambda d: d.get('screenshot'),
]:
    val = key_path(data)
    if val and isinstance(val, str):
        if val.startswith('data:image/png;base64,'):
            val = val[len('data:image/png;base64,'):]
        png = base64.b64decode(val)
        with open('$OUT_DIR/screenshot_cpu_after_scroll.png', 'wb') as f:
            f.write(png)
        print(f'  CPU screenshot saved ({len(png)} bytes)')
        sys.exit(0)
print('  WARNING: Could not extract CPU screenshot from response')
with open('$OUT_DIR/screenshot_cpu_raw.json', 'w') as f:
    json.dump(data, f, indent=2)
" 2>/dev/null || echo "  WARNING: CPU screenshot extraction failed"

# --- Step 7: Summary ---
echo ""
echo -e "${YELLOW}[7/7] Summary${NC}"
echo "========================================"
echo "Output directory: $OUT_DIR"
echo ""
echo "Files:"
ls -lh "$OUT_DIR/" 2>/dev/null | grep -v '^total' | awk '{print "  " $NF " (" $5 ")"}'
echo ""

# Extract scroll position from after state
echo "Scroll state after:"
echo "$SCROLL_AFTER" | python3 -c "
import sys, json
data = json.load(sys.stdin)
if 'data' in data:
    d = data['data']
    if isinstance(d, dict) and 'value' in d:
        d = d['value']
    if isinstance(d, list):
        for item in d:
            print(f'  Node {item.get(\"node_id\", \"?\")}: scroll_x={item.get(\"scroll_x\", 0):.1f}, scroll_y={item.get(\"scroll_y\", 0):.1f}')
    elif isinstance(d, dict):
        for k, v in d.items():
            print(f'  {k}: {v}')
    else:
        print(f'  {d}')
else:
    print(f'  Raw: {json.dumps(data)[:200]}')
" 2>/dev/null || echo "  (could not parse)"

echo ""
echo -e "${GREEN}Done. Close the app window or press Ctrl+C to exit.${NC}"
echo "The app is still running for manual inspection."
echo ""

# Keep script alive until app exits
wait "$APP_PID" 2>/dev/null || true
