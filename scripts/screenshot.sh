#!/usr/bin/env bash
#
# Screenshot script for Azul examples
#
# Takes screenshots of running examples using Azul's debug server API.
# Screenshots are saved to target/screenshots/<example>-<os>.png
#
# Usage:
#   ./scripts/screenshot.sh                    # Run all C examples
#   ./scripts/screenshot.sh hello-world        # Run specific example
#   ./scripts/screenshot.sh --rust kitchen_sink # Run Rust example
#
# Environment:
#   AZUL_DEBUG_PORT - Port for debug server (default: 8765)
#   AZUL_SCREENSHOT_WAIT_MS - Wait time before screenshot (default: 500)

set -e

# Colors
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
WAIT_MS="${AZUL_SCREENSHOT_WAIT_MS:-500}"
SCREENSHOT_DIR="${PROJECT_ROOT}/target/screenshots"

# Detect OS
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS_NAME="macos"
    LIB_PATH="${PROJECT_ROOT}/target/release/libazul.dylib"
    LIB_ENV="DYLD_LIBRARY_PATH"
elif [[ "$OSTYPE" == "linux"* ]]; then
    OS_NAME="linux"
    LIB_PATH="${PROJECT_ROOT}/target/release/libazul.so"
    LIB_ENV="LD_LIBRARY_PATH"
else
    OS_NAME="windows"
    LIB_PATH="${PROJECT_ROOT}/target/release/azul.dll"
    LIB_ENV="PATH"
fi

HEADER_DIR="${PROJECT_ROOT}/target/codegen/v2"
C_EXAMPLES_DIR="${PROJECT_ROOT}/examples/c"
CPP_EXAMPLES_DIR="${PROJECT_ROOT}/examples/cpp"
RUST_EXAMPLES_DIR="${PROJECT_ROOT}/examples/rust"

mkdir -p "${SCREENSHOT_DIR}"

echo -e "${BLUE}============================================${NC}"
echo -e "${BLUE}  Azul Screenshot Script (${OS_NAME})${NC}"
echo -e "${BLUE}============================================${NC}"
echo ""

# Check if library exists
if [[ ! -f "${LIB_PATH}" ]]; then
    echo -e "${RED}ERROR: Library not found at ${LIB_PATH}${NC}"
    echo -e "${YELLOW}Build it with: cargo build --release -p azul-dll --features build-dll${NC}"
    exit 1
fi

# Check if curl is available
if ! command -v curl &> /dev/null; then
    echo -e "${RED}ERROR: curl is required but not installed${NC}"
    exit 1
fi

# Function to take screenshot of a running app
take_screenshot() {
    local example_name="$1"
    local output_file="${SCREENSHOT_DIR}/${example_name}-${OS_NAME}.png"
    
    echo -e "${BLUE}Taking screenshot...${NC}"
    
    # Wait for app to render
    sleep 0.5
    
    # Wait additional frames via debug API
    curl -s -X POST "http://localhost:${DEBUG_PORT}/" \
        -H "Content-Type: application/json" \
        -d "{\"type\":\"wait\",\"ms\":${WAIT_MS}}" > /dev/null 2>&1 || true
    
    # Take native screenshot (includes window decorations)
    local response
    response=$(curl -s -X POST "http://localhost:${DEBUG_PORT}/" \
        -H "Content-Type: application/json" \
        -d '{"type":"take_native_screenshot"}' 2>&1)
    
    # Check if we got a data URI
    if echo "$response" | grep -q "data:image"; then
        # Extract the data field and decode base64
        local data_uri
        data_uri=$(echo "$response" | sed -n 's/.*"data":"\([^"]*\)".*/\1/p')
        
        if [[ -n "$data_uri" ]]; then
            # Remove the data:image/png;base64, prefix and decode
            echo "$data_uri" | sed 's/data:image\/png;base64,//' | base64 -d > "$output_file"
            
            if [[ -f "$output_file" ]] && [[ -s "$output_file" ]]; then
                local size
                size=$(wc -c < "$output_file" | tr -d ' ')
                echo -e "${GREEN}✓ Screenshot saved: ${output_file} (${size} bytes)${NC}"
                return 0
            fi
        fi
    fi
    
    echo -e "${RED}✗ Failed to take screenshot${NC}"
    echo -e "${YELLOW}Response: ${response}${NC}"
    return 1
}

