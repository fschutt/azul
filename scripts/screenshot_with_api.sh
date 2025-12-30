#!/usr/bin/env bash
#
# Screenshot C examples using Azul's native screenshot API
# 
# This script creates a special test binary that:
# 1. Creates a window
# 2. Waits for first render
# 3. Calls take_native_screenshot_base64() 
# 4. Outputs the base64 data URI as JSON
# 5. Exits
#
# The script then uses jq to extract the base64 data and decode it.
#
# Usage: ./scripts/screenshot_with_api.sh
#
# This approach uses Azul's built-in screenshot functionality which
# guarantees window decorations are captured correctly.

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

# Detect OS
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS_NAME="macos"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul_dll.dylib"
elif [[ "$OSTYPE" == "linux"* ]]; then
    OS_NAME="linux"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/libazul_dll.so"
else
    OS_NAME="windows"
    DYLIB_PATH="${PROJECT_ROOT}/target/release/azul_dll.dll"
fi

SCREENSHOT_DIR="${PROJECT_ROOT}/target/screenshots"
HEADER_DIR="${PROJECT_ROOT}/target/codegen/v2"
TEMP_DIR="${PROJECT_ROOT}/target/screenshot-temp"

mkdir -p "${SCREENSHOT_DIR}"
mkdir -p "${TEMP_DIR}"

echo -e "${BLUE}============================================${NC}"
echo -e "${BLUE}  Azul API Screenshot Script${NC}"
echo -e "${BLUE}============================================${NC}"
echo ""

# Check dependencies
if ! command -v jq &> /dev/null; then
    echo -e "${RED}ERROR: jq is not installed${NC}"
    exit 1
fi

# Create a C program that takes a screenshot using Azul's API
cat > "${TEMP_DIR}/screenshot_app.c" << 'EOF'
#include <azul.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// App state
typedef struct {
    const char* output_path;
    int screenshot_taken;
    int frame_count;
} ScreenshotState;

void ScreenshotState_destructor(void* s) { }
AZ_REFLECT(ScreenshotState, ScreenshotState_destructor);

// Take screenshot after window renders
AzUpdate on_frame(AzRefAny data, AzCallbackInfo info) {
    ScreenshotStateRefMut state = ScreenshotStateRefMut_create(&data);
    if (!ScreenshotState_downcastMut(&data, &state)) {
        return AzUpdate_DoNothing;
    }
    
    // Wait a few frames for window to fully render
    state.ptr->frame_count++;
    if (state.ptr->frame_count < 5) {
        ScreenshotStateRefMut_delete(&state);
        return AzUpdate_RefreshDom;
    }
    
    if (state.ptr->screenshot_taken) {
        ScreenshotStateRefMut_delete(&state);
        return AzUpdate_DoNothing;
    }
    
    // Take native screenshot with window decorations
    AzString path = AzString_copyFromBytes(state.ptr->output_path, 0, strlen(state.ptr->output_path));
    AzResultVoidString result = AzCallbackInfo_take_native_screenshot(&info, path);
    
    if (result.tag == AzResultVoidStringTag_Ok) {
        fprintf(stderr, "{\"status\":\"ok\",\"path\":\"%s\"}\n", state.ptr->output_path);
        state.ptr->screenshot_taken = 1;
    } else {
        fprintf(stderr, "{\"status\":\"error\",\"message\":\"screenshot failed\"}\n");
    }
    
    AzString_delete(&path);
    ScreenshotStateRefMut_delete(&state);
    
    // Request window close
    AzCallbackInfo_requestClose(&info);
    
    return AzUpdate_DoNothing;
}

// Simple layout with some content
AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom body = AzDom_createBody();
    
    AzString text = AzString_copyFromBytes("Screenshot Test", 0, 15);
    AzDom label = AzDom_createText(text);
    AzCssProperty font_size = AzCssProperty_fontSize(AzStyleFontSize_px(48.0));
    AzDom_addCssProperty(&label, font_size);
    AzDom_addChild(&body, label);
    
    // Add frame callback to take screenshot after render
    AzEventFilter event = AzEventFilter_nothingDetected();
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzDom_addCallback(&body, event, data_clone, on_frame);
    
    AzCss css = AzCss_empty();
    return AzDom_style(&body, css);
}

int main(int argc, char* argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <output_path.png>\n", argv[0]);
        return 1;
    }
    
    ScreenshotState state = {
        .output_path = argv[1],
        .screenshot_taken = 0,
        .frame_count = 0
    };
    AzRefAny data = ScreenshotState_upcast(state);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzString title = AzString_copyFromBytes("Screenshot", 0, 10);
    window.window_state.title = title;
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;
    
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
EOF

echo -e "${BLUE}--- Building screenshot helper ---${NC}"

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

# Ensure library exists
if [[ ! -f "${DYLIB_PATH}" ]]; then
    echo -e "${YELLOW}Building azul-dll (release)...${NC}"
    cargo build -p azul-dll --release
fi

# Compile the screenshot helper
if cc -o "${TEMP_DIR}/screenshot_app" "${TEMP_DIR}/screenshot_app.c" \
    -I"${HEADER_DIR}" ${CC_FLAGS} ${LINK_FLAGS} 2>&1; then
    echo -e "${GREEN}✓ Screenshot helper built${NC}"
else
    echo -e "${RED}✗ Failed to build screenshot helper${NC}"
    exit 1
fi

echo -e "\n${BLUE}--- Taking test screenshot ---${NC}"

OUTPUT_FILE="${SCREENSHOT_DIR}/test-screenshot-${OS_NAME}.png"

"${TEMP_DIR}/screenshot_app" "${OUTPUT_FILE}" 2>&1 || true

if [[ -f "${OUTPUT_FILE}" ]]; then
    SIZE=$(wc -c < "${OUTPUT_FILE}")
    echo -e "${GREEN}✓ Screenshot saved: ${OUTPUT_FILE} (${SIZE} bytes)${NC}"
else
    echo -e "${RED}✗ Screenshot not created${NC}"
fi

# Cleanup
rm -rf "${TEMP_DIR}"

echo -e "\n${GREEN}Done!${NC}"
