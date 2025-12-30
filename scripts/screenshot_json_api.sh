#!/usr/bin/env bash
#
# Test Azul's JSON GUI Automation API for Screenshots
#
# This script demonstrates using the debug server's JSON API
# to take screenshots of a running Azul application.
#
# The workflow:
# 1. Start an Azul app with AZUL_DEBUG=<port>
# 2. Wait for the app to be ready
# 3. Use curl to send JSON commands to take screenshots
# 4. Use jq to extract the base64 data URI
# 5. Use base64 to decode and save the PNG file
#
# Usage: ./scripts/screenshot_json_api.sh

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${PROJECT_ROOT}"

# Configuration
DEBUG_PORT="${AZUL_DEBUG_PORT:-8765}"
SCREENSHOT_DIR="${PROJECT_ROOT}/target/screenshots"
TIMEOUT=30

# Detect OS
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS_NAME="macos"
elif [[ "$OSTYPE" == "linux"* ]]; then
    OS_NAME="linux"
else
    OS_NAME="windows"
fi

mkdir -p "${SCREENSHOT_DIR}"

echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Azul JSON API Screenshot Test${NC}"
echo -e "${BLUE}════════════════════════════════════════════${NC}"
echo ""

# Check required tools
for tool in curl jq base64; do
    if ! command -v "$tool" &>/dev/null; then
        echo -e "${RED}ERROR: $tool is required. Install it first.${NC}"
        exit 1
    fi
done
echo -e "${GREEN}✓ Required tools available (curl, jq, base64)${NC}"

# Build if needed
if [[ ! -f "${PROJECT_ROOT}/target/release/hello_world_window" ]]; then
    echo -e "${YELLOW}Building hello_world_window example...${NC}"
    cargo build --package azul-dll --bin hello_world_window --features "build-dll" --release 2>&1 | tail -3
fi

echo ""
echo -e "${BLUE}[1/5] Starting Azul app with AZUL_DEBUG=${DEBUG_PORT}...${NC}"

# Kill any existing process on the port
lsof -ti:${DEBUG_PORT} 2>/dev/null | xargs kill -9 2>/dev/null || true

# Start the app with debug server in background
AZUL_DEBUG=${DEBUG_PORT} "${PROJECT_ROOT}/target/release/hello_world_window" &
APP_PID=$!

# Cleanup on exit
cleanup() {
    echo ""
    echo -e "${BLUE}Cleaning up...${NC}"
    kill $APP_PID 2>/dev/null || true
}
trap cleanup EXIT

# Wait for debug server to be ready
echo -e "${BLUE}[2/5] Waiting for debug server on port ${DEBUG_PORT}...${NC}"
for i in $(seq 1 $TIMEOUT); do
    if curl -s "http://localhost:${DEBUG_PORT}/" -X POST -d '{"type":"get_state"}' --connect-timeout 1 >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Debug server ready after ${i}s${NC}"
        break
    fi
    if [[ $i -eq $TIMEOUT ]]; then
        echo -e "${RED}✗ Timeout waiting for debug server${NC}"
        exit 1
    fi
    sleep 1
done

# Wait a bit more for the window to fully render
echo -e "${BLUE}[3/5] Waiting for window to render...${NC}"
sleep 2

# Take native screenshot via JSON API
echo -e "${BLUE}[4/5] Taking native screenshot via JSON API...${NC}"
echo ""
echo "  Request: curl -X POST http://localhost:${DEBUG_PORT}/ -d '{\"type\":\"take_native_screenshot\"}'"

RESPONSE=$(curl -s "http://localhost:${DEBUG_PORT}/" -X POST -d '{"type":"take_native_screenshot"}' --max-time 10)

# Check if we got a response
if [[ -z "$RESPONSE" ]]; then
    echo -e "${RED}✗ No response from debug server${NC}"
    exit 1
fi

# Parse response
SUCCESS=$(echo "$RESPONSE" | jq -r '.success' 2>/dev/null || echo "false")
DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
ERROR=$(echo "$RESPONSE" | jq -r '.error // empty' 2>/dev/null)

echo ""
echo "  Response success: $SUCCESS"

