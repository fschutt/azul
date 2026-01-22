#!/bin/bash
# Test Tab wrap-around

cd /Users/fschutt/Development/azul/tests/e2e
LOG=/tmp/focus_debug.log

pkill -f focus_test 2>/dev/null
sleep 1

cc focus.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o focus_test -Wl,-rpath,../../target/release

echo "Starting focus_test..."
AZUL_DEBUG=8765 ./focus_test > $LOG 2>&1 &
PID=$!
sleep 3

echo "=== Testing Tab Wrap-Around ==="

# Press Tab 4 times to cycle through all boxes and wrap around
# Note: Must send key_up after key_down for the next key press to be recognized
for i in 1 2 3 4; do
    curl -s -X POST http://localhost:8765/ -d '{"op": "key_down", "key": "Tab"}' > /dev/null
    sleep 0.1
    curl -s -X POST http://localhost:8765/ -d '{"op": "key_up", "key": "Tab"}' > /dev/null
    sleep 0.3
    FOCUS=$(curl -s -X POST http://localhost:8765/ -d '{"op": "get_state"}' | jq -r '.window_state.focused_node // "null"')
    echo "After Tab #$i: focused_node = $FOCUS"
done

echo ""
echo "Expected: Tab 1->2, 2->3, 3->4, 4->2 (wrap around)"

kill $PID 2>/dev/null
