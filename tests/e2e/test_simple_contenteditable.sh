#!/bin/bash
# Test script for simple contenteditable
# Usage: ./test_simple_contenteditable.sh

set -e

cd "$(dirname "$0")"

# Compile
echo "Compiling..."
cc simple_contenteditable.c -I../../examples/c -L../../target/debug -lazul -o simple_contenteditable -Wl,-rpath,../../target/debug

# Kill any existing instance
pkill -f simple_contenteditable 2>/dev/null || true
sleep 0.5

# Start with debug server
echo "Starting app with debug server on port 8765..."
AZUL_DEBUG=8765 ./simple_contenteditable 2>&1 &
APP_PID=$!

# Wait for app to start
sleep 2

# Check if app is running
if ! kill -0 $APP_PID 2>/dev/null; then
    echo "ERROR: App failed to start"
    exit 1
fi

echo "App running (PID: $APP_PID)"
echo ""

# Test sequence
echo "=== Test: Click on text ==="
curl -s -X POST http://localhost:8765/ -d '{"op": "click", "x": 200, "y": 120}'
sleep 0.5

echo ""
echo "=== Test: Type 'Hello' ==="
curl -s -X POST http://localhost:8765/ -d '{"op": "text_input", "text": "Hello"}'
sleep 0.5

echo ""
echo "=== Test: Type more text to trigger scroll ==="
curl -s -X POST http://localhost:8765/ -d '{"op": "text_input", "text": " World - this is a long line that should scroll"}'
sleep 0.5

echo ""
echo "=== Test: Type even more ==="
curl -s -X POST http://localhost:8765/ -d '{"op": "text_input", "text": " and keep scrolling to follow the cursor..."}'
sleep 1

echo ""
echo "=== Get final state ==="
curl -s -X POST http://localhost:8765/ -d '{"op": "get_state"}' | head -100

echo ""
echo "Test complete. App still running - press Ctrl+C to stop or run:"
echo "  kill $APP_PID"

# Wait for user
wait $APP_PID 2>/dev/null || true