# Function to shutdown the app
shutdown_app() {
    echo -e "${BLUE}Shutting down app...${NC}"
    
    # Request app to close
    curl -s -X POST "http://localhost:${DEBUG_PORT}/" \
        -H "Content-Type: application/json" \
        -d '{"type":"close"}' > /dev/null 2>&1 || true
    
    # Give it a moment to close gracefully
    sleep 0.5
}

# Function to wait for debug server to be ready
wait_for_server() {
    local max_attempts=30
    local attempt=0
    
    while [[ $attempt -lt $max_attempts ]]; do
        if curl -s -X POST "http://localhost:${DEBUG_PORT}/" \
            -H "Content-Type: application/json" \
            -d '{"type":"get_state"}' > /dev/null 2>&1; then
            return 0
        fi
        sleep 0.2
        ((attempt++))
    done
    
    return 1
}

# Function to run and screenshot a C example
run_c_example() {
    local example_name="$1"
    local source_file="${C_EXAMPLES_DIR}/${example_name}.c"
    local binary_dir="${PROJECT_ROOT}/target/c-examples"
    local binary_file="${binary_dir}/${example_name}"
    local binary_name="${example_name}"
    
    # Windows needs .exe extension
    if [[ "$OSTYPE" != "darwin"* ]] && [[ "$OSTYPE" != "linux"* ]]; then
        binary_file="${binary_file}.exe"
        binary_name="${example_name}.exe"
    fi
    
    if [[ ! -f "$source_file" ]]; then
        echo -e "${YELLOW}Skipping ${example_name}: source not found${NC}"
        return 1
    fi
    
    echo -e "\n${BLUE}--- ${example_name} (C) ---${NC}"
    
    # Compile if needed
    mkdir -p "$binary_dir"
    
    # Copy assets to the binary directory (for examples that need external files)
    if [[ -d "${PROJECT_ROOT}/examples/assets" ]]; then
        cp -r "${PROJECT_ROOT}/examples/assets/"* "$binary_dir/" 2>/dev/null || true
    fi
    
    local cc_flags=""
    local link_flags=""
    
    if [[ "$OSTYPE" == "darwin"* ]]; then
        cc_flags="-framework Cocoa -framework OpenGL -framework IOKit -framework CoreFoundation -framework CoreGraphics"
        link_flags="-L${PROJECT_ROOT}/target/release -lazul -Wl,-rpath,${PROJECT_ROOT}/target/release"
    elif [[ "$OSTYPE" == "linux"* ]]; then
        link_flags="-L${PROJECT_ROOT}/target/release -lazul -Wl,-rpath,${PROJECT_ROOT}/target/release -lm -lpthread -ldl"
    else
        # Windows: Link directly against the DLL (MinGW can do this)
        # Copy DLL to binary dir so it can be found at runtime
        cp "${PROJECT_ROOT}/target/release/azul.dll" "$binary_dir/" 2>/dev/null || true
        link_flags="${PROJECT_ROOT}/target/release/azul.dll -lws2_32 -luserenv -lbcrypt -lntdll -lole32 -lshell32 -lopengl32 -lgdi32 -luser32 -lkernel32 -ladvapi32"
    fi
    
    if ! cc -o "$binary_file" "$source_file" -I"${HEADER_DIR}" $cc_flags $link_flags 2>&1; then
        echo -e "${RED}✗ Failed to compile ${example_name}${NC}"
        return 1
    fi
    
    echo -e "${GREEN}✓ Compiled${NC}"
    
    # Run with debug server from the binary directory (so relative paths work)
    echo -e "${BLUE}Starting app with AZUL_DEBUG=${DEBUG_PORT}...${NC}"
    
    # Set environment and run from binary directory
    export AZUL_DEBUG="${DEBUG_PORT}"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        export DYLD_LIBRARY_PATH="${PROJECT_ROOT}/target/release"
    elif [[ "$OSTYPE" == "linux"* ]]; then
        export LD_LIBRARY_PATH="${PROJECT_ROOT}/target/release"
    fi
    
    # Change to binary dir so relative paths work, then run
    (cd "$binary_dir" && "./${binary_name}") &
    local pid=$!
    
    # Wait for server to be ready
    if ! wait_for_server; then
        echo -e "${RED}✗ Debug server did not start${NC}"
        kill $pid 2>/dev/null || true
        return 1
    fi
    
    echo -e "${GREEN}✓ App started (PID: ${pid})${NC}"
    
    # Take screenshot
    take_screenshot "$example_name"
    local screenshot_result=$?
    
    # Shutdown
    shutdown_app
    
    # Make sure process is dead - give more time for port to be released
    kill $pid 2>/dev/null || true
    wait $pid 2>/dev/null || true
    sleep 1  # Wait for port to be released
    
    return $screenshot_result
}

