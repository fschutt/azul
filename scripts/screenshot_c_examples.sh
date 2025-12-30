#!/usr/bin/env bash
#
# Screenshot all C examples with window decorations
# 
# This script:
# 1. Runs codegen to generate headers
# 2. Builds the azul-dll library
# 3. Compiles all C examples
# 4. Runs each example briefly and takes a native screenshot
# 5. Saves screenshots to target/screenshots/ (e.g., hello-world-macos.png)
#
# Usage: ./scripts/screenshot_c_examples.sh
#
# Requirements:
# - jq (for JSON parsing)
# - base64 decoding (built into macOS/Linux)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get the project root directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${PROJECT_ROOT}"

# Detect OS for screenshot naming
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS_NAME="macos"
    DYLIB_EXT="dylib"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul_dll.dylib"
elif [[ "$OSTYPE" == "linux"* ]]; then
    OS_NAME="linux"
    DYLIB_EXT="so"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul_dll.so"
else
    OS_NAME="windows"
    DYLIB_EXT="dll"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/azul_dll.dll"
fi

SCREENSHOT_DIR="${PROJECT_ROOT}/target/screenshots"
C_EXAMPLES_DIR="${PROJECT_ROOT}/examples/c"
C_BINARIES_DIR="${PROJECT_ROOT}/target/c-examples"
HEADER_DIR="${PROJECT_ROOT}/target/codegen/v2"

# C examples to screenshot
C_EXAMPLES=(
    "hello-world"
    "calc"
    "async"
    "widgets"
    "infinity"
    "xhtml"
)

# Create directories
mkdir -p "${SCREENSHOT_DIR}"
mkdir -p "${C_BINARIES_DIR}"

echo -e "${BLUE}============================================${NC}"
echo -e "${BLUE}  Azul C Examples Screenshot Script${NC}"
echo -e "${BLUE}============================================${NC}"
echo ""
echo "OS: ${OS_NAME}"
echo "Screenshots will be saved to: ${SCREENSHOT_DIR}"
echo ""

# ============================================
# Step 1: Check dependencies
# ============================================
echo -e "${BLUE}--- Step 1: Checking dependencies ---${NC}"

if ! command -v jq &> /dev/null; then
    echo -e "${RED}ERROR: jq is not installed. Install with:${NC}"
    echo "  macOS: brew install jq"
    echo "  Linux: apt install jq"
    exit 1
fi
echo -e "${GREEN}✓ jq found${NC}"

if ! command -v base64 &> /dev/null; then
    echo -e "${RED}ERROR: base64 is not installed${NC}"
    exit 1
fi
echo -e "${GREEN}✓ base64 found${NC}"

# ============================================
# Step 2: Generate codegen files
# ============================================
echo -e "\n${BLUE}--- Step 2: Generating codegen files ---${NC}"

if cargo run -p azul-doc -- codegen all 2>&1; then
    echo -e "${GREEN}✓ Codegen complete${NC}"
else
    echo -e "${RED}✗ Codegen failed${NC}"
    exit 1
fi

# Check if header exists
if [[ ! -f "${HEADER_DIR}/azul.h" ]]; then
    echo -e "${RED}ERROR: ${HEADER_DIR}/azul.h not found${NC}"
    exit 1
fi

# ============================================
# Step 3: Build azul-dll (release mode)
# ============================================
echo -e "\n${BLUE}--- Step 3: Building azul-dll (release) ---${NC}"

if cargo build -p azul-dll --release 2>&1; then
    echo -e "${GREEN}✓ azul-dll built${NC}"
else
    echo -e "${RED}✗ azul-dll build failed${NC}"
    exit 1
fi

# Check if library exists
if [[ ! -f "${DYLIB_PATH}" ]]; then
    echo -e "${RED}ERROR: ${DYLIB_PATH} not found${NC}"
    exit 1
fi

# ============================================
# Step 4: Compile C examples
# ============================================
echo -e "\n${BLUE}--- Step 4: Compiling C examples ---${NC}"

