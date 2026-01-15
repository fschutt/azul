#!/bin/bash
# Debug script for async thread example
# Uses the correct Azul Debug API

set -e

PORT=8772
API="http://localhost:$PORT"
LOGFILE="/tmp/async_debug_$$.log"

echo "═══════════════════════════════════════════════════════════"
echo "  Azul Async Thread Debug Script"
echo "═══════════════════════════════════════════════════════════"
echo ""
echo "Log file: $LOGFILE"
echo ""

# Build if needed
echo "[1] Building..."
cd /Users/fschutt/Development/azul
cargo build -p azul-dll --release --features build-dll 2>&1 | tail -3

# Compile async.c
echo "[2] Compiling async.c..."
cp target/codegen/v2/azul.h examples/c/
cd examples/c
gcc -o async async.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release 2>&1 | grep -v "warning:" || true

# Start the app
echo "[3] Starting async example with AZUL_DEBUG=$PORT..."
AZUL_DEBUG=$PORT ./async 2>"$LOGFILE" &
APP_PID=$!
echo "    PID: $APP_PID"

# Cleanup on exit
cleanup() {
    echo ""
    echo "[Cleanup] Killing app (PID $APP_PID)..."
    kill $APP_PID 2>/dev/null || true
    wait $APP_PID 2>/dev/null || true
    echo "[Cleanup] Done."
}
trap cleanup EXIT

# Wait for server
echo "[4] Waiting for debug server..."
for i in {1..10}; do
    if curl -s "$API/" > /dev/null 2>&1; then
        echo "    Server ready after ${i}s"
        break
    fi
    sleep 1
done

# Check initial state
echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  INITIAL STATE"
echo "═══════════════════════════════════════════════════════════"

echo ""
echo "[5] Getting DOM (get_html_string)..."
HTML=$(curl -s -X POST "$API/" -d '{"op": "get_html_string"}')
echo "$HTML" | jq -r '.data.html // .data.value // .' 2>/dev/null | head -5 || echo "$HTML" | head -200

echo ""
echo "[6] Finding Start button..."
BUTTON_INFO=$(curl -s -X POST "$API/" -d '{"op": "find_node_by_text", "text": "Start"}')
echo "$BUTTON_INFO" | jq . 2>/dev/null || echo "$BUTTON_INFO"

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  CLICKING START BUTTON"
echo "═══════════════════════════════════════════════════════════"

echo ""
echo "[7] Clicking 'Start' button..."
CLICK_RESULT=$(curl -s -X POST "$API/" -d '{"op": "click", "text": "Start"}')
echo "$CLICK_RESULT" | jq . 2>/dev/null || echo "$CLICK_RESULT"

# Wait a moment
sleep 0.5

echo ""
echo "[8] Getting DOM after click..."
HTML_AFTER=$(curl -s -X POST "$API/" -d '{"op": "get_html_string"}')
echo "$HTML_AFTER" | jq -r '.data.html // .data.value // .' 2>/dev/null | grep -oE 'Progress: [0-9]+%|Processing' || echo "$HTML_AFTER" | head -50

echo ""
echo "[9] Thread-related logs immediately after click:"
grep -E "thread|Thread|adding|removing|is_finished|strong_count|WriteBack|run_all_threads|starting thread poll" "$LOGFILE" 2>/dev/null | head -30 || echo "(no matches)"

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  MONITORING PROGRESS (5 seconds)"
echo "═══════════════════════════════════════════════════════════"

for i in {1..10}; do
    sleep 0.5
    HTML=$(curl -s -X POST "$API/" -d '{"op": "get_html_string"}' 2>/dev/null)
    PROGRESS=$(echo "$HTML" | jq -r '.data.html // .data.value // .' 2>/dev/null | grep -oE 'Progress: [0-9]+%' || echo "N/A")
    echo "  [$i] $PROGRESS"
done

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  DEBUG LOGS (thread-related)"
echo "═══════════════════════════════════════════════════════════"

echo ""
echo "Thread-related logs from $LOGFILE:"
grep -E "thread|Thread|adding|removing|is_finished|strong_count|WriteBack|run_all_threads" "$LOGFILE" 2>/dev/null | head -50 || echo "(no matches)"

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  FULL LOG (last 30 lines)"
echo "═══════════════════════════════════════════════════════════"
tail -30 "$LOGFILE"

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  DONE"
echo "═══════════════════════════════════════════════════════════"
