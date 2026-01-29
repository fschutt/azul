#!/bin/bash
# Simple contenteditable debug test script
# Redirects all output to a file, closes window at the end

set -e

cd "$(dirname "$0")"

LOG_FILE="/tmp/ce_debug_$(date +%s).log"
API="http://127.0.0.1:8765"

{
    echo "=== Simple Contenteditable Debug Test ==="
    echo "Started at: $(date)"
    echo ""

    # Kill any existing instance
    pkill -f simple_contenteditable 2>/dev/null || true
    sleep 0.5

    # Rebuild the test binary
    echo "=== Building test binary ==="
    clang -o simple_contenteditable simple_contenteditable.c \
        -I../../target/release -L../../target/release -lazul \
        -rpath @executable_path/../../target/release \
        -Wno-deprecated-declarations 2>&1

    echo "Build complete"
    echo ""

    # Start the application in background
    echo "=== Starting application ==="
    AZUL_DEBUG=8765 ./simple_contenteditable &
    APP_PID=$!
    echo "App PID: $APP_PID"

    # Wait for server to be ready
    echo "Waiting for debug server..."
    for i in {1..30}; do
        if curl -s -m 1 "$API/" >/dev/null 2>&1; then
            echo "Server ready after ${i}x100ms"
            break
        fi
        sleep 0.1
    done

    echo ""
    echo "=== Initial get_state ==="
    curl -s -X POST "$API/" -d '{"op": "get_state"}' 2>&1
    echo ""
    sleep 0.5

    echo ""
    echo "=== get_dom_tree ==="
    curl -s -X POST "$API/" -d '{"op": "get_dom_tree"}' 2>&1
    echo ""
    sleep 0.5

    echo ""
    echo "=== Click at (300, 200) ==="
    curl -s -X POST "$API/" -d '{"op": "click", "x": 300, "y": 200}' 2>&1
    echo ""
    sleep 0.5

    echo ""
    echo "=== get_logs after click ==="
    curl -s -X POST "$API/" -d '{"op": "get_logs"}' 2>&1
    echo ""
    sleep 0.5

    echo ""
    echo "=== Sending text_input 'd' ==="
    curl -s -X POST "$API/" -d '{"op": "text_input", "text": "d"}' 2>&1
    echo ""
    sleep 0.5

    echo ""
    echo "=== get_logs after text_input (CRITICAL - check for text_input processing) ==="
    curl -s -X POST "$API/" -d '{"op": "get_logs"}' 2>&1
    echo ""
    sleep 0.5

    echo ""
    echo "=== get_selection_state ==="
    curl -s -X POST "$API/" -d '{"op": "get_selection_state"}' 2>&1
    echo ""
    sleep 0.5

    echo ""
    echo "=== get_dom_tree after text_input ==="
    curl -s -X POST "$API/" -d '{"op": "get_dom_tree"}' 2>&1
    echo ""
    sleep 0.5

    echo ""
    echo "=== Closing window ==="
    curl -s -X POST "$API/" -d '{"op": "close"}' 2>&1
    echo ""

    # Give it time to close
    sleep 0.5

    echo ""
    echo "=== Test Complete ==="
    echo "Finished at: $(date)"

} > "$LOG_FILE" 2>&1

echo "Test complete. Log file: $LOG_FILE"
echo ""
echo "=== Key excerpts ==="
echo ""
echo "--- Logs after text_input ---"
grep -A 50 "get_logs after text_input" "$LOG_FILE" | head -60
