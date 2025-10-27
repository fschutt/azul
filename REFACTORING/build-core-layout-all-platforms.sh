#!/bin/bash
set -e

echo "=========================================="
echo "Building azul-core and azul-layout for all platforms"
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
declare -a CORE_RESULTS
declare -a LAYOUT_RESULTS

# Function to build core for a target
build_core() {
    local TARGET=$1
    local PLATFORM=$2
    
    echo ""
    echo "${YELLOW}=========================================="
    echo "Building azul-core for: $PLATFORM ($TARGET)"
    echo "==========================================${NC}"
    echo ""
    
    # Add rust target if not installed
    rustup target add "$TARGET" 2>/dev/null || true
    
    # Try to build
    if cargo build --release --target "$TARGET" --package azul-core; then
        echo ""
        echo "${GREEN}✅ SUCCESS: azul-core for $PLATFORM${NC}"
        CORE_RESULTS+=("${GREEN}✅ azul-core $PLATFORM${NC}")
    else
        echo ""
        echo "${RED}❌ FAILED: azul-core for $PLATFORM${NC}"
        CORE_RESULTS+=("${RED}❌ azul-core $PLATFORM${NC}")
        return 1
    fi
}

# Function to build layout for a target
build_layout() {
    local TARGET=$1
    local PLATFORM=$2
    
    echo ""
    echo "${YELLOW}=========================================="
    echo "Building azul-layout for: $PLATFORM ($TARGET)"
    echo "==========================================${NC}"
    echo ""
    
    # Try to build
    if cargo build --release --target "$TARGET" --package azul-layout; then
        echo ""
        echo "${GREEN}✅ SUCCESS: azul-layout for $PLATFORM${NC}"
        LAYOUT_RESULTS+=("${GREEN}✅ azul-layout $PLATFORM${NC}")
    else
        echo ""
        echo "${RED}❌ FAILED: azul-layout for $PLATFORM${NC}"
        LAYOUT_RESULTS+=("${RED}❌ azul-layout $PLATFORM${NC}")
        return 1
    fi
}

# Build for each platform
echo "Building for macOS..."
build_core "x86_64-apple-darwin" "macOS"
build_layout "x86_64-apple-darwin" "macOS"

echo "Building for Linux..."
build_core "x86_64-unknown-linux-musl" "Linux"
build_layout "x86_64-unknown-linux-musl" "Linux"

echo "Building for Windows..."
build_core "x86_64-pc-windows-gnu" "Windows"
build_layout "x86_64-pc-windows-gnu" "Windows"

# Print summary
echo ""
echo "=========================================="
echo "BUILD SUMMARY"
echo "=========================================="
echo "azul-core:"
for result in "${CORE_RESULTS[@]}"; do
    echo -e "  $result"
done
echo ""
echo "azul-layout:"
for result in "${LAYOUT_RESULTS[@]}"; do
    echo -e "  $result"
done
echo "=========================================="