# Compiler flags
if [[ "$OSTYPE" == "darwin"* ]]; then
    CC_FLAGS="-framework Cocoa -framework OpenGL -framework IOKit -framework CoreFoundation -framework CoreGraphics"
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul_dll -Wl,-rpath,${PROJECT_ROOT}/target/release"
elif [[ "$OSTYPE" == "linux"* ]]; then
    CC_FLAGS=""
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul_dll -Wl,-rpath,${PROJECT_ROOT}/target/release -lm -lpthread -ldl"
else
    CC_FLAGS=""
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul_dll"
fi

COMPILED_EXAMPLES=()

for example in "${C_EXAMPLES[@]}"; do
    c_file="${C_EXAMPLES_DIR}/${example}.c"
    if [[ -f "${c_file}" ]]; then
        echo -n "  Compiling ${example}... "
        output_file="${C_BINARIES_DIR}/${example}"
        
        if cc -o "${output_file}" "${c_file}" -I"${HEADER_DIR}" ${CC_FLAGS} ${LINK_FLAGS} 2>/dev/null; then
            echo -e "${GREEN}OK${NC}"
            COMPILED_EXAMPLES+=("${example}")
        else
            echo -e "${RED}FAILED${NC}"
        fi
    else
        echo -e "${YELLOW}  SKIP: ${example}.c not found${NC}"
    fi
done

echo ""
echo "Compiled ${#COMPILED_EXAMPLES[@]} examples"

# ============================================
# Step 5: Take screenshots of each example
# ============================================
echo -e "\n${BLUE}--- Step 5: Taking screenshots ---${NC}"

# Time to wait for window to render (in seconds)
RENDER_DELAY=2

# Function to take screenshot of a running application
take_screenshot() {
    local example_name="$1"
    local screenshot_file="${SCREENSHOT_DIR}/${example_name}-${OS_NAME}.png"
    local binary="${C_BINARIES_DIR}/${example_name}"
    
    echo -n "  Screenshotting ${example_name}... "
    
    if [[ ! -f "${binary}" ]]; then
        echo -e "${RED}binary not found${NC}"
        return 1
    fi
    
    # Start the application in background
    "${binary}" &
    local pid=$!
    
    # Wait for window to render
    sleep ${RENDER_DELAY}
    
    # Check if process is still running
    if ! kill -0 $pid 2>/dev/null; then
        echo -e "${RED}process exited${NC}"
        return 1
    fi
    
    # Take screenshot using OS-specific method
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS: Use screencapture to capture the frontmost window
        # -l requires window ID, so we use -w for interactive or find the window
        # For automation, we'll capture the frontmost window after a brief delay
        
        # Get window ID using AppleScript
        local window_id
        window_id=$(osascript -e 'tell application "System Events" to get the id of first window of first process whose frontmost is true' 2>/dev/null || echo "")
        
        if [[ -n "${window_id}" ]]; then
            screencapture -l${window_id} -x "${screenshot_file}" 2>/dev/null
        else
            # Fallback: capture by window selection (less reliable in automation)
            # Use -w for window capture mode with a timeout
            screencapture -x -T1 "${screenshot_file}" 2>/dev/null || \
            screencapture -x "${screenshot_file}" 2>/dev/null
        fi
        
    elif [[ "$OSTYPE" == "linux"* ]]; then
        # Linux: Try various screenshot tools
        if command -v import &> /dev/null; then
            # ImageMagick's import
            import -window root "${screenshot_file}" 2>/dev/null
        elif command -v gnome-screenshot &> /dev/null; then
            gnome-screenshot -w -f "${screenshot_file}" 2>/dev/null
        elif command -v scrot &> /dev/null; then
            scrot -u "${screenshot_file}" 2>/dev/null
        else
            echo -e "${RED}no screenshot tool found${NC}"
            kill $pid 2>/dev/null
            return 1
        fi
    else
        echo -e "${YELLOW}Windows screenshot not implemented${NC}"
        kill $pid 2>/dev/null
        return 1
    fi
    
    # Kill the application
    kill $pid 2>/dev/null
    wait $pid 2>/dev/null || true
    
    # Verify screenshot was created
    if [[ -f "${screenshot_file}" ]]; then
        local size
        size=$(wc -c < "${screenshot_file}")
        echo -e "${GREEN}OK${NC} (${size} bytes)"
        return 0
    else
        echo -e "${RED}screenshot file not created${NC}"
        return 1
    fi
}

