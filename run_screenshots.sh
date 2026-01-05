#!/bin/bash
export PATH="/c/Users/felix/.cargo/bin:/ucrt64/bin:$PATH"
cd /c/Users/felix/Development/azul
for example in hello-world calc widgets async opengl infinity xhtml; do
    echo "=== Testing $example ==="
    ./scripts/screenshot_single.sh $example 8765
done