if [[ "$SUCCESS" == "true" ]] && [[ -n "$DATA" ]]; then
    echo -e "${GREEN}✓ Screenshot captured successfully${NC}"
    
    # Check if it's a data URI
    if [[ "$DATA" == data:image/png\;base64,* ]]; then
        # Extract base64 part (remove "data:image/png;base64," prefix)
        BASE64_DATA="${DATA#data:image/png;base64,}"
        
        # Decode and save
        OUTPUT_FILE="${SCREENSHOT_DIR}/debug-api-native-${OS_NAME}.png"
        echo "$BASE64_DATA" | base64 -d > "$OUTPUT_FILE"
        
        if [[ -f "$OUTPUT_FILE" ]]; then
            SIZE=$(ls -lh "$OUTPUT_FILE" | awk '{print $5}')
            echo -e "${GREEN}✓ Saved to: ${OUTPUT_FILE} (${SIZE})${NC}"
            
            # Verify it's a valid PNG
            if file "$OUTPUT_FILE" | grep -q PNG; then
                echo -e "${GREEN}✓ Valid PNG file${NC}"
            else
                echo -e "${YELLOW}⚠ File may not be a valid PNG${NC}"
            fi
        else
            echo -e "${RED}✗ Failed to write file${NC}"
        fi
    else
        echo -e "${RED}✗ Response data is not a data URI${NC}"
        echo "  First 100 chars: ${DATA:0:100}"
    fi
else
    echo -e "${RED}✗ Screenshot failed${NC}"
    if [[ -n "$ERROR" ]]; then
        echo "  Error: $ERROR"
    fi
    echo "  Full response: $RESPONSE"
fi

# Also try CPU screenshot
echo ""
echo -e "${BLUE}[5/5] Taking CPU screenshot via JSON API...${NC}"
echo ""
echo "  Request: curl -X POST http://localhost:${DEBUG_PORT}/ -d '{\"type\":\"take_screenshot\"}'"

RESPONSE=$(curl -s "http://localhost:${DEBUG_PORT}/" -X POST -d '{"type":"take_screenshot"}' --max-time 10)

SUCCESS=$(echo "$RESPONSE" | jq -r '.success' 2>/dev/null || echo "false")
DATA=$(echo "$RESPONSE" | jq -r '.data // empty' 2>/dev/null)
ERROR=$(echo "$RESPONSE" | jq -r '.error // empty' 2>/dev/null)

echo ""
echo "  Response success: $SUCCESS"

if [[ "$SUCCESS" == "true" ]] && [[ -n "$DATA" ]]; then
    if [[ "$DATA" == data:image/png\;base64,* ]]; then
        BASE64_DATA="${DATA#data:image/png;base64,}"
        OUTPUT_FILE="${SCREENSHOT_DIR}/debug-api-cpu-${OS_NAME}.png"
        echo "$BASE64_DATA" | base64 -d > "$OUTPUT_FILE"
        
        if [[ -f "$OUTPUT_FILE" ]]; then
            SIZE=$(ls -lh "$OUTPUT_FILE" | awk '{print $5}')
            echo -e "${GREEN}✓ Saved to: ${OUTPUT_FILE} (${SIZE})${NC}"
        fi
    else
        echo -e "${YELLOW}⚠ Response is not a data URI: ${DATA:0:100}${NC}"
    fi
else
    echo -e "${YELLOW}⚠ CPU screenshot failed (this is expected if no GPU rendering is active)${NC}"
    if [[ -n "$ERROR" ]]; then
        echo "  Error: $ERROR"
    fi
fi

# Close the window via API
echo ""
echo -e "${BLUE}Closing window via API...${NC}"
curl -s "http://localhost:${DEBUG_PORT}/" -X POST -d '{"type":"close"}' --max-time 5 >/dev/null 2>&1 || true

echo ""
echo -e "${GREEN}════════════════════════════════════════════${NC}"
echo -e "${GREEN}  Test Complete!${NC}"
echo -e "${GREEN}════════════════════════════════════════════${NC}"
echo ""
echo "Screenshots saved to: ${SCREENSHOT_DIR}/"
ls -la "${SCREENSHOT_DIR}/"*.png 2>/dev/null || echo "  (no PNG files found)"
echo ""
echo "This demonstrates using the JSON GUI automation API:"
echo ""
echo "  1. Start app:  AZUL_DEBUG=8765 ./my_app"
echo "  2. Screenshot: curl -X POST localhost:8765/ -d '{\"type\":\"take_native_screenshot\"}'"
echo "  3. Decode:     echo \"\$base64_data\" | base64 -d > screenshot.png"
echo ""
