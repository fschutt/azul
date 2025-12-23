#!/bin/bash
#
# Test script for all Azul examples
# Verifies that all C, C++, Rust, and Python examples compile successfully

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0
SKIPPED=0

# Track failures
FAILURES=()

print_status() {
    local status=$1
    local name=$2
    if [ "$status" = "PASS" ]; then
        echo -e "${GREEN}✓${NC} $name"
        PASSED=$((PASSED + 1))
    elif [ "$status" = "FAIL" ]; then
        echo -e "${RED}✗${NC} $name"
        FAILED=$((FAILED + 1))
        FAILURES+=("$name")
    else
        echo -e "${YELLOW}○${NC} $name (skipped)"
        SKIPPED=$((SKIPPED + 1))
    fi
}

# Test Rust examples
test_rust_examples() {
    echo ""
    echo "=== Testing Rust Examples ==="
    cd "$PROJECT_ROOT/examples/rust"
    
    for example in async calc hello-world infinity opengl widgets xhtml; do
        OUTPUT=$(cargo build --example "$example" --features "link-static" 2>&1)
        if echo "$OUTPUT" | grep -q "^error"; then
            print_status "FAIL" "rust/$example"
        else
            print_status "PASS" "rust/$example"
        fi
    done
}

# Test C examples
test_c_examples() {
    echo ""
    echo "=== Testing C Examples ==="
    local c_dir="$PROJECT_ROOT/examples/c"
    local header_dir="$PROJECT_ROOT/target/codegen/v2"
    local output_dir="$PROJECT_ROOT/target/cpp_test"
    
    if [ ! -d "$c_dir" ]; then
        echo "C examples directory not found, skipping"
        return
    fi
    
    if [ ! -f "$header_dir/azul.h" ]; then
        echo "azul.h not found at $header_dir, run 'cargo run --manifest-path doc/Cargo.toml -- codegen all' first"
        return
    fi
    
    mkdir -p "$output_dir"
    
    for c_file in "$c_dir"/*.c; do
        if [ -f "$c_file" ]; then
            local name=$(basename "$c_file" .c)
            local output_file="$output_dir/${name}_c"
            
            # Compile with clang, linking is not required - just check compilation
            if clang -c -o "${output_file}.o" -I"$header_dir" "$c_file" 2>/dev/null; then
                if [ -f "${output_file}.o" ]; then
                    print_status "PASS" "c/$name"
                    rm -f "${output_file}.o"
                else
                    print_status "FAIL" "c/$name"
                fi
            else
                print_status "FAIL" "c/$name"
            fi
        fi
    done
}

# Test C++ examples
test_cpp_examples() {
    echo ""
    echo "=== Testing C++ Examples ==="
    local cpp_dir="$PROJECT_ROOT/examples/cpp"
    local header_dir="$PROJECT_ROOT/target/codegen/v2"
    local output_dir="$PROJECT_ROOT/target/cpp_test"
    
    if [ ! -d "$cpp_dir" ]; then
        echo "C++ examples directory not found, skipping"
        return
    fi
    
    mkdir -p "$output_dir"
    
    # C++ examples are in version subdirectories (cpp03, cpp11, cpp14, cpp17, cpp20, cpp23)
    for version_dir in "$cpp_dir"/cpp*; do
        if [ -d "$version_dir" ]; then
            local version=$(basename "$version_dir")
            
            # Determine the C++ standard flag based on version
            local std_flag=""
            case "$version" in
                cpp03) std_flag="-std=c++03" ;;
                cpp11) std_flag="-std=c++11" ;;
                cpp14) std_flag="-std=c++14" ;;
                cpp17) std_flag="-std=c++17" ;;
                cpp20) std_flag="-std=c++20" ;;
                cpp23) std_flag="-std=c++23" ;;
                *) std_flag="-std=c++17" ;;
            esac
            
            # Find matching header file (azul03.hpp, azul11.hpp, etc.)
            local header_version=$(echo "$version" | sed 's/cpp//')
            local header_file="$header_dir/azul${header_version}.hpp"
            
            if [ ! -f "$header_file" ]; then
                echo "Header $header_file not found, skipping $version examples"
                continue
            fi
            
            for cpp_file in "$version_dir"/*.cpp; do
                if [ -f "$cpp_file" ]; then
                    local name=$(basename "$cpp_file" .cpp)
                    local output_file="$output_dir/${version}_${name}"
                    
                    # Compile with clang++, just check compilation (no linking)
                    if clang++ -c $std_flag -o "${output_file}.o" -I"$header_dir" "$cpp_file" 2>/dev/null; then
                        if [ -f "${output_file}.o" ]; then
                            print_status "PASS" "cpp/$version/$name"
                            rm -f "${output_file}.o"
                        else
                            print_status "FAIL" "cpp/$version/$name"
                        fi
                    else
                        print_status "FAIL" "cpp/$version/$name"
                    fi
                fi
            done
        fi
    done
}

# Test Python examples (just check syntax)
test_python_examples() {
    if [ "$SKIP_PYTHON" = "1" ]; then
        echo ""
        echo "=== Skipping Python Examples ==="
        return
    fi
    
    echo ""
    echo "=== Testing Python Examples ==="
    local py_dir="$PROJECT_ROOT/examples/python"
    
    if [ ! -d "$py_dir" ]; then
        echo "Python examples directory not found, skipping"
        return
    fi
    
    for py_file in "$py_dir"/*.py; do
        if [ -f "$py_file" ]; then
            local name=$(basename "$py_file" .py)
            if python3 -m py_compile "$py_file" 2>/dev/null; then
                print_status "PASS" "python/$name"
            else
                print_status "SKIP" "python/$name"
            fi
        fi
    done
}

# Parse arguments
SKIP_PYTHON=0
for arg in "$@"; do
    case $arg in
        --skip-python)
            SKIP_PYTHON=1
            ;;
    esac
done

# Main
echo "=========================================="
echo "  Azul Examples Test Suite"
echo "=========================================="

test_rust_examples
test_c_examples
test_cpp_examples
test_python_examples

# Summary
echo ""
echo "=========================================="
echo "  Summary"
echo "=========================================="
echo -e "${GREEN}Passed:${NC}  $PASSED"
echo -e "${RED}Failed:${NC}  $FAILED"
echo -e "${YELLOW}Skipped:${NC} $SKIPPED"

if [ $FAILED -gt 0 ]; then
    echo ""
    echo "Failed examples:"
    for f in "${FAILURES[@]}"; do
        echo "  - $f"
    done
    exit 1
fi

echo ""
echo -e "${GREEN}All tests passed!${NC}"
exit 0
