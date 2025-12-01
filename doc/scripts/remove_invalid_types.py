#!/usr/bin/env python3
"""
Remove invalid entries from api.json:
1. Primitive types (u8, f32, i16, etc.) - these are built-in
2. Single-letter generic type parameters (T, U, V, etc.)
3. Classes with only "doc": null (empty classes)

Also ensures "doc": null is not serialized.
"""

import json
import sys
from pathlib import Path

# Primitive types that should never be in the API
PRIMITIVE_TYPES = {
    "bool", "f32", "f64", "fn", "i128", "i16", "i32", "i64", "i8", "isize",
    "slice", "u128", "u16", "u32", "u64", "u8", "()", "usize", "c_void",
    "str", "char", "c_char", "c_schar", "c_uchar", "String",
    # Also some that shouldn't be standalone
    "refmut", "ref", "value",
}

def is_generic_type_param(name: str) -> bool:
    """Single uppercase letter = generic type parameter"""
    return len(name) == 1 and name.isupper()

def is_invalid_class_name(name: str) -> bool:
    """Check if a class name should not exist in the API"""
    if name in PRIMITIVE_TYPES:
        return True
    if is_generic_type_param(name):
        return True
    # Check for corrupt callback types (missing "extern C fn")
    if name.startswith("c_void") or ") ->" in name:
        return True
    # Check for weird function pointer syntax
    if name.startswith("fn("):
        return True
    return False

def clean_class_data(class_data: dict) -> dict:
    """Remove null doc fields and other cleanup"""
    if class_data.get("doc") is None:
        del class_data["doc"]
    return class_data

def remove_null_docs_recursive(obj):
    """Recursively remove all 'doc': null entries"""
    if isinstance(obj, dict):
        # Remove doc: null
        if "doc" in obj and obj["doc"] is None:
            del obj["doc"]
        # Recurse into remaining values
        for key, value in list(obj.items()):
            remove_null_docs_recursive(value)
    elif isinstance(obj, list):
        for item in obj:
            remove_null_docs_recursive(item)

def fix_api_json(api_path: Path) -> dict:
    """Remove invalid entries and clean up api.json"""
    
    with open(api_path, 'r') as f:
        api_data = json.load(f)
    
    stats = {
        'primitives_removed': 0,
        'generics_removed': 0,
        'corrupt_removed': 0,
        'null_docs_would_be_removed': 0,
    }
    
    for version_name, version_data in api_data.items():
        if 'api' not in version_data:
            continue
            
        for module_name, module_data in version_data['api'].items():
            if 'classes' not in module_data:
                continue
            
            classes_to_remove = []
            
            for class_name, class_data in module_data['classes'].items():
                if class_name in PRIMITIVE_TYPES:
                    classes_to_remove.append(class_name)
                    stats['primitives_removed'] += 1
                    print(f"  Removing primitive: {module_name}.{class_name}")
                elif is_generic_type_param(class_name):
                    classes_to_remove.append(class_name)
                    stats['generics_removed'] += 1
                    print(f"  Removing generic param: {module_name}.{class_name}")
                elif is_invalid_class_name(class_name):
                    classes_to_remove.append(class_name)
                    stats['corrupt_removed'] += 1
                    print(f"  Removing invalid: {module_name}.{class_name}")
            
            for class_name in classes_to_remove:
                del module_data['classes'][class_name]
    
    # Remove all "doc": null entries recursively
    remove_null_docs_recursive(api_data)
    
    total = stats['primitives_removed'] + stats['generics_removed'] + stats['corrupt_removed']
    
    if total > 0:
        with open(api_path, 'w') as f:
            json.dump(api_data, f, indent=2)
        print(f"\n[OK] Removed {total} invalid entries:")
        print(f"  - {stats['primitives_removed']} primitive types")
        print(f"  - {stats['generics_removed']} generic type parameters")
        print(f"  - {stats['corrupt_removed']} corrupt/invalid names")
    else:
        # Still write to remove null docs
        with open(api_path, 'w') as f:
            json.dump(api_data, f, indent=2)
        print("[OK] No invalid class names found (null docs cleaned)")
    
    return stats


def main():
    script_dir = Path(__file__).parent
    api_path = script_dir.parent.parent / 'api.json'
    
    if not api_path.exists():
        print(f"Error: api.json not found at {api_path}")
        sys.exit(1)
    
    print(f"[FIX] Scanning {api_path} for invalid entries...")
    fix_api_json(api_path)


if __name__ == '__main__':
    main()
