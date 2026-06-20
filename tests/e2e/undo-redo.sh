#!/usr/bin/env bash
# =============================================================================
# undo-redo.sh — E2E test for the app-state undo/redo (mini-git) system.
#
# Mirrors tests/e2e/test_export_code.sh:
#   1. Builds the JSON-serializable counter app (examples/c/hello-world.c)
#      against the DLL.
#   2. Launches it with AZ_DEBUG on a free port.
#   3. POSTs run_e2e_tests with tests/e2e/undo_redo.json (set_app_state +
#      commit_undo_snapshot + undo_app_state / redo_app_state + assert_app_state).
#   4. Verifies every test + every step passed.
#
# The app must be JSON-serializable (AZ_REFLECT_JSON) for set/assert/undo to
# work — hello-world.c serializes its model as { "counter": N }.
#
# Requirements: a built DLL in target/release (libazul + dll/azul.h), curl, jq,
# python3, a C compiler.
#
# Usage:    bash tests/e2e/undo-redo.sh
# Exit:     0 = all passed, 1 = a failure (or the suite did not run)
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PORT=8801
HELLO_BIN="$ROOT/target/release/hello-world-undo"
TEST_JSON="$ROOT/tests/e2e/undo_redo.json"

echo "=== Step 1: Build the serializable counter app ==="
cc -o "$HELLO_BIN" \
    "$ROOT/examples/c/hello-world.c" \
    -I "$ROOT/dll/" \
    -L "$ROOT/target/release" \
    -lazul \
    -Wl,-rpath,"$ROOT/target/release"
echo "  -> Built: $HELLO_BIN"

echo ""
echo "=== Step 2: Launch with AZ_DEBUG=$PORT ==="
AZ_DEBUG=$PORT "$HELLO_BIN" &>/dev/null &
APP_PID=$!

cleanup() {
    echo ""
    echo "=== Cleanup: killing PID $APP_PID ==="
    kill "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
}
trap cleanup EXIT

echo "  -> Waiting for debug server on port $PORT..."
READY=0
for i in $(seq 1 30); do
    if curl -s -o /dev/null "http://localhost:$PORT/" 2>/dev/null; then
        echo "  -> Server ready (attempt $i)"
        READY=1
        break
    fi
    sleep 0.5
done
if [ "$READY" != "1" ]; then
    echo "FAIL: debug server never came up on port $PORT"
    exit 1
fi

echo ""
echo "=== Step 3: Run the undo/redo E2E suite ==="
TESTS=$(cat "$TEST_JSON")
RESPONSE=$(curl -s -X POST "http://localhost:$PORT/" \
    -H 'Content-Type: application/json' \
    -d "{\"op\":\"run_e2e_tests\",\"tests\":$TESTS}")
echo "$RESPONSE" | python3 -m json.tool 2>/dev/null || echo "$RESPONSE"

echo ""
echo "=== Step 4: Check results ==="
# Nesting-agnostic: every E2eTestResult object carries a "steps_failed" field.
# Find them all wherever they sit in the response envelope and require each to
# be status == "pass" with zero failed steps.
TOTAL=$(echo "$RESPONSE" | jq -r \
    '[.. | objects | select(has("steps_failed"))] | length' 2>/dev/null || echo "0")
FAILED=$(echo "$RESPONSE" | jq -r \
    '[.. | objects | select(has("steps_failed")) | select(.status != "pass" or .steps_failed != 0)] | length' 2>/dev/null || echo "ERR")

echo "  tests_found=$TOTAL  failed=$FAILED"

if [ "$TOTAL" = "0" ]; then
    echo "FAIL: the undo/redo suite produced no test results (did it run?)"
    exit 1
fi
if [ "$FAILED" != "0" ]; then
    echo "FAIL: $FAILED undo/redo test(s) failed"
    exit 1
fi

echo "PASS: all $TOTAL undo/redo E2E test(s) passed"
exit 0
