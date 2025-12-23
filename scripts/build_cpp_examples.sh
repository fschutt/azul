#!/bin/bash
# Build script for C++ examples
# Usage: ./scripts/build_cpp_examples.sh [cpp_version]
# Example: ./scripts/build_cpp_examples.sh cpp11
#          ./scripts/build_cpp_examples.sh all

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Paths
HEADER_DIR="$PROJECT_ROOT/target/codegen/v2"
LIB_DIR="$PROJECT_ROOT/target/release"
EXAMPLES_DIR="$PROJECT_ROOT/examples/cpp"
OUTPUT_DIR="$PROJECT_ROOT/target/cpp_examples"

# Library name (without lib prefix and extension)
LIB_NAME="azul"

# Stubs file for missing destructor functions
STUBS_FILE="$HEADER_DIR/azul_stubs.c"
STUBS_OBJ="$OUTPUT_DIR/azul_stubs.o"

# Check if library exists
if [[ ! -f "$LIB_DIR/lib${LIB_NAME}.dylib" ]] && [[ ! -f "$LIB_DIR/lib${LIB_NAME}.so" ]] && [[ ! -f "$LIB_DIR/lib${LIB_NAME}.a" ]]; then
    echo "ERROR: Azul library not found in $LIB_DIR"
    echo "Please build the DLL first with: cargo build --release -p azul-dll --features=\"build-dll\""
    exit 1
fi

# Check if headers exist
if [[ ! -f "$HEADER_DIR/azul.h" ]]; then
    echo "ERROR: Headers not found in $HEADER_DIR"
    echo "Please run the codegen first"
    exit 1
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Compiler detection
if command -v clang++ &> /dev/null; then
    CXX="clang++"
    CC="clang"
elif command -v g++ &> /dev/null; then
    CXX="g++"
    CC="gcc"
else
    echo "ERROR: No C++ compiler found"
    exit 1
fi

echo "Using compiler: $CXX"
echo "Library dir: $LIB_DIR"
echo "Header dir: $HEADER_DIR"
echo "Output dir: $OUTPUT_DIR"
echo ""

# Build stubs object file if needed
if [[ -f "$STUBS_FILE" ]]; then
    echo "Building stubs for missing destructor functions..."
    $CC -c -I"$HEADER_DIR" -o "$STUBS_OBJ" "$STUBS_FILE" 2>/dev/null || {
        echo "Warning: Failed to build stubs, continuing without them"
        STUBS_OBJ=""
    }
else
    STUBS_OBJ=""
fi

# Common flags
COMMON_FLAGS="-I$HEADER_DIR -L$LIB_DIR"

# macOS specific flags
if [[ "$(uname)" == "Darwin" ]]; then
    COMMON_FLAGS="$COMMON_FLAGS -framework Cocoa -framework CoreGraphics -framework CoreText -framework Metal -framework QuartzCore -framework IOKit"
    # Use rpath for dynamic library loading
    COMMON_FLAGS="$COMMON_FLAGS -Wl,-rpath,$LIB_DIR"
fi

# Linux specific flags
if [[ "$(uname)" == "Linux" ]]; then
    COMMON_FLAGS="$COMMON_FLAGS -lpthread -ldl -lm"
    COMMON_FLAGS="$COMMON_FLAGS -Wl,-rpath,$LIB_DIR"
fi

# Function to build a single example
build_example() {
    local cpp_version="$1"
    local source_file="$2"
    local std_flag=""
    
    case "$cpp_version" in
        cpp03) std_flag="-std=c++03" ;;
        cpp11) std_flag="-std=c++11" ;;
        cpp14) std_flag="-std=c++14" ;;
        cpp17) std_flag="-std=c++17" ;;
        cpp20) std_flag="-std=c++20" ;;
        cpp23) std_flag="-std=c++23" ;;
        *) echo "Unknown C++ version: $cpp_version"; return 1 ;;
    esac
    
    local basename=$(basename "$source_file" .cpp)
    local output_file="$OUTPUT_DIR/${cpp_version}_${basename}"
    
    echo -n "  Building $basename... "
    
    # Build with stubs and dynamic library
    if [[ -n "$STUBS_OBJ" ]]; then
        if $CXX $std_flag $COMMON_FLAGS -o "$output_file" "$source_file" "$STUBS_OBJ" -l${LIB_NAME} 2>"$OUTPUT_DIR/${cpp_version}_${basename}.err"; then
            echo "✓ OK"
            rm -f "$OUTPUT_DIR/${cpp_version}_${basename}.err"
            return 0
        fi
    fi
    
    # Try without stubs
    if $CXX $std_flag $COMMON_FLAGS -o "$output_file" "$source_file" -l${LIB_NAME} 2>"$OUTPUT_DIR/${cpp_version}_${basename}.err"; then
        echo "✓ OK"
        rm -f "$OUTPUT_DIR/${cpp_version}_${basename}.err"
        return 0
    fi
    
    # Try static linking with stubs
    if [[ -n "$STUBS_OBJ" ]]; then
        if $CXX $std_flag $COMMON_FLAGS -o "$output_file" "$source_file" "$STUBS_OBJ" "$LIB_DIR/lib${LIB_NAME}.a" 2>"$OUTPUT_DIR/${cpp_version}_${basename}.err"; then
            echo "✓ OK (static)"
            rm -f "$OUTPUT_DIR/${cpp_version}_${basename}.err"
            return 0
        fi
    fi
    
    echo "✗ FAILED"
    echo "    Error log: $OUTPUT_DIR/${cpp_version}_${basename}.err"
    return 1
}

# Function to build all examples for a C++ version
build_version() {
    local cpp_version="$1"
    local version_dir="$EXAMPLES_DIR/$cpp_version"
    
    if [[ ! -d "$version_dir" ]]; then
        echo "Warning: Directory $version_dir does not exist, skipping"
        return 0
    fi
    
    echo "=== Building $cpp_version examples ==="
    
    local success=0
    local failed=0
    
    for source_file in "$version_dir"/*.cpp; do
        if [[ -f "$source_file" ]]; then
            if build_example "$cpp_version" "$source_file"; then
                ((success++))
            else
                ((failed++))
            fi
        fi
    done
    
    echo "  Results: $success succeeded, $failed failed"
    echo ""
    
    return $failed
}

# Main logic
TARGET_VERSION="${1:-all}"

total_failed=0

if [[ "$TARGET_VERSION" == "all" ]]; then
    for ver in cpp03 cpp11 cpp14 cpp17 cpp20 cpp23; do
        if ! build_version "$ver"; then
            ((total_failed++))
        fi
    done
else
    if ! build_version "$TARGET_VERSION"; then
        ((total_failed++))
    fi
fi

echo "=== Build Summary ==="
if [[ $total_failed -eq 0 ]]; then
    echo "All builds completed successfully!"
    echo ""
    echo "Binaries are in: $OUTPUT_DIR"
    echo ""
    echo "To run an example:"
    echo "  export DYLD_LIBRARY_PATH=$LIB_DIR:\$DYLD_LIBRARY_PATH"
    echo "  $OUTPUT_DIR/cpp11_hello-world"
else
    echo "Some builds failed. Check the .err files in $OUTPUT_DIR"
    exit 1
fi
