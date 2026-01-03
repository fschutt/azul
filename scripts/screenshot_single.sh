#!/bin/bash
# Screenshot Single Example Script
# Usage: ./scripts/screenshot_single.sh <example_name> [port]
#
# This script:
# 1. Compiles the DLL if not already done
# 2. Checks headers are present
# 3. Creates target/examples-temp/<example>/ with DLL + headers
# 4. Compiles the example in that folder
# 5. Runs it with AZUL_DEBUG in background
# 6. Waits for startup
# 7. Tests if app is running and port is listening
# 8. Takes screenshot and saves it
# 9. Shuts down the application
# 10. Verifies shutdown

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
EXAMPLE_NAME="${1:-hello-world}"
PORT="${2:-8765}"
STARTUP_WAIT=3
SHUTDOWN_WAIT=2
MAX_RETRIES=5

# Paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TEMP_DIR="$ROOT_DIR/target/examples-temp/$EXAMPLE_NAME"
EXAMPLE_SRC="$ROOT_DIR/examples/c/${EXAMPLE_NAME}.c"

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step() { echo -e "${BLUE}[STEP $1]${NC} $2"; }

# Cleanup function
cleanup() {
    log_info "Cleaning up..."
    # Kill any process on our port
    local pid=$(lsof -ti :$PORT 2>/dev/null || true)
    if [ -n "$pid" ]; then
        kill -9 $pid 2>/dev/null || true
    fi
}

# Set trap for cleanup
trap cleanup EXIT

# Kill any existing process with the same name
log_info "Killing any existing $EXAMPLE_NAME processes..."
killall "$EXAMPLE_NAME" 2>/dev/null || true
sleep 1

# Check if example source exists
if [ ! -f "$EXAMPLE_SRC" ]; then
    log_error "Example source not found: $EXAMPLE_SRC"
    log_info "Available examples:"
    ls -1 "$ROOT_DIR/examples/c/"*.c 2>/dev/null | xargs -I{} basename {} .c || echo "  (none)"
    exit 1
fi

log_info "=========================================="
log_info "Screenshot Test: $EXAMPLE_NAME"
log_info "Port: $PORT"
log_info "=========================================="

# Step 1: Always rebuild DLL
log_step 1 "Building DLL..."

DLL_PATH=""
if [ "$(uname)" == "Darwin" ]; then
    DLL_PATH="$ROOT_DIR/target/release/libazul.dylib"
elif [ "$(uname)" == "Linux" ]; then
    DLL_PATH="$ROOT_DIR/target/release/libazul.so"
else
    DLL_PATH="$ROOT_DIR/target/release/azul.dll"
fi

cd "$ROOT_DIR"
cargo build --release -p azul-dll --features build-dll
if [ ! -f "$DLL_PATH" ]; then
    log_error "Failed to compile DLL"
    exit 1
fi
log_success "DLL compiled: $DLL_PATH"

# Step 2: Check headers are present
log_step 2 "Checking headers..."

HEADER_PATH="$ROOT_DIR/target/codegen/v2/azul.h"
if [ ! -f "$HEADER_PATH" ]; then
    log_error "Header not found: $HEADER_PATH"
    log_info "Run codegen first or check the path"
    exit 1
fi
log_success "Header found: $HEADER_PATH"

# Step 3: Create temp folder with DLL + headers
log_step 3 "Creating temp folder: $TEMP_DIR"

rm -rf "$TEMP_DIR"
mkdir -p "$TEMP_DIR"

# Copy DLL
cp "$DLL_PATH" "$TEMP_DIR/"
log_info "Copied DLL to $TEMP_DIR/"

# Copy header
cp "$HEADER_PATH" "$TEMP_DIR/"
log_info "Copied header to $TEMP_DIR/"

# Copy example source
cp "$EXAMPLE_SRC" "$TEMP_DIR/"
log_info "Copied $EXAMPLE_NAME.c to $TEMP_DIR/"

log_success "Temp folder prepared"

# Step 4: Compile the example
log_step 4 "Compiling example..."

cd "$TEMP_DIR"

EXAMPLE_BIN="$TEMP_DIR/$EXAMPLE_NAME"
DLL_NAME=$(basename "$DLL_PATH")

if [ "$(uname)" == "Darwin" ]; then
    # macOS compilation
    clang -o "$EXAMPLE_BIN" \
        -I"$TEMP_DIR" \
        -L"$TEMP_DIR" \
        -lazul \
        -framework AppKit \
        -framework OpenGL \
        -framework CoreGraphics \
        -framework CoreText \
        -framework CoreFoundation \
        -Wl,-rpath,"$TEMP_DIR" \
        "$EXAMPLE_NAME.c"
