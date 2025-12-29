#!/bin/bash
# Test script for window resize via debug HTTP API
# This script opens a window, resizes it multiple times, and then closes it

set -e

PORT=8765
BASE_URL="http://127.0.0.1:$PORT"

echo "=== Azul Resize Test Script ==="
echo ""

# Start the window in background
echo "[1/6] Starting hello_world_window with AZUL_DEBUG=$PORT..."
cd "$(dirname "$0")/.."
AZUL_DEBUG=$PORT cargo run --package azul-dll --bin hello_world_window --features "build-dll" 2>&1 &
WINDOW_PID=$!

# Wait for the window to start and debug server to be ready
echo "[2/6] Waiting for window to initialize (may take time to compile)..."

# Wait for debug server to be available (up to 60 seconds)
MAX_WAIT=60
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    if curl -s --max-time 2 -X POST -H "Content-Type: application/json" -d '{"type":"get_state"}' "$BASE_URL/" > /dev/null 2>&1; then
        echo "       Debug server ready after ${WAITED}s"
        break
    fi
    sleep 2
    WAITED=$((WAITED + 2))
    echo "       Waiting... (${WAITED}s)"
done

if [ $WAITED -ge $MAX_WAIT ]; then
    echo "ERROR: Debug server not responding after ${MAX_WAIT}s"
    kill $WINDOW_PID 2>/dev/null || true
    exit 1
fi

# Check if the server is responding
echo "[3/6] Debug server connection confirmed!"

# Get initial state
echo ""
echo "[4/6] Getting initial window state..."
INITIAL_STATE=$(curl -s --max-time 5 -X POST -H "Content-Type: application/json" -d '{"type":"get_state"}' "$BASE_URL/")
echo "       Response: $INITIAL_STATE"

# Resize the window multiple times
echo ""
echo "[5/6] Resizing window..."

echo "       Resize to 800x600..."
RESIZE1=$(curl -s --max-time 5 -X POST -H "Content-Type: application/json" -d '{"type":"resize","width":800,"height":600}' "$BASE_URL/")
echo "       Response: $RESIZE1"
sleep 0.5

echo "       Resize to 1024x768..."
RESIZE2=$(curl -s --max-time 5 -X POST -H "Content-Type: application/json" -d '{"type":"resize","width":1024,"height":768}' "$BASE_URL/")
echo "       Response: $RESIZE2"
sleep 0.5

echo "       Resize to 400x300..."
RESIZE3=$(curl -s --max-time 5 -X POST -H "Content-Type: application/json" -d '{"type":"resize","width":400,"height":300}' "$BASE_URL/")
echo "       Response: $RESIZE3"
sleep 0.5

echo "       Resize back to 640x480..."
RESIZE4=$(curl -s --max-time 5 -X POST -H "Content-Type: application/json" -d '{"type":"resize","width":640,"height":480}' "$BASE_URL/")
echo "       Response: $RESIZE4"

# Wait and observe
echo ""
echo "       Waiting 3 seconds to observe window..."
sleep 3

# Get final state
echo ""
echo "[6/6] Getting final window state and closing..."
FINAL_STATE=$(curl -s --max-time 5 -X POST -H "Content-Type: application/json" -d '{"type":"get_state"}' "$BASE_URL/")
echo "       Final state: $FINAL_STATE"

# Close the window
echo ""
echo "       Sending close command..."
CLOSE_RESULT=$(curl -s --max-time 5 -X POST -H "Content-Type: application/json" -d '{"type":"close"}' "$BASE_URL/" 2>/dev/null || echo '{"note":"window closed"}')
echo "       Close response: $CLOSE_RESULT"

# Wait for process to exit
sleep 1
if kill -0 $WINDOW_PID 2>/dev/null; then
    echo "       Window still running, force killing..."
    kill $WINDOW_PID 2>/dev/null || true
fi

echo ""
echo "=== Test Complete ==="
