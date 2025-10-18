#!/bin/bash
set -e

echo "=========================================="
echo "Building azul-dll for all platforms"
echo "=========================================="
echo ""

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

BUILD_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$BUILD_DIR"

# Array to track results
declare -a RESULTS

# Function to build for a target
build_target() {
    local TARGET=$1
    local PLATFORM=$2
    
    echo ""
    echo "${YELLOW}=========================================="
    echo "Building for: $PLATFORM ($TARGET)"
    echo "==========================================${NC}"
    echo ""
    
    # Add rust target if not installed
    rustup target add "$TARGET" 2>/dev/null || true
    
    # Try to build
    if cargo build --release --target "$TARGET" --no-default-features --features desktop-cdylib; then
        echo ""
        echo "${GREEN}✅ SUCCESS: $PLATFORM ($TARGET)${NC}"
        RESULTS+=("${GREEN}✅ $PLATFORM${NC}")
        
        # Show output file info
        OUTPUT_DIR="../target/$TARGET/release"
        if [ "$PLATFORM" = "macOS" ]; then
            OUTPUT_FILE="$OUTPUT_DIR/libazul_dll.dylib"
        elif [ "$PLATFORM" = "Linux" ]; then
            OUTPUT_FILE="$OUTPUT_DIR/libazul_dll.so"
        else
            OUTPUT_FILE="$OUTPUT_DIR/azul_dll.dll"
        fi
        
        if [ -f "$OUTPUT_FILE" ]; then
            FILE_SIZE=$(ls -lh "$OUTPUT_FILE" | awk '{print $5}')
            echo "   Output: $OUTPUT_FILE ($FILE_SIZE)"
        fi
    else
        echo ""
        echo "${RED}❌ FAILED: $PLATFORM ($TARGET)${NC}"
        RESULTS+=("${RED}❌ $PLATFORM${NC}")
    fi
}

# Build for each platform
build_target "x86_64-apple-darwin" "macOS"
build_target "x86_64-unknown-linux-musl" "Linux"
build_target "x86_64-pc-windows-gnu" "Windows"

# Print summary
echo ""
echo "=========================================="
echo "BUILD SUMMARY"
echo "=========================================="
for result in "${RESULTS[@]}"; do
    echo -e "$result"
done
echo "=========================================="