elif [ "$(uname)" == "Linux" ]; then
    # Linux compilation
    gcc -o "$EXAMPLE_BIN" \
        -I"$TEMP_DIR" \
        -L"$TEMP_DIR" \
        -lazul \
        -lGL -lX11 -lpthread -lm \
        -Wl,-rpath,"$TEMP_DIR" \
        "$EXAMPLE_NAME.c"
else
    log_error "Unsupported platform: $(uname)"
    exit 1
fi

if [ ! -f "$EXAMPLE_BIN" ]; then
    log_error "Failed to compile example"
    exit 1
fi

log_success "Example compiled: $EXAMPLE_BIN"

# Step 5: Ensure port is free before starting
log_step 5 "Ensuring port $PORT is free..."

existing_pid=$(lsof -ti :$PORT 2>/dev/null || true)
if [ -n "$existing_pid" ]; then
    log_warn "Port $PORT is in use by PID $existing_pid, killing..."
    kill -9 $existing_pid 2>/dev/null || true
    sleep 1
fi
log_success "Port $PORT is free"

# Step 6: Run example with AZUL_DEBUG
log_step 6 "Starting example with AZUL_DEBUG=$PORT..."

cd "$TEMP_DIR"
AZUL_DEBUG=$PORT "./$EXAMPLE_NAME" > "$TEMP_DIR/stdout.log" 2> "$TEMP_DIR/stderr.log" &
APP_PID=$!

log_info "Started with PID: $APP_PID"
log_info "Waiting ${STARTUP_WAIT}s for startup..."
sleep $STARTUP_WAIT

# Step 7: Verify app is running and port is listening
log_step 7 "Verifying app is running..."

# Check if process is still alive
if ! kill -0 $APP_PID 2>/dev/null; then
    log_error "Process died during startup!"
    log_error "=== STDOUT ==="
    cat "$TEMP_DIR/stdout.log" || true
    log_error "=== STDERR ==="
    cat "$TEMP_DIR/stderr.log" || true
    exit 1
fi
log_info "Process $APP_PID is alive"

# Check if port is listening
port_check=$(lsof -ti :$PORT 2>/dev/null || true)
if [ -z "$port_check" ]; then
    log_error "Port $PORT is not listening!"
    log_error "=== STDERR ==="
    cat "$TEMP_DIR/stderr.log" || true
    kill -9 $APP_PID 2>/dev/null || true
    exit 1
fi
log_success "Port $PORT is listening (PID: $port_check)"

# Quick connectivity test
log_info "Testing HTTP connectivity..."
log_info "Request: POST http://localhost:$PORT/ - {\"type\":\"get_logs\"}"

CONNECTIVITY_RESPONSE_FILE="$TEMP_DIR/connectivity_response.json"
CURL_STDERR_FILE="$TEMP_DIR/curl_stderr.log"

# Use --max-time to prevent hanging, capture curl exit code
curl -s -X POST "http://localhost:$PORT/" \
    -H "Content-Type: application/json" \
    -d '{"type":"get_logs"}' \
    --max-time 30 \
    -o "$CONNECTIVITY_RESPONSE_FILE" \
    2>"$CURL_STDERR_FILE"
curl_exit=$?

log_info "curl exit code: $curl_exit"
if [ -s "$CURL_STDERR_FILE" ]; then
    log_warn "curl stderr: $(cat $CURL_STDERR_FILE)"
fi

# Show app stderr after request
log_info "=== App stderr after get_logs ==="
tail -20 "$TEMP_DIR/stderr.log" 2>/dev/null || true
log_info "=== End app stderr ==="

response_size=$(wc -c < "$CONNECTIVITY_RESPONSE_FILE")
log_info "Response size: $response_size bytes"

if [ $curl_exit -ne 0 ]; then
    log_error "curl failed with exit code $curl_exit"
    log_error "Response saved to: $CONNECTIVITY_RESPONSE_FILE"
    kill -9 $APP_PID 2>/dev/null || true
    exit 1
fi

# Simple check: just verify "status" and "ok" are in the response (handles pretty-print)
if ! grep -q '"status"' "$CONNECTIVITY_RESPONSE_FILE" || ! grep -q '"ok"' "$CONNECTIVITY_RESPONSE_FILE"; then
    log_error "HTTP connectivity test failed"
    log_error "Response saved to: $CONNECTIVITY_RESPONSE_FILE"
    kill -9 $APP_PID 2>/dev/null || true
    exit 1
