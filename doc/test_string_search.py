#!/usr/bin/env python3
"""
Test script to verify that string search finds all expected types.
This script MUST pass before we can consider string search working.
"""

import subprocess
import sys

# Types that MUST be found (type_name, expected_path)
REQUIRED_TYPES = [
    ("NormalizedLinearColorStopVec", "azul_css::props::style::background::NormalizedLinearColorStopVec"),
    ("LayoutBottom", "azul_css::props::layout::position::LayoutBottom"),
    ("LayoutTop", "azul_css::props::layout::position::LayoutTop"),
    ("F32Vec", "azul_css::corety::F32Vec"),
    ("U8Vec", "azul_css::corety::U8Vec"),
    ("GLuintVec", "azul_core::gl::GLuintVec"),
    ("TessellatedSvgNodeVec", "azul_core::svg::TessellatedSvgNodeVec"),
    # Multi-line macro test case
    ("OptionDomNodeId", "azul_core::dom::OptionDomNodeId"),
]

def run_autofix():
    """Run autofix and capture output"""
    result = subprocess.run(
        ["./target/release/azul-docs", "autofix"],
        cwd="/Users/fschutt/Development/azul",
        capture_output=True,
        text=True
    )
    return result.stdout + result.stderr

def main():
    print("🧪 Testing string search functionality...\n")
    
    output = run_autofix()
    
    failed = []
    passed = []
    
    for type_name, expected_path in REQUIRED_TYPES:
        search_marker = f"🔍 Found via string search: {type_name} at {expected_path}"
        
        if search_marker in output:
            passed.append(type_name)
            print(f"✅ {type_name}")
        else:
            failed.append(type_name)
            print(f"❌ {type_name} - NOT FOUND!")
            # Show what we got instead
            if f"❌ Type not found: {type_name}" in output:
                print(f"   (Type was marked as not found)")
    
    print(f"\n📊 Results: {len(passed)}/{len(REQUIRED_TYPES)} passed")
    
    if failed:
        print(f"\n❌ FAILED: The following types were not found:")
        for t in failed:
            print(f"   - {t}")
        sys.exit(1)
    else:
        print("\n✅ ALL TESTS PASSED!")
        sys.exit(0)

if __name__ == "__main__":
    main()
