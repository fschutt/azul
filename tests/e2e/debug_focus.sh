#!/bin/bash
# Simple debug script for focus restyle

cd /Users/fschutt/Development/azul/tests/e2e
LOG=/tmp/focus_debug.log

# Clean up
pkill -f focus_test 2>/dev/null
rm -f $LOG
sleep 1

# Compile
cc focus.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o focus_test -Wl,-rpath,../../target/release

# Start with output to log
echo "Starting focus_test..."
AZUL_DEBUG=8765 ./focus_test > $LOG 2>&1 &
PID=$!
sleep 3

echo "=== BEFORE Tab - Display List Colors ==="
curl -s -X POST http://localhost:8765/ -d '{"op": "get_display_list"}' | jq -r '.data.value.items[] | select(.type == "rect") | "  \(.x),\(.y) -> \(.color)"'

# Send Tab
echo ""
echo "Sending Tab key..."
curl -s -X POST http://localhost:8765/ -d '{"op": "key_down", "key": "Tab"}' > /dev/null
sleep 1

# Get focus state
echo ""
echo "Focus state after Tab:"
curl -s -X POST http://localhost:8765/ -d '{"op": "get_state"}' | jq -r '.window_state.focused_node // "null"'

echo ""
echo "=== AFTER Tab - Display List Colors ==="
curl -s -X POST http://localhost:8765/ -d '{"op": "get_display_list"}' | jq -r '.data.value.items[] | select(.type == "rect") | "  \(.x),\(.y) -> \(.color)"'

# Check for focus color
echo ""
echo "=== Checking for focus color #ff6b6b ==="
if curl -s -X POST http://localhost:8765/ -d '{"op": "get_display_list"}' | grep -q "ff6b6b"; then
    echo "✅ PASS: Focus color #ff6b6b found!"
else
    echo "❌ FAIL: Focus color #ff6b6b NOT found"
fi

# Check for focus border color
echo ""
echo "=== Checking for focus border #f1c40f ==="
if curl -s -X POST http://localhost:8765/ -d '{"op": "get_display_list"}' | grep -q "f1c40f"; then
    echo "✅ PASS: Focus border color #f1c40f found!"
else
    echo "❌ FAIL: Focus border color #f1c40f NOT found"
fi

echo ""
echo "=== All borders in display list ==="
curl -s -X POST http://localhost:8765/ -d '{"op": "get_display_list"}' | jq -r '.data.value.items[] | select(.type == "border") | "  \(.x),\(.y) -> \(.color // "no color field")"' 2>/dev/null || echo "  (no borders or parse error)"

# Close
echo ""
echo "Closing..."
curl -s -X POST http://localhost:8765/ -d '{"op": "close"}' > /dev/null
sleep 1
kill $PID 2>/dev/null

echo ""
echo "=== DEBUG LOG ==="
grep -i "DEBUG" $LOG | tail -20
