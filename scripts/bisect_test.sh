#!/bin/bash
# Git bisect test script - tests if hello-world hangs at 100% CPU

set -e

cd /Users/fschutt/Development/azul

# Build hello-world
echo "Building hello-world..."
if ! cargo build --release --example hello-world 2>&1; then
    echo "Build failed - skipping this commit"
    exit 125  # Skip this commit
fi

# Start hello-world in background
echo "Starting hello-world..."
./target/release/examples/hello-world &
PID=$!

# Wait for startup
sleep 4

# Check if process is still running
if ! ps -p $PID > /dev/null 2>&1; then
    echo "Process crashed - marking as bad"
    exit 1
fi

# Get CPU usage
CPU=$(ps -p $PID -o %cpu= | tr -d ' ')
echo "CPU usage: $CPU%"

# Kill the process
kill -9 $PID 2>/dev/null || true

# Check if CPU is above 50% (indicating a hang)
CPU_INT=${CPU%.*}
if [ "$CPU_INT" -gt 50 ]; then
    echo "HIGH CPU ($CPU%) - BAD commit"
    exit 1
else
    echo "Normal CPU ($CPU%) - GOOD commit"
    exit 0
fi
