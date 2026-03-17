#!/bin/bash
# Test script for text selection and contenteditable
# Uses AZUL_DEBUG API to interact with the app and take screenshots
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PORT=8765
API="http://localhost:$PORT"
OUT_DIR="$SCRIPT_DIR/text-selection-screenshots"
mkdir -p "$OUT_DIR"

# Build the test binary
echo "=== Building text-selection-test ==="
cd "$SCRIPT_DIR"
LIB_DIR="$(cd ../../target/release && pwd)"
cc -o text-selection-test text-selection-test.c \
    -I. -L"$LIB_DIR" -lazul_dll \
    -Wl,-rpath,"$LIB_DIR" \
    -framework CoreFoundation -framework CoreGraphics -framework CoreText \
    -framework AppKit -framework Security
echo "Built successfully."

# Start the app in background with debug server
echo "=== Starting app with AZUL_DEBUG=$PORT ==="
AZUL_DEBUG=$PORT ./text-selection-test &
APP_PID=$!
sleep 2

# Helper to take a screenshot and save it
take_screenshot() {
    local name="$1"
    local file="$OUT_DIR/${name}.png"
    echo "  [screenshot] $name"
    curl -s -X POST "$API/" -d '{"op": "take_screenshot"}' | python3 -c "
import sys, json, base64
resp = json.load(sys.stdin)
if resp.get('status') == 'ok' and resp.get('data', {}).get('screenshot'):
    with open('$file', 'wb') as f:
        f.write(base64.b64decode(resp['data']['screenshot']))
    print('    saved: $file')
else:
    print('    ERROR: ' + json.dumps(resp))
"
}

# Helper to send a command and print result
send() {
    local desc="$1"
    local json="$2"
    echo "  [send] $desc"
    local result
    result=$(curl -s -X POST "$API/" -d "$json")
    echo "    status: $(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','?'))" 2>/dev/null || echo "parse error")"
}

cleanup() {
    echo "=== Cleaning up ==="
    kill $APP_PID 2>/dev/null || true
    wait $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

echo ""
echo "=== Test 1: Initial state ==="
take_screenshot "01_initial"

echo ""
echo "=== Test 2: Get DOM structure ==="
curl -s -X POST "$API/" -d '{"op": "get_dom_tree"}' | python3 -c "
import sys, json
resp = json.load(sys.stdin)
if resp.get('status') == 'ok':
    print('  DOM tree received, nodes:', len(json.dumps(resp.get('data', {}))))
else:
    print('  ERROR:', resp.get('message', 'unknown'))
"

echo ""
echo "=== Test 3: Click on selectable text (should place cursor) ==="
send "click on text" '{"op": "click", "x": 100, "y": 40}'
sleep 0.5
take_screenshot "02_after_click_text"

echo ""
echo "=== Test 4: Check selection state ==="
curl -s -X POST "$API/" -d '{"op": "get_selection_state"}' | python3 -m json.tool 2>/dev/null || echo "  (no selection state)"

echo ""
echo "=== Test 5: Select text by dragging ==="
send "mouse_down" '{"op": "mouse_down", "x": 50, "y": 40, "button": "left"}'
send "mouse_move" '{"op": "mouse_move", "x": 200, "y": 40}'
send "mouse_up" '{"op": "mouse_up", "x": 200, "y": 40, "button": "left"}'
sleep 0.5
take_screenshot "03_after_drag_select"

echo ""
echo "=== Test 6: Check selection state after drag ==="
curl -s -X POST "$API/" -d '{"op": "get_selection_state"}' | python3 -m json.tool 2>/dev/null || echo "  (no selection state)"

echo ""
echo "=== Test 7: Click on contenteditable div ==="
send "click on editable" '{"op": "click", "x": 100, "y": 200}'
sleep 0.5
take_screenshot "04_after_click_editable"

echo ""
echo "=== Test 8: Check selection state in editable ==="
curl -s -X POST "$API/" -d '{"op": "get_selection_state"}' | python3 -m json.tool 2>/dev/null || echo "  (no selection state)"

echo ""
echo "=== Test 9: Type text into contenteditable ==="
send "text input" '{"op": "text_input", "text": "HELLO "}'
sleep 0.5
take_screenshot "05_after_typing"

echo ""
echo "=== Test 10: Check DOM after typing ==="
curl -s -X POST "$API/" -d '{"op": "get_html_string"}' | python3 -c "
import sys, json
resp = json.load(sys.stdin)
if resp.get('status') == 'ok':
    html = resp.get('data', {}).get('value', '')
    # Print first 500 chars
    print('  HTML (first 500 chars):')
    print('  ', html[:500] if isinstance(html, str) else str(html)[:500])
else:
    print('  ERROR:', resp.get('message', 'unknown'))
" 2>/dev/null || echo "  (failed to get HTML)"

echo ""
echo "=== Test 11: Double-click to select a word ==="
send "double-click" '{"op": "double_click", "x": 100, "y": 200}'
sleep 0.5
take_screenshot "06_after_double_click"

echo ""
echo "=== Test 12: Selection state after double-click ==="
curl -s -X POST "$API/" -d '{"op": "get_selection_state"}' | python3 -m json.tool 2>/dev/null || echo "  (no selection state)"

echo ""
echo "=== Test 13: Select all with Ctrl+A ==="
send "ctrl+a" '{"op": "key_down", "key": "a", "modifiers": {"meta": true}}'
sleep 0.5
take_screenshot "07_after_select_all"
curl -s -X POST "$API/" -d '{"op": "get_selection_state"}' | python3 -m json.tool 2>/dev/null || echo "  (no selection state)"

echo ""
echo "=== Test 14: Delete selected text with Backspace ==="
send "backspace" '{"op": "key_down", "key": "Backspace"}'
sleep 0.5
take_screenshot "08_after_delete"

echo ""
echo "=== Test 15: Get logs ==="
curl -s -X POST "$API/" -d '{"op": "get_logs"}' | python3 -c "
import sys, json
resp = json.load(sys.stdin)
logs = resp.get('data', {}).get('value', {}).get('logs', [])
print(f'  Total log entries: {len(logs)}')
for log in logs[-20:]:
    print(f'  [{log.get(\"level\",\"?\")}] {log.get(\"message\",\"\")[:120]}')
" 2>/dev/null || echo "  (failed to get logs)"

echo ""
echo "=== Done. Screenshots in: $OUT_DIR ==="
ls -la "$OUT_DIR/"
