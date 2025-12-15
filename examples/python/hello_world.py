#!/usr/bin/env python3
"""
Simple Azul GUI Hello World example.

To run:
    # From the azul directory:
    cd target/release
    python3 ../../examples/python/hello_world.py
"""

import sys
import os

# Add the directory containing azul.so to Python path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..', 'target', 'release'))

try:
    import azul
    print(f"Successfully imported azul module!")
    print(f"Module path: {azul.__file__}")
    
    # List available classes and functions
    print("\nAvailable in azul:")
    for name in sorted(dir(azul)):
        if not name.startswith('_'):
            obj = getattr(azul, name)
            obj_type = type(obj).__name__
            print(f"  {name}: {obj_type}")
            
except ImportError as e:
    print(f"Failed to import azul: {e}")
    print("\nMake sure you've built the Python extension:")
    print("  cargo build -p azul-dll --lib --no-default-features --features python-extension --release")
    print("\nAnd copied/renamed the library:")
    print("  cp target/release/libazul_dll.dylib target/release/azul.so  # macOS")
    print("  cp target/release/libazul_dll.so target/release/azul.so     # Linux")
    sys.exit(1)