fi
log_success "HTTP connectivity OK"

# Wait a bit before taking screenshot
sleep 1

# Step 8: Take screenshot
log_step 8 "Taking screenshot..."

SCREENSHOT_FILE="$TEMP_DIR/${EXAMPLE_NAME}_screenshot.png"
JSON_RESPONSE_FILE="$TEMP_DIR/screenshot_response.json"

log_info "Request: POST http://localhost:$PORT/ - {\"type\":\"take_native_screenshot\"}"

# Save raw response to file immediately, avoiding memory/ARG_MAX limits
curl -s -X POST "http://localhost:$PORT/" \
    -H "Content-Type: application/json" \
    -d '{"type":"take_native_screenshot"}' \
    --max-time 60 \
    -o "$JSON_RESPONSE_FILE"

log_info "Response saved to $JSON_RESPONSE_FILE ($(ls -lh "$JSON_RESPONSE_FILE" | awk '{print $5}'))"

# Check status and extract screenshot using Python (avoids jq pipe issues with large files)
python3 << EOF
import json
import base64
import sys

try:
    with open("$JSON_RESPONSE_FILE", "r") as f:
        data = json.load(f)
    
    if data.get("status") != "ok":
        print("ERROR: " + data.get("message", "Unknown error"))
        sys.exit(1)
    
    img_data = data["data"]["value"]["data"]
    base64_data = img_data.replace("data:image/png;base64,", "")
    img_bytes = base64.b64decode(base64_data)
    
    with open("$SCREENSHOT_FILE", "wb") as f:
        f.write(img_bytes)
    
    print("OK: {} bytes".format(len(img_bytes)))
    sys.exit(0)
except Exception as e:
    print("ERROR: " + str(e))
    sys.exit(1)
EOF

python_result=$?
if [ $python_result -eq 0 ] && [ -f "$SCREENSHOT_FILE" ] && [ -s "$SCREENSHOT_FILE" ]; then
    log_success "Screenshot saved: $SCREENSHOT_FILE"
    log_info "Size: $(ls -lh "$SCREENSHOT_FILE" | awk '{print $5}')"
else
    log_error "Screenshot extraction failed"
fi

# Wait before shutdown
sleep 1

# Step 9: Shut down application
log_step 9 "Shutting down application..."

log_info "Request: POST http://localhost:$PORT/ - {\"type\":\"close\"}"

CLOSE_RESPONSE_FILE="$TEMP_DIR/close_response.json"
curl -s -X POST "http://localhost:$PORT/" \
    -H "Content-Type: application/json" \
    -d '{"type":"close"}' \
    -o "$CLOSE_RESPONSE_FILE" || true

if grep -q '"status":"ok"' "$CLOSE_RESPONSE_FILE" 2>/dev/null; then
    log_info "Close command sent successfully"
else
    log_warn "Close command may have failed"
fi

log_info "Waiting ${SHUTDOWN_WAIT}s for graceful shutdown..."
sleep $SHUTDOWN_WAIT

# Step 10: Verify shutdown
log_step 10 "Verifying shutdown..."

# Check if process is gone
if kill -0 $APP_PID 2>/dev/null; then
    log_warn "Process still alive, force killing..."
    kill -9 $APP_PID 2>/dev/null || true
    sleep 1
fi

# Check if port is free
port_check=$(lsof -ti :$PORT 2>/dev/null || true)
if [ -n "$port_check" ]; then
    log_warn "Port still in use, killing PID $port_check..."
    kill -9 $port_check 2>/dev/null || true
    sleep 1
fi

# Final verification
port_check=$(lsof -ti :$PORT 2>/dev/null || true)
if [ -z "$port_check" ]; then
    log_success "Application shut down cleanly, port $PORT is free"
else
    log_error "Failed to free port $PORT!"
    exit 1
fi

# Summary
log_info "=========================================="
log_info "SUMMARY"
log_info "=========================================="
log_success "Example: $EXAMPLE_NAME"
log_success "Temp Dir: $TEMP_DIR"

if [ -f "$SCREENSHOT_FILE" ] && [ -s "$SCREENSHOT_FILE" ]; then
    log_success "Screenshot: $SCREENSHOT_FILE ($(ls -lh "$SCREENSHOT_FILE" | awk '{print $5}'))"
else
    log_error "Screenshot: FAILED"
    exit 1
fi

log_info "Log files:"
log_info "  stdout: $TEMP_DIR/stdout.log"
log_info "  stderr: $TEMP_DIR/stderr.log"

log_success "=========================================="
log_success "TEST PASSED"
log_success "=========================================="

exit 0
