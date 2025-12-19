#!/bin/bash
# Test all C++ examples compile against generated headers
# Usage: bash ./scripts/test_cpp_examples.sh

set -e

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HEADER_DIR="$PROJECT_ROOT/target/codegen/v2"
EXAMPLES_DIR="$PROJECT_ROOT/examples/cpp"
BUILD_DIR="$PROJECT_ROOT/target/cpp_test"

# Create build directory
mkdir -p "$BUILD_DIR"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0
FAILED_LIST=""

get_std_flag() {
    case "$1" in
        cpp03) echo "-std=c++03" ;;
        cpp11) echo "-std=c++11" ;;
        cpp14) echo "-std=c++14" ;;
        cpp17) echo "-std=c++17" ;;
        cpp20) echo "-std=c++20" ;;
        cpp23) echo "-std=c++2b" ;;
    esac
}

get_header_file() {
    case "$1" in
        cpp03) echo "azul03.hpp" ;;
        cpp11) echo "azul11.hpp" ;;
        cpp14) echo "azul14.hpp" ;;
        cpp17) echo "azul17.hpp" ;;
        cpp20) echo "azul20.hpp" ;;
        cpp23) echo "azul23.hpp" ;;
    esac
}

echo "========================================"
echo "Testing C++ Examples Compilation"
echo "========================================"
echo ""
echo "Header directory: $HEADER_DIR"
echo "Examples directory: $EXAMPLES_DIR"
echo ""

# Check if headers exist
for std_dir in cpp03 cpp11 cpp14 cpp17 cpp20 cpp23; do
    header=$(get_header_file "$std_dir")
    if [ ! -f "$HEADER_DIR/$header" ]; then
        echo -e "${RED}ERROR: Header $HEADER_DIR/$header not found!${NC}"
        echo "Run 'cargo run -p azul-doc codegen all' first."
        exit 1
    fi
done

# Check if azul.h exists (needed by C++ headers)
if [ ! -f "$HEADER_DIR/azul.h" ]; then
    echo -e "${RED}ERROR: $HEADER_DIR/azul.h not found!${NC}"
    exit 1
fi

echo "All headers found. Starting compilation tests..."
echo ""

# Test each C++ standard
for std_dir in cpp03 cpp11 cpp14 cpp17 cpp20 cpp23; do
    if [ ! -d "$EXAMPLES_DIR/$std_dir" ]; then
        echo -e "${YELLOW}SKIP: $std_dir directory not found${NC}"
        continue
    fi
    
    std_flag=$(get_std_flag "$std_dir")
    header_file=$(get_header_file "$std_dir")
    
    echo "========================================"
    echo "Testing $std_dir ($std_flag)"
    echo "========================================"
    
    for cpp_file in "$EXAMPLES_DIR/$std_dir"/*.cpp; do
        if [ ! -f "$cpp_file" ]; then
            continue
        fi
        
        filename=$(basename "$cpp_file")
        output_name="${filename%.cpp}"
        
        echo -n "  Compiling $filename... "
        
        # Compile with syntax check only (-fsyntax-only) since we don't have libazul
        # Use -I to include the header directory directly
        if clang++ $std_flag \
            -fsyntax-only \
            -I"$HEADER_DIR" \
            -Wno-unused-variable \
            -Wno-unused-function \
            "$cpp_file" 2>"$BUILD_DIR/${std_dir}_${filename}.err"; then
            echo -e "${GREEN}OK${NC}"
            ((PASSED++))
        else
            echo -e "${RED}FAILED${NC}"
            ((FAILED++))
            FAILED_LIST="$FAILED_LIST\n  - $std_dir/$filename"
            
            # Show first 20 lines of errors
            echo -e "${YELLOW}Errors:${NC}"
            head -20 "$BUILD_DIR/${std_dir}_${filename}.err" | sed 's/^/    /'
            echo ""
        fi
    done
    
    echo ""
done

echo "========================================"
echo "Summary"
echo "========================================"
echo -e "Passed: ${GREEN}$PASSED${NC}"
echo -e "Failed: ${RED}$FAILED${NC}"

if [ -n "$FAILED_LIST" ]; then
    echo -e "\nFailed files:${FAILED_LIST}"
fi

if [ $FAILED -gt 0 ]; then
    exit 1
fi

echo -e "\n${GREEN}All examples compiled successfully!${NC}"
