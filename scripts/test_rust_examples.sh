#!/bin/bash
# Test script for Rust examples with both link-static and link-dynamic modes
# Usage: ./scripts/test_rust_examples.sh [static|dynamic|all]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
EXAMPLES_DIR="$ROOT_DIR/examples/rust"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# All examples to test
EXAMPLES=(
    "hello-world"
    "calc"
    "infinity"
    "async"
    "opengl"
    "widgets"
    "xhtml"
)

# Function to build an example
build_example() {
    local example=$1
    local feature=$2
    local mode_name=$3
    
    echo -e "${YELLOW}Building $example with $mode_name...${NC}"
    
    cd "$EXAMPLES_DIR"
    if cargo build --example "$example" --features "$feature" 2>&1; then
        echo -e "${GREEN}✓ $example ($mode_name) - OK${NC}"
        return 0
    else
        echo -e "${RED}✗ $example ($mode_name) - FAILED${NC}"
        return 1
    fi
}

# Function to test all examples with a specific mode
test_mode() {
    local feature=$1
    local mode_name=$2
    local passed=0
    local failed=0
    local failed_list=()
    
    echo ""
    echo "========================================"
    echo "Testing with $mode_name"
    echo "========================================"
    echo ""
    
    for example in "${EXAMPLES[@]}"; do
        if build_example "$example" "$feature" "$mode_name"; then
            ((passed++))
        else
            ((failed++))
            failed_list+=("$example")
        fi
    done
    
    echo ""
    echo "----------------------------------------"
    echo "Results for $mode_name:"
    echo "  Passed: $passed"
    echo "  Failed: $failed"
    if [ ${#failed_list[@]} -gt 0 ]; then
        echo "  Failed examples: ${failed_list[*]}"
    fi
    echo "----------------------------------------"
    
    return $failed
}

# Main script
main() {
    local mode=${1:-all}
    local total_failed=0
    
    echo "========================================"
    echo "Azul Rust Examples Test Script"
    echo "========================================"
    
    case $mode in
        static)
            test_mode "link-static" "link-static"
            total_failed=$?
            ;;
        dynamic)
            test_mode "link-dynamic" "link-dynamic"
            total_failed=$?
            ;;
        all)
            test_mode "link-static" "link-static"
            local static_failed=$?
            
            test_mode "link-dynamic" "link-dynamic"
            local dynamic_failed=$?
            
            total_failed=$((static_failed + dynamic_failed))
            ;;
        *)
            echo "Usage: $0 [static|dynamic|all]"
            exit 1
            ;;
    esac
    
    echo ""
    echo "========================================"
    if [ $total_failed -eq 0 ]; then
        echo -e "${GREEN}All tests passed!${NC}"
    else
        echo -e "${RED}Some tests failed. Total failures: $total_failed${NC}"
    fi
    echo "========================================"
    
    exit $total_failed
}

main "$@"