# Function to run and screenshot a Rust example
run_rust_example() {
    local example_name="$1"
    
    echo -e "\n${BLUE}--- ${example_name} (Rust) ---${NC}"
    
    # Build the example
    if ! cargo build --release -p azul-dll --example "$example_name" 2>&1; then
        echo -e "${RED}✗ Failed to build ${example_name}${NC}"
        return 1
    fi
    
    echo -e "${GREEN}✓ Built${NC}"
    
    local binary_file="${PROJECT_ROOT}/target/release/examples/${example_name}"
    
    # Run with debug server
    echo -e "${BLUE}Starting app with AZUL_DEBUG=${DEBUG_PORT}...${NC}"
    
    AZUL_DEBUG="${DEBUG_PORT}" "$binary_file" &
    local pid=$!
    
    # Wait for server to be ready
    if ! wait_for_server; then
        echo -e "${RED}✗ Debug server did not start${NC}"
        kill $pid 2>/dev/null || true
        return 1
    fi
    
    echo -e "${GREEN}✓ App started (PID: ${pid})${NC}"
    
    # Take screenshot
    take_screenshot "$example_name"
    local screenshot_result=$?
    
    # Shutdown
    shutdown_app
    
    # Make sure process is dead - give more time for port to be released
    kill $pid 2>/dev/null || true
    wait $pid 2>/dev/null || true
    sleep 1  # Wait for port to be released
    
    return $screenshot_result
}

# Main
PASSED=0
FAILED=0
EXAMPLE_TYPE=""
SPECIFIC_EXAMPLE=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --rust)
            EXAMPLE_TYPE="rust"
            shift
            ;;
        --c)
            EXAMPLE_TYPE="c"
            shift
            ;;
        *)
            SPECIFIC_EXAMPLE="$1"
            shift
            ;;
    esac
done

# Run specific example or all
if [[ -n "$SPECIFIC_EXAMPLE" ]]; then
    if [[ "$EXAMPLE_TYPE" == "rust" ]]; then
        if run_rust_example "$SPECIFIC_EXAMPLE"; then
            ((PASSED++))
        else
            ((FAILED++))
        fi
    else
        if run_c_example "$SPECIFIC_EXAMPLE"; then
            ((PASSED++))
        else
            ((FAILED++))
        fi
    fi
else
    # Run all C examples
    for source_file in "${C_EXAMPLES_DIR}"/*.c; do
        if [[ -f "$source_file" ]]; then
            example_name=$(basename "$source_file" .c)
            
            # Skip examples that are not suitable for screenshots
            case "$example_name" in
                infinity|opengl)
                    # infinity: runs indefinitely
                    # opengl: requires specific OpenGL setup
                    echo -e "${YELLOW}Skipping ${example_name} (not suitable for automated screenshots)${NC}"
                    continue
                    ;;
            esac
            
            if run_c_example "$example_name"; then
                ((PASSED++))
            else
                ((FAILED++))
            fi
        fi
    done
fi

echo -e "\n${BLUE}============================================${NC}"
echo -e "${BLUE}  Summary${NC}"
echo -e "${BLUE}============================================${NC}"
echo -e "${GREEN}Passed: ${PASSED}${NC}"
echo -e "${RED}Failed: ${FAILED}${NC}"
echo -e "Screenshots in: ${SCREENSHOT_DIR}"

if [[ $FAILED -gt 0 ]]; then
    exit 1
fi
