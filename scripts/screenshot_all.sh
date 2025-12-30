#!/usr/bin/env bash
#
# Full screenshot pipeline for C examples
# 
# This script:
# 1. Runs codegen
# 2. Runs memtest (optional)
# 3. Builds azul-dll in release mode
# 4. Compiles C examples
# 5. Takes screenshots of each example using OS-level tools
# 6. Saves to target/screenshots/{example}-{os}.png
#
# Usage: ./scripts/screenshot_all.sh [--skip-codegen] [--skip-memtest]

set -e

# Parse arguments
SKIP_CODEGEN=false
SKIP_MEMTEST=true  # Skip by default since it's slow
RUN_MEMTEST=false

for arg in "$@"; do
    case $arg in
        --skip-codegen)
            SKIP_CODEGEN=true
            shift
            ;;
        --skip-memtest)
            SKIP_MEMTEST=true
            shift
            ;;
        --memtest)
            RUN_MEMTEST=true
            SKIP_MEMTEST=false
            shift
            ;;
        *)
            ;;
    esac
done

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${PROJECT_ROOT}"

# Detect OS
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS_NAME="macos"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul_dll.dylib"
    CC_FLAGS="-framework Cocoa -framework OpenGL -framework IOKit -framework CoreFoundation -framework CoreGraphics"
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul_dll -Wl,-rpath,${PROJECT_ROOT}/target/release"
elif [[ "$OSTYPE" == "linux"* ]]; then
    OS_NAME="linux"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul_dll.so"
    CC_FLAGS=""
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul_dll -Wl,-rpath,${PROJECT_ROOT}/target/release -lm -lpthread -ldl"
else
    OS_NAME="windows"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/azul_dll.dll"
    CC_FLAGS=""
    LINK_FLAGS="-L${PROJECT_ROOT}/target/release -lazul_dll"
fi

SCREENSHOT_DIR="${PROJECT_ROOT}/target/screenshots"
C_EXAMPLES_DIR="${PROJECT_ROOT}/examples/c"
C_BINARIES_DIR="${PROJECT_ROOT}/target/c-examples"
HEADER_DIR="${PROJECT_ROOT}/target/codegen/v2"

# Examples to screenshot (excludes opengl which needs special handling)
C_EXAMPLES=(
    "hello-world"
    "calc"
    "widgets"
)

mkdir -p "${SCREENSHOT_DIR}"
mkdir -p "${C_BINARIES_DIR}"

echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║  Azul C Examples - Full Screenshot Pipeline                ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "OS:              ${GREEN}${OS_NAME}${NC}"
echo -e "Screenshots dir: ${GREEN}${SCREENSHOT_DIR}${NC}"
echo -e "Skip codegen:    ${SKIP_CODEGEN}"
echo -e "Run memtest:     ${RUN_MEMTEST}"
echo ""

# ============================================
# Step 1: Codegen
# ============================================
if [[ "${SKIP_CODEGEN}" == "false" ]]; then
    echo -e "${BLUE}━━━ Step 1: Code Generation ━━━${NC}"
    if cargo run -p azul-doc -- codegen all 2>&1 | tail -5; then
        echo -e "${GREEN}✓ Codegen complete${NC}\n"
    else
        echo -e "${RED}✗ Codegen failed${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}━━━ Step 1: Code Generation (skipped) ━━━${NC}\n"
fi

# ============================================
# Step 2: Memtest (optional)
# ============================================
if [[ "${RUN_MEMTEST}" == "true" ]]; then
    echo -e "${BLUE}━━━ Step 2: Memory Test ━━━${NC}"
    if cargo test -p azul-dll --release 2>&1 | tail -10; then
        echo -e "${GREEN}✓ Memtest passed${NC}\n"
    else
        echo -e "${RED}✗ Memtest failed${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}━━━ Step 2: Memory Test (skipped) ━━━${NC}\n"
fi

# ============================================
# Step 3: Build azul-dll
# ============================================
echo -e "${BLUE}━━━ Step 3: Building azul-dll (release) ━━━${NC}"
if cargo build -p azul-dll --release 2>&1 | tail -3; then
    echo -e "${GREEN}✓ azul-dll built${NC}\n"
else
    echo -e "${RED}✗ Build failed${NC}"
    exit 1
fi

# Verify library
if [[ ! -f "${DYLIB_PATH}" ]]; then
    echo -e "${RED}ERROR: Library not found at ${DYLIB_PATH}${NC}"
    exit 1
fi

# ============================================
# Step 4: Compile C examples
# ============================================
echo -e "${BLUE}━━━ Step 4: Compiling C Examples ━━━${NC}"

COMPILED=()
COMPILE_FAILED=()

for example in "${C_EXAMPLES[@]}"; do
    c_file="${C_EXAMPLES_DIR}/${example}.c"
    output="${C_BINARIES_DIR}/${example}"
    
    if [[ ! -f "${c_file}" ]]; then
        echo -e "  ${YELLOW}⊘${NC} ${example} (source not found)"
        continue
    fi
    
    echo -n "  Compiling ${example}... "
    
    if cc -o "${output}" "${c_file}" -I"${HEADER_DIR}" ${CC_FLAGS} ${LINK_FLAGS} 2>/dev/null; then
        echo -e "${GREEN}✓${NC}"
        COMPILED+=("${example}")
    else
        echo -e "${RED}✗${NC}"
        COMPILE_FAILED+=("${example}")
    fi
done