# Alternative: Use the screencapture command with window name
# This is more reliable on macOS
take_screenshot_macos_by_name() {
    local example_name="$1"
    local screenshot_file="${SCREENSHOT_DIR}/${example_name}-${OS_NAME}.png"
    local binary="${C_BINARIES_DIR}/${example_name}"
    
    echo -n "  Screenshotting ${example_name}... "
    
    if [[ ! -f "${binary}" ]]; then
        echo -e "${RED}binary not found${NC}"
        return 1
    fi
    
    # Start the application in background
    "${binary}" &
    local pid=$!
    
    # Wait for window to render
    sleep ${RENDER_DELAY}
    
    # Check if process is still running
    if ! kill -0 $pid 2>/dev/null; then
        echo -e "${RED}process exited${NC}"
        return 1
    fi
    
    # Get the window ID for this process
    # Using CoreGraphics window list
    local window_id
    window_id=$(osascript -e "
        tell application \"System Events\"
            set frontApp to first application process whose unix id is ${pid}
            if (count of windows of frontApp) > 0 then
                return id of window 1 of frontApp
            end if
        end tell
    " 2>/dev/null || echo "")
    
    if [[ -z "${window_id}" || "${window_id}" == "missing value" ]]; then
        # Fallback: try to get window ID using CGWindowListCopyWindowInfo
        # This requires a helper, so we'll use a simpler approach
        sleep 1
        screencapture -x -o -l$(osascript -e 'tell application "System Events" to id of window 1 of (first process whose unix id is '${pid}')' 2>/dev/null || echo "0") "${screenshot_file}" 2>/dev/null || \
        screencapture -x "${screenshot_file}" 2>/dev/null
    else
        screencapture -l${window_id} -x "${screenshot_file}" 2>/dev/null
    fi
    
    # Kill the application
    kill $pid 2>/dev/null
    wait $pid 2>/dev/null || true
    
    # Verify screenshot was created
    if [[ -f "${screenshot_file}" ]]; then
        local size
        size=$(wc -c < "${screenshot_file}")
        echo -e "${GREEN}OK${NC} (${size} bytes)"
        return 0
    else
        echo -e "${RED}screenshot file not created${NC}"
        return 1
    fi
}

SCREENSHOT_COUNT=0
SCREENSHOT_FAILED=0

for example in "${COMPILED_EXAMPLES[@]}"; do
    if [[ "$OSTYPE" == "darwin"* ]]; then
        if take_screenshot_macos_by_name "${example}"; then
            ((SCREENSHOT_COUNT++))
        else
            ((SCREENSHOT_FAILED++))
        fi
    else
        if take_screenshot "${example}"; then
            ((SCREENSHOT_COUNT++))
        else
            ((SCREENSHOT_FAILED++))
        fi
    fi
done

# ============================================
# Summary
# ============================================
echo -e "\n${BLUE}============================================${NC}"
echo -e "${BLUE}  Summary${NC}"
echo -e "${BLUE}============================================${NC}"
echo -e "${GREEN}Screenshots taken:${NC} ${SCREENSHOT_COUNT}"
echo -e "${RED}Failed:${NC} ${SCREENSHOT_FAILED}"
echo ""
echo "Screenshots saved to: ${SCREENSHOT_DIR}"
echo ""

# List generated screenshots
if [[ -d "${SCREENSHOT_DIR}" ]]; then
    echo "Generated files:"
    ls -la "${SCREENSHOT_DIR}"/*.png 2>/dev/null || echo "  (none)"
fi

if [[ ${SCREENSHOT_FAILED} -gt 0 ]]; then
    echo -e "\n${YELLOW}Some screenshots failed. This might be due to:${NC}"
    echo "  - Window not appearing in time (try increasing RENDER_DELAY)"
    echo "  - Application crashing on startup"
    echo "  - Screenshot permissions (System Preferences > Security > Screen Recording)"
    exit 1
fi

echo -e "\n${GREEN}Done!${NC}"
