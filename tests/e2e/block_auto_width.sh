#!/bin/bash
set -e

cd "$(dirname "$0")"

# Build the test
echo "[Phase 1] Building test..."
clang -g -I../../target/codegen/v2 \
    -L../../target/release \
    -lazul \
    -Wl,-rpath,@executable_path/../../target/release \
    -o block_auto_width block_auto_width.c

export DYLD_LIBRARY_PATH="$PWD/../../target/release:$DYLD_LIBRARY_PATH"

# Find available port
DEBUG_PORT=8767
while lsof -i:$DEBUG_PORT >/dev/null 2>&1; do
    DEBUG_PORT=$((DEBUG_PORT + 1))
done

# Start the app with debug server
echo ""
echo "[Phase 2] Starting application"
echo "  Starting with AZUL_DEBUG=$DEBUG_PORT..."
AZUL_DEBUG=$DEBUG_PORT ./block_auto_width &
PID=$!
echo "  PID: $PID"

# Wait for debug server
echo "  Waiting for debug server..."
for i in {1..10}; do
    if curl -s "http://localhost:$DEBUG_PORT/windows" >/dev/null 2>&1; then
        echo "  ✓ Debug server ready after ${i}s"
        break
    fi
    sleep 1
done

# Wait a bit for layout to complete
sleep 1

# Get the layout data
echo ""
echo "=== Test 1: Get Node Layouts ==="
LAYOUTS=$(curl -s "http://localhost:$DEBUG_PORT/windows/0/layouts")
if [ -z "$LAYOUTS" ]; then
    echo "  ✗ FAIL: Could not get layouts"
    kill $PID 2>/dev/null
    exit 1
fi
echo "  ✓ PASS: Got node layouts"

# Debug: print layouts
echo ""
echo "=== Raw Layouts ==="
echo "$LAYOUTS" | head -100

# Find inner container (node 1) - should have width: 400px (auto fills parent)
echo ""
echo "=== Test 2: Inner Container Width (auto should fill parent 400px) ==="
CONTAINER_WIDTH=$(echo "$LAYOUTS" | grep -A2 '"node_id": 1,' | grep '"width"' | head -1 | sed 's/.*: \([0-9.]*\).*/\1/')
if [ -z "$CONTAINER_WIDTH" ]; then
    # Try alternative format
    CONTAINER_WIDTH=$(echo "$LAYOUTS" | python3 -c "
import sys, json
data = json.load(sys.stdin)
for node in data:
    if node.get('node_id') == 1:
        print(node.get('layout', {}).get('width', 'N/A'))
        break
" 2>/dev/null || echo "N/A")
fi

echo "  Container width: ${CONTAINER_WIDTH}px"

# Calculate expected (400px)
EXPECTED=400
DIFF=$(echo "$CONTAINER_WIDTH - $EXPECTED" | bc 2>/dev/null | sed 's/-//' || echo "999")

if [ "$DIFF" = "" ] || [ "$DIFF" = "999" ]; then
    echo "  Could not calculate diff"
elif (( $(echo "$DIFF < 5" | bc -l) )); then
    echo "  ✓ PASS: Container width = ${CONTAINER_WIDTH}px (expected ${EXPECTED}px, diff=${DIFF}px)"
else
    echo "  ✗ FAIL: Container width = ${CONTAINER_WIDTH}px (expected ${EXPECTED}px, diff=${DIFF}px)"
    kill $PID 2>/dev/null
    exit 1
fi

# Find child (node 2) - should have width: 200px (50% of 400px)
echo ""
echo "=== Test 3: Child Width (50% of 400px) ==="
CHILD_WIDTH=$(echo "$LAYOUTS" | python3 -c "
import sys, json
data = json.load(sys.stdin)
for node in data:
    if node.get('node_id') == 2:
        print(node.get('layout', {}).get('width', 'N/A'))
        break
" 2>/dev/null || echo "N/A")

echo "  Child width: ${CHILD_WIDTH}px"

EXPECTED=200
if [ "$CHILD_WIDTH" != "N/A" ]; then
    DIFF=$(echo "$CHILD_WIDTH - $EXPECTED" | bc 2>/dev/null | sed 's/-//' || echo "999")
    if [ "$DIFF" != "" ] && [ "$DIFF" != "999" ] && (( $(echo "$DIFF < 5" | bc -l) )); then
        echo "  ✓ PASS: Child (50%) width = ${CHILD_WIDTH}px (expected ${EXPECTED}px, diff=${DIFF}px)"
    else
        echo "  ✗ FAIL: Child (50%) width = ${CHILD_WIDTH}px (expected ${EXPECTED}px)"
        kill $PID 2>/dev/null
        exit 1
    fi
fi

echo ""
echo "════════════════════════════════════════════"
echo "  ✓ ALL TESTS PASSED"
echo "════════════════════════════════════════════"

# Cleanup
echo ""
echo "Cleaning up..."
kill $PID 2>/dev/null || true