echo ""
echo -e "Compiled: ${GREEN}${#COMPILED[@]}${NC} / Failed: ${RED}${#COMPILE_FAILED[@]}${NC}"
echo ""

# ============================================
# Step 5: Take screenshots
# ============================================
echo -e "${BLUE}━━━ Step 5: Taking Screenshots ━━━${NC}"

# Delay for window to render (seconds)
RENDER_DELAY=2
SCREENSHOT_SUCCESS=0
SCREENSHOT_FAILED=0

take_screenshot_macos() {
    local example="$1"
    local binary="${C_BINARIES_DIR}/${example}"
    local output="${SCREENSHOT_DIR}/${example}-${OS_NAME}.png"
    
    echo -n "  📸 ${example}... "
    
    # Start the app
    "${binary}" &
    local pid=$!
    
    # Wait for window to appear
    sleep ${RENDER_DELAY}
    
    # Check if still running
    if ! kill -0 $pid 2>/dev/null; then
        echo -e "${RED}crashed${NC}"
        return 1
    fi
    
    # Find and capture the window
    # Use screencapture with window ID from process
    local wid
    wid=$(osascript -e "
        tell application \"System Events\"
            try
                set p to first process whose unix id is ${pid}
                if (count of windows of p) > 0 then
                    return id of window 1 of p
                end if
            end try
        end tell
        return \"\"
    " 2>/dev/null || echo "")
    
    if [[ -n "${wid}" && "${wid}" != "" && "${wid}" != "missing value" ]]; then
        screencapture -l"${wid}" -x -o "${output}" 2>/dev/null
    else
        # Fallback: capture front window with short delay
        sleep 0.5
        screencapture -x -w -o "${output}" 2>/dev/null || \
        screencapture -x "${output}" 2>/dev/null
    fi
    
    # Terminate the app
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
    
    # Check result
    if [[ -f "${output}" ]] && [[ $(wc -c < "${output}") -gt 1000 ]]; then
        local size
        size=$(ls -lh "${output}" | awk '{print $5}')
        echo -e "${GREEN}✓${NC} (${size})"
        return 0
    else
        echo -e "${RED}failed${NC}"
        rm -f "${output}" 2>/dev/null
        return 1
    fi
}

take_screenshot_linux() {
    local example="$1"
    local binary="${C_BINARIES_DIR}/${example}"
    local output="${SCREENSHOT_DIR}/${example}-${OS_NAME}.png"
    
    echo -n "  📸 ${example}... "
    
    # Start the app
    "${binary}" &
    local pid=$!
    
    # Wait for window
    sleep ${RENDER_DELAY}
    
    if ! kill -0 $pid 2>/dev/null; then
        echo -e "${RED}crashed${NC}"
        return 1
    fi
    
    # Try different screenshot tools
    local captured=false
    
    if command -v gnome-screenshot &>/dev/null; then
        gnome-screenshot -w -f "${output}" 2>/dev/null && captured=true
    elif command -v scrot &>/dev/null; then
        scrot -u "${output}" 2>/dev/null && captured=true
    elif command -v import &>/dev/null; then
        # ImageMagick - need window ID
        local wid
        wid=$(xdotool search --pid $pid 2>/dev/null | head -1)
        if [[ -n "${wid}" ]]; then
            import -window "${wid}" "${output}" 2>/dev/null && captured=true
        fi
    fi
    
    # Terminate
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
    
    if [[ "${captured}" == "true" ]] && [[ -f "${output}" ]]; then
        local size
        size=$(ls -lh "${output}" | awk '{print $5}')
        echo -e "${GREEN}✓${NC} (${size})"
        return 0
    else
        echo -e "${RED}failed${NC}"
        return 1
    fi
}

for example in "${COMPILED[@]}"; do
    if [[ "$OSTYPE" == "darwin"* ]]; then
        if take_screenshot_macos "${example}"; then
            ((SCREENSHOT_SUCCESS++))
        else
            ((SCREENSHOT_FAILED++))
        fi
    elif [[ "$OSTYPE" == "linux"* ]]; then
        if take_screenshot_linux "${example}"; then
            ((SCREENSHOT_SUCCESS++))
        else
            ((SCREENSHOT_FAILED++))
        fi
    else
        echo -e "  ${YELLOW}⊘${NC} ${example} (Windows not implemented)"
        ((SCREENSHOT_FAILED++))
    fi
done

# ============================================
# Summary
# ============================================
echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  Summary${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "  Screenshots:  ${GREEN}${SCREENSHOT_SUCCESS}${NC} succeeded, ${RED}${SCREENSHOT_FAILED}${NC} failed"
echo -e "  Location:     ${SCREENSHOT_DIR}"
echo ""

# List screenshots
if ls "${SCREENSHOT_DIR}"/*.png 1>/dev/null 2>&1; then
    echo "  Generated files:"
    for f in "${SCREENSHOT_DIR}"/*.png; do
        name=$(basename "$f")
        size=$(ls -lh "$f" | awk '{print $5}')
        echo -e "    ${GREEN}✓${NC} ${name} (${size})"
    done
fi

echo ""

if [[ ${SCREENSHOT_FAILED} -gt 0 ]]; then
    echo -e "${YELLOW}Note: Some screenshots failed. On macOS, ensure:${NC}"
    echo "  - Screen Recording permission is granted for Terminal"
    echo "  - System Preferences > Security & Privacy > Privacy > Screen Recording"
    exit 1
fi

echo -e "${GREEN}✓ All screenshots completed successfully!${NC}"
